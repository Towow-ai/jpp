//! J-08 · 未决出口不是放行判定（B121-2，步 16-0）。
//!
//! 记录的缺陷：`Exit::guard_trusted()` 原为「taint 可信 ∧ `releases()`」，不看出口种类；正式线上的
//! `Unsure(band)` 也 `releases()`。于是在可信材料、正式线上切出未决时，(a) 写在 `unsure` 臂里的
//! 不可逆 `do` 被放行（`duty.rs` 进 unsure 臂压的守卫为真）；(d) unsure 臂返回的 `true` 再守卫
//! 不可逆 `do` 也被放行（`let` 旁路表把未决出口折成证据）。步 16 施工中发现，复现提交 `ccd9fe03`。
//!
//! 何时被什么堵上：步 16-0 在 `Exit::guard_trusted()` 上加「出口已决」（`!is_unsure()`），
//! 守卫栈与旁路表两条路随之关住；步 16 由 `GuardEv::from_exit`（Unsure 恒空）结构性承接。
//! (b) 是对照：同一条线、同一份材料切出 `Act` 时 `act` 臂里的同一动作放行，证明夹具确实是
//! 「可信材料 + 放行等级的线」。依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md §二）。

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

fn 跑(src: &str, p: f64) -> Result<Json, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    // 正式线（放行等级），带全范围指纹
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    let mut l = Ledger::new();
    run(&program, 端口(p), &calib, &动作表(), &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

/// 材料是程序字面量（可信）；三个臂都写同一个不可逆动作。
const 程序: &str = r#"
budget {calls: 2, cost: 1, depth: 8};
let m = mat("一段程序自己写的材料");
handle(cut(judge(state(m), test("该发吗", "k"))), {
    act: fn() { content(do("发邮件", [], 0)) },
    ignore: fn() { "不发" },
    unsure: fn(u) { consume(u, "drop"); content(do("发邮件", [], 0)) }
})
"#;

#[test]
fn a_正式线可信材料上的未决出口不放行unsure臂里的不可逆do() {
    // p = 0.5 落在 (0.2, 0.8) 之间 → Unsure(band)
    let e = 跑(程序, 0.5).expect_err("未决出口不是放行判定，unsure 臂里的不可逆 do 应被 J-08 拒");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn b_对照_同一条线同一份材料切出act时act臂放行() {
    let v = 跑(程序, 0.95).expect("正式线、可信材料上的 act 出口放行不可逆 do");
    assert_eq!(v, Json::from("已发"));
}

/// 同一缺陷的第二条路：unsure 臂返回字面量 `true`，再用它守卫。步 16-0 前 let 旁路表把这次求值里
/// 产生的出口（含未决出口）折成证据，同样放行；16-0 后与 (a) 一起被拒。
const 程序_字面量: &str = r#"
budget {calls: 2, cost: 1, depth: 8};
let m = mat("一段程序自己写的材料");
let ok = handle(cut(judge(state(m), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { true },
    unsure: fn(u) { consume(u, "drop"); true }
});
if ok { content(do("发邮件", [], 0)) } else { "不发" }
"#;

#[test]
fn d_unsure臂返回的true不构成放行证据() {
    let e = 跑(程序_字面量, 0.5).expect_err("未决出口不是放行判定");
    assert!(e.contains("J-08"), "{e}");
}
