//! 搭配层 `ground` 与 `verify`（步 25e，B148）：候选先交执行器跑，执行输出与候选一起渲染成字面材料，再交 JEV 判；
//! 执行失败变成未决；两层嵌套（`map` 里的 `verify`；`search` 的 `ground` 槽闭环）；同一实参只执行一次；
//! 产物 taint 随执行器。执行器用 `ActionRegistry::register` 登记的假动作，不起子进程、不要沙箱；闭包端口，不发请求。
//!
//! 依据：B148；B150、B164（执行器动作的实参与输出形状）；B59/B92（`transform` 承接来源）；
//! 预注册 `地基/过程记录/工程-步25e.md` 一·2·6 (a)–(g)。

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{EntryArgs, Outcome, Session};
use serde_json::{Value as Json, json};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 是非题按题与材料定读数：题里写了「等于 N」时，材料含「输出：N」0.9、含「大约」0.5、其余 0.1；
/// 题里写「测试」时，材料含「失败 0」0.9、其余 0.1。记下每次判的材料。
fn 判断端口<'a>(seen: &'a RefCell<Vec<String>>) -> FnPort<'a> {
    FnPort::judge("fixed-0", move |s, qs| {
        let text = s.on_text();
        Ok::<_, EffectError>(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    seen.borrow_mut().push(text.clone());
                    let p = if q.text.contains("测试") {
                        if text.contains("失败 0") { 0.9 } else { 0.1 }
                    } else {
                        let n = q
                            .text
                            .split("等于 ")
                            .nth(1)
                            .and_then(|r| r.split(' ').next());
                        match n {
                            Some(n) if text.contains(&format!("输出：{n}")) => 0.9,
                            _ if text.contains("大约") => 0.5,
                            _ => 0.1,
                        }
                    };
                    Answer::Noul(p)
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    })
}

/// 生成端口：按 retry_seq 给一轮代码；记下每次的上下文
fn 生成端口<'a>(
    ctxs: &'a RefCell<Vec<Vec<String>>>,
    rounds: &'static [&'static [&'static str]],
) -> FnPort<'a> {
    FnPort::generate("fixed-0", move |_p, ctx, _n, retry| {
        ctxs.borrow_mut().push(
            ctx.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect(),
        );
        Ok(GenResult {
            outputs: rounds[retry as usize].iter().map(|t| json!(t)).collect(),
            ..Default::default()
        })
    })
}

fn 文本(v: &Value) -> String {
    match v.to_json() {
        Json::String(s) => s,
        other => other.to_string(),
    }
}

fn 记录(fields: Vec<(&str, Value)>) -> Value {
    Value::record(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// 假执行器：`exec_py` 按代码给固定输出（「boom」报错、「sleep」超时）；`check_tests` 按代码给通过数。
/// 两者都登记为可逆、`TaintOut::Untrusted`，与真执行器同形；各自计调用次数。
fn 动作表(exec_calls: Rc<Cell<usize>>, check_calls: Rc<Cell<usize>>) -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("exec_py", 0.0, true, TaintOut::Untrusted, move |args| {
        exec_calls.set(exec_calls.get() + 1);
        let code = 文本(&args[0]);
        let (stdout, timed_out) = match code.as_str() {
            "boom" => return Err("NoSandbox: 测试里的假失败".into()),
            "sleep" => ("", true),
            "print(385)" => ("385\n", false),
            "print(285)" => ("285\n", false),
            "print(302)" => ("302\n", false),
            "print(1)" => ("1\n", false),
            "print('大约', 385)" => ("大约 385\n", false),
            _ => ("?\n", false),
        };
        Ok(记录(vec![
            ("stdout", Value::text(stdout)),
            ("stderr", Value::text("")),
            (
                "exit_code",
                if timed_out {
                    Value::Unit
                } else {
                    Value::int(0)
                },
            ),
            ("timed_out", Value::bool(timed_out)),
        ]))
    });
    a.register("check_tests", 0.0, true, TaintOut::Untrusted, move |args| {
        check_calls.set(check_calls.get() + 1);
        let good = 文本(&args[0]) == "good";
        Ok(记录(vec![
            ("passed", Value::int(if good { 1 } else { 0 })),
            ("failed", Value::int(if good { 0 } else { 1 })),
            ("log", Value::list(vec![])),
        ]))
    });
    a
}

/// 把程序写进 `target/` 下的临时目录（import 只收相对路径），经装载器装上库再跑
fn 跑(src: &str, ports: Ports<'_>, acts: &ActionRegistry) -> Result<Outcome, String> {
    let dir = root().join(format!(
        "target/compose-ground-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(&path, src).unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let program = jpp::lower(&loaded.expect("装载").program).expect("lower");
    let calib = 库();
    Session::new(ports, &calib, acts)
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

const 头: &str = r#"import "../../lib/compose/ground.jpp";
import "../../lib/compose/search.jpp";
import "../../lib/compose/carry.jpp";
budget {calls: 60, cost: 0, depth: 64};
let runner = ground("exec_py", fn(c) { [content(c), "", 5] },
                    fn(c, out) { "代码：" + content(c) + "\n输出：" + content(out).stdout });
let correct = test("这段代码的输出等于 385 吗？", "k");
"#;

fn 值(o: &Outcome) -> Json {
    o.value_json()
}

fn 内容(v: &Json) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("不是列表：{v}"))
        .iter()
        .map(|m| {
            m["content"]
                .as_str()
                .unwrap_or_else(|| panic!("不是文本材料：{m}"))
                .to_string()
        })
        .collect()
}

fn 计数() -> (Rc<Cell<usize>>, Rc<Cell<usize>>) {
    (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)))
}

/// (a) verify：三段候选先执行、再判；判断看到的是「代码 + 输出」；接受、拒绝、未决各一；执行 3 次
#[test]
fn a_verify_执行后再判() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let r = verify([mat(\"print(385)\"), mat(\"print(285)\"), mat(\"print('大约', 385)\")], runner, correct);
{{ok: map(accepted(r), fn(x) {{ x.item }}), bad: map(ignored(r), fn(x) {{ x.item }}),
  causes: map(r.pending, fn(p) {{ p.cause }}), pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(内容(&v["ok"]), ["代码：print(385)\n输出：385\n"]);
    assert_eq!(内容(&v["bad"]), ["代码：print(285)\n输出：285\n"]);
    assert_eq!(v["causes"], json!(["band"]));
    assert_eq!(e.get(), 3);
    assert_eq!(
        *seen.borrow(),
        [
            "代码：print(385)\n输出：385\n",
            "代码：print(285)\n输出：285\n",
            "代码：print('大约', 385)\n输出：大约 385\n"
        ]
    );
}

/// (b) 失败：动作报错（NoSandbox）的那段成为未决元素（原因 fail），不判、不中断；超时记录照常渲染、交判断
#[test]
fn b_执行失败成为未决() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let r = verify([mat(\"print(385)\"), mat(\"boom\"), mat(\"sleep\")], runner, correct);
{{ok: len(accepted(r)), bad: len(ignored(r)), causes: map(r.pending, fn(p) {{ p.cause }}),
  failed: map(r.pending, fn(p) {{ is_fail(p.item) }}), pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(v["ok"], json!(1));
    assert_eq!(v["bad"], json!(1), "超时的那段照常判，输出为空，被拒");
    assert_eq!(
        v["causes"],
        json!(["fail:状态含 Fail 材料"]),
        "失败类原因，带明细"
    );
    assert_eq!(v["failed"], json!([true]));
    assert_eq!(e.get(), 3, "失败的那次也算一次执行");
    assert_eq!(seen.borrow().len(), 2, "失败的那段不交判断");
    assert_eq!(o.returned_unsure.len(), 1);
    assert!(
        o.returned_unsure[0].starts_with("unsure(fail"),
        "{:?}",
        o.returned_unsure
    );
}

/// (c) check_tests：按通过数渲染，判断据此分
#[test]
fn c_check_tests_接地() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let tester = ground(\"check_tests\", fn(x) {{ [content(x), [\"f(1) == 1\"], 5] }},
                    fn(x, out) {{ \"通过 \" + text(content(out).passed) + \"，失败 \" + text(content(out).failed) }});
let passes = test(\"测试全部通过了吗？\", \"k\");
let r = verify([mat(\"good\"), mat(\"bad\")], tester, passes);
{{ok: map(accepted(r), fn(x) {{ x.item }}), bad: map(ignored(r), fn(x) {{ x.item }}), pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(内容(&v["ok"]), ["通过 1，失败 0"]);
    assert_eq!(内容(&v["bad"]), ["通过 0，失败 1"]);
    assert_eq!(c.get(), 2);
    assert_eq!(e.get(), 0);
}

/// (d) 两层嵌套一：map 里对两组候选各调一次 verify（两组各一道题），两组的 pending 经 carry 并进外层
#[test]
fn d_map_里的两次核验_carry_到外层() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let groups = [{{cands: [mat(\"print(385)\"), mat(\"print('大约', 385)\")], q: correct}},
              {{cands: [mat(\"print(285)\"), mat(\"print(1)\")], q: test(\"这段代码的输出等于 285 吗？\", \"k\")}}];
let rs = map(groups, fn(g) {{ verify(g.cands, runner, g.q) }});
let all = carry(carry([], rs[0].pending, \"组#0\"), rs[1].pending, \"组#1\");
{{ok: map(rs, fn(r) {{ map(accepted(r), fn(x) {{ x.item }}) }}), via: map(all, fn(p) {{ p.via }}), pending: all}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(内容(&v["ok"][0]), ["代码：print(385)\n输出：385\n"]);
    assert_eq!(内容(&v["ok"][1]), ["代码：print(285)\n输出：285\n"]);
    assert_eq!(v["via"], json!([["组#0"]]), "只有第一组有带内的");
    assert_eq!(e.get(), 4);
}

/// (e) 两层嵌套二（闭环）：search 的 ground 槽接执行器。第 1 轮没有好的，refine 宽限；带内那段（接地后的材料）
/// 进第 2 轮上下文；第 2 轮凑够 width，stop。生成器重提的旧代码不再执行
#[test]
fn e_search_的_ground_槽闭环() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(
        &ctxs,
        &[
            &["print(285)", "print(302)", "print('大约', 385)"],
            &["print(285)", "print(385)", "print(1)"],
        ],
    ));
    let src = format!(
        "{头}let brief = mat(\"需求\");
let propose = fn(frontier, i) {{ gen(\"提 3 段代码\", concat([brief], frontier), 3, i) }};
let r = search([], propose, correct, unit, 3, {{width: 1, unsure_to: \"refine\", ground: runner}});
{{kept: map(r.value, fn(x) {{ x.item }}), found_in: map(r.value, fn(x) {{ x.round }}), reason: r.detail.reason,
  rounds: r.detail.rounds, duplicates: r.detail.duplicates, pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(v["reason"], json!("stop"));
    assert_eq!(v["rounds"], json!(2));
    assert_eq!(内容(&v["kept"]), ["代码：print(385)\n输出：385\n"]);
    assert_eq!(v["found_in"], json!([1]));
    assert_eq!(v["duplicates"], json!(1));
    assert_eq!(e.get(), 5, "重提的 print(285) 不再执行");
    assert_eq!(
        ctxs.borrow()[1],
        ["需求", "代码：print('大约', 385)\n输出：大约 385\n"],
        "带内那段的接地材料进第 2 轮上下文"
    );
}

/// (f) 同一动作、同一实参只执行一次：第二次按账本键取
#[test]
fn f_同一实参只执行一次() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let r = verify([mat(\"print(385)\"), mat(\"print(385)\")], runner, correct);
{{ok: len(accepted(r)), pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(v["ok"], json!(2));
    assert_eq!(e.get(), 1);
}

/// (g) 来源与 taint：接地产物的 taint 随执行器（untrusted），来源里有 transform 与 do
#[test]
fn g_接地产物的_taint_与来源() {
    let seen = RefCell::new(vec![]);
    let (e, c) = 计数();
    let acts = 动作表(e.clone(), c.clone());
    let ports = Ports::new().with(判断端口(&seen));
    let src = format!(
        "{头}let m = runner(mat(\"print(385)\"));
let r = verify([mat(\"print(385)\")], runner, correct);
{{item: accepted(r)[0].item, taint: taint(accepted(r)[0].exit), pending: r.pending}}"
    );
    let o = 跑(&src, ports, &acts).unwrap();
    let v = 值(&o);
    assert_eq!(v["item"]["taint"], json!("Untrusted"), "{v}");
    let origin = v["item"]["origin"].to_string();
    assert!(origin.contains("transform"), "{origin}");
    assert_eq!(
        e.get(),
        1,
        "runner 直接调用与 verify 里同一实参，只执行一次"
    );
}
