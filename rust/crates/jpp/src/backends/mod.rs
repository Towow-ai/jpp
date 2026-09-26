//! 真机端口（`20` §2.3 L6 `backends/<name>/`，B74）。步 14a 自 `jpp-core::effects` 原样搬来；
//! 只有 `ureq` 由 feature `live` 引入（门控与搬之前相同）。步 15b 起 `JevPorts`（判断端口 `JevPort` 加两个占位端口）是真机端口表。
//! 模块约束（`评估①裁定` §十第 12(c) 条，`deps.py` 核）：不引用 `crate::store`、`crate::session`。
//!
//! 步 15g-0：后端注册表（`20` §五 S1「第二个判断器 = `backends/<name>/` 一个模块目录 + 注册 1 行」）。
//! 宿主（`jpp::cli`）的 `--backend` 取值、报告 `mode` 文案、默认模型、B73 画像必带与传输超时告警都从这里取，
//! 不再按某个后端的名字写死。

pub mod claude_p;
pub mod jev;
pub mod stub;

use crate::effects::{EffectPort, Ports, Profile};

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

/// 生成器画像的 `gen` 分表（B149；`20` v2 §五 S12）：只有调度与预算输入（B37），缺字段 = 未测。
/// 生成器画像与判断器画像是两份文件；哈希本步只进报告 `gen_backend` 与 stderr（账本头没有这一位，
/// 过程记录 `工程-步15h-1.md` Q2）。
#[derive(Clone, Debug, PartialEq)]
pub struct GenProfile {
    /// 画像文件内容的哈希
    pub hash: String,
    /// 单次调用费用（美元）；必填（B73 同一口径：价格只从画像来）
    pub cost_usd_per_call: f64,
    pub latency_p95_ms: Option<f64>,
    pub concurrency: Option<usize>,
    /// 子进程超时（秒）；必填（宿主策略值，画像里注明）
    pub timeout_s: f64,
    pub failure_kinds: Vec<String>,
    pub empty_rate: Option<f64>,
    /// 输出 taint 声明；缺省 `untrusted`（B149）。`inherit` 只对确定性枚举器成立，生成器端口不收。
    pub taint_out: crate::Taint,
}

impl GenProfile {
    /// 读画像 JSON 的 `gen` 分表。`taint_out` 只收 `untrusted`、`trusted`。
    pub fn load(bytes: &[u8]) -> Result<GenProfile, String> {
        let j: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("生成器画像不是 JSON：{e}"))?;
        let g = j
            .get("gen")
            .ok_or("生成器画像缺 gen 分表（B149：gen 分表写调度与预算输入）")?;
        let taint_out = match g.get("taint_out").and_then(|v| v.as_str()) {
            None | Some("untrusted") => crate::Taint::Untrusted,
            Some("trusted") => crate::Taint::Trusted,
            Some(other) => {
                return Err(format!(
                    "生成器画像 gen.taint_out = {other}：只收 untrusted 或 trusted（B149）"
                ));
            }
        };
        let need = |k: &str| {
            g.get(k)
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("生成器画像缺 gen.{k}（必填，不回退代码兜底，B73）"))
        };
        Ok(GenProfile {
            hash: jpp_ir::key::hash_of(&["gen-profile", &String::from_utf8_lossy(bytes)]),
            cost_usd_per_call: need("cost_usd_per_call")?,
            latency_p95_ms: g.get("latency_p95_ms").and_then(|v| v.as_f64()),
            concurrency: g
                .get("concurrency")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize),
            timeout_s: need("timeout_s")?,
            failure_kinds: g
                .get("failure_kinds")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            empty_rate: g.get("empty_rate").and_then(|v| v.as_f64()),
            taint_out,
        })
    }
}

/// 生成器注册表里的一行（B149；`20` v2 §五 S12：新增生成器 = `backends/<gen>/` 一个模块目录 + 注册 1 行）。
pub struct GenSpec {
    /// 生成器名（报告 `gen_backend.name`）
    pub name: &'static str,
    /// 画像文件名（`--profiles-dir` 或可执行文件旁 `profiles/` 下）
    pub profile_file: &'static str,
    /// 按模型名与画像建 `gen` 端口
    pub build: fn(model: &str, profile: &GenProfile) -> Box<dyn EffectPort>,
}

/// 生成器注册表：一个生成器一行。
pub const GENERATORS: &[&GenSpec] = &[&claude_p::SPEC];
