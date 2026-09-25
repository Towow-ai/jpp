//! J-06 / E5 有界循环的静态面：`loop`、`iterate` 必带正整数 bound；bound 不是字面量时告警
//! `W-bound`（静态估不出上界，只有运行期能核）。运行期那一半（键重复即停）在运行时。
//!
//! 步 24a 补两项（B69、B111）：
//! - **B69 常量传播**：`bound` 若是名字，且这个名字在**整个程序**里恰好绑定一次、且这一次绑定
//!   是**程序顶层块**的 `let name = <正整数字面量>;`，视为常量，不报 `W-bound`。判据故意粗
//!   （「全程序恰好绑定一次」代替位置敏感的作用域解析，`hooks.call` 不带作用域链，另建通道不在
//!   本批）：真正的遮蔽会让名字在别处再绑定一次，计数 > 1，判定失败退回报 `W-bound`——只会少
//!   报，不会把遮蔽误判成命中。
//! - **B111 调用点一层传播**：某个**具名函数**（`fn name(...) { ... }`，不含 `let` 绑定的函数
//!   字面量）的函数体里，顶层语句或结果直接是 `loop(p, …)`/`iterate(p, …)`、`p` 是它自己的形参
//!   名，体内不报；改在**每个调用点**核对应位置的实参（字面量或 B69 常量即可，否则在调用点报
//!   `W-bound`）。只做「函数 ← 直接调用点」一层，不沿递归边再往外传播——递归调用本身也只是
//!   一个调用点，同一条规则核，不特殊处理。
//! 依据：B69（`地基/附注/2026-09-24-探针首轮裁定.md` §五·1）；B111（`12` §3 J-06 行，A-3b）。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;
use std::collections::HashMap;

/// 依据：12 §5 J-06（loop 必带 bound；静态面判存在性）；11 §诊断 E5；B69、B111（步 24a）。
pub(crate) const RULE: Rule = Rule {
    code: "J-06",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

/// 程序顶层块里 `let name = <正整数字面量>;` 的候选常量（名字 → 值）。「顶层」= `p.body.statements`
/// 本身，不含任何嵌套块或函数体。是否真的可用还要看 `binding_counts` 里这个名字只绑定过一次。
fn top_level_int_consts(p: &Program) -> HashMap<&str, i64> {
    let mut out = HashMap::new();
    for s in &p.body.statements {
        if let Statement::Let { name, value, .. } = s
            && let ExprKind::Integer(k) = value.kind()
            && k > 0
        {
            out.insert(name.as_str(), k);
        }
    }
    out
}

/// 整个程序里每个名字被「绑定」过几次：`let`、具名函数名、函数形参名，顶层与全部嵌套一起数。
/// 计数 > 1 即视为可能被遮蔽，B69 不采信；这是故意偏保守的判据（见文件头注）。
fn binding_counts(p: &Program) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    count_block(&p.body, &mut counts);
    counts
}

fn bump<'a>(counts: &mut HashMap<&'a str, usize>, name: &'a str) {
    *counts.entry(name).or_insert(0) += 1;
}

fn count_block<'a>(b: &'a Block, counts: &mut HashMap<&'a str, usize>) {
    for s in &b.statements {
        match s {
            Statement::Let { name, value, .. } => {
                bump(counts, name.as_str());
                count_expr(value, counts);
            }
            Statement::Function { name, function, .. } => {
                bump(counts, name.as_str());
                count_function(function, counts);
            }
            Statement::Expr(e) => count_expr(e, counts),
        }
    }
    if let Some(r) = &b.result {
        count_expr(r, counts);
    }
}

fn count_function<'a>(f: &'a Function, counts: &mut HashMap<&'a str, usize>) {
    for param in &f.parameters {
        bump(counts, param.name.as_str());
    }
    count_block(&f.body, counts);
}

fn count_expr<'a>(e: &'a Expr, counts: &mut HashMap<&'a str, usize>) {
    match e.kind() {
        ExprKind::Function(f) => count_function(f, counts),
        ExprKind::List(items) => items.iter().for_each(|x| count_expr(x, counts)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| count_expr(x, counts)),
        ExprKind::Call {
            function,
            arguments,
        } => {
            if let Some(callee) = function.expr() {
                count_expr(callee, counts);
            }
            arguments.iter().for_each(|a| count_expr(a, counts));
        }
        ExprKind::Field { value, .. } | ExprKind::Unary { value, .. } => count_expr(value, counts),
        ExprKind::Index { value, index } => {
            count_expr(value, counts);
            count_expr(index, counts);
        }
        ExprKind::Binary { left, right, .. } => {
            count_expr(left, counts);
            count_expr(right, counts);
        }
        ExprKind::If { condition, yes, no } => {
            count_expr(condition, counts);
            count_block(yes, counts);
            count_block(no, counts);
        }
        ExprKind::Block(b) => count_block(b, counts),
        _ => {}
    }
}

/// `bound` 是名字 `n`：能否判定为 B69 常量（顶层单赋值正整数字面量）。
fn resolves_as_const(consts: &HashMap<&str, i64>, counts: &HashMap<&str, usize>, n: &str) -> bool {
    consts.contains_key(n) && counts.get(n) == Some(&1)
}

/// 按名字找**具名函数**（`Statement::Function`），只在顶层块与具名函数体内递归找，不进匿名函数
/// 字面量（B111 范围收窄，见文件头注）。
fn find_function<'a>(p: &'a Program, name: &str) -> Option<&'a Function> {
    find_in_block(&p.body, name)
}

fn find_in_block<'a>(b: &'a Block, name: &str) -> Option<&'a Function> {
    for s in &b.statements {
        if let Statement::Function {
            name: n, function, ..
        } = s
        {
            if n == name {
                return Some(function);
            }
            if let Some(f) = find_in_block(&function.body, name) {
                return Some(f);
            }
        }
    }
    None
}

/// 按 `NodeId` 找具名函数（B111 体内判断：从 `CallSite.function` 拿到当前调用所在的函数 id，
/// 反查它是不是具名函数、形参叫什么）。
fn function_by_id(p: &Program, id: jpp_ir::key::NodeId) -> Option<&Function> {
    find_by_id(&p.body, id)
}

fn find_by_id(b: &Block, id: jpp_ir::key::NodeId) -> Option<&Function> {
    for s in &b.statements {
        if let Statement::Function { function, .. } = s {
            if function.id == id {
                return Some(function);
            }
            if let Some(f) = find_by_id(&function.body, id) {
                return Some(f);
            }
        }
    }
    None
}

/// 表达式是不是 `loop(p, …)`/`iterate(p, …)`，`p` 是形参列表里的哪一个（B111）。
fn bound_arg_is_param(e: &Expr, params: &[jpp_ir::ir::Parameter]) -> Option<usize> {
    let ExprKind::Call {
        function,
        arguments,
    } = e.kind()
    else {
        return None;
    };
    if !matches!(function.name(), Some("loop") | Some("iterate")) {
        return None;
    }
    let ExprKind::Name(n) = arguments.first()?.kind() else {
        return None;
    };
    params.iter().position(|p| p.name == n)
}

/// 具名函数体**顶层**（语句或结果，不下钻嵌套块）是否把自己的某个形参直接喂给 `loop`/`iterate`
/// 的 `bound`；有则返回那个形参的位置（B111）。
fn bound_param_index(f: &Function) -> Option<usize> {
    for s in &f.body.statements {
        let e = match s {
            Statement::Expr(e) => e,
            Statement::Let { value, .. } => value,
            Statement::Function { .. } => continue,
        };
        if let Some(i) = bound_arg_is_param(e, &f.parameters) {
            return Some(i);
        }
    }
    if let Some(r) = &f.body.result {
        if let Some(i) = bound_arg_is_param(r, &f.parameters) {
            return Some(i);
        }
    }
    None
}

/// 一个实参是否「可用作 bound」：字面量正整数，或可解为 B69 常量的名字。
fn arg_ok(consts: &HashMap<&str, i64>, counts: &HashMap<&str, usize>, e: &Expr) -> bool {
    match e.kind() {
        ExprKind::Integer(n) => n > 0,
        ExprKind::Name(n) => resolves_as_const(consts, counts, n),
        _ => false,
    }
}

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let (args, span) = (s.args, s.span);
    let consts = top_level_int_consts(cx.p);
    let counts = binding_counts(cx.p);
    match s.name {
        // E5：有界循环必带 bound
        "loop" => {
            if args.len() != 3 {
                out.push(Diagnostic::error(
                    "J-06",
                    format!("loop 缺 bound：有界循环必须带 bound，要写成 loop(bound, 初值, fn(acc, i))，这里给了 {} 个参数", args.len()),
                    span,
                ));
            } else {
                bound_diag(cx, s, &consts, &counts, args[0], &mut out, "loop");
            }
        }
        // E5 / J-06：iterate 同样是有界循环，bound 必带
        "iterate" => {
            if args.len() != 4 {
                out.push(Diagnostic::error(
                    "J-06",
                    format!("iterate 要写成 iterate(bound, 初值, fn(acc, i), measure)：measure 是 fn(acc) -> Int 或 \"tokens\"，这里给了 {} 个参数", args.len()),
                    span,
                ));
            } else {
                bound_diag(cx, s, &consts, &counts, args[0], &mut out, "iterate");
            }
        }
        _ => {}
    }
    // B111：这次调用是不是在调某个「体内把自己的形参直接喂给 bound」的具名函数；是则核对应
    // 位置的实参（与调用点是 loop/iterate 与否无关——查的是被调函数，不是这次调用本身的内置名）。
    if let Some(target) = find_function(cx.p, s.name)
        && let Some(idx) = bound_param_index(target)
        && let Some(actual) = args.get(idx)
        && !arg_ok(&consts, &counts, actual)
    {
        out.push(Diagnostic::warning(
            "W-bound",
            format!(
                "调用 {} 时第 {} 个实参喂给了 {}（函数体内直接把它当 loop/iterate 的 bound）：不是字面量也不是可判定的顶层常量，静态估不出上界。修法：传整数字面量，或先 let 一个顶层常量再传",
                s.name, idx + 1, target.parameters[idx].name
            ),
            actual.span,
        ));
    }
    out
}

/// `loop`/`iterate` 的 `bound` 实参本身该不该报：字面量走原有的「必须正整数」判据；名字先看
/// B69 常量、再看 B111「体内不报」（自己函数的形参、留给调用点核），否则维持原有 `W-bound`。
#[allow(clippy::too_many_arguments)]
fn bound_diag<'a>(
    cx: &Cx,
    s: &CallSite,
    consts: &HashMap<&str, i64>,
    counts: &HashMap<&str, usize>,
    bound: &'a Expr,
    out: &mut Vec<Diagnostic>,
    which: &str,
) {
    match bound.kind() {
        ExprKind::Integer(n) if n > 0 => {}
        ExprKind::Integer(n) => out.push(Diagnostic::error(
            "J-06",
            format!(
                "{which} 的 bound 是 {n}：bound 必须是正整数，否则{}",
                if which == "loop" {
                    "循环体一次都不跑"
                } else {
                    ""
                }
            ),
            bound.span,
        )),
        ExprKind::Name(n) => {
            if resolves_as_const(consts, counts, n) {
                // B69：命中顶层常量，不报
            } else if s
                .function
                .and_then(|id| function_by_id(cx.p, id))
                .is_some_and(|f| f.parameters.iter().any(|p| p.name == n))
            {
                // B111：bound 直接是自己函数的形参，体内不报，交给调用点核
            } else {
                out.push(Diagnostic::warning(
                    "W-bound",
                    format!("{which} 的 bound 不是字面量：静态估不出上界，只有运行期能核。修法：写成整数字面量，或顶层 let 一个整数常量"),
                    bound.span,
                ));
            }
        }
        _ => out.push(Diagnostic::warning(
            "W-bound",
            format!(
                "{which} 的 bound 不是字面量：静态估不出上界，只有运行期能核。修法：写成整数字面量"
            ),
            bound.span,
        )),
    }
}
