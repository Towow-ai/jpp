//! 范围扩展认证（B91，步 20d-2）：在新风格材料上只检验记录已有的线对（不重新选线），两侧各一次二项上界检验，
//! 过即把该风格的材料指纹并入 `scope.extensions`。依据：`地基/附注/2026-09-24-标注门槛裁定.md` §七。

use super::*;
use jpp_value::value::Op;

/// 扩展批的一行：读数、真值、材料文本。
#[derive(Clone, Debug)]
pub struct ExtendRow {
    pub p: f64,
    pub label: bool,
    pub text: String,
}

/// 扩展认证的参数。
#[derive(Clone, Debug)]
pub struct ExtendOptions {
    /// 试用 α；`None` 或不大于记录的 α 时只按正式 α 检验
    pub alpha_trial: Option<f64>,
    pub quantiles: (f64, f64),
    pub margins: jpp_value::stat::ScopeMargins,
    pub batch: String,
}

impl CalibStore {
    /// **范围扩展认证**（B91）：对键 `key` 的记录，在新风格材料 `rows` 上检验它已有的线对。先按记录选中证书的 α，
    /// 不过再按试用 α。每侧要求已决 ≥ `n_needed_zero_error(α, δ_c)` 且二项上界 ≤ α（K 元记录只验上侧）。
    /// 过了追加一条扩展并返回它；不过返回原因，记录不动。扩展批的样本不并进记录（会改变标注集指纹，
    /// 步 20c 的 `load` 重跑会因此降级）。
    pub fn extend_scope(
        &mut self,
        key: &str,
        rows: &[ExtendRow],
        opt: &ExtendOptions,
    ) -> Result<ScopeExtension, String> {
        let rec = self
            .records
            .get(key)
            .ok_or_else(|| format!("没有校准记录 {key}"))?;
        if rec.fixture || rec.status != "上岗" {
            return Err(format!(
                "键 {key} 的记录不是上岗的认证线（状态 {}{}），没有线对可扩展",
                rec.status,
                if rec.fixture { "，夹具" } else { "" }
            ));
        }
        let c = rec
            .选中的证书()
            .cloned()
            .ok_or_else(|| format!("键 {key} 没有证书"))?;
        if rec
            .scope
            .as_ref()
            .and_then(|s| s.fingerprint.as_ref())
            .is_none()
        {
            return Err(format!(
                "键 {key} 的记录没有认证范围指纹（范围未知，B104-2）：先带 text 重新导入，再扩展"
            ));
        }
        if rows.is_empty() {
            return Err("扩展批是空的".into());
        }
        let op = rec
            .samples
            .iter()
            .find(|s| s.label.is_some())
            .and_then(|s| super::record::反查题型(&s.phys))
            .unwrap_or(Op::Test);
        let _ = op;
        // 与 cut 同一个 δ 与同一边界比较（步 15d-2）
        let delta = CalibStore::cert_delta(rec, &c).ok_or_else(|| {
            "记录的线没有 δ（步 15d-2：δ 只从记录取），扩展认证做不了".to_string()
        })?;
        let (hi, lo, 两侧) = (rec.hi, rec.lo, rec.lower.is_some());
        let 上: Vec<&ExtendRow> = rows
            .iter()
            .filter(|r| jpp_value::stat::decided_up(r.p, hi, delta))
            .collect();
        let 下: Vec<&ExtendRow> = if 两侧 {
            rows.iter()
                .filter(|r| jpp_value::stat::decided_down(r.p, lo, delta))
                .collect()
        } else {
            vec![]
        };
        let 上错 = 上.iter().filter(|r| !r.label).count();
        let 下错 = 下.iter().filter(|r| r.label).count();
        let 检验 = |alpha: f64| -> Result<(), String> {
            let need = jpp_value::stat::n_needed_zero_error(alpha, c.conf_delta);
            if 上.len() < need || (两侧 && 下.len() < need) {
                return Err(format!(
                    "α={alpha}：样本不足（上侧已决 {}{}，每侧需 ≥ {need}）",
                    上.len(),
                    if 两侧 {
                        format!("、下侧已决 {}", 下.len())
                    } else {
                        String::new()
                    }
                ));
            }
            let ua = jpp_value::stat::binomial_upper(上错, 上.len(), c.conf_delta);
            // K 元记录没有下侧：只验上侧
            let ud = 两侧.then(|| jpp_value::stat::binomial_upper(下错, 下.len(), c.conf_delta));
            if ua > alpha || ud.is_some_and(|u| u > alpha) {
                return Err(format!(
                    "α={alpha}：上界超 α（上侧 {上错}/{} 错、上界 {ua:.3}{}）",
                    上.len(),
                    ud.map(|u| format!("；下侧 {下错}/{} 错、上界 {u:.3}", 下.len()))
                        .unwrap_or_default()
                ));
            }
            Ok(())
        };
        let alpha = match 检验(c.alpha) {
            Ok(()) => c.alpha,
            Err(正式) => match opt.alpha_trial.filter(|t| *t > c.alpha) {
                Some(t) => match 检验(t) {
                    Ok(()) => t,
                    Err(试用) => return Err(format!("扩展认证不过：{正式}；{试用}")),
                },
                None => return Err(format!("扩展认证不过：{正式}")),
            },
        };
        let fingerprint = jpp_value::stat::ScopeRanges::from_texts(
            rows.iter().map(|r| r.text.as_str()),
            opt.quantiles,
            Some(opt.margins),
        )
        .expect("rows 非空");
        let ext = ScopeExtension {
            fingerprint,
            n_up: 上.len(),
            n_down: 下.len(),
            alpha,
            batch: opt.batch.clone(),
        };
        let r = self.records.get_mut(key).expect("刚读过");
        r.scope
            .as_mut()
            .expect("刚核过有指纹")
            .extensions
            .push(ext.clone());
        Ok(ext)
    }
}
