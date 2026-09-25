//! 静态检查器：能在不执行的情况下判定的纪律，报文带 `Span` 与规则号。
//!
//! 分工：`interp.rs` 已经在运行期精确把关 J-02（禁自指，要实际材料）、J-05（出口 `consumed` 标记，
//! 返回前核）、J-06 键重复即停、J-07 预算、J-12 Fail、J-18 账本头。这里只做**静态确定**的那一半，
//! 不重复运行期已经判准的东西；宁可漏报也不误报——误报会卡住已经写好的源码。
//!
//! 依据：`11-语言规范-v1.md` §诊断（E1…E16）与 `12-IR与类契约-v0.1.md` §5（J-01…J-18）。
//! 报文格式沿用 Python 检查器：`规则: 一句话。修法：…`。
//!
//! 本 crate 隐藏的设计决定（`20` §2.2 第 7 条）：哪些性质能不执行就判定，以及判不准时的口径。
//! 它只依赖 `jpp-ir` 与 `jpp-effects`（§2.2 第 1、4 条）：不持有客户端、不做 IO、不依赖值模型，
//! 读档案与校准只经 `Profile` 与只读视图 `CalibView`。步 12b 从 `jpp-core::check` 抽出。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::shapes::BUILTINS;
use jpp_effects::views::CalibView;
pub(crate) use jpp_ir::ir::{Block, Expr, Function, Stmt as Statement};
use jpp_ir::ir::{Parameter, Span, Type, TypeName};

mod analysis;
pub mod diag;
pub mod questions;
mod rules;
pub mod shapes;

use analysis::rows::*;
use analysis::view::{ExprKind, View};
use analysis::walk::*;

/// 检查器读的程序就是 IR（步 12d）：预算必填（缺预算在降级处报 `J-07a`），站点表随程序进 `Cx`。
pub(crate) use jpp_ir::ir::Program;

/// 效应名（函数 `!{…}` 标注里能出现的），按效应表注册顺序，只取 `in_effect_row` 为真的效应。
/// `transform` 是记账变换不是效应形式，那一位为假，不在此列。
pub fn effect_names() -> Vec<&'static str> {
    jpp_effects::ALL
        .iter()
        .map(|id| jpp_effects::spec(*id))
        .filter(|s| s.in_effect_row)
        .map(|s| s.name)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// 规则号：`J-xx` / `Exx`（依据文本编号）或 `E-xxx` / `W-xxx`（core 本地码，见 INTERFACE.md）
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(rule: &str, msg: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            rule: rule.into(),
            severity: Severity::Error,
            message: msg.into(),
            span,
        }
    }
    pub fn warning(rule: &str, msg: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            rule: rule.into(),
            severity: Severity::Warning,
            message: msg.into(),
            span,
        }
    }
    pub fn render(&self) -> String {
        format!(
            "{}: {} @{}..{}",
            self.rule, self.message, self.span.start, self.span.end
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect()
    }
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect()
    }
    /// 没有错（warning 不阻塞运行）
    pub fn is_ok(&self) -> bool {
        self.errors().is_empty()
    }
    /// 第一条给定规则号的诊断
    pub fn find(&self, rule: &str) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.rule == rule)
    }
    pub fn render(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| format!("{}\n", d.render()))
            .collect()
    }
}

/// 静态检查一个程序，**不带档案**。
///
/// **「不带档案」不是「档案说假设都成立」**：每条依赖类假设的规则会照 J-15 取保守项
/// 并另报一条 `W-untested`，而档案明说过的那些**不报**——两者因此在报告上分得开。
/// 与「兜底档案的 `hash` 必须是 `None`」是同一条。
pub fn check(program: &Program) -> Report {
    check_with(program, None)
}

/// 静态检查一个程序，**带上模型档案**（`12` §1.2 类假设绑档案字段、§1.3 降级）。
///
/// **这是 §1 缺的那根管道。** 在它之前 `check` 只收一个 `&Program`，
/// **就算降级逻辑写好了，档案字段也没有路径能到达检查器**——没填是缺料，
/// **没有管道是缺结构**。
pub fn check_with_profile(program: &Program, profile: &jpp_effects::Profile) -> Report {
    // **判据是 `hash.is_none()`，不是「传没传参数」**：`Profile::default()` 是兜底，
    // 不是一份档案。按参数判，每个默认库都会被读成「有档案这么说过」。
    if profile.hash.is_none() {
        return check_with(program, None);
    }
    check_with(program, Some(profile))
}

/// 静态检查一个程序，**带上整本校准记录**。
///
/// **为什么不是只多传一份档案**：J-10 的静态那一半（`12`:814 那张表：
/// 「J-10 unsure 上界 | **✓ 估计** | 实测」，**✓ 在静态那一栏**）要的是
/// **各题各自的 `unsure_rate`**，而那住在 `CalibRecord` 里，档案里没有。
/// **在这之前，条文点名的那个静态估计没有任何路径能拿到它要的数**——
/// 与 §1 那根「档案到不了检查器」的管道是同一种缺结构。
pub fn check_with_calib(program: &Program, calib: &dyn CalibView) -> Report {
    // 判据仍是 `hash.is_none()`（与 `check_with_profile` 同一条）：兜底不是一份档案。
    let profile = if calib.profile().hash.is_none() {
        None
    } else {
        Some(calib.profile())
    };
    check_with2(program, profile, Some(calib))
}

/// 宿主动作的静态事实（B108，步 24-0；`no_sandbox` 为 B164 新增）：J-08 静态子面据此分
/// error / 不报，认出输出不可信的动作，E-action-no-sandbox（`rules/e_action_no_sandbox.rs`）
/// 据 `no_sandbox` 无条件报错。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ActionFacts {
    /// 登记为可逆（J-08 只管不可逆动作）
    pub reversible: bool,
    /// 登记的输出 taint 为不可信（`TaintOut::Untrusted`，例如 CLI 的 `read_json`）
    pub output_untrusted: bool,
    /// 这个动作需要操作系统级沙箱才能跑，宿主启动时探测不到（B164）：不管调用点有没有守卫，
    /// `do` 到这个名字就无条件报 `E-action-no-sandbox`——这是环境问题，不是放行策略问题。
    pub no_sandbox: bool,
}

/// 宿主动作表的静态投影：动作名 → [`ActionFacts`]。检查器不依赖运行时，宿主从自己的动作登记表建它。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionTable {
    pub actions: std::collections::BTreeMap<String, ActionFacts>,
    /// 动作声明的输出形状（B51-R2，步 24g）：动作名 → `mat_shape`。与 `actions` 并列而不是
    /// `ActionFacts` 的字段——`MatShape` 带 `Vec<String>`，进 `ActionFacts` 会去掉 `Copy` 并破坏
    /// 全部既有的 `ActionFacts { .. }` 字面量构造点（`runner.rs`、`session/mod.rs`、多个测试）。
    /// 没有声明的动作不在此表，诊断层按此判「判不出来源」。
    pub shapes: std::collections::BTreeMap<String, jpp_effects::MatShape>,
}

/// 静态检查一个程序，带整本校准记录与**宿主动作表**（B108，步 24-0）：与 [`check_with_calib`] 相同，
/// 另让 J-08 静态子面在不可逆动作上报 error（不带动作表时只报 `W-guard-untrusted`）。
/// `Session` 执行前那次检查走这里，于是必然被 J-08 拦下的程序在花调用之前停下。
pub fn check_with_calib_actions(
    program: &Program,
    calib: &dyn CalibView,
    actions: &ActionTable,
) -> Report {
    let profile = if calib.profile().hash.is_none() {
        None
    } else {
        Some(calib.profile())
    };
    check_annotated_with(program, profile, Some(calib), Some(actions)).0
}

/// [`Session::explain`] 的动作表增强版（步 24c，B108 已知限制的收口）：不带整本校准记录（`explain`
/// 本就不带，`Session::run`/`resume`/`replay` 执行前才走 [`check_with_calib_actions`]），
/// 只多一份宿主动作表。**不改 `explain` 本身**（24-0 定的边界），CLI 用它给 `check`（不带
/// `--input` 也照样有）与 `run` 的预跑诊断同一份已知动作表，J-08 静态子面就能对可逆动作不报、
/// 对不可逆动作报 error（而不是没有表时一律降成 `W-guard-untrusted`）。
pub fn explain_with_actions(
    program: &Program,
    profile: Option<&jpp_effects::Profile>,
    actions: &ActionTable,
) -> Report {
    check_annotated_with(program, profile, None, Some(actions)).0
}

/// 已注册的规则单元号，按发出顺序（`rules::RULES` 是唯一注册处）。
pub fn rule_codes() -> Vec<&'static str> {
    rules::RULES.iter().map(|r| r.code).collect()
}

fn check_with(program: &Program, profile: Option<&jpp_effects::Profile>) -> Report {
    check_with2(program, profile, None)
}

fn check_with2(
    program: &Program,
    profile: Option<&jpp_effects::Profile>,
    calib: Option<&dyn CalibView>,
) -> Report {
    check_annotated(program, profile, calib).0
}

/// 静态检查并产出标注表（`20` §2.3：`check → (Report, AnnotTable)`）。
///
/// 标注表按 IR 节点号与站点号索引（入参就是降级产出的 IR，步 12d）。档案与校准的口径同
/// [`check_with_profile`] / [`check_with_calib`]：兜底档案（`hash` 为空）传 `None`。
pub fn check_annotated(
    program: &Program,
    profile: Option<&jpp_effects::Profile>,
    calib: Option<&dyn CalibView>,
) -> (Report, jpp_ir::ir::AnnotTable) {
    check_annotated_with(program, profile, calib, None)
}

fn check_annotated_with(
    program: &Program,
    profile: Option<&jpp_effects::Profile>,
    calib: Option<&dyn CalibView>,
    actions: Option<&ActionTable>,
) -> (Report, jpp_ir::ir::AnnotTable) {
    let sites = &rules::SiteFacts::of(&program.body, &program.sites);
    let cx = rules::Cx {
        p: program,
        sites,
        profile,
        calib,
        actions,
        names: None,
    };
    let mut c = Checker::new(cx);
    // 规则在 `rules::RULES` 一处注册；分析器三趟的位置固定在这里
    c.run_before();
    c.names_and_readings(program);
    c.syntax(program);
    c.effects(program);
    c.run_after();
    let annot = c.annotate(program);
    c.out.sort_by_key(|d| (d.span.start, d.span.end));
    (Report { diagnostics: c.out }, annot)
}

// ---------------------------------------------------------------- 抽象值类别

/// 区分「读数 / 出口 / 题 / 题式 / 其它」——J-01 与 J-05 的静态面要前两档；
/// 题与题式两档让字段名在 check 阶段就能核（施工件 b：题是一等值，字段写错不该等到运行期）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Reading,
    Exit,
    Question,
    Form,
    /// 组合封闭性契约值（B17）：sieve / pair / tally / first_k / iterate / outcome 的结果
    Outcome,
    Other,
}

/// 标注里的类型名 → 类别（只认题、题式与契约值；其余交给别的检查）。读降级解析好的类型名（步 12d）。
fn kind_of_annotation(t: &Type) -> Kind {
    match t {
        Type::Named(TypeName::Question) => Kind::Question,
        Type::Named(TypeName::Form) => Kind::Form,
        Type::Named(TypeName::Outcome) => Kind::Outcome,
        _ => Kind::Other,
    }
}

struct Scope {
    /// 块内所有绑定名。函数体里全可见：解释器的环境节点是共享的，递归与互相引用都成立。
    declared: HashMap<String, Kind>,
    /// 绑定到具名方法的，留下参数表：调用点据此核参数个数与字面量类型
    signatures: HashMap<String, Vec<Option<Type>>>,
    /// 带类型标注的绑定：`Fn¹`（捕获了未决责任）不能当 `Fnω` 用
    annotations: HashMap<String, Type>,
    /// 已经走过的绑定。语句位置的直接引用只能看见这些。
    defined: HashSet<String>,
}

impl Scope {
    fn new() -> Scope {
        Scope {
            declared: HashMap::new(),
            signatures: HashMap::new(),
            annotations: HashMap::new(),
            defined: HashSet::new(),
        }
    }
}

/// 字面量与类型标注各自落在哪一档。只有两边都认得、且不同，才判不符。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Int,
    Decimal,
    Bool,
    Text,
    List,
    Record,
    Method,
    Unit,
}

impl Shape {
    fn of_literal(e: &Expr) -> Option<Shape> {
        Some(match e.kind() {
            ExprKind::Integer(_) => Shape::Int,
            ExprKind::Decimal => Shape::Decimal,
            ExprKind::Bool => Shape::Bool,
            ExprKind::Text(_) => Shape::Text,
            ExprKind::List(_) => Shape::List,
            ExprKind::Record(_) => Shape::Record,
            ExprKind::Function(_) => Shape::Method,
            ExprKind::Unit => Shape::Unit,
            _ => return None,
        })
    }
    fn of_type(t: &Type) -> Option<Shape> {
        Some(match t {
            Type::Function(_, _) | Type::Method(_) => Shape::Method,
            Type::Applied(n, _) | Type::Named(n) => match n {
                TypeName::Int => Shape::Int,
                TypeName::Decimal | TypeName::Float => Shape::Decimal,
                TypeName::Bool => Shape::Bool,
                TypeName::Text => Shape::Text,
                TypeName::List => Shape::List,
                TypeName::Record => Shape::Record,
                TypeName::Fn | TypeName::Method => Shape::Method,
                TypeName::Unit => Shape::Unit,
                // Mat / State / Question / Reading / Exit 等由运行期把关
                _ => return None,
            },
        })
    }
    fn name(&self) -> &'static str {
        match self {
            Shape::Int => "Int",
            Shape::Decimal => "Decimal",
            Shape::Bool => "Bool",
            Shape::Text => "Text",
            Shape::List => "List",
            Shape::Record => "Record",
            Shape::Method => "方法",
            Shape::Unit => "Unit",
        }
    }
    /// 整数字面量可以当小数用；其余不互通
    fn fits(arg: Shape, want: Shape) -> bool {
        arg == want || (arg == Shape::Int && want == Shape::Decimal)
    }
}

struct FuncInfo {
    name: String,
    /// IR 里的函数节点（标注表按它索引）
    node: jpp_ir::key::NodeId,
    params: Vec<Parameter>,
    declared: Option<Vec<String>>,
    /// 函数体的效应行：确定集合 + 效应变量 + 「有认不出的被调者」
    row: Row,
    /// 这个函数返回的是不是一个静态认得出的方法值；是的话它的行
    returns: Option<Row>,
    /// 返回的记录里哪些字段是静态认得出的方法值（方法经**记录字段**传递时契约不丢）
    fields: HashMap<String, Row>,
    span: Span,
    body: Block,
}

struct Checker<'a> {
    /// 规则的读面（B71）：分析器在钩子点把它交给规则
    cx: rules::Cx<'a>,
    out: Vec<Diagnostic>,
    functions: Vec<FuncInfo>,
    by_name: HashMap<String, Vec<usize>>,
    /// 判定为读数的表达式节点（按地址标记，不依赖 Span 唯一）
    readings: HashSet<*const Expr>,
    /// `!{…}` 里有认不得的名字的函数：它们的标注不参与差集核对
    bad_effect_decl: HashSet<usize>,
}

impl<'a> Checker<'a> {
    fn new(cx: rules::Cx<'a>) -> Checker<'a> {
        Checker {
            cx,
            out: vec![],
            functions: vec![],
            by_name: HashMap::new(),
            readings: HashSet::new(),
            bad_effect_decl: HashSet::new(),
        }
    }
}

/// 这个位置上的值是读数吗？只穿过列表 / 记录字面量——它们原样装着值；
/// 调用、字段等节点的类别是它们自己的结果（`cut(judge(…))` 是出口，不是读数）。
pub(crate) fn is_reading_in(readings: &HashSet<*const Expr>, e: &Expr) -> bool {
    if readings.contains(&(e as *const Expr)) {
        return true;
    }
    match e.kind() {
        ExprKind::List(items) => items.iter().any(|x| is_reading_in(readings, x)),
        ExprKind::Record(fields) => fields.iter().any(|(_, x)| is_reading_in(readings, x)),
        _ => false,
    }
}

/// 这个名字在此处的类型标注
pub(crate) fn annotation_in(scopes: &[Scope], name: &str) -> Option<Type> {
    for s in scopes.iter().rev() {
        if s.declared.contains_key(name) {
            return s.annotations.get(name).cloned();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// `!{…}` 能写的效应名只从效应表的 `in_effect_row` 推出；顺序即诊断报文里的列举顺序，
    /// 这里钉住当前四个，效应表增删效应时报文随之变，这条先红（步 12d 从 `ir_side` 搬来）。
    #[test]
    fn 效应行的名字来自效应表() {
        assert_eq!(crate::effect_names(), vec!["judge", "gen", "do", "ask"]);
    }
}
