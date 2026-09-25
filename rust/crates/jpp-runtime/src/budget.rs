//! 预算核、审计重放折算、判断力缺席的处置（20 §2.3 `budget.rs`；B32、B35）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// 开启审计重放（B35）：只凭账本重现首跑，缺记录即 `E-replay`。CLI 的 `--replay` 用它；`--resume` 不用。
    pub fn audit_replay(mut self) -> Self {
        self.audit.on = true;
        self
    }

    /// 设宿主入口参数（B105；步 14b 起取代 `with_host_input`）：`run` 入口把条目绑定进程序环境、
    /// 整份入口的哈希写进账本头 `entry_hash`。依据：B105（地基/附注/2026-09-25-B105-B106裁定.md）
    pub fn with_entry(mut self, entry: EntryArgs) -> Self {
        self.entry = entry;
        self
    }

    /// 审计重放：从账本取到一次记过的调用，按记录计入预算（融合的多道题按 `call` 只计一次；
    /// 老账本 `call == 0` 时按条计，是上界）。
    pub(crate) fn audit_account(&mut self, call: u64, usd: f64, sp: Span) {
        if !self.audit.on {
            return;
        }
        self.audit.last_site = Some(sp);
        if call == 0 || self.audit.seen_calls.insert(call) {
            self.audit.calls += 1;
            self.audit.usd += usd;
        }
    }

    /// 审计重放遇到账本缺的记录：致命，不进 cause，不挂起（B35 第 1 条）。
    // 依据：B35（审计重放缺记录即 E-replay）
    pub(crate) fn replay_missing(&self, what: String, sp: Span) -> Fault {
        Fault::Error(RtError::new(
            Some("E-replay"),
            format!("重放缺账本记录：{what}。只凭账本重放不发新调用；要继续往下请用 --resume"),
            sp,
        ))
    }

    // ---------- 效应 ----------

    /// 预算核（B93，步 22-0）：只报超额，不停程序。超额时返回报文，由调用处决定怎么停发：
    /// 判断站点按缺席处置（`预算停发`），`gen`/`do` 产出失败值，`ask` 挂起。
    /// 审计重放时，账本里记过的调用照记录算进已花（B35）：首跑在哪里停发，重放就在哪里停发。
    pub(crate) fn charge(&self, calls: u64, usd: f64) -> Result<(), String> {
        let (used_calls, used_usd) = (
            self.cost.calls + self.audit.calls,
            self.cost.usd + self.audit.usd,
        );
        if used_calls + calls > self.budget.calls || used_usd + usd > self.budget.cost {
            return Err(format!(
                "预算耗尽：calls {}+{} / {}，cost {:.6}+{:.6} / {:.6}",
                used_calls, calls, self.budget.calls, used_usd, usd, self.budget.cost
            ));
        }
        Ok(())
    }

    /// 记一次停发（B93）：首次停发时出一条 `W-budget`（列首个未发站点），此后只计数。
    pub(crate) fn 记停发(&mut self, n: u64, site: Span, detail: &str) {
        match &mut self.预算停 {
            Some(b) => b.unsent += n,
            None => {
                // 依据：B93（`12` §3 J-07「停」指停发，不是停程序）
                self.trace.warn(format!(
                    "W-budget: @{} 预算耗尽，此后未发的判断记 Unsure(budget)、效应产出失败值，程序照常返回：{detail}",
                    site.start
                ));
                self.预算停 = Some(BudgetStop {
                    exhausted: true,
                    unsent: n,
                    first_site: site.start,
                });
            }
        }
    }

    /// 一组判断预算停发（B93）：按缺席处置——标 `budget` 给 `cut`、记缺席账（首因 `budget`）、不发。
    /// `cut` 遇到这类读数给 `Unsure(budget)`；续跑（非审计）时这些站点重发，审计重放在同一站点同样停发。
    pub(crate) fn 预算停发(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        site: Span,
        detail: &str,
        attempts: u64,
    ) {
        let mut 新 = 0u64;
        for (_, _, k) in items {
            self.absent_marks.insert(k.clone(), "budget".to_string());
            if self.停发键.insert(k.clone()) {
                新 += 1;
            }
        }
        self.记缺席账(items, "budget", detail, attempts);
        self.记停发(新, site, detail);
    }

    /// 缺席事件入账（只增）：尝试次数记在一组的第一题上（B32 逐次计费、B35 重放按它计入预算）。
    pub(crate) fn 记缺席账(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        cause: &str,
        detail: &str,
        attempts: u64,
    ) {
        for (i, (_, _, k)) in items.iter().enumerate() {
            let mk = format!("absent:{k}");
            if self.ledger.get(&mk).is_none() {
                self.ledger.put(Entry::Absent {
                    key: mk,
                    jkey: self.judge_keys.get(k).cloned(),
                    cause: cause.to_string(),
                    detail: detail.to_string(),
                    attempts: if i == 0 { attempts } else { 0 },
                });
            }
        }
    }

    /// 记一组题的缺席或超时（B32）：标记给 `cut`，账本记事件（重放据此复现），trace 留痕。
    pub(crate) fn 记缺席(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        cause: &str,
        site: Span,
        detail: String,
        attempts: u64,
    ) {
        for (_, _, k) in items {
            self.absent_marks.insert(k.clone(), cause.to_string());
        }
        self.记缺席账(items, cause, &detail, attempts);
        for (_, _, k) in items {
            let mk = format!("absent:{k}");
            self.trace.push(
                "absent",
                &mk,
                false,
                0.0,
                site,
                format!("{cause}：{detail}"),
            );
        }
        self.trace.warn(format!(
            "W-{cause}: @{} {} 道题转 Unsure({cause})：{detail}",
            site.start,
            items.len()
        ));
    }

    /// 缺席策略的 `then`（B32）：escalate → 程序挂起待续跑；conservative → 出口 Unsure(absent) 继续；fail → 运行期错误。
    pub(crate) fn 缺席处置(
        &mut self,
        items: &[(Rc<Question>, Rc<Reading>, String)],
        pol: &jpp_ir::ir::AbsentPolicy,
        site: Span,
        detail: String,
        attempts: u64,
    ) -> R<()> {
        // 挂起与报错也入账：只凭账本重放时照记的处置重现（B35「首跑在哪里停，重放就在哪里停」）
        if pol.then != "conservative" {
            self.记缺席账(items, "absent", &detail, attempts);
        }
        match pol.then.as_str() {
            "fail" => err(
                Some("E-rt-absent"),
                format!("判断器缺席（absent.then=fail）：{detail}"),
                site,
            ),
            "conservative" => {
                self.记缺席(items, "absent", site, detail, attempts);
                Ok(())
            }
            _ => {
                self.trace.warn(format!(
                    "W-absent: @{} 判断器缺席，按 absent.then=escalate 挂起：{detail}",
                    site.start
                ));
                Err(Fault::Halt(Pending {
                    cause: "absent".into(),
                    key: items.first().map(|x| x.2.clone()).unwrap_or_default(),
                    site,
                    detail: format!(
                        "判断器缺席：{detail}。恢复后用 --resume 续跑，已完成的判断不再付费"
                    ),
                }))
            }
        }
    }
}
