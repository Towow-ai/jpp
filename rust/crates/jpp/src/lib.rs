//! J++ 宿主 crate 的 lib 目标（B74，步 14a：原 `jpp_core` 的外观与原 `jpp-cli` 合为一个 crate）：
//! 外观（原 `jpp_core` 的全部公开路径不变）、宿主接口 [`Session`]（`session/`）、真机端口（`backends/`）。
//! 命令行是同一 crate 的 bin 目标（`src/cli/`）。内核在 `jpp-runtime` 等 crate，这里只组装。
//!
//! 入口是 [`Session`]（`run`、`replay`、`resume` 等；[`run`] 是它的薄包装）：先静态检查（[`check::check`]），无错再解释执行。`.jpp` 经 [`syntax::parse`] 与 [`lower`]
//! 降到 IR（[`Program`]），降级不执行程序；CLI 负责文件、固定观察表与报告。接口说明见 `INTERFACE.md`。
//!
//! ```ignore
//! use jpp::{run, effects::{FixedPorts, CalibStore}, interp::ActionRegistry, ledger::Ledger};
//! let mut fixed = FixedPorts::new();
//! let mut ledger = Ledger::new();
//! let outcome = run(&program, fixed.ports(), &CalibStore::new(), &ActionRegistry::new(), &mut ledger)?;
//! ```

pub use jpp_check as check;
pub use jpp_value::stat as conformal;
pub mod actions;
pub mod backends;
pub mod effects;
pub mod interp;
pub mod ledger;
pub mod names;
pub mod session;
pub mod store;
pub use jpp_calib::truth;
pub use jpp_runtime::strength;
pub use jpp_value::value;
pub use session::Session;

pub use check::{Diagnostic, Report, Severity, check, check_with_calib, check_with_profile};
pub use effects::{
    CalibRecord, CalibStore, EffectError, EffectPort, FixedPorts, FnPort, JevClient, JevPorts,
    NoCallPorts, Ports, ReplayPorts, obs_key,
};
pub use interp::{
    ActionRegistry, Cost, EntryArgs, EntryMat, EntryValue, Interp, Outcome, RtError, TaintOut,
};
pub use jpp_ir::ir;
pub use jpp_ir::ir::{Block, Budget, Expr, Function, Parameter, Program, Span, Stmt, Type};
pub use ledger::{Entry, Header, Ledger, Trace, TraceEvent};
pub use value::{
    Answer, Env, Exit, ExitKind, Mat, Op, Pending, Question, Reading, State, Taint, Value,
};

/// 运行失败的两种成色：静态检查不过，或运行期出错。两者都带 `Span`，前端据此定位到 `.jpp`。
#[derive(Debug)]
pub enum Error {
    /// 静态检查有错；程序没有开始执行
    Check(Report),
    /// 运行期错（J-01/J-02/J-05 运行面、类型不符、客户端错误等）
    Runtime(RtError),
}

impl Error {
    pub fn render(&self) -> String {
        match self {
            Error::Check(r) => r.render(),
            Error::Runtime(e) => e.render(),
        }
    }
    /// 所有诊断位置（前端用来渲染源码行）
    pub fn spans(&self) -> Vec<Span> {
        match self {
            Error::Check(r) => r.errors().iter().map(|d| d.span).collect(),
            Error::Runtime(e) => vec![e.span],
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for Error {}

/// 检查并执行一个程序。预算来自程序自己的 `budget`（E12 必填，由检查器把关）。
///
/// `ledger` 既是输出也是输入：把上一次运行的账本传进来即重放，命中的键零调用（配 [`NoCallPorts`]
/// 可以验证「同程序重放零调用」）。`actions` 是 `do` 能触发的登记动作表；没登记的动作是 J-11 错。
/// 步 14a 起是 [`Session::run`] 的薄包装。
pub fn run(
    program: &Program,
    ports: Ports<'_>,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    Session::new(ports, calib, actions).run(program, &EntryArgs::default(), ledger)
}

/// 审计重放（B35；21 步 3）：只凭账本重现首跑。与 [`run`] 相同，只是账本里记过的调用照记录计入预算，
/// 缺的记录报 `E-replay`（致命，不进 cause）。续跑用 [`run`]（已记录的不付费、继续往下）。
/// 步 14a 起是 [`Session::replay`] 的薄包装。
pub fn run_replay(
    program: &Program,
    ports: Ports<'_>,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    Session::new(ports, calib, actions).replay(program, &EntryArgs::default(), ledger)
}

/// 带 `fit` 注册表的入口（`12` §6.0 的 fit 桥要用它）。`run` 是它 fit 表为空的特例。
/// 步 14a 起是 [`Session::with_fits`] 的薄包装。
pub fn run_with_fits(
    program: &Program,
    ports: Ports<'_>,
    calib: &CalibStore,
    actions: &ActionRegistry,
    fits: &effects::FitRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error> {
    Session::new(ports, calib, actions)
        .with_fits(fits)
        .run(program, &EntryArgs::default(), ledger)
}

/// 跳过静态检查直接执行——只给检查器本身的对照测试用；正常路径请用 [`run`]。
pub fn run_unchecked(
    program: &Program,
    ports: Ports<'_>,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, RtError> {
    Session::new(ports, calib, actions).run_unchecked(program, ledger)
}

pub use jpp_syntax as syntax;

/// 把表层程序降到 IR（步 12d）：`jpp_syntax::lower` 配上外观层组装的名字表（[`names::CurrentNames`]）。
/// 缺预算、预算块写错在这里报（`J-07a`）。
/// 步 14a 起是 [`Session::compile`] 的薄包装（步 14b 起传空入口声明）。
pub fn lower(p: &syntax::ast::Program) -> Result<Program, Vec<syntax::Diagnostic>> {
    Session::compile(p, &ir::EntryDecl::default())
}
