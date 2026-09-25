//! 效应本体（`20` §2.3 `jpp-effects`，L2）：一种效应「是什么」——输入槽、输出形状、键的组成、
//! taint 规则、调度类别、是否产出读数——以及能力画像的类型、只读视图 trait 与统一效应端口。
//!
//! - `spec`：`EffectSpec` 注册表，每种效应一文件 `kinds/<name>.rs`；运行时、规划、检查器按字段分派（步 15a）。
//! - `profile`：`Profile` 按效应实例分表（步 15d），唯一默认 `Profile::untested()`（步 15d-2 删 `Default`）。
//! - `views`：`CalibView`/`FitView`/`MatStorePort`/`CacheLookup`。
//! - `port`：统一效应端口 `EffectPort`、端口表 `Ports`（步 15b）；`builtin_ports`：`FixedPorts`、
//!   `NoCallPorts`、`ReplayPorts` 三个内置实现、两个占位端口与闭包端口 `FnPort`。旧 `Client` 步 15c 删除。
//!
//! 禁止依赖（`20` §2.3）：`jpp-runtime`、`jpp-calib`、`jpp-ledger`、HTTP。

pub mod builtin_ports;
pub mod kinds;
pub mod port;
pub mod profile;
pub mod spec;
pub mod view;
pub mod views;

pub use builtin_ports::{
    FIXED_MODEL, FixedPorts, FnPort, NoCallPorts, ReplayPorts, UnansweredPort, UnservedPort,
    obs_key,
};
pub use jpp_ir::key::{EffectId, EffectInstance};
pub use kinds::{ALL, PORTED, by_name, find, spec};
pub use port::{
    CallInput, EffectCall, EffectError, EffectOut, EffectPort, GenResult, JudgeResult, Ports,
    Ticket, argmax_index,
};
pub use profile::{
    ActionProfile, ClassAssumptions, EffectProfile, Field, MatShape, Profile, ShapeItems, Tri,
    Window, behavior_hash, hash16, profile_hash, profile_schema,
};
pub use spec::*;
pub use views::*;
