//! 桥：`cut` 判序、出口构造、漂移告警、读数取用（20 §2.3 `bridge.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_value::bridge::DeclaredLine;
use jpp_value::stat::Stat;

/// `cut` 的策略参数（B129 三式；步 20j-1）：`cost` 代价、`alpha` 可接受假放行率、`declare` 作者声明线；
/// `stat` 是线切在读数的哪个统计量上（B153、B154，步 20j-3）。`cost`/`alpha` 的线只来自记录（p_max 上的证书）；
/// `declare` 按作者写的数切（B128）。
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CutOpts {
    pub cost: Option<(f64, f64)>,
    pub alpha: Option<f64>,
    /// 作者声明线（`cuts`、`closed` 为步 20j-3）
    pub declare: Option<DeclaredLine>,
    /// 统计量；缺省 `max`
    pub stat: Stat,
}

/// `键读数` 的组键（B128 `near_line` 的分母）：`max` 组就是校准键（与 20j-1 同），其他统计量另起一组
fn 读数组键(key: &str, stat: &Stat) -> String {
    if stat.is_max() {
        key.to_string()
    } else {
        format!("{key}\u{1f}{}", stat.to_json())
    }
}

/// 出口的臂族（`Exit.op`，`handle` 按它取臂）：出口种类由 `cut` 站点的选项定（B153 (1)）。`max` 按读数的题型；
/// 其他统计量配 `{hi, lo}` 为 test 型，配 `cuts` 为 at 型。
fn 出口族(op: Op, stat: &Stat, line: Option<&DeclaredLine>) -> Op {
    if stat.is_max() {
        op
    } else if line.is_some_and(|l| l.is_cuts()) {
        Op::Measure
    } else {
        Op::Test
    }
}

/// 按键降序（同键按下标升序），与当前档最后一项之差 ≤ `tol` 的并进这一档（`order` 的两种分档共用）
fn 分档(mut ok: Vec<(usize, f64)>, tol: f64) -> Vec<Vec<usize>> {
    ok.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    let mut tiers: Vec<(Vec<usize>, f64)> = vec![];
    for (i, v) in ok {
        match tiers.last_mut() {
            Some((last, prev)) if (*prev - v).abs() <= tol => {
                last.push(i);
                *prev = v;
            }
            _ => tiers.push((vec![i], v)),
        }
    }
    tiers.into_iter().map(|(t, _)| t).collect()
}

impl<'a> Interp<'a> {
    /// 跨对象偏序分档（`12`:134「相邻档并列」）：按可比值降序，**相邻差 ≤ δ 的并列成一档**。
    /// 失败/停止的读数（J-12）单独一档排最后。
    ///
    /// 为什么是偏序不是全序：δ 是同一读数重测的抖动，δ 之内的差**不是真差别**。
    /// 全序会让「0.71 排在 0.70 前面」看起来像个结论。
    ///
    /// 读答案，所以收 `ReadAnswer` 令牌（B57，步 25-2；唯一调用者是构造 `order`）。
    ///
    /// 步 15k（B167）：排序键是 `stat_of(答案, stat)`，与 `cut` 同一个函数。并档容差：概率型统计量
    /// （`max`、`mass`、`confidence`）取线的 δ（现状）；`argmax` 按档位相等；`expect` 按作者给的 `tie`
    /// （缺省 0）。取不到键（统计量与题型不配、`confidence` 没报）时整个 `order` 报错，不把这一条当失败读数。
    /// 依据：B167 (1)–(3)（地基/附注/2026-09-26-批6裁定.md §十五）
    pub(crate) fn order_tiers(
        &self,
        _ra: &crate::caps::Cap<crate::caps::ReadAnswer>,
        rs: &[Rc<Reading>],
        stat: &Stat,
        tie: Option<f64>,
    ) -> Result<Vec<Vec<usize>>, jpp_value::stat::StatError> {
        let mut ok: Vec<(usize, f64)> = vec![];
        let mut failed: Vec<usize> = vec![];
        for (i, r) in rs.iter().enumerate() {
            let a = if r.fail.is_some() {
                None
            } else {
                self.answer_of(r)
            };
            match a {
                Some(a) => {
                    let c = self.置信(r, &a);
                    ok.push((i, jpp_value::stat::stat_of(&a, stat, c)?));
                }
                None => failed.push(i),
            }
        }
        let tol = match stat {
            Stat::Argmax => f64::default(),
            Stat::Expect => tie.unwrap_or_default(),
            _ => self.order_delta(rs),
        };
        let mut tiers = 分档(ok, tol);
        if !failed.is_empty() {
            tiers.push(failed);
        }
        Ok(tiers)
    }

    /// 一条 `select` 读数的候选分档（B166，步 15k）：按候选概率降序，相邻差 ≤ δ 并档（δ 取法同跨对象分档）；
    /// 读数失败或没有答案时全部候选并成一档（没有信息即全部并列）。不产生出口。
    /// 依据：B166 (1)（地基/附注/2026-09-26-批6裁定.md §十四）
    pub(crate) fn candidate_tiers(
        &self,
        _ra: &crate::caps::Cap<crate::caps::ReadAnswer>,
        r: &Rc<Reading>,
    ) -> Vec<Vec<usize>> {
        let a = if r.fail.is_some() {
            None
        } else {
            self.answer_of(r)
        };
        match a {
            Some(Answer::Choice(v)) => 分档(
                v.into_iter().enumerate().collect(),
                self.order_delta(std::slice::from_ref(r)),
            ),
            _ => vec![(0..r.over_len).collect()],
        }
    }

    /// 并列的容差取这条线的 δ（步 15d-2：只从记录取）；记录没有 δ 时不并列（只有完全相等才并列）
    fn order_delta(&self, rs: &[Rc<Reading>]) -> f64 {
        rs.first()
            .and_then(|r| self.calib.line_delta(&self.calib.line(&r.calib)))
            .unwrap_or_default()
    }

    /// `untested` 是 **J-15 的那一位**，与 `kind` 正交：`kind` 决定路由，它只回答
    /// 「这条路上的判据测没测过」。绝大多数出口传 `None`。
    pub(crate) fn new_exit(
        &mut self,
        kind: ExitKind,
        untested: Option<String>,
        op: Op,
        q_hash: &str,
        state_hash: &str,
        taint: Taint,
        sp: Span,
    ) -> Value {
        self.new_exit_from(
            kind,
            untested,
            op,
            q_hash,
            state_hash,
            taint,
            String::new(),
            sp,
        )
    }

    /// 同上，外加「线是哪一级的」（`Exit::line_source`）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_exit_from(
        &mut self,
        kind: ExitKind,
        untested: Option<String>,
        op: Op,
        q_hash: &str,
        state_hash: &str,
        taint: Taint,
        line_source: String,
        sp: Span,
    ) -> Value {
        // 惰性出口（B94）：出口号在 `cut` 时已分配，出口挂回登记它的那一帧
        let (id, 帧) = match self.出口预定.take() {
            Some((id, 帧)) => (id, Some(帧)),
            None => {
                let id = self.next_exit;
                self.next_exit += 1;
                (id, None)
            }
        };
        // 出口只由桥产生：唯一构造在 `jpp_value::bridge::issue`（20 §2.4）
        let e = Rc::new(jpp_value::bridge::issue(jpp_value::bridge::ExitParts {
            id,
            kind,
            untested,
            op,
            q_hash: q_hash.into(),
            state_hash: state_hash.into(),
            taint,
            line_source,
            site: sp,
        }));
        match 帧.and_then(|i| self.frames.get_mut(i)) {
            Some(f) => f.exits.push(e.clone()),
            None => self.frame().exits.push(e.clone()),
        }
        Value::Exit(e)
    }

    /// `allocate` / `unsure_bound` 的入参：只收读数（J-01）。单个读数也当一条收。
    pub(crate) fn readings_of(&self, v: &Value, who: &str, sp: Span) -> R<Vec<Rc<Reading>>> {
        let bad = |t: &str| {
            err(
                Some("J-01"),
                format!(
                    "{who} 只接受读数，收到 {t}：读数没有可读的值，只有它才有「离线多远」这个量"
                ),
                sp,
            )
        };
        match v {
            Value::Reading(r) => Ok(vec![r.clone()]),
            Value::List(l) => {
                let mut out = vec![];
                for x in l.iter() {
                    match x {
                        Value::Reading(r) => out.push(r.clone()),
                        other => return bad(other.type_name()),
                    }
                }
                Ok(out)
            }
            other => bad(other.type_name()),
        }
    }

    /// **漂移告警**（`12`:649「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落告警）」）。
    ///
    /// **挂在「消费这条校准记录」这个动作上，不挂在 `cut` 上。**
    /// 原来只在 `cut` 里发，而实测全集（读 `self.calib` 的位置）有三处在 `cut` 之外：
    /// `allocate`、`unsure_bound`、`delta_for`。**前两处真的在用这条线**——
    /// `uncertainty` 读 `lines_for`（`strength.rs:76`），`unsure_bound` 读
    /// **只认「上岗」记录**的 `unsure_rate`（`strength.rs:155`），而它交出去的是
    /// **J-10 的联合上界，一条语言自己承诺的保证**。读数分布移开之后那个数不再成立，
    /// 程序拿到一个**静默失效的上界**，零告警。**失败开放**，且正落在「长处」那一侧。
    ///
    /// **`delta_for` 没接进来**：它取的是档案的迟滞带宽 δ，不是线；
    /// 漂移监控管的是**这条线还成不成立**。**这是一条判断不是实测**，记在此处。
    ///
    /// **只告警，不动状态**：停岗是人下的判断（走 `put`）。一个只报不动的机制
    /// **造不出永久锁**——而复岗今天不存在，所以这一点是承重的。
    ///
    /// 每个键每次运行只报一次：**一条天天响的告警等于没有告警**。
    /// 键按「每次运行」去重，所以多个消费方共用同一个键时仍然只响一次。
    pub(crate) fn 报漂移(&mut self, key: &str, sp: Span) {
        if self.drift_reported.contains(key) {
            return;
        }
        if let Some(d) = self.calib.drift(key) {
            if d.可停岗() {
                self.drift_reported.insert(key.to_string());
                self.trace.warn(format!(
                    "W-drift: @{} 键 {key} 的近期读数分布与定线时的标注分布已经移开（KS={:.3} PSI={:.3}，参照 {} 条 / 近期 {} 条）。                         已自动标为停岗候选（B25）：本趟起这条线的出口照常路由，但不得放行不可逆 do；正式停岗由人确认。修法【需接线人】：--calib-out 写出候选后，用 jpp calib-confirm <目录> <键> --suspend 或 --keep",
                    sp.start, d.ks, d.psi, d.n_ref, d.n_recent
                ));
            }
        }
    }

    /// 这批读数各自的校准键上都查一遍漂移（`allocate` / `unsure_bound` 用）。
    pub(crate) fn 报漂移_批(&mut self, rs: &[Rc<Reading>], sp: Span) {
        let keys: Vec<String> = rs.iter().map(|r| r.calib.clone()).collect();
        for k in keys {
            self.报漂移(&k, sp);
        }
    }

    /// 把 `cut` 查到的一条校准记录记进账本（`Ledger::calib_used`）。库里没有的键不记：
    /// 重放时它照样查不到，照样是冷，出口一致。
    pub(crate) fn note_calib(&mut self, key: &str) {
        // 本趟每键只记一次；是否追加 `CalibUsed` 条目由账本按「该键最后一条的哈希」定（B124，步 18a）
        if !self.本趟已记校准.insert(key.to_string()) {
            return;
        }
        if let Some(j) = self.calib.record_json(key) {
            let h = jpp_value::value::hash_of(&[&j.to_string()]);
            if let Err(e) = self.ledger.append_calib_used(key, &h, j) {
                self.账本错.get_or_insert(e);
            }
        }
    }

    /// 题式级记录：查到就记进账本（`calib_used`），再取记录。代价分支与非代价分支共用，避免再漏记
    /// （补丁请求 3：只凭账本重放要补回同一条线）。
    pub(crate) fn 查题式(&mut self, r: &Reading) -> Option<Lookup> {
        let f = self.calib.chain(&r.calib, r.form_hash.as_deref()).form?;
        self.note_calib(&f.key);
        Some(f.rec)
    }

    /// 类级记录（B34）：查到就记进账本，再取记录。`key` 是 `cut` 用的有效校准键（类别标签）。
    pub(crate) fn 查类(&mut self, key: &str) -> Option<Lookup> {
        let c = self.calib.chain(key, None).class?;
        self.note_calib(&c.key);
        Some(c.rec)
    }

    /// 程序里的 `cut`（B94，步 23c）：只把读数与线（校准键、代价）绑定，返回未解析出口，不刷新。
    /// 出口号此刻分配（与改前同序），出口在第一次被检视时由 [`Self::解析出口`] 解析、挂回本帧。
    /// `lazy_cut` 关掉即改前行为：当场刷新并解析。依据：B94
    pub(crate) fn 过桥(
        &mut self,
        r: &Rc<Reading>,
        calib_key: Option<&str>,
        opts: CutOpts,
        sp: Span,
    ) -> R<Value> {
        if !self.plan.lazy_cut {
            return self.cut(r, calib_key, opts, sp);
        }
        let id = self.next_exit;
        self.next_exit += 1;
        let c = Rc::new(PendingCut {
            reading: r.clone(),
            calib: calib_key.map(String::from),
            cost: opts.cost,
            alpha: opts.alpha,
            declare: opts.declare,
            stat: opts.stat,
            site: sp,
            id,
            frame: self.frames.len() - 1,
            resolved: std::cell::RefCell::new(None),
        });
        self.frame().cuts.push(c.clone());
        Ok(Value::Cut(c))
    }

    /// 解析一个惰性出口（B94）：先在检视点刷新，再照改前的 `cut` 查线、过线、记账；
    /// 解析一次、缓存出口，复制出去的各份得到同一个出口。
    pub(crate) fn 解析出口(&mut self, c: &Rc<PendingCut>) -> R<Rc<Exit>> {
        if let Some(e) = c.exit() {
            return Ok(e);
        }
        // 检视点（B94）：「被检视时刷新」取代「cut 时刷新」
        self.flush("inspect")?;
        self.出口预定 = Some((c.id, c.frame));
        let opts = CutOpts {
            cost: c.cost,
            alpha: c.alpha,
            declare: c.declare.clone(),
            stat: c.stat.clone(),
        };
        let v = self.cut(&c.reading, c.calib.as_deref(), opts, c.site);
        self.出口预定 = None;
        let Value::Exit(e) = v? else {
            return err(Some("E-rt-arg"), "cut 没有给出出口", c.site);
        };
        *c.resolved.borrow_mut() = Some(e.clone());
        Ok(e)
    }

    /// 检视一个值（B94）：值里（列表、记录、`stop` 里）的未解析出口全部解析，换成出口；
    /// 没有未解析出口的值原样返回。内置与构造的实参、`if` 条件、运算、取字段在用值前都经这里。
    pub(crate) fn 检视(&mut self, v: Value) -> R<Value> {
        if !含惰性出口(&v) {
            return Ok(v);
        }
        // 惰性生成值（B149、B160，步 15h-2）：值里的生成在下面逐个取回（未交出的先刷新交出，再收层）；
        // 推测与提升期间不取回（内部报文，调用方放弃）
        if self.生成等待中() {
            let mut gens = vec![];
            未取生成(&v, &mut gens);
            if let Some(g) = gens.first() {
                return Err(crate::gen_pending::不等生成(g.site));
            }
        }
        Ok(match v {
            Value::Gen(g) => self.解析生成(&g)?,
            Value::Cut(c) => Value::Exit(self.解析出口(&c)?),
            Value::List(l) => {
                let mut out = Vec::with_capacity(l.len());
                for x in l.iter() {
                    out.push(self.检视(x.clone())?);
                }
                Value::list(out)
            }
            Value::Record(r) => {
                let mut out = Vec::with_capacity(r.len());
                for (k, x) in r.iter() {
                    out.push((k.clone(), self.检视(x.clone())?));
                }
                Value::record(out)
            }
            Value::Stop(x) => Value::Stop(Rc::new(self.检视((*x).clone())?)),
            other => other,
        })
    }

    /// 解析各帧里读数账本键为 `key` 的未解析出口（审查修复 1）：谱系放行按账本键查放行表，同一读数
    /// 被几条线切出的出口都要先有等级，否则没被检视的那条（可能不放行）查不到。依据：B72-4、B94
    pub(crate) fn 解析同键出口(&mut self, key: &str) -> R<()> {
        let 待: Vec<Rc<PendingCut>> = self
            .frames
            .iter()
            .flat_map(|f| f.cuts.iter())
            .filter(|c| c.exit().is_none() && c.reading.ledger_key == key)
            .cloned()
            .collect();
        for c in &待 {
            self.解析出口(c)?;
        }
        Ok(())
    }

    /// 解析各帧里全部未解析出口（审查修复 3a）：`do`/`gen`/`ask` 与判断共用预算，改前这些出口在 `cut`
    /// 处就已刷新计费；效应求值前先解析，调用的先后与出口种类与改前相同。没有未解析出口时什么都不做。
    pub(crate) fn 解析全部帧(&mut self) -> R<()> {
        let 待: Vec<Rc<PendingCut>> = self
            .frames
            .iter()
            .flat_map(|f| f.cuts.iter())
            .filter(|c| c.exit().is_none())
            .cloned()
            .collect();
        for c in &待 {
            self.解析出口(c)?;
        }
        Ok(())
    }

    /// 帧返回前（函数返回、程序结束）解析本帧登记的惰性出口（B94）：J-05 按出口种类核责任，
    /// 所以帧里的出口在帧弹出前都要有种类。
    pub(crate) fn 解析本帧(&mut self) -> R<()> {
        let cuts = std::mem::take(&mut self.frame().cuts);
        for c in &cuts {
            self.解析出口(c)?;
        }
        Ok(())
    }

    pub(crate) fn cut(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        opts: CutOpts,
        sp: Span,
    ) -> R<Value> {
        let 已记 = self.exit_grades.len();
        let stat = opts.stat.clone();
        let v = match opts.declare.clone() {
            Some(line) => self.cut_declared(r, calib_key, &line, &stat, sp)?,
            None if stat.is_max() => self.cut_inner(r, calib_key, opts, sp)?,
            // B153 (1)：别的统计量不借 p_max 上的认证线
            None => self.cut_stat_cold(r, calib_key, &stat, sp)?,
        };
        // 本趟该键切过的读数（B128 `near_line` 的分母；`20` v2 §4.4 第 9 条「本趟该键读数」），按统计量分组
        if let Some(a) = self.answer_of(r) {
            let c = self.置信(r, &a);
            if let Ok(x) = jpp_value::stat::stat_of(&a, &stat, c) {
                self.键读数
                    .entry(读数组键(calib_key.unwrap_or(&r.calib), &stat))
                    .or_default()
                    .push(x);
            }
        }
        if let Value::Exit(e) = &v {
            *e.ledger_key.borrow_mut() = r.ledger_key.clone();
            // 看 p 之前就返回的出口（J-09 证据不足）没用上线：等级 Cold（步 20f 逐出口记线等级）
            if self.exit_grades.len() == 已记 {
                e.grade.set(Some(LineGrade::Cold));
                self.exit_grades.push(serde_json::json!({
                    "site": sp.start, "exit": e.label(), "grade": LineGrade::Cold.name(), "releases": e.releases(),
                    "item": 材料摘要(&r.state_hash),
                }));
            }
            // B133（步 25-2b）：记下这个出口的报告行，元素构造 `element` 按出口 id 找行（`cut` 每次恰写一行）
            if self.exit_grades.len() > 已记 {
                self.exit_rows.insert(e.id, 已记);
                // 步 20j-3：行上写统计量（不是 max 才写，B153）与判断器报的自报置信度（B154 (1)「报告 exits 行可见」；
                // 夹具缺省不写）
                let 置信 = self.置信表.borrow().get(&r.id).copied();
                let row = &mut self.exit_grades[已记];
                if !stat.is_max() {
                    row["stat"] = stat.to_json();
                }
                if let Some(c) = 置信 {
                    row["confidence"] = json!(c);
                }
            }
            // 谱系放行（B72-4，步 17b）：本趟出口表登记这个出口（函数在 guard.rs，运行时轨）
            self.登记出口放行(e);
        }
        Ok(v)
    }

    pub(crate) fn cut_inner(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        opts: CutOpts,
        sp: Span,
    ) -> R<Value> {
        let (cost, alpha) = (opts.cost, opts.alpha);
        // 刷新点（12 §2.2:129）：cut 要读答案，所以先把这一层发出去
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib);
        // 查找链由校准侧一次给出（B44：题键 → 题式键 → 冷；桥不构造键，S7）
        let 链 = self.calib.chain(key, r.form_hash.as_deref());
        let rec = 链.question.rec.clone();
        // 账本记下这次查到的记录（全文 + 哈希），只凭账本重放时据此补回当时的线
        self.note_calib(key);
        // J-16：fit 的训练集 ≠ 保形集。同源就是「拿训练数据给自己打分」，
        // 过线的那条线因此不再是独立的证据。
        if let Some(fit_name) = r.calib.strip_prefix("fit:") {
            if let Some(f) = self.fits.get(fit_name) {
                if !rec.set_id.is_empty() && rec.set_id == f.trained_from {
                    return err(
                        Some("J-16"),
                        format!(
                            "fit {fit_name} 的训练集与保形集同源（{}）：不相交约束违反，过线的那条线不再是独立证据",
                            rec.set_id
                        ),
                        sp,
                    );
                }
            }
        }
        // 12:148 判序第一步：**先 insufficient**（该题声明的决定性证据槽不在状态里 → 不信任 p，J-09）。
        // 它拦的是「模型对一道没有证据可依的题照样给出一个自信的 p」——那个 p 会照常过线变成 Act。
        // 这是唯一一处**在看 p 之前**就把它挡住的检查，所以排在 taint 与过线之前。
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(
                ExitKind::Unsure(format!("insufficient:{missing}")),
                None,
                r.op,
                &r.q_hash,
                &r.state_hash,
                r.state_taint,
                sp,
            ));
        }
        // 12:150「出口 taint 继承状态 taint」、§2.11「cut 继承」。
        // 状态的 taint 由 `State::new` 折算好（on/ctx/ref/over 取并），读数带着它过来。
        let taint = r.state_taint;
        // **两件正交的事一起算出来**：`kind` 是路由键，`untested` 是 J-15 的那一位。
        // 它们分开的理由见 `Exit::untested` 的注释——合进 `cause` 就退化成
        // `cold`/`no_perm`/`no_ece`/`no_klimit` 那条已经走过一次并且停了的路。
        //
        // `untested` 这里带的是**一对**：载体名 + 这一格的修法提示。**修法跟着载体走，
        // 不靠「记得去告警那边加一个分支」**——否则就是把「漏加路由的失效方式是静默的」
        // 在这个专为消除它而建的机制内部再造一遍（漏加只会少一句修法，不会报错）。
        // **线从哪一级来**（`12`:136「题级样本不够时用模式级校准做先验收缩」的查找那一半）。
        //
        // **只有「上岗」才有线**——这是 `lines_for` 一直以来的口径，在这里显式化。
        // 冷、待真值都算「这道题没有自己的线」：**待真值尤其要说清**，它是「证据积累中、
        // 真值还没到」，`get` 给它合成的 `0.65/0.35` **是缺省值不是线**，
        // 拿它当线用就是替不确定做了乐观的默认。
        //
        // 查不到题级，就查这一类（`phys` + `literal_mode`）。**查得到也必须留痕**：
        // 模式级的线不能冒充题级的线。
        let (线, 线源, 夹具): (Option<(f64, f64)>, String, bool) = if rec.status == "上岗"
            || rec.status == "停岗候选"
        {
            // **两件正交的事，不许挤进一个字段。**
            // 「题级 / 模式级」答的是**哪一层**；「手填 / 证书」答的是**凭什么**。
            // 第一版我拿后者盖掉了前者，层级信息就没了——`题级有线时用自己的` 当场红。
            (
                Some((rec.hi, rec.lo)),
                format!("题级·{}", 凭据(&rec)),
                rec.fixture_line(),
            )
        } else if rec.status == "停岗" {
            (None, String::new(), false)
        } else if r.calib.starts_with("fit:") {
            // **`fit` 的结果不借模式级先验。** `cut(fit结果)` 不带第二参时 `key` 就是
            // `fit:{名}`，而那条记录几乎从不上岗（fit 的校准住在 `error_rate` 里，不是一条线）。
            // 掉到 `mode_key("noul", …)` 上就是**跨种借线**——fit 的可靠性与「裸 noul 判断
            // 这一类的可靠性」毫无关系。**这正是模式键按 `phys` 分格要避免的那件事
            // 从另一道门进来**：`fit` 的 `op.phys()` 是它输入的物理形式，不是它自己的。
            (None, String::new(), false)
        } else if r.fail.is_some() {
            // Fail 读数根本走不到过线比较；这里先算线只会白告警一句「借用了模式级先验」，
            // 而它其实什么也没借。**与「没用上线的出口不留来源」同一条**——
            // 审计物上留一句没发生的事，和留一个没用上的来源是同一种假话。
            (None, String::new(), false)
        } else if let Some(f) = 链.form.clone().filter(|f| {
            self.note_calib(&f.key);
            matches!(f.rec.status.as_str(), "上岗" | "停岗候选")
        }) {
            // **题式级**：题键没有上岗记录，而这道题由一个有上岗记录的题式填出。
            // **留痕**：题式线不冒充题级线。
            let (fk, f) = (f.key, f.rec);
            self.trace.warn(format!(
                "W-form-line: @{} 题级校准键 {key} 无上岗记录，用题式级线 {}（n={}）；出口带 line_source=题式级",
                sp.start, fk.trim_start_matches('\u{1f}').replace('\u{1f}', ":"), f.n
            ));
            if let Some(t) = f.truth_gate.as_ref().filter(|g| g.starts_with("临时上岗")) {
                let ae = f
                    .selected
                    .as_ref()
                    .map(|c| format!("（alpha_eff={:.3}，B89）", c.alpha_eff))
                    .unwrap_or_default();
                self.trace.warn(format!(
                    "W-provisional: @{} 这条题式级线是{}{ae}",
                    sp.start, t
                ));
            }
            (
                Some((f.hi, f.lo)),
                format!("题式级·{}", 凭据(&f)),
                f.fixture_line(),
            )
        } else if let Some(c) = 链.class.clone().filter(|c| {
            // 依据：B34（类键只命中以该类别为对象、在混合样本上认证过的记录）、B44（借线只经类键）。
            // 题式记录停岗时不借类线：不绕过题式自己的停岗（取拒绝侧，过程记录 步 20b）。
            let 题式停岗 = 链.form.as_ref().is_some_and(|f| f.rec.status == "停岗");
            self.note_calib(&c.key);
            !题式停岗 && matches!(c.rec.status.as_str(), "上岗" | "停岗候选")
        }) {
            // **类级**：题键、题式键都没有线，作者声明的类别有一条类认证记录。
            // **留痕**：类线不冒充题级或题式级线。
            let (ck, c) = (c.key, c.rec);
            self.trace.warn(format!(
                "W-class-line: @{} 题级校准键 {key} 无上岗记录{}，借类级线 {}（n={}）；出口带 line_source=类级",
                sp.start,
                if 链.form.is_some() { "、题式记录也无线" } else { "" },
                ck.trim_start_matches('\u{1f}').replace('\u{1f}', ":"),
                c.n
            ));
            if let Some(t) = c.truth_gate.as_ref().filter(|g| g.starts_with("临时上岗")) {
                let ae = c
                    .selected
                    .as_ref()
                    .map(|x| format!("（alpha_eff={:.3}，B89）", x.alpha_eff))
                    .unwrap_or_default();
                self.trace.warn(format!(
                    "W-provisional: @{} 这条类级线是{}{ae}",
                    sp.start, t
                ));
            }
            (
                Some((c.hi, c.lo)),
                format!("类级·{}", 凭据(&c)),
                c.fixture_line(),
            )
        } else {
            // 真值通道导入过、但没过上岗门的题式：**说出来**，不静默当冷键
            if let Some(t) = 链.form.as_ref().and_then(|f| f.rec.truth_gate.as_ref()) {
                self.trace.warn(format!(
                    "W-form-pending: @{} 题式级记录未上岗（{}）；本题按冷键处理",
                    sp.start, t
                ));
            }
            // **B44：模式键不在查找链上。** 模式级记录只作 `commission` 的先验输入，不是认证线；
            // 题键、题式键、类键都没有上岗记录就是冷（原 `W-mode-prior` 回退已拆除）。
            (None, String::new(), false)
        };
        // **代价线**（B29 前端补齐 `cut(…, {cost: [fp, fn]})`）：线只取按这个代价矩阵认证过的证书，
        // 先题级、后题式级；找不到就是冷——**不从别的证书或夹具线借**。
        let 代价缺线 = cost.is_some();
        // 所选代价证书所在记录是「停岗」（补丁请求 2）：与题级停岗同一路由（drift）
        let mut 代价停岗 = false;
        // 代价线所选的那张证书（等级按它算，B72）
        let mut 代价证书: Option<jpp_effects::views::CertView> = None;
        let (线, 线源, 夹具) = match (cost, alpha) {
            (None, None) => (线, 线源, 夹具),
            _ => {
                // 选证书：给了代价只看同代价证书；只给 α 时只看非代价证书。
                // 只给代价：α 最小的一张，线 (证书.hi, min(记录 lo, 证书.hi))（步 20j-1 前的行为不变）。
                // 给了 α（B129，步 20j-1）：α ≤ a 里在自己认证样本上已决条数最多的一张，并列取 α 小者；
                // hi 与 lo 取自同一次认证——与它同 α、同标注集指纹的下侧证书配成两侧线，配不上即单侧线
                // （lo = 0，Ignore 不可达）。两张证书的保证不拼接（主会话 2026-09-25）。
                let 同代价 = |rec: &Lookup| -> Option<(jpp_effects::views::CertView, (f64, f64))> {
                    let 候 = rec.certs.iter().filter(|c| match cost {
                        Some((fp, fn_)) => matches!(c.cost, Some((a, b)) if (a - fp).abs() < jpp_value::stat::BOUNDARY_EPS && (b - fn_).abs() < jpp_value::stat::BOUNDARY_EPS),
                        None => c.cost.is_none(),
                    });
                    match alpha {
                        None => 候
                            .min_by(|a, b| {
                                a.alpha
                                    .partial_cmp(&b.alpha)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|c| (c.clone(), (c.hi, rec.lo.min(c.hi)))),
                        Some(a) => {
                            let 配对 = |c: &jpp_effects::views::CertView| {
                                rec.lower.as_ref().filter(|l| {
                                    (l.alpha - c.alpha).abs() < jpp_value::stat::BOUNDARY_EPS
                                        && !c.label_fp.is_empty()
                                        && l.label_fp == c.label_fp
                                })
                            };
                            let 已决 = |c: &jpp_effects::views::CertView| {
                                c.n_accepted + 配对(c).map_or(0, |l| l.n_accepted)
                            };
                            候.filter(|c| c.alpha <= a + jpp_value::stat::BOUNDARY_EPS)
                                .max_by(|x, y| {
                                    已决(x).cmp(&已决(y)).then(
                                        y.alpha
                                            .partial_cmp(&x.alpha)
                                            .unwrap_or(std::cmp::Ordering::Equal),
                                    )
                                })
                                .map(|c| (c.clone(), (c.hi, 配对(c).map_or(0.0, |l| l.hi))))
                        }
                    }
                };
                // 题式级记录查到就入账（补丁请求 3）：只凭账本重放时据此补回同一张证书
                let 题式 = self.查题式(r);
                // 候选：题级在前、题式级在后；每张证书带上**它所在记录自己的状态**（补丁请求 2）
                let mut 候选: Vec<(jpp_effects::views::CertView, (f64, f64), &str, String)> =
                    vec![];
                if let Some((c, l)) = 同代价(&rec) {
                    候选.push((c, l, "题级", rec.status.clone()));
                }
                if let Some(f) = 题式.as_ref() {
                    if let Some((c, l)) = 同代价(f) {
                        候选.push((c, l, "题式级", f.status.clone()));
                    }
                }
                // 类级在最后（B34 查找链同序；题式停岗时不借，同非代价分支）
                let 题式停岗 = 题式.as_ref().is_some_and(|f| f.status == "停岗");
                let 类 = if 题式停岗 { None } else { self.查类(key) };
                if let Some(((c, l), k)) = 类.as_ref().and_then(|k| 同代价(k).map(|c| (c, k))) {
                    候选.push((c, l, "类级", k.status.clone()));
                }
                let 可用 = 候选
                    .iter()
                    .find(|(_, _, _, st)| st == "上岗" || st == "停岗候选");
                match 可用 {
                    // 依据：B25（停岗只看所选记录）；B29（线只取按这个代价认证的证书）
                    Some((c, l, 层, _)) if rec.status != "停岗" => {
                        代价证书 = Some(c.clone());
                        let 代价 = cost
                            .map(|(fp, fn_)| format!("·代价(fp={fp},fn={fn_})"))
                            .unwrap_or_default();
                        let 选阿尔法 = alpha.map(|a| format!("·alpha≤{a}")).unwrap_or_default();
                        (
                            Some(*l),
                            format!("{层}·{}证书:α={:.2}{代价}{选阿尔法}", 试用前缀(c), c.alpha),
                            false,
                        )
                    }
                    _ => {
                        代价停岗 = 候选.iter().any(|(_, _, _, st)| st == "停岗");
                        (None, String::new(), false)
                    }
                }
            }
        };
        // 判序在 `jpp_value::bridge::decide`（纯函数，一处）；这里只把查好的事实交过去。
        // δ：用了题式级线就取题式记录的 δ，否则取题级记录的 δ（只在比线时用得上）。
        // 用了类级线取类记录的 δ（步 20b）。
        let 所用链 = |线源: &str| -> &jpp_effects::views::Link {
            match (
                线源.starts_with("题式级"),
                线源.starts_with("类级"),
                链.form.as_ref(),
                链.class.as_ref(),
            ) {
                (true, _, Some(f), _) => f,
                (_, true, _, Some(c)) => c,
                _ => &链.question,
            }
        };
        // 步 15d-2：δ 只从记录取（`line_delta`）；没有 δ 的线出口 `Unsure(untested)`，载体 `Delta`（`20` §3.9）
        let delta = match (&代价证书, alpha) {
            // B129（步 20j-1）：α 选出的证书，δ 按它算
            (Some(c), Some(_)) => self.calib.line_delta(&Lookup {
                selected: Some(c.clone()),
                ..所用链(&线源).rec.clone()
            }),
            _ => self.calib.line_delta(&所用链(&线源).rec),
        };
        let 需要答案 = r.fail.is_none()
            && !self.absent_marks.contains_key(&r.ledger_key)
            && rec.status != "停岗"
            && !代价停岗
            && 线.is_some();
        let (kind, untested) = jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
            fail: r.fail.as_deref(),
            absent: self.absent_marks.get(&r.ledger_key).map(String::as_str),
            suspended: rec.status == "停岗" || 代价停岗,
            line: 线,
            cost_requested: 代价缺线,
            answer: if 需要答案 {
                self.answer_of(r)
            } else {
                None
            },
            delta,
            mode_share: r.mode_share.get(),
        });
        // 每条既有路由各加一句「若该量未测，取保守项并**告警**」（`12` §2.11）。
        // `cold` 本来就取保守线，缺的是那句告警——**没有告警，「用了保守线」与「线本来就这么宽」
        // 在痕迹上分不开**，跟兜底档案 `hash` 必须是 `None` 是同一条。
        //
        // **就这一处告警**，所有载体走同一条。新增载体只要在上面的分支里带上修法提示，
        // 这里不需要动；忘了带也只是少一句提示，不会漏掉告警本身。
        // B129（步 20j-1）：给了 α 而没有 α ≤ a 的证书，冷的修法按「选不到证书」写
        let untested = match (untested, alpha) {
            (Some((c, _)), Some(a)) if c == "calib_line" => Some((
                "alpha_line".to_string(),
                format!(
                    "修法【作者可改】：这道题的记录里没有 α ≤ {a} 的证书（alpha 只在已有证书里选，不放宽，B129）；放宽 alpha、按这个 α 重新认证（calib-import --alpha），或去掉 alpha 用记录选中的证书"
                ),
            )),
            (u, _) => u,
        };
        // B130（步 20j-1）：冷出口（载体 calib_line）一趟一键一条，其余载体照旧逐出口报
        let 报 = match &untested {
            Some((c, _)) if c == "calib_line" => self
                .unknown_reported
                .insert(format!("W-untested-calib_line\u{1f}{key}")),
            _ => true,
        };
        if let Some((carrier, 修法)) = untested.as_ref().filter(|_| 报) {
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind { ExitKind::Unsure(c) => c.clone(), other => format!("{other:?}") }
            ));
        }
        // **没用上线的出口不留来源**（fail / 冷 / 停岗）：留一个来源就是谎称有线。
        let 留痕 = if matches!(kind, ExitKind::Unsure(ref c) if c == "cold" || c == "drift" || c == "absent" || c == "latency" || c == "no_candidate" || c.starts_with("fail:"))
        {
            String::new()
        } else {
            线源.clone()
        };
        // **强出口建在一条未经认证的线上，要出告警。**
        //
        // 判据是 `certs.is_empty()`，**不是 `n` 的大小**：三个出货示例全部拿到强出口，
        // 用的是 `n = 1` 的手填线、零告警——**同一个字段 `n`，一条路上 1 就够，
        // 另一条路上 22 还不够，中间没有任何东西把这个差别说出来**。
        // 说出那个差别的是「有没有证书」，不是那个数。
        self.报漂移(key, sp);
        // 类级线的漂移看类键（步 20b）；题式级沿用原口径不变
        let 类键 = 链.class.as_ref().map(|c| c.key.clone());
        if let Some(ck) = 类键.filter(|_| 线源.starts_with("类级") && !留痕.is_empty()) {
            self.报漂移(&ck, sp);
        }
        // **标签来源可疑的证书给出强出口也要留痕。**
        //
        // 与 `bounded_side` **不告警**那条的分界：`bounded_side` 今天只有一个取值，
        // **一条在每个已认证键上都响的告警承载零信息**；而 `label_source` 区分得开记录，
        // 它响的时候是在说一件**别的记录不成立**的事。
        //
        // **这条告警的正确稳态是「消失」**——声明过就不响了。一条稳态为零的告警，
        // 与一条永久噪声不是一回事。
        if matches!(kind, ExitKind::Act | ExitKind::Pick(_) | ExitKind::At(_)) {
            if let Some(c) = &rec.selected {
                if c.label_source_suspicious {
                    self.trace.warn(format!(
                        "W-label-source: @{} 键 {key} 的证书没说清标签是怎么选的（{}），而这里给出了强出口。                         若标签集是一个被选出来的子集、而选择判据与「对不对」相关，**这张证书上的每个数都同向有偏**。                         修法【需接线人】：set_label_source 是 Rust API，`.jpp` 作者调不到",
                        sp.start, c.label_source
                    ));
                }
            }
        }
        // **夹具线**（B29）：线来自宿主 `put` 的测试记录或没有证书的记录。J-03 同约束宿主：
        // 这样的出口照常路由，但不算放行不可逆 `do` 的可信合取项（J-08 按 `guard_trusted` 查）。
        // 原 `W-uncertified` 并入这里：判据仍是「有没有证书」，外加 `put` 写的夹具位。
        let 夹具出口 = 夹具 && !留痕.is_empty();
        if 夹具出口 && matches!(kind, ExitKind::Act | ExitKind::Pick(_) | ExitKind::At(_)) {
            self.trace.warn(format!(
                "W-fixture-line: @{} 键 {key} 的线是夹具线（宿主 put 写入或没有认证证书，n={}），出口 {} 照常路由，但按 B29 不算放行不可逆 do 的可信合取项。修法【需接线人】：用 calib-import 或 commission 从带真值样本认证这条线",
                sp.start, rec.n,
                match &kind { ExitKind::Pick(k) => format!("pick({k})"), other => format!("{other:?}") }
            ));
        }
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            r.op,
            &r.q_hash,
            &r.state_hash,
            taint,
            留痕,
            sp,
        );
        if let Value::Exit(e) = &出口 {
            // 依据：B75（类线出口可路由、不放行不可逆 do）；步 20a-1 起并入 `LineGrade::Class`
            let 类线 = 线源.starts_with("类级") && !e_line_empty(&e.line_source);
            // 依据：B72（试用线：用上的那张证书是试用 α 认证的；代价线按所选代价证书）
            let 用证书 = if cost.is_some() || alpha.is_some() {
                代价证书.clone()
            } else {
                所用链(&线源).rec.selected.clone()
            };
            // 等级派生读有效 α（B89）：`CertView.trial` = 证书试用 α 认证 ∨ alpha_eff > α（`jpp-calib::cert_view`）
            let 试用 = !e_line_empty(&e.line_source) && 用证书.as_ref().is_some_and(|c| c.trial);
            if 试用 {
                let c = 用证书.as_ref().expect("试用即有证书");
                // 依据：B72（试用线出口可路由、报 W-trial-line，文本带 α 与 n）；B72-4（步 17b 起等级随材料传递）
                self.trace.warn(format!(
                    "W-trial-line: @{} 键 {} 的线是试用线（α={}，alpha_eff={:.3}，n={}，认证半上侧已决 {}；依据 B72、B89），出口 {} 照常路由，按 B72 不算放行不可逆 do 的可信合取项；等级随材料传递（B72-4）：由本出口选出的材料在下一跳再判断时，下一跳的出口也不算放行不可逆 do 的可信合取项。修法【作者可改】：补足带真值的标注（固定序下字面题式约 60 条，B86）经 calib-import 认证正式线；真值是模型标注时，复核要覆盖认证集（B89）",
                    sp.start,
                    所用链(&线源).key,
                    c.alpha,
                    c.alpha_eff,
                    所用链(&线源).rec.n,
                    c.n_accepted,
                    e.label()
                ));
            }
            // **停岗候选**（B25）：记录已是候选，或本趟漂移信号刚把它标成候选（`报漂移` 在上面已调用）。
            // 用的是题式级线时看题式键。候选线的出口照常路由，不算放行不可逆 do 的可信合取项。
            // 类级线看类键（步 20b：停岗候选与 B68 范围都按实际用的那条记录核）
            let 用链 = 所用链(&线源);
            let 用键 = 用链.key.clone();
            let 候选 = !e_line_empty(&e.line_source)
                && (用链.rec.status == "停岗候选" || self.drift_reported.contains(&用键));
            e.suspend_candidate.set(候选);
            if 候选 && matches!(e.kind, ExitKind::Act | ExitKind::Pick(_) | ExitKind::At(_)) {
                self.trace.warn(format!("W-suspend-candidate: @{} 键 {用键} 是停岗候选（B25），出口 {:?} 不得放行不可逆 do", sp.start, e.kind));
            }
            // 依据：B68（认证范围：线用在认证集材料范围之外，出口照常路由，不算放行不可逆 do 的可信合取项；
            // 记录没有指纹时按范围内处理）
            // B91（步 20d-2）：主指纹外的材料落进某条范围扩展即不算范围外；试用级扩展使出口等级降为试用
            let mut 扩展试用 = false;
            if !e_line_empty(&e.line_source) {
                if let (Some(sc), Some(fp)) = (用链.rec.scope.as_ref(), r.fp.as_ref()) {
                    let 扩展 = sc.outside(fp).and_then(|_| {
                        用链
                            .rec
                            .scope_extensions
                            .iter()
                            .position(|(x, _)| x.outside(fp).is_none())
                    });
                    if let Some(i) = 扩展 {
                        扩展试用 = 用链.rec.scope_extensions[i].1;
                        // 依据：B91（范围扩展按 α 分档；扩展内的出口等级取记录与扩展之低者）
                        self.trace.warn(format!(
                            "W-scope-extension: @{} 键 {用键} 的线用在认证范围外、第 {} 条范围扩展之内的材料上（{}，B91）；出口 {:?} 照常路由{}",
                            sp.start,
                            i + 1,
                            if 扩展试用 { "试用级扩展" } else { "与记录同级的扩展" },
                            e.kind,
                            if 扩展试用 { "，等级按试用，不放行不可逆 do" } else { "" }
                        ));
                    } else if let Some((量, v, lo, hi)) = sc.outside(fp) {
                        e.scope_out.set(true);
                        self.trace.warn(format!(
                            "W-calib-scope: @{} 键 {用键} 的线在 {} 条材料上认证{}，本材料 {量} = {v:.3} 在认证范围 [{lo:.3}, {hi:.3}] 之外；出口 {:?} 照常路由，按 B68 不算放行不可逆 do 的可信合取项。修法【作者可改】：用这类材料补一批带真值的标注（Fable 复核），经 calib-import 并入范围",
                            sp.start,
                            sc.n,
                            用链.rec.scope_n_text.map(|k| format!("（范围来自 {k}/{} 条带文本样本，B104）", 用链.rec.n)).unwrap_or_default(),
                            e.kind
                        ));
                    }
                }
            }
            // 依据：B104-1（线按 δ 平移过而证书没记 δ：路由照常、不放行，每键每趟报一次）
            if !e_line_empty(&e.line_source) && !夹具出口 {
                let 平移无带宽 = 用证书
                    .as_ref()
                    .is_some_and(|c| c.delta_shifted && c.delta.is_none());
                e.delta_unknown.set(平移无带宽);
                if 平移无带宽
                    && self
                        .unknown_reported
                        .insert(format!("W-delta-unknown\u{1f}{用键}"))
                {
                    // 依据：B104-1
                    self.trace.warn(format!(
                        "W-delta-unknown: @{} 键 {用键} 的证书未记录认证带宽（旧证书），出口照常路由，不作放行不可逆 do 的可信合取项（B104）。修法【宿主】：jpp calib-import 重新导入，或等 load（步 20c）重跑写回 δ",
                        sp.start
                    ));
                }
                // 依据：B104-2（记录没有材料指纹 = 范围未知：路由照常、不放行，每键每趟报一次）
                let 范围未知 = 用链.rec.scope.is_none();
                e.scope_unknown.set(范围未知);
                if 范围未知
                    && self
                        .unknown_reported
                        .insert(format!("W-scope-unknown\u{1f}{用键}"))
                {
                    // 依据：B104-2
                    self.trace.warn(format!(
                        "W-scope-unknown: @{} 键 {用键} 的记录没有认证范围指纹（认证集没有材料文本），出口照常路由，不作放行不可逆 do 的可信合取项（B104）。修法【宿主】：带 text 重新导入，或经 B91 扩展并入范围",
                        sp.start
                    ));
                }
            }
            // 逐出口记线等级（步 20f；步 20a-1 起为 `LineGrade`，优先序见其文档）。
            // 出口不进账本，重放时按账本头的记录重算出同一张表。
            let 用线 = !e_line_empty(&e.line_source);
            let 所用 = 所用链(&线源);
            // B19 修订的临时上岗门控，或有效 α 超过试用 α（B89 解读 (b)，步 20c）
            let 临时 = 所用
                .rec
                .truth_gate
                .as_ref()
                .is_some_and(|g| g.starts_with("临时上岗"))
                || (用线 && 用证书.as_ref().is_some_and(|c| c.provisional));
            if 用线
                && let Some(c) = 用证书.as_ref().filter(|c| c.provisional)
                && self
                    .unknown_reported
                    .insert(format!("W-provisional\u{1f}{}", 所用.key))
            {
                // 依据：B89 解读 (b)（地基/附注/2026-09-25-批量裁定.md §五）
                self.trace.warn(format!(
                    "W-provisional: @{} 键 {} 的线有效 α={:.3} 超过试用 α，等级为临时上岗（Provisional，B89），出口照常路由，不作放行不可逆 do 的可信合取项。修法【作者可改】：复核覆盖认证集（B89）",
                    sp.start, 所用.key, c.alpha_eff
                ));
            }
            let grade = if !用线 {
                LineGrade::Cold
            } else if 夹具出口 {
                LineGrade::Fixture
            } else if 类线 {
                LineGrade::Class
            } else if 试用 {
                LineGrade::Trial
            } else if 临时 {
                LineGrade::Provisional
            } else if 扩展试用 {
                // B91：试用级扩展内，正式线（Certified / Form）降为 Trial
                LineGrade::Trial
            } else if 线源.starts_with("题式级") {
                LineGrade::Form
            } else {
                LineGrade::Certified
            };
            e.grade.set(Some(grade));
            // B161（步 25d；主会话 2026-09-26 取保守读法，待 Fable 补注）：计入联合界的 α 只取「这条线的 α 在这份
            // 材料上成立」的出口——正式或题式级、有证书、不在范围外、范围与带宽都已知时取 alpha_eff；试用、临时上岗、
            // 声明线、夹具、类线、冷、范围外、范围未知、带宽未知、无证书一律按 1 计（留 None）。联合界是对外报的担保数，
            // 宁可报大。依据：B161
            if matches!(grade, LineGrade::Certified | LineGrade::Form)
                && !e.scope_out.get()
                && !e.scope_unknown.get()
                && !e.delta_unknown.get()
                && let Some(c) = 用证书.as_ref()
            {
                e.alpha.set(Some(c.alpha_eff));
            }
            // releases：唯一判定点 `Exit::releases`（等级加正交位，不含 taint）
            let releases = e.releases();
            let mut rec = serde_json::json!({
                "site": sp.start, "exit": e.label(), "grade": grade.name(), "releases": releases,
                // B120 (b)（步 20h-2）：材料编号（状态哈希前 12 位）；`sieve` 另补 `index`、`pos`
                "item": 材料摘要(&r.state_hash),
            });
            if 用线 {
                rec["key"] = Json::String(所用.key.clone());
            }
            if e.scope_out.get() {
                rec["scope_out"] = Json::Bool(true);
            }
            if e.suspend_candidate.get() {
                rec["suspend_candidate"] = Json::Bool(true);
            }
            if e.delta_unknown.get() {
                rec["delta_unknown"] = Json::Bool(true);
            }
            if e.scope_unknown.get() {
                rec["scope_unknown"] = Json::Bool(true);
            }
            self.exit_grades.push(rec);
        }
        Ok(出口)
    }
}

/// 报告 `exits` 行的材料编号（B120 (b)，步 20h-2）：状态哈希前 12 位，与待标清单的 `item` 同源可对。
fn 材料摘要(state_hash: &str) -> String {
    state_hash.chars().take(12).collect()
}

/// 值里有没有惰性出口（B94）：出口本身，或列表、记录、`stop` 里有
/// 值里未取回的生成（步 15h-2），按出现顺序（取回顺序另按登记序）
fn 未取生成(v: &Value, out: &mut Vec<Rc<PendingGen>>) {
    match v {
        Value::Gen(g) if g.value().is_none() => out.push(g.clone()),
        Value::List(l) => l.iter().for_each(|x| 未取生成(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| 未取生成(x, out)),
        Value::Stop(x) => 未取生成(x, out),
        _ => {}
    }
}

fn 含惰性出口(v: &Value) -> bool {
    match v {
        Value::Cut(_) | Value::Gen(_) => true,
        Value::List(l) => l.iter().any(含惰性出口),
        Value::Record(r) => r.iter().any(|(_, x)| 含惰性出口(x)),
        Value::Stop(x) => 含惰性出口(x),
        _ => false,
    }
}

/// 作者声明线（B128，步 20j-1）：`cut(r, {declare: {hi, lo?}})`。
impl<'a> Interp<'a> {
    /// 声明分支：按作者写的数切，不查记录定线、不看停岗、不平移 δ、不取严（B128）。判序：J-09 证据不足
    /// → Fail → 缺席 → 过线（`test`：p ≥ hi 为 act、p ≤ lo 为 ignore，其间 band；K 元：p_max ≥ hi 出
    /// pick / at，否则 band；`select` 的置换众数规则照旧）。等级 `Declared`；声明线以 `CalibUsed` 入账（B142）。
    /// 步 20j-3：`stat` 不是 `max` 时先经 `stat_of` 取统计量再过线（`{hi, lo}` 出 test 型出口，`cuts` 出
    /// `At(ℓ)`，出口的臂族随之，B153）；两端开闭按 `closed`（B165）。
    pub(crate) fn cut_declared(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        line: &DeclaredLine,
        stat: &Stat,
        sp: Span,
    ) -> R<Value> {
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib).to_string();
        let 族 = 出口族(r.op, stat, Some(line));
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(
                ExitKind::Unsure(format!("insufficient:{missing}")),
                None,
                族,
                &r.q_hash,
                &r.state_hash,
                r.state_taint,
                sp,
            ));
        }
        let absent = self.absent_marks.get(&r.ledger_key).cloned();
        let 用线 = r.fail.is_none() && absent.is_none();
        let 判序 = |answer: Option<Answer>| {
            jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
                fail: r.fail.as_deref(),
                absent: absent.as_deref(),
                suspended: false,
                line: Some((line.hi, line.lo)),
                cost_requested: false,
                answer,
                // 声明线按写的数用：不平移 δ（B128）
                delta: Some(jpp_value::stat::DECLARED_DELTA),
                mode_share: r.mode_share.get(),
            })
        };
        let (kind, untested) = if !用线 {
            判序(None)
        } else {
            let a = self.answer_of(r).expect("刷新之后答案必然在");
            if stat.is_max() && r.op != Op::Test {
                // K 元单侧线：置换众数规则照旧；上端开（B165）时 p_max 须严格越过 hi
                let p = jpp_value::stat::stat_of(&a, &Stat::Max, None).unwrap_or(f64::MIN);
                match 判序(Some(a)) {
                    (ExitKind::Pick(_) | ExitKind::At(_), _)
                        if !line.closed_hi && !jpp_value::stat::beyond_up(p, line.hi) =>
                    {
                        (ExitKind::Unsure("band".into()), None)
                    }
                    k => k,
                }
            } else {
                let c = self.置信(r, &a);
                let s = match jpp_value::stat::stat_of(&a, stat, c) {
                    Ok(s) => s,
                    Err(jpp_value::stat::StatError::Options(m)) => {
                        return err(Some("E-cut-options"), format!("cut 的 stat：{m}"), sp);
                    }
                    Err(jpp_value::stat::StatError::Unavailable(m)) => {
                        // 依据：B154 (3)（不静默退回 p_max）
                        return err(
                            Some("E-stat-unavailable"),
                            format!(
                                "@{} cut 的 stat: \"confidence\" 取不到数：{m}。修法：换一个随答案报 confidence 的判断器（画像 H9 reports_confidence: true），或在夹具观察里给 confidence（B154）",
                                sp.start
                            ),
                            sp,
                        );
                    }
                };
                (jpp_value::bridge::past_declared(s, line), None)
            }
        };
        if let Some((carrier, 修法)) = &untested {
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind {
                    ExitKind::Unsure(c) => c.clone(),
                    other => format!("{other:?}"),
                }
            ));
        }
        let 线源 = if 用线 {
            let mut s = if line.is_cuts() {
                format!("作者声明·cuts={:?}", line.cuts)
            } else {
                format!("作者声明·hi={}·lo={}", line.hi, line.lo)
            };
            if let Some(c) = line.closed_json() {
                s += &format!("·closed={c}");
            }
            if !stat.is_max() {
                s += &format!("·stat={}", stat.to_json());
            }
            s
        } else {
            String::new()
        };
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            族,
            &r.q_hash,
            &r.state_hash,
            r.state_taint,
            线源,
            sp,
        );
        if let Value::Exit(e) = &出口
            && 用线
        {
            self.note_declared(&key, sp.start, line, stat);
            e.grade.set(Some(LineGrade::Declared));
            // 宿主接受作者声明线放行（B128，步 20j-2）：在写报告行、登记谱系之前置位（两处都读 `releases()`）
            let 接受 = self.entry.accept.declared_lines;
            e.host_accepts_declared.set(接受);
            let evidence = self.声明证据(&key, r, line, stat);
            let n = evidence["labelled"].as_u64().unwrap_or(0);
            // J-08 拒绝报文的「宿主未声明接受作者线」补句只对未接受的出口成立
            if !接受 {
                self.声明出口.insert(
                    e.id,
                    format!("@{} {}，标注 {n} 条", sp.start, 声明说明(line, stat)),
                );
            }
            // 报告的 `declared`：线的数、站点与非缺省端位（缺省值不写，20j-1 的行逐字节不变）
            let mut declared = line.numbers_json();
            declared.insert("site".into(), json!(sp.start));
            if let Some(c) = line.closed_json() {
                declared.insert("closed".into(), c);
            }
            self.exit_grades.push(json!({
                "site": sp.start, "exit": e.label(), "grade": LineGrade::Declared.name(), "releases": e.releases(),
                "item": 材料摘要(&r.state_hash), "key": key,
                "declared": declared,
                "evidence": evidence,
            }));
        }
        Ok(出口)
    }

    /// `stat` 不是 `max` 而没有 `declare`（B153 (1)、B154 (2)）：认证在 p_max 上的线对别的统计量无效，不借——
    /// 出 `Unsure(cold)`，等级 `Cold`，不查记录（不调 `note_calib`）。判序照旧：J-09 证据不足 → Fail → 缺席 → 冷。
    /// `W-untested` 一趟一（键，统计量）一条：去重键含统计量，不吞同键 p_max 冷出口的四条出路告警（B130）。
    pub(crate) fn cut_stat_cold(
        &mut self,
        r: &Reading,
        calib_key: Option<&str>,
        stat: &Stat,
        sp: Span,
    ) -> R<Value> {
        self.flush("cut")?;
        let key = calib_key.unwrap_or(&r.calib).to_string();
        let 族 = 出口族(r.op, stat, None);
        if let Some(missing) = r.missing_evidence.first() {
            return Ok(self.new_exit(
                ExitKind::Unsure(format!("insufficient:{missing}")),
                None,
                族,
                &r.q_hash,
                &r.state_hash,
                r.state_taint,
                sp,
            ));
        }
        let absent = self.absent_marks.get(&r.ledger_key).cloned();
        let (kind, untested) = if r.fail.is_some() || absent.is_some() {
            jpp_value::bridge::decide(&jpp_value::bridge::CutInput {
                fail: r.fail.as_deref(),
                absent: absent.as_deref(),
                suspended: false,
                line: None,
                cost_requested: false,
                answer: None,
                delta: None,
                mode_share: None,
            })
        } else {
            jpp_value::bridge::cold_for_stat(stat)
        };
        // 依据：B153 (1)（认证线在 p_max 上，对别的统计量无效，不借）、B130（冷出口告警一趟一键一条）
        let 报 = untested.is_some()
            && self.unknown_reported.insert(format!(
                "W-untested-calib_line\u{1f}{key}\u{1f}{}",
                stat.to_json()
            ));
        if let Some((carrier, 修法)) = untested.as_ref().filter(|_| 报) {
            // 依据：J-15（未测取保守并带修法）；B153 (1)
            self.trace.warn(format!(
                "W-untested: @{} {carrier} 在本次路径上没有被测量，按 J-15 取保守项（出口 {}）。{修法}",
                sp.start,
                match &kind {
                    ExitKind::Unsure(c) => c.clone(),
                    other => format!("{other:?}"),
                }
            ));
        }
        let 出口 = self.new_exit_from(
            kind,
            untested.map(|(carrier, _)| carrier),
            族,
            &r.q_hash,
            &r.state_hash,
            r.state_taint,
            String::new(),
            sp,
        );
        if let Value::Exit(e) = &出口 {
            e.grade.set(Some(LineGrade::Cold));
            self.exit_grades.push(json!({
                "site": sp.start, "exit": e.label(), "grade": LineGrade::Cold.name(), "releases": e.releases(),
                "item": 材料摘要(&r.state_hash), "key": key,
            }));
        }
        Ok(出口)
    }

    /// 读数的自报置信度（B154）：判断器随答案报了（本趟发出或从账本取回）就用它；没报而判断实例是固定观察
    /// 端口时取夹具缺省 p_max（B154 (1)「缺省 = p_max」；只凭账本重放用账本头的 model_id，同为固定端口）；
    /// 其他判断器没报即没有（不退回 p_max，B154 (3)）。
    pub(crate) fn 置信(&self, r: &Reading, a: &Answer) -> Option<f64> {
        if let Some(c) = self.置信表.borrow().get(&r.id) {
            return Some(*c);
        }
        if r.model_id == jpp_effects::FIXED_MODEL {
            return jpp_value::stat::stat_of(a, &Stat::Max, None).ok();
        }
        None
    }

    /// 声明线入账（B142，步 20j-1）：追加 `CalibUsed`，键 `declared:<校准键>@<站点>`，记录
    /// `{line: "declared", hi, lo, site}`；本趟同键只记一次。续接或重放时账本已有该键而数不同，报 `W-header`
    /// （并列、不拒绝，与 B83 换线同）。依据：B142（地基/附注/2026-09-25-库层批4裁定.md §五）
    /// 步 20j-3：记录加 `stat`（不是 `max` 才写）、`cuts`（代替 `hi`/`lo`）、`closed`（不是全闭才写）；缺省值不写，
    /// 20j-1 记下的普通声明线记录哈希不变（旧账本续接、重放不因本步报 `W-header`）。
    pub(crate) fn note_declared(
        &mut self,
        key: &str,
        site: usize,
        line: &DeclaredLine,
        stat: &Stat,
    ) {
        let k = format!("{}{key}@{site}", jpp_ledger::DECLARED_PREFIX);
        if !self.本趟已记校准.insert(k.clone()) {
            return;
        }
        let mut m = serde_json::Map::new();
        m.insert("line".into(), json!("declared"));
        m.extend(line.numbers_json());
        m.insert("site".into(), json!(site));
        if !stat.is_max() {
            m.insert("stat".into(), stat.to_json());
        }
        if let Some(c) = line.closed_json() {
            m.insert("closed".into(), c);
        }
        let record = Json::Object(m);
        let h = jpp_value::value::hash_of(&[&record.to_string()]);
        if let Some(old) = self.ledger.view().calib_used.get(&k)
            && old.get("hash").and_then(|x| x.as_str()) != Some(h.as_str())
        {
            // 依据：B142 (3)；J-18（账本头不同即不承诺重放一致）
            self.trace.warn(format!(
                "W-header: 账本头不同，不承诺重放一致：declared 旧 {} 新 {} @{site}（B142）",
                记录说明(&old["record"]),
                声明说明(line, stat)
            ));
        }
        // 步 18b：经账本端口追加（与 `note_calib` 同一条路，层末落盘）；端口报错先记下，层末报 `E-ledger-io`
        if let Err(e) = self.ledger.append_calib_used(&k, &h, record) {
            self.账本错.get_or_insert(e);
        }
    }

    /// 运行结束时（B142 (3)）：账本里有 `declared:` 键、而程序里该站点已没有声明，报 `W-header`。
    /// 「该站点仍有声明」按静态看：该站点的 `cut` 带含 `declare` 的记录字面量，或参数不是字面量（可能仍有
    /// 声明，按有计，零误报）。本趟走到过的站点已在 [`Self::note_declared`] 比过，这里跳过。
    pub(crate) fn 声明站点核对(&mut self, program: &jpp_ir::ir::Program) {
        use jpp_ir::ir::{Host, Node};
        let mut 可能有声明: HashSet<usize> = HashSet::new();
        jpp_ir::ir::walk(&program.body, &mut |e| {
            if let Node::Cut { rest, .. } = &e.node {
                let 有 = rest.iter().any(|a| match &a.node {
                    Node::Host(Host::Record(fs)) => fs.iter().any(|(k, _)| k == "declare"),
                    Node::Host(Host::Text(_)) => false,
                    _ => true,
                });
                if 有 {
                    可能有声明.insert(e.span.start);
                }
            }
        });
        let 旧: Vec<(String, Json)> = self
            .ledger
            .view()
            .calib_used
            .iter()
            .filter(|(k, _)| {
                k.starts_with(jpp_ledger::DECLARED_PREFIX) && !self.本趟已记校准.contains(*k)
            })
            .map(|(k, v)| (k.clone(), v["record"].clone()))
            .collect();
        for (_, r) in 旧 {
            let site = r["site"].as_u64().unwrap_or_default() as usize;
            if !可能有声明.contains(&site) {
                // 依据：B142 (3)
                self.trace.warn(format!(
                    "W-header: 账本头不同，不承诺重放一致：declared 旧 {} 新 无 @{site}（程序里该站点已没有声明线，B142）",
                    记录说明(&r)
                ));
            }
        }
    }

    /// 声明线旁的证据（B128；`20` v2 §4.4 第 9 条）：同键记录只读。证据记录取查找链上第一条有线的记录
    /// （题级 → 题式级 → 类级），都没有取题键记录。`near_line` 在运行结束时由 [`Self::声明证据定稿`] 填。
    /// 步 20j-3：同键标注样本是 p_max 上的，`stat` 不是 `max`（或 `cuts` 线）时无从核，`errors_at_line` 为 `null`
    /// （`labelled` 照数，`certified` 照给作对照）；开端（B165）按严格比较数错。
    fn 声明证据(&self, key: &str, r: &Reading, line: &DeclaredLine, stat: &Stat) -> Json {
        let 链 = self.calib.chain(key, r.form_hash.as_deref());
        let 有线 =
            |l: &jpp_effects::views::Link| matches!(l.rec.status.as_str(), "上岗" | "停岗候选");
        let 层们 = [
            (Some(&链.question), "题级"),
            (链.form.as_ref(), "题式级"),
            (链.class.as_ref(), "类级"),
        ];
        let 有线层 = 层们
            .iter()
            .find_map(|(l, 层)| l.filter(|l| 有线(l)).map(|l| (l, *层)));
        let 证据键 = 有线层.map_or(链.question.key.as_str(), |(l, _)| l.key.as_str());
        let 样本 = self.calib.labelled(证据键);
        let 单侧 = r.op != Op::Test;
        let 可核 = stat.is_max() && !line.is_cuts();
        let 上 = |p: f64| {
            if line.closed_hi {
                jpp_value::stat::decided_up(p, line.hi, jpp_value::stat::DECLARED_DELTA)
            } else {
                jpp_value::stat::beyond_up(p, line.hi)
            }
        };
        let 下 = |p: f64| {
            if line.closed_lo {
                jpp_value::stat::decided_down(p, line.lo, jpp_value::stat::DECLARED_DELTA)
            } else {
                jpp_value::stat::beyond_down(p, line.lo)
            }
        };
        let 错 = 样本
            .iter()
            .filter(|(p, 对)| {
                if 上(*p) {
                    !对
                } else {
                    !单侧 && 下(*p) && *对
                }
            })
            .count();
        let certified = match 有线层 {
            Some((l, 层)) => {
                json!({"hi": l.rec.hi, "lo": l.rec.lo, "grade": 记录等级名(&l.rec, 层), "key": l.key})
            }
            None => Json::Null,
        };
        json!({
            "labelled": 样本.len(),
            "errors_at_line": if 样本.is_empty() || !可核 { Json::Null } else { json!(错) },
            "near_line": {"window": 声明窗口, "count": 0, "share": 0.0},
            "certified": certified,
        })
    }

    /// 运行结束时填 `near_line`（本趟该键全部切过的读数里，落在线 ±0.2 内的条数与占比）并发
    /// `W-declared-line`（一趟一键一条）。依据：B128；`20` v2 §4.4 第 9 条
    pub(crate) fn 声明证据定稿(&mut self) {
        let 接受 = self.entry.accept.declared_lines;
        let mut 已报 = HashSet::new();
        let mut 告警 = vec![];
        for row in self.exit_grades.iter_mut() {
            // 线的数：`{hi, lo}` 或 `cuts`（步 20j-3）；不是声明行即跳过
            let d = &row["declared"];
            let (线们, 线文) = match (d["hi"].as_f64(), d["lo"].as_f64(), d["cuts"].as_array()) {
                (Some(hi), Some(lo), _) => (vec![hi, lo], format!("hi={hi} lo={lo}")),
                (_, _, Some(c)) => (
                    c.iter().filter_map(|x| x.as_f64()).collect::<Vec<f64>>(),
                    format!("cuts={}", d["cuts"]),
                ),
                _ => continue,
            };
            let 线文 = match d.get("closed") {
                Some(c) => format!("{线文} closed={c}"),
                None => 线文,
            };
            let key = row["key"].as_str().unwrap_or_default().to_string();
            // 统计量（行上只在不是 max 时有）：`键读数` 的组与告警去重都按（键，统计量）
            let 统计量 = row.get("stat").cloned();
            let 组键 = match &统计量 {
                Some(s) => format!("{key}\u{1f}{s}"),
                None => key.clone(),
            };
            let ps = self.键读数.get(&组键).cloned().unwrap_or_default();
            let 近 = |x: f64, 线: f64| (x - 线).abs() <= 声明窗口 + jpp_value::stat::BOUNDARY_EPS;
            let count = ps
                .iter()
                .filter(|x| 线们.iter().any(|线| 近(**x, *线)))
                .count();
            let share = if ps.is_empty() {
                0.0
            } else {
                (count as f64 / ps.len() as f64 * 1e4).round() / 1e4
            };
            row["evidence"]["near_line"] =
                json!({"window": 声明窗口, "count": count, "share": share});
            if 已报.insert(组键) {
                let ev = &row["evidence"];
                // 步 20j-2：宿主接受与否写进末句
                let 放行 = if 接受 {
                    "宿主已接受作者线放行（--release-on-declared），不可逆动作的后果由宿主担责"
                } else {
                    "放行不可逆动作须宿主接受作者线（CLI：--release-on-declared）"
                };
                let 错 = match (ev["errors_at_line"].as_u64(), &统计量) {
                    (Some(k), _) => format!("按此线切错 {k} 条"),
                    (None, Some(s)) => format!("标注在 p_max 上，对 stat={s} 无从核"),
                    (None, None) => "无标注可核".to_string(),
                };
                let 线文 = match &统计量 {
                    Some(s) => format!("{线文} stat={s}"),
                    None => 线文,
                };
                let 认证 = match ev["certified"].as_object() {
                    Some(c) => format!(
                        "同键认证线 hi={} lo={}（{}）",
                        c["hi"],
                        c["lo"],
                        c["grade"].as_str().unwrap_or_default()
                    ),
                    None => "同键无认证线".to_string(),
                };
                // 依据：B128（地基/附注/2026-09-25-作者主权与策略表达裁定.md §一）
                告警.push(format!(
                    "W-declared-line: @{} 键 {key} 用作者声明线 {线文}（B128）：同键标注 {} 条，{错}；本趟线附近 ±{声明窗口} 的读数 {count}/{}（{share}）；{认证}。出口按你写的数路由，不作错误率保证；{放行}",
                    row["declared"]["site"],
                    ev["labelled"],
                    ps.len()
                ));
            }
        }
        for w in 告警 {
            self.trace.warn(w);
        }
    }
}

/// `near_line` 的半宽：常量在 `jpp-value::stat` 的常量表（基线刷新 2026-09-26 挪入）
use jpp_value::stat::NEAR_LINE_WINDOW as 声明窗口;

/// 声明线的一段文字（J-08 拒绝报文、`W-header`）：`hi=0.7 lo=0.3`、`cuts=[0.5, 1.5]`，非缺省端位与统计量附后
fn 声明说明(line: &DeclaredLine, stat: &Stat) -> String {
    if stat.is_max() {
        line.describe()
    } else {
        format!("{} stat={}", line.describe(), stat.to_json())
    }
}

/// 账本里一条声明记录的文字（`W-header` 的「旧」）：与 [`声明说明`] 同形
fn 记录说明(r: &Json) -> String {
    let mut s = match r.get("cuts") {
        Some(c) => format!("cuts={c}"),
        None => format!("hi={} lo={}", r["hi"], r["lo"]),
    };
    if let Some(c) = r.get("closed") {
        s += &format!(" closed={c}");
    }
    if let Some(x) = r.get("stat") {
        s += &format!(" stat={x}");
    }
    s
}

/// 记录的线等级名（`evidence.certified.grade`）：夹具 / 类级 / 试用 / 临时上岗 / 题式级 / 题级，与出口等级同名
fn 记录等级名(rec: &Lookup, 层: &str) -> &'static str {
    let g = if rec.fixture_line() {
        LineGrade::Fixture
    } else if 层 == "类级" {
        LineGrade::Class
    } else if rec.selected.as_ref().is_some_and(|c| c.trial) {
        LineGrade::Trial
    } else if rec.selected.as_ref().is_some_and(|c| c.provisional)
        || rec
            .truth_gate
            .as_ref()
            .is_some_and(|g| g.starts_with("临时上岗"))
    {
        LineGrade::Provisional
    } else if 层 == "题式级" {
        LineGrade::Form
    } else {
        LineGrade::Certified
    };
    g.name()
}
