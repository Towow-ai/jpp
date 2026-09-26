//! 两类**放行方向**缺陷的回归（设计总账 K-069 / K-075，K-182 / K-203；2026-09-23 修）。
//!
//! 1. 推测执行与向量化越过用户函数执行 `do`：`has_impure` 只认内置名，调用用户函数一律当纯。
//!    条件为假的分支里 `state(mat(side(1)))` 被提前求值，`side` 里的 `do` 真的执行了。
//! 2. taint 洗白：不可信内容经 `+` 拼接、`join`、`text`、`m.content`，或经不可信 `do` 的 `Fail`，
//!    再 `mat()` 就成了可信材料，放行了不可逆 `do`（宪法 IFC 行，J-08）。
//!
//! 每条都是**修前失败、修后通过**的最小程序，原样记在
//! `地基/过程记录/2026-09-23-修复放行缺陷.md`。

mod common;
use std::cell::RefCell;
use std::rc::Rc;

use jpp::TaintOut;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{lower, syntax::parse};

/// 判断按 `p` 恒定作答、把每道题记进共享日志；不该生成、不该问人
/// （步 15c：原 `impl Client` 的桩改为闭包端口，用 `Rc<RefCell<..>>` 与动作表共享日志）
fn 记序端口(p: f64, 日志: Rc<RefCell<Vec<String>>>) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            for q in qs {
                日志.borrow_mut().push(format!("judge:{}", q.text));
            }
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

fn 动作表(日志: &Rc<RefCell<Vec<String>>>) -> ActionRegistry {
    let mut a = ActionRegistry::new();
    let l = 日志.clone();
    a.register("记一笔", 0.0, true, TaintOut::Trusted, move |args| {
        l.borrow_mut().push(format!("do:记一笔{}", args.len()));
        Ok(Value::text("记了"))
    });
    let l = 日志.clone();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        l.borrow_mut().push("do:发邮件".into());
        Ok(Value::text("已发"))
    });
    a.register("取外部数据", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::text("外面来的一段内容"))
    });
    a.register("外部失败", 0.0, true, TaintOut::Untrusted, |_| {
        Err("对方服务超时".into())
    });
    a
}

fn 跑(src: &str, p: f64, passes: Passes) -> (Result<jpp::Outcome, String>, Vec<String>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 日志 = Rc::new(RefCell::new(vec![]));
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.65, 0.35, 100);
    let actions = 动作表(&日志);
    let mut ledger = Ledger::new();
    let budget = program.budget.clone();
    let mut it = Interp::new(
        记序端口(p, 日志.clone()),
        &mut ledger,
        &calib,
        &actions,
        budget,
    );
    it.passes = passes;
    let out = it.run(&program).map_err(|e| e.render());
    let l = 日志.borrow().clone();
    (out, l)
}

// ---------- 1. 推测执行不得越过用户函数执行 do ----------

const 推测越界: &str = r#"
budget {calls: 6, cost: 1, depth: 16};
fn side(x) -> Mat !{do} { do("记一笔", [x], 0) }
let 条件 = handle(cut(judge(state(mat("条件材料")), test("条件成立吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let r = if 条件 {
    handle(cut(judge(state(side(1)), test("分支题", "k"))), {
        act: fn() { 1 }, ignore: fn() { 2 },
        unsure: fn(u) { consume(u, "drop"); 3 }})
} else { 0 };
{r: r}
"#;

#[test]
fn 推测执行不越过用户函数执行do() {
    // p=0.1：条件判为 ignore，分支不该走，side 里的 do 一次都不该执行
    let (out, 日志) = 跑(推测越界, 0.1, Passes::default());
    let out = out.unwrap_or_else(|e| panic!("应当跑完：{e}"));
    assert_eq!(out.value_json()["r"], serde_json::json!(0));
    assert!(
        !日志.iter().any(|x| x.starts_with("do:")),
        "未走的分支里的 do 被执行了：{日志:?}"
    );
    // 与关掉全部 pass 的结果一致
    let (_, 无pass日志) = 跑(推测越界, 0.1, Passes::none());
    assert_eq!(日志, 无pass日志, "开 pass 与关 pass 的效应序列应一致");
}

const 向量化越界: &str = r#"
budget {calls: 10, cost: 1, depth: 64};
fn side(x) -> Mat !{do} { do("记一笔", [x], 0) }
let r = map([1, 2, 3], fn(i) !{judge, do} {
    let e = cut(judge(state(side(i)), test("行吗", "k")));
    handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 2 }})
});
{r: r}
"#;

#[test]
fn 向量化不把后续各轮的do提前() {
    let (out, 日志) = 跑(向量化越界, 0.9, Passes::default());
    out.unwrap_or_else(|e| panic!("应当跑完：{e}"));
    let (_, 无pass日志) = 跑(向量化越界, 0.9, Passes::none());
    assert_eq!(日志, 无pass日志, "do 与 judge 的先后次序被 pass 改变了");
}

// ---------- 2. taint 不得经派生路径洗白 ----------

fn 守卫程序(派生: &str) -> String {
    format!(
        r#"
budget {{calls: 3, cost: 1, depth: 8}};
let 脏 = do("取外部数据", [], 0);
let 拆了 = content(脏);
let 洗白了 = {派生};
let 可以发吗 = handle(cut(judge(state(洗白了), test("该发吗", "k"))), {{
    act: fn() {{ true }}, ignore: fn() {{ false }},
    unsure: fn(u) {{ consume(u, "drop"); false }}}});
{{r: if 可以发吗 {{ content(do("发邮件", [], 0)) }} else {{ "没发" }}}}
"#
    )
}

#[test]
fn 派生文本都不能洗白() {
    for (名, 派生) in [
        ("拼接", r#"mat(拆了 + "！")"#),
        ("前缀拼接", r#"mat("转述：" + 拆了)"#),
        ("join", r#"mat(join([拆了, "!"], ""))"#),
        ("字段取内容", r#"mat(脏.content + "")"#),
        ("失败值转文字", r#"mat(text(do("外部失败", [], 0)))"#),
        ("text", r#"mat(text({x: 拆了}))"#),
        ("记录再拼接", r#"mat({outer: 拆了 + "。"})"#),
    ] {
        let (out, 日志) = 跑(&守卫程序(派生), 0.9, Passes::default());
        let e = out.err().unwrap_or_else(|| {
            panic!("{名}：派生的不可信内容不该洗白成可信，放行了不可逆 do：{日志:?}")
        });
        assert!(e.contains("J-08"), "{名} 该是 J-08：{e}");
        assert!(
            !日志.iter().any(|x| x == "do:发邮件"),
            "{名}：不可逆 do 被执行了"
        );
    }
}

#[test]
fn 不可信do的失败值不能洗白() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let f = do("外部失败", [], 0);
let 可以发吗 = handle(cut(judge(state(mat(f)), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); true }});
{r: if 可以发吗 { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let (out, 日志) = 跑(src, 0.9, Passes::default());
    let e = out
        .err()
        .unwrap_or_else(|| panic!("不可信动作的失败值不该成为可信材料：{日志:?}"));
    assert!(e.contains("J-08"), "该是 J-08：{e}");
    assert!(!日志.iter().any(|x| x == "do:发邮件"));
}

/// 反面：程序自己的字面量不受牵连（不能为了堵洞把可信的也拦掉）。
#[test]
fn 无关字面量不被误伤() {
    let src = r#"
budget {calls: 3, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 拆了 = content(脏);
let 干净 = mat("源码里写的另一句话" + "，与外部内容无关");
let 可以发吗 = handle(cut(judge(state(干净), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
{r: if 可以发吗 { content(do("发邮件", [], 0)) } else { "没发" }, x: len(拆了)}
"#;
    let (out, _) = 跑(src, 0.9, Passes::default());
    let out = out.unwrap_or_else(|e| panic!("无关字面量不该被拦：{e}"));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}
