//! B52 旁路测试（步 21）：未决责任的归属按可达性核，可达性延伸进闭包捕获；捕获未决的方法只在它是
//! 某条责任的**唯一可达路径**时是 `Fn¹`——调用一次后责任按实际去向处置，再调用或丢弃为运行期 J-05；
//! 经多条路径可达（B17 的 `{pending, resume}` 写法）不触发。另测 B115（A-12）：`W-untyped-transfer`
//! 只对具名函数报，函数字面量不报。
//!
//! 依据：`20` v2 附录 A B52、§3.5；`13` §3 末 B52 注；`21` 步 21 与「步 21 追加」B115 句；
//! 预注册：`地基/过程记录/工程-步21.md` §一。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
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

fn 跑(src: &str) -> Result<Outcome, (Option<String>, String)> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    run(
        &program,
        端口(&calls),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| match e {
        jpp::Error::Runtime(r) => (r.rule.clone(), r.message.clone()),
        other => (None, other.render()),
    })
}

/// 造一个只经返回的方法可达的未决：`e` 在 `造` 的帧里切出，帧返回后只有这个方法捕获着它
const 唯一路径: &str = r#"
budget {calls: 4, cost: 0, depth: 16};
fn 造() {
    let e = cut(judge(state(mat("甲")), test("行吗", "k")));
    fn() { handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {r: "unsure", exit: u} }}) }
}
let k = 造();
TAIL
"#;

#[test]
fn 唯一路径的方法调用一次_结果带着责任交出() {
    let o = 跑(&唯一路径.replace("TAIL", "let a = k();\na")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        o.returned_unsure.len(),
        1,
        "责任随第一次调用的结果转交：{:?}",
        o.returned_unsure
    );
}

#[test]
fn 唯一路径的方法调用两次是j05() {
    let Err((rule, msg)) = 跑(&唯一路径.replace("TAIL", "let a = k();\nlet b = k();\n[a, b]"))
    else {
        panic!("Fn¹ 第二次调用应当报 J-05")
    };
    assert_eq!(rule.as_deref(), Some("J-05"));
    assert!(msg.contains("Fn¹") && msg.contains("调用过一次"), "{msg}");
}

#[test]
fn 唯一路径的方法丢弃是j05并指出那个方法() {
    let Err((rule, msg)) = 跑(&唯一路径.replace("TAIL", "1")) else {
        panic!("唯一路径被丢弃应当报 J-05")
    };
    assert_eq!(rule.as_deref(), Some("J-05"));
    assert!(msg.contains("Fn¹"), "报文要说出唯一路径是哪个方法：{msg}");
    assert!(
        msg.find("转交").unwrap() < msg.find("drop").unwrap(),
        "修法先给转交（B95）：{msg}"
    );
}

#[test]
fn 用掉的唯一路径不再算转交() {
    // 调用一次、丢掉那次的结果、再把方法本身返回：方法不能再调用，经它「可达」只是字面上的
    let Err((rule, _)) = 跑(&唯一路径.replace("TAIL", "let a = k();\nk")) else {
        panic!("用掉的 Fn¹ 不是责任的路径，应当报 J-05")
    };
    assert_eq!(rule.as_deref(), Some("J-05"));
}

#[test]
fn 多路径可达的续接方法可以调用多次() {
    // B17 的写法：同一份未决同时在 pending 字段与 resume 方法里——多路径，不是 Fn¹
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
fn 造() {
    let e = cut(judge(state(mat("甲")), test("行吗", "k")));
    {pending: [e], resume: fn() { handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {r: "unsure", exit: u} }}) }}
}
let o = 造();
let a = o.resume();
let b = o.resume();
{a: a, b: b, pending: o.pending}
"#;
    let o = 跑(src).unwrap_or_else(|e| panic!("多路径可达不触发 Fn¹：{e:?}"));
    assert_eq!(o.returned_unsure.len(), 1, "{:?}", o.returned_unsure);
}

#[test]
fn 两个方法捕获同一责任也不是唯一路径() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
fn 造() {
    let e = cut(judge(state(mat("甲")), test("行吗", "k")));
    [fn() { e }, fn() { e }]
}
let ks = 造();
let a = ks[0]();
let b = ks[0]();
[a, b, ks[1]()]
"#;
    let o = 跑(src).unwrap_or_else(|e| panic!("两条路径不触发 Fn¹：{e:?}"));
    assert_eq!(o.returned_unsure.len(), 1, "{:?}", o.returned_unsure);
}

#[test]
fn 字面量不报_具名函数报_untyped_transfer() {
    // B115（A-12）：两处都把出口如实带出、返回类型都没提 Exit；只有具名函数有调用者从签名读它
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
fn 具名(x) { cut(judge(state(mat(x)), test("行吗", "k"))) }
let a = 具名("甲");
let b = map(["乙"], fn(x) { cut(judge(state(mat(x)), test("行吗", "k"))) });
[a, b]
"#;
    let o = 跑(src).unwrap_or_else(|e| panic!("{e:?}"));
    let w: Vec<&String> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-untyped-transfer"))
        .collect();
    assert_eq!(w.len(), 1, "只有具名函数报：{w:?}");
    assert!(w[0].contains("具名"), "{w:?}");
    assert_eq!(
        o.returned_unsure.len(),
        2,
        "两份未决都随返回值交出：{:?}",
        o.returned_unsure
    );
}
