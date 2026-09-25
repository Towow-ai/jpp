//! `W-legacy-fn-type`（K-052，步 24h；`14-实施计划-把语言做完整-v1.md` §十二第 7 条）：形参标注为
//! 旧式 `Type::Function`（无效应行，`Fn(A, B) -> C` 不带 `-!{...}->`）时，若该名字在函数体内被
//! 当作方法调用（`name(...)`），报警——旧式类型不携带效应契约，调用点看不出这个方法会不会触发
//! 判断/动作，与新式 `Type::Method`（`Fn(A) -!{judge}-> B`，含效应行）形成对照。
//!
//! 递归进函数体内的嵌套函数字面量（闭包捕获外层形参、在体内调用是同一个问题，不能因为写成
//! 「返回一个闭包」就漏报）；形参名被 `let`/嵌套函数同名参数重绑后停止（遮蔽后已不是那个形参，
//! 同项目「宁可漏报」方向）。

#![allow(unused_imports)]
use std::collections::HashSet;

use super::{Cx, Hooks, Rule};
use crate::*;
use jpp_ir::ir::Type;

/// 依据：14-实施计划-把语言做完整-v1.md §十二第 7 条（K-052）；步 24h。
pub(crate) const RULE: Rule = Rule {
    code: "W-legacy-fn-type",
    requires: &[],
    hooks: Hooks {
        after: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    let mut out = vec![];
    scan_block(&cx.p.body, &mut out);
    out
}

fn scan_block(b: &Block, out: &mut Vec<Diagnostic>) {
    for s in &b.statements {
        match s {
            Statement::Let { value, .. } => scan_expr(value, out),
            Statement::Function { function, .. } => {
                check_function(function, out);
                scan_block(&function.body, out);
            }
            Statement::Expr(e) => scan_expr(e, out),
        }
    }
    if let Some(r) = &b.result {
        scan_expr(r, out);
    }
}

fn scan_expr(e: &Expr, out: &mut Vec<Diagnostic>) {
    if let ExprKind::Function(f) = e.kind() {
        check_function(f, out);
    }
    e.children().into_iter().for_each(|x| scan_expr(x, out));
    for b in e.blocks() {
        scan_block(b, out);
    }
}

/// 一个函数：先收「旧式 `Fn` 类型」的形参名，体内没有这样的形参直接跳过；否则在体内找调用点。
fn check_function(f: &Function, out: &mut Vec<Diagnostic>) {
    let legacy: Vec<&str> = f
        .parameters
        .iter()
        .filter(|p| matches!(&p.annotation, Some(Type::Function(_, _))))
        .map(|p| p.name.as_str())
        .collect();
    if legacy.is_empty() {
        return;
    }
    for name in legacy {
        find_calls(&f.body, name, out, f);
    }
}

/// 在块内找 `name(...)` 调用点；`name` 被 `let`/嵌套函数形参重绑后停止往下找（遮蔽）。
fn find_calls(b: &Block, name: &str, out: &mut Vec<Diagnostic>, owner: &Function) {
    for s in &b.statements {
        match s {
            Statement::Let { name: n, value, .. } => {
                find_calls_expr(value, name, out, owner);
                if n == name {
                    return; // 遮蔽：这个块里往后 `name` 已经是别的东西
                }
            }
            Statement::Function { function, .. } => {
                if function.parameters.iter().any(|p| p.name == name) {
                    continue; // 嵌套函数的同名形参遮蔽外层
                }
                find_calls(&function.body, name, out, owner);
            }
            Statement::Expr(e) => find_calls_expr(e, name, out, owner),
        }
    }
    if let Some(r) = &b.result {
        find_calls_expr(r, name, out, owner);
    }
}

fn find_calls_expr(e: &Expr, name: &str, out: &mut Vec<Diagnostic>, owner: &Function) {
    if call_name(e) == Some(name) {
        // 依据：14-实施计划-把语言做完整-v1.md §十二第 7 条（K-052）；步 24h
        out.push(Diagnostic::warning(
            "W-legacy-fn-type",
            format!(
                "参数 {name} 标注为旧式 `Fn(...) -> ...`（不带效应行），在函数体内被当作方法调用：旧式类型不携带效应契约，调用点看不出这个方法会不会触发判断/动作。修法：给参数标注加效应行，例如 `Fn(...) -!{{judge}}-> ...`（不确定效应时留空 `-!{{}}-> ` 声明纯函数）"
            ),
            e.span,
        ));
    }
    if let ExprKind::Function(f) = e.kind() {
        if !f.parameters.iter().any(|p| p.name == name) {
            find_calls(&f.body, name, out, owner);
        }
        return; // 已经递归过嵌套函数体，不再走通用 children 路径重复递归
    }
    e.children()
        .into_iter()
        .for_each(|x| find_calls_expr(x, name, out, owner));
    for b in e.blocks() {
        // `blocks()` 只给 If 的两支与 Block 表达式（Function 体已在上面单独处理）
        find_calls(b, name, out, owner);
    }
}
