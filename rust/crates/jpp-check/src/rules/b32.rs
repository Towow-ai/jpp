//! B32 时延预算的静态面（原 `check.rs` 预算节）。跨度收集移到 `analysis/spans.rs`。

#![allow(unused_imports)]
use super::sites::site_of;
use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：B32（时延预算的静态面）。
pub(crate) const RULE: Rule = Rule {
    code: "B32",
    requires: &[],
    hooks: Hooks {
        before: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    latency(cx)
}

/// **B32 时延预算的静态面**：估计层数 × 画像 p95 超过 `budget.latency_p95` 即拒绝。
///
/// 估计按「每个判断站点（`judge` / `sieve`）至少一层」计；同状态的题会融合进同一层，
/// 所以这是**上界**，在循环或函数体里的站点另计为「不止一遍」、此时只有下界。
/// 下界已超 → 错误；只有上界超而下界不超 → 告警。没有档案 p95 → `W-untested`。
fn latency(cx: &Cx) -> Vec<Diagnostic> {
    let (p, profile) = (cx.p, cx.profile);
    let mut out = vec![];
    let b = &p.budget;
    let Some(limit) = b.latency_p95 else {
        return out;
    };
    let mut 一次 = 0usize;
    let mut 多次 = 0usize;
    walk_block(&p.body, &mut |e| {
        if matches!(call_name(e), Some("judge") | Some("sieve")) {
            if cx.sites.repeated(site_of(e)) {
                多次 += 1;
            } else {
                一次 += 1;
            }
        }
    });
    if 一次 + 多次 == 0 {
        return out;
    }
    let Some(p95) = profile.and_then(|pr| pr.latency_p95()) else {
        out.push(Diagnostic::warning(
                "W-untested",
                format!("budget.latency_p95 = {limit}s，但没有档案的 p95 时延，估计不了层数 × p95（B32）。修法：--profile 加载档案"),
                b_span(p),
            ));
        return out;
    };
    let 下界 = p95; // 至少一层
    let 上界 = (一次 + 多次) as f64 * p95;
    if 下界 > limit {
        out.push(Diagnostic::error(
                "E-latency",
                format!("时延预算 {limit}s 小于一层判断的 p95 时延 {p95}s：这个计划在第一层就会超时（B32，规划器拒绝）。修法：放宽 latency_p95，或减少判断层"),
                b_span(p),
            ));
    } else if 多次 == 0 && 上界 > limit {
        out.push(Diagnostic::error(
                "E-latency",
                format!("估计 {一次} 层 × p95 {p95}s = {上界:.2}s，超过时延预算 {limit}s（B32，规划器拒绝）。估计按每个判断站点一层计，同状态的题会融合；确知可融合时放宽预算"),
                b_span(p),
            ));
    } else if 多次 > 0 && 上界 > limit {
        out.push(Diagnostic::warning(
                "W-latency",
                format!("{多次} 个判断站点在循环或函数体里，层数静态估不出上界；已知站点 × p95 = {上界:.2}s 已超时延预算 {limit}s（B32）。运行期超出预算的站点会转 Unsure(latency)"),
                b_span(p),
            ));
    }
    out
}
