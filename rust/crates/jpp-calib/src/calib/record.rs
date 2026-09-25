//! 校准记录、样本、证书与标注来源。（拆 calib.rs：原第 10–578 行，只搬不改）

use jpp_effects::profile::hash16;
use jpp_value::value::Op;
use serde::{Deserialize, Serialize};
use serde_json::json;

// 步 14a：`LiteralMode` 搬到 `jpp-ir::key`（`20` §2.3 `CalibKey` 的一维），`Sample` 搬到
// `jpp-effects::views`（运行时交给校准侧的载荷，运行时不依赖本 crate）；原路径重导出，定义不改。
pub use jpp_effects::views::Sample;
pub use jpp_ir::key::LiteralMode;

/// 记录里的证据是哪来的。**它是算出来的，不是填出来的。**
///
/// **为什么不做成一个可写字段**：Python 的 `CalibRecord.source: str = ""` 全仓
/// **零引用**——没人写、也没人读。一个自由字符串正是「给没有类型的东西补来源」那个
/// 形状：替身会把它填成让检查恰好通过的值，而纪律就在替身上成立、在真机上失效。
/// 算出来的答案填不错。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// 既没有线也没有证据
    空,
    /// 宣称了 `n` 条，却一条证据也没有——**以前分不出来的正是这一种**
    宿主手填,
    /// `n` 与实际标注条数一致
    程序积累,
    /// **只判过、没标注过**：有观察，一条标注也没有。
    ///
    /// 以前它掉进「程序积累」——因为 `n == labeled()` 在两边都是 0 时**偶然为真**，
    /// **而门正按这个判**。「算出来的答案填不错」的前提是**那个算法对**。
    /// 它也不是「混合」：混合读起来像「两种都有一些」，而这里一种都没有。
    只有观察,
    /// 两者都有且对不上
    混合,
}

/// **标签是怎么选出来的。**
///
/// 判据与 `cluster_unit` 同源：**不记「簇是怎么分的」，`n` 这个数就没有意义；
/// 不记「标签是怎么选的」，那张证书也没有意义。**
/// **一张在「两模型都同意」的子集上认证出来的证书，和一张在全体上认证出来的，
/// 不是同一个测量**——它们以前占同一个格子，只不过第二个还没被算出来。
///
/// **为什么是带载荷的枚举而不是自由字符串。** `provenance()` 那次的理由是
/// 「算出来的答案填不错」，**但这里算不出来**——标签怎么选的是系统之外的事实。
/// 枚举给的是自由字符串给不了的那一样：**`全体` 是一个要有人明确声明的断言，
/// 不是一个谁都会漂进去的默认。** 代价落在**有一种结构上全新的标签来源的人**身上，
/// 他必须来改 core——**而那正是该改的地方，因为一种新的结构改变了证书的含义**。
/// 具体判据写在 `选择子集` 的载荷里，不用改 core。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LabelSource {
    /// 标注覆盖了总体，没有选择。**这是一个断言，要有人明确说出口。**
    全体,
    /// 标注只覆盖了一个**被选出来**的子集。
    选择子集 {
        /// 按什么选的（如「两个模型都同意」）
        判据: String,
        /// 这个选择判据与「对不对」的相关性。
        /// **`None` = 没测，不是「不相关」**——与 `Tri::未测` 同一条：
        /// **一个没测过相关性的选择子集，比一个声明了 0.0 的更可疑，不是更不可疑。**
        与对错相关: Option<f64>,
    },
    /// **没人说过。** 缺失值**不许默认成「全体」**——那是替不确定说了确定，
    /// 而且方向是**乐观的那一侧**。
    #[default]
    未声明,
}

impl LabelSource {
    /// 这个来源声明本身可不可疑。**`未声明` 与「选了子集却没测相关性」都算**。
    pub fn 可疑(&self) -> bool {
        matches!(
            self,
            LabelSource::未声明
                | LabelSource::选择子集 {
                    与对错相关: None,
                    ..
                }
        )
    }
    /// 进证书地址用的短名
    pub fn addr(&self) -> String {
        match self {
            LabelSource::全体 => "全体".into(),
            LabelSource::选择子集 {
                判据, 与对错相关
            } => match 与对错相关 {
                Some(r) => format!("子集({判据},r={r:.4})"),
                None => format!("子集({判据},r=未测)"),
            },
            LabelSource::未声明 => "未声明".into(),
        }
    }
}

/// 存进记录的证书。
///
/// **`conf_delta` 不是 `CalibRecord.delta`。** 前者是二项上界的置信水平，
/// 后者是带宽 δ（喂 `delta_for`）。**两个不同的保证不共用一个名字**——
/// 「共用机制的前提是要保证的东西相同，不是听起来像同一类」。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cert {
    /// 风险目标：放行区里的假放行率上界
    pub alpha: f64,
    /// 二项上界的置信水平（**不是带宽 δ**）
    pub conf_delta: f64,
    /// 认证住的线
    pub hi: f64,
    pub n_accepted: usize,
    pub n_errors: usize,
    /// 实测上界，必须 ≤ `alpha`
    pub ucb: f64,
    /// **按什么分的簇**。不记它，`n` 这个数本身就没有意义——
    /// 它会让「73 条」读起来像 73 次独立观察，而实际独立单位可能是 19。
    pub cluster_unit: String,
    /// 簇级认证用了几次重采样、判定规则是什么。**`None` = 按条认证，没有重采样。**
    ///
    /// **这条规则是未标定的**：原型只报了「200 次里有解几次」，没有定「几次算过」。
    /// 我取「全过才算过」（说不准往拒绝那边倒），**记下来是为了它可审**，
    /// 不是因为它被标定过。
    #[serde(default)]
    pub resample: Option<(usize, String)>,
    /// **这条线是哪个代价矩阵定的**。`None` = 线由证书自己找（无代价矩阵）。
    /// 它进地址：**换代价矩阵 = 换一个测量，不是覆盖同一个**。
    #[serde(default)]
    pub cost: Option<(f64, f64)>,
    /// **这张证书界定的是哪一侧**。今天只有一个取值，而它必须是**数据不是注释**。
    ///
    /// 保形只界定**放行那一侧**的风险。另一侧既没有界、也没有出口
    /// （`commission` 置 `lo = 0.0`，于是 `p <= lo → Ignore` 实际不可达），
    /// **而 J-05 仍然强制作者为那条永不执行的分支写代码**。
    ///
    /// 置 `lo = 0.0` 的原注释说「说不准往拒绝那边倒」，**而那句话把「拒绝」默认
    /// 等同于「不给 `Act`」**。`Act` 与 `Ignore` 在出口代数里是**对称的两个判定**，
    /// 哪一个安全取决于程序怎么用——写 `if 不安全(x) { 拦下 }` 时，
    /// **`Ignore` 才是放行的那个答案**。
    ///
    /// **它今天不进地址**：只有一个取值，进去也分不出任何东西，只会把现有地址全改一遍。
    /// **哪天有双侧证书，它必须进地址**——那时两侧界不同就是两个测量。
    #[serde(default = "单侧声明")]
    pub bounded_side: String,
    /// **这张证书是在什么样的标签上算出来的**（冻结在认证那一刻）。
    ///
    /// 它**进地址**：换标签来源 = 换一个测量。不进就是后一张盖掉前一张，
    /// **那正是刚修好的覆盖那个坑换了一根轴重演**。
    ///
    /// 与 `bounded_side` **不进地址**的分界：那个今天只有一个取值，
    /// 进去分不出任何东西；这个有好几个，**不同规则同一条理由**。
    #[serde(default)]
    pub label_source: LabelSource,
    /// **这张证书是在哪一份标注集上算出来的**——`(p, label)` 对的规范形指纹。
    ///
    /// **它是算出来的，不是填的。** 与 `label_source` **必须声明**恰好相反，
    /// 而两者是同一条理由的两侧：**算得出来的不许填**（填得错），
    /// **算不出来的必须有人明确说**（标签怎么选的是系统之外的事实）。
    ///
    /// **它进地址**：两份不同的标注集是两个测量，哪怕 α、簇单位、`label_source` 全同。
    /// 与 `label_source` 的分工——**那个说「怎么选的」，这个说「是哪一份」，
    /// 互相替代不了。**
    #[serde(default)]
    pub label_fp: String,
    /// **线是怎么选出来的、在哪批数据上认证的**（B24 的多重比较要求）。
    ///
    /// `None` = 旧证书：选线与认证用的是同一批数据（二项上界只对固定线成立，
    /// 对「在同批数据上最大化后选出的线」不成立）。`Some` = 拆分样本：
    /// 选线只用选线半，认证在认证半上对选出的那一对只检验一次。
    /// **它进地址**（有值时）：同批选线与拆分认证是两个不同的测量。
    /// 为空时不序列化，旧记录与旧证书的哈希不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
    /// **认证等级**（B72）：同一个拆分认证在正式 α 下产出的证书是 `Formal`，在导入参数
    /// `alpha_trial` 下产出的是 `Trial`。等级按 α 分档、写在证书上而不是由运行时按 α 推断
    /// （正式 α 是导入参数，运行时不知道它）。`Formal` 时不序列化，旧记录与旧证书的哈希不变；
    /// 不进地址（α 已在地址里）。
    #[serde(default, skip_serializing_if = "CertGrade::is_formal")]
    pub grade: CertGrade,
    /// **有效 α 与真值基准**（B89）：只在进线真值有模型标注时写；`computed` / `human` 真值的证书为空、
    /// 不序列化（逐字节不变），不进地址。旧证书为空时由视图按真值账补算（见 `CalibStore::alpha_eff_of`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eff: Option<AlphaEff>,
}

/// 模型真值记录的有效 α（B89）：P(T 错 | A) ≤ P(L 错 | A) + P(L ≠ T | A)。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlphaEff {
    /// 相对复核基准的假放行率上界
    pub alpha_eff: f64,
    /// 这个上界的置信：复核全覆盖 1 − δ；部分覆盖 1 − δ − (1 − 复核下界置信)
    pub conf: f64,
    /// 真值基准：`model:<复核者>`（以、连接）或 `human`
    pub truth_baseline: String,
    /// 怎么算的：`full-review` / `review-in-A` / `review-over-c`
    pub basis: String,
    /// 导入时的试用 α（B89 解读 (b)，步 20c）：alpha_eff 超过它即 `Provisional`。为空（旧证书）时按
    /// `stat::ALPHA_TRIAL_DEFAULT`；为空不序列化，旧证书逐字节不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_alpha: Option<f64>,
}

/// 证书的认证等级（B72）。20a 并入 `LineGrade`（`Trial` 一行）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CertGrade {
    /// 正式 α（缺省 0.10）认证
    #[default]
    Formal,
    /// 试用 α（缺省 0.25）认证：可路由，不放行不可逆 `do`
    Trial,
}

impl CertGrade {
    pub fn is_formal(&self) -> bool {
        *self == CertGrade::Formal
    }
}

/// 选线与认证的方式（B24；B85、B86 加方法）。
///
/// `method` 的取值：`split` / `split-strata`（B24 / B75 的种子分半，旧证书）、
/// `split-stratified` / `split-strata-stratified`（B85 分层交替分半）、`fixed-sequence`（B86 固定序，
/// 不拆分）。步 20c 的 `load` 按这个名字重跑对应过程，所以重跑要用的输入都写在这里。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub method: String,
    /// 分半用的种子：样本按 `(p, 真值)` 排成规范序后，`splitmix64(seed ^ 规范序下标)` 的最低位定归属
    /// （与行序无关；2026-09-23 前用的是插入下标）。B85 只用它定各段的起点；固定序不用（记 0）
    pub seed: u64,
    /// 选线半的条数（固定序记 0）
    pub n_select: usize,
    /// 认证半的条数（固定序记全部带标注条数）
    pub n_certify: usize,
    /// 选线半上满足条件的候选线对数；固定序为两侧通过前缀里的合法线对数（只作记录，认证不依赖它）
    pub candidates: usize,
    /// 候选规则与平局规则的版本名（`fixed-sequence/v1`、`split-stratified/v1`）。为空 = 旧证书，
    /// 不序列化、不进地址，旧记录逐字节不变
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// 固定序的步长 s（解析后的整数，不是「缺省」：池大小变了缺省也会变，重跑要当时那个数）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    /// 生成候选与判区用的带宽 δ（与 `cut` 同源：记录的 `delta` 或画像的 δ）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    /// 固定序两侧生成的候选数 `(上, 下)`；K 元单侧线下侧记 0
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<(usize, usize)>,
    /// 固定序两侧通过前缀的长度 `(上, 下)`，即第一个不通过的候选的下标（全部通过时等于生成数）。
    /// 检验过的候选数 = min(stop_index + 1, generated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_index: Option<(usize, usize)>,
    /// 序贯认证（B87）的过程记录；其余方法为空，不序列化
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequential: Option<SeqCert>,
}

/// 序贯认证（B87，步 20h）写进证书的全部输入与停时状态：步 20c 的 `load` 凭它与标注集重跑。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeqCert {
    /// 到达顺序：`random`（带标注样本规范序后按种子置换）或 `two-ends`（B88 待标清单的顺序）
    pub order: String,
    /// 置换或组内随机用的种子
    pub seed: u64,
    /// 每批条数：每批后过一遍固定序
    pub batch: usize,
    /// 混合 e 过程的权重，对应备择 p₁ ∈ {0, α/4, α/2, 3α/4}
    pub weights: [f64; 4],
    /// 覆盖目标 τ；为空 = `settled`（停在再标也不会更宽处）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_target: Option<f64>,
    /// 停时已处理的到达条数（之后到达的已标样本不参与判定）
    pub stopped_at: usize,
    /// 抽样框读数（排序后）：候选由它生成，覆盖按它算
    pub pool: Vec<f64>,
    /// 到达顺序：每条是规范序（按 `(p, 真值)`）带标注样本的下标
    pub arrival: Vec<usize>,
    /// 停时上侧、下侧各候选的混合 E 与已处理条数（K 元单侧线下侧为空）
    pub e_upper: Vec<f64>,
    pub n_upper: Vec<usize>,
    pub e_lower: Vec<f64>,
    pub n_lower: Vec<usize>,
}

/// 标注集指纹：`(p, label)` 对的**规范形**，排序后取 `canon` 再 sha256 前 16 位。
/// **对写入顺序不敏感**——同一批对、不同顺序，指纹相同。
pub(super) fn 标注集指纹(样本: &[&Sample]) -> String {
    let mut pairs: Vec<(String, u8)> = 样本
        .iter()
        .filter_map(|s| Some((format!("{:.10}", s.p?), s.label?)))
        .collect();
    pairs.sort();
    hash16(&json!(pairs))
}

/// `null` → 空表。见 `CalibRecord::samples` 的注释。
fn null当空表<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Sample>, D::Error> {
    Ok(Option::<Vec<Sample>>::deserialize(d)?.unwrap_or_default())
}

/// **`phys` 反查题型**。`Op::phys()`（`value.rs:133`）是 1:1 的，所以反查是全的；
/// **但 `absorb` 收任何 `phys` 字符串，认不得的不猜**——返回 `None`，
/// 由调用方决定那意味着什么（这里意味着 `unsure_rate` 测不出来，于是不写）。
pub(super) fn 反查题型(phys: &str) -> Option<Op> {
    match phys {
        "noul" => Some(Op::Test),
        "choice" => Some(Op::Select),
        "score" => Some(Op::Measure),
        _ => None,
    }
}

/// **标注集上实测的 unsure 率**：在刚认证下来的这条线上，有多大比例的标注样本会落进 unsure。
///
/// **判据逐字抄 `cut` 的出口路由**（`interp.rs`），按题型分开：
/// - noul：`p >= hi + δ` → `Act`，`p <= lo - δ` → `Ignore`，**其余是 `Unsure("band")`**；
/// - choice：置换没测（`mode_share` 为空）→ `Unsure(untested)`，测了不一致（< 1）→ `Unsure(tie)`，
///   一致且 `p >= hi` → `Pick`，其余 `Unsure(band)`；**没有 δ，也没有低侧出口**；
/// - score：`p >= hi` → `At`，其余 `Unsure(band)`。
/// **这一条是它有意义的全部理由**——测的必须是消费方真的会碰上的那个事件，
/// 而不是一个长得像它的量。（Codex 评审 PR #27：原先对 choice / score 也套 noul 的判据。）
///
/// **算不出就不写**（题型认不得，或 noul 没有 δ）：**`None` 的既有含义是「未知，按 1 计最保守」**，
/// 而写一个不知道在哪条判据上测的数进去，比空着更糟。
/// **哪条路走得到那个分支**：内核自己写的样本 `phys` 一律来自 `Op::phys()`，**走不到**；
/// 走得到的只有**手写的 `--calib` JSON**（`absorb` 收任何 `phys` 字符串）。不是死代码。
///
/// **δ 的时效**：noul 的率测在认证那一刻的 δ 上。档案换了、δ 变了，这个率就不再成立；
/// 所以认证时把 δ 记进 `unsure_rate_delta`，由 [`CalibStore::usable_unsure_rate`] 核对。
/// （原先这里说「账本头的 `profile_hash` 会让 `W-header` 响」——新账本没有旧头，那条载体接不住。）
pub(super) fn 经验unsure率(
    按条: &[(f64, bool)],
    众数: &[Option<f64>],
    op: Option<Op>,
    hi: f64,
    lo: f64,
    delta: Option<f64>,
) -> Option<f64> {
    if 按条.is_empty() {
        return None;
    }
    let u = match op? {
        Op::Test => {
            let d = delta?;
            按条
                .iter()
                // 与 cut 同一边界比较（步 15d-2）
                .filter(|(p, _)| {
                    !jpp_value::stat::decided_up(*p, hi, d)
                        && !jpp_value::stat::decided_down(*p, lo, d)
                })
                .count()
        }
        // 依据：B63（K 元划分出口 p_max ≥ hi + δ；与 `cut` 同一判据）
        Op::Select => {
            let d = delta.unwrap_or(0.0);
            按条
                .iter()
                .enumerate()
                .filter(|(i, (p, _))| match 众数.get(*i).copied().flatten() {
                    Some(ms) if ms >= 1.0 => !jpp_value::stat::decided_up(*p, hi, d),
                    _ => true,
                })
                .count()
        }
        Op::Measure => {
            let d = delta.unwrap_or(0.0);
            按条
                .iter()
                .filter(|(p, _)| !jpp_value::stat::decided_up(*p, hi, d))
                .count()
        }
    };
    Some((u as f64 / 按条.len() as f64 * 10000.0).round() / 10000.0)
}

pub(super) fn 单侧声明() -> String {
    "证书只界定 p ≥ hi 一侧的假放行；p ≤ lo 一侧没有界，且 lo = 0 使该出口实际不可达".to_string()
}

/// 校准记录（§2.3）：线只从这里来。状态「冷」→ cut 给 Unsure(cold)。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalibRecord {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    pub status: String,
    /// 覆盖档案 δ；None = 用档案里这种题式的 δ
    #[serde(default)]
    pub delta: Option<f64>,
    /// 标注集上实测的 unsure 率（J-10 的联合上界用）；None = 未知，按 1 计最保守。
    ///
    /// **生产者是 `commission`**（见 [`经验unsure率`]）：认证成功时在刚定下来的线上
    /// 按 `cut` 的出口判据逐条数一遍。**在 2026-09-21 之前它没有生产者**——
    /// 只有 `set_unsure_rate`（Rust API）与 `--calib` 装载器（读 JSON 里已经填好的数），
    /// 于是走真实路径的记录这个字段恒为 `None`，**J-10 的界在真实路径上恒等于 `n`**。
    #[serde(default)]
    pub unsure_rate: Option<f64>,
    /// **`unsure_rate` 是在哪条 δ 上测的**（只有 noul 记录的率依赖 δ）。
    /// 装载时换了档案、δ 变了，这个率就不再描述 `cut` 实际的出口；
    /// [`CalibStore::usable_unsure_rate`] 见到对不上就当未知（按 1 计）。
    /// `None` = 手填的率或旧记录，照旧采信；为空时不序列化，旧记录哈希不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsure_rate_delta: Option<f64>,
    /// 保形集 id（J-16：fit 的训练集 ≠ 保形集）
    #[serde(default)]
    pub set_id: String,
    /// **标注集 id**。必须给出且 ≠ `set_id`——否则**代价线与保形线同源**，
    /// 那是 J-16 同一条纪律：拿训练数据给自己打分，过线的那条线不再是独立证据。
    #[serde(default)]
    pub label_set_id: String,
    /// 标注集的**去处**（给人看的定位，不是检查依据）。
    /// **检查只信指纹**——去处解析不了是「这台机器上没有那份数据」，不是「检查失败」。
    #[serde(default)]
    pub label_locator: String,
    /// **声明的**标注集指纹。非空时 `commission` 会核它与实际用到的那批对不对得上。
    #[serde(default)]
    pub label_fp: String,
    /// **这条记录的标签是怎么选出来的**（见 [`LabelSource`]）。
    /// 证书在认证那一刻把它冻进 `Cert.label_source`——**后来改声明不追改已发的证书**，
    /// 与「线重算之后已经发出的出口不改」同一条。
    #[serde(default)]
    pub label_source: LabelSource,
    /// **这条线凭什么上岗**（`12` §2.3「保形分位数（含代价矩阵的风险控制）」）。
    ///
    /// `None` 有两种读法，靠 [`CalibRecord::provenance`] 区分：宿主手填的线**本来就没有证书**
    /// （I4：线来自程序之外，宿主为它负责），而程序积累的证据**没证书就上不了岗**。
    ///
    /// **两条记录可以有一模一样的 `hi`/`lo`，背后却是两张不同的证书**——一张 α=0.10
    /// 一张 α=0.40，一张按簇取一张按条取。所以它必须跟着进 `calib_hash`：
    /// **只哈希线，就覆盖了线、没覆盖线的凭据。**
    /// **按 `(α, conf_delta, 簇单位)` 寻址**，不是一格覆盖一格。
    ///
    /// 实测过的坑：同一个键认证两次，α=0.60 线 0.195 → α=0.80 线 **0.000**
    /// （**从「过线才放行」变成「全放行」**），而被覆盖过的记录与只认证过一次的记录
    /// **逐字段相同**。**哈希是封条，不是地址**——它答「变了没有」，不答「这是哪一批
    /// 材料、哪个 α 上的」。**要防的不是篡改，是误用与合并**：没有东西被改，
    /// 是**两个不同的测量占了同一个格子**。
    ///
    /// 限定进了地址，取值不同就是不同的键，于是**并存而不是覆盖**。
    #[serde(default)]
    pub certs: std::collections::BTreeMap<String, Cert>,
    /// 运行期积累的观察（与 Python `CalibRecord.samples` 同位）。
    ///
    /// **Python 那边写 `null` 表示「没有标注集」**，这里映射成空表。
    /// 之所以敢合并这两者：**Python 自己的消费方 `runtime.py:1149` 是 `if not rec.samples`,
    /// `None` 与 `[]` 走同一条路**——合并的是一个本来就没有行为差别的区分。
    /// **带标注的那些才是 `n` 的来源**；无标注的只是观察。
    #[serde(default, deserialize_with = "null当空表")]
    pub samples: Vec<Sample>,
    /// **真值通道的账**（B19，`calib-import` 写）：标注来源计数、弃权、人工抽检与上岗门的结论。
    /// `None` = 这条记录不是经真值通道来的；**序列化时不出现**，老记录的哈希不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth: Option<TruthSummary>,
    /// **下侧证书**（真值通道写，镜像认证）：`certs` 只界定 `p ≥ hi` 一侧的假放行，
    /// 且把 `lo` 置 0（Ignore 不可达）。这张证书在同一批标注上界定 `p ≤ lo` 一侧的
    /// **漏放行**（真值为真却给 Ignore），过了才把 `lo` 抬起来。`None` = 没做过，老行为。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<Cert>,
    /// 认证集范围（B24 补充）。`None` = 老记录或非真值通道记录；序列化时不出现
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<CalibScope>,
    /// **夹具记录**（B29）：由宿主 `put` 写入的测试记录。`false` 时不序列化，老记录哈希不变。
    /// 夹具线给出的出口带 `W-fixture-line`，不得作为放行不可逆 `do` 的可信合取项。
    /// 认证程序（`commission*`）上岗时清掉这一位。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixture: bool,
    /// **重跑分歧检验**（B28 / B9）：该键上重跑的错误是否被检验为独立（`Some(true)` = 通过）。
    /// 只有通过时，重复读数与 `band → 重跑` 才能当降错手段；`None` = 没测过，按持久错误对待。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_independent: Option<bool>,
    /// **类记录的来源计数**（B75）：来源身份（题式键或手写题面哈希）→ 进线条数。
    /// 只有类记录写；空时不序列化，旧记录哈希不变。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sources: std::collections::BTreeMap<String, u64>,
    /// **带标注样本的材料指纹**（B68；PR #31 P1）：真值通道导入带 `text` 的行时逐条追加。
    /// 条数等于带标注样本数时，认证范围指纹才由它们算出（与认证用同一批样本）；不等说明有样本
    /// 没有文本（旧样本或本批部分行缺文本），不写指纹。只是一个多重集，与 `samples` 不按位置对应。
    /// 空时不序列化，旧记录哈希不变。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_fps: Vec<[f64; 7]>,
    /// **题类**（B76；步 12e-2 移到 20a-1）：认证集进线行的基础题类（`question_kind(op, request,
    /// 未知槽形, 题式声明)`），各行一致时由 `calib-import` 写，不一致（混合来源的类记录）时不写。
    /// 分类字段，与 `sources` 并列：**不进键，不进 `calib_hash`**（它是 `op`、`request`、槽声明的函数，
    /// 不携带新信息，B76）。为空时不序列化，旧记录逐字节不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<jpp_ir::question_kind::QuestionKind>,
    /// **被重跑写回替换下来的旧证书**（B117 (b)，步 20c）：`load` 重跑复现后把新证书写回，旧证书（含下侧证书）
    /// 原样追加在这里，不删（「后来改声明不追改已发的证书」的形式要求）。为空时不序列化，旧记录逐字节不变。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub certs_history: Vec<Cert>,
}

/// 真值通道对一个校准键的结论（B19 / B13）。
///
/// **`gate` 是算出来的**：模型标注只有在同键有人工抽检、且一致率不低于门槛时，
/// 才能单独撑起上岗；否则记录停在「待真值」，`gate` 写明「待核」及原因。
/// 门槛是导入时的参数（画像或命令行给），**不写死在规则里**。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TruthSummary {
    /// 按来源计的带真值条数（`human` / `computed` / `model:<名>`）
    pub sources: std::collections::BTreeMap<String, u64>,
    /// 标注者标了「模棱两可」的条数：**不进线**
    pub ambiguous: u64,
    /// `ambiguous / 全部标注行`。高 = 题面外延可能未定（B13）
    pub abstain_rate: f64,
    /// 人工抽检：同一条材料既有人工标注又有模型标注时的一致率
    #[serde(default)]
    pub spot_check: Option<SpotCheck>,
    /// 本次导入用的抽检门槛
    pub spot_check_min: f64,
    /// `上岗` / `待核：<原因>` / `认证不过：<原因>`
    pub gate: String,
    /// 导入批次（标注文件名等），进 `label_set_id`
    pub batch: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpotCheck {
    pub batches: Vec<String>,
    pub n: u64,
    pub agree: u64,
    pub rate: f64,
    /// 一致率的单侧置信下界（B19 修正按它判门槛）；老记录没有
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<f64>,
    /// 下界的置信水平
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conf: Option<f64>,
    /// 模型复核者（B36：`model:<名>` 带批次号的复核行）。只有人工复核时为空、不序列化，
    /// 所以人工抽检批次的记录逐字节不变。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_reviewers: Vec<String>,
}

/// 认证集的范围（B24 补充，本版只写来源；风格指纹与范围外告警待做）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CalibScope {
    /// 进线的标注批次
    pub batches: Vec<String>,
    /// 按来源计的带真值条数
    pub sources: std::collections::BTreeMap<String, u64>,
    pub note: String,
    /// 认证集的材料指纹范围（B68）。`None` = 认证集没带材料文本：范围未知，出口照常路由、不放行
    /// （B104-2，步 20h-1 起；此前按范围内处理）。为空时不序列化，旧记录哈希不变。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<jpp_value::stat::ScopeRanges>,
    /// 指纹由认证集里多少条带文本的样本给出（B104-2）；全部带文本时为空、不序列化
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_text: Option<usize>,
    /// **范围扩展**（B91，步 20d-2）：新风格材料上只验已有线对、按 α 分档认证过的风格指纹。落在扩展范围内的
    /// 材料不算范围外；出口等级取记录等级与扩展等级之低者。为空时不序列化，旧记录逐字节不变。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ScopeExtension>,
}

/// 一条范围扩展（B91）：字段集照裁定 `{fingerprint, n_up, n_down, alpha, batch}`。
/// 扩展等级不另存：`alpha` 大于记录选中证书的 α 即试用，否则与记录同级。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScopeExtension {
    pub fingerprint: jpp_value::stat::ScopeRanges,
    /// 上侧已决条数（零错或上界 ≤ α）
    pub n_up: usize,
    /// 下侧已决条数（K 元记录没有下侧，记 0）
    pub n_down: usize,
    /// 所过的 α（正式或试用）
    pub alpha: f64,
    /// 扩展批次（标注文件名）
    pub batch: String,
}

impl Cert {
    /// 这张证书的**地址**：限定进键，取值不同 → 键不同 → 并存。
    pub fn addr(&self) -> String {
        let c = match self.cost {
            Some((fp, fn_)) => format!("\u{1f}cost({fp},{fn_})"),
            None => String::new(),
        };
        // **不用 `{:.4}`**：那会让第五位小数不同的两张证书拿到同一个地址，
        // 而 `certs` 是 `BTreeMap<addr, Cert>`——**后写的静默覆盖先写的**，
        // 正是这套寻址要消除的那件事。今天 α 全是调用方写的字面量（实测全集：
        // 0.01 / 0.10 / 0.35 / 0.40 / 0.45 / 0.60 / 0.80），**所以撞不到**；
        // 但 α 是公开 API 的参数，**「调用方传一个算出来的 α」是完全正常的事**。
        // `{:?}` 给 f64 的往返精度，一行换掉一颗按取值决定生死的雷。
        //
        // **`cluster_unit` 与 `label_source.addr()` 里都有自由文本**
        // （后者含 `判据: String`，现值「两个模型都同意」）。**文本里若含 `\u{1f}` 就撞地址。**
        // 所以自由文本那两段先哈成定长，**分隔符就再也不可能出现在段内**。
        let sel = match &self.selection {
            Some(x) => {
                // 规则版本、步长、δ 只在有值时追加（B85/B86 的新证书），旧证书地址不变
                let rule = match &x.rule {
                    Some(r) => format!(
                        "\u{1f}rule({},step={:?},δ={:?})",
                        hash16(&json!(r)),
                        x.step,
                        x.delta
                    ),
                    None => String::new(),
                };
                format!(
                    "\u{1f}sel({},{},{},{}){rule}",
                    x.method, x.seed, x.n_select, x.n_certify
                )
            }
            None => String::new(),
        };
        format!(
            "α={:?}\u{1f}δ={:?}\u{1f}{}{c}\u{1f}{}\u{1f}fp={}{sel}",
            self.alpha,
            self.conf_delta,
            hash16(&json!(self.cluster_unit)),
            hash16(&json!(self.label_source.addr())),
            self.label_fp
        )
    }
}

impl CalibRecord {
    /// **这条线是不是夹具线**（B29）：`put` 写的测试记录，或者根本没有一张证书撑着。
    /// 没有证书 = 不是认证程序从带真值样本产出的，按 J-03（同约束宿主）不算凭据。
    pub fn fixture_line(&self) -> bool {
        self.fixture || self.选中的证书().is_none()
    }
    /// **选中的那张证书：α 最小的一张。**
    ///
    /// α 越小 = 风险目标越严 = 线越高 = 放行越少 = **越保守**。取最小的那张，
    /// 于是**后认一个更松的 α 永远不会把线放宽**——覆盖那个坑从规则上就不存在了。
    /// 并列时按地址取第一个，保证同一份记录每次选出同一张。
    pub fn 选中的证书(&self) -> Option<&Cert> {
        self.certs.values().min_by(|a, b| {
            // 先按 α：越小 = 风险目标越严 = 越保守
            a.alpha
                .partial_cmp(&b.alpha)
                .unwrap_or(std::cmp::Ordering::Equal)
                // **同 α 时取线更高的那张。** 代价矩阵进来之后这一格才有内容：
                // 一张 `fn` 重的证书（线 0.385、放行 63）和一张 `fp` 重的（线 0.780、放行 6）
                // 可以在同一个 α 上都认得住，**按地址字符串挑就可能挑中宽松的那张**。
                // 线更高 = 放行更少 = 往拒绝那边倒。
                .then_with(|| b.hi.partial_cmp(&a.hi).unwrap_or(std::cmp::Ordering::Equal))
                // 最后按地址定死，保证同一份记录每次选出同一张
                .then_with(|| a.addr().cmp(&b.addr()))
        })
    }
    /// 积累了多少条观察（**含无标注的**）
    pub fn observations(&self) -> usize {
        self.samples.len()
    }
    /// 其中有真值的有多少条——**只有这些能撑起一条线**
    pub fn labeled(&self) -> usize {
        self.samples.iter().filter(|s| s.label.is_some()).count()
    }
    /// **这条线让哪些出口种类不可达**（`"act"` / `"ignore"`），**算出来的不是填的**。
    ///
    /// `p <= lo` 在 `lo = 0` 时只有 `p` 恰为 0 才成立，`p >= hi` 在 `hi = 1` 时同理
    /// ——那一侧的出口实际上没有了，**而 J-05 仍然强制作者为它写一臂**。
    pub fn 不可达出口(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.hi >= 1.0 {
            v.push("act");
        }
        if self.lo <= 0.0 {
            v.push("ignore");
        }
        v
    }
    /// 证据来源：**算出来的**。见 `Provenance`。
    pub fn provenance(&self) -> Provenance {
        // **按 `labeled()` 显式分档**，不靠一个在 0 上偶然成立的等式。
        match (self.n, self.labeled(), self.observations()) {
            (0, _, 0) => Provenance::空,
            (n, _, 0) if n > 0 => Provenance::宿主手填,
            // **`只有观察` 要求 `n == 0`。**
            //
            // 原来写的是 `(_, 0, obs) if obs > 0`——于是「宿主手填了 n=73 的线、
            // 程序又跑出 1 条无标注观察」被判成 `只有观察`，**把宿主那 73 条说没了**。
            // **这一格固定观察测不到**：测试要么只 `put`、要么只 `absorb`，
            // **而真机的工作流天然是两者叠加**（有线的键上跑程序）。
            // E-JPP-LIVE 第一次真机跑完折证据时撞出来的。
            (0, 0, obs) if obs > 0 => Provenance::只有观察,
            (n, l, _) if n as usize == l => Provenance::程序积累,
            _ => Provenance::混合,
        }
    }
}
