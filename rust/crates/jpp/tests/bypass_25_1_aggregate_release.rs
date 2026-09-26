//! J-08 · 聚合出口不作放行证据（步 25-1 热修，放行方向）。
//!
//! 记录的缺陷：`tally` 的 `exists`/`all` 与 `first_k` 的出口由构造签发，等级为 `None`；
//! `Exit::releases()` 对 `None` 等级取真，taint 取元素出口里最差的一个，聚合出口没有账本键、谱系检查
//! 为空。于是元素出口用的是夹具线（或试用线、类线、范围外等不放行等级）时，聚合出口在 `handle`
//! 分派处仍给出放行证据，不可逆 `do` 被执行。库轨步 25 开工时在 main `f48a855f` 上复现。
//!
//! 何时被什么堵上：步 25-1 把 `tally`、`first_k` 签发的聚合出口的等级置为 `Cold`（聚合出口本身没有
//! 用上任何一条线），`releases()` 为假，`GuardEv::from_exit` 不给判断证据。这是最保守的读法：
//! (e) 记下它的代价——元素全部来自正式线时聚合出口也不放行；更宽的「按分量派生放行」待 Fable 裁定。
//! (f)(g) 是对照。依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md）、`12` §3 J-08；
//! 预注册 `地基/过程记录/工程-步25-1.md`。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

fn 端口<'a>(p: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
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
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

/// 线的两种等级：夹具线（宿主 `put`，不放行）与正式线（放行等级，带全范围指纹）
enum 线 {
    夹具,
    正式,
}

fn 跑(src: &str, 线: 线) -> Result<Json, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    match 线 {
        线::夹具 => {
            calib.put("k", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
        }
        线::正式 => common::certified(&mut calib, "k", 0.8, 0.2, 50),
    }
    let mut l = Ledger::new();
    // p = 0.95：每个元素都切出 act，没有未决
    run(&program, 端口(0.95), &calib, &动作表(), &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

/// 材料是程序字面量（可信）；`聚合` 是 `tally(r)` 或 `first_k(r, 1)`，绑定为 `a`，`守卫` 取它的一个出口。
/// 未决清单随返回值交出，程序本身过静态检查（J-05）。
fn 程序(聚合: &str, 守卫: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
let r = sieve(["一段程序自己写的材料", "另一段程序自己写的材料"], test("该发吗", "k"));
let a = {聚合};
{{sent: handle({守卫}, {{
    act: fn() {{ content(do("发邮件", [], 0)) }},
    ignore: fn() {{ "不发" }},
    unsure: fn(u) {{ u }}
}}), pending: a.pending}}
"#
    )
}

#[test]
fn a_夹具线上的tally_exists不放行不可逆do() {
    let e = 跑(&程序("tally(r)", "a.value.exists"), 线::夹具)
        .expect_err("聚合出口不是放行判定，act 臂里的不可逆 do 应被 J-08 拒");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn b_夹具线上的tally_all不放行不可逆do() {
    let e = 跑(&程序("tally(r)", "a.value.all"), 线::夹具).expect_err("同 (a)");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn c_夹具线上的first_k出口不放行不可逆do() {
    let e = 跑(&程序("first_k(r, 1)", "a.value.exit"), 线::夹具).expect_err("同 (a)");
    assert!(e.contains("J-08"), "{e}");
}

/// 值级证据的第二条路：臂返回 `true`，再用它守卫（B121：证据随值走）。
#[test]
fn d_聚合出口臂返回的true不构成放行证据() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let r = sieve(["一段程序自己写的材料", "另一段程序自己写的材料"], test("该发吗", "k"));
let t = tally(r);
let ok = handle(t.value.exists, {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
{sent: if ok { content(do("发邮件", [], 0)) } else { "不发" }, pending: t.pending}
"#;
    let e = 跑(src, 线::夹具).expect_err("聚合出口不是放行判定");
    assert!(e.contains("J-08"), "{e}");
}

/// 本步的保守代价：元素全部来自正式线、可信材料，聚合出口也不放行（待 Fable 裁定派生规则）。
#[test]
fn e_正式线上的聚合出口也不放行_保守代价() {
    let e = 跑(&程序("tally(r)", "a.value.exists"), 线::正式)
        .expect_err("25-1 最保守读法：聚合出口一律不作放行证据");
    assert!(e.contains("J-08"), "{e}");
}

/// 对照：(e) 的同一条正式线、同一份材料，直接切出的出口放行。证明 (e) 的夹具确是放行等级。
#[test]
fn f_对照_正式线上直接切出的出口放行() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("一段程序自己写的材料")), test("该发吗", "k"))), {
    act: fn() { content(do("发邮件", [], 0)) },
    ignore: fn() { "不发" },
    unsure: fn(u) { u }
})
"#;
    let v = 跑(src, 线::正式).expect("正式线、可信材料上的 act 出口放行不可逆 do");
    assert_eq!(v, Json::from("已发"));
    // 同一程序换成夹具线，直接切出的出口被拒：本文件的线等级确实起作用
    let e = 跑(src, 线::夹具).expect_err("夹具线上直接切出的出口不放行");
    assert!(e.contains("J-08"), "{e}");
}

/// 对照：聚合出口照常路由，种类、计数不变；不含 `do` 的程序正常返回。
#[test]
fn g_对照_聚合出口照常路由() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let r = sieve(["一段程序自己写的材料", "另一段程序自己写的材料"], test("该发吗", "k"));
let t = tally(r);
let f = first_k(r, 1);
{exists: exit_kind(t.value.exists), all: exit_kind(t.value.all), count: t.value.count,
 first: exit_kind(f.value.exit),
 routed: handle(t.value.exists, {act: fn() { "有" }, ignore: fn() { "无" }, unsure: fn(u) { u }}),
 pending: [t.pending, f.pending]}
"#;
    let v = 跑(src, 线::夹具).expect("不含 do 的程序照常跑完");
    assert_eq!(v["exists"], Json::from("act"));
    assert_eq!(v["all"], Json::from("act"));
    assert_eq!(v["count"], serde_json::json!([2, 2]));
    assert_eq!(v["first"], Json::from("act"));
    assert_eq!(v["routed"], Json::from("有"));
}
