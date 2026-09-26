//! **J-08 在这门语言最推荐的那条写法上完全不生效。**
//!
//! `self.guards` **只在 `ExprKind::If` 一处 push**，而 J-08 的判定是
//! `if !action.reversible && !self.guards.is_empty()` ——**空栈直接跳过整条检查**。
//! 于是写在 `handle` 臂里的不可逆 `do`，在 J-08 眼里是「无条件执行」，完全不受管。
//!
//! **而那不是怪写法，它就是 §5 与 J-05 推荐的写法**：每个 Unsure 都要被 handler 消费。
//!
//! **`tests/guard.rs` 十一条全绿照不出来，因为十一条全走 `if`。**
//!
//! 判据（总控订正）：**handler 的 `act` 臂根本不是无条件的——它之所以执行，
//! 正是因为那次 `cut` 切出了 `Act`。那个出口就是守卫。**

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::Value;
use jpp::value::{Answer, Question, State, Taint};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 步 15c：原 `impl Client for 桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
/// 改为一个 judge 闭包端口，`p` 是 `Op::Test` 的固定答案。
fn 桩端口<'a>(p: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        // **答案形状要跟题走**：给 select 题回 Noul，撞的是 validate_answer，不是 J-08
        let answers = qs
            .iter()
            .map(|q| match q.op {
                jpp::value::Op::Select => {
                    // B63：select 带 δ 迟滞（δ = 0.15），胜出概率须 ≥ hi + δ = 0.95；取 0.99 留出余量
                    let mut v = vec![0.01; s.over.len().max(1)];
                    v[0] = 1.0 - 0.01 * (s.over.len().max(1) - 1) as f64;
                    Answer::Choice(v)
                }
                jpp::value::Op::Measure => Answer::Score(q.scale.iter().map(|_| 0.5).collect()),
                jpp::value::Op::Test => Answer::Noul(p),
            })
            .collect();
        // select 题要报置换一致率，否则走的是 `Unsure(untested:permutation)`（J-15 那一位），
        // **`pick` 臂根本不会跑**——那样测的就不是 J-08 了。
        let (ms, pm) = qs
            .iter()
            .map(|q| match q.op {
                jpp::value::Op::Select => (Some(1.0), 2),
                _ => (None, 0),
            })
            .collect::<(Vec<_>, Vec<_>)>();
        Ok(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
            mode_share: ms,
            perms: pm,
            confidence: vec![],
        })
    }))
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    // 取外部数据：可逆，产物 **untrusted**
    a.register("取外部数据", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::Text("外面来的".into(), Taint::Trusted.into()))
    });
    // 取内部数据：可逆，产物 trusted
    a.register("取内部数据", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::Text("自己的".into(), Taint::Trusted.into()))
    });
    // 发邮件：**不可逆**
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

fn 跑(src: &str) -> Result<(Json, Vec<String>), String> {
    跑p(src, 0.95)
}

fn 跑p(src: &str, p: f64) -> Result<(Json, Vec<String>), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    let mut l = Ledger::new();
    run(&program, 桩端口(p), &calib, &动作表(), &mut l)
        .map(|o| (o.value_json(), o.trace.warnings.clone()))
        .map_err(|e| e.render())
}

/// **A：`do` 在 `if` 体内**——这条今天就拦得住，是对照臂。
#[test]
fn a_写在if体内拦得住() {
    let e = 跑(r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 该发 = handle(cut(judge(state(脏), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
if 该发 { content(do("发邮件", [], 0)) } else { "没发" }
"#)
    .expect_err("该被 J-08 拦住");
    assert!(e.contains("J-08"), "{e}");
}

/// **B：`do` 在 handler 的 `act` 臂里**——**这一条是这一包的红**。
/// 同一份不可信材料、同一道判断、同一个不可逆动作，**唯一差别是 `do` 写在哪里**。
#[test]
fn b_写在handler臂里也要拦得住() {
    let r = 跑(r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
handle(cut(judge(state(脏), test("该发吗", "k"))), {
    act: fn() { content(do("发邮件", [], 0)) },
    ignore: fn() { "没发" },
    unsure: fn(u) { consume(u, "drop"); "没发" }})
"#);
    let e = r.expect_err("**修前这里放行了 {\"r\":\"已发\"}，J-08 一个字都没有**");
    assert!(e.contains("J-08"), "{e}");
}

/// **正面：可信状态上切出来的 `Act`，臂里的不可逆 `do` 要照常通过。**
///
/// 这一条是这次改动的**回归闸**：改完之后**每个 `handle` 都会压一层守卫**，
/// 于是 `guards.is_empty()` 在一大批以前为真的地方变成了假。
/// **没有这一条，就会发出一个把正常程序也拦住的规则，而且要等 `partial.rs` 才发现。**
#[test]
fn 可信状态上的臂照常通过() {
    let (v, _) = 跑(r#"
budget {calls: 4, cost: 1, depth: 8};
let 干净 = do("取内部数据", [], 0);
handle(cut(judge(state(干净), test("该发吗", "k"))), {
    act: fn() { content(do("发邮件", [], 0)) },
    ignore: fn() { "没发" },
    unsure: fn(u) { consume(u, "drop"); "没发" }})
"#)
    .expect("可信状态上的判断该放行");
    assert_eq!(v, json!("已发"));
}

/// **`unsure` 臂也要压守卫。** 那里拿到的是未决责任，**本来就不是一个放行判定**——
/// 所以它不提供 `trusted` 合取项。但**不压就是 `guards.is_empty()` → 不受管 → `do` 自由执行**，
/// **那正是这一包在修的同一个洞**。
#[test]
fn unsure臂里的不可逆do也受管() {
    // p=0.5 落在 0.8/0.2 的带内 → 走 unsure 臂
    let e = 跑p(
        r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
handle(cut(judge(state(脏), test("该发吗", "k"))), {
    act: fn() { "发了" },
    ignore: fn() { "没发" },
    unsure: fn(u) { consume(u, "drop"); content(do("发邮件", [], 0)) }})
"#,
        0.5,
    )
    .expect_err("unsure 臂里的不可逆 do 不该自由执行");
    assert!(e.contains("J-08"), "{e}");
}

/// **`pick` 臂与 `act` 臂是同一件事。** 总控只点名了 `act`/`ignore`，
/// 但 `Pick`/`At` 同样是「这一臂之所以跑，是因为那个出口这么切了」——
/// **漏掉它就是把同一个洞挪过去一个枚举分支。**
#[test]
fn pick臂同样受管() {
    let e = 跑(r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let s = state(mat("对象"), {over: [mat("甲"), mat("乙")], ctx: [脏]});
handle(cut(judge(s, select("挑一个", "k"))), {
    pick: fn(k) { content(do("发邮件", [], 0)) },
    unsure: fn(u) { consume(u, "drop"); "没发" }})
"#)
    .expect_err("pick 臂里的不可逆 do 同样不该自由执行");
    assert!(e.contains("J-08"), "{e}");
}

/// **嵌套时按合取语义放行，把这个形状钉住。**
/// 外层 `if` 里有一个 trusted 合取项时，内层脏出口的臂**不再拦**——
/// 这与 `if` 今天的语义一致（`any` 一个可信合取项即放行），
/// **但它是一个失败开放的形状，要写明而不是日后再发现**。
#[test]
fn 外层可信守卫会让内层脏臂通过这个形状已钉住() {
    let r = 跑(r#"
budget {calls: 6, cost: 1, depth: 8};
let 干净 = do("取内部数据", [], 0);
let 脏 = do("取外部数据", [], 0);
let 可信判断 = handle(cut(judge(state(干净), test("行吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
if 可信判断 {
    handle(cut(judge(state(脏), test("该发吗", "k"))), {
        act: fn() { content(do("发邮件", [], 0)) },
        ignore: fn() { "没发" },
        unsure: fn(u) { consume(u, "drop"); "没发" }})
} else { "没发" }
"#);
    assert!(
        r.is_ok(),
        "**合取语义下这里放行**——形状钉住，不是意外：{r:?}"
    );
}

/// **证书只界定放行那一侧。** `commission` 强制 `lo = 0.0`，于是任何拿到证书的键，
/// `Ignore` 实际不可达（`p <= 0` 才出），**而 J-05 仍然强制作者为那条永不执行的分支写代码**。
///
/// 原注释给的理由是「说不准往拒绝那边倒」，**而那句话把「拒绝」默认等同于「不给 `Act`」**。
/// **`Act` 与 `Ignore` 在出口代数里是对称的两个判定**，哪一个安全取决于程序怎么用——
/// 写 `if 不安全(x) { 拦下 }` 的时候，**`Ignore` 才是放行的那个答案**。
#[test]
fn 证书要说出自己只界定哪一侧() {
    use jpp::effects::{LiteralMode, Sample};
    let mut c = CalibStore::new();
    for i in 0..30 {
        c.absorb(
            "k",
            Sample {
                p: Some(0.30 + i as f64 * 0.02),
                label: Some(if i > 6 { 1 } else { 0 }),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
    let cert = c.commission("k", 0.45, 0.10, "条").expect("认得动");
    // **这件事要是数据，不是一行注释**
    assert!(
        cert.bounded_side.contains("p ≥ hi"),
        "要说出界的是哪一侧：{}",
        cert.bounded_side
    );
    assert!(
        cert.bounded_side.contains("lo"),
        "并且要说出另一侧的实况：{}",
        cert.bounded_side
    );
    assert_eq!(c.get("k").lo, 0.0);
    println!("证书的单侧声明：{}", cert.bounded_side);
}

#[allow(dead_code)]
fn _用到(t: Taint) -> bool {
    t == Taint::Trusted
}
