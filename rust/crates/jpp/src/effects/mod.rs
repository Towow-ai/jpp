//! 效应适配（原 `jpp_core::effects`，步 14a 起在 `jpp`，原路径重导出）：统一效应端口（观察 = 模型调用或
//! 固定记录，步 15b、15c）、校准记录（线只从记录来）。

mod profile;

// 统一效应端口、三个内置端口与闭包端口（步 15b、15c；旧 `Client` 步 15c 删除）
pub use jpp_effects::builtin_ports::{
    FIXED_MODEL, FixedAsk, FixedGen, FixedJudge, FixedPorts, FnPort, NoCallPorts, RefusePort,
    ReplayPorts, UnansweredPort, UnservedPort, obs_key,
};
pub use jpp_effects::port::{
    CallInput, EffectCall, EffectError, EffectOut, EffectPort, GenResult, JudgeResult, Ports,
    Ticket, argmax_index,
};
pub use jpp_effects::{EffectId, EffectInstance};
// 真机客户端 `JevClient` 步 14a 搬到 `backends/jev`（B74），步 15b 起由 `JevPorts` 接成端口，这里原路径重导出。
pub use crate::backends::jev::*;
// 校准记录、校准库与 fit 注册表在步 11 搬进 `jpp-calib`，这里原路径重导出。
pub use jpp_calib::calib::*;
pub use jpp_calib::fit::*;
pub use profile::*;
