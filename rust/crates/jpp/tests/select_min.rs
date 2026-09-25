//! `select` 的最小断言：**先红再建**。
//!
//! 一条 `select` 走 `JevClient` + 本地替身 transport，断言**出口是 `Unsure`、
//! 而原因不是 `tie`**，并且**带着「这个量没测过」的痕迹**。
//!
//! 今天它该红两次：
//! 1. 原因是 `tie` —— 而 `tie` 的语义是「**测了，不一致**」，这里是「**没测**」；
//! 2. 没有任何 `W-untested` —— 一个被声明为判据、却在本次路径上没被测量的量，
//!    应当留下痕迹（`12` J-15 加宽后的措辞）。
//!
//! **`tie` 兼两职的代价**：两种情形在**账本**里是同一个值，而账本是审计物。
//! 这与「兜底档案的 `hash` 必须是 `None` 而不是兜底值的哈希」是同一条判据。
//! 而且路由真的不同——`tie` 的既定去向是「逐候选 noul」，**而 K-noul 路径本来就是
//! 逐候选 noul，路过去是空转**。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JevClient, Ports};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 一个**能指定 `mode_share`** 的判断端口，判断转发给内层 `JevClient`，只把 `mode_share`
/// 覆写成夹具指定的值。`None` 模拟 K-noul 那条路——那上面根本没有「置换」这回事；`Some(0.5)`
/// 模拟**测了、正逆两序选了不同的候选**（K=2 上判据是二值的）。
/// 步 15c：原 `impl Client` 的桩改为闭包端口（生成/问人程序不会调用，未注册）。
fn 不发置换端口(inner: &RefCell<JevClient>, mode_share: Option<f64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("jev-1.13.0", move |s, qs| {
        let mut r = inner.borrow_mut().judge(s, qs)?;
        // K-noul 路径上 mode_share 无从谈起：它是 None，不是 0 也不是 1
        r.mode_share = qs.iter().map(|_| mode_share).collect();
        Ok(r)
    }))
}

#[test]
fn select没测过置换时不该说成tie() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("对象"), {over: [mat("甲"), mat("乙"), mat("丙")]});
let e = cut(judge(s, select("挑一个", "k")));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let inner = RefCell::new(JevClient::with_transport(
        "jev-1.13.0",
        Box::new(|_b: &Json| {
            Ok(json!({"answers": {"q0": {"probabilities": {"c0": 0.9, "c1": 0.05, "c2": 0.05}}}}))
        }),
    ));
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        不发置换端口(&inner, None),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("程序应当跑完");

    let 出口 = out.value_json()["出口"].as_str().unwrap_or("").to_string();
    // 先把两条红都打印出来，确认第二条不是被第一条挡住的
    println!("【红一】出口 = {出口}");
    println!("【红二】告警 = {:?}", out.trace.warnings);
    // 红一：现在是 tie，而 tie 的语义是「测了，不一致」
    assert!(
        出口.starts_with("unsure") && !出口.contains("tie"),
        "没测过置换不该说成 tie（tie = 测了但不一致；这里是没测）。实际出口 {出口}"
    );
    // 红二：一个被声明为判据、却没被测量的量，要留下痕迹
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested")),
        "「置换一致性没测过」要留痕（J-15 加宽后的措辞）。实际告警：{:?}",
        out.trace.warnings
    );
}

/// 跑一段 select 源码，`mode_share` 由调用方指定（`None` = 没测过，`Some(x)` = 测了）。
fn 跑select(src: &str, mode_share: Option<f64>) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let inner = RefCell::new(JevClient::with_transport(
        "jev-1.13.0",
        Box::new(|_b: &Json| {
            Ok(json!({"answers": {"q0": {"probabilities": {"c0": 0.9, "c1": 0.05, "c2": 0.05}}}}))
        }),
    ));
    let mut ledger = Ledger::new();
    run(
        &program,
        不发置换端口(&inner, mode_share),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("程序应当跑完")
}

/// handler 里把两件事都读出来：`unsure_cause` 是**路由键**，`untested` 是 **J-15 那一位**。
const 读两位的程序: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("对象"), {over: [mat("甲"), mat("乙"), mat("丙")]});
let e = cut(judge(s, select("挑一个", "k")));
handle(e, {
  pick: fn(k) { {原因: "pick", 未测: "", 出口: exit_kind(e)} },
  unsure: fn(u) { consume(u, "drop"); {原因: unsure_cause(u), 未测: untested(u), 出口: exit_kind(e)} }
})
"#;

/// **硬要求一：那一位必须对 handler 可见，不能只进 trace。**
///
/// 理由是**路由真的不同**：`tie` 的既定去向是「逐候选 noul」（§5 handler 库），
/// 而没测过的那条路（K-noul）**本来就是逐候选 noul，路过去是空转**。handler 看不见
/// 那一位，就只能把两种情形当同一件事办——**那正是要消除的东西**。
///
/// 这个测试与下一个成对：同一段程序、同一个 handler，只有 `mode_share` 一个变量不同，
/// **两种情形必须在 handler 里读出不同的值**。任一半单独看都证明不了「分得开」。
#[test]
fn 没测过那一位对handler可见() {
    let out = 跑select(读两位的程序, None);
    let v = out.value_json();
    println!("【没测】{v}");
    assert_eq!(
        v["原因"],
        json!("untested"),
        "路由键要是通用的 untested（五个载体共用一条路由），不是每个载体一条"
    );
    assert_eq!(
        v["未测"],
        json!("permutation"),
        "handler 要能读出**是哪个量**没测——只进 trace 不算可见"
    );
    assert_eq!(
        v["出口"],
        json!("unsure(untested:permutation)"),
        "审计面（程序自己带进返回值的 exit_kind）要带着那一位"
    );
}

/// **硬要求二：`tie` 不许兼职。**「测了、正逆两序不一致」与「这条路上根本没有置换这回事」
/// 在 handler 里必须是两个值。K=2 上判据是二值的（1.0 或 0.5）——**测得出「不一致」，
/// 测不出「多不一致」**，这是那个数的限制，不是那道门的限制。
#[test]
fn 测了不一致才是tie而且不带那一位() {
    let out = 跑select(读两位的程序, Some(0.5));
    let v = out.value_json();
    println!("【测了不一致】{v}");
    assert_eq!(
        v["原因"],
        json!("tie"),
        "测了、不一致 = tie（12:151 只把 tie 指派给这一种）"
    );
    assert_eq!(v["未测"], json!(""), "测过了就不该亮 J-15 那一位");
    assert_eq!(
        v["出口"],
        json!("unsure(tie)"),
        "tie 的标签不该被那一位污染"
    );
    assert!(
        !out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested")),
        "测过了不该报 W-untested：{:?}",
        out.trace.warnings
    );
}

/// **机制要推广，不能只接置换一个载体。** 这一条是检验：**线未测**（`cold`）是五个载体里
/// 的第一个，它**保留自己的 `cause`**（`cold → 保守线 + 标记` 是 §5 已有的路由，动它就是
/// 在拆一条已经接好的线），J-15 那一位挂在旁边。
///
/// 只接置换那一个载体，这个机制就只是给 select 开的一个特例——而 J-15 加宽的全部理由
/// 就是「五个载体一条规则管完」。**拿刚做完的一半去描述整件事**是上一位留下的教训。
#[test]
fn 线未测是同一位而且cause不动() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("对象"));
let e = cut(judge(s, test("成立吗", "没上岗的键")));
handle(e, {
  act: fn() { {原因: "act", 未测: "", 出口: exit_kind(e)} },
  ignore: fn() { {原因: "ignore", 未测: "", 出口: exit_kind(e)} },
  unsure: fn(u) { consume(u, "drop"); {原因: unsure_cause(u), 未测: untested(u), 出口: exit_kind(e)} }
})
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    // 校准库里**没有**这个键 → `CalibStore::get` 给出 `status: 冷` 的兜底记录：线从来没
    // 在标注集上定过。这正是载体一。
    let calib = CalibStore::new();
    let inner = RefCell::new(JevClient::with_transport(
        "jev-1.13.0",
        Box::new(|_b: &Json| Ok(json!({"answers": {"q0": {"noul": 0.9}}}))),
    ));
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        不发置换端口(&inner, None),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("程序应当跑完");
    let v = out.value_json();
    println!("【线未测】{v}");
    println!("【告警】{:?}", out.trace.warnings);
    assert_eq!(
        v["原因"],
        json!("cold"),
        "路由键不动：cold 是 §5 已有的路由（保守线 + 标记）"
    );
    assert_eq!(
        v["未测"],
        json!("calib_line"),
        "线未测是 J-15 的同一位，只是载体不同"
    );
    assert_eq!(
        v["出口"],
        json!("unsure(cold|untested:calib_line)"),
        "路由键在前，那一位挂后面"
    );
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested") && w.contains("calib_line")),
        "「每条既有路由各加一句：若该量未测，取保守项并**告警**」——没有告警，「用了保守线」与「线本来就这么宽」在痕迹上分不开。实际告警：{:?}",
        out.trace.warnings
    );
}

/// **「付钱 → 测了 → `Pick` 可给」的那一半**，以及一条断言：**这个夹具里 p 在 `hi` 之上**。
///
/// 后一件是必要的。INTERFACE §四·二·七·六 给「为什么不让没测过骑 `band` 这条既有路由」
/// 写了方向——「`band` 说的是 p 落在带内，而这一格 p 可能远在 `hi` 之上，写进审计物就是
/// 一句假话」。**凡给通则标方向，必须附一个当场能红的程序**：同一个夹具、只把 `mode_share`
/// 从 `None` 换成 `1.0` 就拿到 `Pick`，这就证明了 p ≥ `hi`（0.9 ≥ 0.65）——那一格若写成
/// `band`，账本上记的就是一件没发生的事。
#[test]
fn 测了一致且过线才给pick_并钉住这个夹具的p在线上() {
    let out = 跑select(读两位的程序, Some(1.0));
    let v = out.value_json();
    println!("【测了一致】{v}");
    assert_eq!(
        v["原因"],
        json!("pick"),
        "测了、一致、p ≥ hi → Pick；这也钉住了本夹具的 p 在线之上"
    );
    assert_eq!(v["出口"], json!("pick(0)"), "argmax 是 c0（0.9）");
    assert!(
        !out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested")),
        "测过了不该报 W-untested：{:?}",
        out.trace.warnings
    );
}
