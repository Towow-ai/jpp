//! 桥：`cut` 判序、出口构造、漂移告警、读数取用（20 §2.3 `bridge.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// 跨对象偏序分档（`12`:134「相邻档并列」）：按可比值降序，**相邻差 ≤ δ 的并列成一档**。
    /// 失败/停止的读数（J-12）单独一档排最后。
    ///
    /// 为什么是偏序不是全序：δ 是同一读数重测的抖动，δ 之内的差**不是真差别**。
    /// 全序会让「0.71 排在 0.70 前面」看起来像个结论。
    ///
    /// 读答案，所以收 `ReadAnswer` 令牌（B57，步 25-2；唯一调用者是构造 `order`）。
    pub(crate) fn order_tiers(
        &self,
        _ra: &crate::caps::Cap<crate::caps::ReadAnswer>,
        rs: &[Rc<Reading>],
    ) -> Vec<Vec<usize>> {
        let ans = &*self.answers.borrow();
        let mut ok: Vec<(usize, f64)> = vec![];
        let mut failed: Vec<usize> = vec![];
        for (i, r) in rs.iter().enumerate() {
            match rank_value(ans, r) {
                Some(v) => ok.push((i, v)),
                None => failed.push(i),
            }
        }
        ok.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        // 并列的容差取这条线的 δ（步 15d-2：只从记录取）；记录没有 δ 时不并列（只有完全相等才并列）
        let delta = rs
            .first()
            .and_then(|r| self.calib.line_delta(&self.calib.line(&r.calib)))
            .unwrap_or(0.0);
        let mut tiers: Vec<Vec<usize>> = vec![];
        for (i, v) in ok {
            match tiers.last_mut() {
                Some(last)
                    if (rank_value(ans, &rs[*last.last().unwrap()]).unwrap_or(v) - v).abs()
                        <= delta =>
                {
                    last.push(i)
                }
                _ => tiers.push(vec![i]),
            }
        }
        if !failed.is_empty() {
            tiers.push(failed);
        }
        tiers
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
            self.ledger.note_calib_used(key, &h, j);
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
        cost: Option<(f64, f64)>,
        sp: Span,
    ) -> R<Value> {
        if !self.plan.lazy_cut {
            return self.cut(r, calib_key, cost, sp);
        }
        let id = self.next_exit;
        self.next_exit += 1;
        let c = Rc::new(PendingCut {
            reading: r.clone(),
            calib: calib_key.map(String::from),
            cost,
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
        let v = self.cut(&c.reading, c.calib.as_deref(), c.cost, c.site);
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
        Ok(match v {
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
        cost: Option<(f64, f64)>,
        sp: Span,
    ) -> R<Value> {
        let 已记 = self.exit_grades.len();
        let v = self.cut_inner(r, calib_key, cost, sp)?;
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
        cost: Option<(f64, f64)>,
        sp: Span,
    ) -> R<Value> {
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
        let (线, 线源, 夹具) = match cost {
            None => (线, 线源, 夹具),
            Some((fp, fn_)) => {
                let 同代价 = |rec: &Lookup| -> Option<jpp_effects::views::CertView> {
                    rec.certs.iter()
                        .filter(|c| matches!(c.cost, Some((a, b)) if (a - fp).abs() < 1e-12 && (b - fn_).abs() < 1e-12))
                        .min_by(|a, b| a.alpha.partial_cmp(&b.alpha).unwrap_or(std::cmp::Ordering::Equal))
                        .cloned()
                };
                // 题式级记录查到就入账（补丁请求 3）：只凭账本重放时据此补回同一张证书
                let 题式 = self.查题式(r);
                // 候选：题级在前、题式级在后；每张证书带上**它所在记录自己的状态**（补丁请求 2）
                let mut 候选: Vec<(jpp_effects::views::CertView, f64, &str, String)> = vec![];
                if let Some(c) = 同代价(&rec) {
                    候选.push((c, rec.lo, "题级", rec.status.clone()));
                }
                if let Some(f) = 题式.as_ref() {
                    if let Some(c) = 同代价(f) {
                        候选.push((c, f.lo, "题式级", f.status.clone()));
                    }
                }
                // 类级在最后（B34 查找链同序；题式停岗时不借，同非代价分支）
                let 题式停岗 = 题式.as_ref().is_some_and(|f| f.status == "停岗");
                let 类 = if 题式停岗 { None } else { self.查类(key) };
                if let Some((c, k)) = 类.as_ref().and_then(|k| 同代价(k).map(|c| (c, k))) {
                    候选.push((c, k.lo, "类级", k.status.clone()));
                }
                let 可用 = 候选
                    .iter()
                    .find(|(_, _, _, st)| st == "上岗" || st == "停岗候选");
                match 可用 {
                    // 依据：B25（停岗只看所选记录）；B29（线只取按这个代价认证的证书）
                    Some((c, lo, 层, _)) if rec.status != "停岗" => {
                        代价证书 = Some(c.clone());
                        (
                            Some((c.hi, lo.min(c.hi))),
                            format!(
                                "{层}·{}证书:α={:.2}·代价(fp={fp},fn={fn_})",
                                试用前缀(c),
                                c.alpha
                            ),
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
        let delta = self.calib.line_delta(&所用链(&线源).rec);
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
        if let Some((carrier, 修法)) = &untested {
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
            let 用证书 = if cost.is_some() {
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
fn 含惰性出口(v: &Value) -> bool {
    match v {
        Value::Cut(_) => true,
        Value::List(l) => l.iter().any(含惰性出口),
        Value::Record(r) => r.iter().any(|(_, x)| 含惰性出口(x)),
        Value::Stop(x) => 含惰性出口(x),
        _ => false,
    }
}
