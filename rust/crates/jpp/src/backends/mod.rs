//! 真机端口（`20` §2.3 L6 `backends/<name>/`，B74）。步 14a 自 `jpp-core::effects` 原样搬来；
//! 只有 `ureq` 由 feature `live` 引入（门控与搬之前相同）。步 15b 起 `JevPorts`（判断端口 `JevPort` 加两个占位端口）是真机端口表。
//! 模块约束（`评估①裁定` §十第 12(c) 条，`deps.py` 核）：不引用 `crate::store`、`crate::session`。
//!
//! 步 15g-0：后端注册表（`20` §五 S1「第二个判断器 = `backends/<name>/` 一个模块目录 + 注册 1 行」）。
//! 宿主（`jpp::cli`）的 `--backend` 取值、报告 `mode` 文案、默认模型、B73 画像必带与传输超时告警都从这里取，
//! 不再按某个后端的名字写死。

pub mod jev;
pub mod stub;

use crate::effects::{Ports, Profile};

/// 一个注册后端建好的端口表（步 15g-0）。
pub trait BackendPorts {
    fn ports(&mut self) -> Ports<'_>;
    fn model_id(&self) -> String;
}

/// 注册表里的一个后端（步 15g-0，`20` §五 S1）。
pub struct BackendSpec {
    /// `--backend` 的取值
    pub name: &'static str,
    /// 没给 `--model` 时的模型名（也是画像按 `<model>.json` 解析时的文件名）
    pub default_model: &'static str,
    /// 报告 `mode` 字段的文案
    pub mode_label: &'static str,
    /// 这个后端有网络传输：画像缺 `transport.timeout_s` 时报 `W-untested`（过程记录 `工程-传输超时.md`）
    pub transport: bool,
    /// B127 过渡守卫：校准记录尚无模型分量（B60），这个后端能否带 `--calib`/`--calib-out`（只有 jev 能）。
    /// 20a-2 给校准键落 `model` 后删去本字段与 `cli/options.rs` 的检查。
    pub calib: bool,
    /// 按模型名与画像建端口表；画像由宿主按 B73 解析好传入
    pub build: Build,
}

/// 后端的构造函数：模型名与画像 → 端口表
pub type Build = fn(model: &str, profile: &Profile) -> Result<Box<dyn BackendPorts>, String>;

impl std::fmt::Debug for BackendSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BackendSpec({})", self.name)
    }
}

impl PartialEq for BackendSpec {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// 注册表：一个后端一行。
pub const REGISTRY: &[&BackendSpec] = &[&jev::SPEC, &stub::SPEC];

/// 按 `--backend` 取值找后端。
pub fn by_name(name: &str) -> Option<&'static BackendSpec> {
    REGISTRY.iter().copied().find(|s| s.name == name)
}

/// 注册后端的名字，用 `|` 连起来（报文用）。
pub fn names() -> String {
    REGISTRY
        .iter()
        .map(|s| s.name)
        .collect::<Vec<_>>()
        .join("|")
}
