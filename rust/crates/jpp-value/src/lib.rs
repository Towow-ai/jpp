//! J++ 的 L1 层（`20` §2.1、§2.3「L1 · `jpp-value`」）：值模型与统计。
//!
//! 步 8a（R）：`jpp-core` 的 `value.rs` 原样搬入 [`value`]，`conformal.rs` 原样搬入 [`stat`]；
//! `jpp-core` 在原路径（`jpp_core::value`、`jpp_core::conformal`）重导出。只依赖 `jpp-ir`（`20` §2.2）。
//! 桥的纯函数、契约值的唯一构造与可见性收紧在后续小步。依据：`21` §三·4 步 8。

pub mod bridge;
pub mod contract;
pub mod guard_ev;
pub mod prov;
pub mod raw;
pub mod stat;
pub mod value;
