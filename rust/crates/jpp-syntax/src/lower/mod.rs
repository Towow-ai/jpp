//! 降级（`20` §2.3「L1（续）· `jpp-syntax::lower`」）：表层 AST 直接降到 IR（步 12d）。
//!
//! 名字到节点的映射不写在这里：调用方传入 [`NameTable`]（效应表来自 `jpp-effects`，构造表与宿主
//! 内置表来自运行时，由外观层组装），本模块只依赖 `jpp-ir`。被 `let`、函数名或形参遮蔽的名字按
//! 用户名字处理。节点号按先序分配，站点表记每个站点的所属函数与最近外层 `Loop` / 高阶 / 构造站点。
//!
//! 预算必填：缺预算在这里报 `J-07a`（用户看到的预算存在性诊断只此一处）。效应形式、语言形式与内核
//! 构造不作一等值（`20·B56`）：它们的名字出现在调用位置以外报 `E-form-as-value`。

mod budget;
mod fn_hash;

use crate::{Diagnostic, ast as a};
use jpp_ir::ir::{
    Block, Expr, Function, Host, IR_VERSION, NameClass, NameTable, Node, Parameter, Program,
    SiteInfo, SiteKind, SiteTable, Span, Stmt, Type, TypeName,
};
use jpp_ir::key::{NodeId, SiteId};
use std::collections::HashSet;

/// 缺预算的报文（原检查器 J-07 的缺预算分支，步 12d 移到降级，规则号改 `J-07a`）。
/// 依据：J-07（12 §5，预算是对总和的断言）；20 §2.3 `lower`「用户看到的预算存在性诊断只此一处」。
const NO_BUDGET: &str = "J-07a: 程序缺 budget：预算是对调用数与花费总和的断言，没有它就没有「超即停」的依据。修法：源码首行写 `budget {calls: N, cost: X, depth: D};`";

/// 表层程序降到 IR。预算与程序体都降完再报错：缺预算的程序（例如库文件）照样报出体内的
/// `E-form-as-value`；诊断按预算在前、程序体按源顺序排列。
pub fn lower(program: &a::Program, names: &dyn NameTable) -> Result<Program, Vec<Diagnostic>> {
    let span = span(program.body.span);
    let budget = match &program.budget {
        Some(b) => budget::budget(b),
        None => Err(Diagnostic::new(NO_BUDGET, program.body.span)),
    };
    let mut cx = Cx {
        names,
        next_node: 0,
        sites: vec![],
        functions: vec![],
        enclosing: vec![],
        scopes: vec![HashSet::new()],
        errors: vec![],
    };
    let body = cx.block(&program.body);
    match budget {
        Ok(budget) if cx.errors.is_empty() => Ok(Program {
            version: IR_VERSION,
            budget,
            body,
            sites: SiteTable { sites: cx.sites },
            span,
            entry: Default::default(),
        }),
        Ok(_) => Err(cx.errors),
        Err(d) => Err(std::iter::once(d).chain(cx.errors).collect()),
    }
}

/// 名字在值位置上可不可以出现：只有宿主内置与用户名字可以（`20·B56`；`20` §2.3 `lower` 不变量
/// 「只有宿主内置表里的名字可以作值」）。
fn form_kind(class: &NameClass) -> Option<&'static str> {
    match class {
        NameClass::Plain | NameClass::HigherOrder => None,
        NameClass::Effect(_) => Some("效应形式"),
        NameClass::Construct => Some("内核构造"),
        NameClass::State
        | NameClass::Cut
        | NameClass::Fit
        | NameClass::Loop
        | NameClass::Handle
        | NameClass::Consume(_) => Some("语言形式"),
    }
}

fn span(s: a::Span) -> Span {
    s.into()
}

/// 表层位置就是 IR 位置（同一对字节偏移）：降级原样搬过去。
impl From<a::Span> for Span {
    fn from(s: a::Span) -> Span {
        Span {
            start: s.start,
            end: s.end,
        }
    }
}

impl PartialEq<a::Span> for Span {
    fn eq(&self, other: &a::Span) -> bool {
        self.start == other.start && self.end == other.end
    }
}

impl PartialEq<Span> for a::Span {
    fn eq(&self, other: &Span) -> bool {
        self.start == other.start && self.end == other.end
    }
}

/// 作者写的类型标注解析成类型化的 [`Type`]：类型名成为 [`TypeName`]（`B71`：只解析标注，不推断）。
fn ty(t: &a::Type) -> Type {
    match t {
        a::Type::Named(n) => Type::Named(TypeName::parse(n)),
        a::Type::Applied(n, args) => {
            Type::Applied(TypeName::parse(n), args.iter().map(ty).collect())
        }
        a::Type::Function(args, result) => {
            Type::Function(args.iter().map(ty).collect(), Box::new(ty(result)))
        }
        a::Type::Method {
            parameters,
            result,
            effects,
            captures_responsibility,
        } => Type::Method(jpp_ir::ir::MethodType {
            params: parameters.iter().map(ty).collect(),
            ret: Box::new(ty(result)),
            effects: effects.clone(),
            captures_responsibility: *captures_responsibility,
        }),
    }
}

struct Cx<'a> {
    names: &'a dyn NameTable,
    next_node: u32,
    sites: Vec<SiteInfo>,
    functions: Vec<NodeId>,
    enclosing: Vec<SiteId>,
    scopes: Vec<HashSet<String>>,
    /// `E-form-as-value`（降完再一并报）
    errors: Vec<Diagnostic>,
}

impl Cx<'_> {
    fn node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    fn site(&mut self, node: NodeId, kind: SiteKind, span: Span) -> SiteId {
        let id = SiteId(self.sites.len() as u32);
        self.sites.push(SiteInfo {
            id,
            node,
            kind,
            span,
            function: self.functions.last().copied(),
            enclosing: self.enclosing.last().copied(),
        });
        id
    }

    fn shadowed(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn block(&mut self, b: &a::Block) -> Block {
        // 块内的绑定名在整个块里可见（与检查器的作用域规则同口径：先用后定义由检查器报 E-name，
        // 这里只要保证被遮蔽的名字不被当成语言形式）
        self.scopes.push(
            b.statements
                .iter()
                .filter_map(|s| match s {
                    a::Statement::Let { name, .. } | a::Statement::Function { name, .. } => {
                        Some(name.clone())
                    }
                    a::Statement::Expression(_) => None,
                })
                .collect(),
        );
        let mut statements = vec![];
        for s in &b.statements {
            statements.push(match s {
                a::Statement::Let {
                    name,
                    annotation,
                    value,
                    span: at,
                } => {
                    let value = self.expr(value);
                    Stmt::Let {
                        name: name.clone(),
                        annotation: annotation.as_ref().map(ty),
                        value,
                        span: span(*at),
                    }
                }
                a::Statement::Function {
                    name,
                    function,
                    span: at,
                } => {
                    let function = self.function(function);
                    Stmt::Function {
                        name: name.clone(),
                        function,
                        span: span(*at),
                    }
                }
                a::Statement::Expression(e) => Stmt::Expr(self.expr(e)),
            });
        }
        let result = b.result.as_ref().map(|r| Box::new(self.expr(r)));
        self.scopes.pop();
        Block {
            statements,
            result,
            span: span(b.span),
        }
    }

    fn function(&mut self, f: &a::Function) -> Function {
        let id = self.node_id();
        self.functions.push(id);
        self.scopes
            .push(f.parameters.iter().map(|p| p.name.clone()).collect());
        let body = self.block(&f.body);
        self.scopes.pop();
        self.functions.pop();
        Function {
            id,
            parameters: f
                .parameters
                .iter()
                .map(|p| Parameter {
                    name: p.name.clone(),
                    annotation: p.annotation.as_ref().map(ty),
                    span: span(p.span),
                })
                .collect(),
            result_type: f.result_type.as_ref().map(ty),
            effects: f.effects.clone(),
            body,
            source_hash: fn_hash::source_hash(f),
        }
    }

    fn exprs(&mut self, xs: &[a::Expr]) -> Vec<Expr> {
        xs.iter().map(|x| self.expr(x)).collect()
    }

    fn expr(&mut self, e: &a::Expr) -> Expr {
        use a::ExprKind as K;
        let id = self.node_id();
        let at = span(e.span);
        let node = match &e.kind {
            K::Integer(v) => Node::Host(Host::Integer(*v)),
            K::Decimal(v) => Node::Host(Host::Decimal(*v)),
            K::Bool(v) => Node::Host(Host::Bool(*v)),
            K::Text(v) => Node::Host(Host::Text(v.clone())),
            K::Unit => Node::Host(Host::Unit),
            K::Name(n) => {
                // 依据：B56（20 附录 A，效应形式与内核构造不作一等值）；20 §2.3 `lower` 不变量
                if !self.shadowed(n)
                    && let Some(what) = form_kind(&self.names.classify(n))
                {
                    self.errors.push(Diagnostic::new(
                        format!(
                            "E-form-as-value: `{n}` 是{what}，只能出现在调用位置（`{n}(…)`），不能作值（赋值、传参、放进容器）：\
                             作值的调用没有站点与标注，绕开了检查与计划。修法：在用到的地方直接调用，或包成方法 `fn(x) {{ {n}(…) }}` 再传"
                        ),
                        e.span,
                    ));
                }
                Node::Host(Host::Name(n.clone()))
            }
            K::List(xs) => Node::Host(Host::List(self.exprs(xs))),
            K::Record(fs) => Node::Host(Host::Record(
                fs.iter().map(|(k, v)| (k.clone(), self.expr(v))).collect(),
            )),
            K::Function(f) => Node::Host(Host::Function(self.function(f))),
            K::Call {
                function,
                arguments,
            } => self.call(id, function, arguments, at),
            K::Field { value, field } => Node::Host(Host::Field {
                value: Box::new(self.expr(value)),
                field: field.clone(),
            }),
            K::Index { value, index } => Node::Host(Host::Index {
                value: Box::new(self.expr(value)),
                index: Box::new(self.expr(index)),
            }),
            K::Unary { op, value } => Node::Host(Host::Unary {
                op: op.clone(),
                value: Box::new(self.expr(value)),
            }),
            K::Binary { op, left, right } => Node::Host(Host::Binary {
                op: op.clone(),
                left: Box::new(self.expr(left)),
                right: Box::new(self.expr(right)),
            }),
            K::If { condition, yes, no } => {
                let site = self.site(id, SiteKind::If, at);
                Node::Host(Host::If {
                    condition: Box::new(self.expr(condition)),
                    yes: self.block(yes),
                    no: self.block(no),
                    site,
                })
            }
            K::Block(b) => Node::Host(Host::Block(self.block(b))),
        };
        Expr { id, node, span: at }
    }

    /// 调用位置上的被调者：名字在这里不是值，不查 `E-form-as-value`。
    fn callee(&mut self, function: &a::Expr) -> Expr {
        match &function.kind {
            a::ExprKind::Name(n) => Expr {
                id: self.node_id(),
                node: Node::Host(Host::Name(n.clone())),
                span: span(function.span),
            },
            _ => self.expr(function),
        }
    }

    fn host_call(&mut self, function: &a::Expr, arguments: &[a::Expr]) -> Node {
        let callee = Box::new(self.callee(function));
        Node::Host(Host::Call {
            callee,
            args: self.exprs(arguments),
            site: None,
        })
    }

    fn call(&mut self, id: NodeId, function: &a::Expr, arguments: &[a::Expr], at: Span) -> Node {
        let (name, class) = match &function.kind {
            a::ExprKind::Name(n) if !self.shadowed(n) => (n.clone(), self.names.classify(n)),
            _ => (String::new(), NameClass::Plain),
        };
        match class {
            NameClass::Plain => self.host_call(function, arguments),
            NameClass::HigherOrder => {
                let site = self.site(id, SiteKind::HigherOrder(name), at);
                let callee = Box::new(self.callee(function));
                self.enclosing.push(site);
                let args = self.exprs(arguments);
                self.enclosing.pop();
                Node::Host(Host::Call {
                    callee,
                    args,
                    site: Some(site),
                })
            }
            NameClass::Effect(effect) => {
                let site = self.site(id, SiteKind::Effect(effect), at);
                let slots = self.names.slots(effect);
                let inputs = arguments
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let slot = slots
                            .get(i)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("#{i}"));
                        (slot, self.expr(a))
                    })
                    .collect();
                Node::Effect {
                    effect,
                    inputs,
                    site,
                }
            }
            NameClass::Construct => {
                let site = self.site(id, SiteKind::Construct(name.clone()), at);
                self.enclosing.push(site);
                let args = self.exprs(arguments);
                self.enclosing.pop();
                Node::Construct { name, args, site }
            }
            NameClass::Consume(how) => {
                let site = self.site(id, SiteKind::Consume(how), at);
                Node::Consume {
                    how,
                    args: self.exprs(arguments),
                    site,
                }
            }
            // 以下语言形式要求至少一个实参；缺实参时退回宿主调用，由检查器报数目错
            _ if arguments.is_empty() => self.host_call(function, arguments),
            NameClass::State => {
                let site = self.site(id, SiteKind::State, at);
                let on = Box::new(self.expr(&arguments[0]));
                let opts = arguments.get(1).map(|a| Box::new(self.expr(a)));
                let rest = self.exprs(arguments.get(2..).unwrap_or(&[]));
                Node::State {
                    on,
                    opts,
                    rest,
                    site,
                }
            }
            NameClass::Cut => {
                let site = self.site(id, SiteKind::Cut, at);
                let reading = Box::new(self.expr(&arguments[0]));
                Node::Cut {
                    reading,
                    rest: self.exprs(&arguments[1..]),
                    site,
                }
            }
            NameClass::Fit => {
                let site = self.site(id, SiteKind::Fit, at);
                Node::Fit {
                    args: self.exprs(arguments),
                    site,
                }
            }
            NameClass::Loop => {
                let site = self.site(id, SiteKind::Loop, at);
                let bound = Box::new(self.expr(&arguments[0]));
                self.enclosing.push(site);
                let rest = self.exprs(&arguments[1..]);
                self.enclosing.pop();
                Node::Loop { bound, rest, site }
            }
            NameClass::Handle if arguments.len() >= 2 => {
                let site = self.site(id, SiteKind::Handle, at);
                let exit = Box::new(self.expr(&arguments[0]));
                let arms = Box::new(self.expr(&arguments[1]));
                Node::Handle {
                    exit,
                    arms,
                    rest: self.exprs(&arguments[2..]),
                    site,
                }
            }
            NameClass::Handle => self.host_call(function, arguments),
        }
    }
}
