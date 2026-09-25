//! 能力画像：类型与档案哈希在步 9 搬进 `jpp-effects::profile`，这里原路径重导出；
//! `calib_hash` 依赖校准库，步 11 随校准库进 `jpp-calib`，这里原路径重导出。（步 4c 自 `effects.rs` 原样搬出）

pub use jpp_effects::profile::{
    ActionProfile, ClassAssumptions, EffectProfile, Field, MatShape, Profile, ShapeItems, Tri,
    Window, behavior_hash, profile_hash, profile_schema,
};

pub use jpp_calib::calib_hash;
