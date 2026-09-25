//! 固定序认证（B86；`calib-import` 的缺省方式）：候选阈值只由读数按计数生成，按从严到宽的固定顺序
//! 逐个检验，第一次不过即停；K 元单侧线同形。共用的前置与上岗在 `commission`。

use jpp_value::stat::Certificate;

use super::commission::{上侧声明, 下侧声明, 多元声明};
use super::*;

impl CalibStore {
    /// **两侧认证，固定序、不拆分**（B86；`calib-import` 的缺省方式）。
    ///
    /// 候选阈值只由读数按计数生成（[`fixed_sequence_candidates`]，不读标签），按从严到宽的固定顺序
    /// 逐个用 Clopper–Pearson 上界检验「该候选已决区错误率 > α」，第一次不过即停；两侧各在通过前缀里
    /// 取 `l + δ ≤ h − δ` 且已决最多的一对（平局取先到：外层上侧从严到宽，内层下侧从严到宽）。
    ///
    /// 为什么不拆分也不过拟合：以读数为条件，候选序列在看标签之前就固定了；任何为真的原假设被拒，
    /// 必先拒掉序列中第一个为真的原假设，于是每侧族错误率 ≤ δ_c，与拆分认证同级，不花一半样本
    /// （推导见 `地基/附注/2026-09-24-标注门槛裁定.md` §一·2）。线仍只来自带真值样本：没有标签时
    /// 没有候选会被拒绝，输出是「认证不过」，不存在线（J-03）。
    ///
    /// `step` 为 `None` 时取 [`fixed_sequence_step`]；证书写解析后的整数、规则版本、δ、两侧生成数与停点，
    /// 步 20c 的 `load` 凭标注集与这些字段即可重跑。
    pub fn commission_two_sided_fixed_sequence_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        step: Option<usize>,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        let pre = self.两侧前置(key, alpha, conf_delta)?;
        let delta = pre.delta;
        let 按条 = pre.按条();
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        let (正, 负) = (
            按条.iter().filter(|x| x.1).count(),
            按条.iter().filter(|x| !x.1).count(),
        );
        if 正.min(负) < n_needed {
            return Err(Refusal::跑不成(format!(
                "待核：样本不足（正例 {正}、负例 {负}，固定序零错误也需每侧已决 ≥ {n_needed}）"
            )));
        }
        let s = 固定序步长(step)?.unwrap_or_else(|| fixed_sequence_step(按条.len()));
        let ps: Vec<f64> = 按条.iter().map(|x| x.0).collect();
        let (ups, downs) = fixed_sequence_candidates(&ps, alpha, conf_delta, delta, s);
        let up = 固定序前缀(&按条, &ups, alpha, conf_delta, true);
        let down = 固定序前缀(&按条, &downs, alpha, conf_delta, false);
        // 两侧通过前缀里取合法线对：已决之和最大，平局取先到
        let mut best: Option<(usize, usize, usize)> = None;
        let mut 合法对 = 0usize;
        for (i, a) in up.通过.iter().enumerate() {
            for (j, d) in down.通过.iter().enumerate() {
                if d.阈 + delta > a.阈 - delta {
                    continue;
                }
                合法对 += 1;
                if best.map(|b| a.已决 + d.已决 > b.2).unwrap_or(true) {
                    best = Some((i, j, a.已决 + d.已决));
                }
            }
        }
        let Some((i, j, _)) = best else {
            // 第一个不通过的候选（上侧先查）；两侧都没停却没有合法对时如实写 1.0
            let 停 = up.停.or(down.停);
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: 停.map(|x| x.上界).unwrap_or(1.0),
                best_hi: 停.map(|x| x.阈).unwrap_or(1.0),
                best_n_accepted: 停.map(|x| x.已决).unwrap_or(0),
                n_needed,
            }));
        };
        let (a, d) = (up.通过[i], down.通过[j]);
        let sel = Selection {
            method: "fixed-sequence".into(),
            seed: 0,
            n_select: 0,
            n_certify: 按条.len(),
            candidates: 合法对,
            rule: Some(FIXED_SEQUENCE_RULE.into()),
            step: Some(s),
            delta: Some(delta),
            generated: Some((ups.len(), downs.len())),
            stop_index: Some((up.通过.len(), down.通过.len())),
            sequential: None,
        };
        let 规则 = "固定序认证（B86）：候选由读数按计数生成，从严到宽逐个检验上界 ≤ α，第一次不过即停；两侧通过前缀里取已决最多的合法线对".to_string();
        let (hi, lo) = (
            (a.阈 - delta).clamp(0.0, 1.0),
            (d.阈 + delta).clamp(0.0, 1.0),
        );
        let upper = pre.证书(
            alpha,
            conf_delta,
            hi,
            (a.已决, a.错, a.上界),
            &规则,
            上侧声明(),
            &sel,
            grade,
        );
        let lower = pre.证书(
            alpha,
            conf_delta,
            lo,
            (d.已决, d.错, d.上界),
            &规则,
            下侧声明(),
            &sel,
            grade,
        );
        Ok(self.两侧上岗(key, upper, lower, &按条, delta))
    }
}

impl CalibStore {
    /// **K 元单侧认证，固定序**（B86 与 B63 同形）：只生成上侧候选，通过前缀里取已决最多者
    /// （平局取先到，即更严者），`hi = h − δ`，`lo = 0`，没有下侧证书。
    pub fn commission_upper_fixed_sequence_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        step: Option<usize>,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        let pre = self.单侧前置(key, alpha, conf_delta)?;
        let delta = pre.delta;
        let 按条 = pre.按条();
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        let 对 = 按条.iter().filter(|x| x.1).count();
        if 对 < n_needed {
            return Err(Refusal::跑不成(format!(
                "待核：样本不足（argmax 正确 {对} 条，固定序零错误也需已决 ≥ {n_needed}）"
            )));
        }
        let s = 固定序步长(step)?.unwrap_or_else(|| fixed_sequence_step(按条.len()));
        let ps: Vec<f64> = 按条.iter().map(|x| x.0).collect();
        let (ups, _) = fixed_sequence_candidates(&ps, alpha, conf_delta, delta, s);
        let up = 固定序前缀(&按条, &ups, alpha, conf_delta, true);
        let mut best: Option<候选检验> = None;
        for a in &up.通过 {
            if best.map(|b| a.已决 > b.已决).unwrap_or(true) {
                best = Some(*a);
            }
        }
        let Some(a) = best else {
            let 停 = up.停;
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: 停.map(|x| x.上界).unwrap_or(1.0),
                best_hi: 停.map(|x| x.阈).unwrap_or(1.0),
                best_n_accepted: 停.map(|x| x.已决).unwrap_or(0),
                n_needed,
            }));
        };
        let sel = Selection {
            method: "fixed-sequence".into(),
            seed: 0,
            n_select: 0,
            n_certify: 按条.len(),
            candidates: up.通过.len(),
            rule: Some(FIXED_SEQUENCE_RULE.into()),
            step: Some(s),
            delta: Some(delta),
            generated: Some((ups.len(), 0)),
            stop_index: Some((up.通过.len(), 0)),
            sequential: None,
        };
        let hi = (a.阈 - delta).clamp(0.0, 1.0);
        let 规则 = "K 元单侧固定序认证（B86、B63）：候选由读数按计数生成，从严到宽逐个检验，第一次不过即停；取通过前缀里已决最多的 h";
        let cert = pre.证书(
            alpha,
            conf_delta,
            hi,
            (a.已决, a.错, a.上界),
            规则,
            多元声明(),
            &sel,
            grade,
        );
        Ok(self.单侧上岗(key, cert, &按条, pre.op, delta))
    }
}

/// 固定序候选规则与平局规则的版本名（写进证书；改规则必须改名，`load` 按名重跑）。
pub const FIXED_SEQUENCE_RULE: &str = "fixed-sequence/v1";

/// 固定序的缺省步长：池的 5%，半数进一，至少 1（整数算术：⌊(n + 10) / 20⌋）。
///
/// 研究者与裁定者的 Python 用 `round(0.05·n)`（半数取偶），两者只在 n ≡ 10 (mod 20) 时差 1；
/// 取整方式属于规则版本 `fixed-sequence/v1`，证书写解析后的整数，重跑不依赖这个函数。
pub fn fixed_sequence_step(n: usize) -> usize {
    ((n + 10) / 20).max(1)
}

/// **固定序的候选阈值，只由读数生成**（B86；规则 `fixed-sequence/v1`）。
///
/// 上侧：读数降序 v_1 ≥ … ≥ v_n，j 从 `n_needed_zero_error(α, δ_c)` 起取 v_j，把 j 扩到并列组边缘 k，
/// 依次给出组值 v_j 与到下一个不同值的中点（k = n 时只有组值），只收 t ≥ δ、与上一个相同的不重收，
/// 下一轮 j = k + s。下侧对称（升序，只收 t ≤ 1 − δ）。两个列表都是从严到宽，即检验顺序。
/// **不收标签**：同一批读数换标签、换行序，候选序列逐位相同（这是固定序有效的前提）。
pub fn fixed_sequence_candidates(
    ps: &[f64],
    alpha: f64,
    conf_delta: f64,
    delta: f64,
    step: usize,
) -> (Vec<f64>, Vec<f64>) {
    let nn = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
    candidates_from(ps, nn, delta, step)
}

/// 候选生成的本体：首个候选从读数最高（最低）的 `first` 条起（固定序传 `n_needed`，序贯传
/// `seq_first`，B87 方案 A）。其余规则同 [`fixed_sequence_candidates`]。
pub(crate) fn candidates_from(
    ps: &[f64],
    first: usize,
    delta: f64,
    step: usize,
) -> (Vec<f64>, Vec<f64>) {
    let nn = first.max(1);
    let step = step.max(1);
    let side = |vals: &[f64], top: bool| -> Vec<f64> {
        let n = vals.len();
        let mut out: Vec<f64> = vec![];
        let mut j = nn;
        while j <= n {
            let v = vals[j - 1];
            let mut k = j;
            while k < n && vals[k] == v {
                k += 1;
            }
            let mut ts = vec![v];
            if k < n {
                ts.push((v + vals[k]) / 2.0);
            }
            for t in ts {
                let ok = if top { t >= delta } else { t <= 1.0 - delta };
                if ok && out.last() != Some(&t) {
                    out.push(t);
                }
            }
            j = k + step;
        }
        out
    };
    let mut desc: Vec<f64> = ps.to_vec();
    desc.sort_by(|a, b| b.total_cmp(a));
    let mut asc: Vec<f64> = ps.to_vec();
    asc.sort_by(|a, b| a.total_cmp(b));
    (side(&desc, true), side(&asc, false))
}

/// 显式步长必须 ≥ 1（命令行已挡，这里挡 API 调用）。
///
/// **样本不足的判定放在调用处、按正负例数、在生成候选之前**：某一侧的正确样本 P < n_needed 时，
/// 任何候选都过不了——候选集大小 m ≥ n_needed，错误数 k ≥ m − P；而 P(Bin(m, p) ≤ k) ≥ (1 − p)^{m−k}
/// （前 m − k 次全错即成立），所以 Clopper–Pearson 上界 ucb(k, m) ≥ ucb(0, m − k) > α（m − k ≤ P < n_needed）。
/// 提前返回「待核：样本不足」与跑完再拒的判定相同，只是报文说对了原因（B72 据此接着试试用档）。
fn 固定序步长(step: Option<usize>) -> Result<Option<usize>, Refusal> {
    if step == Some(0) {
        return Err(Refusal::跑不成("固定序步长 --step 必须 ≥ 1".into()));
    }
    Ok(step)
}

/// 一个候选的检验结果：阈值、已决条数、错误数、上界。
#[derive(Clone, Copy, Debug)]
struct 候选检验 {
    阈: f64,
    已决: usize,
    错: usize,
    上界: f64,
}

/// 一侧的固定序：通过前缀与第一个不通过的候选（全部通过时为 `None`）。
struct 前缀 {
    通过: Vec<候选检验>,
    停: Option<候选检验>,
}

/// 按序检验一侧的候选，第一次不过即停。上侧判区 p ≥ h、错误是真值为假；下侧判区 p ≤ l、错误是真值为真。
fn 固定序前缀(
    按条: &[(f64, bool)],
    cands: &[f64],
    alpha: f64,
    conf_delta: f64,
    top: bool,
) -> 前缀 {
    let mut 通过 = vec![];
    for &t in cands {
        let acc: Vec<&(f64, bool)> = 按条
            .iter()
            .filter(|x| if top { x.0 >= t } else { x.0 <= t })
            .collect();
        let k = acc.iter().filter(|x| x.1 != top).count();
        let u = jpp_value::stat::binomial_upper(k, acc.len(), conf_delta);
        let c = 候选检验 {
            阈: t,
            已决: acc.len(),
            错: k,
            上界: u,
        };
        if acc.is_empty() || u > alpha {
            return 前缀 {
                通过, 停: Some(c)
            };
        }
        通过.push(c);
    }
    前缀 { 通过, 停: None }
}
