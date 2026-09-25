//! 校准记录、样本、证书与 `CalibStore`（线只从记录来）。（步 4c 自 `effects.rs` 原样搬出）
//!
//! 2026-09-24 按职责拆成四个文件，只搬不改（`20` 每文件不超过 800 行）：`record`（记录、样本、
//! 证书、标注来源）、`store`（`CalibStore` 本体与写入口）、`commission`（认证与选线）、
//! `access`（漂移、查询、持久化、键构造、读线）。公共路径不变。

mod access;
mod commission;
mod extend;
mod fixed_sequence;
mod record;
mod rerun;
mod sequential;
mod store;

pub use extend::{ExtendOptions, ExtendRow};
pub use fixed_sequence::{FIXED_SEQUENCE_RULE, fixed_sequence_candidates, fixed_sequence_step};
pub use record::*;
use record::{单侧声明, 反查题型, 标注集指纹, 经验unsure率};
pub use rerun::RerunOutcome;
/// `phys` 反查题型（给 `lib.rs` 的有效 α 补算用）
pub(crate) fn 反查题型_pub(phys: &str) -> Option<jpp_value::value::Op> {
    反查题型(phys)
}
pub use sequential::{SEQUENTIAL_RULE, SeqSpec, random_arrival, seq_first, two_ends_order};
pub use store::*;
