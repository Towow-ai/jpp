//! IR 本体（步 12a，`20` §2.3「L0 · `jpp-ir`」）：类型化节点、站点表、标注表、良构检查、打印。
//!
//! 步 12a 旁路构建，步 12b 起检查器读它，步 12c 运行时读它；步 12d 起由 `jpp_syntax::lower` 从表层
//! AST 直接产出（核心语法树与其转换器删除），是检查器与运行时唯一的程序表示。
//!
//! 与 `20` §2.3 字面的差异（节点字段按现行调用约定取实参原样，步 12d 由降级统一成目标字段）：
//! 语言形式的参数保留为表达式列表；函数仍内联在表达式里（`Program.functions` 与 `FnId`
//! 随步 13a 的 `TriggerPlan` 引入）；`Loop` 的体是函数表达式而非块。

mod annot;
mod names;
mod print;
mod types;
mod wellformed;

pub use annot::{Annot, AnnotTable, DutyKind, EffectRow, Phi, SiteAnnot};
pub use names::{NameClass, NameTable};
pub use print::print;
pub use types::{AbsentPolicy, Budget, MethodType, Parameter, Span, Type, TypeName};
pub use wellformed::wellformed;

use crate::key::{EffectId, NodeId, SiteId};
use serde::{Deserialize, Serialize};

/// IR 版本；打印带它。
pub const IR_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub version: u32,
    /// 必填（J-07a：预算存在性由类型保证，缺预算在降级处报）
    pub budget: Budget,
    pub body: Block,
    pub sites: SiteTable,
    /// 整个程序的源位置（预算类诊断落在这里）
    #[serde(default)]
    pub span: Span,
    /// 宿主入口参数声明（B106）：由 `jpp::Session::compile` 从宿主交进的名字表写入（唯一写入处），
    /// 检查器名字趟读它。空时不序列化、不打印，不进任何哈希与金样。
    /// 依据：B106（地基/附注/2026-09-25-B105-B106裁定.md §三）
    #[serde(default, skip_serializing_if = "EntryDecl::is_empty")]
    pub entry: EntryDecl,
}

/// 宿主入口参数声明（B106）：名字、种类与宿主声明的 taint。源码级声明语法未定（`21` A-13）。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryDecl {
    pub params: Vec<EntryParam>,
    /// 宿主接受作者声明线放行（B128，步 20j-2；CLI `--release-on-declared`）。检查器的 J-08 静态子面读它；
    /// 为假不序列化，不打印（现有 `ir.txt` 不变）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub accept_declared: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl EntryDecl {
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryParam {
    pub name: String,
    pub kind: EntryKind,
    pub taint: EntryTaint,
}

/// 入口条目的两种（B105-1）：值条目按读出规则绑定，材料条目绑定为材料
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Value,
    Mat,
}

/// 宿主声明的 taint（`jpp-ir` 不依赖 `jpp-value`，这里另记一份二值；与 `jpp_value::value::Taint` 一一对应）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryTaint {
    Trusted,
    Untrusted,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub result: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    Let {
        name: String,
        annotation: Option<Type>,
        value: Expr,
        span: Span,
    },
    Function {
        name: String,
        function: Function,
        span: Span,
    },
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub id: NodeId,
    pub parameters: Vec<Parameter>,
    pub result_type: Option<Type>,
    pub effects: Option<Vec<String>>,
    pub body: Block,
    /// 函数的结构哈希（方法身份，`13` §4；`transform` 的键与捕获指纹用）。步 12c 起运行时的闭包
    /// 取它：口径与步 12c 之前运行时对核心语法树函数的哈希相同（`hash_of(["fn", 源码函数的 JSON])`），
    /// 所以账本键逐字节不变。步 12d 删除核心语法树时改由降级按同一口径产出。
    #[serde(default)]
    pub source_hash: String,
}

/// 降级后不可变。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub id: NodeId,
    pub node: Node,
    pub span: Span,
}

/// 每种语言形式恰一个变体；五种效应共用 `Effect`（`20` §2.3、T1）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    /// `state(on[, {ctx, ref, over}])`
    State {
        on: Box<Expr>,
        opts: Option<Box<Expr>>,
        rest: Vec<Expr>,
        site: SiteId,
    },
    /// 效应：输入按 `EffectSpec.input_schema` 的槽名排列；多出的实参记为 `#<序号>`，由 `wellformed` 报出
    Effect {
        effect: EffectId,
        inputs: Vec<(String, Expr)>,
        site: SiteId,
    },
    /// 桥：`cut(reading[, key][, {cost}])`
    Cut {
        reading: Box<Expr>,
        rest: Vec<Expr>,
        site: SiteId,
    },
    Fit {
        args: Vec<Expr>,
        site: SiteId,
    },
    /// `loop(bound, init, fn)`
    Loop {
        bound: Box<Expr>,
        rest: Vec<Expr>,
        site: SiteId,
    },
    /// `handle(exit, {act, ignore, unsure, …})`
    Handle {
        exit: Box<Expr>,
        arms: Box<Expr>,
        rest: Vec<Expr>,
        site: SiteId,
    },
    /// 未决责任的三条消费路：`consume` / `escalate` / `literalize`
    Consume {
        how: ConsumeHow,
        args: Vec<Expr>,
        site: SiteId,
    },
    /// 内核构造，经构造表解析（`20` §五 S3）
    Construct {
        name: String,
        args: Vec<Expr>,
        site: SiteId,
    },
    Host(Host),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsumeHow {
    Consume,
    Escalate,
    Literalize,
}

impl ConsumeHow {
    pub fn name(&self) -> &'static str {
        match self {
            ConsumeHow::Consume => "consume",
            ConsumeHow::Escalate => "escalate",
            ConsumeHow::Literalize => "literalize",
        }
    }
}

/// 宿主表达式：字面量、名字、容器、函数、调用、条件、块。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Host {
    Integer(i64),
    Decimal(f64),
    Bool(bool),
    Text(String),
    Unit,
    Name(String),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Function(Function),
    /// 高阶宿主调用（`map`/`filter`/`fold` 等）带站点：推测与向量化的触发点
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        site: Option<SiteId>,
    },
    Field {
        value: Box<Expr>,
        field: String,
    },
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: String,
        value: Box<Expr>,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        yes: Block,
        no: Block,
        site: SiteId,
    },
    Block(Block),
}

/// 站点的种类。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteKind {
    State,
    Effect(EffectId),
    Cut,
    Fit,
    Loop,
    Handle,
    Consume(ConsumeHow),
    Construct(String),
    HigherOrder(String),
    If,
}

/// 一个站点：id、所在节点、源位置、所属函数、最近的外层循环或高阶调用站点。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteInfo {
    pub id: SiteId,
    pub node: NodeId,
    pub kind: SiteKind,
    pub span: Span,
    /// 所属函数节点；顶层为 `None`
    pub function: Option<NodeId>,
    /// 最近的外层 `Loop` 或高阶调用站点
    pub enclosing: Option<SiteId>,
}

/// 站点表：按 `SiteId` 顺序排列，`sites[i].id == SiteId(i)`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteTable {
    pub sites: Vec<SiteInfo>,
}

impl SiteTable {
    pub fn get(&self, id: SiteId) -> Option<&SiteInfo> {
        self.sites.get(id.0 as usize).filter(|s| s.id == id)
    }
}

/// IR 层的诊断：降级失败或良构检查不过。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IrDiag {
    pub code: String,
    pub message: String,
    pub span: Span,
}

impl IrDiag {
    pub fn new(code: &str, message: impl Into<String>, span: Span) -> IrDiag {
        IrDiag {
            code: code.into(),
            message: message.into(),
            span,
        }
    }
}

impl Expr {
    /// 这个节点自己的站点（没有则 `None`）。
    pub fn site(&self) -> Option<SiteId> {
        match &self.node {
            Node::State { site, .. }
            | Node::Effect { site, .. }
            | Node::Cut { site, .. }
            | Node::Fit { site, .. }
            | Node::Loop { site, .. }
            | Node::Handle { site, .. }
            | Node::Consume { site, .. }
            | Node::Construct { site, .. }
            | Node::Host(Host::If { site, .. }) => Some(*site),
            Node::Host(Host::Call { site, .. }) => *site,
            _ => None,
        }
    }

    /// 直接子表达式（按源顺序；块与函数体不在内，见 [`Expr::blocks`]）。
    pub fn children(&self) -> Vec<&Expr> {
        let mut v: Vec<&Expr> = vec![];
        match &self.node {
            Node::State { on, opts, rest, .. } => {
                v.push(on);
                v.extend(opts.iter().map(|b| b.as_ref()));
                v.extend(rest);
            }
            Node::Effect { inputs, .. } => v.extend(inputs.iter().map(|(_, e)| e)),
            Node::Cut { reading, rest, .. } => {
                v.push(reading);
                v.extend(rest);
            }
            Node::Fit { args, .. } | Node::Consume { args, .. } | Node::Construct { args, .. } => {
                v.extend(args)
            }
            Node::Loop { bound, rest, .. } => {
                v.push(bound);
                v.extend(rest);
            }
            Node::Handle {
                exit, arms, rest, ..
            } => {
                v.push(exit);
                v.push(arms);
                v.extend(rest);
            }
            Node::Host(h) => match h {
                Host::List(xs) => v.extend(xs),
                Host::Record(fs) => v.extend(fs.iter().map(|(_, e)| e)),
                Host::Call { callee, args, .. } => {
                    v.push(callee);
                    v.extend(args);
                }
                Host::Field { value, .. } | Host::Unary { value, .. } => v.push(value),
                Host::Index { value, index } => {
                    v.push(value);
                    v.push(index);
                }
                Host::Binary { left, right, .. } => {
                    v.push(left);
                    v.push(right);
                }
                Host::If { condition, .. } => v.push(condition),
                _ => {}
            },
        }
        v
    }

    /// 直接含的块（`if` 两支、块表达式、函数体）。
    pub fn blocks(&self) -> Vec<&Block> {
        match &self.node {
            Node::Host(Host::If { yes, no, .. }) => vec![yes, no],
            Node::Host(Host::Block(b)) => vec![b],
            Node::Host(Host::Function(f)) => vec![&f.body],
            _ => vec![],
        }
    }
}

impl Block {
    /// 块内的表达式（按源顺序）与嵌套函数体。
    pub fn exprs(&self) -> Vec<&Expr> {
        let mut v = vec![];
        for s in &self.statements {
            match s {
                Stmt::Let { value, .. } => v.push(value),
                Stmt::Expr(e) => v.push(e),
                Stmt::Function { .. } => {}
            }
        }
        v.extend(self.result.iter().map(|b| b.as_ref()));
        v
    }
    pub fn functions(&self) -> Vec<&Function> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Stmt::Function { function, .. } => Some(function),
                _ => None,
            })
            .collect()
    }
}

/// 先序遍历程序中的每个表达式。
pub fn walk<'a>(b: &'a Block, f: &mut dyn FnMut(&'a Expr)) {
    for s in &b.statements {
        match s {
            Stmt::Let { value, .. } => walk_expr(value, f),
            Stmt::Expr(e) => walk_expr(e, f),
            Stmt::Function { function, .. } => walk(&function.body, f),
        }
    }
    if let Some(r) = &b.result {
        walk_expr(r, f);
    }
}

/// 在块里（含嵌套函数体）按节点号找表达式。
pub fn find_expr(b: &Block, id: NodeId) -> Option<&Expr> {
    let mut found = None;
    walk(b, &mut |e| {
        if found.is_none() && e.id == id {
            found = Some(e);
        }
    });
    found
}

pub fn walk_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    for c in e.children() {
        walk_expr(c, f);
    }
    for b in e.blocks() {
        walk(b, f);
    }
}
