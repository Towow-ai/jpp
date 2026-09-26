//! 推测、提升、向量化共用的分析（步 13a 从 `jpp-core/src/interp/mod.rs` 原样搬来）。
//!
//! 纯语法的部分（`has_impure`、`has_branch`、`names_in`、`judged_state`）由各 pass 在计划期用；
//! 按环境解析名字的部分（`may_effect`、`may_touch_world`）经 [`crate::Hooks`] 在运行期用，环境经
//! [`EnvView`] 读（运行时实现）。判断口径与搬家前逐字相同：K-069/K-075 的修法（看函数体、
//! 看不透的调用按有效应）原样保留。

use jpp_effects::SchedClass;
use jpp_effects::view::{Callee, K, kind};
use jpp_ir::ir::{Block, Expr, Stmt};
use jpp_ir::plan::{EnvView, ValueSummary};
use std::collections::{BTreeSet, HashSet};

/// 这个表达式是 `judge(状态, 题)` 吗；是的话给出**状态那一段的节点**
pub(crate) fn judged_state(e: &Expr) -> Option<&Expr> {
    let K::Call { callee, args } = kind(e) else {
        return None;
    };
    if !callee
        .name()
        .and_then(jpp_effects::by_name)
        .is_some_and(|s| s.produces_reading)
    {
        return None;
    }
    args.first().copied()
}

/// 推测、提升、向量化可以**提前求值**的内置：不触世界、不刷新判断、不销账、不打印。
/// 不在这张表上的内置（`do`/`gen`/`ask`/`transform`/`cut`/`handle`/`consume`/`sieve`/`print`…）
/// 与**任何看不透的调用**一样按「可能有效应」处理（K-069：未分析的能力一律保守表示为可能带效应）。
/// `judge` 在表上：它只登记、不发出，推测本来就是在登记它。
const 可提前求值的内置: &[&str] = &[
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "mat",
    "content",
    "taint",
    "key_of",
    "len",
    "range",
    "append",
    "concat",
    "slice",
    "contains",
    "sum",
    "reverse",
    "keys",
    "with",
    "has",
    "text",
    "join",
    "min",
    "max",
    "abs",
    "floor",
    "exit_kind",
    "unsure_cause",
    "untested",
    "line_source",
    "cert",
    "is_fail",
    "fail",
    "stop",
    "map",
    "filter",
    "fold",
];

/// **看环境判断**：这个表达式求值时会不会产生效应（K-069 / K-075，2026-09-23 修）。
///
/// 旧的 `has_impure` 只认内置名，调用用户函数一律当纯，于是推测执行在条件为假的分支里
/// 提前求值 `state(mat(side(1)))`，`side` 里的 `do` 真的执行了。这里改为：
/// - 调用内置：只有「可提前求值」表上的才算纯；
/// - 调用用户函数（或把函数当值传给 `map` 之类）：**看函数体**，不看它的效应声明——
///   空声明不是纯净的证明；递归时同一个函数只看一次；
/// - 看不透的调用（名字解析不到、调用一个表达式的结果、字段里的方法）：按有效应。
pub(crate) fn may_effect(e: &Expr, env: &dyn EnvView) -> bool {
    let mut seen: HashSet<String> = HashSet::new();
    effect_in_expr(e, env, &mut seen, 严格)
}

/// 只问「会不会触世界」（`do`/`gen`/`ask`/`transform`/`escalate`/`literalize`，含藏在用户函数里的）。
/// `lift` 用它判断能不能**越过**一句（不求值那一句，只改判断与动作的先后）；`cut`、`handle`
/// 这类只刷新判断或销账的内置不算触世界。看不透的调用仍按触世界处理。
pub(crate) fn may_touch_world(e: &Expr, env: &dyn EnvView) -> bool {
    let mut seen: HashSet<String> = HashSet::new();
    effect_in_expr(e, env, &mut seen, 触世界)
}

const 严格: bool = true;
const 触世界: bool = false;
const 触世界的内置: &[&str] = &["do", "gen", "ask", "transform", "escalate", "literalize"];

fn effect_in_value(v: &ValueSummary, seen: &mut HashSet<String>, strict: bool) -> bool {
    match v {
        ValueSummary::Builtin(b) => {
            if strict {
                !可提前求值的内置.contains(&b.as_str())
            } else {
                触世界的内置.contains(&b.as_str())
            }
        }
        ValueSummary::Fn(c) => {
            if !seen.insert(c.identity().to_string()) {
                return false;
            }
            effect_in_block(&c.function().body, &*c.env(), seen, strict)
        }
        ValueSummary::Container(items) => items().iter().any(|x| effect_in_value(x, seen, strict)),
        ValueSummary::Data => false,
    }
}

fn effect_in_block(b: &Block, env: &dyn EnvView, seen: &mut HashSet<String>, strict: bool) -> bool {
    b.statements.iter().any(|st| match st {
        Stmt::Let { value, .. } => effect_in_expr(value, env, seen, strict),
        Stmt::Expr(e) => effect_in_expr(e, env, seen, strict),
        Stmt::Function { function, .. } => effect_in_block(&function.body, env, seen, strict),
    }) || b
        .result
        .as_ref()
        .is_some_and(|r| effect_in_expr(r, env, seen, strict))
}

fn effect_in_expr(e: &Expr, env: &dyn EnvView, seen: &mut HashSet<String>, strict: bool) -> bool {
    match kind(e) {
        K::Call { callee, args } => {
            let callee = match callee.name() {
                // 名字解析不到（函数体里的局部名、形参）：调用它就看不透
                Some(n) => match env.lookup(n) {
                    Some(v) => effect_in_value(&v, seen, strict),
                    None => true,
                },
                // 调用一个表达式的结果、字段里的方法：看不透
                None => true,
            };
            callee || args.iter().any(|a| effect_in_expr(a, env, seen, strict))
        }
        // 函数被当作值传走（`map(xs, side)`）：它之后会被调用
        K::Name(n) => env
            .lookup(n)
            .is_some_and(|v| matches!(v, ValueSummary::Fn(_)) && effect_in_value(&v, seen, strict)),
        K::Function(f) => effect_in_block(&f.body, env, seen, strict),
        K::List(items) => items.iter().any(|x| effect_in_expr(x, env, seen, strict)),
        K::Record(fs) => fs.iter().any(|(_, x)| effect_in_expr(x, env, seen, strict)),
        K::Field { value, .. } | K::Unary { value, .. } => effect_in_expr(value, env, seen, strict),
        K::Index { value, index } => {
            effect_in_expr(value, env, seen, strict) || effect_in_expr(index, env, seen, strict)
        }
        K::Binary { left, right, .. } => {
            effect_in_expr(left, env, seen, strict) || effect_in_expr(right, env, seen, strict)
        }
        K::If { condition, yes, no } => {
            effect_in_expr(condition, env, seen, strict)
                || effect_in_block(yes, env, seen, strict)
                || effect_in_block(no, env, seen, strict)
        }
        K::Block(b) => effect_in_block(b, env, seen, strict),
        _ => false,
    }
}

/// 表达式里有没有会产生副作用或改状态的调用（提升不能跨过它们）
pub(crate) fn has_impure(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| {
        // 效应按 `EffectSpec.sched` 认（步 15a，`20` A2 与 §3.x 不变量 (2)）：`Immediate` 的效应当场执行，
        // 提升与推测不能跨过；`escalate`、`literalize` 两个消费形式隐含效应，按名字认。
        if let K::Call { callee, .. } = x
            && let Some(n) = callee.name()
            && (jpp_effects::by_name(n).is_some_and(|s| s.sched == SchedClass::Immediate)
                || matches!(n, "escalate" | "literalize"))
        {
            found = true;
        }
    });
    found
}

/// 表达式里有没有分支或循环（**不跨分支**：`12`:13）
pub(crate) fn has_branch(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| match x {
        K::If { .. } => found = true,
        K::Call { callee, .. } => {
            if matches!(
                callee.name(),
                Some("loop" | "iterate" | "map" | "filter" | "fold" | "handle" | "pair")
            ) {
                found = true;
            }
        }
        _ => {}
    });
    found
}

/// 表达式里有没有短路运算（`&&`、`||`）：右侧不一定求值，直线段提升把它当分支，遇到即停
/// （步 23c 审查修复 4，同 13a「不跨分支」，`12`:13）。块表达式里的语句也看，因为 `speculate::expr` 会进块。
pub(crate) fn has_short_circuit(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |x| match x {
        K::Binary { op, .. } if matches!(*op, "&&" | "||") => found = true,
        K::Block(b) if block_has_short_circuit(b) => found = true,
        _ => {}
    });
    found
}

/// 块里（含嵌套块）每个短路运算（`&&`、`||`）右侧子树的节点号：右侧不一定求值。直线段提升穿进
/// 被调函数体时，这些节点上的站点不收（步 23c 复核修复 9，与 `has_short_circuit` 同一理由）。
pub(crate) fn short_rhs_nodes(b: &Block) -> BTreeSet<jpp_ir::key::NodeId> {
    let mut out = BTreeSet::new();
    jpp_ir::ir::walk(b, &mut |e| {
        if let K::Binary { op, right, .. } = kind(e)
            && matches!(op, "&&" | "||")
        {
            jpp_ir::ir::walk_expr(right, &mut |x| {
                out.insert(x.id);
            });
        }
    });
    out
}

fn block_has_short_circuit(b: &Block) -> bool {
    b.statements.iter().any(|st| match st {
        Stmt::Let { value, .. } => has_short_circuit(value),
        Stmt::Expr(v) => has_short_circuit(v),
        Stmt::Function { .. } => false,
    }) || b.result.as_ref().is_some_and(|r| has_short_circuit(r))
}

pub(crate) fn names_in(e: &Expr, out: &mut BTreeSet<String>) {
    walk(e, &mut |x| {
        if let K::Name(n) = x {
            out.insert(n.to_string());
        }
    });
}

/// 遍历一个表达式（不进函数体、不进分支体）。语言形式与效应节点的名字作为一个名字节点被访问，
/// 与步 12c 前遍历源码树时访问被调用者那个名字节点同口径。
fn walk(e: &Expr, f: &mut impl FnMut(&K)) {
    let k = kind(e);
    f(&k);
    match k {
        K::List(items) => items.iter().for_each(|x| walk(x, f)),
        K::Record(fs) => fs.iter().for_each(|(_, x)| walk(x, f)),
        K::Function(_) => {}
        K::Call { callee, args } => {
            match callee {
                Callee::Name(n) => f(&K::Name(n)),
                Callee::Expr(c) => walk(c, f),
            }
            args.iter().for_each(|x| walk(x, f));
        }
        K::Field { value, .. } => walk(value, f),
        K::Index { value, index } => {
            walk(value, f);
            walk(index, f);
        }
        K::Unary { value, .. } => walk(value, f),
        K::Binary { left, right, .. } => {
            walk(left, f);
            walk(right, f);
        }
        K::If { condition, .. } => walk(condition, f),
        _ => {}
    }
}
