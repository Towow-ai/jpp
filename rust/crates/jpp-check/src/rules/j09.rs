//! J-09 静态面，两条（步 24d）：
//! - **H4**（`12` §1.2）：产出读数的效应（`judge`，按 `produces_reading` 字段泛化匹配，不写死
//!   名字）调用处，画像给了但 `one_hop` 未测时点名（`insufficient` 先查这条降级本身在运行期已
//!   生效，`bridge.rs::cut` 判序第一步）。
//! - **K-207**（2026-09-24 设计收口盘点）：`test`/`select` 的第三参 `{evidence: […], …}`，声明的
//!   槽名必须是 `on`/`ctx`/`ref`/`over` 之一——运行期 `missing_evidence`（`jpp-runtime/src/lib.rs`）
//!   对不认识的名字静默当「不缺」处理，`insufficient` 的保护因此悄悄失效而不自知，定为 error。
//!   `measure` 现行实现是严格三参（`题面, [档位…], calib`），没有第四个选项参数、不支持
//!   `evidence` 声明，因此本条不检查 `measure`。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 决定性证据槽名的合法集合（`12` §2.1，`state` 的四个槽）。
const EVIDENCE_SLOTS: &[&str] = &["on", "ctx", "ref", "over"];

/// 依据：12 §5 J-09；H4、K-207（步 24d）。
pub(crate) const RULE: Rule = Rule {
    code: "J-09",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    h4_one_hop(cx, s, &mut out);
    if matches!(s.name, "test" | "select") {
        k207_evidence_slots(s, &mut out);
    }
    out
}

/// H4：`judge`（或任何未来注册的 `produces_reading` 效应）调用处，画像给了但 `one_hop` 未测。
fn h4_one_hop(cx: &Cx, s: &CallSite, out: &mut Vec<Diagnostic>) {
    let Some(sp) = jpp_effects::by_name(s.name) else {
        return;
    };
    if !sp.produces_reading {
        return;
    }
    let Some(p) = cx.profile else { return };
    // 依据：H4（12 §1.2，步 24d）
    if p.one_hop() == jpp_effects::Tri::未测 {
        out.push(Diagnostic::warning(
            "W-untested",
            "画像没有测过 one_hop（H4）：未测按 true 处理，insufficient 先查（J-09）保留",
            s.span,
        ));
    }
}

/// K-207：`{evidence: […]}` 里的槽名必须是 `on`/`ctx`/`ref`/`over` 之一，否则那一条声明是
/// 静默失效的保护——运行期从不警告，只会一直放行（`missing_evidence` 对不认识的名字当作「不缺」）。
fn k207_evidence_slots(s: &CallSite, out: &mut Vec<Diagnostic>) {
    let Some(opts) = s.args.get(2) else { return };
    let ExprKind::Record(fields) = opts.kind() else {
        return;
    };
    let Some((_, ev)) = fields.iter().find(|(k, _)| k == "evidence") else {
        return;
    };
    let ExprKind::List(items) = ev.kind() else {
        return;
    };
    for item in items {
        let ExprKind::Text(name) = item.kind() else {
            continue;
        };
        if !EVIDENCE_SLOTS.contains(&name.as_str()) {
            // 依据：K-207（2026-09-24 设计收口盘点，步 24d）
            out.push(Diagnostic::error(
                "J-09",
                format!(
                    "evidence 声明的槽名 \"{name}\" 不是合法的证据槽：只能是 on/ctx/ref/over 之一，不认识的名字会被运行期静默当「不缺」处理，insufficient 的保护因此失效。修法：改成 on/ctx/ref/over 里的一个，或者这条证据要求写错了"
                ),
                item.span,
            ));
        }
    }
}
