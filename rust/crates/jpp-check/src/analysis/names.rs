//! 名字趟：作用域、名字解析与读数推断（E-name / E-field / E-arity / E-type / W-shadow 是分析器
//! 自身的报错）。读数被用错的位置与内置调用交给规则钩子（J-01、J-05，见 `rules/`）。

#![allow(unused_imports)]
use crate::*;

impl Checker<'_> {
    pub(crate) fn names_and_readings(&mut self, p: &Program) {
        let mut scopes = vec![];
        // 宿主入口参数（B106：读 `Program.entry`，由 `Session::compile` 写入）：最外层之外的一层，
        // 已定义、类别 Other。依据：B106（地基/附注/2026-09-25-B105-B106裁定.md §三）
        if !p.entry.is_empty() {
            let mut host = Scope::new();
            for e in &p.entry.params {
                host.declared.insert(e.name.clone(), Kind::Other);
                host.defined.insert(e.name.clone());
            }
            scopes.push(host);
        }
        self.scan_block(&p.body, &mut scopes, 0);
    }

    /// `fn_depth > 0` 表示正在某个函数体内：该位置能看见所属块的全部绑定。
    pub(crate) fn scan_block(&mut self, b: &Block, scopes: &mut Vec<Scope>, fn_depth: usize) {
        let mut scope = Scope::new();
        for s in &b.statements {
            match s {
                Statement::Let {
                    name,
                    span,
                    value,
                    annotation,
                } => {
                    scope.declared.insert(name.clone(), Kind::Other);
                    if let Some(t) = annotation {
                        scope.annotations.insert(name.clone(), t.clone());
                    }
                    if let ExprKind::Function(f) = value.kind() {
                        scope.signatures.insert(
                            name.clone(),
                            f.parameters.iter().map(|p| p.annotation.clone()).collect(),
                        );
                    }
                    self.shadow(name, *span);
                }
                Statement::Function {
                    name,
                    span,
                    function,
                } => {
                    scope.declared.insert(name.clone(), Kind::Other);
                    scope.signatures.insert(
                        name.clone(),
                        function
                            .parameters
                            .iter()
                            .map(|p| p.annotation.clone())
                            .collect(),
                    );
                    self.shadow(name, *span);
                }
                Statement::Expr(_) => {}
            }
        }
        scopes.push(scope);
        for s in &b.statements {
            match s {
                Statement::Let { name, value, .. } => {
                    let k = self.scan_expr(value, scopes, fn_depth);
                    let top = scopes.last_mut().unwrap();
                    top.declared.insert(name.clone(), k);
                    top.defined.insert(name.clone());
                }
                Statement::Function { name, function, .. } => {
                    self.scan_function(function, scopes, fn_depth);
                    scopes.last_mut().unwrap().defined.insert(name.clone());
                }
                Statement::Expr(e) => {
                    self.scan_expr(e, scopes, fn_depth);
                }
            }
        }
        if let Some(r) = &b.result {
            self.scan_expr(r, scopes, fn_depth);
        }
        scopes.pop();
    }

    pub(crate) fn scan_function(&mut self, f: &Function, scopes: &mut Vec<Scope>, fn_depth: usize) {
        let mut scope = Scope::new();
        for p in &f.parameters {
            scope.declared.insert(
                p.name.clone(),
                p.annotation
                    .as_ref()
                    .map(kind_of_annotation)
                    .unwrap_or(Kind::Other),
            );
            scope.defined.insert(p.name.clone());
            if let Some(t) = &p.annotation {
                scope.annotations.insert(p.name.clone(), t.clone());
            }
            self.shadow(&p.name, p.span);
        }
        scopes.push(scope);
        self.scan_block(&f.body, scopes, fn_depth + 1);
        scopes.pop();
    }

    pub(crate) fn shadow(&mut self, name: &str, span: Span) {
        if is_builtin(name) {
            self.out.push(Diagnostic::warning(
                "W-shadow",
                format!("名字 {name} 盖住了同名内置操作：这个块里 {name}(…) 调的是你的定义，内置版本取不到了。修法：换个名字"),
                span,
            ));
        }
    }

    /// 名字在当前位置的类别；None = 没有这个绑定
    pub(crate) fn lookup(&self, scopes: &[Scope], name: &str) -> Option<Kind> {
        scopes
            .iter()
            .rev()
            .find_map(|s| s.declared.get(name).copied())
    }

    pub(crate) fn defined_yet(&self, scopes: &[Scope], name: &str, fn_depth: usize) -> bool {
        for s in scopes.iter().rev() {
            if s.declared.contains_key(name) {
                return fn_depth > 0 || s.defined.contains(name);
            }
        }
        false
    }

    /// 这个名字在此处绑定到的具名方法的参数表（被更近的绑定盖住时取不到）
    pub(crate) fn signature(&self, scopes: &[Scope], name: &str) -> Option<Vec<Option<Type>>> {
        for s in scopes.iter().rev() {
            if s.declared.contains_key(name) {
                return s.signatures.get(name).cloned();
            }
        }
        None
    }

    /// 参数个数不对（E-arity）、字面量与标注不符（E-type）。两者都只在静态确定时才报。
    pub(crate) fn named_call(
        &mut self,
        name: &str,
        sig: &[Option<Type>],
        args: &[&Expr],
        span: Span,
    ) {
        if args.len() != sig.len() {
            self.out.push(Diagnostic::error(
                "E-arity",
                format!("{name} 要 {} 个参数，这里给了 {}", sig.len(), args.len()),
                span,
            ));
            return;
        }
        for (annotation, arg) in sig.iter().zip(args) {
            let (Some(t), Some(got)) = (annotation.as_ref(), Shape::of_literal(arg)) else {
                continue;
            };
            let Some(want) = Shape::of_type(t) else {
                continue;
            };
            if !Shape::fits(got, want) {
                self.out.push(Diagnostic::error(
                    "E-type",
                    format!(
                        "{name} 的这个参数标的是 {}，这里给的是 {} 字面量",
                        want.name(),
                        got.name()
                    ),
                    arg.span,
                ));
            }
        }
    }

    /// 这个名字在此处是否解析成内置（没有被用户绑定盖住）
    pub(crate) fn resolves_to_builtin(&self, scopes: &[Scope], name: &str) -> bool {
        is_builtin(name) && self.lookup(scopes, name).is_none()
    }

    pub(crate) fn scan_expr(&mut self, e: &Expr, scopes: &mut Vec<Scope>, fn_depth: usize) -> Kind {
        let kind = self.scan_expr_inner(e, scopes, fn_depth);
        if kind == Kind::Reading {
            self.readings.insert(e as *const Expr);
        }
        kind
    }

    pub(crate) fn scan_expr_inner(
        &mut self,
        e: &Expr,
        scopes: &mut Vec<Scope>,
        fn_depth: usize,
    ) -> Kind {
        match e.kind() {
            ExprKind::Name(n) => {
                if self.resolves_to_builtin(scopes, n) {
                    return Kind::Other;
                }
                match self.lookup(scopes, n) {
                    Some(k) => {
                        if !self.defined_yet(scopes, n, fn_depth) {
                            self.out.push(Diagnostic::error(
                                "E-name",
                                format!(
                                    "名字 {n} 在定义之前被用。修法：把 {n} 的定义移到这一行之前"
                                ),
                                e.span,
                            ));
                        }
                        k
                    }
                    None => {
                        self.out.push(Diagnostic::error(
                            "E-name",
                            format!("未定义的名字 {n}：既不是这个块里的绑定，也不是内置操作"),
                            e.span,
                        ));
                        Kind::Other
                    }
                }
            }
            ExprKind::List(items) => {
                for x in items {
                    self.scan_expr(x, scopes, fn_depth);
                }
                Kind::Other
            }
            ExprKind::Record(fields) => {
                for (_, x) in fields {
                    self.scan_expr(x, scopes, fn_depth);
                }
                Kind::Other
            }
            ExprKind::Function(f) => {
                self.scan_function(f, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::Block(b) => {
                self.scan_block(b, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::If { condition, yes, no } => {
                if self.scan_expr(condition, scopes, fn_depth) == Kind::Reading {
                    self.on_reading(condition.span, "读数不能当 if 的条件", false);
                }
                self.scan_block(yes, scopes, fn_depth);
                self.scan_block(no, scopes, fn_depth);
                Kind::Other
            }
            ExprKind::Field { value, field } => {
                match self.scan_expr(value, scopes, fn_depth) {
                    Kind::Reading => self.on_reading(value.span, format!("读数没有字段 {field} 可读"), false),
                    Kind::Question if !crate::shapes::QUESTION_FIELDS.contains(&field.as_str()) => self.out.push(Diagnostic::error(
                        "E-field",
                        format!("题没有字段 {field}。修法：改成题的可读字段之一：{}", crate::shapes::QUESTION_FIELDS.join("、")),
                        e.span,
                    )),
                    Kind::Form if !crate::shapes::FORM_FIELDS.contains(&field.as_str()) => self.out.push(Diagnostic::error(
                        "E-field",
                        format!("题式没有字段 {field}。修法：改成题式的可读字段之一：{}", crate::shapes::FORM_FIELDS.join("、")),
                        e.span,
                    )),
                    Kind::Outcome if !crate::shapes::OUTCOME_FIELDS.contains(&field.as_str()) => self.out.push(Diagnostic::error(
                        "E-field",
                        format!("契约值没有字段 {field}。修法：改成契约的字段之一：{}（sieve 的 ignore 与题在 detail 里；未观察项是 pending 里 cause=budget 的项）", crate::shapes::OUTCOME_FIELDS.join("、")),
                        e.span,
                    )),
                    _ => {}
                }
                Kind::Other
            }
            ExprKind::Index { value, index } => {
                let k = self.scan_expr(value, scopes, fn_depth);
                self.scan_expr(index, scopes, fn_depth);
                // judge(state, [题…]) 给的是读数列表，取下标仍是读数
                k
            }
            ExprKind::Unary { op, value } => {
                if self.scan_expr(value, scopes, fn_depth) == Kind::Reading {
                    self.on_reading(value.span, format!("读数不能做一元 {op}"), false);
                }
                Kind::Other
            }
            ExprKind::Binary { op, left, right } => {
                let a = self.scan_expr(left, scopes, fn_depth);
                let b = self.scan_expr(right, scopes, fn_depth);
                // **只有算术那一面归 H5 管。** 比较是 I3（跨题读数不成恒等式）的事，
                // `12`:328 J-01 的依赖假设栏写的是「H5（为假时**「算术在宿主」**降 warn，桥不变）」
                // ——降的是算术，不是比较。听起来像同一类，要保证的东西不同。
                let 算术 = matches!(op.as_str(), "+" | "-" | "*" | "/" | "%");
                if a == Kind::Reading {
                    self.on_reading(
                        left.span,
                        format!("读数不能做 {op}：读数不可比、不可算"),
                        算术,
                    );
                }
                if b == Kind::Reading {
                    self.on_reading(
                        right.span,
                        format!("读数不能做 {op}：读数不可比、不可算"),
                        算术,
                    );
                }
                Kind::Other
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                if let Some(fe) = function.expr() {
                    self.scan_expr(fe, scopes, fn_depth);
                }
                for a in &arguments {
                    self.scan_expr(a, scopes, fn_depth);
                }
                let builtin = match function.kind() {
                    ExprKind::Name(n) if self.resolves_to_builtin(scopes, n) => Some(n.to_string()),
                    _ => None,
                };
                if let ExprKind::Name(n) = function.kind() {
                    if let Some(sig) = self.signature(scopes, n) {
                        self.named_call(n, &sig, &arguments, e.span);
                    }
                }
                match builtin {
                    Some(n) => {
                        self.on_scan_call(scopes, &n, &arguments);
                        // 效应按 `EffectSpec.output_shape` 定类别（步 15a）：读数 → 读数；人的回答 → 出口
                        let effect_kind =
                            jpp_effects::by_name(&n).and_then(|s| match s.output_shape {
                                jpp_effects::OutputShape::Readings => Some(Kind::Reading),
                                jpp_effects::OutputShape::Answer => Some(Kind::Exit),
                                _ => None,
                            });
                        match n.as_str() {
                            _ if effect_kind.is_some() => effect_kind.expect("刚判过"),
                            "cut" | "unsure" => Kind::Exit,
                            "test" | "select" | "measure" | "fill" => Kind::Question,
                            "form" => Kind::Form,
                            "pair" | "tally" | "first_k" | "iterate" | "outcome" => Kind::Outcome,
                            // 单道题返回一个契约值；题列表、题式 + 填法返回契约值的列表
                            "sieve"
                                if arguments.len() == 2
                                    && !matches!(arguments[1].kind(), ExprKind::List(_)) =>
                            {
                                Kind::Outcome
                            }
                            _ => Kind::Other,
                        }
                    }
                    None => Kind::Other,
                }
            }
            _ => Kind::Other,
        }
    }
}
