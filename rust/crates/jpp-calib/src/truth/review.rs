//! 真值通道的两组辅助（从 `truth.rs` 拆出，控制单文件行数）：序贯到达顺序（B87、B88，步 20h）；
//! 复核行不带读数的回接与有效 α（B89，步 20i）。

use std::collections::{BTreeMap, BTreeSet};

use super::{ImportOptions, LabelRow, SeqImport, is_human};
use crate::calib::{CalibStore, SpotCheck};
use jpp_value::value::Op;

/// 序贯的到达顺序（B87、B88）：随机顺序为规范序带标注样本的确定性置换；两端先标时按待标清单的顺序，
/// 已标条目必须是清单的前缀（最后一组可以没标完）。
pub(super) fn 序贯到达(
    store: &CalibStore,
    key: &str,
    chosen: &[(&LabelRow, bool)],
    sq: &SeqImport,
    opt: &ImportOptions,
) -> Result<crate::calib::SeqSpec, String> {
    let mut 规范: Vec<(f64, bool)> = chosen.iter().map(|(r, l)| (r.p, *l)).collect();
    规范.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (arrival, pool, order) = if !sq.two_ends {
        // B87 修订 (2)：到达顺序必须与标签无关。先按 (p, item) 排序（item 是材料标识），按种子置换，
        // 再按 (p, 真值) 逐条对到规范序的空槽。旧做法按 (p, 真值) 规范序置换，并列读数块里谁先到由标签决定
        let mut 行序: Vec<(f64, &str, bool)> = chosen
            .iter()
            .map(|(r, l)| (r.p, r.item.as_str(), *l))
            .collect();
        行序.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)));
        let mut 用 = vec![false; 规范.len()];
        let mut arrival = vec![];
        for i in crate::calib::random_arrival(行序.len(), opt.seed) {
            let v = (行序[i].0, 行序[i].2);
            let k = (0..规范.len())
                .find(|k| !用[*k] && 规范[*k] == v)
                .ok_or("认证不过：到达顺序对不上标注")?;
            用[k] = true;
            arrival.push(k);
        }
        (arrival, None, "random")
    } else {
        let frame = sq
            .frame
            .as_ref()
            .ok_or("认证不过：两端先标需要抽样框（--from-ledger）")?;
        let ps: Vec<f64> = frame.iter().map(|x| x.1).collect();
        let delta = store.delta_for(&store.get(key), Op::Test).ok_or(
            "记录没有 δ：两端先标要调用方先给 δ（calib-import 从 --profile 取；步 15d-2）",
        )?;
        let 顺序 = crate::calib::two_ends_order(
            &ps,
            opt.alpha,
            opt.conf_delta,
            delta,
            opt.step,
            &sq.weights,
            opt.seed,
        );
        let 已标: BTreeMap<&str, (f64, bool)> = chosen
            .iter()
            .map(|(r, l)| (r.item.as_str(), (r.p, *l)))
            .collect();
        let 前缀: Vec<&str> = 顺序
            .iter()
            .take(已标.len())
            .map(|(i, _)| frame[*i].0.as_str())
            .collect();
        if 前缀.iter().any(|x| !已标.contains_key(x)) {
            return Err(format!(
                "待核：两端先标的标注不是清单顺序的前缀（已标 {} 条，须是清单前 {} 条）",
                已标.len(),
                已标.len()
            ));
        }
        // 到达下标：按清单前缀逐条找规范序里尚未用过的同值样本
        let mut 用 = vec![false; 规范.len()];
        let mut arrival = vec![];
        for x in &前缀 {
            let v = 已标[x];
            let i = (0..规范.len())
                .find(|i| !用[*i] && 规范[*i] == v)
                .ok_or("认证不过：到达顺序对不上标注")?;
            用[i] = true;
            arrival.push(i);
        }
        (arrival, Some(ps), "two-ends")
    };
    Ok(crate::calib::SeqSpec {
        pool,
        arrival,
        two_ends: sq.two_ends,
        order: order.into(),
        seed: opt.seed,
        batch: sq.batch,
        weights: sq.weights,
        coverage_target: sq.coverage_target,
    })
}

/// 这一行指向哪道题（键或题式键；类行按来源身份），复核行按它与同材料的标注行配对。
fn 题身份(r: &LabelRow) -> String {
    let base = match (&r.key, &r.form) {
        (Some(k), _) => k.clone(),
        (None, Some(f)) => f.key().unwrap_or_default(),
        _ => String::new(),
    };
    format!("{base}\u{1f}{}", r.class.clone().unwrap_or_default())
}

/// **B89：复核文件不得带读数与出口。** 复核行（带 `spot_check`）出现 `p`、`pick`、`exit` 或 `reading` 即拒收
/// 整批（`E-review-leak`）；复核行的读数（与 K 元的 argmax）从同一道题、同一材料的标注行回接，
/// 没有标注行可回接时拒收。非复核行照旧要自带读数。
pub(super) fn 复核行回接(rows: &[LabelRow]) -> Result<Vec<LabelRow>, String> {
    let mut 标注: BTreeMap<(String, String), (f64, Option<usize>)> = BTreeMap::new();
    for r in rows
        .iter()
        .filter(|r| r.spot_check.is_none() && !r.p.is_nan())
    {
        标注
            .entry((题身份(r), r.item.clone()))
            .or_insert((r.p, r.pick));
    }
    let mut out = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let mut r = r.clone();
        if r.spot_check.is_some() {
            let 漏 = [
                (!r.p.is_nan()).then_some("读数 p"),
                r.pick.is_some().then_some("argmax pick"),
                r.exit.is_some().then_some("出口 exit"),
                r.reading.is_some().then_some("读数 reading"),
            ];
            if let Some(f) = 漏.into_iter().flatten().next() {
                return Err(format!(
                    "E-review-leak: 第 {} 行是复核行却带了{f}（B89：复核者不得见判断器的答案；复核行只写 item、label、source、spot_check，读数由标注行回接）",
                    i + 1
                ));
            }
            let (p, pick) = 标注
                .get(&(题身份(&r), r.item.clone()))
                .copied()
                .ok_or_else(|| {
                    format!(
                        "第 {} 行：复核行 {} 没有同一道题的标注行可回接读数",
                        i + 1,
                        r.item
                    )
                })?;
            r.p = p;
            r.pick = pick;
        } else if r.exit.is_some() || r.reading.is_some() {
            return Err(format!(
                "第 {} 行：标注行不收 exit / reading 字段（读数只写在 p）",
                i + 1
            ));
        }
        out.push(r);
    }
    Ok(out)
}

/// B89 的有效 α：返回（写进证书的 `AlphaEff`，是否降为试用，是否为临时上岗）。已决区 A 按新证书的线与 δ 算。
/// 解读 (b)（`附注/2026-09-25-批量裁定.md` §五，步 20c）：alpha_eff ≤ α 按 α 取等级；α < alpha_eff ≤ 试用 α 为
/// `Trial`；alpha_eff > 试用 α 为 `Provisional`（路由、不放行）。
#[allow(clippy::too_many_arguments)]
pub(super) fn 有效阿尔法(
    store: &CalibStore,
    key: &str,
    addr: &str,
    chosen: &[(&LabelRow, bool)],
    复核对: &[(f64, bool)],
    spot: Option<&SpotCheck>,
    reviewers: &BTreeSet<String>,
    opt: &ImportOptions,
) -> Option<(crate::calib::AlphaEff, bool, bool)> {
    let rec = store.records.get(key)?;
    let c = rec.certs.get(addr)?;
    let op = rec
        .samples
        .iter()
        .find(|s| s.label.is_some())
        .map(|s| match s.phys.as_str() {
            "choice" => Op::Select,
            "score" => Op::Measure,
            _ => Op::Test,
        })
        .unwrap_or(Op::Test);
    let _ = op;
    // 与 cut 同一个 δ 与同一边界比较（步 15d-2）；取不到 δ 时已决区为空
    let delta = CalibStore::cert_delta(rec, c);
    let (hi, lo) = (c.hi, rec.lower.as_ref().map(|l| l.hi));
    let 在已决区 = |p: f64| {
        delta.is_some_and(|d| {
            jpp_value::stat::decided_up(p, hi, d)
                || lo.is_some_and(|lo| jpp_value::stat::decided_down(p, lo, d))
        })
    };
    let baseline = if reviewers.is_empty() {
        "human".to_string()
    } else {
        format!(
            "model:{}",
            reviewers
                .iter()
                .map(|r| r.trim_start_matches("model:"))
                .collect::<Vec<_>>()
                .join("、")
        )
    };
    // 1. 复核全覆盖：A 内每条进线行的真值都是构造、人工或复核行，且记录里没有这批之外的样本
    let 本批全部 = rec.labeled() == chosen.len();
    let 全覆盖 = 本批全部
        && chosen
            .iter()
            .filter(|(r, _)| 在已决区(r.p))
            .all(|(r, _)| is_human(&r.source) || r.spot_check.is_some());
    let (ae, conf, basis) = if 全覆盖 {
        (
            c.alpha,
            jpp_value::stat::alpha_eff_conf(opt.conf_delta, None),
            "full-review",
        )
    } else {
        let 区内复核: Vec<&(f64, bool)> = 复核对.iter().filter(|(p, _)| 在已决区(*p)).collect();
        let rc = Some(opt.spot_check_conf);
        if !区内复核.is_empty() {
            let agree = 区内复核.iter().filter(|x| x.1).count() as u64;
            let a_lb =
                jpp_value::stat::agreement_lower(agree, 区内复核.len() as u64, opt.spot_check_conf);
            (
                jpp_value::stat::alpha_eff(c.alpha, a_lb, None),
                jpp_value::stat::alpha_eff_conf(opt.conf_delta, rc),
                "review-in-A",
            )
        } else if let Some(sc) = spot {
            let a_lb = sc.lower.unwrap_or_else(|| {
                jpp_value::stat::agreement_lower(sc.agree, sc.n, opt.spot_check_conf)
            });
            let 带标注: Vec<f64> = rec
                .samples
                .iter()
                .filter(|s| s.label.is_some())
                .filter_map(|s| s.p)
                .collect();
            let cov =
                带标注.iter().filter(|p| 在已决区(**p)).count() as f64 / 带标注.len().max(1) as f64;
            (
                jpp_value::stat::alpha_eff(c.alpha, a_lb, Some(cov)),
                jpp_value::stat::alpha_eff_conf(opt.conf_delta, rc),
                "review-over-c",
            )
        } else {
            (
                jpp_value::stat::ALPHA_EFF_NONE,
                jpp_value::stat::alpha_eff_conf(opt.conf_delta, rc),
                "no-review",
            )
        }
    };
    // 试用上限：导入时的试用 α；没开试用（或不大于 α）时按缺省值（B89 解读 (b)）
    let 试用上限 = opt
        .alpha_trial
        .filter(|t| *t > c.alpha)
        .unwrap_or(jpp_value::stat::ALPHA_TRIAL_DEFAULT);
    let 超 = ae > c.alpha;
    let 临时 = ae > 试用上限;
    Some((
        crate::calib::AlphaEff {
            alpha_eff: ae,
            conf,
            truth_baseline: baseline,
            basis: basis.into(),
            // 只在 alpha_eff 超过 α 时记（此时等级取决于它），其余证书逐字节同 20i
            trial_alpha: 超.then_some(试用上限),
        },
        超 && !临时,
        临时,
    ))
}
