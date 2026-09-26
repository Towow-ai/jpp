//! B94 下半旁路测试（步 23c）：直线段提升按 13b 的三个条件穿过用户函数调用，同一状态的多次 `judge`
//! 提升到段首登记，段内第一次检视时一次发出；`if` 分支内不提升；只有一处的状态不提升。
//!
//! 依据：B94（`评估/2026-09-24-仪表读数4诊断与裁定.md` §三·3、§八）；`20` v2 §2.5；`21` 步 23c；
//! 预注册 `地基/过程记录/工程-步23c.md`。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::Passes;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome};
use jpp::{lower, syntax::parse};

fn 端口<'a>(每次: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        每次.borrow_mut().push(qs.len());
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 跑(src: &str, passes: Passes) -> (Outcome, Vec<usize>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let actions = ActionRegistry::new();
    let mut it = jpp::interp::Interp::new(
        端口(&每次),
        &mut ledger,
        &calib,
        &actions,
        program.budget.clone(),
    );
    it.passes = passes;
    let o = it
        .run(&program)
        .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()));
    let v = 每次.borrow().clone();
    (o, v)
}

/// 四个包装函数直线段顺序调用，每个立即 `judge → cut → handle`（探针 entity-align 非设计者版的形状）
const 四个包装: &str = r#"
budget {calls: 10, cost: 0, depth: 16};
fn 问(s, 题面) {
    handle(cut(judge(s, test(题面, "k"))), {act: fn() { "是" }, ignore: fn() { "否" }, unsure: fn(u) { {r: "未决", exit: u} }})
}
let s = state(mat("同一份材料"));
let a = 问(s, "题一");
let b = 问(s, "题二");
let c = 问(s, "题三");
let d = 问(s, "题四");
[a, b, c, d]
"#;

#[test]
fn 四个包装函数直线段_一次调用() {
    let (o, 每次) = 跑(四个包装, Passes::default());
    assert_eq!(每次, vec![4], "同一状态四道题提升到段首，一次发出");
    assert_eq!(o.value_json(), serde_json::json!(["是", "是", "是", "是"]));
    let (o2, 每次2) = 跑(
        四个包装,
        Passes {
            lift: false,
            ..Passes::default()
        },
    );
    assert_eq!(
        每次2,
        vec![1, 1, 1, 1],
        "关掉 lift 即改前行为：每个包装各一次"
    );
    assert_eq!(o2.value_json(), o.value_json(), "出口不变");
}

#[test]
fn if分支内的judge不提升() {
    let src = r#"
budget {calls: 10, cost: 0, depth: 16};
fn 问(s, 题面) {
    handle(cut(judge(s, test(题面, "k"))), {act: fn() { "是" }, ignore: fn() { "否" }, unsure: fn(u) { {r: "未决", exit: u} }})
}
let s = state(mat("同一份材料"));
let a = 问(s, "题一");
let b = if a == "是" { 问(s, "题二") } else { "没问" };
[a, b]
"#;
    let (_, 每次) = 跑(src, Passes::default());
    assert_eq!(每次, vec![1, 1], "分支里的那一问不提前登记");
}

#[test]
fn 只有一处的状态不提升() {
    let src = r#"
budget {calls: 10, cost: 0, depth: 16};
fn 问(s, 题面) {
    handle(cut(judge(s, test(题面, "k"))), {act: fn() { "是" }, ignore: fn() { "否" }, unsure: fn(u) { {r: "未决", exit: u} }})
}
let s1 = state(mat("材料甲"));
let s2 = state(mat("材料乙"));
let a = 问(s1, "题一");
let b = 问(s2, "题一");
[a, b]
"#;
    let (o, 每次) = 跑(src, Passes::default());
    assert_eq!(每次, vec![1, 1]);
    assert_eq!(o.layers.len(), 2, "不同状态各一处：不提升，层数照旧");
}
