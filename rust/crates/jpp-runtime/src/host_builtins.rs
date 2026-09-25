//! 内置分派表与通用内置（列表、文本、出口读出等）；内核构造的臂在 `constructs/`（20 §2.3 `host_builtins.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_effects::EffectSpec;

impl<'a> Interp<'a> {
    // ---------- 内置 ----------

    pub(crate) fn builtin(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        // 效应经注册表取 `EffectSpec`，按字段分派（步 15a，`20` A2），不在本表按效应名分支
        if let Some(s) = jpp_effects::by_name(name) {
            return self.effect_builtin(s, name, args, sp);
        }
        // 内核构造经注册表分派：按 `ConstructSpec.privileges` 造能力令牌再调实现（B57，步 25-2）
        if let Some(c) = crate::caps::construct(name) {
            return self.call_construct(c, name, args, sp);
        }
        match name {
            "state" => self.make_state(&args, sp),
            "cut" => self.b_cut(name, args, sp),
            "handle" => self.b_handle(name, args, sp),
            "consume" => self.b_consume(name, args, sp),
            "mat" => self.b_mat(name, args, sp),
            "content" => self.b_content(name, args, sp),
            "unsure" => self.b_unsure(name, args, sp),
            // **J-15 那一位对 handler 可见**，不是只进 trace（`12` §2.11 硬要求一）。
            // 理由是路由真的不同：`tie` 的既定去向是「逐候选 noul」，而没测过的那条路
            // （K-noul）**本来就是逐候选 noul，路过去是空转**。handler 看不见那一位，
            // 就只能把两种情形当同一件事办——**那正是要消除的东西**。
            //
            // 返回 `""` 表示「都测过了」，返回载体名表示「那个量没测」。与 `unsure_cause`
            // 一样是**只读快照，不转移责任**（见 tests/duty.rs：读原因不算处理）。
            "taint" => self.b_taint(name, args, sp),
            "line_source" => self.b_line_source(name, args, sp),
            "untested" => self.b_untested(name, args, sp),
            "unsure_cause" => self.b_unsure_cause(name, args, sp),
            // 合法去向之一：把责任交给明确关联的人工请求（效应 ask）
            "escalate" => self.b_escalate(name, args, sp),
            // 合法去向之一：接走旧责任、按更字面的题重问，产生新的待处理出口（效应 judge）
            "literalize" => self.b_literalize(name, args, sp),
            "pending" => self.b_pending(name, args, sp),
            "fail" => self.b_fail(name, args, sp),
            "is_fail" => self.b_is_fail(name, args, sp),
            // 账本键取用：读法内置，与 taint / unsure_cause 同类（B138 (3)，步 25-2c 由构造表移来）
            "key_of" => self.b_key_of(name, args, sp),
            "exit_kind" => self.b_exit_kind(name, args, sp),
            "loop" => self.b_loop(name, args, sp),
            "stop" => self.b_stop(name, args, sp),
            "len" => self.b_len(name, args, sp),
            "map" | "filter" => self.b_map(name, args, sp),
            "fold" => self.b_fold(name, args, sp),
            "range" => self.b_range(name, args, sp),
            "append" => self.b_append(name, args, sp),
            "concat" => self.b_concat(name, args, sp),
            "slice" => self.b_slice(name, args, sp),
            "contains" => self.b_contains(name, args, sp),
            "sum" => self.b_sum(name, args, sp),
            "min" | "max" => self.b_min(name, args, sp),
            "abs" => self.b_abs(name, args, sp),
            "floor" => self.b_floor(name, args, sp),
            "reverse" => self.b_reverse(name, args, sp),
            "keys" => self.b_keys(name, args, sp),
            "has" => self.b_has(name, args, sp),
            "with" => self.b_with(name, args, sp),
            "text" => self.b_text(name, args, sp),
            "join" => self.b_join(name, args, sp),
            "print" => self.b_print(name, args, sp),
            _ => err(Some("E-rt-name"), format!("未知内置 {name}"), sp),
        }
    }
    /// 账本键取用（B17 不变量 3：证据只存账本键）。步 25-2c 起是读法内置，不是内核构造（B138 (3)）。
    #[allow(unused_variables)]
    pub(crate) fn b_key_of(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        // 账本键：出口取它来自的那条账本记录；读数取自己的键；契约值取它的证据列表
        arity(1)?;
        match &args[0] {
            Value::Exit(e) | Value::Duty(e) => Ok(Value::text(&e.ledger_key.borrow())),
            Value::Reading(r) => Ok(Value::text(&r.ledger_key)),
            v if is_outcome(v) => Ok(v.get("evidence").unwrap_or(Value::list(vec![]))),
            other => err(
                Some("E-rt-arg"),
                format!("key_of 收出口、读数或契约值，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_judge(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        // 12:129 的向量化形式：`judge(ss: [State], qs) → [Readings]`「状态列表，同层并发」。
        // 同一道题问多个对象——判断向量（`order`）要的正是这个形状。
        // 它们同层登记，所以一次刷新就全发出去。
        if let Value::List(states) = &args[0] {
            let mut qs = vec![];
            match &args[1] {
                Value::Question(q) => qs.push(q.clone()),
                Value::List(l) => {
                    for q in l.iter() {
                        match q {
                            Value::Question(q) => qs.push(q.clone()),
                            _ => return err(Some("E-rt-arg"), "judge 的题列表里有非题", sp),
                        }
                    }
                }
                _ => return err(Some("E-rt-arg"), "judge 的第二个参数要是题或题列表", sp),
            }
            let mut out = vec![];
            for st in states.iter() {
                let Value::State(st) = st else {
                    return err(Some("E-rt-arg"), "judge 的状态列表里有非状态", sp);
                };
                let rs = self.judge(st, &qs, sp)?;
                // 单题时每个对象给一条读数（而不是一个只有一条的列表），`order` 才好用
                out.push(if qs.len() == 1 {
                    rs.into_iter().next().expect("单题一条")
                } else {
                    Value::list(rs)
                });
            }
            return Ok(Value::list(out));
        }
        let Value::State(s) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                "judge(state | [states], question | [questions])",
                sp,
            );
        };
        match &args[1] {
            Value::Question(q) => Ok(self.judge(s, &[q.clone()], sp)?.remove(0)),
            Value::List(l) => {
                let mut qs = vec![];
                for q in l.iter() {
                    match q {
                        Value::Question(q) => qs.push(q.clone()),
                        _ => return err(Some("E-rt-arg"), "judge 的题列表里有非题", sp),
                    }
                }
                Ok(Value::list(self.judge(s, &qs, sp)?))
            }
            _ => err(Some("E-rt-arg"), "judge 的第二个参数要是题或题列表", sp),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_cut(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        if n == 0 || n > 3 {
            return err(
                Some("E-rt-arg"),
                "cut(reading)、cut(reading, calib_key)、cut(reading, {cost: [fp, fn]}) 或 cut(reading, calib_key, {cost: [fp, fn]})",
                sp,
            );
        }
        // 第二、三位：校准键（Text）与代价（记录 {cost: [fp, fn]}，B29）。线仍只来自记录：
        // 给了代价，就用该记录上按这个代价矩阵认证过的那张证书的线；没有这张证书 → 冷。
        let mut calib: Option<String> = None;
        let mut cost: Option<(f64, f64)> = None;
        for a in args.iter().skip(1) {
            match a {
                Value::Text(k, _) if calib.is_none() && cost.is_none() => {
                    calib = Some(k.to_string())
                }
                Value::Record(_) if cost.is_none() => {
                    let c = a.get("cost").ok_or_else(|| {
                        Fault::Error(RtError::new(
                            Some("E-rt-arg"),
                            "cut 的记录参数只认 {cost: [fp, fn]}",
                            sp,
                        ))
                    })?;
                    let nums: Vec<f64> = match &c {
                        Value::List(l) => l
                            .iter()
                            .filter_map(|x| match x {
                                Value::Int(i, _) => Some(*i as f64),
                                Value::Float(f, _) => Some(*f),
                                _ => None,
                            })
                            .collect(),
                        _ => vec![],
                    };
                    if nums.len() != 2 || nums.iter().any(|x| !(*x > 0.0)) {
                        return err(
                            Some("E-rt-arg"),
                            "cost 要是两个正数 [fp, fn]：放错一条（假放行）与漏掉一条（假拒绝）的代价",
                            sp,
                        );
                    }
                    cost = Some((nums[0], nums[1]));
                }
                other => {
                    return err(
                        Some("J-03"),
                        format!(
                            "cut 的校准参数必须是校准记录的键（Text）或代价记录 {{cost: [fp, fn]}}，不能是字面量线；收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        match &args[0] {
            Value::Reading(r) => self.cut(r, calib.as_deref(), cost, sp),
            Value::List(l) => {
                let mut out = vec![];
                for r in l.iter() {
                    match r {
                        Value::Reading(r) => out.push(self.cut(r, calib.as_deref(), cost, sp)?),
                        _ => return err(Some("E-rt-arg"), "cut 的列表里有非读数", sp),
                    }
                }
                Ok(Value::list(out))
            }
            other => err(
                Some("E-rt-arg"),
                format!("cut 只收读数，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_handle(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let Value::Exit(e) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                format!("handle 的第一个参数要是出口，收到 {}", args[0].type_name()),
                sp,
            );
        };
        let e = e.clone();
        self.handle(&e, &args[1], sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_consume(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let Value::Text(how, _) = &args[1] else {
            return err(Some("E-rt-arg"), "consume(exit | [exits], \"drop\")", sp);
        };
        if how.as_ref() != "drop" {
            return err(
                Some("E-rt-arg"),
                "consume 目前只支持 \"drop\"；升级用 ask",
                sp,
            );
        }
        // B95（步 21）：契约值不能整份 drop——一行丢掉整份未决清单（含预算未观察项）比转交还短，
        // 正是非设计者程序绕过 J-05 的写法。依据：B95、`12` §3 J-05 注
        if is_outcome(&args[0]) {
            return err(
                Some("J-05"),
                "consume 不收契约值：整份丢掉会把它的未决清单（含预算停机没观察到的项）一起从输出里抹掉。修法：转交——把 undecided(o) 与 unobserved(o)（或 o.pending）放进返回值，元素投影保留 exit 字段；判过而拿不准的项（原因不是 budget、absent、latency）确实不进入任何输出、不参与路由，才逐项丢：map(filter(undecided(o), fn(p) { p.cause != \"absent\" && p.cause != \"latency\" }), fn(p) { consume(p.exit, \"drop\") })，其余转交",
                sp,
            );
        }
        let list: Vec<Value> = match &args[0] {
            Value::List(l) => l.iter().cloned().collect(),
            v => vec![v.clone()],
        };
        for v in &list {
            match v {
                Value::Exit(e) | Value::Duty(e) => {
                    // B95：缺席类原因不是「判过而拿不准」，是没观察到；丢掉等于把预算停机或判断器缺席
                    // 从输出里抹掉。去向只剩转交或 escalate。依据：B95、B93（缺席原因集合）
                    let c = e.cause();
                    if e.is_unsure() && !e.consumed.get() && 缺席类原因.contains(&c.as_str()) {
                        return err(
                            Some("E-drop-unobserved"),
                            format!(
                                "{} 是没观察到的项（原因 {c}），不能 drop：它不是判过而拿不准，丢掉就把{}从输出里抹掉了。修法：转交——放进返回值（契约值的写 unobserved(o) 或 o.pending，元素投影保留 exit 字段），或 escalate(u, …) 交给人",
                                e.label(),
                                if c == "budget" {
                                    "预算停机"
                                } else {
                                    "判断器缺席"
                                }
                            ),
                            sp,
                        );
                    }
                }
                _ => return err(Some("E-rt-arg"), "consume 只收出口或未决责任", sp),
            }
        }
        for v in &list {
            if let Value::Exit(e) | Value::Duty(e) = v {
                if e.is_unsure() && !e.consumed.get() {
                    // drop 是合法去向（12 §6「unsure 显式丢弃并记账」），但要留痕
                    self.trace.warn(format!(
                            "W-drop-vs-escalate: 显式丢弃了未决责任 {}（题 {}）；drop 合法且已记账，但只有 escalate 会把它交给人",
                            e.label(),
                            头(&e.q_hash, 8)
                        ));
                    self.dropped.push(e.clone());
                }
                e.consumed.set(true);
                *e.consumed_by.borrow_mut() = "consume:drop".into();
            }
        }
        Ok(Value::Unit)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_gen(
        &mut self,
        s: &'static EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(4)?;
        let (Value::Text(p, _), ctx, Value::Int(k, _), Value::Int(r, _)) =
            (&args[0], &args[1], &args[2], &args[3])
        else {
            return err(Some("E-rt-arg"), "gen(prompt, [ctx], n, retry_seq)", sp);
        };
        let (ctx, _) = self.as_mats(ctx, "ctx", sp)?;
        let p = p.to_string();
        self.generate(s, &p, &ctx, *k as usize, *r, sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_do(
        &mut self,
        s: &'static EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::Text(a, _), Value::List(l), Value::Int(i, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "do(action, [args], iter_seq)", sp);
        };
        let a = a.to_string();
        let l: Vec<Value> = l.iter().cloned().collect();
        self.do_(s, &a, &l, *i, sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_ask(
        &mut self,
        spec: &'static EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::State(s), Value::Question(q)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "ask(state, question)", sp);
        };
        let (s, q) = (s.clone(), q.clone());
        self.ask(spec, &s, &q, sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_transform(
        &mut self,
        s: &'static EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        if n < 1 {
            return err(Some("E-rt-arg"), "transform(f, mats…)", sp);
        }
        let Value::Fn(f) = &args[0] else {
            return err(Some("E-rt-arg"), "transform 的第一个参数要是函数", sp);
        };
        let f = f.clone();
        self.transform(s, &f, &args[1..], sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_mat(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        Ok(Value::Mat(Rc::new(self.as_mat(&args[0], "mat", sp)?)))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_content(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            // 读出规则（B33 第 2 点）：从 untrusted 材料读出的值，所有叶子标 untrusted。
            // 值自己带着来源，拼接、join、text、取字段之后仍带着（内置输出 ∨ 输入），
            // 取代此前按值匹配的旁路表（982d7ca）：那张表没有作用域，同时漏与串。
            // B84：读出的叶子同时带材料的来源读数
            Value::Mat(m) => Ok(json_to_value(&m.content).with_prov(&m.prov())),
            Value::Reading(_) => err(Some("J-01"), "读数没有内容可读；只能经 cut 离开", sp),
            other => err(
                Some("E-rt-arg"),
                format!("content 只收材料，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_unsure(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            // 重新包装：责任继续由这个出口带着，交给调用者
            Value::Duty(e) => Ok(Value::Exit(e.clone())),
            Value::Text(c, _) => {
                let c = c.to_string();
                Ok(self.new_exit(
                    ExitKind::Unsure(c),
                    None,
                    Op::Test,
                    "explicit",
                    "",
                    Taint::Trusted,
                    sp,
                ))
            }
            other => err(
                Some("E-rt-arg"),
                format!(
                    "unsure(未决责任) 重新包装，或 unsure(原因: Text) 新造一个；收到 {}",
                    other.type_name()
                ),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_taint(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        if args.len() != 1 {
            return err(Some("E-rt-arg"), "taint(出口)", sp);
        }
        match &args[0] {
            // **宪法第 44 行唯一那条 IFC 纪律建在 taint 上，而 `taint` 从 `cause`
            // 删掉之后，`.jpp` 作者再没有任何东西能说出「这个判断站在不可信材料上」**
            // ——J-08 只会在 `do` 那里**拒绝**，作者拿不到任何**在被拒绝之前**读得到的东西。
            // 对照：「测没测过」被判为必须是一位正交的、对 handler 可见的东西。
            // **重的那条待遇更弱**，这里把它补齐。
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(match e.taint {
                Taint::Trusted => "trusted",
                Taint::Untrusted => "untrusted",
            })),
            other => err(
                Some("E-rt-arg"),
                format!("taint 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_line_source(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        if args.len() != 1 {
            return err(Some("E-rt-arg"), "line_source(出口)", sp);
        }
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.line_source)),
            other => err(
                Some("E-rt-arg"),
                format!("line_source 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_untested(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(e.untested().unwrap_or(""))),
            other => err(
                Some("E-rt-arg"),
                format!("untested 只收未决责任或出口，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_unsure_cause(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::Duty(e) | Value::Exit(e) => Ok(Value::text(&e.cause())),
            other => err(
                Some("E-rt-arg"),
                format!(
                    "unsure_cause 只收未决责任或出口，收到 {}",
                    other.type_name()
                ),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_escalate(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2])
        else {
            return err(
                Some("J-05"),
                "escalate(未决责任, state, 题)：第一个参数要是 unsure 臂收到的那份责任",
                sp,
            );
        };
        let (u, s, q) = (u.clone(), s.clone(), q.clone());
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "escalate".into();
        // escalate 隐含的效应是「输出为人的回答」的那一个（按注册表字段取，步 15a）
        let spec = jpp_effects::ALL
            .into_iter()
            .map(jpp_effects::spec)
            .find(|e| e.output_shape == jpp_effects::OutputShape::Answer)
            .expect("注册表里有问人");
        self.ask(spec, &s, &q, sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_literalize(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2])
        else {
            return err(
                Some("J-05"),
                "literalize(未决责任, state, 更字面的题)：第一个参数要是 unsure 臂收到的那份责任",
                sp,
            );
        };
        let (u, s, mut q) = (u.clone(), s.clone(), q.clone());
        // B84：重问的题来源 = 那个未决出口
        let 未决键 = u.ledger_key.borrow().clone();
        if !未决键.is_empty() {
            Rc::make_mut(&mut q).from_key.insert(未决键);
        }
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "literalize".into();
        let reading = self.judge(&s, &[q], sp)?.remove(0);
        match reading {
            Value::Reading(r) => self.cut(&r, None, None, sp),
            other => Ok(other),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_pending(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let Value::Text(c, _) = &args[0] else {
            return err(Some("E-rt-arg"), "pending(reason: Text)", sp);
        };
        Err(Fault::Halt(Pending {
            cause: "explicit".into(),
            key: String::new(),
            site: sp,
            detail: c.to_string(),
        }))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_fail(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let Value::Text(c, t) = &args[0] else {
            return err(Some("E-rt-arg"), "fail(reason: Text)", sp);
        };
        Ok(Value::Fail(Rc::from(c.as_ref()), t.clone()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_is_fail(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        Ok(Value::Bool(
            matches!(args[0], Value::Fail(..)),
            Taint::Trusted.into(),
            GuardEv::EMPTY,
        ))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_exit_kind(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::Exit(e) => Ok(Value::text(&e.label())),
            _ => err(Some("E-rt-arg"), "exit_kind 只收出口", sp),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_loop(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let Value::Int(b, _) = &args[0] else {
            return err(Some("J-06"), "loop 的 bound 必须是整数字面量或整数值", sp);
        };
        let (b, init, step) = (*b, args[1].clone(), args[2].clone());
        self.loop_(b, init, &step, sp)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_stop(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        Ok(Value::Stop(Rc::new(args[0].clone())))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_len(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::List(l) => Ok(Value::Int(l.len() as i64, Taint::Trusted.into())),
            Value::Text(t, _) => Ok(Value::Int(t.chars().count() as i64, Taint::Trusted.into())),
            Value::Record(r) => Ok(Value::Int(r.len() as i64, Taint::Trusted.into())),
            other => err(
                Some("E-rt-type"),
                format!("len 不适用于 {}", other.type_name()),
                sp,
            ),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_map(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::List(l), f) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), format!("{name}(list, fn)"), sp);
        };
        self.vectorize_ahead(f, l, sp);
        let mut out = vec![];
        for it in l.iter() {
            let r = self.apply(f.clone(), vec![it.clone()], sp)?;
            if name == "map" {
                out.push(r);
                continue;
            }
            // filter 的谓词必须返回 Bool。返回别的东西以前被**静默当假**：
            // 出口、未决责任传进来会无声消失，正是 13 §3 要堵的那类。
            match r {
                Value::Bool(true, _, _) => out.push(it.clone()),
                Value::Bool(false, _, _) => {}
                other => {
                    return err(
                        Some("E-rt-type"),
                        format!(
                            "filter 的谓词要返回 Bool（真假），收到 {}。返回别的东西以前被当成假、元素被静默丢掉。修法：让谓词自己算出真假再返回",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        Ok(Value::list(out))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_fold(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::List(l), init, f) = (&args[0], &args[1], &args[2]) else {
            return err(Some("E-rt-arg"), "fold(list, init, fn(acc, x))", sp);
        };
        let mut acc = init.clone();
        for it in l.iter() {
            acc = self.apply(f.clone(), vec![acc, it.clone()], sp)?;
        }
        Ok(acc)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_range(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "range(a, b)", sp);
        };
        Ok(Value::list((*a..*b).map(Value::int).collect()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_append(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "append(list, v)", sp);
        };
        let mut v: Vec<Value> = l.iter().cloned().collect();
        v.push(args[1].clone());
        Ok(Value::list(v))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_concat(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::List(a), Value::List(b)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "concat(a, b)", sp);
        };
        Ok(Value::list(a.iter().chain(b.iter()).cloned().collect()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_slice(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::List(l), Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "slice(list, a, b)", sp);
        };
        let a = (*a).clamp(0, l.len() as i64) as usize;
        let b = (*b).clamp(a as i64, l.len() as i64) as usize;
        Ok(Value::list(l[a..b].to_vec()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_contains(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "contains(list, v)", sp);
        };
        for it in l.iter() {
            match it.equals(&args[1]) {
                Some(true) => return Ok(Value::Bool(true, Taint::Trusted.into(), GuardEv::EMPTY)),
                None => return err(Some("J-01"), "读数不可比", sp),
                _ => {}
            }
        }
        Ok(Value::Bool(false, Taint::Trusted.into(), GuardEv::EMPTY))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_sum(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "sum(list)", sp);
        };
        let mut acc = Value::Int(0, Taint::Trusted.into());
        for it in l.iter() {
            acc = self.binop("+", acc, it.clone(), sp)?;
        }
        Ok(acc)
    }
    #[allow(unused_variables)]
    pub(crate) fn b_min(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), format!("{name}(Int, Int)"), sp);
        };
        Ok(Value::Int(
            if name == "min" { *a.min(b) } else { *a.max(b) },
            Taint::Trusted.into(),
        ))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_abs(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            // 13 §6：i64::MIN 没有对应的正数，取绝对值同样越界
            Value::Int(a, _) => Ok(Value::Int(
                a.checked_abs()
                    .ok_or_else(|| overflow("取绝对值", *a, 0, sp))?,
                Taint::Trusted.into(),
            )),
            Value::Float(a, _) => Ok(Value::Float(a.abs(), Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "abs(number)", sp),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_floor(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::Float(a, _) => Ok(Value::Int(a.floor() as i64, Taint::Trusted.into())),
            Value::Int(a, _) => Ok(Value::Int(*a, Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "floor(number)", sp),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_reverse(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "reverse(list)", sp);
        };
        Ok(Value::list(l.iter().rev().cloned().collect()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_keys(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let Value::Record(r) = &args[0] else {
            return err(Some("E-rt-arg"), "keys(record)", sp);
        };
        Ok(Value::list(r.iter().map(|(k, _)| Value::text(k)).collect()))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_has(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::Record(_), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "has(record, key)", sp);
        };
        Ok(Value::Bool(
            args[0].get(k).is_some(),
            Taint::Trusted.into(),
            GuardEv::EMPTY,
        ))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_with(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(3)?;
        let (Value::Record(r), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "with(record, key, value)", sp);
        };
        let mut v: Vec<(String, Value)> = r
            .iter()
            .filter(|(kk, _)| kk.as_str() != k.as_ref())
            .cloned()
            .collect();
        v.push((k.to_string(), args[2].clone()));
        Ok(Value::record(v))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_text(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        match &args[0] {
            Value::Text(t, _) => Ok(Value::text(t)),
            Value::Reading(_) => err(Some("J-01"), "读数不能转文字", sp),
            // 读出规则：材料的文字带材料的位；其余由分派处的 ∨ 输入给出
            // B84：同时带材料的来源读数
            Value::Mat(m) => Ok(Value::Text(Rc::from(m.text().as_str()), m.prov())),
            other => Ok(Value::text(other.to_json().to_string().trim_matches('"'))),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn b_join(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(2)?;
        let (Value::List(l), Value::Text(sep, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "join([Text], sep)", sp);
        };
        let parts: Vec<String> = l
            .iter()
            .map(|v| match v {
                Value::Text(t, _) => t.to_string(),
                o => o.to_json().to_string(),
            })
            .collect();
        Ok(Value::text(&parts.join(sep)))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_print(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        arity(1)?;
        let s = args[0].to_json().to_string();
        self.trace.push("print", "", false, 0.0, sp, s);
        Ok(Value::Unit)
    }
}
