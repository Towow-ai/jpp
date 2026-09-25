//! J-14 的静态面：`state` 的 on 槽恰一个判断对象（关系用一对）。字面列表超过两个即报。
//!
//! 步 24d 补 H8（`12` §1.2，`multi_object_crosstalk` 类假设）：按画像字段决定 error 还是
//! warn（B39 逐行方向：守卫行取保住守卫的一侧）——未测或 `true`（同状态多对象逐对象判存在
//! 串扰，B62 已有真机证据）时维持 error；画像明说 `false`（该判断器没有这种串扰）时降 warn，
//! 并说出是哪个字段让它降的，同 `j01.rs::reading_err_h5` 的写法。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-14（状态 on 恰一个判断对象）；H8（步 24d）。
pub(crate) const RULE: Rule = Rule {
    code: "J-14",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    if s.name != "state" {
        return out;
    }
    if let Some(ExprKind::List(items)) = s.args.first().map(|a| a.kind()) {
        if items.len() > 2 {
            let msg = format!(
                "state 的 on 槽放了 {} 个对象：一题一对象（关系用一对）。修法：逐个对象建状态，或把它们放进 over 槽当候选",
                items.len()
            );
            let span = s.args[0].span;
            // 依据：H8（12 §1.2，步 24d）
            match cx.profile.map(|p| p.multi_object_crosstalk()) {
                Some(jpp_effects::Tri::假) => {
                    out.push(Diagnostic::warning(
                        "J-14",
                        format!("{msg}。档案 multi_object_crosstalk: false → H8 不成立，按 12 §1.3 降 warn"),
                        span,
                    ));
                }
                // 依据：H8（12 §1.2，步 24d）——档案明说 true → H8 成立 → 照常是错，不报未测
                Some(jpp_effects::Tri::真) => out.push(Diagnostic::error("J-14", msg, span)),
                // 依据：H8、J-15（12 §1.3 末行）——档案说「未测」，或根本没有档案 →
                // 取该假设为真（保守项），另报 W-untested
                other => {
                    out.push(Diagnostic::error("J-14", msg, span));
                    let 载体 = if other.is_some() {
                        "档案里 multi_object_crosstalk 未测"
                    } else {
                        "本次没有加载档案，multi_object_crosstalk 无任何测量支持"
                    };
                    // 依据：J-15（12 §1.3 末行）
                    out.push(Diagnostic::warning(
                        "W-untested",
                        format!("{载体}：按 J-15 取 H8 成立（保守项），J-14 一对象仍是错"),
                        span,
                    ));
                }
            }
        }
    }
    out
}
