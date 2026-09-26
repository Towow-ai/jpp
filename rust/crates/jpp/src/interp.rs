//! 运行时加规划的宿主侧组装（步 14a，B74）：原 `jpp_core::interp` 的路径与用法不变。
//!
//! `20` §2.2 第 3 条：运行时（`jpp-runtime`）不依赖规划（`jpp-plan`）。步 13a 时解释器在 `run` 入口
//! 按自身的 `passes` 现算计划，钩子默认 `jpp_plan::Hooks`，那是临时依赖（`COORDINATION.md`「14a 撤」）。
//! 撤掉之后，pass 开关与算计划这一步留在宿主：本模块的 [`Interp`] 包着运行时的解释器，带 `passes`，
//! `run` 前一刻用 `jpp_plan::plan` 算出计划、连同 `jpp_plan::Hooks` 交给运行时。算法与时点都与
//! 步 13a 相同（入口处、按当时的开关算），所以外部行为不变。`Session`（`session/`）走同一条路径。

use std::ops::{Deref, DerefMut};

use jpp_effects::Ports;
use jpp_ir::ir::{Budget, Program};
use jpp_ledger::LedgerPort;
pub use jpp_plan::Passes;
pub use jpp_runtime::*;

/// 运行时解释器加 pass 开关。字段与方法经 `Deref` 取运行时的；构造与 `run` 在这里包一层。
pub struct Interp<'a> {
    inner: jpp_runtime::Interp<'a>,
    /// 编译 pass 开关（`12` §4；定义在 `jpp-plan`）。`run` 入口据它算出计划
    pub passes: Passes,
}

impl<'a> Interp<'a> {
    /// 不带 fit 表的入口（绝大多数程序不用 fit）。效应调用只经端口表（步 15b、15c）。
    pub fn new(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn jpp_effects::views::CalibView,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        Interp {
            inner: jpp_runtime::Interp::new(ports, ledger, calib, actions, budget),
            passes: Passes::default(),
        }
    }

    pub fn with_fits(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn jpp_effects::views::CalibView,
        actions: &'a ActionRegistry,
        fits: Fits<'a>,
        budget: Budget,
    ) -> Interp<'a> {
        Interp {
            inner: jpp_runtime::Interp::with_fits(ports, ledger, calib, actions, fits, budget),
            passes: Passes::default(),
        }
    }

    /// 审计重放（B35）：见运行时同名方法
    pub fn audit_replay(self) -> Self {
        Interp {
            inner: self.inner.audit_replay(),
            passes: self.passes,
        }
    }

    /// 宿主入口参数（B105，步 14b）：见运行时同名方法
    pub fn with_entry(self, entry: EntryArgs) -> Self {
        Interp {
            inner: self.inner.with_entry(entry),
            passes: self.passes,
        }
    }

    /// 生成缓存（步 15h-2）：见运行时同名方法
    pub fn with_gen_cache(
        self,
        cache: std::rc::Rc<std::cell::RefCell<jpp_runtime::GenCache>>,
    ) -> Self {
        Interp {
            inner: self.inner.with_gen_cache(cache),
            passes: self.passes,
        }
    }

    /// 一次运行：按 `passes` 算出计划（`jpp_plan::plan`），连同 `jpp_plan::Hooks` 交给运行时。
    pub fn run(self, program: &Program) -> Result<Outcome, RtError> {
        let plan = jpp_plan::plan(program, &self.passes);
        self.inner.run(program, plan, &jpp_plan::Hooks)
    }
}

impl<'a> Deref for Interp<'a> {
    type Target = jpp_runtime::Interp<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Interp<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
