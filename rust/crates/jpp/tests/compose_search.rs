//! 搭配层 `search` 与 `carry`（步 25c，B148）：提出 → 接地 → 可行域 → 目标（是非题）→ 前 width 个成前沿，
//! iterate 的终止线；未决三条去向（carry / refine / 交人）；失败的一轮 = 零候选 + 一条 `Unsure(fail)`；
//! 两层嵌套（search 嵌进 map 再 carry 到外层；search 的 propose 里再调 sieve）；每轮的判断层数；ground 槽；
//! 判过的候选不再判、refine 第一轮的宽限、好候选按轮累积、value 元素的合成出口。闭包端口，不发请求。
//!
//! 依据：B148；B132（iterate）；B131、B161（compose、cert）；主会话 2026-09-26（失败一轮的口径；每个原语
//! 至少一个两层嵌套用例；Q4–Q6、Q10 裁定）；预注册 `地基/过程记录/工程-步25c.md` 一·2 (a)–(g)、三·2 (h)–(i)、
//! 三·4 (k)、五·2（累积后的断言与 (j)）。

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::path::PathBuf;
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

/// 是非题按题与材料定读数：「合适」题——含「好」0.9、含「待」0.5、其余 0.1；「好记」题——含「记」0.9、
/// 含「待」0.5、其余 0.1；「参照」题——含「好」0.9、其余 0.1。记下每次判的 (题, 材料)。
fn 判断端口<'a>(seen: &'a RefCell<Vec<(String, String)>>) -> FnPort<'a> {
    FnPort::judge("fixed-0", move |s, qs| {
        let text = s.on_text();
        Ok::<_, EffectError>(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    seen.borrow_mut().push((q.text.clone(), text.clone()));
                    let p = if q.text.contains("好记") {
                        if text.contains('记') {
                            0.9
                        } else if text.contains('待') {
                            0.5
                        } else {
                            0.1
                        }
                    } else if text.contains('好') {
                        0.9
                    } else if text.contains('待') && q.text.contains("合适") {
                        0.5
                    } else {
                        0.1
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

type 生成表 = dyn Fn(&[Json], u64) -> Result<Vec<&'static str>, String>;

/// 生成端口：输出由 `table(上下文, retry_seq)` 定，`Err` 当生成器报的失败（运行时产出 `Fail`）；记下每次收到的上下文
fn 生成端口<'a>(ctxs: &'a RefCell<Vec<Vec<Json>>>, table: &'a 生成表) -> FnPort<'a> {
    FnPort::generate("fixed-0", move |_p, ctx, _n, retry| {
        ctxs.borrow_mut().push(ctx.to_vec());
        Ok(match table(ctx, retry) {
            Ok(items) => GenResult {
                outputs: items.into_iter().map(|t| json!(t)).collect(),
                ..Default::default()
            },
            Err(why) => GenResult {
                failure: Some(why),
                ..Default::default()
            },
        })
    })
}

/// 按 retry_seq 取固定的一轮输出
fn 逐轮(
    rounds: &'static [&'static [&'static str]],
) -> impl Fn(&[Json], u64) -> Result<Vec<&'static str>, String> {
    move |_ctx, retry| Ok(rounds[retry as usize].to_vec())
}

/// 把程序写进 `target/` 下的临时目录（import 只收相对路径），经装载器装上库再跑
fn 跑(src: &str, ports: Ports<'_>) -> Result<Outcome, String> {
    let dir = root().join(format!(
        "target/compose-search-{}-{}",
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
    let acts = ActionRegistry::new();
    Session::new(ports, &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

const 头: &str = r#"import "../../lib/compose/search.jpp";
import "../../lib/compose/carry.jpp";
budget {calls: 60, cost: 0, depth: 64};
let brief = mat("需求");
let propose = fn(frontier, i) { gen("提 3 个候选", concat([brief], frontier), 3, i) };
let fits = test("这个候选合适吗？", "k");
let memorable = test("这个候选好记吗？", "k");
"#;

fn 值(o: &Outcome) -> Json {
    o.value_json()
}

/// 材料列表 → 内容列表
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

fn 上下文(c: &[Json]) -> Vec<String> {
    c.iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

/// (a) carry：带内候选进 pending，via 记 "search#0"，trail 不动；程序照常返回（J-05 满足）
#[test]
fn a_carry_未决带_via_上传() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[&["好甲", "待乙", "坏丙"]]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, unit, 1, {{width: 2}});
{{kept: map(r.value, fn(e) {{ e.item }}), items: map(r.pending, fn(p) {{ p.item }}),
  via: map(r.pending, fn(p) {{ p.via }}), trails: map(r.pending, fn(p) {{ len(p.trail) }}),
  causes: map(r.pending, fn(p) {{ p.cause }}), reason: r.detail.reason, pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(内容(&v["kept"]), ["好甲"]);
    assert_eq!(内容(&v["items"]), ["待乙"]);
    assert_eq!(v["via"], json!([["search#0"]]));
    assert_eq!(v["trails"], json!([0]), "层名不进 trail（Q1）");
    assert_eq!(v["causes"], json!(["band"]));
    assert_eq!(v["reason"], json!("bound"));
    assert_eq!(o.returned_unsure, ["unsure(band)"]);
}

/// (b) refine：带内候选的材料并进下一轮 propose 的上下文，出口仍在 pending；生成器把它原样再提一次时
/// 不再判、按原出口计（每份材料至多判一次）
#[test]
fn b_refine_未决材料进下一轮前沿() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[
        &["好甲", "待乙", "坏丙"],
        &["好丁", "好戊", "坏己"],
        &["好庚", "好辛", "好壬"],
    ]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, unit, 3, {{width: 3, unsure_to: \"refine\"}});
{{kept: map(r.value, fn(e) {{ e.item }}), items: map(r.pending, fn(p) {{ p.item }}),
  via: map(r.pending, fn(p) {{ p.via }}), reason: r.detail.reason, duplicates: r.detail.duplicates,
  pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    let c = ctxs.borrow();
    assert_eq!(c.len(), 2, "第 2 轮后累积的好候选到 width 3");
    assert_eq!(
        上下文(&c[1]),
        ["需求", "好甲", "待乙"],
        "带内的「待乙」并进第 2 轮前沿"
    );
    assert_eq!(内容(&v["items"]), ["待乙"], "细化过的出口照样在 pending");
    assert_eq!(v["via"], json!([["search#0"]]));
    assert_eq!(内容(&v["kept"]), ["好甲", "好丁", "好戊"]);
    assert_eq!(v["reason"], json!("stop"));

    // 生成器把「待乙」原样再提一次：判过的不再判（Q6），按原出口计；第 2 轮有新的好候选「好丁」，照样往下走
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[
        &["好甲", "待乙", "坏丙"],
        &["好丁", "待乙", "坏己"],
        &["好庚", "好辛", "好壬"],
    ]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["reason"], json!("stop"));
    assert_eq!(v["duplicates"], json!(1));
    assert_eq!(ctxs.borrow().len(), 3);
    assert_eq!(上下文(&ctxs.borrow()[2]), ["需求", "好甲", "好丁"]);
    let 待乙判了 = seen.borrow().iter().filter(|(_, m)| m == "待乙").count();
    assert_eq!(待乙判了, 1, "重提的「待乙」不再判");
    assert_eq!(内容(&v["items"]), ["待乙"], "未决的「待乙」只记一次");
}

/// (c) 交人：unsure_to 给 ask_human，带内候选用它未决时的那道题交人（可行域未决的用可行域题，
/// 目标未决的用目标题）；没人答时在第一次问人处挂起，挂起原因是这次问人（不是 escalate 次数上限）；
/// 有人答时程序返回，pending 元素的出口换成人的回答
#[test]
fn c_ask_交人_没答挂起() {
    let src = format!(
        "import \"../../lib/compose/ask.jpp\";\n{}",
        头.replace("depth: 64}", "depth: 64, escalate: 2}")
    ) + "let r = search([], propose, fits, memorable, 1, {width: 3, unsure_to: ask_human});
{kept: map(r.value, fn(e) { e.item }), kinds: map(r.pending, fn(p) { exit_kind(p.exit) }), pending: r.pending}";
    let 可行域问 = ("这个候选合适吗？".to_string(), "待乙".to_string());
    let 目标问 = ("这个候选好记吗？".to_string(), "好待丙".to_string());

    for 答 in [None, Some(Answer::Noul(0.95))] {
        let seen = RefCell::new(vec![]);
        let ctxs = RefCell::new(vec![]);
        let asked = RefCell::new(vec![]);
        // 好记甲：两题都过；待乙：可行域带内；好待丙：可行域过、目标带内
        let t = 逐轮(&[&["好记甲", "待乙", "好待丙"]]);
        let ports = Ports::new()
            .with(判断端口(&seen))
            .with(生成端口(&ctxs, &t))
            .with(FnPort::ask("fixed-0", |s, q| {
                asked.borrow_mut().push((q.text.clone(), s.on_text()));
                Ok(答.clone())
            }));
        let o = 跑(&src, ports).unwrap();
        match 答 {
            None => {
                assert_eq!(
                    *asked.borrow(),
                    std::slice::from_ref(&可行域问),
                    "第一次问人没答即挂起"
                );
                assert!(o.value.is_none(), "没人答：挂起");
                let causes: Vec<&str> = o.pending.iter().map(|p| p.cause.as_str()).collect();
                assert_eq!(
                    causes,
                    ["ask"],
                    "挂起的原因是这次问人，不是次数上限（budget.escalate）"
                );
                assert!(
                    o.pending[0].detail.contains("这个候选合适吗？"),
                    "{:?}",
                    o.pending
                );
            }
            Some(_) => {
                assert_eq!(
                    *asked.borrow(),
                    [可行域问.clone(), 目标问.clone()],
                    "各用各的题问"
                );
                let v = 值(&o);
                assert_eq!(内容(&v["kept"]), ["好记甲"]);
                assert_eq!(
                    v["kinds"],
                    json!(["act", "act"]),
                    "人答了，pending 元素的出口是人的回答"
                );
            }
        }
    }
}

/// (d) 失败的一轮：propose 返回 Fail，这一轮零候选、pending 多一条 cause "fail"（带 via），
/// 前沿不变，下一轮照常（失败不算停滞，measure 减 1）
#[test]
fn d_失败一轮_零候选加一条_unsure_fail() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = |_ctx: &[Json], retry: u64| -> Result<Vec<&'static str>, String> {
        match retry {
            0 => Err("timeout: 测试里的假失败".into()),
            _ => Ok(vec!["好甲", "好乙", "坏丙"]),
        }
    };
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, unit, 3, {{width: 2}});
{{kept: map(r.value, fn(e) {{ e.item }}), causes: map(r.pending, fn(p) {{ p.cause }}),
  via: map(r.pending, fn(p) {{ p.via }}), failed: map(r.pending, fn(p) {{ is_fail(p.item) }}),
  reason: r.detail.reason, rounds: r.detail.rounds, fails: r.detail.fails,
  measures: r.detail.measures, pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["causes"], json!(["fail"]));
    assert_eq!(v["via"], json!([["search#0"]]));
    assert_eq!(v["failed"], json!([true]));
    assert_eq!(v["fails"], json!(1));
    assert_eq!(v["reason"], json!("stop"), "失败后下一轮照常，凑够 width");
    assert_eq!(v["rounds"], json!(2));
    assert_eq!(
        v["measures"],
        json!([8, 7]),
        "失败一轮 measure 减 1，不触发 noshrink"
    );
    assert_eq!(内容(&v["kept"]), ["好甲", "好乙"]);
    assert_eq!(上下文(&ctxs.borrow()[1]), ["需求"], "失败后前沿不变");
    assert_eq!(o.returned_unsure, ["unsure(fail)"]);
}

/// (e1) 两层嵌套：两份需求各跑一次 search（嵌在 map 里），两次的 pending 经 carry 并进外层，
/// via 记下两层；程序照常返回（J-05 满足）
#[test]
fn e1_map_里的两次搜索_carry_到外层() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = |ctx: &[Json], _retry: u64| -> Result<Vec<&'static str>, String> {
        Ok(match ctx[0].as_str().unwrap() {
            "需求一" => vec!["好一甲", "待一乙", "坏一丙"],
            _ => vec!["好二甲", "待二乙", "坏二丙"],
        })
    };
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let briefs = [mat(\"需求一\"), mat(\"需求二\")];
let rs = map(briefs, fn(b) {{
    search([], fn(frontier, i) {{ gen(\"提 3 个候选\", concat([b], frontier), 3, i) }}, fits, unit, 1, {{width: 2}})
}});
let all = carry(carry([], rs[0].pending, \"brief#0\"), rs[1].pending, \"brief#1\");
{{kept: map(rs, fn(r) {{ map(r.value, fn(e) {{ content(e.item) }}) }}), items: map(all, fn(p) {{ p.item }}),
  via: map(all, fn(p) {{ p.via }}), pending: all}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["kept"], json!([["好一甲"], ["好二甲"]]));
    assert_eq!(内容(&v["items"]), ["待一乙", "待二乙"]);
    assert_eq!(
        v["via"],
        json!([["search#0", "brief#0"], ["search#0", "brief#1"]])
    );
    assert_eq!(o.returned_unsure.len(), 2);
}

/// (e2) 两层嵌套：search 的 propose 里先用另一道题 sieve 前沿，只把能当参照的交给生成器（构造嵌进搭配）
#[test]
fn e2_propose_里再调_sieve() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[&["好甲", "待乙", "坏丙"], &["好丁", "好戊", "坏己"]]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let anchor = test(\"这个候选能当参照吗？\", \"k\");
let pick = fn(frontier, i) {{
    let pre = sieve(frontier, anchor);
    gen(\"提 3 个候选\", concat([brief], map(accepted(pre), fn(e) {{ e.item }})), 3, i)
}};
let r = search([mat(\"好锚一\"), mat(\"坏锚二\")], pick, fits, unit, 2, {{width: 2}});
{{kept: map(r.value, fn(e) {{ e.item }}), reason: r.detail.reason, pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    let c = ctxs.borrow();
    assert_eq!(
        上下文(&c[0]),
        ["需求", "好锚一"],
        "propose 里的 sieve 滤掉了坏锚"
    );
    assert_eq!(上下文(&c[1]), ["需求", "好甲"]);
    assert_eq!(内容(&v["kept"]), ["好甲", "好丁"], "累积前沿的前 2 个");
    assert_eq!(v["reason"], json!("stop"));
}

/// (f) 每轮的判断层数：objective 为 unit 时每轮 1 层；给了是非题目标时每轮 2 层（可行域与目标有依赖）
#[test]
fn f_每轮判断层数() {
    const 无目标: &[&[&str]] = &[&["好甲", "待乙", "坏丙"], &["好记丁", "好记戊", "坏己"]];
    const 有目标: &[&[&str]] = &[&["好记甲", "待乙", "坏丙"], &["好记丁", "好记戊", "坏己"]];
    for (objective, table, per_round) in [("unit", 无目标, 1usize), ("memorable", 有目标, 2)]
    {
        let seen = RefCell::new(vec![]);
        let ctxs = RefCell::new(vec![]);
        let t = 逐轮(table);
        let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
        let src = format!(
            "{头}let r = search([], propose, fits, {objective}, 2, {{width: 3}});
{{kept: map(r.value, fn(e) {{ e.item }}), reason: r.detail.reason, pending: r.pending}}"
        );
        let o = 跑(&src, ports).unwrap();
        let v = 值(&o);
        assert_eq!(
            v["reason"],
            json!("stop"),
            "{objective}：第 2 轮累积到 width 3"
        );
        let first = table[0][0];
        assert_eq!(内容(&v["kept"]), [first, "好记丁", "好记戊"], "{objective}");
        assert_eq!(o.layers.len(), 2 * per_round, "{objective}：{:?}", o.layers);
    }
}

/// (g) ground 槽：判断看到的是接地后的材料，留下的元素也是接地后的材料
#[test]
fn g_ground_接地后再判() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[&["好甲", "待乙", "坏丙"]]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, unit, 1, {{width: 1, ground: fn(m) {{ mat(\"候选：\" + content(m)) }}}});
{{kept: map(r.value, fn(e) {{ e.item }}), pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(内容(&v["kept"]), ["候选：好甲"]);
    let judged: Vec<String> = seen.borrow().iter().map(|(_, m)| m.clone()).collect();
    assert_eq!(judged, ["候选：好甲", "候选：待乙", "候选：坏丙"]);
}

/// (h) 判过的候选不再判（Q6）：本轮内的重复与重提的旧候选都按原出口计、计入 duplicates；重提旧的好候选
/// 再加一个新的好候选算进展（Q10 累积）；一轮全是旧候选即没有新的好候选，按停滞以 noshrink 结束，不走 repeat
#[test]
fn h_判过的不再判_全重复算零新候选() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[
        &["好甲", "待乙", "好甲"],
        &["好甲", "好丁", "坏戊"],
        &["好丁", "好甲", "坏戊"],
    ]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, unit, 3, {{width: 4}});
{{kept: map(r.value, fn(e) {{ e.item }}), found_in: map(r.value, fn(e) {{ e.round }}), reason: r.detail.reason,
  rounds: r.detail.rounds, duplicates: r.detail.duplicates, measures: r.detail.measures, pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(
        v["reason"],
        json!("noshrink"),
        "第 3 轮全是旧候选，没有新的好候选"
    );
    assert_eq!(v["rounds"], json!(3));
    assert_eq!(v["duplicates"], json!(5), "第 1 轮 1、第 2 轮 1、第 3 轮 3");
    assert_eq!(
        v["measures"],
        json!([16, 12, 8, 8]),
        "第 2 轮旧好候选加新的「好丁」，算进展"
    );
    assert_eq!(内容(&v["kept"]), ["好甲", "好丁"]);
    assert_eq!(v["found_in"], json!([0, 1]));
    let judged: Vec<String> = seen.borrow().iter().map(|(_, m)| m.clone()).collect();
    assert_eq!(judged, ["好甲", "待乙", "好丁", "坏戊"], "每份材料只判一次");
}

/// (i) refine 的第一个没失败的轮次一个好的都没有时不算停滞（Q4），细化的候选进第 2 轮；只宽限一次；
/// 同样的输入在 carry 下第 1 轮后就 noshrink，value 为空
#[test]
fn i_refine_第一轮零好候选宽限一轮() {
    let run = |how: &str, rounds: &'static [&'static [&'static str]]| {
        let seen = RefCell::new(vec![]);
        let ctxs = RefCell::new(vec![]);
        let t = 逐轮(rounds);
        let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
        let src = format!(
            "{头}let r = search([], propose, fits, unit, 3, {{width: 2, unsure_to: \"{how}\"}});
{{kept: map(r.value, fn(e) {{ e.item }}), found_in: map(r.value, fn(e) {{ e.round }}), reason: r.detail.reason,
  rounds: r.detail.rounds, measures: r.detail.measures, pending: r.pending}}"
        );
        let o = 跑(&src, ports).unwrap();
        let c: Vec<Vec<String>> = ctxs.borrow().iter().map(|c| 上下文(c)).collect();
        (值(&o), c)
    };
    const 先无后有: &[&[&str]] = &[
        &["待甲", "坏乙", "坏丙"],
        &["好丁", "好戊", "坏己"],
        &["好庚", "好辛", "好壬"],
    ];
    const 两轮都无: &[&[&str]] = &[
        &["待甲", "坏乙", "坏丙"],
        &["坏丁", "坏戊", "坏己"],
        &["好庚", "好辛", "好壬"],
    ];

    let (v, c) = run("refine", 先无后有);
    assert_eq!(v["reason"], json!("stop"));
    assert_eq!(v["rounds"], json!(2));
    assert_eq!(v["measures"], json!([8, 7]), "宽限一轮：停滞度减 1");
    assert_eq!(c[1], ["需求", "待甲"], "带内的「待甲」细化进第 2 轮");
    assert_eq!(内容(&v["kept"]), ["好丁", "好戊"]);
    assert_eq!(v["found_in"], json!([1, 1]));

    let (v, _) = run("refine", 两轮都无);
    assert_eq!(v["reason"], json!("noshrink"), "只宽限一次");
    assert_eq!(v["rounds"], json!(2));

    let (v, c) = run("carry", 先无后有);
    assert_eq!(v["reason"], json!("noshrink"), "carry 不宽限");
    assert_eq!(v["rounds"], json!(1));
    assert_eq!(c.len(), 1);
    assert_eq!(v["kept"], json!([]));
}

/// (j) 好候选按轮累积（Q10，取代 Q5）：生成器把旧的好候选带回来、再加一个新的，搜索照样往下走，value 是
/// 累积前沿，每个元素标第一次判好的轮次；一轮只有旧的好候选、没有新的，算停滞
#[test]
fn j_好候选按轮累积() {
    let run = |rounds: &'static [&'static [&'static str]]| {
        let seen = RefCell::new(vec![]);
        let ctxs = RefCell::new(vec![]);
        let t = 逐轮(rounds);
        let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
        let src = format!(
            "{头}let r = search([], propose, fits, unit, 3, {{width: 3}});
{{kept: map(r.value, fn(e) {{ e.item }}), found_in: map(r.value, fn(e) {{ e.round }}), reason: r.detail.reason,
  rounds: r.detail.rounds, duplicates: r.detail.duplicates, pending: r.pending}}"
        );
        let o = 跑(&src, ports).unwrap();
        let c: Vec<Vec<String>> = ctxs.borrow().iter().map(|c| 上下文(c)).collect();
        (值(&o), c)
    };
    const 旧的加新的: &[&[&str]] = &[&["好甲", "好乙", "坏丙"], &["好甲", "好丁", "坏戊"]];
    const 只有旧的: &[&[&str]] = &[&["好甲", "好乙", "坏丙"], &["好甲", "坏戊", "坏己"]];

    let (v, c) = run(旧的加新的);
    assert_eq!(c[1], ["需求", "好甲", "好乙"], "前沿是累积的好候选");
    assert_eq!(
        v["reason"],
        json!("stop"),
        "旧的好候选加一个新的，累积到 width"
    );
    assert_eq!(v["rounds"], json!(2));
    assert_eq!(内容(&v["kept"]), ["好甲", "好乙", "好丁"]);
    assert_eq!(v["found_in"], json!([0, 0, 1]));
    assert_eq!(v["duplicates"], json!(1));

    let (v, _) = run(只有旧的);
    assert_eq!(v["reason"], json!("noshrink"), "只有旧的好候选，没有新的");
    assert_eq!(v["rounds"], json!(2));
    assert_eq!(内容(&v["kept"]), ["好甲", "好乙"]);
    assert_eq!(v["found_in"], json!([0, 0]));
}

/// (k) value 元素的出口是 trail 上全部出口的 compose(…, "all")（B148，25d 开放 compose）：给了目标题时两个分量，
/// cert 读到合成出口的形状（alpha 为 unit、n_unknown 2）；单个出口的 n_unknown 是 1；种类仍是 act
#[test]
fn k_value_元素出口是合成出口() {
    let seen = RefCell::new(vec![]);
    let ctxs = RefCell::new(vec![]);
    let t = 逐轮(&[&["好记甲", "待乙", "坏丙"]]);
    let ports = Ports::new().with(判断端口(&seen)).with(生成端口(&ctxs, &t));
    let src = format!(
        "{头}let r = search([], propose, fits, memorable, 1, {{width: 1}});
let e = r.value[0];
{{kind: exit_kind(e.exit), cert: cert(e.exit), last: cert(e.trail[0]), trail: len(e.trail), pending: r.pending}}"
    );
    let o = 跑(&src, ports).unwrap();
    let v = 值(&o);
    assert_eq!(v["kind"], json!("act"));
    assert_eq!(v["trail"], json!(1), "trail 上是可行域出口");
    assert_eq!(
        v["cert"]["alpha"],
        Json::Null,
        "合成出口的 alpha 为 unit：{v}"
    );
    assert_eq!(v["cert"]["n_unknown"], json!(2), "两个分量各按未知计：{v}");
    assert_eq!(v["cert"]["alpha_bound"], json!(1.0));
    assert_eq!(v["last"]["n_unknown"], json!(1), "单个出口：{v}");
}
