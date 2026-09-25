//! 效应行推断与标注核对（E-effect / W-effect / E-effect-name）。效应行类型在 `rows.rs`。

#![allow(unused_imports)]
use crate::*;

impl Checker<'_> {
    pub(crate) fn effects(&mut self, p: &Program) {
        self.collect_functions(&p.body);
        // 不动点：concrete 只增、vars 只增、opaque 只从 false 到 true，单调，必然停
        for _ in 0..=self.functions.len() {
            let mut changed = false;
            for i in 0..self.functions.len() {
                let body = self.functions[i].body.clone();
                let owner = self.owner_of(i);
                let known = self.annotated_methods(&self.functions[i]);
                let row = self.row_of_block(&body, &owner, &known);
                let returns = self.method_value_row(body.result.as_deref(), &owner, &known);
                let fields = self.method_fields(body.result.as_deref(), &owner, &known);
                if row != self.functions[i].row
                    || returns != self.functions[i].returns
                    || fields != self.functions[i].fields
                {
                    self.functions[i].row = row;
                    self.functions[i].returns = returns;
                    self.functions[i].fields = fields;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.effect_names(p);
        let inst = self.instantiate_all(p);
        self.declared_vs_row(&inst);
        self.param_contracts(p);
    }

    /// 效应名本身要认得。`!{…}` 与类型位上的效应行只认效应表里 `in_effect_row` 为真的那四个（`effect_names`）。
    ///
    /// 不校验的后果不是「少报一条」，是**报错方向反了**：前端的词法按 Unicode 取标识符，
    /// 所以 `!{ε}` 会被当成一个叫 ε 的具体效应解析成功，再报一条「少了 judge」——
    /// 作者以为是自己漏标了效应，其实是语言还没有效应变量这个东西。
    pub(crate) fn effect_names(&mut self, p: &Program) {
        let mut bad: Vec<(usize, Vec<String>)> = vec![];
        for (i, f) in self.functions.iter().enumerate() {
            if let Some(d) = &f.declared {
                let u = unknown_effects(d);
                if !u.is_empty() {
                    bad.push((i, u));
                }
            }
        }
        for (i, u) in bad {
            let (name, span) = (self.functions[i].name.clone(), self.functions[i].span);
            self.out.push(unknown_effect_diag(
                &format!("{name} 的 !{{…}} 标注"),
                &u,
                span,
            ));
            // 名字都不认识，这条标注就没法拿来做差集——别再报「少了 judge」那种指向错误方向的诊断
            self.bad_effect_decl.insert(i);
        }
        // 类型位上的效应行（`Fn(A) -!{…}-> B`）同样校验
        let mut found: Vec<(String, Vec<String>, Span)> = vec![];
        let mut note = |what: String, t: &Type, span: Span| {
            let mut rows = vec![];
            collect_method_rows(t, &mut rows);
            for r in rows {
                let u = unknown_effects(&r);
                if !u.is_empty() {
                    found.push((what.clone(), u, span));
                }
            }
        };
        for f in &self.functions {
            for param in &f.params {
                if let Some(t) = &param.annotation {
                    note(format!("参数 {} 的类型", param.name), t, param.span);
                }
            }
        }
        // let 绑定上的类型标注同样校验
        fn lets(b: &Block, out: &mut Vec<(String, Type, Span)>) {
            for st in &b.statements {
                match st {
                    Statement::Let {
                        name,
                        annotation: Some(t),
                        span,
                        ..
                    } => out.push((format!("绑定 {name} 的类型"), t.clone(), *span)),
                    Statement::Function { function, .. } => lets(&function.body, out),
                    _ => {}
                }
            }
        }
        let mut annotated: Vec<(String, Type, Span)> = vec![];
        lets(&p.body, &mut annotated);
        for (what, t, span) in &annotated {
            note(what.clone(), t, *span);
        }
        for (what, u, span) in found {
            self.out.push(unknown_effect_diag(&what, &u, span));
        }
    }

    /// 参数**类型位**上的效应行是契约：`f: Fn(A) -!{judge}-> B` 说的是「这个位置只收效应不超过
    /// judge 的方法」。实参超出就报在**调用点**——诊断该指着传错东西的地方，不是函数定义。
    ///
    /// 这一条是效应行跟着**类型**走才有的能力：不靠推断、不靠调用点实例化，标了就当场有约束。
    pub(crate) fn param_contracts(&mut self, p: &Program) {
        let mut found: Vec<Diagnostic> = vec![];
        walk_block(&p.body, &mut |e| {
            let ExprKind::Call {
                function,
                arguments,
            } = e.kind()
            else {
                return;
            };
            let ExprKind::Name(callee) = function.kind() else {
                return;
            };
            let Some(ids) = self.by_name.get(callee) else {
                return;
            };
            if ids.len() != 1 {
                return;
            }
            for (k, param) in self.functions[ids[0]].params.iter().enumerate() {
                let Some(t) = &param.annotation else { continue };
                let Some((Some(row), _)) = t.as_method() else {
                    continue;
                };
                let allowed: BTreeSet<String> = row.iter().cloned().collect();
                let Some(arg) = arguments.get(k) else {
                    continue;
                };
                let Some((who, actual)) = self.arg_effects(arg) else {
                    continue;
                };
                let missing: Vec<String> = actual.difference(&allowed).cloned().collect();
                if missing.is_empty() {
                    continue;
                }
                let all: Vec<String> = allowed.union(&actual).cloned().collect();
                found.push(Diagnostic::error(
                    "E-effect",
                    format!(
                        "{callee} 的第 {} 个参数 {} 标成只收 !{{{}}} 的方法，这次传的 {who} 会 {}：类型上的效应行是契约，实参必须在它之内。修法：把参数类型标成 -!{{{}}}->，或换一个不带这些效应的方法",
                        k + 1,
                        param.name,
                        allowed.iter().cloned().collect::<Vec<_>>().join(", "),
                        missing.join(", "),
                        all.join(", ")
                    ),
                    arg.span,
                ));
            }
        });
        self.out.extend(found);
    }

    /// 实参静态认得出的效应集：具名方法或带 `!{…}` 的字面方法才算，其余不判（宁可漏报也不误报）
    pub(crate) fn arg_effects(&self, a: &Expr) -> Option<(String, BTreeSet<String>)> {
        match a.kind() {
            ExprKind::Name(g) => {
                let ids = self.by_name.get(g)?;
                if ids.len() != 1 {
                    return None;
                }
                let f = &self.functions[ids[0]];
                let row = match &f.declared {
                    Some(d) => d.iter().cloned().collect(),
                    None if !f.row.open() => f.row.concrete.clone(),
                    None => return None,
                };
                Some((g.to_string(), row))
            }
            ExprKind::Function(f) => f
                .effects
                .as_ref()
                .map(|d| ("这个字面方法".to_string(), d.iter().cloned().collect())),
            _ => None,
        }
    }

    pub(crate) fn owner_of(&self, i: usize) -> Owner {
        Owner {
            id: i,
            params: self.functions[i]
                .params
                .iter()
                .enumerate()
                .map(|(k, p)| (p.name.clone(), k))
                .collect(),
        }
    }

    /// 纪律 4：扫全程序的调用点，把各处实参实例化出来的效应并起来，供核 `!{…}` 上界用。
    /// 注意这**不是**函数自己的行——行按调用点传播，这里只服务于「标注必须盖住所有用法」。
    pub(crate) fn instantiate_all(&self, p: &Program) -> Vec<Instantiated> {
        let mut out: Vec<Instantiated> = self
            .functions
            .iter()
            .map(|_| Instantiated::default())
            .collect();
        let mut visit = |e: &Expr| {
            let ExprKind::Call {
                function,
                arguments,
            } = e.kind()
            else {
                return;
            };
            let ExprKind::Name(callee) = function.kind() else {
                return;
            };
            let Some(ids) = self.by_name.get(callee) else {
                return;
            };
            if ids.len() != 1 {
                return;
            }
            let id = ids[0];
            if self.functions[id].row.vars.is_empty() {
                return;
            }
            out[id].called = true;
            let empty = Owner::default();
            let known = HashMap::new();
            for (o, k) in &self.functions[id].row.vars {
                if *o != id {
                    out[id].incomplete = true;
                    continue;
                }
                match arguments.get(*k) {
                    Some(a) => {
                        let row = self.invoked(a, &empty, &known);
                        out[id].effects.extend(row.concrete.iter().cloned());
                        // 实参本身还带变量或认不出：这个调用点没给出完整信息
                        out[id].incomplete |= row.open();
                    }
                    None => out[id].incomplete = true,
                }
            }
        };
        walk_block(&p.body, &mut visit);
        // 带效应变量却从没被调用过：一个变量都没定死，反方向的提示不能发
        for (i, f) in self.functions.iter().enumerate() {
            if !f.row.vars.is_empty() && !out[i].called {
                out[i].incomplete = true;
            }
        }
        out
    }

    /// 定义处核标注：确定的那一半 + 各调用点实例化出来的那一半，都必须被 `!{…}` 盖住
    pub(crate) fn declared_vs_row(&mut self, inst: &[Instantiated]) {
        for i in 0..self.functions.len() {
            let Some(declared) = self.functions[i].declared.clone() else {
                continue;
            };
            if self.bad_effect_decl.contains(&i) {
                continue;
            }
            let declared: BTreeSet<String> = declared.into_iter().collect();
            let row = self.functions[i].row.clone();
            let name = self.functions[i].name.clone();
            let span = self.functions[i].span;

            let mut actual = row.concrete.clone();
            actual.extend(inst[i].effects.iter().cloned());

            let missing: Vec<String> = actual.difference(&declared).cloned().collect();
            if !missing.is_empty() {
                let all: Vec<String> = declared.union(&actual).cloned().collect();
                self.out.push(Diagnostic::error(
                    "E-effect",
                    format!(
                        "{name} 的效应标注少了 {}：函数体里这些效应实际会发生，标注是调用者估预算的依据。修法：写成 !{{{}}}",
                        missing.join(", "),
                        all.join(", ")
                    ),
                    span,
                ));
            }
            // 反方向要完整信息：体内有认不出的被调者、或效应变量没被所有调用点定死，就不提示
            if row.opaque || inst[i].incomplete {
                continue;
            }
            let unused: Vec<String> = declared.difference(&actual).cloned().collect();
            if !unused.is_empty() {
                self.out.push(Diagnostic::warning(
                    "W-effect",
                    format!(
                        "{name} 标了 {} 但函数体里看不到：多标不出错，只是预算会估高",
                        unused.join(", ")
                    ),
                    span,
                ));
            }
        }
    }

    pub(crate) fn collect_functions(&mut self, b: &Block) {
        for s in &b.statements {
            match s {
                Statement::Function {
                    name,
                    function,
                    span,
                } => {
                    self.push_function(name.clone(), function, *span);
                    self.collect_functions(&function.body);
                }
                Statement::Let {
                    name, value, span, ..
                } => {
                    if let ExprKind::Function(f) = value.kind() {
                        self.push_function(name.clone(), f, *span);
                    }
                    self.collect_anon(value);
                }
                Statement::Expr(e) => self.collect_anon(e),
            }
        }
        if let Some(r) = &b.result {
            self.collect_anon(r);
        }
    }

    pub(crate) fn push_function(&mut self, name: String, f: &Function, span: Span) {
        let id = self.functions.len();
        self.functions.push(FuncInfo {
            name: name.clone(),
            node: f.id,
            params: f.parameters.clone(),
            declared: f.effects.clone(),
            row: Row::default(),
            returns: None,
            fields: HashMap::new(),
            span,
            body: f.body.clone(),
        });
        self.by_name.entry(name).or_default().push(id);
    }

    /// 带效应标注的匿名方法也要核（`map(xs, fn(x) !{do} { … })`）
    pub(crate) fn collect_anon(&mut self, e: &Expr) {
        let mut found: Vec<(Function, Span)> = vec![];
        walk_expr(e, &mut |x| {
            if let ExprKind::Function(f) = x.kind() {
                if f.effects.is_some() {
                    found.push((f.clone(), x.span));
                }
            }
        });
        for (f, span) in found {
            self.functions.push(FuncInfo {
                name: "匿名方法".into(),
                node: f.id,
                params: f.parameters.clone(),
                declared: f.effects.clone(),
                row: Row::default(),
                returns: None,
                fields: HashMap::new(),
                span,
                body: f.body.clone(),
            });
        }
    }

    /// 函数体里带方法类型标注的名字：`Type::Method` 的效应行跟着**类型**走，
    /// 所以方法经参数、记录字段、返回值传递时这一条契约不丢。
    pub(crate) fn annotated_methods(&self, f: &FuncInfo) -> Known {
        let mut out = HashMap::new();
        let mut note = |p: &Parameter| {
            if let Some(t) = &p.annotation {
                if let Some((effects, _)) = t.as_method() {
                    match effects {
                        Some(e) => {
                            out.insert(p.name.clone(), Some(e.to_vec()));
                        }
                        None => {
                            out.entry(p.name.clone()).or_insert(None);
                        }
                    }
                }
            }
        };
        for p in &f.params {
            note(p);
        }
        walk_block(&f.body, &mut |e| {
            if let ExprKind::Function(inner) = e.kind() {
                for p in &inner.parameters {
                    note(p);
                }
            }
        });
        out
    }

    // --------- 推断本体

    pub(crate) fn row_of_block(&self, b: &Block, owner: &Owner, known: &Known) -> Row {
        // 块内重新绑定的名字盖住同名的形参：这之后调的是新绑定，不是那个参数。整块都当成被盖住
        // （保守方向：名字落回已知函数表，查不准退成未知，不会把实参的行算到本函数头上）
        let shadowed: Vec<&str> = b
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Let { name, .. } | Statement::Function { name, .. } => {
                    Some(name.as_str())
                }
                Statement::Expr(_) => None,
            })
            .filter(|n| owner.params.contains_key(*n))
            .collect();
        let narrowed;
        let owner = if shadowed.is_empty() {
            owner
        } else {
            narrowed = owner.without(shadowed.into_iter());
            &narrowed
        };

        let mut row = Row::default();
        for s in &b.statements {
            match s {
                Statement::Let { value, .. } => row.absorb(&self.row_of_expr(value, owner, known)),
                // 嵌套的具名方法是定义不是调用；它自己会被单独核
                Statement::Function { .. } => {}
                Statement::Expr(e) => row.absorb(&self.row_of_expr(e, owner, known)),
            }
        }
        if let Some(r) = &b.result {
            row.absorb(&self.row_of_expr(r, owner, known));
        }
        row
    }

    pub(crate) fn row_of_expr(&self, e: &Expr, owner: &Owner, known: &Known) -> Row {
        let mut row = Row::default();
        match e.kind() {
            // 纪律 1：造一个方法值不执行它
            ExprKind::Function(_) => {}
            ExprKind::Call {
                function,
                arguments,
            } => {
                let callee = match function.kind() {
                    ExprKind::Name(n) => {
                        row.absorb(&self.row_of_callee(n, &arguments, owner, known));
                        Some(n)
                    }
                    // 当场造当场调：体内的效应确实会发生
                    ExprKind::Function(f) => {
                        let inner = owner.without(f.parameters.iter().map(|p| p.name.as_str()));
                        row.absorb(&self.row_of_block(&f.body, &inner, known));
                        None
                    }
                    // `g(…)(x)`：g 返回的方法会被调用——方法经**返回值**传递时契约不丢
                    ExprKind::Call {
                        function: inner_f,
                        arguments: inner_args,
                    } => {
                        if let Some(fe) = function.expr() {
                            row.absorb(&self.row_of_expr(fe, owner, known));
                        }
                        match inner_f.kind() {
                            ExprKind::Name(g) => match self.returned_row(g) {
                                Some((id, ret)) => row.absorb(&self.instantiate(
                                    &ret,
                                    id,
                                    &inner_args,
                                    owner,
                                    known,
                                )),
                                None => row.opaque = true,
                            },
                            _ => row.opaque = true,
                        }
                        None
                    }
                    // `g(…).字段(x)`：记录字段上的方法会被调用——方法经**记录字段**传递时契约不丢
                    ExprKind::Field { value, field } => {
                        if let Some(fe) = function.expr() {
                            row.absorb(&self.row_of_expr(fe, owner, known));
                        }
                        match value.kind() {
                            ExprKind::Call {
                                function: gf,
                                arguments: gargs,
                            } => match gf.kind() {
                                ExprKind::Name(g) => match self.field_row(g, field) {
                                    Some((id, fr)) => {
                                        row.absorb(&self.instantiate(&fr, id, &gargs, owner, known))
                                    }
                                    None => row.opaque = true,
                                },
                                _ => row.opaque = true,
                            },
                            _ => row.opaque = true,
                        }
                        None
                    }
                    _ => {
                        if let Some(fe) = function.expr() {
                            row.absorb(&self.row_of_expr(fe, owner, known));
                        }
                        row.opaque = true;
                        None
                    }
                };
                // 落在已知高阶位上的方法会被调用
                let positions: Vec<usize> = callee.map(method_positions).unwrap_or_default();
                for (k, a) in arguments.iter().enumerate() {
                    if positions.contains(&k) {
                        row.absorb(&self.invoked(a, owner, known));
                    } else if callee == Some("handle") && k == 1 {
                        if let ExprKind::Record(arms) = a.kind() {
                            for (_, v) in arms {
                                row.absorb(&self.invoked(v, owner, known));
                            }
                            continue;
                        }
                        row.absorb(&self.row_of_expr(a, owner, known));
                    } else {
                        row.absorb(&self.row_of_expr(a, owner, known));
                    }
                }
            }
            ExprKind::List(items) => items
                .iter()
                .for_each(|x| row.absorb(&self.row_of_expr(x, owner, known))),
            ExprKind::Record(fields) => fields
                .iter()
                .for_each(|(_, x)| row.absorb(&self.row_of_expr(x, owner, known))),
            ExprKind::Field { value, .. } => row.absorb(&self.row_of_expr(value, owner, known)),
            ExprKind::Index { value, index } => {
                row.absorb(&self.row_of_expr(value, owner, known));
                row.absorb(&self.row_of_expr(index, owner, known));
            }
            ExprKind::Unary { value, .. } => row.absorb(&self.row_of_expr(value, owner, known)),
            ExprKind::Binary { left, right, .. } => {
                row.absorb(&self.row_of_expr(left, owner, known));
                row.absorb(&self.row_of_expr(right, owner, known));
            }
            ExprKind::If { condition, yes, no } => {
                row.absorb(&self.row_of_expr(condition, owner, known));
                row.absorb(&self.row_of_block(yes, owner, known));
                row.absorb(&self.row_of_block(no, owner, known));
            }
            ExprKind::Block(b) => row.absorb(&self.row_of_block(b, owner, known)),
            _ => {}
        }
        row
    }

    /// 被调者是个名字时，这次调用的效应行
    pub(crate) fn row_of_callee(
        &self,
        n: &str,
        arguments: &[&Expr],
        owner: &Owner,
        known: &Known,
    ) -> Row {
        if let Some(eff) = builtin_effect(n) {
            return Row::effect(eff);
        }
        // 类型标注自己带了效应行：契约跟着类型走
        if let Some(row) = known.get(n) {
            match row {
                Some(effects) => return Row::of(effects.iter()),
                // 标了 Fn 但没给行：能留变量就留变量，留不了才算未知
                None => {
                    return match owner.params.get(n) {
                        Some(k) => Row::var(owner.id, *k),
                        None => Row::opaque(),
                    };
                }
            }
        }
        // 纪律 3：被调者是本函数的方法参数 → 留一个效应变量，等调用点实例化
        if let Some(k) = owner.params.get(n) {
            return Row::var(owner.id, *k);
        }
        if is_builtin(n) {
            return Row::default();
        }
        let Some(ids) = self.by_name.get(n) else {
            return Row::opaque();
        };
        if ids.len() != 1 {
            return Row::opaque();
        }
        let base = self.base_row(ids[0]);
        self.instantiate(&base, ids[0], arguments, owner, known)
    }

    /// 已知函数对调用者露出的行：标注是上界（它自己在定义处被核），没标就用推断出来的确定部分
    pub(crate) fn base_row(&self, id: usize) -> Row {
        let f = &self.functions[id];
        let mut row = match &f.declared {
            Some(d) => Row::of(d.iter()),
            None => Row {
                concrete: f.row.concrete.clone(),
                ..Row::default()
            },
        };
        row.vars = f.row.vars.clone();
        row.opaque |= f.row.opaque;
        row
    }

    /// 把被调者留下的效应变量，用这次调用的实参实例化（纪律 3：按调用点，不取并集）
    pub(crate) fn instantiate(
        &self,
        row: &Row,
        callee: usize,
        arguments: &[&Expr],
        owner: &Owner,
        known: &Known,
    ) -> Row {
        let mut out = Row {
            concrete: row.concrete.clone(),
            opaque: row.opaque,
            vars: BTreeSet::new(),
        };
        for (o, k) in &row.vars {
            if *o != callee {
                out.opaque = true;
                continue;
            }
            match arguments.get(*k) {
                Some(a) => out.absorb(&self.invoked(a, owner, known)),
                None => out.opaque = true,
            }
        }
        out
    }

    /// 这个位置上的方法会被调用：它的效应算进来
    pub(crate) fn invoked(&self, a: &Expr, owner: &Owner, known: &Known) -> Row {
        match a.kind() {
            ExprKind::Function(f) => {
                let inner = owner.without(f.parameters.iter().map(|p| p.name.as_str()));
                let mut row = self.row_of_block(&f.body, &inner, known);
                if let Some(d) = &f.effects {
                    // 标了就按标注的上界算确定部分；这个匿名方法自己另行被核
                    row.concrete = d.iter().cloned().collect();
                }
                row
            }
            // 具名方法：这里没有实参可给，它自己的效应变量只能退成未知
            ExprKind::Name(n) => self.row_of_callee(n, &[], owner, known),
            _ => {
                let mut row = self.row_of_expr(a, owner, known);
                row.opaque = true;
                row
            }
        }
    }

    /// 这个函数返回的是不是一个方法值；是的话它的行是什么（`(函数 id, 行)`）
    pub(crate) fn returned_row(&self, n: &str) -> Option<(usize, Row)> {
        let ids = self.by_name.get(n)?;
        if ids.len() != 1 {
            return None;
        }
        self.functions[ids[0]].returns.clone().map(|r| (ids[0], r))
    }

    /// 这个函数返回的记录里，某个字段是不是静态认得出的方法值
    pub(crate) fn field_row(&self, n: &str, field: &str) -> Option<(usize, Row)> {
        let ids = self.by_name.get(n)?;
        if ids.len() != 1 {
            return None;
        }
        self.functions[ids[0]]
            .fields
            .get(field)
            .cloned()
            .map(|r| (ids[0], r))
    }

    /// 结果位上的记录字面量里，哪些字段是方法值
    pub(crate) fn method_fields(
        &self,
        e: Option<&Expr>,
        owner: &Owner,
        known: &Known,
    ) -> HashMap<String, Row> {
        let mut out = HashMap::new();
        let Some(e) = e else { return out };
        match e.kind() {
            ExprKind::Record(fields) => {
                for (k, v) in fields {
                    if let Some(row) = self.method_value_row(Some(v), owner, known) {
                        out.insert(k.clone(), row);
                    }
                }
            }
            ExprKind::Block(b) => return self.method_fields(b.result.as_deref(), owner, known),
            _ => {}
        }
        out
    }

    /// 块的结果位上是不是一个静态认得出的方法值
    pub(crate) fn method_value_row(
        &self,
        e: Option<&Expr>,
        owner: &Owner,
        known: &Known,
    ) -> Option<Row> {
        let e = e?;
        match e.kind() {
            ExprKind::Name(n) => {
                if let Some(k) = owner.params.get(n) {
                    return Some(Row::var(owner.id, *k));
                }
                if let Some(Some(effects)) = known.get(n) {
                    return Some(Row::of(effects.iter()));
                }
                let ids = self.by_name.get(n)?;
                if ids.len() != 1 {
                    return None;
                }
                Some(self.base_row(ids[0]))
            }
            ExprKind::Function(f) => {
                let inner = owner.without(f.parameters.iter().map(|p| p.name.as_str()));
                let mut row = self.row_of_block(&f.body, &inner, known);
                if let Some(d) = &f.effects {
                    row.concrete = d.iter().cloned().collect();
                }
                Some(row)
            }
            ExprKind::Block(b) => self.method_value_row(b.result.as_deref(), owner, known),
            ExprKind::If { yes, no, .. } => {
                let a = self.method_value_row(yes.result.as_deref(), owner, known)?;
                let b = self.method_value_row(no.result.as_deref(), owner, known)?;
                let mut row = a;
                row.absorb(&b);
                Some(row)
            }
            _ => None,
        }
    }
}
