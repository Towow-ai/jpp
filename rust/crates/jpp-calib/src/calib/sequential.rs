//! 序贯认证（B87，步 20h；主会话 2026-09-24 按方案 A）：标注按批到达，每个候选阈值维护一个混合 e 过程，
//! E ≥ 1/δ 即拒绝「该候选已决区错误率 ≥ α」，每批后按固定序过一遍两侧前缀。
//!
//! 有效性：混合似然比在原假设下是非负上鞅，Ville 不等式对任何停时成立，所以「每批看一眼再决定是否继续」
//! 不损失置信水平；固定序只用到每个检验的有效性，组合后每侧族错误率仍 ≤ δ
//! （`地基/附注/2026-09-24-标注门槛裁定.md` §二·1）。每个候选只用 E 判定、不用 Clopper–Pearson（方案 A）。
//! B87 修订（`附注/2026-09-24-B104裁定.md` §三）：候选是 B86 固定序序列里框内已决数 ≥ `seq_first` 的那些；
//! 序贯与固定序各自每侧族错误率 ≤ δ，零错、全标、每侧候选 ≥ `seq_first` 条时线对相同，一般情形两者可宽可严。
//! 零错门槛：正式 24、试用 9（固定序 22、9）。
//!
//! 顺序条件（§二·2）：两端先标（`two-ends`）时，已决集不等于首组的候选只在已决集全部标完后才读 E——
//! 部分标注时它只见到首组那些最极端的样本，不是均匀样本，e 过程不成立。

use super::commission::{上侧声明, 下侧声明, 多元声明};
use super::fixed_sequence::{candidates_from, fixed_sequence_step};
use super::*;
use jpp_value::stat;

/// 序贯候选规则与停止规则的版本名（写进证书）。
pub const SEQUENTIAL_RULE: &str = "sequential/v1";

/// 调用方给的序贯规格（导入参数与抽样框）。
#[derive(Clone, Debug)]
pub struct SeqSpec {
    /// 抽样框读数；`None` = 用全部带标注样本的读数
    pub pool: Option<Vec<f64>>,
    /// 到达顺序：规范序（按 `(p, 真值)`）带标注样本的下标，须是 `0..n` 的一个排列
    pub arrival: Vec<usize>,
    /// 是否两端先标（顺序条件只对它生效）
    pub two_ends: bool,
    /// 写进证书的顺序名与种子
    pub order: String,
    pub seed: u64,
    pub batch: usize,
    pub weights: [f64; 4],
    /// 覆盖目标 τ；`None` = settled
    pub coverage_target: Option<f64>,
}

/// 混合 e 过程零错过线所需条数（正式 24、试用 9；算法在 `jpp_value::stat::e_zero_error_needed`）。
pub fn seq_first(alpha: f64, conf_delta: f64, weights: &[f64; 4]) -> usize {
    stat::e_zero_error_needed(alpha, conf_delta, weights)
}

/// 序贯候选（B87 修订 (1)）：B86 的候选序列（首个 `n_needed` 条）里，框内已决数 ≥ `seq_first` 的那些。
/// 不平移格：同一格上与固定序可比，零错全标时线对相同。
pub(crate) fn 序贯候选(
    pool: &[f64],
    alpha: f64,
    conf_delta: f64,
    weights: &[f64; 4],
    delta: f64,
    step: usize,
) -> (Vec<f64>, Vec<f64>) {
    let first = seq_first(alpha, conf_delta, weights);
    let nn = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
    let (ups, downs) = candidates_from(pool, nn, delta, step);
    let 够 = |t: &f64, top: bool| {
        pool.iter()
            .filter(|p| if top { **p >= *t } else { **p <= *t })
            .count()
            >= first
    };
    (
        ups.into_iter().filter(|t| 够(t, true)).collect(),
        downs.into_iter().filter(|t| 够(t, false)).collect(),
    )
}

/// 随机到达顺序：`0..n` 按 `splitmix64(seed ^ i)` 排序（确定性置换）。
pub fn random_arrival(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    v.sort_by_key(|i| (super::commission::splitmix64(seed ^ *i as u64), *i));
    v
}

/// **两端先标的清单顺序**（B88）：返回 `(框下标, 组名)`。组 j 由上侧候选 j 与下侧候选 j 相对前一组新增的
/// 框内条目组成（两侧交替，组内按种子随机），最后是其余条目（组名「余」，随机）。候选与序贯同一规则
/// （首个候选 `seq_first` 条）。只读读数。
pub fn two_ends_order(
    pool: &[f64],
    alpha: f64,
    conf_delta: f64,
    delta: f64,
    step: Option<usize>,
    weights: &[f64; 4],
    seed: u64,
) -> Vec<(usize, String)> {
    let n = pool.len();
    let s = step.unwrap_or_else(|| fixed_sequence_step(n));
    let (ups, downs) = 序贯候选(pool, alpha, conf_delta, weights, delta, s);
    let mut 已排 = vec![false; n];
    let mut out: Vec<(usize, String)> = vec![];
    // 组号只数非空的组（候选的组值与中点常覆盖同一批条目）
    let mut 组 = 0usize;
    let 洗 = |xs: &mut Vec<usize>, tag: u64| {
        xs.sort_by_key(|i| {
            (
                super::commission::splitmix64(seed ^ tag.wrapping_mul(0x1_0000_0001) ^ *i as u64),
                *i,
            )
        })
    };
    for j in 0..ups.len().max(downs.len()) {
        let mut 上: Vec<usize> = ups
            .get(j)
            .map(|h| (0..n).filter(|i| !已排[*i] && pool[*i] >= *h).collect())
            .unwrap_or_default();
        洗(&mut 上, 2 * j as u64 + 1);
        for i in &上 {
            已排[*i] = true;
        }
        let mut 下: Vec<usize> = downs
            .get(j)
            .map(|l| (0..n).filter(|i| !已排[*i] && pool[*i] <= *l).collect())
            .unwrap_or_default();
        洗(&mut 下, 2 * j as u64 + 2);
        for i in &下 {
            已排[*i] = true;
        }
        if 上.is_empty() && 下.is_empty() {
            continue;
        }
        组 += 1;
        for k in 0..上.len().max(下.len()) {
            if let Some(i) = 上.get(k) {
                out.push((*i, format!("上{组}")));
            }
            if let Some(i) = 下.get(k) {
                out.push((*i, format!("下{组}")));
            }
        }
    }
    let mut 余: Vec<usize> = (0..n).filter(|i| !已排[*i]).collect();
    洗(&mut 余, 0);
    out.extend(余.into_iter().map(|i| (i, "余".to_string())));
    out
}

/// 一侧候选的 e 过程状态。
struct 一侧 {
    阈: Vec<f64>,
    /// 框内已决条数（全标时的条数）
    框: Vec<usize>,
    e: Vec<[f64; 4]>,
    n: Vec<usize>,
    错: Vec<usize>,
    top: bool,
}

impl 一侧 {
    fn new(阈: Vec<f64>, pool: &[f64], top: bool) -> Self {
        let 框 = 阈
            .iter()
            .map(|t| {
                pool.iter()
                    .filter(|p| if top { **p >= *t } else { **p <= *t })
                    .count()
            })
            .collect();
        let k = 阈.len();
        一侧 {
            阈,
            框,
            e: vec![stat::E_START; k],
            n: vec![0; k],
            错: vec![0; k],
            top,
        }
    }
    fn 更新(&mut self, p: f64, 真: bool, alpha: f64) {
        let p1 = stat::e_alternatives(alpha);
        for j in 0..self.阈.len() {
            let 在 = if self.top {
                p >= self.阈[j]
            } else {
                p <= self.阈[j]
            };
            if !在 {
                continue;
            }
            let 错 = 真 != self.top;
            self.n[j] += 1;
            if 错 {
                self.错[j] += 1;
            }
            for (e, p) in self.e[j].iter_mut().zip(p1) {
                *e *= stat::e_factor(alpha, p, 错);
            }
        }
    }
    fn 混合(&self, j: usize, w: &[f64; 4]) -> f64 {
        (0..4).map(|m| w[m] * self.e[j][m]).sum()
    }
    /// 候选 j 此刻可否算通过：E ≥ 1/δ，且（两端先标时）是首组或已标完
    fn 可过(&self, j: usize, w: &[f64; 4], thresh: f64, two_ends: bool) -> bool {
        self.混合(j, w) >= thresh
            && (!two_ends || self.框[j] == self.框[0] || self.n[j] >= self.框[j])
    }
    /// 通过前缀长度（第一次不过即停）
    fn 前缀(&self, w: &[f64; 4], thresh: f64, two_ends: bool) -> usize {
        (0..self.阈.len())
            .take_while(|j| self.可过(*j, w, thresh, two_ends))
            .count()
    }
    /// 候选 j 是否已「定」：已标完，或剩余全对也到不了 1/δ
    fn 已定(&self, j: usize, w: &[f64; 4], thresh: f64, alpha: f64) -> bool {
        self.n[j] >= self.框[j] || self.潜力(j, w, alpha, self.框[j] - self.n[j]) < thresh
    }
    /// 再来 r 条全对时的混合 E
    fn 潜力(&self, j: usize, w: &[f64; 4], alpha: f64, r: usize) -> f64 {
        stat::e_mixture_after(&self.e[j], w, alpha, r)
    }
    /// 候选 j 还需约几条零错样本才到 1/δ（不以框内剩余封顶：框外新标的材料也可以补进来）
    fn 还需(&self, j: usize, w: &[f64; 4], thresh: f64, alpha: f64) -> usize {
        (0..10_000)
            .find(|r| self.潜力(j, w, alpha, *r) >= thresh)
            .unwrap_or(10_000)
    }
}

/// 一次序贯运行的结果。
struct 结局 {
    停: Option<usize>,
    /// 选中的 (上侧候选下标, 下侧候选下标)
    对: Option<(usize, Option<usize>)>,
    上: 一侧,
    下: Option<一侧>,
    处理: usize,
}

#[allow(clippy::too_many_arguments)]
fn 运行(
    按条: &[(f64, bool)],
    pool: &[f64],
    spec: &SeqSpec,
    alpha: f64,
    conf_delta: f64,
    delta: f64,
    s: usize,
    两侧: bool,
) -> 结局 {
    let w = &spec.weights;
    let thresh = stat::e_threshold(conf_delta);
    let (ups, downs) = 序贯候选(pool, alpha, conf_delta, w, delta, s);
    let mut 上 = 一侧::new(ups, pool, true);
    let mut 下 = 两侧.then(|| 一侧::new(downs, pool, false));
    let batch = spec.batch.max(1);
    let mut 处理 = 0usize;
    for 块 in spec.arrival.chunks(batch) {
        for &i in 块 {
            let (p, t) = 按条[i];
            上.更新(p, t, alpha);
            if let Some(d) = 下.as_mut() {
                d.更新(p, t, alpha);
            }
        }
        处理 += 块.len();
        let ju = 上.前缀(w, thresh, spec.two_ends);
        let jd = 下.as_ref().map(|d| d.前缀(w, thresh, spec.two_ends));
        // 合法线对（平局规则与固定序同：外层上侧从严到宽、内层下侧从严到宽，已决之和严格大于才换）
        let mut 对们: Vec<(usize, Option<usize>, usize, f64)> = vec![];
        for a in 0..ju {
            match (&下, jd) {
                (Some(d), Some(jd)) => {
                    for b in 0..jd {
                        if d.阈[b] + delta > 上.阈[a] - delta {
                            continue;
                        }
                        let c = stat::coverage(pool, Some(上.阈[a]), Some(d.阈[b]));
                        对们.push((a, Some(b), 上.n[a] + d.n[b], c));
                    }
                }
                _ => 对们.push((a, None, 上.n[a], stat::coverage(pool, Some(上.阈[a]), None))),
            }
        }
        if 对们.is_empty() {
            continue;
        }
        let 选 = match spec.coverage_target {
            Some(tau) => {
                let mut best: Option<&(usize, Option<usize>, usize, f64)> = None;
                for x in 对们.iter().filter(|x| x.3 >= tau) {
                    if best.map(|b| x.3 < b.3).unwrap_or(true) {
                        best = Some(x);
                    }
                }
                best.map(|b| (b.0, b.1))
            }
            None => {
                let fu = ju >= 上.阈.len() || 上.已定(ju, w, thresh, alpha);
                let fd = match (&下, jd) {
                    (Some(d), Some(jd)) => jd >= d.阈.len() || d.已定(jd, w, thresh, alpha),
                    _ => true,
                };
                if fu && fd {
                    let mut best: Option<&(usize, Option<usize>, usize, f64)> = None;
                    for x in &对们 {
                        if best.map(|b| x.2 > b.2).unwrap_or(true) {
                            best = Some(x);
                        }
                    }
                    best.map(|b| (b.0, b.1))
                } else {
                    None
                }
            }
        };
        if let Some(对) = 选 {
            return 结局 {
                停: Some(处理),
                对: Some(对),
                上,
                下,
                处理,
            };
        }
    }
    结局 {
        停: None,
        对: None,
        上,
        下,
        处理,
    }
}

/// 未停时的门控文本（B87）。
fn 未停报文(
    r: &结局,
    pool: &[f64],
    w: &[f64; 4],
    alpha: f64,
    conf_delta: f64,
    delta: f64,
    two_ends: bool,
) -> String {
    let thresh = stat::e_threshold(conf_delta);
    let 侧 = |d: &一侧| -> (String, usize) {
        let j = d.前缀(w, thresh, two_ends);
        if j >= d.阈.len() {
            ("全过".into(), 0)
        } else {
            (format!("{:.2}", d.混合(j, w)), d.还需(j, w, thresh, alpha))
        }
    };
    let (eu, mu) = 侧(&r.上);
    let (ed, md) = r.下.as_ref().map(侧).unwrap_or(("—".into(), 0));
    // 当前合法对与全部候选都过时最宽合法对的覆盖
    let 最宽 = |ju: usize, jd: Option<usize>| -> f64 {
        let mut c = f64::default();
        for a in 0..ju {
            match (&r.下, jd) {
                (Some(d), Some(jd)) => {
                    for b in 0..jd {
                        if d.阈[b] + delta <= r.上.阈[a] - delta {
                            c = c.max(stat::coverage(pool, Some(r.上.阈[a]), Some(d.阈[b])));
                        }
                    }
                }
                _ => c = c.max(stat::coverage(pool, Some(r.上.阈[a]), None)),
            }
        }
        c
    };
    let c = 最宽(
        r.上.前缀(w, thresh, two_ends),
        r.下.as_ref().map(|d| d.前缀(w, thresh, two_ends)),
    );
    let cmax = 最宽(r.上.阈.len(), r.下.as_ref().map(|d| d.阈.len()));
    format!(
        "待核：序贯未停：已标 {}，上侧 E={eu}，下侧 E={ed}，当前合法线对覆盖 {c:.2}，继续标最多可到 {cmax:.2}，按当前零错还需约 {} 条",
        r.处理,
        mu + md
    )
}

impl CalibStore {
    /// **两侧认证，序贯 e 过程**（B87；`--certify sequential`）。候选由抽样框读数生成（首个 `seq_first` 条），
    /// 带标注样本按 `spec.arrival` 的顺序、每 `spec.batch` 条一批到达，每批后按固定序过两侧前缀；停止规则
    /// 缺省 settled，给了覆盖目标 τ 时在覆盖 ≥ τ 的最窄合法对处停。未停时不认证，门控报还差多少。
    pub fn commission_two_sided_sequential_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        step: Option<usize>,
        grade: CertGrade,
        spec: &SeqSpec,
    ) -> Result<Cert, Refusal> {
        let pre = self.两侧前置(key, alpha, conf_delta)?;
        self.序贯(key, alpha, conf_delta, step, grade, spec, pre, true)
    }

    /// K 元单侧的序贯认证（B87 与 B63 同形）：只有上侧。
    pub fn commission_upper_sequential_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        step: Option<usize>,
        grade: CertGrade,
        spec: &SeqSpec,
    ) -> Result<Cert, Refusal> {
        let pre = self.单侧前置(key, alpha, conf_delta)?;
        self.序贯(key, alpha, conf_delta, step, grade, spec, pre, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn 序贯(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        step: Option<usize>,
        grade: CertGrade,
        spec: &SeqSpec,
        pre: super::commission::前置,
        两侧: bool,
    ) -> Result<Cert, Refusal> {
        let bad = |why: String| Refusal::跑不成(why);
        if step == Some(0) {
            return Err(bad("序贯步长 --step 必须 ≥ 1".into()));
        }
        let w = spec.weights;
        if !stat::e_weights_valid(&w) {
            return Err(bad("混合权重须非负且和为 1".into()));
        }
        let delta = pre.delta;
        // 规范序：与到达下标同一口径
        let mut 按条 = pre.按条();
        按条.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let n = 按条.len();
        let mut 见 = vec![false; n];
        if spec.arrival.len() != n
            || spec
                .arrival
                .iter()
                .any(|i| *i >= n || std::mem::replace(&mut 见[*i], true))
        {
            return Err(bad(format!(
                "到达顺序须是全部 {n} 条带标注样本的一个排列（序贯要这次导入给出该键的全部标注）"
            )));
        }
        let mut pool: Vec<f64> = spec
            .pool
            .clone()
            .unwrap_or_else(|| 按条.iter().map(|x| x.0).collect());
        pool.sort_by(|a, b| a.total_cmp(b));
        // 判定等价的预检：某侧已标的正确样本 P < seq_first 时任何候选都过不了——候选集 m 条里错 k ≥ m − P，
        // 每个分量 w_j (p_j/α)^k ((1 − p_j)/(1 − α))^{m−k} ≤ w_j ((1 − p_j)/(1 − α))^{P}，混合 E 不超过 P 条零错的
        // E，而后者 < 1/δ。提前报样本不足，只是报文说对原因（B72 据此接着试试用档）
        let first = seq_first(alpha, conf_delta, &w);
        let (正, 负) = (
            按条.iter().filter(|x| x.1).count(),
            按条.iter().filter(|x| !x.1).count(),
        );
        // 只在没有抽样框时预检：有框（两端先标、边标边导）时报「未停」与还差多少，更有用
        if spec.pool.is_none() && ((两侧 && 正.min(负) < first) || (!两侧 && 正 < first)) {
            return Err(bad(if 两侧 {
                format!("待核：样本不足（正例 {正}、负例 {负}，序贯零错误也需每侧已决 ≥ {first}）")
            } else {
                format!("待核：样本不足（argmax 正确 {正} 条，序贯零错误也需已决 ≥ {first}）")
            }));
        }
        let s = step.unwrap_or_else(|| fixed_sequence_step(pool.len()));
        let r = 运行(&按条, &pool, spec, alpha, conf_delta, delta, s, 两侧);
        let (Some(停), Some((a, b))) = (r.停, r.对) else {
            return Err(bad(未停报文(
                &r,
                &pool,
                &w,
                alpha,
                conf_delta,
                delta,
                spec.two_ends,
            )));
        };
        let thresh = stat::e_threshold(conf_delta);
        let seq = SeqCert {
            order: spec.order.clone(),
            seed: spec.seed,
            batch: spec.batch,
            weights: w,
            coverage_target: spec.coverage_target,
            stopped_at: 停,
            pool: pool.clone(),
            arrival: spec.arrival.clone(),
            e_upper: (0..r.上.阈.len()).map(|j| r.上.混合(j, &w)).collect(),
            n_upper: r.上.n.clone(),
            e_lower: r
                .下
                .as_ref()
                .map(|d| (0..d.阈.len()).map(|j| d.混合(j, &w)).collect())
                .unwrap_or_default(),
            n_lower: r.下.as_ref().map(|d| d.n.clone()).unwrap_or_default(),
        };
        let sel = Selection {
            method: "sequential".into(),
            seed: spec.seed,
            n_select: 0,
            n_certify: 停,
            candidates: 0,
            rule: Some(SEQUENTIAL_RULE.into()),
            step: Some(s),
            delta: Some(delta),
            generated: Some((
                r.上.阈.len(),
                r.下.as_ref().map(|d| d.阈.len()).unwrap_or(0),
            )),
            stop_index: Some((
                r.上.前缀(&w, thresh, spec.two_ends),
                r.下
                    .as_ref()
                    .map(|d| d.前缀(&w, thresh, spec.two_ends))
                    .unwrap_or(0),
            )),
            sequential: Some(seq),
        };
        let 证据 = |d: &一侧, j: usize| {
            let (n, k) = (d.n[j], d.错[j]);
            (n, k, jpp_value::stat::binomial_upper(k, n, conf_delta))
        };
        let 规则 = format!(
            "序贯认证（B87，{}）：混合 e 过程 E ≥ 1/δ 即拒绝，每批后按固定序过两侧前缀；停时 {停} 条；ucb 为停时已决样本的 Clopper–Pearson 上界，仅作记录，判定按 E",
            spec.order
        );
        let hi = stat::clamp_unit(r.上.阈[a] - delta);
        if 两侧 {
            let d = r.下.as_ref().expect("两侧");
            let b = b.expect("两侧有下侧");
            let lo = stat::clamp_unit(d.阈[b] + delta);
            let upper = pre.证书(
                alpha,
                conf_delta,
                hi,
                证据(&r.上, a),
                &规则,
                上侧声明(),
                &sel,
                grade,
            );
            let lower = pre.证书(
                alpha,
                conf_delta,
                lo,
                证据(d, b),
                &规则,
                下侧声明(),
                &sel,
                grade,
            );
            let 全部 = pre.按条();
            Ok(self.两侧上岗(key, upper, lower, &全部, delta))
        } else {
            let cert = pre.证书(
                alpha,
                conf_delta,
                hi,
                证据(&r.上, a),
                &规则,
                多元声明(),
                &sel,
                grade,
            );
            Ok(self.单侧上岗(key, cert, &pre.按条(), pre.op, delta))
        }
    }
}
