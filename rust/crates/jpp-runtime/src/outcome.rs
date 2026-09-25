//! 运行入口与 `Outcome` 组装：`run`、契约值的构造与记账位置（20 §2.3 `outcome.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// 一次运行。计划（`jpp-plan` 按 pass 开关算出）与钩子由宿主交进来（步 14a；此前在这里由
    /// `self.passes` 现算，算法与时点不变：宿主在调用 `run` 前一刻算）。运行时只读计划、问钩子。
    pub fn run(
        mut self,
        program: &Program,
        plan: jpp_ir::plan::Plan,
        hooks: &'a dyn jpp_ir::plan::PlanHooks,
    ) -> Result<Outcome, RtError> {
        self.plan = plan;
        self.hooks = hooks;
        // 依据：B77、J-18（只凭账本重放不比 `calib_hash`，续接两者都比）
        let 场合 = if self.audit.on {
            HeaderCompare::Replay
        } else {
            HeaderCompare::Resume
        };
        // 账本 v3（步 18a，B124）：头在 run() 入口定稿、之后不再改；命中的校准记录在 `CalibUsed` 条目，
        // 入口与当前校准视图逐键比（比对函数在 `jpp-ledger`，B77 的两种场合、B83 的报文都在那里）。
        let calib = self.calib;
        self.ledger.set_header_checked(
            Header::new(
                self.budget.calls,
                self.budget.cost,
                &self.model_id,
                RENDER_VERSION,
                HANDLER_VERSION,
            )
            // 头在 run() 入口定稿：档案是运行前就定下的输入，不该等跑完再补
            .with_profile_hash(self.calib.profile().hash.clone())
            .with_behavior_hash(self.calib.profile().behavior_hash.clone())
            // **运行开始那一刻的校准库**。这里没有「哪一刻」的选择余地：
            // `Interp` 拿的是 `&dyn CalibView`（只读），**一次运行之内它长不了**；
            // 增长只发生在两次运行之间（宿主拿 `Outcome.evidence` 去 `absorb`）。
            .with_calib_hash(Some(self.calib.hash()))
            // 宿主入口参数的哈希（B105-2，整份入口）；无入口时仍为 None（金样不变）
            .with_entry_hash(self.entry.hash()),
            场合,
            &|k: &str| calib.record_json(k),
        );
        if let Some(w) = self.ledger.header_warning.take() {
            self.trace.warn(w);
        }
        // `budget.escalate` 是**问人的总次数上限**，不是「每次运行 k 次」（12:177、:180
        // 「恢复 = 从头重跑…ask 的答案作为账本条目参与重放」）。核上限时要把账本里**已经问过**的
        // 那些算进去——否则上限 2 在三轮恢复里能问到 6 次人。问人是最贵的效应，这个方向是多花钱。
        //
        // 但**不能把它们计进 `cost.asks`**：那个字段报的是「这次运行实际问了几次人」，
        // 重放时本来就该是 0（CLI 的 library_lifecycle 正是这么断言的，它是对的）。
        // 所以另存一个「账本里已有多少次」，只参与核上限，不进 Cost。
        self.asks_in_ledger = self
            .ledger
            .entries
            .iter()
            // 已问未答的不算（步 7 起它们也入账；上限数的是得到回答的问人次数，与入账前一致）
            .filter(|e| {
                matches!(
                    e,
                    Entry::Ask {
                        answer: Some(_),
                        ..
                    }
                )
            })
            .count() as u64;
        self.frames.push(Frame {
            name: "<program>".into(),
            exits: vec![],
            cuts: vec![],
            returns_exit: true,
        });
        let env = env_child(&root_env());
        // 宿主入口参数（B105）：绑定在程序最外层之外，程序自己的同名 `let` 照常遮蔽它。
        // 值条目按读出规则（叶子 taint = 宿主声明）；材料条目盖 origin = input。
        // `purpose` 以名字 `purpose` 绑定为不可信 Text（B105-3），先于值条目。B105-3 的放行论证以题面
        // taint（B58）为前提：目的文本填进题面，切出的出口不可信，不能单独放行不可逆 `do`。步 17b 两者同落。
        // 依据：B105（地基/附注/2026-09-25-B105-B106裁定.md §一、§二）、B58
        if let Some(p) = &self.entry.purpose {
            env_define(
                &env,
                "purpose",
                Value::Text(Rc::from(p.as_str()), Taint::Untrusted.into()),
            );
        }
        for v in &self.entry.values {
            env_define(
                &env,
                &v.name,
                json_to_value(&v.value).with_prov(&jpp_value::prov::Provenance::from(v.taint)),
            );
        }
        for m in &self.entry.materials {
            let bound = m.bound_mat();
            self.entry_mat_names
                .insert(bound.hash.clone(), m.name.clone());
            env_define(&env, &m.name, Value::Mat(Rc::new(bound)));
        }
        // 运行时读 IR（步 12c）；步 12d 起入参就是降级产出的 IR
        let result = self.eval_block(&program.body, &env).and_then(|v| {
            // 刷新点：程序结束（登记了却没人读的判断，到这里也要发出并记账）
            self.flush("end")?;
            // 程序返回前解析顶层帧的惰性出口（B94：返回值离开程序是检视点）
            self.解析本帧()?;
            // 推错的那些：推测花了调用、花了预算，**花掉的必须留痕**。
            let 没用上: Vec<&String> = self.speculated.iter().filter(|k| !self.speculation_used.contains(*k)).collect();
            if !没用上.is_empty() {
                let n = 没用上.len();
                let 例 = 没用上.iter().take(3).map(|k| 头(k, 8)).collect::<Vec<_>>().join(", ");
                self.trace.warn(format!("W-spec-unused: 推测了 {n} 个站点没被走到（{例}…）：这些调用花掉了，结果留在账本里但程序没用上"));
            }
            Ok(v)
        });
        match result {
            Ok(v) => {
                let frame = self.frames.pop().unwrap();
                let mut in_value = HashSet::new();
                collect_exit_ids(&v, &mut in_value);
                let mut returned = vec![];
                for e in frame
                    .exits
                    .iter()
                    .filter(|e| e.is_unsure() && !e.consumed.get())
                {
                    if in_value.contains(&e.id) {
                        e.consumed.set(true);
                        *e.consumed_by.borrow_mut() = "returned".into();
                        returned.push(e.label());
                    } else {
                        return Err(RtError::new(
                            Some("J-05"),
                            format!(
                                "程序结束时有未消费的 {}（题 {}）。{}",
                                e.label(),
                                头(&e.q_hash, 8),
                                self.j05_fix(e)
                            ),
                            e.site,
                        ));
                    }
                }
                self.drop_then_return(&v, &in_value);
                if !returned.is_empty() {
                    self.trace
                        .warn(format!("returned_unsure: {}", returned.join(", ")));
                }
                Ok(Outcome {
                    value: Some(v),
                    pending: vec![],
                    trace: self.trace,
                    cost: self.cost,
                    returned_unsure: returned,
                    layers: self.layers,
                    evidence: self.evidence,
                    exits: self.exit_grades,
                    questions: self.questions,
                    suspend_candidates: {
                        let mut v: Vec<String> = self.drift_reported.iter().cloned().collect();
                        v.sort();
                        v
                    },
                    budget: self.预算停.clone(),
                })
            }
            Err(Fault::Halt(p)) => Ok(Outcome {
                value: None,
                pending: vec![p],
                trace: self.trace,
                cost: self.cost,
                returned_unsure: vec![],
                layers: self.layers,
                evidence: self.evidence,
                exits: self.exit_grades,
                questions: self.questions,
                suspend_candidates: {
                    let mut v: Vec<String> = self.drift_reported.iter().cloned().collect();
                    v.sort();
                    v
                },
                budget: self.预算停.clone(),
            }),
            Err(Fault::Error(e)) => Err(e),
        }
    }

    /// 三路过滤的求值（`05` §1）。返回每道题一份 `{question, act, ignore, unsure, unobserved, stopped}`。
    ///
    /// - 每个元素保留原值（`item`）、输入位置（`index`）、出口（`exit`）、未决原因（`cause`）、
    ///   以及经过的前几次过滤（`trail`，由上一次过滤的出口组成）。
    /// - **完整输入时 act / ignore / unsure 不漏、互斥**；同一元素出现两次就在流里出现两次（各带自己的 index），
    ///   同状态同题只问一次（账本同键去重）。
    /// - **预算提前停止**：没问到的元素进 `unobserved`，不混进 ignore 或 unsure；`stopped` 写明原因。
    ///   这里接住 budget 停机是因为「部分观察 + 未观察范围」本身就是这个算子的一个合法结果；
    ///   之后的判断照常受预算约束。
    /// - 输入元素若本身是一次过滤的产物（带 `item` / `exit` / `trail` 的记录），取它的 `item` 当材料，
    ///   `trail` 接上：**产物与输入同形，可再过滤**（组合封闭）。
    /// - act / ignore 出口在这里被路由，记为已消费；unsure 出口放进 unsure 流，责任随返回值转交（J-05）。
    /// 构造一个契约值：经 `jpp_value::contract::build`（唯一构造，核 B17 两条不变量）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn outcome_value(
        kind: &str,
        value: Value,
        pending: Vec<Value>,
        evidence: Vec<Value>,
        resume: Value,
        spent: (i64, f64),
        detail: Value,
        purpose: Value,
        sp: Span,
    ) -> R<Value> {
        jpp_value::contract::build(jpp_value::contract::Parts {
            kind: kind.to_string(),
            value,
            pending,
            evidence,
            resume,
            spent,
            detail,
            purpose,
        })
        // 依据：B17（内置构造交出的契约值不变量不成立即运行时缺陷，按规则编号报出，不静默放过）
        .map_err(|r| Fault::Error(RtError::new(Some(r.rule), r.message, sp)))
    }

    /// 未决清单的一项：`{element, exit, cause}`
    pub(crate) fn pending_entry(element: Value, exit: &Value) -> Value {
        jpp_value::contract::pending_entry(element, exit)
    }

    /// 本次调用前的记账位置：之后据此算 `spent`，并从 trace 里取本构造触发的判断账本键
    pub(crate) fn mark(&self) -> (usize, u64, f64) {
        (self.trace.events.len(), self.cost.calls, self.cost.usd)
    }

    /// 这一段里判断过的账本键，以及它们花掉的调用与费用。
    /// **按账本记录算，不按本次实际发出的算**：重放时读出同样的调用号与费用，
    /// 契约值因此逐字节不变（J-18）。一次调用里融合了多道题，只计一次。
    pub(crate) fn since(&self, m: (usize, u64, f64)) -> (Vec<Value>, (i64, f64)) {
        let mut keys: Vec<String> = vec![];
        for e in &self.trace.events[m.0.min(self.trace.events.len())..] {
            if jpp_effects::by_name(&e.kind).is_some_and(|s| s.produces_reading)
                && !keys.contains(&e.key)
            {
                keys.push(e.key.clone());
            }
        }
        let mut calls: Vec<u64> = vec![];
        let mut usd = 0.0;
        for (i, k) in keys.iter().enumerate() {
            if let Some(Entry::Judge { call, cost, .. }) = self.ledger.get(k) {
                // 老账本没有调用号：每个键算一次调用（上界）
                let id = if *call == 0 {
                    u64::MAX - i as u64
                } else {
                    *call
                };
                if !calls.contains(&id) {
                    calls.push(id);
                    usd += cost;
                }
            }
        }
        let _ = m.1;
        (
            keys.into_iter().map(|k| Value::text(&k)).collect(),
            (calls.len() as i64, usd),
        )
    }

    /// 输入若是契约值：取出 (产出列表, 未决清单, 证据)；否则 (原列表, 空, 空)。
    /// 契约值的产出不是列表时报错——只有列表产出能交给吃集合的构造。
    pub(crate) fn unpack(
        &self,
        v: &Value,
        who: &str,
        sp: Span,
    ) -> R<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        if is_outcome(v) {
            let items = match v.get("value") {
                Some(Value::List(l)) => l.iter().cloned().collect(),
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "{who} 收到的契约值产出不是列表（是 {}），不能按集合处理；取出其中的列表再交给 {who}",
                            other.map(|x| x.type_name()).unwrap_or("Unit")
                        ),
                        sp,
                    );
                }
            };
            return Ok((items, list_of(v.get("pending")), list_of(v.get("evidence"))));
        }
        match v {
            Value::List(l) => Ok((l.iter().cloned().collect(), vec![], vec![])),
            other => err(
                Some("E-rt-arg"),
                format!("{who} 要列表或契约值，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
}
