//! B95 旁路测试（步 21 追加，运行时半）：丢弃未决的入口收窄，转交成为第一个修法。
//! 四条（`评估/2026-09-24-仪表读数4诊断与裁定.md` §八 B95「怎么改」）：契约值 drop → 错；
//! `Unsure(budget)`（及 `absent`）drop → `E-drop-unobserved`；drop 后又返回 → `W-drop-then-return`；
//! `handle` 臂内单个出口 drop → 通过并留 `W-drop-vs-escalate`。
//!
//! 「drop 后又返回编号」按预注册收窄为运行期看得见的形状：出口本身、它所在的元素记录、或带它来源键的
//! 投影（`item`）；只凭 `index` 算出的编号不带来源，留给步 24 的静态面（`过程记录/工程-步21.md` §一）。
//!
//! 依据：B95；`12` §3 J-05 注；`21` 「步 21 追加（B95 运行时半）」。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, run};
use jpp::{lower, syntax::parse};

/// 是非题恒 0.5：落在线带 [0.25, 0.75] 内 → `Unsure(band)`
fn 端口<'a>(calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        *calls.borrow_mut() += 1;
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

/// 判断器始终报错：缺席策略 conservative → `Unsure(absent)`
fn 缺席端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |_s, _qs| {
        Err(EffectError("连不上".into()))
    }))
}

/// 读法函数与 `lib/outcome.jpp` 同源（同 `bypass_b81_b82.rs`：库文本接在 budget 行后面）
fn 程序(src: &str) -> String {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/outcome.jpp"),
    )
    .expect("读得到 lib/outcome.jpp");
    let (budget, rest) = src.trim_start().split_once('\n').expect("第一行是 budget");
    format!("{budget}\n{lib}\n{rest}")
}

fn 跑_用(src: &str, ports: Ports<'_>) -> Result<Outcome, (Option<String>, String)> {
    let program = lower(&parse(&程序(src)).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let mut l = Ledger::new();
    run(&program, ports, &calib, &ActionRegistry::new(), &mut l).map_err(|e| match e {
        jpp::Error::Runtime(r) => (r.rule.clone(), r.message.clone()),
        other => (None, other.render()),
    })
}

fn 跑(src: &str) -> Result<Outcome, (Option<String>, String)> {
    let calls = RefCell::new(0);
    跑_用(src, 端口(&calls))
}

fn 告警<'o>(o: &'o Outcome, code: &str) -> Vec<&'o String> {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(code))
        .collect()
}

#[test]
fn 契约值不能整份drop() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let o = sieve(["甲", "乙"], test("行吗", "k"));
consume(o, "drop");
len(accepted(o))
"#;
    let Err((rule, msg)) = 跑(src) else {
        panic!("consume(契约值, \"drop\") 应当报错")
    };
    assert_eq!(rule.as_deref(), Some("J-05"));
    assert!(msg.contains("不收契约值"), "{msg}");
    assert!(
        msg.contains("undecided(o)") && msg.contains("unobserved(o)"),
        "报文给转交写法：{msg}"
    );
}

#[test]
fn 预算未观察项不能drop() {
    // 三份材料、预算一次调用：第一份之后的材料记 Unsure(budget)（sieve 现行的预算停机路径）
    let src = r#"
budget {calls: 1, cost: 0, depth: 16};
let o = sieve(["甲", "乙", "丙"], test("行吗", "k"));
map(unobserved(o), fn(p) { consume(p.exit, "drop") });
{n: len(accepted(o)), pending: undecided(o)}
"#;
    let Err((rule, msg)) = 跑(src) else {
        panic!("drop Unsure(budget) 应当报 E-drop-unobserved")
    };
    assert_eq!(rule.as_deref(), Some("E-drop-unobserved"), "{msg}");
    assert!(msg.contains("budget"), "{msg}");
}

#[test]
fn 缺席项不能drop() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "conservative"}};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let c = unsure_cause(e);
consume(e, "drop");
c
"#;
    let Err((rule, msg)) = 跑_用(src, 缺席端口()) else {
        panic!("drop Unsure(absent) 应当报 E-drop-unobserved")
    };
    assert_eq!(rule.as_deref(), Some("E-drop-unobserved"), "{msg}");
    assert!(msg.contains("absent"), "{msg}");
}

#[test]
fn drop后又返回元素要告警() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let o = sieve(["甲", "乙"], test("行吗", "k"));
let d = map(undecided(o), fn(p) { consume(p.exit, "drop") });
{names: map(undecided(o), fn(p) { p.item }), n: len(accepted(o))}
"#;
    let o = 跑(src).unwrap_or_else(|e| panic!("drop 是合法去向，只告警：{e:?}"));
    let w = 告警(&o, "W-drop-then-return");
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("2 个"), "{w:?}");
    // 整条元素记录（带 exit）返回同样告警
    let src2 = src.replace("map(undecided(o), fn(p) { p.item })", "undecided(o)");
    let o = 跑(&src2).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        告警(&o, "W-drop-then-return").len(),
        1,
        "{:?}",
        o.trace.warnings
    );
}

#[test]
fn handle臂内单个出口drop照旧通过() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
handle(cut(judge(state(mat("甲")), test("行吗", "k"))),
       {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "未决" }})
"#;
    let o = 跑(src).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(o.value_json(), serde_json::json!("未决"));
    assert_eq!(
        告警(&o, "W-drop-vs-escalate").len(),
        1,
        "{:?}",
        o.trace.warnings
    );
    assert!(
        告警(&o, "W-drop-then-return").is_empty(),
        "{:?}",
        o.trace.warnings
    );
}
