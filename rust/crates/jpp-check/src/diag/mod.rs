//! 诊断层（B13）：只读题面字面量的题式诊断。全部是告警（`W-diag-*`），不阻止运行。
//!
//! 分工：`b13.rs` 是规则本体 [`diagnose_question`]，输入一道题的字面信息，不依赖运行时；
//! 本文件把程序里的题字面量收集出来喂给它（`Checker::diagnose`）。需要判断器的「前提在材料里
//! 被做出了吗」留到 `21` 步 26；「问的东西在面前材料里吗」的**形状面**（B51-R2 静态消费者，
//! `shape_check`）步 24g 已接入 `kind.rs::scan` 的判断站点遍历，不需要判断器。
//!
//! `kind.rs` 是题类推断的检查器一侧（B76，步 12e-1）：题字面量的基础类、判断站点上的精化类、
//! `E-kind-conflict`；题字面量的基础类也写进 [`QuestionLit::kind`]，B13 的提及类规则读它；
//! 步 24g 起同一趟遍历还调 `shape_check` 产出 `W-diag-shape`。
//!
//! 依据：`12` §3 J-17 后「诊断层规则集第一批（B13）」；B 栏 B13 及其两次补充；B76；B51-R2。

pub mod b13;
pub mod kind;

pub use b13::{DiagCx, QuestionLit, diagnose_fill, diagnose_question, shape_check};
pub use kind::{KindSite, question_kinds};

use crate::*;

/// 收集程序里所有字面量题面（`test` / `select` / `measure` 的题面、`form` 的模板、
/// `fill` 的填法），逐一诊断。题面不是字面量的（运行期才算出来）不诊断。
pub(crate) fn diagnose(p: &Program) -> Vec<Diagnostic> {
    let cx = DiagCx::default();
    let mut 绑定: HashMap<String, Expr> = HashMap::new();
    收let绑定(&p.body, &mut 绑定);
    let mut found = vec![];
    walk_block(&p.body, &mut |e| {
        let args = call_args(e);
        match call_name(e) {
            Some(op @ ("test" | "select" | "measure")) => {
                if let Some(ExprKind::Text(t)) = args.first().map(|a| a.kind()) {
                    let (request, decl) = kind::decl_of(e, &绑定);
                    found.push(diagnose_question(
                        &QuestionLit::with_decl(op, t, false, e.span, request, &decl),
                        &cx,
                    ));
                }
            }
            Some("form") => {
                if let (Some(ExprKind::Text(op)), Some(ExprKind::Text(t))) = (
                    args.first().map(|a| a.kind()),
                    args.get(1).map(|a| a.kind()),
                ) {
                    let (request, decl) = kind::decl_of(e, &绑定);
                    found.push(diagnose_question(
                        &QuestionLit::with_decl(op, t, true, e.span, request, &decl),
                        &cx,
                    ));
                }
            }
            Some("fill") => {
                let (Some(f), Some(r)) = (args.first(), args.get(1)) else {
                    return;
                };
                let f = match f.kind() {
                    ExprKind::Name(n) => 绑定.get(n).unwrap_or(f),
                    _ => f,
                };
                if call_name(f) != Some("form") {
                    return;
                }
                let fa = call_args(f);
                let (Some(ExprKind::Text(op)), Some(ExprKind::Text(t))) =
                    (fa.first().map(|a| a.kind()), fa.get(1).map(|a| a.kind()))
                else {
                    return;
                };
                let ExprKind::Record(fields) = r.kind() else {
                    return;
                };
                // 只收字面量填法；不是字面量的槽记作 None（仍参与「缺槽 / 多槽」核对）
                let fills: Vec<(String, Option<String>)> = fields
                    .iter()
                    .map(|(k, v)| {
                        let lit = match v.kind() {
                            ExprKind::Text(s) => Some(s.clone()),
                            ExprKind::Integer(i) => Some(i.to_string()),
                            _ => None,
                        };
                        (k.clone(), lit)
                    })
                    .collect();
                found.push(diagnose_fill(op, t, &fills, e.span, &cx));
            }
            _ => {}
        }
    });
    found.into_iter().flatten().collect()
}
