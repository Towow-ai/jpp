//! 内置分派表与通用内置（列表、文本、出口读出等）；内核构造的臂在 `constructs/`（20 §2.3 `host_builtins.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_effects::EffectSpec;
use jpp_value::bridge::DeclaredLine;
use jpp_value::stat::Stat;

impl<'a> Interp<'a> {
    // ---------- 内置 ----------

    pub(crate) fn builtin(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        // 声明式拟合的闭包体内（B153 (2)，步 20j-4）：效应、内核构造、出口与责任形式、读答案的刷新点一律拒绝。
        // 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一：体内不得出现效应、构造或刷新点）
        if self.拟合中 > 0 && 拟合内禁(name) {
            return err(
                Some("E-fit-declare-effect"),
                format!(
                    "声明式拟合的闭包里不能调用 {name}：闭包在桥内对统计量求值，只做计算（算术、比较、if、max / min 等纯内置），不发判断、不调效应、不跑构造、不造出口（B153）。修法：把 {name} 移到 fit 之外，结果经 extra 传入"
                ),
                sp,
            );
        }
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
            "cert" => self.b_cert(name, args, sp),
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
            // 文本与数据内置（B157）+ 带种子伪随机（B158）：一支多名转发，实现在 `builtins_text.rs`
            // （步 7t，`21` 原文「步 7c」；与已造的旧步 7c——B83 续接命中记录——撞号改称）。
            "split" | "lower" | "upper" | "trim" | "replace" | "starts_with" | "ends_with"
            | "index_of" | "chars" | "regex_match" | "regex_find" | "sort" | "sort_by"
            | "parse_json" | "to_json" | "hash" | "date_parse" | "date_format" | "date_add"
            | "rand" | "rand_int" | "shuffle" => self.text_builtin(name, args, sp),
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
                "cut(reading)、cut(reading, calib_key)、cut(reading, {declare | cost | alpha | stat}) 或 cut(reading, calib_key, {…})",
                sp,
            );
        }
        // 第二、三位：校准键（Text）与策略记录（B129 三式：`declare` 作者声明线、`cost` 代价、`alpha` 可接受
        // 假放行率；步 20j-3 加 `stat` 统计量，B153、B154）。`cost`/`alpha` 的线仍只来自记录；`declare` 按作者
        // 写的数切（B128）。策略记录的解析在 `解析策略`（`sieve` 的 `{line}` 同用）。依据：B128、B129
        let mut calib: Option<String> = None;
        let mut opts = super::bridge::CutOpts::default();
        let mut 有记录 = false;
        let 选项错 = |msg: String| -> R<Value> { err(Some("E-cut-options"), msg, sp) };
        for a in args.iter().skip(1) {
            match a {
                Value::Text(k, _) if calib.is_none() && !有记录 => calib = Some(k.to_string()),
                Value::Record(_) if !有记录 => {
                    有记录 = true;
                    opts = 解析策略(a, sp)?;
                }
                // 裸数字读作作者声明线 `{declare: {hi: 数}}`（B188 第 1 条、B129 推翻条件 (2) 提前触发；批 9）
                Value::Float(..) | Value::Int(..) if !有记录 => {
                    有记录 = true;
                    let hi = 数值(a).unwrap_or_default();
                    opts.declare = Some(jpp_value::bridge::DeclaredLine::two_sided(hi, hi, false));
                }
                other => {
                    return err(
                        Some("J-03"),
                        format!(
                            "cut 的校准参数是校准记录的键（Text）、策略记录 {{declare | cost | alpha | stat}} 或一个数（读作声明线 {{declare: {{hi: 数}}}}）；收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        // `select`/`measure` 只有单侧线（B63 同形）：声明线给了 lo 即错
        let 读数们: Vec<&Rc<Reading>> = match &args[0] {
            Value::Reading(r) => vec![r],
            Value::List(l) => l
                .iter()
                .filter_map(|x| match x {
                    Value::Reading(r) => Some(r),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        };
        for r in &读数们 {
            if let Err(m) = 核选项(r, &opts) {
                return 选项错(m);
            }
        }
        // 声明式拟合的结果（B153 (2)，步 20j-4）：只走声明分支
        let 有拟合 = match &args[0] {
            Value::Score(_) => true,
            Value::List(l) => l.iter().any(|x| matches!(x, Value::Score(_))),
            _ => false,
        };
        if 有拟合 && let Err(m) = 核拟合选项(&opts) {
            return 选项错(m);
        }
        match &args[0] {
            Value::Reading(r) => self.过桥(r, calib.as_deref(), opts, sp),
            Value::Score(s) => self.cut_score(s, opts, sp),
            Value::List(l) => {
                let mut out = vec![];
                for r in l.iter() {
                    match r {
                        Value::Reading(r) => {
                            out.push(self.过桥(r, calib.as_deref(), opts.clone(), sp)?)
                        }
                        Value::Score(s) => out.push(self.cut_score(s, opts.clone(), sp)?),
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
            return err(
                Some("E-rt-arg"),
                "consume(exit | [exits], \"drop\" | \"branch\")",
                sp,
            );
        };
        // "branch"（issue #56）：程序没有丢掉这次未决，而是把候选都留下、各自跟进（J-05 的细化去向）。
        // 与 drop 一样算消费、一样按账本键解除，但不进丢弃表：不报 W-drop-vs-escalate，跟进的项
        // 出现在返回值里也不报 W-drop-then-return
        let branch = match how.as_ref() {
            "drop" => false,
            "branch" => true,
            _ => {
                return err(
                    Some("E-rt-arg"),
                    "consume 只支持 \"drop\"（显式丢弃）与 \"branch\"（留下候选各自跟进）；升级用 ask",
                    sp,
                );
            }
        };
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
                    if e.is_unsure()
                        && (!e.consumed.get() || crate::duty::已被吸收(e))
                        && 缺席类原因.contains(&c.as_str())
                    {
                        // 依据：B95；被合成吸收过的分量同样核（B162）
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
                // 被 compose、tally 吸收过的分量仍可按边处理：责任在合成出口里，这里按账本键解除（B162）
                if e.is_unsure() && (!e.consumed.get() || crate::duty::已被吸收(e)) {
                    if branch {
                        self.登记解除(&e.clone(), "consume(…, \"branch\")", sp);
                    } else {
                        // drop 是合法去向（12 §6「unsure 显式丢弃并记账」），但要留痕
                        self.trace.warn(format!(
                            "W-drop-vs-escalate: 显式丢弃了未决责任 {}（题 {}）；drop 合法且已记账，但只有 escalate 会把它交给人",
                            e.label(),
                            头(&e.q_hash, 8)
                        ));
                        self.dropped.push(e.clone());
                        self.登记解除(&e.clone(), "consume(…, \"drop\")", sp);
                    }
                }
                e.consumed.set(true);
                *e.consumed_by.borrow_mut() = if branch {
                    "consume:branch"
                } else {
                    "consume:drop"
                }
                .into();
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
            // 依据：B153 (2)（Score 不能读出为数）
            Value::Score(_) => err(
                Some("J-01"),
                "声明式拟合的结果（Score）没有内容可读：它不是数，只能进 cut 的声明线或同一拟合的 order（B153）",
                sp,
            ),
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
        self.登记解除(&u, "escalate", sp);
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
        // 第四参可选：与 `sieve` 同形的选项 `{line}`，原样交给重问那一次的 `cut`（B128 补齐，过程记录
        // 工程-sieve声明线.md §四）。先解析选项，再销旧责任：选项错时旧责任不被动过
        if n != 3 {
            arity(4)?;
        }
        let line = match args.get(3) {
            Some(o) => crate::constructs::sieve::解析筛选项(name, o, sp)?,
            None => super::bridge::CutOpts::default(),
        };
        let (Value::Duty(u), Value::State(s), Value::Question(q)) = (&args[0], &args[1], &args[2])
        else {
            return err(
                Some("J-05"),
                "literalize(未决责任, state, 更字面的题, {line?})：第一个参数要是 unsure 臂收到的那份责任",
                sp,
            );
        };
        let (u, s, mut q) = (u.clone(), s.clone(), q.clone());
        // B84：重问的题来源 = 那个未决出口
        let 未决键 = u.ledger_key.borrow().clone();
        if !未决键.is_empty() {
            Rc::make_mut(&mut q).from_key.insert(未决键);
        }
        self.登记解除(&u, "literalize", sp);
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "literalize".into();
        let reading = self.judge(&s, &[q], sp)?.remove(0);
        match reading {
            Value::Reading(r) => {
                // 依据：B128（逐读数核选项与 cut 同一套，B153 (1)）
                if let Err(m) = 核选项(&r, &line) {
                    return err(Some("E-cut-options"), m, sp);
                }
                self.cut(&r, None, line, sp)
            }
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
        // 步 20j-4：声明式拟合的闭包要在概率与期望档位上取大取小（B153 (2) 的 `max(a.p, b.p)`），
        // 所以收 Float；Int 与 Float 混用提升为 Float，与算术同一条规则（B70）。两个 Int 照旧是 Int
        let 取 = |v: &Value| match v {
            Value::Int(i, _) => Some(*i as f64),
            Value::Float(f, _) => Some(*f),
            _ => None,
        };
        match (&args[0], &args[1]) {
            (Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Int(
                if name == "min" { *a.min(b) } else { *a.max(b) },
                Taint::Trusted.into(),
            )),
            (x, y) => match (取(x), 取(y)) {
                (Some(a), Some(b)) => Ok(Value::Float(
                    if name == "min" { a.min(b) } else { a.max(b) },
                    Taint::Trusted.into(),
                )),
                _ => err(Some("E-rt-arg"), format!("{name}(数, 数)"), sp),
            },
        }
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

/// 策略记录里的数：Int 或 Float（`cut` 的 `declare`/`cost`/`alpha`）
/// `cut` 的 `stat`（B153、B154、B167；步 20j-3）：`"max"`、`"expect"`、`"confidence"`、`"argmax"`（`cut` 另拒，
/// 见 [`核选项`]）或 `{mass: [单元下标…]}`（去重升序）。
pub(crate) fn 解析统计量(v: &Value) -> Result<Stat, String> {
    let 形式 =
        "stat 只认 \"max\" / \"expect\" / \"confidence\" / {mass: [单元下标…]}（B153、B154）";
    match v {
        Value::Text(s, _) => match &**s {
            "max" => Ok(Stat::Max),
            "argmax" => Ok(Stat::Argmax),
            "expect" => Ok(Stat::Expect),
            "confidence" => Ok(Stat::Confidence),
            other => Err(format!("{形式}；收到 \"{other}\"")),
        },
        Value::Record(f) => {
            if f.len() != 1 || f[0].0 != "mass" {
                return Err(format!("{形式}；记录只收 mass 一个字段"));
            }
            let Value::List(l) = &f[0].1 else {
                return Err("mass 要是单元下标的列表：{mass: [0, 1]}".into());
            };
            let mut cells = vec![];
            for x in l.iter() {
                match x {
                    Value::Int(i, _) if *i >= 0 => cells.push(*i as usize),
                    _ => return Err("mass 的单元要是非负整数（候选下标或档位下标）".into()),
                }
            }
            if cells.is_empty() {
                return Err("mass 的单元不能为空".into());
            }
            cells.sort_unstable();
            cells.dedup();
            Ok(Stat::Mass(cells))
        }
        _ => Err(形式.into()),
    }
}

/// `cut` 的 `declare`（B128；`cuts` 为 B153，`closed` 为 B165；步 20j-1、20j-3）：`{hi, lo?, closed?: {hi?, lo?}}`
/// 或 `{cuts: [c₁ < c₂ < …], closed?: {cuts}}`。数的范围随统计量与读数定，在 [`核选项`] 里逐条读数核。
fn 解析声明(d: &Value) -> Result<DeclaredLine, String> {
    let 形式 = "declare 要写成 {hi: 数, lo?: 数}：act 当且仅当读数 ≥ hi，ignore 当且仅当读数 ≤ lo，其间 unsure(band)；只给 hi 时 lo = hi（B128）；stat: \"expect\" 另可写 {cuts: [c₁, c₂, …]} 分桶出 at（B153）";
    let Value::Record(f) = d else {
        return Err(形式.into());
    };
    if let Some((k, _)) = f
        .iter()
        .find(|(k, _)| !matches!(k.as_str(), "hi" | "lo" | "cuts" | "closed"))
    {
        return Err(format!(
            "declare 只收 hi、lo、cuts、closed：{{declare: {{hi: 0.7, lo: 0.3}}}}；收到字段 {k}"
        ));
    }
    let mut line = match (d.get("hi"), d.get("cuts")) {
        (Some(_), Some(_)) => return Err("declare 的 hi 与 cuts 只给一个（B153）".into()),
        (None, None) => return Err(形式.into()),
        (None, Some(c)) => {
            if d.get("lo").is_some() {
                return Err("declare 的 cuts 不与 lo 同给：分桶线没有上下侧（B153）".into());
            }
            let cuts: Vec<f64> = match &c {
                Value::List(l) => l.iter().filter_map(数值).collect(),
                _ => vec![],
            };
            let 个数 = match &c {
                Value::List(l) => l.len(),
                _ => 0,
            };
            if cuts.is_empty() || cuts.len() != 个数 || cuts.windows(2).any(|w| w[0] >= w[1]) {
                return Err(
                    "declare 的 cuts 要是严格递增的数列：{cuts: [0.5, 1.5, 2.5]}（B153）".into(),
                );
            }
            DeclaredLine::with_cuts(cuts)
        }
        (Some(h), None) => {
            let Some(hi) = 数值(&h) else {
                return Err(形式.into());
            };
            let lo = match d.get("lo") {
                None => hi,
                Some(v) => 数值(&v).ok_or("declare 的 lo 要是数")?,
            };
            DeclaredLine::two_sided(hi, lo, d.get("lo").is_some())
        }
    };
    if let Some(c) = d.get("closed") {
        // 依据：B165 (2)(3)（地基/附注/2026-09-26-批6裁定.md §十三）
        let Value::Record(cf) = &c else {
            return Err("closed 要写成 {hi: Bool, lo: Bool} 或 {cuts: Bool}（B165）".into());
        };
        for (k, v) in cf.iter() {
            // B176（步 20j-3 追加 (7)）：cuts 线的 closed.cuts 收 Bool 或与 cuts 等长的 [Bool]，逐切点定开闭
            if k == "cuts" && line.is_cuts() {
                line.closed_cuts = match v {
                    Value::Bool(b, ..) => vec![*b; line.cuts.len()],
                    Value::List(xs) => {
                        let bs: Vec<bool> = xs
                            .iter()
                            .filter_map(|x| match x {
                                Value::Bool(b, ..) => Some(*b),
                                _ => None,
                            })
                            .collect();
                        if bs.len() != xs.len() || bs.len() != line.cuts.len() {
                            return Err(format!(
                                "closed.cuts 要是 true / false，或与 cuts 等长（{} 个）的布尔列表，逐切点定开闭（B176）",
                                line.cuts.len()
                            ));
                        }
                        bs
                    }
                    _ => {
                        return Err(
                            "closed.cuts 要是 true / false，或与 cuts 等长的布尔列表（B176）"
                                .into(),
                        );
                    }
                };
                continue;
            }
            let Value::Bool(b, ..) = v else {
                return Err(format!("closed.{k} 要是 true 或 false（B165）"));
            };
            match (k.as_str(), line.is_cuts()) {
                ("hi", false) => line.closed_hi = *b,
                ("lo", false) if line.lo_given => line.closed_lo = *b,
                ("lo", false) => {
                    return Err(
                        "只给 hi 的单线（lo = hi）不收 closed.lo：写出 lo 再定下端开闭（B165 (3)）"
                            .into(),
                    );
                }
                ("cuts", true) => unreachable!("上面已处理"),
                ("cuts", false) => return Err("closed.cuts 只配 cuts 线（B165）".into()),
                ("hi" | "lo", true) => {
                    return Err(
                        "cuts 线的端位写 closed: {cuts: false}，不收 closed.hi / lo（B165）".into(),
                    );
                }
                (other, _) => {
                    return Err(format!(
                        "closed 只收 hi、lo（或 cuts 线的 cuts）；收到字段 {other}（B165）"
                    ));
                }
            }
        }
    }
    Ok(line)
}

/// `cut` 的策略记录（第二或第三位的记录参数）解析成 [`CutOpts`](super::bridge::CutOpts)：B129 三式 `declare` /
/// `cost` / `alpha` 与 20j-3 的 `stat`。`sieve` 的 `{line: …}` 原样走这里，字段、取值与报错与 `cut` 相同
/// （B128 补齐，过程记录 `地基/过程记录/工程-sieve声明线.md`）。逐读数的搭配另由 [`核选项`] 核。
pub(crate) fn 解析策略(a: &Value, sp: Span) -> R<super::bridge::CutOpts> {
    // 依据：B128、B129（策略三式都在 cut 上；sieve 的 line 原样走这里）
    let mut opts = super::bridge::CutOpts::default();
    let 选项错 = |msg: String| -> R<super::bridge::CutOpts> { err(Some("E-cut-options"), msg, sp) };
    let Value::Record(fields) = a else {
        return err(
            Some("E-rt-arg"),
            format!("cut 的策略参数要是记录，收到 {}", a.type_name()),
            sp,
        );
    };
    if let Some((k, _)) = fields
        .iter()
        .find(|(k, _)| !matches!(k.as_str(), "declare" | "cost" | "alpha" | "stat"))
    {
        return err(
            Some("E-rt-arg"),
            format!(
                "cut 的记录参数只认 declare / cost / alpha / stat：{{declare: {{hi, lo?}}}}、{{cost: [fp, fn]}}、{{alpha: a}}、{{stat: \"expect\", declare: {{…}}}}；收到字段 {k}"
            ),
            sp,
        );
    }
    if let Some(c) = a.get("cost") {
        let nums: Vec<f64> = match &c {
            Value::List(l) => l.iter().filter_map(数值).collect(),
            _ => vec![],
        };
        if nums.len() != 2 || nums.iter().any(|x| !(*x > 0.0)) {
            return err(
                Some("E-rt-arg"),
                "cost 要是两个正数 [fp, fn]：放错一条（假放行）与漏掉一条（假拒绝）的代价",
                sp,
            );
        }
        opts.cost = Some((nums[0], nums[1]));
    }
    if let Some(x) = a.get("alpha") {
        match 数值(&x) {
            Some(v) if v > 0.0 && v < 1.0 => opts.alpha = Some(v),
            _ => {
                return 选项错(
                    "alpha 要是 (0, 1) 之间的数：可接受的假放行率上界，cut 按它在同键证书里选 α ≤ alpha 的一张（B129）".into(),
                );
            }
        }
    }
    if let Some(s) = a.get("stat") {
        // 依据：B153 (1)、B154 (2)（地基/附注/2026-09-26-批6裁定.md §一、§二）
        opts.stat = match 解析统计量(&s) {
            Ok(s) => s,
            Err(m) => return 选项错(m),
        };
    }
    if let Some(d) = a.get("declare") {
        // 依据：B128、B129（地基/附注/2026-09-25-作者主权与策略表达裁定.md §一、§二）
        if opts.cost.is_some() || opts.alpha.is_some() {
            return 选项错(
                "declare 不能与 cost / alpha 同给：声明线没有错误率保证，cost / alpha 无消费者（B129）"
                    .into(),
            );
        }
        opts.declare = match 解析声明(&d) {
            Ok(l) => Some(l),
            Err(m) => return 选项错(m),
        };
    }
    if !opts.stat.is_max() && (opts.cost.is_some() || opts.alpha.is_some()) {
        return 选项错(format!(
            "stat: {} 不收 cost / alpha：二者在 p_max 上的证书里选线，对别的统计量无效；按这个统计量切写 declare（B153）",
            opts.stat.to_json()
        ));
    }
    // B187（批 9 第 3 格）：统计量上没有现成的回答——`mass` 以外的统计量不写线是缺分档参数（形状错）。
    // 放在共用解析里，`cut`、`sieve` 与 `literalize` 的 `{line}` 同一口径
    if opts.declare.is_none() && !opts.stat.is_max() && !matches!(opts.stat, Stat::Mass(_)) {
        return 选项错(format!(
            "cut 的 stat: {} 没有写线：这个统计量上没有现成的回答，要给分档参数——{{stat: {}, declare: {{hi: …, lo: …}}}} 或 {{declare: {{cuts: […]}}}}（B153、B187）",
            opts.stat.to_json(),
            opts.stat.to_json()
        ));
    }
    Ok(opts)
}

/// 逐条读数核 `stat` 与 `declare` 的搭配（B153 (1)、B165 (3)；步 20j-3）：统计量与题型、`mass` 下标、`cuts` 只配
/// `expect`、数的范围（`expect` 为 [0, K−1]，其余 [0, 1]）、K 元 `max` 线不收下侧。违者 `E-cut-options`。
pub(crate) fn 核选项(r: &Reading, opts: &super::bridge::CutOpts) -> Result<(), String> {
    let 档数 = match r.op {
        Op::Test => 0,
        Op::Select => r.over_len,
        Op::Measure => r.scale.len(),
    };
    match &opts.stat {
        Stat::Max | Stat::Confidence => {}
        Stat::Argmax => {
            return Err(
                "stat: \"argmax\" 是胜出单元的下标，cut 不在它上面划线（order 用，B167）".into(),
            );
        }
        Stat::Mass(cells) => {
            if r.op == Op::Test {
                return Err("stat: {mass: …} 只用于 K 元题（select / measure）：test 读数只有 p 一个统计量（B153）".into());
            }
            if let Some(c) = cells.iter().find(|c| **c >= 档数) {
                return Err(format!(
                    "mass 的单元 {c} 越界：这道题只有 {档数} 个单元（下标 0..{档数}）（B153）"
                ));
            }
        }
        Stat::Expect => {
            if r.op != Op::Measure {
                return Err(format!(
                    "stat: \"expect\" 只用于 measure（有序划分的期望档位）；这道题是 {}（B153）",
                    r.op.phys()
                ));
            }
        }
    }
    let Some(l) = &opts.declare else {
        return Ok(());
    };
    if l.is_cuts() && !matches!(opts.stat, Stat::Expect) {
        return Err("declare 的 cuts 只配 stat: \"expect\"（期望档位分桶出 at，B153）".into());
    }
    if opts.stat.is_max() && r.op != Op::Test && l.lo_given {
        return Err(
            "select / measure 的声明线只收 hi：K 元划分没有下侧线，p_max ≥ hi 出 pick / at，否则 unsure(band)（B128、B63）".into(),
        );
    }
    if matches!(opts.stat, Stat::Expect) {
        let 上界 = 档数.saturating_sub(1) as f64;
        let 数们: Vec<f64> = if l.is_cuts() {
            l.cuts.clone()
        } else {
            vec![l.hi, l.lo]
        };
        if 数们.iter().any(|x| !(0.0..=上界).contains(x)) || (!l.is_cuts() && l.lo > l.hi) {
            return Err(format!(
                "stat: \"expect\" 的线要在 [0, {上界}] 之间（这道 measure 有 {档数} 档）且 lo ≤ hi；收到 {}（B153）",
                l.describe()
            ));
        }
    } else if !(0.0..=1.0).contains(&l.hi) || !(0.0..=1.0).contains(&l.lo) || l.lo > l.hi {
        return Err(format!(
            "declare 的两个数要在 [0, 1] 之间且 lo ≤ hi；收到 hi={}、lo={}（B128）",
            l.hi, l.lo
        ));
    }
    Ok(())
}

/// `cut(Score, …)` 的选项（B153 (2)；步 20j-4 预注册第 0 节第 7 条与补记）：只收 `declare`（`{hi, lo?, closed?}`
/// 或 `{cuts, closed?}`）；数的范围不限，只要 `lo ≤ hi`。`stat`、`cost`、`alpha` 报错（`Score` 已是标量、没有证书）。
fn 核拟合选项(opts: &super::bridge::CutOpts) -> Result<(), String> {
    if !opts.stat.is_max() {
        return Err("声明式拟合的结果已是一个数，cut 不收 stat（B153）".into());
    }
    if opts.cost.is_some() || opts.alpha.is_some() {
        return Err(
            "声明式拟合没有证书，cut 不收 cost / alpha；按你的数切写 declare（B153）".into(),
        );
    }
    // B187（批 9 第 3 格）：拟合分数不是判断器的回答，不写线是缺分档参数
    if opts.declare.is_none() {
        return Err(
            "声明式拟合的结果是一个分，没有现成的回答：要写线 cut(s, {declare: {hi: …, lo: …}}) 或 {declare: {cuts: […]}}（B153、B187）".into(),
        );
    }
    if let Some(l) = &opts.declare
        && !l.is_cuts()
        && l.lo > l.hi
    {
        return Err(format!(
            "declare 要 lo ≤ hi；收到 hi={}、lo={}（B128）",
            l.hi, l.lo
        ));
    }
    Ok(())
}

/// 声明式拟合的闭包体内不许调用的名字（B153 (2)）：效应、内核构造、出口与责任形式、读答案的刷新点内置。
/// `if`、`loop` 与纯内置放行（步 20j-4 预注册第 0 节第 4 条）。
fn 拟合内禁(name: &str) -> bool {
    jpp_effects::by_name(name).is_some()
        || crate::caps::construct(name).is_some()
        || matches!(
            name,
            "state"
                | "cut"
                | "handle"
                | "consume"
                | "escalate"
                | "literalize"
                | "unsure"
                | "pending"
                | "content"
                | "mat"
        )
}

fn 数值(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i, _) => Some(*i as f64),
        Value::Float(f, _) => Some(*f),
        _ => None,
    }
}
