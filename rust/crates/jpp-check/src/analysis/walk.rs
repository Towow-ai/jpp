//! 遍历与调用识别的助手（原 `check.rs` 中段的自由函数，只搬不改）。

#![allow(unused_imports)]
use crate::*;

/// 结果里**裸着出现**的名字：被任何调用吃进去的不算（`content(x)` / `is_fail(x)` / `len(x)` …）。
/// 那些调用各自会处理 Fail，或者当场报错；这条只找「原样交出去」的。
pub(crate) fn collect_bare_names(e: &Expr, out: &mut HashSet<String>) {
    match e.kind() {
        ExprKind::Name(n) => {
            out.insert(n.to_string());
        }
        ExprKind::List(items) => items.iter().for_each(|x| collect_bare_names(x, out)),
        ExprKind::Record(fs) => fs.iter().for_each(|(_, x)| collect_bare_names(x, out)),
        ExprKind::Block(b) => {
            if let Some(r) = &b.result {
                collect_bare_names(r, out);
            }
        }
        ExprKind::If { yes, no, .. } => {
            for b in [yes, no] {
                if let Some(r) = &b.result {
                    collect_bare_names(r, out);
                }
            }
        }
        // 调用、取字段、下标、算术都会「用」这个值，Fail 在那里各有各的处置，不是原样交出去
        _ => {}
    }
}

/// 不在效应表效应行名字（`effect_names`）里的名字
pub(crate) fn unknown_effects(names: &[String]) -> Vec<String> {
    let known = effect_names();
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| !known.contains(&n.as_str()))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn unknown_effect_diag(what: &str, unknown: &[String], span: Span) -> Diagnostic {
    Diagnostic::error(
        "E-effect-name",
        format!(
            "{what}里有认不得的效应名：{}。效应形式只有 {} 这四种（`transform` 是记账变换不是效应形式，不写进标注）。修法：改成这四个之一；想表达「效应随传进来的方法而定」不用写名字——缺省标注就是推断，调用点会实例化",
            unknown.join(", "),
            effect_names().join(" / ")
        ),
        span,
    )
}

/// 类型里所有方法效应行
pub(crate) fn collect_method_rows(t: &Type, out: &mut Vec<Vec<String>>) {
    match t {
        Type::Method(m) => {
            if let Some(e) = &m.effects {
                out.push(e.clone());
            }
            m.params.iter().for_each(|p| collect_method_rows(p, out));
            collect_method_rows(&m.ret, out);
        }
        Type::Function(ps, r) => {
            ps.iter().for_each(|p| collect_method_rows(p, out));
            collect_method_rows(r, out);
        }
        Type::Applied(_, args) => args.iter().for_each(|a| collect_method_rows(a, out)),
        Type::Named(_) => {}
    }
}

pub(crate) fn is_builtin(n: &str) -> bool {
    BUILTINS.contains(&n)
}

/// 这个内置调用会发生哪种效应
///
/// 效应本身读注册表（步 15a，`20` A2）：进效应行的效应（`EffectSpec.in_effect_row`）即它自己。
/// 另有三个构造与消费形式隐含效应：`sieve`、`literalize` 发判断，`escalate` 发问人；它们按
/// 所隐含效应的字段（产出读数 / 输出是人的回答）从注册表取名，不在这里写效应名。
pub(crate) fn builtin_effect(n: &str) -> Option<&'static str> {
    use jpp_effects::{ALL, OutputShape, spec};
    if let Some(s) = jpp_effects::by_name(n) {
        return s.in_effect_row.then_some(s.name);
    }
    let implied = |f: fn(&jpp_effects::EffectSpec) -> bool| {
        ALL.into_iter().map(spec).find(|s| f(s)).map(|s| s.name)
    };
    match n {
        "literalize" | "sieve" => implied(|s| s.produces_reading),
        "escalate" => implied(|s| s.output_shape == OutputShape::Answer),
        _ => None,
    }
}

/// 高阶内置里哪一位收方法
///
/// 效应的方法位读 `EffectSpec.input_schema` 里种类为 `Fn` 的槽（步 15a，`20` A2）；其余是构造与宿主内置。
pub(crate) fn method_positions(builtin: &str) -> Vec<usize> {
    if let Some(s) = jpp_effects::by_name(builtin) {
        return s
            .input_schema
            .iter()
            .enumerate()
            .filter(|(_, d)| d.kind == jpp_effects::SlotKind::Fn)
            .map(|(i, _)| i)
            .collect();
    }
    match builtin {
        "map" | "filter" => vec![1],
        "fold" | "loop" => vec![2],
        "iterate" => vec![2, 3],
        "pair" => vec![2],
        _ => vec![],
    }
}

pub(crate) fn call_name(e: &Expr) -> Option<&str> {
    match e.kind() {
        ExprKind::Call { function, .. } => function.name(),
        _ => None,
    }
}

pub(crate) fn call_args(e: &Expr) -> Vec<&Expr> {
    match e.kind() {
        ExprKind::Call { arguments, .. } => arguments,
        _ => vec![],
    }
}

pub(crate) fn mentions(e: &Expr, name: &str) -> bool {
    let mut found = false;
    walk_expr(e, &mut |x| {
        if let ExprKind::Name(n) = x.kind() {
            if n == name {
                found = true;
            }
        }
    });
    found
}

pub(crate) fn walk_expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match e.kind() {
        ExprKind::List(items) => items.iter().for_each(|x| walk_expr(x, f)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| walk_expr(x, f)),
        ExprKind::Function(func) => walk_block(&func.body, f),
        ExprKind::Call {
            function,
            arguments,
        } => {
            if let Some(callee) = function.expr() {
                walk_expr(callee, f);
            }
            arguments.iter().for_each(|x| walk_expr(x, f));
        }
        ExprKind::Field { value, .. } => walk_expr(value, f),
        ExprKind::Index { value, index } => {
            walk_expr(value, f);
            walk_expr(index, f);
        }
        ExprKind::Unary { value, .. } => walk_expr(value, f),
        ExprKind::Binary { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        ExprKind::If { condition, yes, no } => {
            walk_expr(condition, f);
            walk_block(yes, f);
            walk_block(no, f);
        }
        ExprKind::Block(b) => walk_block(b, f),
        _ => {}
    }
}

pub(crate) fn walk_block(b: &Block, f: &mut impl FnMut(&Expr)) {
    for s in &b.statements {
        match s {
            Statement::Let { value, .. } => walk_expr(value, f),
            Statement::Function { function, .. } => walk_block(&function.body, f),
            Statement::Expr(e) => walk_expr(e, f),
        }
    }
    if let Some(r) = &b.result {
        walk_expr(r, f);
    }
}

/// 收集所有 `let 名 = 值` 绑定（含嵌套块与函数体）。同名的后写覆盖先写——
/// 不分作用域，最多把两处同名当成一处多算倍数，往保守那边偏。
pub(crate) fn 收let绑定(b: &Block, out: &mut std::collections::HashMap<String, Expr>) {
    let mut 子块: Vec<&Block> = vec![];
    for s in &b.statements {
        match s {
            Statement::Let { name, value, .. } => {
                out.insert(name.clone(), value.clone());
                收子块(value, &mut 子块);
            }
            Statement::Function { function, .. } => 子块.push(&function.body),
            Statement::Expr(e) => 收子块(e, &mut 子块),
        }
    }
    if let Some(r) = &b.result {
        收子块(r, &mut 子块);
    }
    for x in 子块 {
        收let绑定(x, out);
    }
}

/// 表达式里**最外一层**的子块（更深的由 `收let绑定` 递归时再收）。
pub(crate) fn 收子块<'a>(e: &'a Expr, out: &mut Vec<&'a Block>) {
    match e.kind() {
        ExprKind::Block(b) => out.push(b),
        ExprKind::Function(f) => out.push(&f.body),
        ExprKind::If { condition, yes, no } => {
            收子块(condition, out);
            out.push(yes);
            out.push(no);
        }
        ExprKind::List(items) => items.iter().for_each(|x| 收子块(x, out)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| 收子块(x, out)),
        ExprKind::Call {
            function,
            arguments,
        } => {
            if let Some(callee) = function.expr() {
                收子块(callee, out);
            }
            arguments.iter().for_each(|x| 收子块(x, out));
        }
        ExprKind::Field { value, .. } | ExprKind::Unary { value, .. } => 收子块(value, out),
        ExprKind::Index { value, index } => {
            收子块(value, out);
            收子块(index, out);
        }
        ExprKind::Binary { left, right, .. } => {
            收子块(left, out);
            收子块(right, out);
        }
        _ => {}
    }
}

/// 预算声明所在的位置（`Program` 不单独记 budget 的 span，用整个程序的 span）
pub(crate) fn b_span(p: &Program) -> jpp_ir::ir::Span {
    p.span
}
