//! 计划的数据类型与运行期钩子（步 13a，`20` §2.3「L0 · `jpp-ir`」`plan` 子模块、「L3 · `jpp-plan`」）。
//!
//! 规划（`jpp-plan`）写 [`Plan`]，运行时只读它；运行期才算得出的那一半经 [`PlanHooks`] 注入，
//! trait 在这里定义、实现在 `jpp-plan`（`20` §2.2 第 3 条、T3）。运行时因此不自己做推测与向量化的分析：
//! 它拿到的是「哪些站点可以提前登记」，自己只求值与登记。
//!
//! 与 `20` §2.3 字面的差异（步 13a 过程记录与 `过程记录/总账待补条目.md` 各记一行，主会话 2026-09-24 认可）：
//! - **触发点以节点号为键**（`BTreeMap<NodeId, _>`），不以 `SiteId`：块内位置的触发点是 `let` 语句，
//!   `let` 没有站点；取被绑定的值表达式的节点号。
//! - **`instantiate` 带环境视图**（[`EnvView`]）而不是只带实参摘要：HEAD 判断「会不会产生效应」时按
//!   运行期环境解析名字（名字绑定的是哪个方法值、方法体里又调了谁），换成纯静态摘要会让现有测试的结果变，
//!   步 13a 就不再是 R。形参已由运行时绑进环境，实参摘要经视图查得。
//! - 高阶触发点（`map`/`filter`）在 HEAD 按内置身份触发（任何一次 `map`/`filter` 调用），不按站点；
//!   静态一半是每个函数体的候选目标（[`Plan::bodies`]），动态一半是 `instantiate`。
//! - `SitePlan`（`phys`、`fission`、`sched_class`、`concurrency`）等有消费者的 pass 落地时再加（步 23；
//!   阶段评估① §五·9「有消费者才建」）；`select_within` 随步 22。

use crate::ir::{Expr, Function};
use crate::key::NodeId;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// 函数的身份：函数节点的节点号（`Function.id`）。
pub type FnId = NodeId;

/// 估计值：未测即未知，不写 0（`20` §2.3 `jpp-plan`，`14` 附录）。
#[derive(Clone, Debug, PartialEq)]
pub enum Estimate<T> {
    Known { lo: T, hi: T },
    Unknown(String),
}

/// 一次运行的计划：规划写、运行时读。
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// 同状态、同层的题合成一次调用（`fuse`）
    pub fuse: bool,
    /// 高阶调用（`map`/`filter`）后续各轮的目标站点提前登记（`vectorize`）
    pub vectorize: bool,
    /// 惰性过桥（B94，步 23c，`lazy_cut`）：`cut` 返回未解析出口，第一次被检视时才刷新、解析
    pub lazy_cut: bool,
    /// 推测：`let` 值表达式的节点号 → 从这条语句起、`if` 两侧分支体里的候选站点（`speculate`）
    pub triggers: BTreeMap<NodeId, TriggerPlan>,
    /// 提升：`let` 值表达式（本身是一次 `judge`）的节点号 → 后续同状态语句的提升步（`lift`）
    pub lifts: BTreeMap<NodeId, LiftPlan>,
    /// 每个函数体的候选目标站点（静态一半；向量化时经 [`PlanHooks::instantiate`] 按运行期环境筛）
    pub bodies: BTreeMap<FnId, Vec<TargetSite>>,
    /// 直线段提升穿过函数调用（B94，步 23c，随 `lift`）：`let` 值表达式的节点号 → 从这条语句起的
    /// 直线段里的候选站点与调用（经 [`PlanHooks::segment`] 按运行期环境筛）
    pub segments: BTreeMap<NodeId, Vec<TargetSite>>,
    /// 调用数、层数估计：`plan` pass 未落地，恒为未知
    pub calls_est: Estimate<u32>,
    pub layers_est: Estimate<u32>,
}

impl Plan {
    /// 什么都不提前的计划（全部 pass 关）
    pub fn empty() -> Plan {
        Plan {
            fuse: false,
            vectorize: false,
            lazy_cut: false,
            triggers: BTreeMap::new(),
            lifts: BTreeMap::new(),
            bodies: BTreeMap::new(),
            segments: BTreeMap::new(),
            calls_est: Estimate::Unknown("plan pass 未落地".into()),
            layers_est: Estimate::Unknown("plan pass 未落地".into()),
        }
    }
}

/// 触发点的种类。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// 块内第 `n` 条语句求值之前（推测 `if` 两侧）
    BlockFrom(usize),
}

/// 一个触发点：从这里出发可提前登记的目标站点，按遍历顺序（登记顺序即层内题序，决定账本字节）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerPlan {
    pub kind: TriggerKind,
    pub targets: Vec<TargetSite>,
}

/// 目标站点：一个 `judge` 节点。能否提前登记还要看运行期环境（钩子按 [`EnvView`] 核）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSite {
    /// `judge` 节点的节点号
    pub node: NodeId,
    /// 状态与题两段表达式里用到的名字（运行时按它们求值）
    pub needs_bound: BTreeSet<String>,
    /// 步 13b：这是一次对用户函数的调用（向量化时穿进被调函数体），不是 `judge` 节点
    pub call: bool,
}

/// `instantiate` 的结果（步 13b）：可提前登记的站点，或穿进一次用户函数调用后的站点。
/// 运行时按顺序执行：`Site` 在当前环境里求状态与题并登记；`Enter` 在当前环境里求被调者与实参
/// （实参只有名字或字面量），在被调函数的捕获环境上绑好形参，再对 `inner` 同样执行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Site(NodeId),
    Enter { call: NodeId, inner: Vec<Target> },
}

/// 提升计划：头一条 `let` 是一次 `judge`，其后各条语句按顺序的处置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftPlan {
    /// 头之后逐条语句（静态上遇到必停处即截断）
    pub steps: Vec<LiftStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftStep {
    /// 该语句在块内的下标
    pub index: usize,
    /// 该语句的值表达式的节点号
    pub node: NodeId,
    /// 绑定的名字
    pub name: String,
    /// 同状态的 `judge`：提前求值；否则只是越过它
    pub lift: bool,
    /// 这一句重新绑定了头状态里用到的名字：处理完即停
    pub stop_after: bool,
}

/// 「会不会产生效应」问的是哪一种：严格（任何不在可提前求值表上的调用）或只问触世界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Strict,
    World,
}

/// 运行期环境的只读视图：规划的钩子经它按名字查值的摘要，运行时实现。
pub trait EnvView {
    fn lookup(&self, name: &str) -> Option<ValueSummary>;
}

/// 方法值的视图：函数体、身份（结构哈希）与捕获环境。
pub trait FnView {
    fn function(&self) -> &Function;
    fn identity(&self) -> &str;
    fn env(&self) -> Rc<dyn EnvView>;
}

/// 值的摘要：钩子判断效应只需要这些。
#[derive(Clone)]
pub enum ValueSummary {
    /// 不含方法的数据
    Data,
    /// 内置（按名字）
    Builtin(String),
    /// 方法值
    Fn(Rc<dyn FnView>),
    /// 列表或记录：元素的摘要，按需展开
    Container(Rc<dyn Fn() -> Vec<ValueSummary>>),
}

/// 运行期钩子：规划里只有运行期才算得出的那一半（`20` §2.3 `jpp-plan`、§4.5 第 2 条）。
///
/// 返回的目标站点都是 `judge` 节点，按遍历顺序；每个都已核过「此刻能提前求值它的状态与题」
/// （两段表达式按 `env` 解析都不会产生效应）。运行时对它们只做求值与登记。
pub trait PlanHooks {
    /// 方法值 `body` 的一轮（形参已绑进 `env`）里可提前登记的目标（向量化）；节点号在 `body`
    /// 或被穿进的函数体里。步 13b 起穿过对用户函数的调用（多层包装）。
    fn instantiate(&self, plan: &Plan, body: &Function, env: &dyn EnvView) -> Vec<Target>;
    /// 触发点 `at`（`let` 值表达式的节点号，所在块 `block`）处可推测登记的目标站点。
    fn speculate<'b>(
        &self,
        plan: &Plan,
        at: NodeId,
        block: &'b crate::ir::Block,
        env: &dyn EnvView,
    ) -> Vec<&'b Expr>;
    /// 触发点 `at`（`let` 值表达式的节点号，所在块 `block`）起的直线段里可提前登记的目标（B94 下半，
    /// 步 23c）：`Site` 的节点号在 `block` 里，`Enter` 的调用节点在 `block` 里、内层在被调函数体里。
    fn segment(
        &self,
        plan: &Plan,
        at: NodeId,
        block: &crate::ir::Block,
        env: &dyn EnvView,
    ) -> Vec<Target>;
    /// 表达式求值时会不会产生效应（`Reach::Strict`）或触世界（`Reach::World`）。提升逐句问它。
    fn may_effect(&self, e: &Expr, env: &dyn EnvView, reach: Reach) -> bool;
}
