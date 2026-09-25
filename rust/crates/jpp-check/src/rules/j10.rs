//! J-10 unsure 上界的静态面（原 `check.rs` 预算节）。跨度收集移到 `analysis/spans.rs`。

#![allow(unused_imports)]
use super::sites::site_of;
use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-10（无标注集时用联合界 Σuᵢ 作上界，超 budget.unsure 即报）。
pub(crate) const RULE: Rule = Rule {
    code: "J-10",
    requires: &[],
    hooks: Hooks {
        before: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    j10(cx)
}

/// **J-10 的静态那一半**（`12`:591「unsure 预算：……无标注集时用联合界 Σuᵢ 作上界；
/// **超 `budget.unsure` 即报**」；`12`:814 的 ✓ 在**静态**那一栏）。
///
/// **它在任何模型调用发生之前跑**——这是它与运行期那一半的全部区别。
/// 运行期的 `unsure_bound` 报的时候钱已经花了。
///
/// **只报不停**（`Diagnostic::warn`）：条文写的是「即报」。`calls`/`cost`/`escalate`
/// 超了都 `Halt`，**这一格不**——别顺手「修正」成一致。
///
/// **Σuᵢ 是上界这件事有两个前提，缺一个就在诊断里说出来**：
/// 1. **键要是字面量**。`test(题面, calib)` 的 `calib` 是表达式，算出来才知道；
///    **静态看不见的按 1 计**（与 `unsure_bound` 对未知键的处置同口径，最保守）。
/// 2. **站点不在循环里**。循环里的一个站点在运行期会成为多道题，
///    **而静态数不出几道**——所以有循环内站点时，这个和**不再是上界**，诊断里明写。
fn j10(cx: &Cx) -> Vec<Diagnostic> {
    let (p, calib) = (cx.p, cx.calib);
    let mut out = vec![];
    let (b, Some(store)) = (&p.budget, calib) else {
        return out;
    };
    let Some(limit) = b.unsure else { return out };
    // **一个构造站点在运行期可能成为多道读数**（Codex 评审 PR #27）：
    // `let q = test(…)` 之后 `judge(s1, q)`、`judge(s2, q)` 是两条读数，
    // `judge([s1, s2, s3], q)` 是三条。按构造站点各计一次会低估，把一个真实的 2u 报成 u。
    // 这里只做静态数得出的那部分：let 绑定的题被几处 judge/sieve 引用、状态参数是字面列表
    // （或绑定到字面列表 / `state(…)`）时的长度。长度静态看不见的，按「不是上界」如实说出来。
    let mut 绑定: std::collections::HashMap<String, Expr> = Default::default();
    收let绑定(&p.body, &mut 绑定);
    let 状态倍数 = |e: &Expr| -> Option<usize> {
        let e = match e.kind() {
            ExprKind::Name(n) => 绑定.get(n).unwrap_or(e),
            _ => e,
        };
        match e.kind() {
            ExprKind::List(items) => Some(items.len()),
            _ if call_name(e) == Some("state") => Some(1),
            _ => None,
        }
    };
    let 是构造 = |e: &Expr| {
        matches!(
            call_name(e),
            Some("test") | Some("select") | Some("measure")
        )
    };
    // 站点（按 span 起点）→ 运行期读数的静态倍数；`None` = 有一处用法倍数看不见
    let mut 倍数: std::collections::HashMap<usize, Option<usize>> = Default::default();
    let mut 加 = |起: usize, m: Option<usize>| {
        let e = 倍数.entry(起).or_insert(Some(0));
        *e = match (*e, m) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        };
    };
    walk_block(&p.body, &mut |e| {
        if !matches!(call_name(e), Some("judge") | Some("sieve")) {
            return;
        }
        let args = call_args(e);
        let (Some(对象), Some(题)) = (args.first(), args.get(1)) else {
            return;
        };
        let m = 状态倍数(对象);
        let 题们: Vec<&Expr> = match 题.kind() {
            ExprKind::List(items) => items.iter().collect(),
            _ => vec![题],
        };
        for q in 题们 {
            if 是构造(q) {
                加(q.span.start, m);
            } else if let ExprKind::Name(n) = q.kind() {
                if let Some(v) = 绑定.get(n) {
                    if 是构造(v) {
                        加(v.span.start, m);
                    }
                }
            }
        }
    });
    let mut 和 = 0.0f64;
    let mut 站点数 = 0usize;
    let mut 未知 = 0usize;
    let mut 循环内 = 0usize;
    let mut 倍数未知 = 0usize;
    let mut 首站点: Option<jpp_ir::ir::Span> = None;
    walk_block(&p.body, &mut |e| {
        let 键位 = match call_name(e) {
            Some("test") | Some("select") => 1,
            Some("measure") => 2,
            _ => return,
        };
        let ExprKind::Call { arguments, .. } = e.kind() else {
            return;
        };
        站点数 += 1;
        if 首站点.is_none() {
            首站点 = Some(e.span);
        }
        if cx.sites.repeated(site_of(e)) {
            循环内 += 1;
        }
        let u = match arguments.get(键位).map(|a| a.kind()) {
            Some(ExprKind::Text(k)) => match store.unsure_rate(k) {
                // **与 `unsure_bound` 同口径**（`strength.rs`）：同一个 `CalibStore::usable_unsure_rate`
                Some(u) => u,
                None => {
                    未知 += 1;
                    1.0
                }
            },
            // 键不是字面量：静态看不见，按 1 计
            _ => {
                未知 += 1;
                1.0
            }
        };
        // 没有被 judge/sieve 直接用到的站点（例如传进函数）按 1 计，与之前同口径
        let m = match 倍数.get(&e.span.start) {
            None | Some(Some(0)) => 1,
            Some(Some(m)) => *m,
            Some(None) => {
                倍数未知 += 1;
                1
            }
        };
        和 += u * m as f64;
    });
    if 站点数 == 0 {
        return out;
    }
    let span = 首站点.unwrap_or(p.span);
    if 和 <= limit {
        // 步 24h（B101 (b) 缺口，总账 K-185 备注 (b) 点出）：没超预算不代表这个和真是上界——
        // 循环/函数体内站点、或倍数看不见的站点，都会让「Σ ≤ limit」这句话本身不可信。
        if 循环内 > 0 || 倍数未知 > 0 {
            // 依据：B101（12 §5 J-10 行；(b) 循环/函数体内站点即使未超预算也要提示「不是上界」）
            out.push(Diagnostic::warning(
                "W-unsure-not-bound",
                format!(
                    "unsure 预算联合界 Σuᵢ = {和:.4} ≤ budget.unsure = {limit:.4}，但 {循环内} 个站点在循环或函数体里、{倍数未知} 个站点问向静态数不出长度的对象列表：这个和不是真正的上界，实际负担可能更高，只是眼下算出来没有超。修法：把 loop/iterate 的实际轮数或对象列表长度算进预算余量，不要只看这条诊断的沉默当作安全"
                ),
                span,
            ));
        }
        return out;
    }
    let mut 循环话 = if 循环内 > 0 {
        format!(
            "；**其中 {循环内} 个站点在循环或函数体里，所以这个和不是上界**（那样一个站点在运行期是多道题，静态数不出几道；函数体算进来是因为静态判不了它被调用几次），真实的 unsure 负担只会更高"
        )
    } else {
        String::new()
    };
    if 倍数未知 > 0 {
        循环话.push_str(&format!("；**另有 {倍数未知} 个站点被 judge/sieve 问向静态数不出长度的对象列表，这个和也不是上界**"));
    }
    out.push(Diagnostic::warning(
            "J-10",
            format!(
                "unsure 预算可能不够：{站点数} 个判断站点的联合界 Σuᵢ = {和:.4} > budget.unsure = {limit:.4}（其中 {未知} 个站点没有可用的 unsure_rate，按 1 计最保守）{循环话}。\
                 **这条在任何模型调用之前就报**——运行期那一半报的时候钱已经花了。\
                 修法【作者可改】：调高 budget.unsure，或减少判断站点；\
                 【需接线人】给那些键写上岗记录（`commission` 会在认证时算出 unsure_rate）",
            ),
            span,
        ));
    out
}
