//! 上岗的正门：`commission` 系列认证与选线辅助。（拆 calib.rs：原第 801–1386、1697 行起）

use jpp_value::value::Op;

use super::*;
use jpp_value::stat::Certificate;

impl CalibStore {
    /// **上岗的正门**：拿这条键积累来的标注样本跑保形认证，**认过才上岗，线由证书定**。
    ///
    /// `cluster_unit` **必须由调用方声明**，不许从数据推断。推断出来的默认会造出一张
    /// 写着「按条核过」的证书，**而真相是没人说过簇是什么**——那正是「给没有类型的东西
    /// 补来源」那个形状。声明「对象段」而样本没有簇 id 是**错，不是降级**。
    ///
    /// 认证不过时返回 `Certificate::Refused`，**带着 `n_needed`**——
    /// 它是唯一告诉作者「这条路有终点」的东西。**记录不动，留在 `待真值`**：
    /// 不阻塞、但也不放行。
    pub fn commission(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
    ) -> Result<Cert, Refusal> {
        self.commission_inner(
            key,
            alpha,
            conf_delta,
            cluster_unit,
            None,
            CertGrade::Formal,
        )
    }

    /// **代价矩阵定线、证书定能不能上岗**（那条裁定的两半合起来）。
    ///
    /// `certify` 自己会去找一条最宽的、仍被认证住的线；**给了代价矩阵就不找了**——
    /// 线由 `cost_line` 在标注集上按 `fp·#误放行 + fn·#漏放行` 最小定出来，
    /// 证书只回答**这条线在这批数据上的假放行上界够不够 α**。
    /// 这是那条裁定的字面实现：**代价决定线定在哪，证书决定能不能上岗，不是二选一。**
    pub fn commission_costed(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
        cost: (f64, f64),
    ) -> Result<Cert, Refusal> {
        self.commission_costed_graded(
            key,
            alpha,
            conf_delta,
            cluster_unit,
            cost,
            CertGrade::Formal,
        )
    }

    /// 代价线带认证等级（步 20a-2a，B129 的 `calib-import --cost`）：导入与其他认证方式同一套 B72 规则，
    /// 正式 α 不过再按试用 α 认证，证书记 `Trial`（可路由、不放行不可逆 `do`）；`load` 重跑时按证书自己的等级。
    /// 定线与证书内容与 [`CalibStore::commission_costed`] 相同，只多一个等级。
    pub fn commission_costed_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
        cost: (f64, f64),
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        self.commission_inner(key, alpha, conf_delta, cluster_unit, Some(cost), grade)
    }

    fn commission_inner(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        cluster_unit: &str,
        cost: Option<(f64, f64)>,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        // **先卡区间再造证书**：α=0 会让 `n_needed_zero_error` 得到 inf，
        // 而 `serde_json::to_value` 在 NaN/inf 上失败 → `calib_hash` 把整条记录哈成 Null，
        // **两条坏记录于是哈出同一个值**。区间在进哈希之前卡住。
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = match self.records.get(key) {
            Some(r) => r,
            None => return Err(bad("没有这条记录")),
        };
        if rec.status == "停岗" {
            // 停岗是人下的判断，不是样本数的函数——**重新认证不是复岗的通道**
            return Err(bad("停岗的键不经由 commission 复岗"));
        }
        let rec_lsrc = rec.label_source.clone();
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        // **一条记录只能有一个题型。** 代价路径一直查这个，而这条非代价路径从来没查过——
        // **不同尺不可比**是本项目自己的判据，认证路径上漏了一处。
        //
        // 这一条也是 `LabelSource::选择子集.与对错相关` **能是一个标量的前提**：
        // 相关性按题型分叉（noul +0.527 / choice +0.418 / **score −0.062，反向**），
        // 一条记录一个值之所以够用，**正是因为一条记录就是一个题型**。
        // 在这之前那是约定不是保证——`absorb` 收任何 `phys` 字符串。
        {
            let mut 见到: std::collections::BTreeSet<&str> = Default::default();
            for s in &带标注 {
                见到.insert(s.phys.as_str());
            }
            if 见到.len() > 1 {
                return Err(Refusal::跑不成(format!(
                    "键 {key} 的标注样本混了 {} 种物理形式（{}）：不同尺不可比，一条记录只能认一个题型的线",
                    见到.len(),
                    见到.into_iter().collect::<Vec<_>>().join(" / ")
                )));
            }
        }
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        // **给 `unsure_rate` 算出料用的两样**，趁 `rec` / `带标注` 还活着先取下来。
        // δ 的口径必须与 `cut` 里那一处**同源**（`delta_for`：记录自带的优先，否则档案的），
        // 否则「认证时测的 unsure 率」与「运行期真的会 unsure 的率」测的不是同一件事。
        let 题型认证时 = 反查题型(&带标注[0].phys);
        let 众数认证时: Vec<Option<f64>> = 带标注.iter().map(|s| s.mode_share).collect();
        // 三种题型的出口都带 δ 迟滞（B63 起 choice / score 也是），率都绑 δ
        let 绑delta = |d: Option<f64>| d;
        // certify 线与代价线不按 δ 平移，线就是检验区的边：cut 用 δ = 0（批量裁定解读 (a)，步 15d-2），
        // 率按同一个 δ 测
        let delta认证时: Option<f64> = 题型认证时.map(|_| 0.0);
        // **指纹算的是「用到的那些 `(p, label)` 对的规范形」，不是源文件字节。**
        // 源文件会被重新导出、重新排序——按字节算会在数据没变时乱跳，
        // **而乱跳的检查会教会人绕过它**。
        let label_fp = 标注集指纹(&带标注);
        if !rec.label_fp.is_empty() && rec.label_fp != label_fp {
            return Err(Refusal::跑不成(format!(
                "键 {key} 声明的标注集是 {:?}（指纹 {}），而这次用到的那批指纹是 {label_fp}：对不上就不认证",
                rec.label_set_id, rec.label_fp
            )));
        }
        let 按条: Vec<(f64, bool)> = 带标注
            .iter()
            .map(|s| (s.p.expect("已滤"), s.label == Some(1)))
            .collect();

        // **代价线分支**：线不由证书自己找，由代价矩阵在标注集上定。
        if let Some((fp, fn_)) = cost {
            // Python `runtime.py:1147` 直接 raise：select / measure 的代价线**未定**。
            // 让它悄悄什么也不做，比报错糟。
            if 带标注.iter().any(|s| s.phys != "noul") {
                return Err(Refusal::跑不成(format!(
                    "cut(cost=) 只对 test 题有定义（select / measure 的代价线未定）；键 {key} 上有非 noul 的样本"
                )));
            }
            // J-16：代价线与保形线不能同源。**这一条 Python 也拦**（`runtime.py:1151`）。
            let (lsid, sid) = (rec.label_set_id.clone(), rec.set_id.clone());
            if lsid.is_empty() || lsid == sid {
                return Err(Refusal::跑不成(format!(
                    "J-16: 校准键 {key} 的标注集 id（{lsid:?}）必须给出且 ≠ 保形集 id（{sid:?}）；代价线与保形线不能同源"
                )));
            }
            let cl = match jpp_value::stat::cost_line(&按条, fp, fn_) {
                Ok(c) => c,
                Err(e) => return Err(Refusal::跑不成(e)),
            };
            // 证书只回答「这条线够不够 α」，不再自己找线
            let ucb = jpp_value::stat::binomial_upper(cl.n_false_accept, cl.n_accepted, conf_delta);
            if cl.n_accepted == 0 || ucb > alpha {
                return Err(Refusal::认证不过(Certificate::Refused {
                    best_ucb: ucb,
                    best_hi: cl.line,
                    best_n_accepted: cl.n_accepted,
                    n_needed: jpp_value::stat::n_needed_zero_error(alpha, conf_delta),
                }));
            }
            let cert = Cert {
                alpha,
                conf_delta,
                hi: cl.line,
                n_accepted: cl.n_accepted,
                n_errors: cl.n_false_accept,
                ucb,
                cluster_unit: cluster_unit.into(),
                resample: None,
                cost,
                bounded_side: 单侧声明(),
                label_source: rec_lsrc.clone(),
                label_fp: label_fp.clone(),
                selection: None,
                grade,
                eff: None,
            };
            let r = self.records.get_mut(key).expect("刚读过");
            r.certs.insert(cert.addr(), cert.clone());
            let 选中 = r.选中的证书().cloned().expect("刚插进去");
            r.hi = 选中.hi;
            r.lo = 0.0;
            r.status = "上岗".into();
            r.fixture = false;
            r.unsure_rate =
                经验unsure率(&按条, &众数认证时, 题型认证时, r.hi, r.lo, delta认证时);
            r.unsure_rate_delta = 绑delta(delta认证时);
            return Ok(cert);
        }
        let (cert_result, resample) = if cluster_unit == "条" {
            (jpp_value::stat::certify(&按条, alpha, conf_delta), None)
        } else {
            // 声明了簇，就必须每条都带簇 id
            if 带标注.iter().any(|s| s.cluster.is_none()) {
                return Err(bad("声明了簇单位，但有样本没有簇 id：这是错，不是降级"));
            }
            let 三元: Vec<(f64, bool, String)> = 带标注
                .iter()
                .map(|s| {
                    (
                        s.p.expect("已滤"),
                        s.label == Some(1),
                        s.cluster.clone().expect("已核"),
                    )
                })
                .collect();
            // **全过才算过**（说不准往拒绝那边倒）。这条规则**未标定**——原型只报了
            // 「200 次里有解几次」，没有定「几次算过」。记进证书是为了它可审。
            const R: usize = 200;
            let mut 最差: Option<Certificate> = None;
            let mut 全过 = true;
            let mut 成的: Option<Certificate> = None;
            for seed in 0..R as u64 {
                let c = jpp_value::stat::certify(
                    &jpp_value::stat::cluster_subsample(&三元, seed),
                    alpha,
                    conf_delta,
                );
                if c.is_refused() {
                    全过 = false;
                    最差 = Some(c);
                    break;
                }
                if 成的.is_none() {
                    成的 = Some(c);
                }
            }
            let out = if 全过 {
                成的.expect("R > 0")
            } else {
                最差.expect("刚设的")
            };
            (out, Some((R, "全过才算过（未标定）".to_string())))
        };

        match cert_result {
            Certificate::Refused { .. } => Err(Refusal::认证不过(cert_result)),
            Certificate::Line {
                hi,
                n_accepted,
                n_errors,
                ucb,
            } => {
                let cert = Cert {
                    alpha,
                    conf_delta,
                    hi,
                    n_accepted,
                    n_errors,
                    ucb,
                    cluster_unit: cluster_unit.into(),
                    resample,
                    cost,
                    bounded_side: 单侧声明(),
                    label_source: rec_lsrc.clone(),
                    label_fp: label_fp.clone(),
                    selection: None,
                    grade,
                    eff: None,
                };
                let r = self.records.get_mut(key).expect("刚读过");
                // 同一个地址是**更新那一格**；不同地址是**新增一格**
                r.certs.insert(cert.addr(), cert.clone());
                // `hi`/`lo` 是**选择规则算出来的视图**，不是第二处真相——
                // 它和 `line_source` 报的那张必须出自同一个函数，否则两处会各说各的。
                let 选中 = r.选中的证书().cloned().expect("刚插进去");
                r.hi = 选中.hi;
                // **线由证书定，不由调用方写。**
                //
                // `lo = 0.0`：**证书只管放行那一侧**（损失 = 放行区里的假放行），
                // 弃权那一侧它一个字也没说。没有凭据就不放行任何 `Ignore`，
                // 于是带宽最大、`Unsure` 最多——**说不准往拒绝那边倒**。
                // 保留记录原有的 `lo` 是不行的：`hi` 可能落到它下面（实测 hi=0.295 而
                // 缺省 lo=0.35），那样带就翻了，而 `put` 的 `lo ≤ hi` 也会被自己人违反。
                r.lo = 0.0;
                r.status = "上岗".into();
                r.fixture = false;
                // **J-10 的 uᵢ 在这里被测出来**（`12`:591「有标注集时用**经验**联合 unsure 率」；
                // `12`:814 那一栏写的是「✓ 估计 | **实测**」）。在这之前这个字段**只有消费方没有生产者**：
                // 走完 `absorb` → `commission` 的真实路径拿到的仍是 `None`，
                // `unsure_bound` 按 1 计，**界退化成 `union_bound == n`——一条真的、但什么也没说的界。**
                r.unsure_rate =
                    经验unsure率(&按条, &众数认证时, 题型认证时, r.hi, r.lo, delta认证时);
                r.unsure_rate_delta = 绑delta(delta认证时);
                Ok(cert)
            }
        }
    }

    /// **两侧联合认证，拆分样本版**（B24 的多重比较要求）。
    ///
    /// 在同一批数据上既选线对又算二项上界时，上界只对**固定**的线成立，
    /// 对「在同批数据上最大化已决条数后选出的线」不成立——候选越多，真实错误率越可能超 α。
    /// 这里把带标注的样本排成规范序（按 `(p, 真值)`），再按 `splitmix64(seed ^ 下标)` 的最低位分成两半：
    /// **选线半**上照原规则选出一对 `(h, l)`；**认证半**上对这一对只检验一次，两侧各一个二项上界。
    ///
    /// 认证半里任一侧的已决条数小于零错误所需条数（`n_needed_zero_error`）时，
    /// 如实停在待核（`跑不成`，原因以「待核」开头），不降低门槛。
    ///
    /// 这是旧的种子分半（证书方法 `split` / `split-strata`），留给步 20c 按证书方法重跑现有记录；
    /// 新导入的 `--certify split` 走 [`Self::commission_two_sided_split_stratified_graded`]（B85）。
    pub fn commission_two_sided_split(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
    ) -> Result<Cert, Refusal> {
        self.commission_two_sided_split_graded(key, alpha, conf_delta, seed, CertGrade::Formal)
    }

    /// 同上，证书写上认证等级（B72：导入时正式 α 不过再按试用 α 认证，证书记 `Trial`）。
    /// 算法与正式档完全相同，等级只是写进证书的一个事实。
    pub fn commission_two_sided_split_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        self.两侧拆分认证(key, alpha, conf_delta, seed, grade, 分法::种子)
    }

    /// **两侧拆分认证，分层交替分半**（B85；`--certify split`）。
    ///
    /// 与 [`Self::commission_two_sided_split_graded`] 只差分半：规范序下负例段、正例段各自按序交替进
    /// 选线半与认证半，`seed` 只定各段的起点（带来源分层时各来源各段各自交替）。四格大小于是各约
    /// n/4，不再由种子偶然决定；分半只看条目在段内的序号，与它错不错无关，拆分的独立性不变。
    /// 证书方法 `split-stratified`（带来源分层 `split-strata-stratified`），规则版本 `split-stratified/v1`。
    pub fn commission_two_sided_split_stratified_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        self.两侧拆分认证(key, alpha, conf_delta, seed, grade, 分法::交替)
    }

    fn 两侧拆分认证(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
        how: 分法,
    ) -> Result<Cert, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        let pre = self.两侧前置(key, alpha, conf_delta)?;
        let delta = pre.delta;
        // **分半不许随行序变**（Codex 评审 PR #28）：同一批 `(p, 真值)` 换个行序，
        // 按插入下标分半会分出不同的两半、可能改变认证结论，而 `label_fp` 对行序不敏感，
        // 证书地址与审计元数据却分不出这两次。先排成规范序（与指纹同一口径），再按下标分。
        // 样本带分层（B75 类记录）时按来源各自分半后合并。
        let (选线半, 认证半, 分层) = 分半(&pre.条目, seed, how);
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        let (正, 负) = (
            选线半.iter().filter(|x| x.1).count(),
            选线半.iter().filter(|x| !x.1).count(),
        );
        if 正.min(负) < n_needed {
            return Err(bad(&format!(
                "待核：选线半样本不足（正例 {正}、负例 {负}，零错误也需每侧 ≥ {n_needed}；选线半 {} 条、认证半 {} 条）",
                选线半.len(),
                认证半.len()
            )));
        }
        let (best, candidates) = 选两侧线对(&选线半, delta, alpha, conf_delta);
        let Some((h, l)) = best else {
            let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: 1.0,
                best_hi: 1.0,
                best_n_accepted: 0,
                n_needed,
            }));
        };
        // 认证半：对选出的这一对只检验一次
        let up: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 >= h).collect();
        let down: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 <= l).collect();
        if up.len() < n_needed || down.len() < n_needed {
            return Err(bad(&format!(
                "待核：认证半样本不足（上侧已决 {}、下侧已决 {}，零错误也需每侧 ≥ {}；选线半 {} 条、认证半 {} 条）",
                up.len(),
                down.len(),
                n_needed,
                选线半.len(),
                认证半.len()
            )));
        }
        let ka = up.iter().filter(|x| !x.1).count();
        let kd = down.iter().filter(|x| x.1).count();
        let ua = jpp_value::stat::binomial_upper(ka, up.len(), conf_delta);
        let ud = jpp_value::stat::binomial_upper(kd, down.len(), conf_delta);
        if ua > alpha || ud > alpha {
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: ua.max(ud),
                best_hi: h,
                best_n_accepted: up.len() + down.len(),
                n_needed,
            }));
        }
        let sel = Selection {
            method: how.方法名(分层).into(),
            seed,
            n_select: 选线半.len(),
            n_certify: 认证半.len(),
            candidates,
            rule: how.规则版本().map(String::from),
            step: None,
            // 新证书记认证时的 δ（候选 B104：`cut` 据此不放宽）；旧种子分半不记，旧证书逐字节不变
            delta: how.规则版本().map(|_| delta),
            generated: None,
            stop_index: None,
            sequential: None,
        };
        let 规则 = "两侧联合选线（拆分样本）：选线半上取两区上界各 ≤ α 且已决最多的一对，认证半上对该对各检验一次".to_string();
        let (hi, lo) = ((h - delta).clamp(0.0, 1.0), (l + delta).clamp(0.0, 1.0));
        let upper = pre.证书(
            alpha,
            conf_delta,
            hi,
            (up.len(), ka, ua),
            &规则,
            上侧声明(),
            &sel,
            grade,
        );
        let lower = pre.证书(
            alpha,
            conf_delta,
            lo,
            (down.len(), kd, ud),
            &规则,
            下侧声明(),
            &sel,
            grade,
        );
        Ok(self.两侧上岗(key, upper, lower, &pre.按条(), delta))
    }

    /// 两侧认证共用的前置检查与取数：α、δ_c 区间，停岗，带标注样本，只收 test 题。
    pub(super) fn 两侧前置(
        &self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
    ) -> Result<前置, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = self.records.get(key).ok_or_else(|| bad("没有这条记录"))?;
        if rec.status == "停岗" {
            return Err(bad("停岗的键不经由认证复岗"));
        }
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        if 带标注.iter().any(|s| s.phys != "noul") {
            return Err(bad("两侧认证只对 test 题有定义"));
        }
        // 步 15d-2：平移认证的 δ 由调用方显式给（`set_delta`；CLI 从 `--profile` 取），不从画像取、不兜底
        let delta = rec.delta.ok_or_else(|| {
            bad("记录没有 δ：按 δ 平移的认证要调用方先 set_delta（CLI 的 calib-import 从 --profile 取），不兜底")
        })?;
        Ok(前置 {
            delta,
            条目: 带标注
                .iter()
                .map(|s| (s.p.expect("已滤"), s.label == Some(1), s.stratum.clone()))
                .collect(),
            label_fp: 标注集指纹(&带标注),
            lsrc: rec.label_source.clone(),
            op: Op::Test,
        })
    }

    /// 两侧证书落进记录：上侧证书进 `certs`，下侧证书进 `lower`，线由证书定，并测出 unsure 率。
    pub(super) fn 两侧上岗(
        &mut self,
        key: &str,
        upper: Cert,
        lower: Cert,
        按条: &[(f64, bool)],
        delta: f64,
    ) -> Cert {
        let (hi, lo) = (upper.hi, lower.hi);
        let r = self.records.get_mut(key).expect("刚读过");
        r.certs.insert(upper.addr(), upper.clone());
        r.hi = hi;
        r.lo = lo;
        r.lower = Some(lower);
        r.status = "上岗".into();
        r.fixture = false;
        r.unsure_rate = 经验unsure率(按条, &[], Some(Op::Test), hi, lo, Some(delta));
        r.unsure_rate_delta = Some(delta);
        upper
    }
}

impl CalibStore {
    /// **K 元划分（`select` / `measure`）的单侧认证，拆分样本版**（B63）。
    ///
    /// 样本的 `p` 是胜出候选（或档位）的概率 p_max，`label` 是「argmax 是否等于真值」。
    /// 认证与 B24 上侧同形：选线半上取二项上界 ≤ α 且已决最多的 `h`，认证半上对它只检验一次；
    /// 记录的 `hi = h − δ`，于是 `cut` 的 `p_max ≥ hi + δ` 正好是 `p_max ≥ h`。
    /// K 元划分没有否定出口，`lo` 无消费者，记 0。旧的种子分半，见两侧版的说明。
    pub fn commission_upper_split(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
    ) -> Result<Cert, Refusal> {
        self.commission_upper_split_graded(key, alpha, conf_delta, seed, CertGrade::Formal)
    }

    /// 同上，证书写上认证等级（B72：K 元单侧线同样按 α 分档）。
    pub fn commission_upper_split_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        self.单侧拆分认证(key, alpha, conf_delta, seed, grade, 分法::种子)
    }

    /// K 元单侧拆分认证，分层交替分半（B85；`--certify split`）。
    pub fn commission_upper_split_stratified_graded(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
    ) -> Result<Cert, Refusal> {
        self.单侧拆分认证(key, alpha, conf_delta, seed, grade, 分法::交替)
    }

    fn 单侧拆分认证(
        &mut self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
        seed: u64,
        grade: CertGrade,
        how: 分法,
    ) -> Result<Cert, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        let pre = self.单侧前置(key, alpha, conf_delta)?;
        let delta = pre.delta;
        let (选线半, 认证半, 分层) = 分半(&pre.条目, seed, how);
        let n_needed = jpp_value::stat::n_needed_zero_error(alpha, conf_delta);
        // 选线：h 取样本值与相邻中点；h − δ ≥ 0 才可表达
        let mut ps: Vec<f64> = 选线半.iter().map(|x| x.0).collect();
        ps.sort_by(|a, b| a.total_cmp(b));
        ps.dedup();
        let mut cands = ps.clone();
        for w in ps.windows(2) {
            cands.push((w[0] + w[1]) / 2.0);
        }
        cands.sort_by(|a, b| a.total_cmp(b));
        cands.dedup();
        let mut 满足 = 0usize;
        let mut best: Option<(f64, usize)> = None;
        for &h in &cands {
            if h < delta {
                continue;
            }
            let acc: Vec<&(f64, bool)> = 选线半.iter().filter(|x| x.0 >= h).collect();
            if acc.len() < n_needed {
                continue;
            }
            let k = acc.iter().filter(|x| !x.1).count();
            if jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha {
                满足 += 1;
                if best.map(|b| acc.len() > b.1).unwrap_or(true) {
                    best = Some((h, acc.len()));
                }
            }
        }
        let Some((h, _)) = best else {
            return Err(bad(&format!(
                "待核：选线半上没有满足 α 的单侧线（选线半 {} 条，零错误也需已决 ≥ {n_needed}）",
                选线半.len()
            )));
        };
        let up: Vec<&(f64, bool)> = 认证半.iter().filter(|x| x.0 >= h).collect();
        if up.len() < n_needed {
            return Err(bad(&format!(
                "待核：认证半样本不足（已决 {}，零错误也需 ≥ {n_needed}；选线半 {} 条、认证半 {} 条）",
                up.len(),
                选线半.len(),
                认证半.len()
            )));
        }
        let ka = up.iter().filter(|x| !x.1).count();
        let ua = jpp_value::stat::binomial_upper(ka, up.len(), conf_delta);
        if ua > alpha {
            return Err(Refusal::认证不过(Certificate::Refused {
                best_ucb: ua,
                best_hi: h,
                best_n_accepted: up.len(),
                n_needed,
            }));
        }
        let sel = Selection {
            method: how.方法名(分层).into(),
            seed,
            n_select: 选线半.len(),
            n_certify: 认证半.len(),
            candidates: 满足,
            rule: how.规则版本().map(String::from),
            step: None,
            delta: how.规则版本().map(|_| delta),
            generated: None,
            stop_index: None,
            sequential: None,
        };
        let hi = (h - delta).clamp(0.0, 1.0);
        let 规则 =
            "K 元单侧选线（拆分样本，B63）：选线半上取上界 ≤ α 且已决最多的 h，认证半上检验一次";
        let cert = pre.证书(
            alpha,
            conf_delta,
            hi,
            (up.len(), ka, ua),
            规则,
            多元声明(),
            &sel,
            grade,
        );
        Ok(self.单侧上岗(key, cert, &pre.按条(), pre.op, delta))
    }

    /// K 元单侧认证共用的前置：同一种 select / measure 样本；δ 取该题型的 δ。
    pub(super) fn 单侧前置(
        &self,
        key: &str,
        alpha: f64,
        conf_delta: f64,
    ) -> Result<前置, Refusal> {
        let bad = |why: &str| Refusal::跑不成(why.to_string());
        if !(alpha > 0.0 && alpha < 1.0) || !(conf_delta > 0.0 && conf_delta < 1.0) {
            return Err(bad("alpha 与 conf_delta 必须在开区间 (0, 1)"));
        }
        let rec = self.records.get(key).ok_or_else(|| bad("没有这条记录"))?;
        if rec.status == "停岗" {
            return Err(bad("停岗的键不经由认证复岗"));
        }
        let 带标注: Vec<&Sample> = rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .collect();
        if 带标注.is_empty() {
            return Err(bad("没有带标注的样本：线只从标注记录来"));
        }
        let phys = 带标注[0].phys.clone();
        if 带标注.iter().any(|s| s.phys != phys) || !(phys == "choice" || phys == "score") {
            return Err(bad("K 元单侧认证只对同一种 select / measure 样本有定义"));
        }
        let op = if phys == "choice" {
            Op::Select
        } else {
            Op::Measure
        };
        // 步 15d-2：δ 由调用方显式给，不从画像取、不兜底
        let _ = op;
        let delta = rec.delta.ok_or_else(|| {
            bad("记录没有 δ：按 δ 平移的认证要调用方先 set_delta（CLI 的 calib-import 从 --profile 取），不兜底")
        })?;
        Ok(前置 {
            delta,
            条目: 带标注
                .iter()
                .map(|s| (s.p.expect("已滤"), s.label == Some(1), s.stratum.clone()))
                .collect(),
            label_fp: 标注集指纹(&带标注),
            lsrc: rec.label_source.clone(),
            op,
        })
    }

    pub(super) fn 单侧上岗(
        &mut self,
        key: &str,
        cert: Cert,
        按条: &[(f64, bool)],
        op: Op,
        delta: f64,
    ) -> Cert {
        let mut 全体: Vec<(f64, bool)> = 按条.to_vec();
        全体.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let r = self.records.get_mut(key).expect("刚读过");
        r.certs.insert(cert.addr(), cert.clone());
        r.hi = cert.hi;
        r.lo = 0.0;
        r.lower = None;
        r.status = "上岗".into();
        r.fixture = false;
        r.unsure_rate = 经验unsure率(&全体, &[], Some(op), cert.hi, 0.0, Some(delta));
        r.unsure_rate_delta = Some(delta);
        cert
    }
}

/// 认证前取好的数：带宽 δ、`(p, 真值, 分层)` 条目、标注集指纹、标签来源（K 元另带题型）。
pub(super) struct 前置 {
    pub(super) delta: f64,
    pub(super) 条目: Vec<(f64, bool, Option<String>)>,
    pub(super) label_fp: String,
    pub(super) lsrc: LabelSource,
    pub(super) op: Op,
}

impl 前置 {
    pub(super) fn 按条(&self) -> Vec<(f64, bool)> {
        self.条目.iter().map(|x| (x.0, x.1)).collect()
    }
    /// 造一张证书：`(已决, 错, 上界)` 与界定哪一侧由调用方给
    #[allow(clippy::too_many_arguments)]
    pub(super) fn 证书(
        &self,
        alpha: f64,
        conf_delta: f64,
        hi: f64,
        (n_accepted, n_errors, ucb): (usize, usize, f64),
        规则: &str,
        bounded_side: String,
        sel: &Selection,
        grade: CertGrade,
    ) -> Cert {
        Cert {
            alpha,
            conf_delta,
            hi,
            n_accepted,
            n_errors,
            ucb,
            cluster_unit: "条".into(),
            resample: Some((0, 规则.to_string())),
            cost: None,
            bounded_side,
            label_source: self.lsrc.clone(),
            label_fp: self.label_fp.clone(),
            selection: Some(sel.clone()),
            grade,
            eff: None,
        }
    }
}

pub(super) fn 上侧声明() -> String {
    "上侧：界定 p ≥ hi + δ 一侧的假放行".into()
}
pub(super) fn 下侧声明() -> String {
    "下侧：界定 p ≤ lo − δ 一侧的漏放行；hi 字段此处存的是 lo".into()
}
pub(super) fn 多元声明() -> String {
    "上侧：界定 p_max ≥ hi + δ 时 argmax 错误的比例".into()
}

/// 在给定样本上选两侧线对（拆分认证在选线半上用的选线规则）。
/// 返回 `((h, l), 满足条件的候选对数)`；`h`、`l` 是判区边界（未扣 δ）。
fn 选两侧线对(
    按条: &[(f64, bool)],
    delta: f64,
    alpha: f64,
    conf_delta: f64,
) -> (Option<(f64, f64)>, usize) {
    let mut ps: Vec<f64> = 按条.iter().map(|x| x.0).collect();
    ps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ps.dedup();
    let mut cands: Vec<f64> = ps.clone();
    for w in ps.windows(2) {
        cands.push((w[0] + w[1]) / 2.0);
    }
    cands.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cands.dedup();
    let up: Vec<(f64, usize)> = cands
        .iter()
        .filter_map(|&h| {
            let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 >= h).collect();
            if acc.is_empty() || h < delta {
                return None;
            }
            let k = acc.iter().filter(|x| !x.1).count();
            (jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha)
                .then_some((h, acc.len()))
        })
        .collect();
    let down: Vec<(f64, usize)> = cands
        .iter()
        .filter_map(|&l| {
            let acc: Vec<&(f64, bool)> = 按条.iter().filter(|x| x.0 <= l).collect();
            if acc.is_empty() || l > 1.0 - delta {
                return None;
            }
            let k = acc.iter().filter(|x| x.1).count();
            (jpp_value::stat::binomial_upper(k, acc.len(), conf_delta) <= alpha)
                .then_some((l, acc.len()))
        })
        .collect();
    let mut best: Option<(f64, f64, usize)> = None;
    let mut n = 0;
    for a in &up {
        for d in &down {
            if d.0 + delta > a.0 - delta {
                continue;
            }
            n += 1;
            if best.map(|b| a.1 + d.1 > b.2).unwrap_or(true) {
                best = Some((a.0, d.0, a.1 + d.1));
            }
        }
    }
    (best.map(|b| (b.0, b.1)), n)
}

/// 分半的方法：旧的种子分半（B24），或分层交替分半（B85）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum 分法 {
    种子,
    交替,
}

impl 分法 {
    fn 方法名(self, 分层: bool) -> &'static str {
        match (self, 分层) {
            (分法::种子, false) => "split",
            (分法::种子, true) => "split-strata",
            (分法::交替, false) => "split-stratified",
            (分法::交替, true) => "split-strata-stratified",
        }
    }
    /// 旧的种子分半不写规则版本（证书与地址逐字节同旧）
    fn 规则版本(self) -> Option<&'static str> {
        match self {
            分法::种子 => None,
            分法::交替 => Some("split-stratified/v1"),
        }
    }
}

/// 分半的一半：`(p, 真值)` 按规范序。
type 半 = Vec<(f64, bool)>;

/// 拆分认证的分半（B24；B75 分层；B85 交替）。每层（来源，按 BTreeMap 序，无分层的一层在前）内排成
/// 规范序（按 `(p, 真值)`）：
/// - `种子`：按 `splitmix64(seed ^ 层内下标)` 的最低位分两半；
/// - `交替`：负例段、正例段各自按序交替，第 i 条进选线半当且仅当 (i + 起点) 为偶，
///   起点 = `splitmix64(seed ^ (2·层号 + 段号)) & 1`，段号负例 0、正例 1。
///
/// 各层依次合并。没有任何样本带分层时只有一层，种子分半与分层之前逐位相同。第三个返回值：是否分了层。
fn 分半(条目: &[(f64, bool, Option<String>)], seed: u64, how: 分法) -> (半, 半, bool) {
    type 按层 = std::collections::BTreeMap<Option<String>, Vec<(f64, bool)>>;
    let mut 层: 按层 = Default::default();
    for (p, t, s) in 条目 {
        层.entry(s.clone()).or_default().push((*p, *t));
    }
    let 分层 = 层.keys().any(|k| k.is_some());
    let (mut 选线半, mut 认证半) = (vec![], vec![]);
    for (层号, (_, mut xs)) in 层.into_iter().enumerate() {
        xs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        match how {
            分法::种子 => {
                for (i, x) in xs.iter().enumerate() {
                    if splitmix64(seed ^ i as u64) & 1 == 0 {
                        选线半.push(*x)
                    } else {
                        认证半.push(*x)
                    }
                }
            }
            分法::交替 => {
                for 段号 in [false, true] {
                    let 起点 = splitmix64(seed ^ (2 * 层号 as u64 + u64::from(段号))) & 1;
                    for (i, x) in xs.iter().filter(|x| x.1 == 段号).enumerate() {
                        if (i as u64 + 起点).is_multiple_of(2) {
                            选线半.push(*x)
                        } else {
                            认证半.push(*x)
                        }
                    }
                }
            }
        }
    }
    (选线半, 认证半, 分层)
}

/// 确定性分半用的混合函数（splitmix64）。
pub(super) fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod 分半测试 {
    use super::*;

    /// 没有分层时与步 20f 之前的分半逐位相同（旧算法原样抄在这里对照）；带分层时按来源各自分半。
    // 依据：B24（拆分样本）；B75（分层分半）
    #[test]
    fn 无分层时分半不变() {
        let xs: Vec<(f64, bool, Option<String>)> = (0..57)
            .map(|i| ((i * 37 % 100) as f64 / 100.0, i % 3 == 0, None))
            .collect();
        let (a, b, 分层) = 分半(&xs, 20260923, 分法::种子);
        let mut 规范序: Vec<(f64, bool)> = xs.iter().map(|s| (s.0, s.1)).collect();
        规范序.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (mut oa, mut ob) = (vec![], vec![]);
        for (i, x) in 规范序.iter().enumerate() {
            if splitmix64(20260923 ^ i as u64) & 1 == 0 {
                oa.push(*x)
            } else {
                ob.push(*x)
            }
        }
        assert_eq!((a, b, 分层), (oa, ob, false));
        let ys: Vec<(f64, bool, Option<String>)> = (0..40)
            .map(|i| {
                (
                    i as f64 / 40.0,
                    i % 2 == 1,
                    Some(if i < 20 { "甲" } else { "乙" }.to_string()),
                )
            })
            .collect();
        let (a, b, 分层) = 分半(&ys, 7, 分法::种子);
        assert!(分层);
        assert_eq!(a.len() + b.len(), 40);
    }

    /// B85：无分层时与研究者 `compare.py::split_halves(stratified=True)` 同一算法（起点 = splitmix64(seed ^ 段号) & 1）。
    // 依据：B85
    #[test]
    fn 交替分半与研究者实现同式() {
        let xs: Vec<(f64, bool, Option<String>)> = (0..30)
            .map(|i| ((i * 7 % 30) as f64 / 30.0, i % 3 != 0, None))
            .collect();
        let (a, b, _) = 分半(&xs, 20260923, 分法::交替);
        let mut canon: Vec<(f64, bool)> = xs.iter().map(|s| (s.0, s.1)).collect();
        canon.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (mut sel, mut cert) = (vec![], vec![]);
        for truth in [false, true] {
            let start = splitmix64(20260923 ^ u64::from(truth)) & 1;
            for (i, x) in canon.iter().filter(|x| x.1 == truth).enumerate() {
                if (i as u64 + start).is_multiple_of(2) {
                    sel.push(*x)
                } else {
                    cert.push(*x)
                }
            }
        }
        assert_eq!((a, b), (sel, cert));
    }
}
