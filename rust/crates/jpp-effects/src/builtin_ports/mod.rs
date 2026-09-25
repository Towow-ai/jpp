//! 内置端口（`20` §2.3：`FixedPorts`、`NoCallPorts`、`ReplayPorts` 三个内置实现，覆盖经端口的全部效应
//! 实例；`21` 步 15b）与测试桩、小后端用的 `FnPort`（步 15c）。`EffectId` 的变体名允许出现在本目录
//! （`20` A2、§11.1）。

pub mod fixed;
pub mod fn_port;
pub mod refuse;

pub use fixed::{FIXED_MODEL, FixedAsk, FixedGen, FixedJudge, FixedPorts, obs_key};
pub use fn_port::FnPort;
pub use refuse::{NoCallPorts, RefusePort, ReplayPorts, UnansweredPort, UnservedPort};
