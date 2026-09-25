//! 题类推断的检查器一侧（B76，步 12e-1）：对题字面量算基础类，在判断站点上状态构造可见时算精化类，
//! 题式槽声明与可见结构矛盾时报 `E-kind-conflict`。推断函数本身在 `jpp_ir::question_kind`，
//! 运行时（步 12e-2）调同一个函数。
//!
//! 静态口径：只在结构确定时精化、只在结构确定时报矛盾。看不见绑定的名字、`over` 里的
//! `mat("字面")` 这类不确定的形状一律记作未知（宁可漏报，`lib.rs` 文件头）。基础类与精化类不同
//! 不算矛盾（B76 (4)：以精化类为准，两者都留）。
//!
//! 声明从哪里读：`over_kind` 只从 `form(题型, 模板, {…})` 的第三个参数记录读，经 `fill` 沿用
//! （与步 12e-2 的 `Form.over_kind` 同位）；`request` 从 `test`/`select` 的第三个参数或题式记录读。
//! `accepts`（B23）在步 8 的 `Form.slots.accepts` 落地前不读，提及类因此只在函数层出现。

use std::collections::{HashMap, HashSet};

use crate::*;
use jpp_ir::key::Op;
use jpp_ir::question_kind::{
    KindSource, OnShape, OverKind, OverShape, QuestionKind, Request, SlotDecls, SlotShape,
    kind_conflict, question_kind,
};

use super::b13::op_of;

/// 一道题在程序里的题类：基础类，以及判断站点上可见的精化类。
#[derive(Clone, Debug, PartialEq)]
pub struct KindSite {
    /// 题（`test`/`select`/`measure`/`fill`/`form` 调用）或判断站点的源位置
    pub span: Span,
    pub op: Op,
    pub base: (QuestionKind, KindSource),
    /// 判断站点上状态构造可见时的精化类；题字面量本身为 `None`
    pub refined: Option<(QuestionKind, KindSource)>,
}

/// 从表达式认出的一道题：题型、请求、题式槽声明。
#[derive(Clone, Copy, Debug)]
struct QInfo {
    op: Op,
    request: Option<Request>,
    decl: SlotDecls,
}

type Bindings = HashMap<String, Expr>;

/// 程序里全部可认出的题与判断站点的题类（供测试与后续诊断规则读）。
pub fn question_kinds(p: &Program) -> Vec<KindSite> {
    scan(p, None, None).0
}

/// `E-kind-conflict`（题式槽声明与可见结构矛盾）与 `W-diag-shape`（B51-R2 静态消费者，步 24g；
/// `actions`/`profile` 缺时后者不判，同 J-08/J-11 静态子面「没有表不报」口径）。
pub(crate) fn conflicts(
    p: &Program,
    actions: Option<&crate::ActionTable>,
    profile: Option<&jpp_effects::Profile>,
) -> Vec<Diagnostic> {
    scan(p, actions, profile).1
}

fn scan(
    p: &Program,
    actions: Option<&crate::ActionTable>,
    profile: Option<&jpp_effects::Profile>,
) -> (Vec<KindSite>, Vec<Diagnostic>) {
    let mut 绑定: Bindings = HashMap::new();
    收let绑定(&p.body, &mut 绑定);
    let mut sites = vec![];
    let mut diags = vec![];
    walk_block(&p.body, &mut |e| {
        let args = call_args(e);
        match call_name(e) {
            Some("test" | "select" | "measure" | "form") => {
                let Some(q) = question_of(e, &绑定, &mut HashSet::new()) else {
                    return;
                };
                let base = question_kind(q.op, q.request, &SlotShape::UNKNOWN, &q.decl);
                if let Some(c) = kind_conflict(q.op, &SlotShape::UNKNOWN, &q.decl) {
                    diags.push(conflict_diag(&c.reason, e.span));
                }
                sites.push(KindSite {
                    span: e.span,
                    op: q.op,
                    base,
                    refined: None,
                });
            }
            Some("judge") => {
                let (Some(s), Some(qs)) = (args.first(), args.get(1)) else {
                    return;
                };
                let shape = state_shape(s, &绑定);
                // 步 24g：状态材料能静态确定来自哪个声明过 `mat_shape` 的动作时才给形状面
                let action = do_source(s, &绑定);
                judge_site(
                    e.span, shape, qs, &绑定, action, actions, profile, &mut sites, &mut diags,
                );
            }
            Some("sieve") => {
                let (Some(m), Some(qs)) = (args.first(), args.get(1)) else {
                    return;
                };
                // `sieve(pair(…), q)`：每个元素是一对，判断时 `on` 是一对（关系题的载体）
                let on = if is_call_to(m, "pair", &绑定) {
                    OnShape::Pair
                } else {
                    OnShape::Unknown
                };
                let shape = SlotShape {
                    on,
                    ..SlotShape::UNKNOWN
                };
                // `sieve` 筛的是一批材料，不是单一 `state(...)`：`do_source` 认不出 `state` 形状，
                // 天然给 `None`——本批不为 `sieve` 单独扩来源追溯（预注册范围收窄）
                let action = do_source(m, &绑定);
                judge_site(
                    e.span, shape, qs, &绑定, action, actions, profile, &mut sites, &mut diags,
                );
            }
            _ => {}
        }
    });
    (sites, diags)
}

/// 判断站点：题可以是一道题或题的列表字面量。
#[allow(clippy::too_many_arguments)]
fn judge_site(
    span: Span,
    shape: SlotShape,
    qs: &Expr,
    绑定: &Bindings,
    action: Option<&str>,
    actions: Option<&crate::ActionTable>,
    profile: Option<&jpp_effects::Profile>,
    sites: &mut Vec<KindSite>,
    diags: &mut Vec<Diagnostic>,
) {
    let qs: Vec<&Expr> = match resolve(qs, 绑定, &mut HashSet::new()).kind() {
        ExprKind::List(items) => items.iter().collect(),
        _ => vec![qs],
    };
    // 依据：B51-R2（12 §2.2；诊断层消费者，步 24g）
    let mat_shape = action
        .zip(actions)
        .and_then(|(name, t)| t.shapes.get(name))
        .cloned();
    let diag_cx = crate::diag::b13::DiagCx {
        mat_shape,
        shape_action: action.map(str::to_string),
        one_hop: profile.map(|p| p.one_hop()).unwrap_or_default(),
        arithmetic_capable: profile.map(|p| p.arithmetic_capable()).unwrap_or_default(),
    };
    for q in qs {
        let Some(q) = question_of(q, 绑定, &mut HashSet::new()) else {
            continue;
        };
        let base = question_kind(q.op, q.request, &SlotShape::UNKNOWN, &q.decl);
        let refined = question_kind(q.op, q.request, &shape, &q.decl);
        // 与状态无关的矛盾已在题字面量处报过，这里只报状态可见后才出现的
        if kind_conflict(q.op, &SlotShape::UNKNOWN, &q.decl).is_none()
            && let Some(c) = kind_conflict(q.op, &shape, &q.decl)
        {
            diags.push(conflict_diag(&c.reason, span));
        }
        if let Some(d) = crate::diag::b13::shape_check(refined.0, span, &diag_cx) {
            diags.push(d);
        }
        sites.push(KindSite {
            span,
            op: q.op,
            base,
            refined: Some(refined),
        });
    }
}

/// `state(on, …)` 的 `on` 材料能否静态确定来自 `do("字面动作名", …)`：直接内联
/// `state(mat(do("名", …)))`，或 `on` 是绑定给某个名字后解得到同一形状。判不出（容器、非字面、
/// 多材料合并等）一律 `None`——B51-R2 消费者按此放过，零假拒绝面（步 24g）。复用 `resolve`，
/// 与 `state_shape` 同一套「同名后写覆盖先写、往保守偏」的让位追溯（本文件既有口径，B76）。
fn do_source<'a>(s: &'a Expr, 绑定: &'a Bindings) -> Option<&'a str> {
    let s = resolve(s, 绑定, &mut HashSet::new());
    if call_name(s) != Some("state") {
        return None;
    }
    let on = *call_args(s).first()?;
    let on = resolve(on, 绑定, &mut HashSet::new());
    let on = if call_name(on) == Some("mat") {
        resolve(*call_args(on).first()?, 绑定, &mut HashSet::new())
    } else {
        on
    };
    if call_name(on) != Some("do") {
        return None;
    }
    match call_args(on).first().map(|a| a.kind()) {
        Some(ExprKind::Text(name)) => Some(name.as_str()),
        _ => None,
    }
}

fn conflict_diag(reason: &str, span: Span) -> Diagnostic {
    // 依据：B76（附注/2026-09-24-评估①裁定.md §六「不唯一与推不出时怎么处理」(3)）
    Diagnostic::error(
        "E-kind-conflict",
        format!(
            "题类声明与结构矛盾：{reason}（B76）。题类由题型、请求、状态槽形与题式槽声明推出，声明只能在结构不唯一处定下题类，不能改写结构已经决定的题类。修法：改正题式的 over_kind 声明，或改正状态的槽"
        ),
        span,
    )
}

/// 名字沿 `let` 绑定解开（防环）。
fn resolve<'a>(e: &'a Expr, 绑定: &'a Bindings, seen: &mut HashSet<String>) -> &'a Expr {
    match e.kind() {
        ExprKind::Name(n) if seen.insert(n.to_string()) => match 绑定.get(n) {
            Some(v) => resolve(v, 绑定, seen),
            None => e,
        },
        _ => e,
    }
}

fn is_call_to(e: &Expr, name: &str, 绑定: &Bindings) -> bool {
    call_name(resolve(e, 绑定, &mut HashSet::new())) == Some(name)
}

/// 记录字面量里某字段的文本字面量
fn text_field<'a>(rec: Option<&&'a Expr>, key: &str) -> Option<&'a str> {
    let ExprKind::Record(fields) = rec?.kind() else {
        return None;
    };
    let (_, v) = fields.iter().find(|(k, _)| k == key)?;
    match v.kind() {
        ExprKind::Text(t) => Some(t.as_str()),
        _ => None,
    }
}

/// 题字面量（或题式）的请求与槽声明，供 `QuestionLit::with_decl` 算基础类。
pub(crate) fn decl_of(e: &Expr, 绑定: &Bindings) -> (Option<Request>, SlotDecls) {
    question_of(e, 绑定, &mut HashSet::new())
        .map(|q| (q.request, q.decl))
        .unwrap_or_default()
}

/// 表达式是不是一道可认出的题（或题式）；认不出返回 `None`。
fn question_of(e: &Expr, 绑定: &Bindings, seen: &mut HashSet<String>) -> Option<QInfo> {
    let e = resolve(e, 绑定, seen);
    let args = call_args(e);
    match call_name(e)? {
        name @ ("test" | "select") => Some(QInfo {
            op: op_of(name),
            request: text_field(args.get(2), "request").and_then(Request::parse),
            decl: SlotDecls::default(),
        }),
        "measure" => Some(QInfo {
            op: Op::Measure,
            request: None,
            decl: SlotDecls::default(),
        }),
        "form" => {
            let ExprKind::Text(op) = args.first()?.kind() else {
                return None;
            };
            let rec = args.get(2);
            Some(QInfo {
                op: op_of(op),
                request: text_field(rec, "request").and_then(Request::parse),
                decl: SlotDecls {
                    over_kind: text_field(rec, "over_kind").and_then(OverKind::parse),
                    accepts: None,
                },
            })
        }
        "fill" => question_of(args.first()?, 绑定, seen),
        _ => None,
    }
}

/// `state(on, {ctx, ref, over})` 的槽形；看不见状态构造时全为未知。
fn state_shape(s: &Expr, 绑定: &Bindings) -> SlotShape {
    let s = resolve(s, 绑定, &mut HashSet::new());
    if call_name(s) != Some("state") {
        return SlotShape::UNKNOWN;
    }
    let args = call_args(s);
    let on = match args.first().map(|a| a.kind()) {
        Some(ExprKind::List(items)) if items.len() == 2 => OnShape::Pair,
        Some(ExprKind::List(items)) if items.len() == 1 => OnShape::One,
        Some(ExprKind::Text(_)) => OnShape::One,
        Some(ExprKind::Call { function, .. }) if function.name() == Some("mat") => OnShape::One,
        _ => OnShape::Unknown,
    };
    let rec: Option<&[(String, Expr)]> = match args.get(1).map(|a| a.kind()) {
        Some(ExprKind::Record(fields)) => Some(fields),
        None => Some(&[]),
        _ => None,
    };
    let Some(rec) = rec else {
        return SlotShape {
            on,
            ..SlotShape::UNKNOWN
        };
    };
    let field = |k: &str| rec.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    let over = match field("over") {
        None => OverShape::Empty,
        Some(v) => over_shape(v, 绑定),
    };
    let question_material = ["ctx", "ref"].iter().any(|k| {
        field(k).is_some_and(|v| match v.kind() {
            ExprKind::List(items) => items.iter().any(|m| rendered_question(m, 绑定)),
            _ => rendered_question(v, 绑定),
        })
    });
    SlotShape {
        on,
        over,
        question_material,
    }
}

/// `over` 列表字面量的形状：全是文本字面量 → 标签；全是题 → 题值；全是 `mat(非字面量)` → 计算材料；
/// 其余（名字、混合、`mat("字面")`）→ 未知。
fn over_shape(v: &Expr, 绑定: &Bindings) -> OverShape {
    let ExprKind::List(items) = v.kind() else {
        return OverShape::Unknown;
    };
    if items.is_empty() {
        return OverShape::Unknown;
    }
    let all = |f: &dyn Fn(&Expr) -> bool| items.iter().all(f);
    if all(&|x| matches!(x.kind(), ExprKind::Text(_))) {
        OverShape::Labels
    } else if all(&|x| question_of(x, 绑定, &mut HashSet::new()).is_some()) {
        OverShape::Questions
    } else if all(&|x| computed_mat(x, 绑定)) {
        OverShape::Materials
    } else {
        OverShape::Unknown
    }
}

/// `mat(x)`，`x` 沿绑定解开后是调用、取字段或取下标（运行期算出）。解不开的名字（形参）
/// 可能是调用者给的字面文本，不算。
fn computed_mat(x: &Expr, 绑定: &Bindings) -> bool {
    call_name(x) == Some("mat")
        && call_args(x).first().is_some_and(|a| {
            let a = resolve(a, 绑定, &mut HashSet::new());
            matches!(
                a.kind(),
                ExprKind::Call { .. } | ExprKind::Field { .. } | ExprKind::Index { .. }
            ) && question_of(a, 绑定, &mut HashSet::new()).is_none()
                && call_name(a) != Some("mat")
        })
}

/// `mat(题)`：由题值渲染的材料（B4）
fn rendered_question(m: &Expr, 绑定: &Bindings) -> bool {
    call_name(m) == Some("mat")
        && call_args(m)
            .first()
            .is_some_and(|q| question_of(q, 绑定, &mut HashSet::new()).is_some())
}
