//! pass `speculate`（`12`:610 v0.1.1 修订记录 1「judge 推测提升」）与 `vectorize` 共用的静态一半：
//! 从一个触发点出发，哪些 `judge` 站点在语法上可以提前登记。
//!
//! 步 13a 从运行时的 `speculate_ahead`/`speculate_in_ifs`/`speculate_branch_with`/`speculate_branch`/
//! `speculate_expr` 搬来，遍历顺序与剪枝口径逐字相同；搬走的是「判」，「求值与登记」留在运行时。
//! 运行期那一半（状态与题两段表达式按环境会不会产生效应）在 [`crate::Hooks`]。
//!
//! **只推测 `judge`**：`do`/`gen`/`ask` 有代价或触世界，猜错要回滚而我们没有回滚（红队 06 A1）。
//! **只走直线段**：遇到新绑定的名字记进 `born`，依赖它们的站点此刻算不出状态，不推。

use crate::analysis::{has_impure, names_in};
use jpp_effects::view::{Callee, K, kind};
use jpp_ir::ir::{Block, Expr, Stmt};
use jpp_ir::plan::TargetSite;
use std::collections::{BTreeSet, HashSet};

/// 块 `b` 从第 `from` 条语句起，`if` 两侧分支体里的候选站点（原 `speculate_ahead`）。
pub fn block_from(b: &Block, from: usize) -> Vec<TargetSite> {
    let mut out = vec![];
    let mut born: HashSet<String> = HashSet::new();
    for st in b.statements.iter().skip(from) {
        match st {
            Stmt::Let { name, value, .. } => {
                in_ifs(value, &born, &mut out, false);
                born.insert(name.clone());
            }
            Stmt::Expr(e) => in_ifs(e, &born, &mut out, false),
            Stmt::Function { name, .. } => {
                born.insert(name.clone());
            }
        }
    }
    if let Some(r) = &b.result {
        in_ifs(r, &born, &mut out, false);
    }
    out
}

/// 方法体一轮里的候选站点（原 `vectorize_ahead` 对闭包体调 `speculate_branch_with(体, 空)`）。
pub fn body(b: &Block) -> Vec<TargetSite> {
    let mut out = vec![];
    branch_with(b, &HashSet::new(), &mut out, true);
    out
}

/// 在表达式里找 `if`，看它两侧的分支体（原 `speculate_in_ifs`）
fn in_ifs(e: &Expr, born: &HashSet<String>, out: &mut Vec<TargetSite>, d: bool) {
    match kind(e) {
        K::If { yes, no, .. } => {
            branch_with(yes, born, out, d);
            branch_with(no, born, out, d);
        }
        K::Block(inner) => {
            if let Some(r) = &inner.result {
                in_ifs(r, born, out, d);
            }
        }
        _ => {}
    }
}

/// 原 `speculate_branch_with`
fn branch_with(b: &Block, outer_born: &HashSet<String>, out: &mut Vec<TargetSite>, d: bool) {
    let mut born = outer_born.clone();
    for st in &b.statements {
        match st {
            Stmt::Let { name, value, .. } => {
                expr(value, &born, out, d);
                born.insert(name.clone());
            }
            Stmt::Expr(e) => expr(e, &born, out, d),
            Stmt::Function { name, .. } => {
                born.insert(name.clone());
            }
        }
    }
    if let Some(r) = &b.result {
        expr(r, &born, out, d);
    }
}

/// 原 `speculate_branch`：块表达式里另起一份 `born`（与搬家前同口径）
fn branch(b: &Block, out: &mut Vec<TargetSite>, d: bool) {
    branch_with(b, &HashSet::new(), out, d);
}

/// 原 `speculate_expr` 的「判」：遇到 `judge(状态, 题)` 记为候选并停；含副作用的调用整棵不推。
fn expr(e: &Expr, born: &HashSet<String>, out: &mut Vec<TargetSite>, d: bool) {
    // 含副作用的调用：整棵子树都不推（不跨分支推测 do/gen/ask）
    if has_impure(e) {
        return;
    }
    if let K::Call {
        callee,
        args: arguments,
    } = kind(e)
        && let Some(n) = callee.name()
        && jpp_effects::by_name(n).is_some_and(|s| s.produces_reading)
        && arguments.len() == 2
    {
        // 用到分支体内才产生的名字：此刻算不出状态
        let mut used = BTreeSet::new();
        names_in(e, &mut used);
        if used.iter().any(|u| born.contains(u)) {
            return;
        }
        out.push(TargetSite {
            node: e.id,
            needs_bound: used,
            call: false,
        });
        return;
    }
    // 步 13b：向量化（`d`）时，对用户函数的调用也是候选：被调者是一个名字、实参只有名字或字面量
    // （提前求值实参不执行任何东西）。名字是否解析为方法值、实参会不会产生效应，运行期由钩子判。
    // `if` 两侧的推测（`d` 为假）不穿过调用，与 13a 相同。
    if d && let K::Call {
        callee: Callee::Expr(c),
        args,
    } = kind(e)
        && matches!(kind(c), K::Name(_))
        && args.iter().all(|a| simple_arg(a))
    {
        let mut used = BTreeSet::new();
        names_in(e, &mut used);
        if !used.iter().any(|u| born.contains(u)) {
            out.push(TargetSite {
                node: e.id,
                needs_bound: used,
                call: true,
            });
        }
    }
    // 往下走（只在纯表达式里）
    match kind(e) {
        K::Call { callee, args } => {
            if let Callee::Expr(c) = callee {
                expr(c, born, out, d);
            }
            for a in args {
                expr(a, born, out, d);
            }
        }
        K::List(items) => items.iter().for_each(|x| expr(x, born, out, d)),
        K::Record(fs) => fs.iter().for_each(|(_, x)| expr(x, born, out, d)),
        K::Field { value, .. } => expr(value, born, out, d),
        K::Binary { left, right, .. } => {
            expr(left, born, out, d);
            expr(right, born, out, d);
        }
        K::Block(b) => branch(b, out, d),
        _ => {}
    }
}

/// 实参是名字或字面量（步 13b 穿过调用的条件 1）
fn simple_arg(a: &Expr) -> bool {
    matches!(
        kind(a),
        K::Name(_) | K::Integer(_) | K::Decimal(_) | K::Bool(_) | K::Text(_) | K::Unit
    )
}
