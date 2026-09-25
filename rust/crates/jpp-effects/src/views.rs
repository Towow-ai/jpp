//! 只读视图 trait（`20` §2.3）：检查器、运行时经它们读校准、拟合、料库与跨运行缓存，
//! 不依赖 `jpp-calib`、`jpp-store` 的具体类型（`20` §2.2 第 4 条、§2.3 `jpp-runtime` 禁止依赖）。
//!
//! 步 9 只声明，没有实现者。签名只用今天已存在的类型：校准键仍是字符串（结构化 `CalibKey` 在步 20a），
//! `ReadingMeta` 尚不存在，所以 `CalibView` 按键查而不是按读数元数据构键（S7 在步 11 落位时再收紧）。
//! 与 `20` §2.3 目标签名的逐处差异记在 `地基/过程记录/工程-步9.md`。

use jpp_ir::key::CacheKey;
use jpp_value::value::Mat;

/// 一张证书的只读视图：桥用它选代价线、判凭据、查标签来源（步 11b）。
#[derive(Clone, Debug, PartialEq)]
pub struct CertView {
    /// 风险目标：放行区里的假放行率上界
    pub alpha: f64,
    /// 认证住的线
    pub hi: f64,
    /// 这条线由哪个代价矩阵定；`None` = 无代价矩阵
    pub cost: Option<(f64, f64)>,
    /// 标签来源可疑（未声明怎样选的标签）：强出口建在它上面要留痕
    pub label_source_suspicious: bool,
    /// 标签来源的描述（告警原文用）
    pub label_source: String,
    /// 试用 α 认证的证书（B72）：出口可路由，不放行不可逆 `do`
    pub trial: bool,
    /// 认证半上的已决条数（上侧；`W-trial-line` 文本用）
    pub n_accepted: usize,
    /// 认证时用的带宽 δ（证书 `selection.delta`；候选 B104）。`None` = 旧证书，`cut` 用运行期 δ
    pub delta: Option<f64>,
    /// 这张证书的线是否按认证带宽平移过（hi = h − δ）：拆分、固定序、序贯认证是，`certify` / 代价线不是
    /// （它们的线就是被检验区的边，任何 δ ≥ 0 的判区都在被检验区内）。平移过而 `delta` 缺 → δ 未知（B104-1）
    pub delta_shifted: bool,
    /// 有效 α（B89）：模型真值记录相对复核基准的假放行率上界；非模型真值等于 `alpha`。
    /// 大于 `alpha` 时 `trial` 为真（不放行不可逆 `do`）
    pub alpha_eff: f64,
    /// 有效 α 超过试用 α（B89 解读 (b)，步 20c）：等级 `Provisional`（路由、不放行），此时 `trial` 为假
    pub provisional: bool,
}

/// 一条校准记录的只读视图：`cut` 判序与告警需要的全部字段（步 11b 定全，`20` §2.3）。
/// 记录类型在 `jpp-calib`；运行时只经这个视图读，不依赖记录类型。
#[derive(Clone, Debug, PartialEq)]
pub struct Lookup {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    /// 「上岗」「停岗候选」「停岗」「冷」「待真值」
    pub status: String,
    /// 记录的 δ；`None` = 用画像的 δ
    pub delta: Option<f64>,
    /// 宿主 `put` 写的夹具记录（J-03：不算放行不可逆 `do` 的可信合取项）
    pub fixture: bool,
    /// 保形集标识（J-16：与 fit 训练集不相交）
    pub set_id: String,
    /// 真值通道的门控文本；`None` = 未经真值通道
    pub truth_gate: Option<String>,
    /// 全部证书（按记录内地址顺序）
    pub certs: Vec<CertView>,
    /// 选中的那张证书（α 最小；同 α 取线更高者），见 `jpp-calib` 的 `选中的证书`
    pub selected: Option<CertView>,
    /// 重跑分歧检验：`Some(true)` = 错误独立，`band → 重跑` 可启用（B9/B28）
    pub rerun_independent: Option<bool>,
    /// 认证范围的材料指纹（B68）；`None` = 不核范围
    pub scope: Option<jpp_value::stat::ScopeRanges>,
    /// 指纹只由这么多条带文本样本给出（B104-2 子集指纹）；全部带文本时为空
    pub scope_n_text: Option<usize>,
    /// 范围扩展（B91，步 20d-2）：（扩展指纹，是否试用级）。主指纹外的材料落进某条扩展即不算范围外
    pub scope_extensions: Vec<(jpp_value::stat::ScopeRanges, bool)>,
}

impl Lookup {
    /// 这条线是不是夹具线（B29）：夹具记录，或没有一张证书撑着。
    pub fn fixture_line(&self) -> bool {
        self.fixture || self.selected.is_none()
    }
}

/// 校准库的只读视图。`jpp-calib::CalibStore` 实现它；运行时与 `strength` 只经它读校准（步 11b）。
///
/// 与 `20` §2.3 目标签名 `lookup_for(&ReadingMeta)` 的差距：校准键仍是字符串（结构化 `CalibKey`
/// 在步 20a）；查找链的各级记录由校准侧 `chain` 一次给出（桥不构造键，S7），选哪一级仍由桥按状态判；
/// 类键一级（B34）步 20b 接入。
pub trait CalibView {
    /// 按校准键查记录；库里没有该键时 `None`。
    fn lookup(&self, key: &str) -> Option<Lookup>;
    /// 同上，库里没有时给冷记录（与 `CalibStore::get` 同口径：线是缺省值，不是线）。
    fn line(&self, key: &str) -> Lookup;
    /// 校准库的哈希（进账本头，J-18）
    fn hash(&self) -> String;
    /// 该键可用于 J-10 的未决率（只认上岗、且认证时的 δ 与现在一致）
    fn unsure_rate(&self, key: &str) -> Option<f64>;
    /// **`cut` 用的 δ**（步 15d-2，`20` §3.9「桥用的 δ 只从校准记录取」；B104 max 分支与 `W-delta-mismatch` 退役）：
    /// 1. 选中证书记了认证带宽（`selection.delta`）→ 它；
    /// 2. 选中证书不按 δ 平移（`certify` 线、代价线：线就是检验区的边）→ 0（批量裁定解读 (a) 的直接推论）；
    /// 3. 记录自带 δ（夹具线、`set_delta`、`load` 写回）→ 它；
    /// 4. 否则 `None`：出口 `Unsure(untested)`，载体 `Delta`。
    fn line_delta(&self, rec: &Lookup) -> Option<f64> {
        if let Some(c) = rec.selected.as_ref() {
            if let Some(d) = c.delta {
                return Some(d);
            }
            if !c.delta_shifted {
                return Some(0.0);
            }
        }
        rec.delta
    }
    /// 这次运行加载的能力画像
    fn profile(&self) -> &crate::profile::Profile;
    /// 该键的无标签漂移报告（参照 = 带标注样本，近期 = 无标注样本）
    fn drift(&self, key: &str) -> Option<jpp_value::stat::DriftReport>;
    /// 记录全文（进账本 `calib_used`，只凭账本重放时据此补回）
    fn record_json(&self, key: &str) -> Option<serde_json::Value>;
    /// 这道读数的查找链（B44：题键 → 题式键 → 类键 → 冷；模式键不在链上）。键由校准侧构造。
    /// `key` 是 `cut` 用的有效校准键，同时是类别标签（B34）。
    fn chain(&self, key: &str, form_hash: Option<&str>) -> Chain;
}

/// 查找链上的一级：键与记录（无记录时是冷记录）。
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub key: String,
    pub rec: Lookup,
}

/// 一道读数的查找链（步 11b-2）：题级一定有；题式级在读数由题式填出时才有；
/// 类级（步 20b，B34）在键是作者写的类别标签时才有（`fit:` 键没有）。
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub question: Link,
    pub form: Option<Link>,
    pub class: Option<Link>,
}

/// 拟合记录的只读视图（`fit`）。记录类型在 `jpp-calib`，这里用关联类型，不反向依赖。
pub trait FitView {
    type Record;
    fn get(&self, fit_ref: &str) -> Option<&Self::Record>;
}

/// 料库端口：内容寻址存材料（步 18 由 `jpp-store` 实现）。
pub trait MatStorePort {
    fn put(&mut self, m: &Mat) -> String;
    fn get(&self, addr: &str) -> Option<Mat>;
    /// 该材料上做过的观察（缓存键）
    fn marks(&self, addr: &str) -> Vec<CacheKey>;
}

/// 跨运行的缓存读数（B40，步 19 启用）。
#[derive(Clone, Debug, PartialEq)]
pub struct CachedReading {
    /// 读数记录（账本条目里的原样 JSON；结构化的 `ReadingRecord` 在步 10 之后）
    pub record: serde_json::Value,
    /// 来源账本与条目
    pub source: String,
}

/// 跨运行读数查找（步 19 由 `jpp-store` 实现）。
pub trait CacheLookup {
    fn get(&self, k: &CacheKey) -> Option<CachedReading>;
}

// 步 14a 自 `jpp-calib::calib::record` 原样搬来：运行时产出、校准侧吸收的样本（`Outcome.evidence`）。
use jpp_ir::key::LiteralMode;
use serde::{Deserialize, Serialize};

/// 一条运行期观察（`12`:347 标为「未定」的**运行期写入口**的载荷）。
///
/// **与 Python `CalibRecord.samples` 的 `[[p, label]]` 同族，但带上了记账要的几样。**
/// 总控立的规矩在这里落地：**`mode_share` 不许裸记——必须和 `perms` 一起**进账本和
/// 校准记录。一个没有 `perms` 的一致率不是测量结果，是一个孤零零的小数。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// 标量概率。**`None` = 这个物理形式没有定义好的标量 `p`**（select / measure）：
    /// 强给它一个标量就是造一个不同尺的数，而「不同尺不可比」是本项目自己的判据。
    pub p: Option<f64>,
    /// 真值。**`None` = 还没有**——`cut` 切出来的读数本身从不携带真值。
    /// 这一位是 `n` 与 `observations` 分家的全部理由。
    pub label: Option<u8>,
    /// 用了几个置换（0 = 没测）
    pub perms: usize,
    /// 置换众数占比；与 `perms` 成对
    pub mode_share: Option<f64>,
    /// 字面模式（`12`:136 第五维）
    pub mode: LiteralMode,
    /// 物理形式：noul / choice / score
    pub phys: String,
    /// 这条观察属于哪个**簇**（通常是对象段）。**可交换性在我们这里不是被时间打破的，
    /// 是被材料复用打破的**——同一段落的多条读数不是多次独立观察。
    /// `None` = 这条没有簇 id，**于是它只能参与「按条」的认证**；
    /// 声明按对象段却没有簇 id 是**错，不是降级**。
    #[serde(default)]
    pub cluster: Option<String>,
    /// **分层**（B75 混合样本）：这条样本来自哪个来源（题式键，无题式的手写题取题面哈希）。
    /// 带分层的样本在拆分认证时按来源各自分半后合并。`None` 时不序列化，旧记录逐字节不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stratum: Option<String>,
}

// 步 14a 自 `jpp-calib::fit` 原样搬来：`fit` 记录（运行时经 `FitView` 读）。
use std::rc::Rc;

/// `fit` 注册表（`12`:319「每个 `FitRef` 的训练集 id、特征键、指纹种类、错误率、版本；
/// 注册约束见 §2.9」，:314「**只能训练产生**」）。
///
/// `fit` 是第七种形式之外的**桥**：它把跨题的多个读数合成**一个仍然是读数的东西**，
/// 因而仍要过线。作者不用它也能合并两道题（`cut` 出两个出口再写 `if`），
/// 但那样一来**合并这一步的不确定性就消失了**——两个 `act` 合出来的结论看着和一个 `act`
/// 一样确定，而它其实经过了一个没有校准过的函数。
pub struct FitRecord {
    /// 特征：`(校准键, 指纹种类)`，**逐项**要与输入读数相同（J-04）
    pub features: Vec<(String, String)>,
    /// 训练样本数（J-16：`n ≥ max(50, 20×特征数)`）
    pub n: u64,
    /// 训练集 id（J-16：训练集 ≠ 保形集）
    pub trained_from: String,
    #[allow(clippy::type_complexity)]
    pub f: Rc<dyn Fn(&[f64]) -> f64>,
}
