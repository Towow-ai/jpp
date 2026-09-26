//! 存储（`20` v2 §2.3 `jpp::store`，原 L4 `jpp-store`，B74；`21` 步 18）。
//!
//! **隐藏的决定**：东西存在哪、什么格式、怎么迁移。内核其他部分只见 trait（运行时经
//! `jpp-effects` 的视图 trait 读它，步 19 起有 `CacheLookup`、`MatStorePort`）。
//!
//! **禁止依赖**：运行时（`jpp_runtime`）与 `crate::backends`（`scripts/deps.py` 的模块级约束）。
//!
//! 步 18-0（R）先建三件：[`blob`]（`Blob` trait 与内存、目录两个后端）、[`calib`]（校准记录的
//! 装载、夹具合并策略与写回，原在 `cli/run_io.rs`）、[`ProfileLoader`]（画像字节装载；路径解析
//! 仍在 CLI，B73）。步 18a 加 [`migrations`]（账本 v2 → v3）。步 18b 加 [`LedgerFile`]（逐行落盘的
//! 账本，B55）。缓存索引随步 19。

pub mod blob;
pub mod calib;
mod ledger_file;
pub mod migrations;
mod profile;

pub use blob::{Blob, DirBlob, IoErr, MemBlob};
pub use ledger_file::LedgerFile;
pub use profile::ProfileLoader;
