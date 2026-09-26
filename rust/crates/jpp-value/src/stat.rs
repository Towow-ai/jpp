//! 保形弃权域：**先回答「这条线能不能被认证」，再谈线定在哪**。
//!
//! **从 `foundation/experiments/conformal-proto` 原样搬过来，不重写**（总控点名）。
//! 原型那三道「空放行区不算解」的保护**一道不少地搬全了**——原型自己实测过
//! **单独去掉任何一道都不会变红，三道一起去掉才红**（`ecal.rs` 6 条里红 4 条）。
//! **少搬一道，看起来是对的，而且看不出来。**

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

/// 二项比例的**精确**上置信界（Clopper–Pearson）。
///
/// 为什么不用 Hoeffding：在我们的 n 上 Hoeffding 的松弛是
/// `sqrt(ln(1/δ)/2n)`——n=20 时 **0.24**，对任何低于 24% 的风险目标都是空的。
/// 精确二项界在小 n 上紧得多，而且这里的损失本来就是 0/1，用不着次高斯放缩。
pub fn binomial_upper(k: usize, n: usize, conf_delta: f64) -> f64 {
    // **`n == 0` 返回 1.0 是承重的，不是防御性写法。** 空放行区的风险**没有定义**，
    // 返回 0.0（「零错所以零风险」）会让「全弃权」在算术上满足任何 α，于是全弃权被
    // 报成「认证通过的线」。返回 1.0 是「说不准时往拒绝那边倒」的直接实例。
    if n == 0 || k >= n {
        return 1.0;
    }
    let log_choose = (1..=k)
        .map(|i| ((n - i + 1) as f64 / i as f64).ln())
        .sum::<f64>();
    let (mut lo, mut hi) = (k as f64 / n as f64, 1.0);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        // P(Bin(n, mid) ≤ k)
        // mid >= k/n: the largest relevant mass is at k. Starting from 0
        // can underflow even when the CDF is large (e.g. n=1000, k=900).
        let mut term = (log_choose + k as f64 * mid.ln() + (n - k) as f64 * (-mid).ln_1p()).exp();
        let mut cdf = term;
        for i in (1..=k).rev() {
            term *= i as f64 / (n - i + 1) as f64 * (1.0 - mid) / mid;
            cdf += term;
        }
        if cdf > conf_delta { lo = mid } else { hi = mid }
    }
    (lo + hi) / 2.0
}

/// 零错时要认证到 α 所需的**放行条数**：`ln δ / ln(1−α)`。
/// 这是「就算一条都不错，样本也得有这么多」的下限，与读数准不准无关。
pub fn n_needed_zero_error(alpha: f64, conf_delta: f64) -> usize {
    (conf_delta.ln() / (1.0 - alpha).ln()).ceil() as usize
}

/// 一次实验性阈值扫描的结果。**拒绝是一等出口**，不是错误。
/// `ucb` 是逐阈值二项上界；同批选线尚无选择校正，不能解释为整体风险保证。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Certificate {
    /// 认证成功：`hi` 之上放行，风险上界 `ucb ≤ α`。
    Line {
        hi: f64,
        n_accepted: usize,
        n_errors: usize,
        ucb: f64,
    },
    /// **认证失败**：任何非平凡的线都给不出 ≤ α 的上界。
    /// `best_ucb` 是这批数据上能拿到的**最紧**上界（对应最保守的非空放行区）。
    Refused {
        best_ucb: f64,
        best_hi: f64,
        best_n_accepted: usize,
        n_needed: usize,
    },
}

impl Certificate {
    /// 拒绝时该点亮的 J-15 载体名。**不新造机制**：
    /// 「这条线没被认证」与「置换没测过」「线是冷的」是同一个性质——
    /// **一个被声明为判据、但在本次路径上没有被测量的量**。
    pub fn untested_carrier(&self) -> Option<&'static str> {
        match self {
            Certificate::Line { .. } => None,
            Certificate::Refused { .. } => Some("conformal_line"),
        }
    }
    pub fn is_refused(&self) -> bool {
        matches!(self, Certificate::Refused { .. })
    }
}

/// 保形风险控制（RCPS 形状）：**从最宽的线往紧里走，取第一个上界 ≤ α 的线**。
///
/// `samples`：`(读数 p, 这条读数蕴含的判断对不对)`。损失 = 放行区里的假放行。
///
/// Experimental scan: thresholds are selected on the same samples used for
/// pointwise Clopper–Pearson bounds. No selection correction or independent
/// validation set is implemented. `Certificate` is the existing API name,
/// not a distribution-free guarantee for the selected threshold.
///
/// **空放行区不算解。** 这一条是纪律不是实现细节：`t = 1.0` 上「放行 0 条、
/// 假放行 0 条」在算术上满足任何 α，但它说的是「全弃权」——把全弃权报成
/// 「认证通过的线」，正是**结论把注意力从依据上引开**的那个形状。
pub fn certify(samples: &[(f64, bool)], alpha: f64, conf_delta: f64) -> Certificate {
    let mut pts: Vec<(f64, bool)> = samples.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut ps: Vec<f64> = pts.iter().map(|x| x.0).collect();
    ps.dedup();
    let mut cands = vec![0.0];
    for w in ps.windows(2) {
        cands.push((w[0] + w[1]) / 2.0);
    }
    // **不放 1.0**：那是全弃权，不是线。（这是三道保护的第二道，见 `binomial_upper` 的 `n == 0`。）
    let mut best: Option<(f64, f64, usize)> = None; // (ucb, hi, n_accepted)
    for t in cands {
        let acc: Vec<&(f64, bool)> = pts.iter().filter(|x| x.0 >= t).collect();
        // 第三道。**实测：单独去掉这一道不会变红**——候选表里本来就没有让放行区为空的阈值。
        // 三道一起去掉才红（tests/ecal.rs 的「空放行区不算解」，6 条里红 4 条）。
        if acc.is_empty() {
            continue;
        }
        let k = acc.iter().filter(|x| !x.1).count();
        let ucb = binomial_upper(k, acc.len(), conf_delta);
        if best.as_ref().map(|b| ucb < b.0).unwrap_or(true) {
            best = Some((ucb, t, acc.len()));
        }
        if ucb <= alpha {
            return Certificate::Line {
                hi: t,
                n_accepted: acc.len(),
                n_errors: k,
                ucb,
            };
        }
    }
    let (best_ucb, best_hi, best_n) = best.unwrap_or((1.0, 1.0, 0));
    Certificate::Refused {
        best_ucb,
        best_hi,
        best_n_accepted: best_n,
        n_needed: n_needed_zero_error(alpha, conf_delta),
    }
}

/// **无标签**漂移统计。`12`:410 那一行里唯一带「必备」二字的东西，而它今天两边都没有：
/// `jv/calib.py` 的 `drift_stat` 是声明了从不算的字段（全仓只有写死的 `0.0`），
/// `core/calib.py::should_suspend` 是另一个系统的、**要标签**的错误率监控。
///
/// 这里给的是只用读数分布的两个量——**不需要真值，所以每次运行都能算**：
/// - `ks`：两样本 Kolmogorov–Smirnov 统计量（读数分布位移）
/// - `psi`：population stability index（分桶质量迁移，工业上常用 0.1 / 0.25 两档）
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriftReport {
    pub ks: f64,
    pub psi: f64,
    pub n_ref: usize,
    pub n_recent: usize,
    /// **这个 KS 显著吗**（α=0.05 的两样本临界值）。**`false` = 不显著。**
    ///
    /// **原来这一位叫 `underpowered`，而那是个错名**：200 条对 200 条的**同分布**
    /// 数据 `ks = 0 < crit`，于是它报「功效不足」——**而真相是功效充足、没有漂移**。
    /// **「测不出来」与「测出来没有漂」被压成了同一位。**
    /// 实测：`同分布 200v200 → underpowered = true`。
    pub significant: bool,
    /// **样本小到连完全分离都不显著**——那才是真的功效不足。
    /// `ks` 的上限是 1，所以判据是 `crit > 1`，即 `1/n_ref + 1/n_recent > (1/1.36)²`。
    /// **这一位与 J-15 同族：量本身没被可信地测量。**
    pub underpowered: bool,
}

impl DriftReport {
    /// **这份报告够不够当停岗的依据。**
    ///
    /// **它不停岗**——`12`:396 写的是**告警**，停岗仍是人下的判断。它只回答
    /// 「拿这个去停岗站不站得住」。
    ///
    /// `underpowered` 时**一律不够**。当初加那一位的理由是「一次 20 条的抽样不该把一个键
    /// 停岗」；**而更硬的理由是：复岗今天不存在（`commission` 明写不经由它复岗），
    /// 所以一次假停岗是永久的。** 两种错的代价不对称，不对称的那一侧是不可逆的那一侧。
    pub fn 可停岗(&self) -> bool {
        self.significant && !self.underpowered
    }
}

pub fn drift(reference: &[f64], recent: &[f64], bins: usize) -> DriftReport {
    let ks = {
        let mut a = reference.to_vec();
        let mut b = recent.to_vec();
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        b.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let mut all: Vec<f64> = a.iter().chain(b.iter()).copied().collect();
        all.sort_by(|x, y| x.partial_cmp(y).unwrap());
        all.iter()
            .map(|t| {
                let fa = a.iter().filter(|x| *x <= t).count() as f64 / a.len().max(1) as f64;
                let fb = b.iter().filter(|x| *x <= t).count() as f64 / b.len().max(1) as f64;
                (fa - fb).abs()
            })
            .fold(0.0, f64::max)
    };
    let hist = |v: &[f64]| -> Vec<f64> {
        let mut h = vec![0.0; bins];
        for x in v {
            let i = ((x * bins as f64).floor() as usize).min(bins - 1);
            h[i] += 1.0;
        }
        let n = v.len().max(1) as f64;
        h.into_iter().map(|c| c / n).collect()
    };
    let (ha, hb) = (hist(reference), hist(recent));
    let eps = 1e-4;
    let psi = ha
        .iter()
        .zip(hb.iter())
        .map(|(a, b)| {
            let (a, b) = (a.max(eps), b.max(eps));
            (b - a) * (b / a).ln()
        })
        .sum::<f64>();
    // KS 的 α=0.05 临界值 ≈ 1.36·sqrt(1/n1 + 1/n2)；小于它就分不出移没移。
    let crit =
        1.36 * (1.0 / reference.len().max(1) as f64 + 1.0 / recent.len().max(1) as f64).sqrt();
    DriftReport {
        ks,
        psi,
        n_ref: reference.len(),
        n_recent: recent.len(),
        significant: ks >= crit,
        // **完全分离（ks = 1）都不显著，才叫功效不足**
        underpowered: crit > 1.0,
    }
}

/// 把标注集按**对象段**分簇，每簇取一条——簇级保形。
///
/// 为什么需要它：可交换性在我们这里**不是被时间打破的，是被材料复用打破的**。
/// E-CAL 那 297 条读数只由 61 个不同片段重组而成；按对象段分簇后
/// noul 36 簇 / choice 37 簇 / score 28 簇。**同一段落的多条读数不是多次独立观察。**
pub fn cluster_subsample(samples: &[(f64, bool, String)], seed: u64) -> Vec<(f64, bool)> {
    let mut by: BTreeMap<&str, Vec<(f64, bool)>> = BTreeMap::new();
    for (p, l, seg) in samples {
        by.entry(seg.as_str()).or_default().push((*p, *l));
    }
    let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    by.values()
        .map(|v| {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            v[(s >> 33) as usize % v.len()]
        })
        .collect()
}

/// **代价比线**（`12` §2.3；从 Python `calib.py::cost_line` 原样搬）。
///
/// 在标注集上找使经验代价 `fp·#误放行 + fn·#漏放行` 最小的阈值 `t`；出口 `Act` 当 `p ≥ t`。
/// 这是贝叶斯代价比线的**经验版**（校准好时二者收敛）；**保形风险控制的有限样本修正不在这里**。
/// 它只保证一件事：**线随代价矩阵移动、不由程序手写。**
///
/// **「并列取更高的 `t`」是承重的**：代价相同时取更严的那条，**宁可 unsure**。
/// 丢了它，并列时会滑向更宽松的线，而那正是「说不准往拒绝那边倒」要防的。
pub fn cost_line(samples: &[(f64, bool)], fp: f64, fn_: f64) -> Result<CostLine, String> {
    if samples.is_empty() {
        return Err("cost_line: 标注集为空，线只从记录来（I4）".into());
    }
    if fp < 0.0 || fn_ < 0.0 || fp + fn_ == 0.0 {
        return Err("cost_line: 代价必须非负且不全为 0".into());
    }
    let mut pts: Vec<(f64, bool)> = samples.to_vec();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // **这里不去重，`certify` 那边去重——两个函数的候选规则不一样，别照抄邻居的。**
    // Python `cost_line` 的 `ps` 没有 dedup，于是重复值之间的「中点」就是那个值本身，
    // **候选表里因此含有样本点本身**。我第一版顺手抄了 `certify` 的 `ps.dedup()`，
    // 线就从 0.59 掉到 0.585——放行集合恰好没变，**所以三个数里有两个仍然对得上**。
    let ps: Vec<f64> = pts.iter().map(|x| x.0).collect();
    let mut cands = vec![0.0];
    for w in ps.windows(2) {
        cands.push((w[0] + w[1]) / 2.0);
    }
    cands.push(1.0 + 1e-9);
    let (mut best_t, mut best_c): (f64, Option<f64>) = (0.0, None);
    for t in cands {
        let c: f64 = pts.iter().filter(|(p, l)| *p >= t && !*l).count() as f64 * fp
            + pts.iter().filter(|(p, l)| *p < t && *l).count() as f64 * fn_;
        // **并列取更高的 `t`**（宁可 unsure）——与 Python 逐字同序
        if best_c.is_none()
            || c < best_c.expect("已判")
            || (c == best_c.expect("已判") && t > best_t)
        {
            best_t = t;
            best_c = Some(c);
        }
    }
    let n = pts.len();
    // Keep the reject-all sentinel. Clamping it to 1 accepts scores equal to 1
    // and makes the returned acceptance counts disagree with the optimized cost.
    let line = best_t;
    let acc: Vec<&(f64, bool)> = pts.iter().filter(|(p, _)| *p >= line).collect();
    let 误放行 = acc.iter().filter(|(_, l)| !*l).count();
    Ok(CostLine {
        line,
        cost: best_c.unwrap_or(0.0),
        n,
        fp,
        fn_,
        n_accepted: acc.len(),
        n_false_accept: 误放行,
        // **放行集合为空时假放行率无定义**，不是 0——返回 `None`，与 `binomial_upper` 的
        // `n == 0 → 1.0` 同一条：空放行区上的「零错」不是证据。
        false_accept_rate: if acc.is_empty() {
            None
        } else {
            Some(误放行 as f64 / acc.len() as f64)
        },
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CostLine {
    /// Inclusive threshold; a value above 1 represents rejecting all scores.
    pub line: f64,
    pub cost: f64,
    pub n: usize,
    pub fp: f64,
    pub fn_: f64,
    /// 放行集合大小。**必须和 `false_accept_rate` 一起看**——
    /// 一个缩小的放行集合把率压低了，那不是变好。
    pub n_accepted: usize,
    pub n_false_accept: usize,
    /// 放行集合为空时是 `None`，不是 0
    pub false_accept_rate: Option<f64>,
}

// ---------------------------------------------------------------- 认证范围的材料指纹（B68）

/// 材料指纹的量名（B68 第 1 条）：只用可从材料算出的统计量，不问判断器「是不是同一风格」（P7）。
/// 顺序固定，[`material_fingerprint`] 按这个顺序给值。
pub const FP_NAMES: [&str; 7] = [
    "字符数",
    "中文比例",
    "拉丁字母比例",
    "数字比例",
    "标点空白比例",
    "行数",
    "平均行长",
];

/// 一段材料文本的指纹（B68）：字符数；中文、拉丁字母、数字、标点与空白四类字符的比例；
/// 行数与平均行长。比例的分母是字符数；空文本的比例全为 0。
pub fn material_fingerprint(text: &str) -> [f64; 7] {
    let (mut n, mut cjk, mut latin, mut digit, mut punct) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        n += 1;
        if ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c) {
            cjk += 1;
        } else if c.is_numeric() {
            digit += 1;
        } else if c.is_alphabetic() {
            latin += 1;
        } else {
            punct += 1;
        }
    }
    let lines = text.lines().count().max(1);
    let r = |k: usize| if n == 0 { 0.0 } else { k as f64 / n as f64 };
    [
        n as f64,
        r(cjk),
        r(latin),
        r(digit),
        r(punct),
        lines as f64,
        n as f64 / lines as f64,
    ]
}

/// 认证集的范围：每个指纹量在认证集上的分位区间（B68）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScopeRanges {
    /// 取区间用的分位（导入参数 `scope_quantiles`，缺省 [0.01, 0.99]）
    pub quantiles: (f64, f64),
    /// 算指纹用的材料条数
    pub n: usize,
    /// 按 [`FP_NAMES`] 顺序：（量名，下界，上界）。有 `margins` 时已含边距。
    pub ranges: Vec<(String, f64, f64)>,
    /// 区间边距（B68 修订）：尺度量乘除 k、比例量加减 m。缺省 = 旧记录，区间是裸分位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<ScopeMargins>,
}

/// 认证范围的边距参数（B68 修订）：分位区间只去离群点，同风格容差由边距给。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScopeMargins {
    /// 尺度量（字符数、行数、平均行长）：[q_lo / k, q_hi × k]
    pub k: f64,
    /// 比例量（中文、拉丁、数字、标点空白）：[q_lo − m, q_hi + m] ∩ [0, 1]
    pub m: f64,
}

impl Default for ScopeMargins {
    fn default() -> Self {
        ScopeMargins { k: 2.0, m: 0.10 }
    }
}

/// 指纹量是否尺度量（按 [`FP_NAMES`] 下标）；其余为比例量。
fn is_scale(i: usize) -> bool {
    matches!(i, 0 | 5 | 6)
}

impl ScopeRanges {
    /// 由一批材料文本算出范围。没有文本时 `None`。
    pub fn from_texts<'a>(
        texts: impl IntoIterator<Item = &'a str>,
        quantiles: (f64, f64),
        margins: Option<ScopeMargins>,
    ) -> Option<ScopeRanges> {
        let fps: Vec<[f64; 7]> = texts.into_iter().map(material_fingerprint).collect();
        Self::from_fingerprints(&fps, quantiles, margins)
    }

    /// 同 [`Self::from_texts`]，输入是已算好的材料指纹（校准记录里存的是指纹，不存文本）。
    pub fn from_fingerprints(
        fps: &[[f64; 7]],
        quantiles: (f64, f64),
        margins: Option<ScopeMargins>,
    ) -> Option<ScopeRanges> {
        if fps.is_empty() {
            return None;
        }
        let q = |xs: &mut Vec<f64>, p: f64| {
            xs.sort_by(|a, b| a.total_cmp(b));
            let i = ((xs.len() - 1) as f64 * p).round() as usize;
            xs[i.min(xs.len() - 1)]
        };
        let ranges = FP_NAMES
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let mut xs: Vec<f64> = fps.iter().map(|f| f[k]).collect();
                let lo = q(&mut xs, quantiles.0);
                let hi = q(&mut xs, quantiles.1);
                let (lo, hi) = match margins {
                    None => (lo, hi),
                    Some(g) if is_scale(k) => (lo / g.k, hi * g.k),
                    Some(g) => ((lo - g.m).max(0.0), (hi + g.m).min(1.0)),
                };
                (name.to_string(), lo, hi)
            })
            .collect();
        Some(ScopeRanges {
            quantiles,
            n: fps.len(),
            ranges,
            margins,
        })
    }

    /// 这段材料的指纹落在哪个量的区间外；都在区间内时 `None`。返回（量名，值，下界，上界）。
    pub fn outside(&self, fp: &[f64; 7]) -> Option<(String, f64, f64, f64)> {
        self.ranges
            .iter()
            .zip(fp.iter())
            .find(|((_, lo, hi), v)| **v < *lo - 1e-12 || **v > *hi + 1e-12)
            .map(|((name, lo, hi), v)| (name.clone(), *v, *lo, *hi))
    }
}

// ---------------------------------------------------------------- 序贯 e 过程（B87，步 20h）

/// 混合 e 过程的四个备择 p₁ ∈ {0, α/4, α/2, 3α/4}（B87 §二·1）。
pub fn e_alternatives(alpha: f64) -> [f64; 4] {
    [0.0, alpha / 4.0, alpha / 2.0, 3.0 * alpha / 4.0]
}

/// 一条到达样本对备择 p₁ 的似然比因子：错 × p₁/α，对 × (1 − p₁)/(1 − α)。
/// 原假设（错误率 ≥ α）下期望 ≤ 1，乘积是非负上鞅（Ville）。
pub fn e_factor(alpha: f64, p1: f64, error: bool) -> f64 {
    if error {
        p1 / alpha
    } else {
        (1.0 - p1) / (1.0 - alpha)
    }
}

/// 每个分量乘积的初值
pub const E_START: [f64; 4] = [1.0; 4];

/// 拒绝门槛 1/δ
pub fn e_threshold(conf_delta: f64) -> f64 {
    1.0 / conf_delta
}

/// 各分量乘积 `e` 再乘 r 条全对之后的混合 E。
pub fn e_mixture_after(e: &[f64; 4], weights: &[f64; 4], alpha: f64, r: usize) -> f64 {
    let p1 = e_alternatives(alpha);
    (0..4)
        .map(|m| weights[m] * e[m] * e_factor(alpha, p1[m], false).powi(r as i32))
        .sum()
}

/// 混合 e 过程零错过线所需条数：最小 n 使 Σ w_j ((1 − p_j)/(1 − α))^n ≥ 1/δ。缺省权重下正式 24、试用 9。
pub fn e_zero_error_needed(alpha: f64, conf_delta: f64, weights: &[f64; 4]) -> usize {
    (1..100_000)
        .find(|n| e_mixture_after(&E_START, weights, alpha, *n) >= e_threshold(conf_delta))
        .unwrap_or(100_000)
}

/// 混合权重合法：四个非负、和为 1。
pub fn e_weights_valid(w: &[f64; 4]) -> bool {
    w.iter().all(|x| *x >= 0.0) && (w.iter().sum::<f64>() - 1.0).abs() <= 1e-9
}

/// 覆盖：框内读数落在 p ≥ h 或 p ≤ l 的占比（无标签可算；空框为 0）。
pub fn coverage(pool: &[f64], h: Option<f64>, l: Option<f64>) -> f64 {
    if pool.is_empty() {
        return 0.0;
    }
    let k = pool
        .iter()
        .filter(|p| h.is_some_and(|h| **p >= h) || l.is_some_and(|l| **p <= l))
        .count();
    k as f64 / pool.len() as f64
}

/// 夹到 [0, 1]
pub fn clamp_unit(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------- 有效 α（B89，步 20i）

/// 一致率的单侧置信下界（Clopper–Pearson）：`1 − 不一致率的上界`。`n = 0` 时为 0（没有复核就没有下界）。
pub fn agreement_lower(agree: u64, n: u64, conf: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    1.0 - binomial_upper((n - agree) as usize, n as usize, 1.0 - conf)
}

/// 复核下界缺置信时的缺省（B19 修正：单侧 95%）
pub const REVIEW_CONF_DEFAULT: f64 = 0.95;

/// B161（步 25d）：联合界里按 1 计的分量取的值（无证书、夹具、类线、冷、范围外）；联合界本身也在这里封顶——
/// 它是概率上界 P(结果错) ≤ min(1, Σ α_i)。
pub const ALPHA_UNKNOWN: f64 = 1.0;
/// B161：`ask` 出口计入联合界的 α——人答即真值（B31）。
pub const ALPHA_HUMAN: f64 = 0.0;

/// B89 并集界：复核批次落在已决区 A 内时 `α + (1 − a_lb(A))`；不是从 A 抽的（旧记录）时
/// `α + (1 − a_lb) / c`，c 为认证集在 A 内的占比。`c = 0` 或没有复核时为 1（不放行）。
pub fn alpha_eff(alpha: f64, a_lb: f64, c: Option<f64>) -> f64 {
    match c {
        None => alpha + (1.0 - a_lb),
        Some(c) if c > 0.0 => alpha + (1.0 - a_lb) / c,
        Some(_) => 1.0,
    }
}

/// alpha_eff 的置信：全覆盖 1 − δ；部分覆盖 1 − δ − (1 − 复核下界置信)。
pub fn alpha_eff_conf(conf_delta: f64, review_conf: Option<f64>) -> f64 {
    match review_conf {
        None => 1.0 - conf_delta,
        Some(rc) => 1.0 - conf_delta - (1.0 - rc),
    }
}

/// 没有复核可依时的有效 α：1（不放行）
pub const ALPHA_EFF_NONE: f64 = 1.0;

/// 试用 α 的缺省（B72；与 `calib-import --alpha-trial` 缺省同值）。B89 解读 (b)（步 20c）：有效 α 超过试用 α 的线
/// 等级为 `Provisional`；证书没记导入时的试用 α（`AlphaEff.trial_alpha` 为空）时按这个值。
pub const ALPHA_TRIAL_DEFAULT: f64 = 0.25;

/// 旧证书（缺 δ）的候选 δ（B117 (c)，步 20c 的 `load` 重跑用）：候选阈值网格 = 带标注读数的去重值加两两中点（值种类多时相邻中点）；上侧 δ = g − hi
/// （g ≥ hi，按 12 位小数规整掉浮点减法的尾差），有下侧证书时只留 lo − δ 也落在网格上的那些。
pub fn shift_candidates(ps: &[f64], hi: f64, lo: Option<f64>) -> Vec<f64> {
    let mut v: Vec<f64> = ps.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup();
    // 拆分认证的阈值是**选线半**里相邻两值的中点，在全体读数里未必相邻（中间的值可能全落在认证半）：
    // 所以取任意两值的中点。读数值种类多时（连续读数）两两中点太多，退为相邻中点——这时选线半与全体
    // 的相邻关系几乎处处相同。上限是规则常数，不是判断器属性。
    const 两两上限: usize = 64;
    let mut grid = v.clone();
    if v.len() <= 两两上限 {
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                grid.push((a + b) / 2.0);
            }
        }
    } else {
        for w in v.windows(2) {
            grid.push((w[0] + w[1]) / 2.0);
        }
    }
    let 规整 = |x: f64| (x * 1e12).round() / 1e12;
    let 在网格 = |x: f64| grid.iter().any(|g| (g - x).abs() < 1e-9);
    let mut out: Vec<f64> = grid
        .iter()
        .filter(|g| **g >= hi)
        .map(|g| 规整(g - hi))
        .filter(|d| lo.is_none_or(|l| 在网格(l - d)))
        .collect();
    out.sort_by(|a, b| a.total_cmp(b));
    out.dedup();
    out
}

// ---------------------------------------------------------------- 已决区的边界（步 15d-2）

/// 已决区边界的往返容差。线存成 `hi = h − δ`（`lo = l + δ`），判区算 `hi + δ`：浮点下这一减一加
/// 可能差 1ulp，读数恰好等于证书的 h 时会被判成 band（27a F3 实测 5 条）。取值理由：h、δ 都在 [0, 1]，
/// 一减一加的舍入误差不超过 2ulp(1) ≈ 4.4e-16；1e-12 比它大三个数量级、比读数精度（1e-3）小九个数量级，
/// 只收回往返误差，不放宽任何真实读数（放行方向的放宽上界是 1e-12）。不改记录格式（直接存 h 要改校准
/// 记录格式，20a 后冻结，走格式步）。Python 内核 `foundation/jv/runtime.py::_decide` 用同一容差。
/// 依据：主会话 2026-09-25（15d-2 统一边界比较；容差取 1e-12 量级并写明理由）；认证、`unsure_rate`、复核已决区、
/// 范围扩展与 `cut` 共用下面两个函数。
pub const BOUNDARY_EPS: f64 = 1e-12;

/// 读数落在上侧已决区：`p ≥ h`，`h = hi + δ`（按容差比较）。
pub fn decided_up(p: f64, hi: f64, delta: f64) -> bool {
    p >= hi + delta - BOUNDARY_EPS
}

/// 读数落在下侧已决区：`p ≤ l`，`l = lo − δ`（按容差比较）。
pub fn decided_down(p: f64, lo: f64, delta: f64) -> bool {
    p <= lo - delta + BOUNDARY_EPS
}

/// 作者声明线 `evidence.near_line` 的半宽（B128；`20` v2 §4.4 第 9 条「本趟该键读数落在线 ±0.2 内的条数与占比」）。
/// 语言级的报告口径，不是判断器性质（读数抖动是画像的 δ，不在这里）；`stat` 线按该统计量自己的单位计（`expect`
/// 为档位，步 20j-3）。原在 `jpp-runtime/src/bridge.rs`，基线刷新 2026-09-26 挪入常量表（`grep_constants.口径.md`）。
pub const NEAR_LINE_WINDOW: f64 = 0.2;

/// 作者声明线的 δ：按写的数切，线附近不留 ±δ 的带（B128「不平移 δ」）。认证线的 δ 只从校准记录取（步 15d-2），
/// 声明线没有记录，这里是语言规定的 0，不是判断器性质。声明分支判序、`past_declared` 与 `evidence.errors_at_line`
/// 共用它（原为三处裸 `0.0`，基线刷新 2026-09-26 收进常量表）。
pub const DECLARED_DELTA: f64 = 0.0;

/// 声明线开端（B165 (4)）：`s` 严格越过上线，`s > hi + ε`。闭端仍用 [`decided_up`]（认证共用，不改）。
pub fn beyond_up(s: f64, hi: f64) -> bool {
    s > hi + BOUNDARY_EPS
}

/// 声明线开端（B165 (4)）：`s` 严格越过下线，`s < lo − ε`。
pub fn beyond_down(s: f64, lo: f64) -> bool {
    s < lo - BOUNDARY_EPS
}

// ---------------------------------------------------------------- 读数的统计量（B153、B154、B167；步 20j-3）

/// 线切在（或排序按）读数的哪个统计量上。`cut`、`order`、声明式拟合共用这一个枚举（B167 (1)）。
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Stat {
    /// `test` 的 p；K 元题的 p_max（缺省，现状）
    #[default]
    Max,
    /// 胜出单元的下标（有序划分即档位）；`cut` 不在它上面划线，`order` 用
    Argmax,
    /// 有序划分的期望档位 Σ ℓ·p_ℓ（`measure` 专用）
    Expect,
    /// 声明的单元并集的概率和（K 元题专用）；下标去重升序
    Mass(Vec<usize>),
    /// 判断器随答案自报的置信度（B154；须画像 H9）
    Confidence,
}

impl Stat {
    /// 报告与告警里的名字
    pub fn name(&self) -> &'static str {
        match self {
            Stat::Max => "max",
            Stat::Argmax => "argmax",
            Stat::Expect => "expect",
            Stat::Mass(_) => "mass",
            Stat::Confidence => "confidence",
        }
    }
    /// 规范 JSON 形（声明记录、报告 `exits` 行）：`"expect"`、`{"mass": [0, 1]}`
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Stat::Mass(cells) => serde_json::json!({ "mass": cells }),
            s => serde_json::Value::String(s.name().into()),
        }
    }
    /// 缺省统计量（`max`）不写进记录与报告：20j-1 的声明记录哈希因此不变
    pub fn is_max(&self) -> bool {
        matches!(self, Stat::Max)
    }
}

/// [`stat_of`] 取不到数的两种原因：选项与读数不配（`E-cut-options`）、判断器没给这个数（`E-stat-unavailable`）。
#[derive(Clone, Debug, PartialEq)]
pub enum StatError {
    Options(String),
    Unavailable(String),
}

/// 读数的统计量——全仓只在这里算（B167 (1)）。`confidence` 是判断器随答案给的第二个输出，不在 `Answer` 里，
/// 由调用者传入（与 `mode_share` 放在 `JudgeResult` 而不进 `Answer` 同理：`Answer` 加字段要改全仓每处 `match`）；
/// 没有这个数时返回 `Unavailable`，不退回 p_max（B154 (3)）。
/// 依据：B153 (1)、B154 (2)、B167 (1)（地基/附注/2026-09-26-批6裁定.md §一、§二、§十五）
pub fn stat_of(
    a: &crate::value::Answer,
    s: &Stat,
    confidence: Option<f64>,
) -> Result<f64, StatError> {
    use crate::value::Answer;
    let 有单元 = |v: &[f64]| -> Result<(), StatError> {
        if v.is_empty() {
            return Err(StatError::Options("读数没有单元".into()));
        }
        Ok(())
    };
    match (s, a) {
        (Stat::Max, Answer::Noul(p)) => Ok(*p),
        (Stat::Max, Answer::Choice(v) | Answer::Score(v)) => {
            有单元(v)?;
            Ok(v.iter().copied().fold(f64::MIN, f64::max))
        }
        (Stat::Argmax, Answer::Choice(v) | Answer::Score(v)) => {
            有单元(v)?;
            Ok(crate::bridge::argmax(v).0 as f64)
        }
        (Stat::Expect, Answer::Score(v)) => {
            有单元(v)?;
            Ok(v.iter().enumerate().map(|(l, p)| l as f64 * p).sum())
        }
        (Stat::Mass(cells), Answer::Choice(v) | Answer::Score(v)) => {
            if cells.is_empty() {
                return Err(StatError::Options("mass 的单元不能为空".into()));
            }
            let mut 和 = 0.0;
            for &c in cells {
                let Some(p) = v.get(c) else {
                    return Err(StatError::Options(format!(
                        "mass 的单元 {c} 越界：这道题只有 {} 个单元（下标 0..{}）",
                        v.len(),
                        v.len()
                    )));
                };
                和 += p;
            }
            Ok(和)
        }
        (Stat::Confidence, _) => confidence.ok_or_else(|| {
            StatError::Unavailable(
                "判断器没有随答案给出 confidence（画像 H9 reports_confidence 未测或为假，或账本未记）".into(),
            )
        }),
        (Stat::Argmax | Stat::Mass(_), Answer::Noul(_)) => Err(StatError::Options(format!(
            "{} 只用于 K 元题（select / measure）：test 读数只有 p 一个统计量",
            s.name()
        ))),
        (Stat::Expect, _) => Err(StatError::Options(
            "expect 只用于 measure（有序划分的期望档位）".into(),
        )),
    }
}

#[cfg(test)]
mod stat_of_tests {
    use super::*;
    use crate::value::Answer;

    #[test]
    fn 统计量按定义取数() {
        let c = Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]);
        let s = Answer::Score(vec![0.1, 0.2, 0.6, 0.1]);
        assert_eq!(stat_of(&c, &Stat::Max, None), Ok(0.4));
        assert_eq!(stat_of(&c, &Stat::Argmax, None), Ok(2.0));
        assert!((stat_of(&c, &Stat::Mass(vec![0, 1]), None).unwrap() - 0.55).abs() < 1e-12);
        assert!((stat_of(&s, &Stat::Expect, None).unwrap() - 1.7).abs() < 1e-12);
        assert_eq!(stat_of(&Answer::Noul(0.8), &Stat::Max, None), Ok(0.8));
        assert_eq!(
            stat_of(&Answer::Noul(0.8), &Stat::Confidence, Some(0.55)),
            Ok(0.55)
        );
        assert!(matches!(
            stat_of(&Answer::Noul(0.8), &Stat::Confidence, None),
            Err(StatError::Unavailable(_))
        ));
        assert!(matches!(
            stat_of(&c, &Stat::Expect, None),
            Err(StatError::Options(_))
        ));
        assert!(matches!(
            stat_of(&c, &Stat::Mass(vec![4]), None),
            Err(StatError::Options(_))
        ));
        assert!(matches!(
            stat_of(&Answer::Noul(0.8), &Stat::Mass(vec![0]), None),
            Err(StatError::Options(_))
        ));
    }
}
