//! 装载时重跑认证（步 20c；J-03 文件面、B86、B87、B104-1、K-085）。
//!
//! `load` 对每条带证书、非夹具的记录，按证书写的方法（`selection.method`）在记录自带的带标注样本上
//! 重跑同一个认证过程。
//!
//! **复现判据**（主会话 2026-09-25 定，待 Fable 在批量裁定里确认）：重跑认证通过，且**决定出口的量**
//! 逐位相同：
//! - 每张证书的 `hi`、`alpha`、`conf_delta`、`cost`、`cluster_unit`、`grade`；
//! - 方法与方法参数，即认证过程的输入（见 [`方法参数`]）；
//! - 记录的 `hi`、`lo` 与下侧证书的 `hi`。
//!
//! 过程的输出（已决数、错数、`ucb`、`selection` 里的 n_select、n_certify、candidates、generated、stop_index、
//! 序贯的停时与 E）**不参与比较**：它们随规范序定义的改动而变，不决定出口。
//! 复现后把重跑出的证书与测出的 unsure 率写回记录，证书上写的就是验证过的数。
//!
//! 不复现、跑不成、没有重跑过程的方法（同批两侧选线，步 20g 已删）、标注集缺失，记录一律降为夹具
//! （`fixture = true`，等级 `Fixture`，出口照常路由、不放行）。手写或改过线的证书因此拿不到放行等级。
//!
//! B104-1 的旧证书（按 δ 平移过而 `selection.delta` 缺）：用当前 δ（装载时的画像；无画像为兜底）重跑，
//! 线逐位复现即把 δ 写回 `selection.delta`（h、l 由样本定，hi = h − δ 只有一个解），否则降夹具。

use super::*;

/// 一条记录在装载时重跑的结论。
#[derive(Clone, Debug, PartialEq)]
pub enum RerunOutcome {
    /// 不重跑：没有证书，或本来就是夹具记录
    NotApplicable,
    /// 复现，记录逐字节不变
    Reproduced,
    /// 复现，记录按重跑结果改写（写回 δ、验证过的计数或 unsure 率），附改了什么
    Rewritten(Vec<String>),
    /// 不复现，降为夹具，附原因
    Demoted(String),
}

/// 方法参数：`selection` 里作为认证过程**输入**的字段（method、seed、rule、step、δ；序贯另有顺序名、种子、批、
/// 权重、覆盖目标、抽样框、到达顺序）。过程的**输出**（n_select、n_certify、candidates、generated、stop_index、
/// 序贯的停时与各候选的 E）与已决数、ucb 同属计数，不参与比较：它们随规范序定义的改动而变，
/// 而不决定出口。
fn 方法参数(s: &Option<Selection>) -> Option<Selection> {
    s.clone().map(|x| Selection {
        n_select: 0,
        n_certify: 0,
        candidates: 0,
        generated: None,
        stop_index: None,
        sequential: x.sequential.map(|q| SeqCert {
            stopped_at: 0,
            e_upper: vec![],
            n_upper: vec![],
            e_lower: vec![],
            n_lower: vec![],
            ..q
        }),
        ..x
    })
}

/// 决定出口的量里第一个不同的字段名（复现判据的证书一半）；全同为 `None`。
fn 线差(a: &Cert, b: &Cert) -> Option<&'static str> {
    if a.hi != b.hi {
        Some("hi")
    } else if a.alpha != b.alpha {
        Some("alpha")
    } else if a.conf_delta != b.conf_delta {
        Some("conf_delta")
    } else if a.cost != b.cost {
        Some("cost")
    } else if a.cluster_unit != b.cluster_unit {
        Some("cluster_unit")
    } else if a.grade != b.grade {
        Some("grade")
    } else if 方法参数(&a.selection) != 方法参数(&b.selection) {
        Some("selection")
    } else {
        None
    }
}

fn 写带宽(c: &mut Cert, d: f64) {
    if let Some(s) = c.selection.as_mut() {
        s.delta = Some(d);
    }
}

/// 一张证书按某个 δ 重跑复现后的产物：新证书、新下侧证书（选中证书才有意义）、临时库里的记录。
struct 复现 {
    新: Cert,
    新下: Option<Cert>,
    新记录: CalibRecord,
}

impl CalibStore {
    /// **按证书重跑认证**（步 20c，B117）。只读 `self`，返回结论与改写后的记录
    /// （`Reproduced` / `NotApplicable` 时为 `None`）。
    pub fn rerun_record(&self, rec: &CalibRecord) -> (RerunOutcome, Option<CalibRecord>) {
        if rec.certs.is_empty() || rec.fixture {
            // 没有证书的记录本来就是夹具线（`fixture_line`）；夹具记录的证书不作凭据
            return (RerunOutcome::NotApplicable, None);
        }
        let 降 = |why: String| {
            let mut r = rec.clone();
            r.fixture = true;
            (RerunOutcome::Demoted(why), Some(r))
        };
        let 选中 = rec.选中的证书().cloned().expect("certs 非空");
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return 降("标注集缺失：记录里没有带标注的样本".into());
        }
        let 指纹 = 标注集指纹(&带标注);
        let ps: Vec<f64> = 带标注.iter().filter_map(|s| s.p).collect();
        // 不平移的证书（certify、代价线）：δ = 0（批量裁定解读 (a)，步 15d-2）
        let 记录带宽 = 0.0;
        let mut out = rec.clone();
        let mut 改 = vec![];
        for (addr, c) in &rec.certs {
            if c.label_fp != 指纹 {
                return 降(format!(
                    "标注集对不上：证书 {} 的标注集指纹 {} ≠ 记录里带标注样本的指纹 {指纹}",
                    短(addr),
                    c.label_fp
                ));
            }
            let 是选中 = c == &选中;
            // B117 (c)：按 δ 平移过而没记 δ 的旧证书，δ 由样本与记录的线解出（不取画像）
            let 缺带宽 = c.selection.as_ref().is_some_and(|s| s.delta.is_none());
            let (用带宽, 复现) = match c.selection.as_ref().and_then(|s| s.delta) {
                Some(d) => match self.试一张(rec, c, d, 是选中, false) {
                    Ok(x) => (d, x),
                    Err(why) => return 降(format!("证书 {} {why}", 短(addr))),
                },
                None if !缺带宽 => match self.试一张(rec, c, 记录带宽, 是选中, false) {
                    // 不平移的证书（certify、代价线）：线就是被检验区的边，δ = 0（步 15d-2）
                    Ok(x) => (记录带宽, x),
                    Err(why) => return 降(format!("证书 {} {why}", 短(addr))),
                },
                None => {
                    let 下 = if 是选中 {
                        rec.lower.as_ref().map(|l| l.hi)
                    } else {
                        None
                    };
                    let mut 成: Vec<(f64, 复现)> = vec![];
                    let mut 首因: Option<String> = None;
                    for d in jpp_value::stat::shift_candidates(&ps, c.hi, 下) {
                        match self.试一张(rec, c, d, 是选中, true) {
                            Ok(x) => 成.push((d, x)),
                            Err(why) => {
                                首因.get_or_insert(why);
                            }
                        }
                    }
                    match 成.len() {
                        1 => 成.pop().expect("恰一个"),
                        0 => {
                            return 降(format!(
                                "证书 {} 缺 δ，由样本与线解不出能复现的 δ（B117 (c)）{}",
                                短(addr),
                                首因.map(|w| format!("：{w}")).unwrap_or_default()
                            ));
                        }
                        _ => {
                            let ds: Vec<String> = 成.iter().map(|(d, _)| format!("{d}")).collect();
                            return 降(format!(
                                "证书 {} 缺 δ，由样本与线解出的 δ 不唯一（{}，B117 (c)）",
                                短(addr),
                                ds.join("、")
                            ));
                        }
                    }
                }
            };
            let 复现 {
                mut 新,
                mut 新下,
                新记录,
            } = 复现;
            if 是选中 {
                // 有效 α（B89）是导入在认证之后写的，不是认证过程的输出：沿用
                if let (Some(n), Some(o)) = (新下.as_mut(), rec.lower.as_ref()) {
                    n.eff = o.eff.clone();
                }
                if out.lower != 新下 {
                    改.push("下侧证书改写为重跑结果".into());
                    // B117 (b)：旧证书原样移入 certs_history
                    out.certs_history.extend(rec.lower.clone());
                    out.lower = 新下;
                }
                if out.unsure_rate != 新记录.unsure_rate
                    || out.unsure_rate_delta != 新记录.unsure_rate_delta
                {
                    改.push(format!(
                        "unsure_rate {:?}（δ {:?}）→ {:?}（δ {:?}）",
                        out.unsure_rate,
                        out.unsure_rate_delta,
                        新记录.unsure_rate,
                        新记录.unsure_rate_delta
                    ));
                    out.unsure_rate = 新记录.unsure_rate;
                    out.unsure_rate_delta = 新记录.unsure_rate_delta;
                }
            }
            新.eff = c.eff.clone();
            if &新 != c {
                改.push(if 缺带宽 {
                    format!(
                        "证书 {} 写回解出的 δ={用带宽}（B104-1、B117 (c)）并改写为重跑结果",
                        短(addr)
                    )
                } else {
                    format!("证书 {} 改写为重跑结果", 短(addr))
                });
                // B117 (b)：同址替换、旧证书移入 certs_history；写回 δ 可能改变地址（有 `rule` 的证书，δ 进地址）
                out.certs_history.push(c.clone());
                out.certs.remove(addr);
                out.certs.insert(新.addr(), 新);
            }
        }
        if 改.is_empty() {
            (RerunOutcome::Reproduced, None)
        } else {
            (RerunOutcome::Rewritten(改), Some(out))
        }
    }

    /// 按 δ = `d` 重跑一张证书并按复现判据核对；`补带宽` 为真时（旧证书）把 `d` 当作证书的 δ 比较。
    fn 试一张(
        &self,
        rec: &CalibRecord,
        c: &Cert,
        d: f64,
        是选中: bool,
        补带宽: bool,
    ) -> Result<复现, String> {
        let (mut 新, 新记录) = self.重跑一张(rec, c, d)?;
        let mut 期望 = c.clone();
        let mut 新下 = 新记录.lower.clone();
        if 补带宽 {
            写带宽(&mut 期望, d);
        }
        // 旧种子分半的过程本身不写 δ（旧证书逐字节不变）；重跑产物按所用 δ 补上，写回过 δ 的证书才可再次复现
        if c.selection.is_some() {
            for x in std::iter::once(&mut 新).chain(新下.as_mut()) {
                if x.selection.as_ref().is_some_and(|s| s.delta.is_none()) {
                    写带宽(x, d);
                }
            }
        }
        if let Some(f) = 线差(&新, &期望) {
            return Err(format!(
                "重跑不复现：{f} 不同（记录 hi={}；重跑 hi={}，δ={d}）",
                c.hi, 新.hi
            ));
        }
        if 是选中 {
            // 线由选中证书定：记录的 hi / lo 与下侧证书的线也要复现
            let 下同 = match (&rec.lower, &新下) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    let mut a = a.clone();
                    if 补带宽 {
                        写带宽(&mut a, d);
                    }
                    线差(&a, b).is_none()
                }
                _ => false,
            };
            if !(下同 && 新记录.hi == rec.hi && 新记录.lo == rec.lo) {
                return Err(format!(
                    "重跑后记录的线不复现（记录 hi={}、lo={}；重跑 hi={}、lo={}{}，δ={d}）",
                    rec.hi,
                    rec.lo,
                    新记录.hi,
                    新记录.lo,
                    if 下同 {
                        ""
                    } else {
                        "；下侧证书的线不同"
                    }
                ));
            }
        }
        Ok(复现 {
            新, 新下, 新记录
        })
    }

    /// 在只含这条记录的临时库里按一张证书的方法重跑：记录去掉证书与下侧证书、状态置「待真值」、
    /// δ 固定为 `d`。返回重跑出的证书与临时库里的记录。
    fn 重跑一张(
        &self,
        rec: &CalibRecord,
        c: &Cert,
        d: f64,
    ) -> Result<(Cert, CalibRecord), String> {
        let key = rec.key.clone();
        let mut 临时 = CalibStore::new();
        临时.profile = self.profile.clone();
        let mut r = rec.clone();
        r.certs.clear();
        r.lower = None;
        r.status = "待真值".into();
        r.fixture = false;
        r.delta = Some(d);
        r.unsure_rate = None;
        r.unsure_rate_delta = None;
        临时.records.insert(key.clone(), r);
        let 两侧 = rec
            .samples
            .iter()
            .find(|s| s.label.is_some())
            .is_none_or(|s| s.phys == "noul");
        let (a, dc, g) = (c.alpha, c.conf_delta, c.grade);
        let res = match &c.selection {
            None => match (c.cost, c.resample.as_ref()) {
                // 代价证书按它自己的等级重跑（步 20a-2a：`calib-import --cost` 会写试用级代价证书；
                // 按正式等级重跑时等级不同，判「不复现」而降夹具）。依据：B117 (a)（试用位是决定等级的量）
                (Some(cost), _) => {
                    临时.commission_costed_graded(&key, a, dc, &c.cluster_unit, cost, g)
                }
                (None, Some((0, 规则))) if 规则.starts_with("两侧联合选线：") => {
                    return Err("的方法是同批两侧选线（步 20g 已删除，没有重跑过程）".into());
                }
                (None, _) => 临时.commission(&key, a, dc, &c.cluster_unit),
            },
            Some(s) => match (s.method.as_str(), 两侧) {
                ("split" | "split-strata", true) => {
                    临时.commission_two_sided_split_graded(&key, a, dc, s.seed, g)
                }
                ("split" | "split-strata", false) => {
                    临时.commission_upper_split_graded(&key, a, dc, s.seed, g)
                }
                ("split-stratified" | "split-strata-stratified", true) => {
                    临时.commission_two_sided_split_stratified_graded(&key, a, dc, s.seed, g)
                }
                ("split-stratified" | "split-strata-stratified", false) => {
                    临时.commission_upper_split_stratified_graded(&key, a, dc, s.seed, g)
                }
                ("fixed-sequence", true) => {
                    临时.commission_two_sided_fixed_sequence_graded(&key, a, dc, s.step, g)
                }
                ("fixed-sequence", false) => {
                    临时.commission_upper_fixed_sequence_graded(&key, a, dc, s.step, g)
                }
                ("sequential", 两) => {
                    let Some(sq) = &s.sequential else {
                        return Err("的方法 sequential 缺序贯过程记录".into());
                    };
                    let spec = SeqSpec {
                        pool: Some(sq.pool.clone()),
                        arrival: sq.arrival.clone(),
                        two_ends: sq.order == "two-ends",
                        order: sq.order.clone(),
                        seed: sq.seed,
                        batch: sq.batch,
                        weights: sq.weights,
                        coverage_target: sq.coverage_target,
                    };
                    if 两 {
                        临时.commission_two_sided_sequential_graded(&key, a, dc, s.step, g, &spec)
                    } else {
                        临时.commission_upper_sequential_graded(&key, a, dc, s.step, g, &spec)
                    }
                }
                (m, _) => return Err(format!("的方法 {m:?} 没有重跑过程")),
            },
        };
        match res {
            Ok(新) => Ok((新, 临时.records.remove(&key).expect("刚插进去"))),
            Err(Refusal::认证不过(_)) => Err("重跑认证不过".into()),
            Err(Refusal::跑不成(why)) => Err(format!("重跑跑不成：{why}")),
        }
    }

    /// 对库里每条记录按证书重跑（步 20c），按结论改写或降级，返回装载报告（每条非「复现」的记录一行）。
    pub fn recertify_all(&mut self) -> Vec<String> {
        let mut keys: Vec<String> = self.records.keys().cloned().collect();
        keys.sort();
        let mut report = vec![];
        for k in keys {
            let (o, r) = self.rerun_record(&self.records[&k]);
            if let Some(r) = r {
                self.records.insert(k.clone(), r);
            }
            match o {
                // 依据：J-03 文件面（`20` v2 §2.3 不变量）、B104-1（旧证书证明 δ 后写回）
                RerunOutcome::Demoted(why) => report.push(format!(
                    "W-calib-rerun: 键 {} 降为夹具（J-03 文件面，步 20c）：{why}",
                    短(&k)
                )),
                RerunOutcome::Rewritten(what) => report.push(format!(
                    "calib-rerun: 键 {} 重跑复现，记录已改写：{}",
                    短(&k),
                    what.join("；")
                )),
                RerunOutcome::Reproduced | RerunOutcome::NotApplicable => {}
            }
        }
        report
    }
}

fn 短(addr: &str) -> String {
    addr.replace('\u{1f}', "·")
}
