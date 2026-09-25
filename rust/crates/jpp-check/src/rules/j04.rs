//! J-04 静态面，两条：
//!
//! - **H2**（`12` §1.2，步 24d）：`test`/`select`/`measure` 三种输出结构的画像字段未测时点名。
//!   `fixed_output_types` 管三种输出结构是否固定（无「都不是」除非显式给），三个构造都受它管；
//!   `select_sums_to_one` 是 `select` 特有的「候选概率和恒为 1」假设，只在 `select` 上核。
//!   字段为已测（`true`/`false`）时都不报。
//! - **比较性指纹**（步 24e，`order` 半，`12`:255）：`order` 收一个**字面列表**、每项都能确定
//!   来自 `judge(state, <字面题式构造>)`（内联写或绑定给恰好出现一次的名字）时，逐项比较题式
//!   的 `(构造名, 题面文字, 档位列表)`；不同即报——这是运行期 `repeat.rs::order` 的 `q_hash`/
//!   `over_len`/`scale` 不可比检查在**能静态确定**时的编译期镜像，判不出来源的项一律按可比放过
//!   （零假拒绝面，运行期兜底）。`fit` 半（跨题联合判断的指纹核验）需要检查器拿到 fit 注册表，
//!   `Cx` 目前没有这个视图，留给 J-16/K-034 那批一起补管线。`over_len` 不同但 `state` 的 `over`
//!   槽也是字面列表时可另行核长度，本批不做（题面/档位已是最常见的误用，先做这条）。

#![allow(unused_imports)]
use std::collections::HashMap;

use super::{CallSite, Cx, Hooks, Rule};
use crate::*;
use jpp_effects::spec::SlotKind;
use jpp_ir::ir::{Host, Node};

/// 依据：12 §5 J-04；H2（步 24d）、比较性指纹 `order` 半（步 24e）。
pub(crate) const RULE: Rule = Rule {
    code: "J-04",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    if matches!(s.name, "test" | "select" | "measure") {
        h2_output_types(cx, s, &mut out);
    }
    if s.name == "order" {
        order_comparability(cx, s, &mut out);
    }
    out
}

fn h2_output_types(cx: &Cx, s: &CallSite, out: &mut Vec<Diagnostic>) {
    let Some(p) = cx.profile else { return };
    // 依据：H2（12 §1.2，步 24d）
    if p.fixed_output_types() == jpp_effects::Tri::未测 {
        out.push(Diagnostic::warning(
            "W-untested",
            format!(
                "画像没有测过 fixed_output_types（H2）：{} 的输出结构假定固定，没有「都不是」除非显式给",
                s.name
            ),
            s.span,
        ));
    }
    // 依据：H2（12 §1.2，步 24d）
    if s.name == "select" && p.select_sums_to_one() == jpp_effects::Tri::未测 {
        out.push(Diagnostic::warning(
            "W-untested",
            "画像没有测过 select_sums_to_one（H2）：select 的候选概率和假定恒为 1",
            s.span,
        ));
    }
}

/// 题式构造的比较键：(构造名, 题面文字, 档位列表)。`evidence`/`presupposition` 等选项不参与
/// 比较（更细的情形，本批不比，方向是少报不多报）。
type QKey<'a> = (&'a str, &'a str, Vec<&'a str>);

fn literal_text(e: &Expr) -> Option<&str> {
    match e.kind() {
        ExprKind::Text(t) => Some(t.as_str()),
        _ => None,
    }
}

fn question_key(e: &Expr) -> Option<QKey<'_>> {
    let Node::Construct { name, args, .. } = &e.node else {
        return None;
    };
    match name.as_str() {
        "test" | "select" => Some((name.as_str(), literal_text(args.first()?)?, vec![])),
        "measure" => {
            let text = literal_text(args.first()?)?;
            let ExprKind::List(items) = args.get(1)?.kind() else {
                return None;
            };
            let scale = items.iter().map(literal_text).collect::<Option<Vec<_>>>()?;
            Some((name.as_str(), text, scale))
        }
        _ => None,
    }
}

/// 程序里每个名字被 `let`/形参绑定的次数，与绑定为「恰好一次」时那唯一一次的值表达式（形参
/// 没有值表达式，只占计数、不进 `first`）。绑定超过一次（遮蔽/重写）的名字不收——同
/// `j06.rs::binding_counts` 的保守方向：判不清楚宁可放过，不误报。
fn unique_bindings(p: &Program) -> HashMap<&str, &Expr> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut first: HashMap<&str, &Expr> = HashMap::new();
    count_block(&p.body, &mut counts, &mut first);
    counts
        .into_iter()
        .filter(|(_, n)| *n == 1)
        .filter_map(|(k, _)| first.get(k).map(|v| (k, *v)))
        .collect()
}

fn count_block<'a>(
    b: &'a Block,
    counts: &mut HashMap<&'a str, usize>,
    first: &mut HashMap<&'a str, &'a Expr>,
) {
    for s in &b.statements {
        match s {
            Statement::Let { name, value, .. } => {
                *counts.entry(name.as_str()).or_default() += 1;
                first.entry(name.as_str()).or_insert(value);
                count_expr(value, counts, first);
            }
            Statement::Function { name, function, .. } => {
                *counts.entry(name.as_str()).or_default() += 1;
                count_block(&function.body, counts, first);
            }
            Statement::Expr(e) => count_expr(e, counts, first),
        }
    }
    if let Some(r) = &b.result {
        count_expr(r, counts, first);
    }
}

fn count_expr<'a>(
    e: &'a Expr,
    counts: &mut HashMap<&'a str, usize>,
    first: &mut HashMap<&'a str, &'a Expr>,
) {
    match &e.node {
        Node::Host(Host::Function(f)) => {
            for p in &f.parameters {
                *counts.entry(p.name.as_str()).or_default() += 1;
            }
            count_block(&f.body, counts, first);
        }
        Node::Host(Host::If {
            condition, yes, no, ..
        }) => {
            count_expr(condition, counts, first);
            count_block(yes, counts, first);
            count_block(no, counts, first);
        }
        Node::Host(Host::Block(b)) => count_block(b, counts, first),
        _ => e
            .children()
            .into_iter()
            .for_each(|x| count_expr(x, counts, first)),
    }
}

/// 把一个表达式解析成它的题式比较键：直接是「产出读数」的效应调用（`judge`）就按 `SlotKind::
/// Questions` 槽泛化取题式实参（不写死 "judge"/"questions" 字样，同 20 A2）；是名字就查「恰好
/// 绑定一次」的表，递归解一层。查不到、不是产出读数的效应、题面不是字面构造，一律 `None`
/// （按可比放过，零假拒绝面，运行期兜底）。
fn resolve<'a>(e: &'a Expr, uniques: &HashMap<&'a str, &'a Expr>) -> Option<QKey<'a>> {
    let target = match &e.node {
        Node::Effect { .. } => e,
        Node::Host(Host::Name(n)) => *uniques.get(n.as_str())?,
        _ => return None,
    };
    let Node::Effect { effect, inputs, .. } = &target.node else {
        return None;
    };
    let sp = jpp_effects::spec(*effect);
    if !sp.produces_reading {
        return None;
    }
    let pos = sp
        .input_schema
        .iter()
        .position(|d| d.kind == SlotKind::Questions)?;
    let (_, q) = inputs.get(pos)?;
    question_key(q)
}

fn order_comparability(cx: &Cx, s: &CallSite, out: &mut Vec<Diagnostic>) {
    let Some(list_arg) = s.args.first() else {
        return;
    };
    let ExprKind::List(items) = list_arg.kind() else {
        return;
    };
    if items.len() < 2 {
        return;
    }
    let uniques = unique_bindings(cx.p);
    let mut first: Option<(QKey<'_>, &Expr)> = None;
    for item in items {
        let Some(key) = resolve(item, &uniques) else {
            continue;
        };
        match &first {
            None => first = Some((key, item)),
            Some((fkey, _)) if fkey == &key => {}
            Some((fkey, fitem)) => {
                // 依据：12 §5 J-04（比较性；order 只排同一道题跨对象的读数，镜像
                // `repeat.rs::b_order` 的 q_hash 不可比检查）
                out.push(Diagnostic::error(
                    "J-04",
                    format!(
                        "order 里的读数来自不同的题：{:?}（{:?}）与 {:?}（本项）——跨题的读数不可比（各有各的校准线，p 不在同一把尺子上）",
                        fkey, fitem.span, key
                    ),
                    item.span,
                ));
                return;
            }
        }
    }
}
