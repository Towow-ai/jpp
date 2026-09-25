//! 内核构造（20 §2.3 `constructs/*.rs`）：每个构造一个文件；步 4a 从 `builtin` 的分派臂机械拆出。

mod allocate;
pub(crate) mod compose;
mod contract;
pub(crate) mod element;
mod fit;
mod iterate;
mod pair;
mod questions;
mod repeat;
mod sieve;
mod tally;
