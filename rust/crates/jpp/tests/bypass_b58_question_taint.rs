//! 绕过测试 B58 题面 taint（步 17b；`20` 写的路径是 `tests/bypass/b33_question.rs`，按仓库惯例放在这里）。
//!
//! 题带 taint = 填入值、计算出的题面文本与题式模板的 taint 之 ∨，字面题为 Trusted；读数与出口 taint =
//! 状态 ∨ 题。判断器读到的题面与读到的材料一样可能被注入：不可信文本经 `fill` 进题面、对可信材料发问，
//! 切出的出口不能作为可信合取项放行不可逆 `do`。题的 taint 不进题哈希与账本键。
//! 依据：B58（`12` §2.11 第 4 条；`20` v2 §3.10「文本 → 题面」「`judge` → 读数」两行）。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

fn 端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.95)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
            confidence: vec![],
        })
    }))
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    // 取外部：可逆，产物不可信（模拟 gen 或入口带来的文本）
    a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::Text("北京".into(), Taint::Trusted.into()))
    });
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

fn 跑在(src: &str, l: &mut Ledger) -> Result<(Json, u64), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    run(&program, 端口(), &calib, &动作表(), l)
        .map(|o| (o.value_json(), o.cost.calls))
        .map_err(|e| e.render())
}

fn 跑(src: &str) -> Result<Json, String> {
    跑在(src, &mut Ledger::new()).map(|(v, _)| v)
}

/// 可信材料、正式线；题由 `题` 表达式给出（前面可用 `外来`：不可信文本 "北京"）。
/// 返回 {t: taint(e), m: as_mat 之后材料的 taint, w: act 臂里不可逆 do 的结果}
fn 程序(题: &str) -> String {
    format!(
        "budget {{calls: 4, cost: 1, depth: 8}};\nlet 外来 = content(do(\"取外部\", [], 0));\nlet 提到 = form(\"test\", \"材料里提到{{city}}吗\", {{calib: \"k\"}});\nlet q = {题};\nlet e = cut(judge(state(mat(\"我下周去北京出差\")), q));\nhandle(e, {{act: fn() {{ content(do(\"发邮件\", [], 0)) }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})\n"
    )
}

fn 出口taint(题: &str) -> Json {
    let src = format!(
        "budget {{calls: 4, cost: 1, depth: 8}};\nlet 外来 = content(do(\"取外部\", [], 0));\nlet 提到 = form(\"test\", \"材料里提到{{city}}吗\", {{calib: \"k\"}});\nlet q = {题};\nlet e = cut(judge(state(mat(\"我下周去北京出差\")), q));\nlet m = mat(e);\n{{t: taint(e), m: m.taint}}\n"
    );
    跑(&src).unwrap()
}

#[test]
fn a_不可信填入值让出口不可信且不放行() {
    let v = 出口taint("fill(提到, {city: 外来})");
    assert_eq!(v["t"], Json::from("untrusted"), "{v}");
    let e =
        跑(&程序("fill(提到, {city: 外来})")).expect_err("题面含不可信填入值，不能放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn b_字面填入值出口可信且放行() {
    let v = 出口taint("fill(提到, {city: \"北京\"})");
    assert_eq!(v["t"], Json::from("trusted"), "{v}");
    assert_eq!(
        跑(&程序("fill(提到, {city: \"北京\"})")).unwrap(),
        Json::from("已发")
    );
}

#[test]
fn c_计算出的不可信题面文本() {
    let v = 出口taint("test(\"材料里提到\" + 外来 + \"吗\", \"k\")");
    assert_eq!(v["t"], Json::from("untrusted"), "{v}");
    let e = 跑(&程序("test(\"材料里提到\" + 外来 + \"吗\", \"k\")"))
        .expect_err("计算出的题面含不可信文本");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn d_不可信模板的题式() {
    // 17b 解释登记 (a)：题式模板本身来自不可信文本
    let v = 出口taint(
        "fill(form(\"test\", 外来 + \"在{city}吗\", {calib: \"k\"}), {city: \"材料里\"})",
    );
    assert_eq!(v["t"], Json::from("untrusted"), "{v}");
}

#[test]
fn e_题面taint不进账本键_重放0调用() {
    // 同一题面、同一状态、同一站点：可信题与不可信题是同一道题、同一个账本键（键含站点，所以经同一个函数问）
    let src = "budget {calls: 4, cost: 1, depth: 8};\nlet 外来 = content(do(\"取外部\", [], 0));\nlet 提到 = form(\"test\", \"材料里提到{city}吗\", {calib: \"k\"});\nlet a = fill(提到, {city: \"北京\"});\nlet b = fill(提到, {city: 外来});\nlet s = state(mat(\"我下周去北京出差\"));\nfn 判(q) !{judge} { cut(judge(s, q)) }\nlet ea = 判(a);\nlet eb = 判(b);\n{ta: taint(ea), tb: taint(eb), same: a.hash == b.hash}\n";
    let mut l = Ledger::new();
    let (v, calls) = 跑在(src, &mut l).unwrap();
    assert_eq!(v["ta"], Json::from("trusted"), "{v}");
    assert_eq!(v["tb"], Json::from("untrusted"), "{v}");
    let 判断条目 = l
        .entries
        .iter()
        .filter(|e| matches!(e, jpp::ledger::Entry::Judge { .. }))
        .count();
    assert_eq!(
        判断条目, 1,
        "同一道题只问一次（calls = {calls}，含 do）：{v}"
    );
    let (v2, calls2) = 跑在(src, &mut l).unwrap();
    assert_eq!(calls2, 0, "重放 0 调用");
    assert_eq!(v, v2);
}

#[test]
fn f_不可信题判出的出口作材料也不可信() {
    let v = 出口taint("fill(提到, {city: 外来})");
    assert_eq!(v["m"], Json::from("untrusted"), "{v}");
    let v = 出口taint("fill(提到, {city: \"北京\"})");
    assert_eq!(v["m"], Json::from("trusted"), "{v}");
}
