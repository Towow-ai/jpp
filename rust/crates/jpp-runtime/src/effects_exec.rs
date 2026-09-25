//! 效应执行：`do`、`gen`、`ask`、`transform` 的键、账本与 taint（20 §2.3 `effects_exec.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_effects::{EffectSpec, TaintRule};

/// 效应输出的 taint 按 `EffectSpec.taint_rule` 定（步 15a；规则表在注册表，格运算在 `jpp-value`，
/// `20` §2.3）。`inherited` 是输入 taint 的 ∨；`declared` 是声明位（`do` 的动作声明）。记账变换
/// 今天没有声明位，按 `inherit` 处理，与步 15a 之前逐字节相同。
fn out_taint(rule: TaintRule, inherited: Taint, declared: Option<TaintOut>) -> Taint {
    match rule {
        TaintRule::Trusted => Taint::Trusted,
        TaintRule::Inherit => inherited,
        TaintRule::Declared => match declared {
            Some(TaintOut::Trusted) => Taint::Trusted,
            Some(TaintOut::Untrusted) => Taint::Untrusted,
            Some(TaintOut::Inherit) | None => inherited,
        },
    }
}

/// 动作输出与声明形状不符时给出报文（B51-R2）：列表按项数计，非列表算 1 项；单项尺寸按规范 JSON
/// 文本（字符串取原文）的字符数计。`settled` 本版只记录，不核（静态面在步 24）。
fn shape_violation(shape: &jpp_effects::MatShape, out: &Json) -> Option<String> {
    use jpp_effects::ShapeItems;
    let items: Vec<&Json> = match out {
        Json::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    let bound = match shape.items {
        ShapeItems::One => Some(1),
        ShapeItems::AtMost(k) => Some(k),
        ShapeItems::Unbounded => None,
    };
    if let Some(k) = bound
        && items.len() > k
    {
        // 依据：B51-R2（20 附录 A；步 15d 运行期核对）
        return Some(format!(
            "ShapeMismatch: 声明至多 {k} 项，实际 {} 项（B51-R2）",
            items.len()
        ));
    }
    if let Some(limit) = shape.item_size {
        for (i, it) in items.iter().enumerate() {
            let n = match it {
                Json::String(t) => t.chars().count(),
                other => canon(other).chars().count(),
            };
            if n > limit {
                // 依据：B51-R2（20 附录 A；步 15d 运行期核对）
                return Some(format!(
                    "ShapeMismatch: 第 {i} 项 {n} 字符，超声明的单项上界 {limit}（B51-R2）"
                ));
            }
        }
    }
    None
}

impl<'a> Interp<'a> {
    /// 判断调用经端口发出（步 15b）：产出读数的效应的端口，输入是一个状态加一组题。
    pub(crate) fn call_judge(
        &mut self,
        state: &State,
        questions: &[&Question],
    ) -> Result<jpp_effects::JudgeResult, EffectError> {
        let effect = jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断");
        let input = CallInput::StateQuestions {
            state: state.clone(),
            questions: questions.iter().map(|q| (*q).clone()).collect(),
        };
        match self.ports.call(effect, input)? {
            EffectOut::Readings(r) => Ok(r),
            _ => Err(EffectError("判断端口返回的不是读数".into())),
        }
    }

    /// 一窗判断一次交端口（步 15e）：各组一个调用，端口内并发；结果按组的顺序。端口拒收整批时每组同错。
    pub(crate) fn call_judge_many(
        &mut self,
        窗: &[super::flush::待发],
    ) -> Vec<Result<jpp_effects::JudgeResult, EffectError>> {
        let effect = jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断");
        let inputs = 窗
            .iter()
            .map(|g| CallInput::StateQuestions {
                state: (*g.state).clone(),
                questions: g.items.iter().map(|(q, _, _)| (**q).clone()).collect(),
            })
            .collect();
        match self.ports.call_many(effect, inputs) {
            Ok(rs) => rs
                .into_iter()
                .map(|r| match r {
                    Ok(EffectOut::Readings(r)) => Ok(r),
                    Ok(_) => Err(EffectError("判断端口返回的不是读数".into())),
                    Err(e) => Err(e),
                })
                .collect(),
            Err(e) => 窗.iter().map(|_| Err(e.clone())).collect(),
        }
    }

    /// 效应内置的分派（步 15a，`20` A2 与 §2.3 `effects_exec.rs`）：只读 `EffectSpec` 的字段。
    /// 产出读数的效应走分层登记（刷新点成批发出）；其余是 `Immediate`，当场执行：触世界的走动作
    /// 登记处，输出是人的回答的走问人，不进效应行的是宿主记账变换，余下输出材料的走生成端口。
    pub(crate) fn effect_builtin(
        &mut self,
        s: &'static jpp_effects::EffectSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        use jpp_effects::{OutputShape, SchedClass};
        if s.produces_reading {
            debug_assert_eq!(s.sched, SchedClass::Layered);
            return self.b_judge(name, args, sp);
        }
        debug_assert_eq!(s.sched, SchedClass::Immediate);
        if s.side_effecting {
            self.b_do(s, name, args, sp)
        } else if s.output_shape == OutputShape::Answer {
            self.b_ask(s, name, args, sp)
        } else if !s.in_effect_row {
            self.b_transform(s, name, args, sp)
        } else {
            self.b_gen(s, name, args, sp)
        }
    }

    pub(crate) fn do_(
        &mut self,
        s: &'static EffectSpec,
        name: &str,
        args: &[Value],
        iter_seq: i64,
        sp: Span,
    ) -> R<Value> {
        // 惰性过桥（B94，审查修复 3a）：此前切出、还没检视的出口先解析，判断在这次效应之前计费，
        // 与改前 `cut` 当场刷新的先后相同（预算紧时出口种类不变）
        self.解析全部帧()?;
        let action = self.actions.actions.get(name).cloned().ok_or_else(|| {
            Fault::Error(RtError::new(
                Some("J-11"),
                {
                    let mut 表: Vec<String> = self
                        .actions
                        .actions
                        .iter()
                        .map(|(n, a)| {
                            format!(
                                "{n}（{}）",
                                if a.reversible {
                                    "可逆"
                                } else {
                                    "**不可逆**"
                                }
                            )
                        })
                        .collect();
                    表.sort();
                    // **J-08 保护的是不可逆动作，而作者此前没有任何办法知道哪些动作不可逆。**
                    // 一个作者无法查询的安全边界，等于没有边界。这里顺手把它变成可查的。
                    format!(
                        "动作 {name} 未登记：do 只能触发登记过的动作（register）。本次登记了：{}",
                        表.join("、")
                    )
                },
                sp,
            ))
        })?;
        let args_canon: Vec<String> = args.iter().map(|a| canon(&a.to_json())).collect();
        let key = self.effect_key_of(
            s.name,
            &[
                &sp.start.to_string(),
                name,
                &args_canon.join("\u{1f}"),
                &iter_seq.to_string(),
            ],
        );
        if let Some(Entry::Effect {
            output,
            output_mat,
            cost,
            ..
        }) = self.ledger.get(&key)
        {
            // 账本 v3 记下了首跑的来源边（B84、B92）；重放时再并上由实参重算的边（同一程序同一结果）
            let v = match entry_to_effect_value(output, output_mat.as_deref()) {
                Value::Mat(m) => {
                    Value::Mat(Rc::new((*m).clone().with_sources(&from_keys_of(args))))
                }
                other => other,
            };
            let cost = *cost;
            if self.audit.on {
                // B38（步 15d）：do 计一次调用与它的费用（首跑 charge(1, action.cost)，在执行前核）
                self.audit.usd += cost;
                self.audit.calls += 1;
            }
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, name.into());
            return Ok(v);
        }
        // J-08：不可逆 `do` 的唯一放行点（`20` v2 §2.3 `guard.rs::release`；步 16）
        self.release(name, action.reversible, sp)?;
        // B38（步 15d）：budget.calls 计所有效应调用，do 每次执行计一次。依据：B38（20 附录 A）
        // B93（步 22-0）：超预算不执行、不写意向，产出失败值（J-12 走 Unsure(fail)），程序照常往下
        if let Err(detail) = self.charge(1, action.cost) {
            self.记停发(1, sp, &detail);
            return Ok(预算失败值(&detail));
        }
        if self.audit.on {
            return Err(self.replay_missing(format!("do「{name}」"), sp));
        }
        // 入参 taint 要**递归看容器**：材料嵌在记录字段或嵌套列表里时，顶层 match 看不见它，
        // 以前落进 `_ => Trusted`，于是 Inherit 的动作拿到「入参全可信」——脏材料喂进 do 出来就干净了。
        // 与 J-01 的 `unwrap_or(false)`、taint 反序列化兜底、`mat(content(脏))` 同一形状。
        let taint_in = args
            .iter()
            .fold(Taint::Trusted, |t, a| Taint::join(t, taint_of(a)));
        let taint = out_taint(s.taint_rule, taint_in, Some(action.taint_out));
        // B51-R2（步 15d）：声明了输出形状的动作，返回后核基数与单项尺寸；违反即失败值
        let result = (action.f)(args).and_then(|v| match &action.mat_shape {
            Some(shape) => shape_violation(shape, &v.to_json()).map_or(Ok(v), Err),
            None => Ok(v),
        });
        let out = match result {
            // `derived_from` 与 taint 一样要**折算**，不是恒清零：`do(…, [m])` 的产物
            // 当然仍派生自 m 那道题。**这是保持同一跳，不是增加一跳**（`12` J-02 范围裁定）。
            // 闭包在 `as_mat(exit)` 那里自然截断——做成传递闭包会重演「逐字传播让几乎所有
            // 输出不可用」（宪法第 44 行），材料越传越「派生自所有题」，J-02 最后拦住一切。
            Ok(v) => Value::Mat(Rc::new(
                Mat::new(
                    v.to_json(),
                    &format!("do:{name}"),
                    vec![format!("do:{key}")],
                    taint,
                    derived_of(args),
                )
                .with_sources(&from_keys_of(args)),
            )),
            Err(msg) => {
                // 失败信息同样来自外面：Fail 带动作的输出位，`text(f)` 经 ∨ 输入带出去
                let m = format!("{name}: {msg}");
                // B84：失败值的来源 = 实参的来源（与输出材料同一条边）
                Value::Fail(
                    Rc::from(m.as_str()),
                    Provenance::new(taint, from_keys_of(args)),
                )
            }
        };
        self.cost.usd += action.cost;
        self.cost.calls += 1;
        let (output, output_mat) = effect_value_to_entry(&out);
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: output_mat.map(Box::new),
            kind: s.name.into(),
            output,
            cost: action.cost,
        });
        self.trace
            .push(s.name, &key, false, action.cost, sp, name.into());
        Ok(out)
    }

    pub(crate) fn generate(
        &mut self,
        s: &'static EffectSpec,
        prompt: &str,
        ctx: &[Mat],
        n: usize,
        retry_seq: i64,
        sp: Span,
    ) -> R<Value> {
        // 审查修复 3a：同 `do_`
        self.解析全部帧()?;
        let ctx_hash: Vec<&str> = ctx.iter().map(|m| m.hash.as_str()).collect();
        // 12:158「键：(site, prompt_hash, ctx_hash, n, retry_seq)」——site 排第一位。
        // 缺了它，同一段 prompt 在两个站点生成会撞键，第二个站点命中第一个的输出。
        // gen 比 judge 更容易撞：prompt 常是字面量，两处写同一句话很正常。
        let key = self.effect_key_of(
            s.name,
            &[
                &sp.start.to_string(),
                prompt,
                &ctx_hash.join(","),
                &n.to_string(),
                &retry_seq.to_string(),
            ],
        );
        let taint = ctx
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        let taint = out_taint(s.taint_rule, taint, None);
        // 与 do 同：并入 ctx 各材料的 derived_from（保一跳）
        let derived: BTreeSet<String> = ctx
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：来源出口键同样承接 ctx
        // B92（步 18c）：承接 ctx 的来源边，种类不变
        let from = ctx
            .iter()
            .fold(Sources::empty(), |s, m| s.union(&m.prov().sources));
        let wrap = |outs: &[Json]| {
            Value::list(
                outs.iter()
                    .map(|o| {
                        Value::Mat(Rc::new(
                            Mat::new(
                                o.clone(),
                                &format!("gen:{prompt}"),
                                vec![format!("gen:{key}")],
                                taint,
                                derived.clone(),
                            )
                            .with_sources(&from),
                        ))
                    })
                    .collect(),
            )
        };
        if let Some(Entry::Effect { output, cost, .. }) = self.ledger.get(&key) {
            let outs: Vec<Json> = output.as_array().cloned().unwrap_or_default();
            let cost = *cost;
            self.audit_account(0, cost, sp);
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, prompt.into());
            return Ok(wrap(&outs));
        }
        // B93（步 22-0）：超预算不发，产出失败值（J-12），程序照常往下
        if let Err(detail) = self.charge(1, 0.0) {
            self.记停发(1, sp, &detail);
            return Ok(预算失败值(&detail));
        }
        if self.audit.on {
            return Err(self.replay_missing(format!("gen「{prompt}」"), sp));
        }
        let ctx_json: Vec<Json> = ctx.iter().map(|m| m.content.clone()).collect();
        let input = CallInput::Prompt {
            prompt: prompt.to_string(),
            ctx: ctx_json,
            n,
            retry_seq: retry_seq as u64,
        };
        let res = match self.ports.call(s.id, input) {
            Ok(EffectOut::Mats(r)) => Ok(r),
            Ok(_) => Err(EffectError("生成端口返回的不是材料".into())),
            Err(e) => Err(e),
        }
        .map_err(|e| {
            Fault::Error(RtError::new(
                Some("E-rt-client"),
                format!("gen 失败：{}", e.0),
                sp,
            ))
        })?;
        // 13 §5：后端已经返回 = 钱已经花了。先记事实（费用、token、账本），再核预算决定下一步。
        self.cost.calls += 1;
        self.cost.tokens += res.tokens;
        self.cost.usd += res.cost;
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: s.name.into(),
            output: Json::Array(res.outputs.clone()),
            cost: res.cost,
        });
        self.trace
            .push(s.name, &key, false, res.cost, sp, prompt.into());
        // 实际费用超出时停的是下一步（下一次发出前的核对，B93）
        Ok(wrap(&res.outputs))
    }

    pub(crate) fn ask(
        &mut self,
        s: &'static EffectSpec,
        state: &Rc<State>,
        q: &Rc<Question>,
        sp: Span,
    ) -> R<Value> {
        // 审查修复 3a：同 `do_`
        self.解析全部帧()?;
        let key = self.effect_key_of(s.name, &[&state.hash, &q.hash]);
        // 已答的照答；已问未答的：重放照记的给出（Pending），续跑再问一次
        let recorded = match self.ledger.get(&key) {
            Some(Entry::Ask {
                answer: Some(a), ..
            }) => Some(Some(a.clone())),
            Some(Entry::Ask { answer: None, .. }) if self.audit.on => Some(None),
            _ => None,
        };
        // B38（步 15d）：只凭账本重放时，记过的 ask 照记录计入调用（首跑在哪里停，重放就在哪里停）
        if recorded.is_some() && self.audit.on {
            self.audit.calls += 1;
        }
        let answer = if let Some(answer) = recorded {
            answer
        } else {
            let limit = self.budget.escalate.unwrap_or(0);
            // 已问过的（账本里的）+ 这次运行新问的，一起核总上限
            if self.asks_in_ledger + self.cost.asks >= limit {
                return Err(Fault::Halt(Pending {
                    cause: "budget.escalate".into(),
                    key,
                    site: sp,
                    detail: format!("ask 次数已到上限 {limit}"),
                }));
            }
            if self.audit.on {
                return Err(self.replay_missing(format!("ask「{}」", q.text), sp));
            }
            // B38（步 15d）：ask 也计入 budget.calls；它同时受 budget.escalate 约束，两道限制并存
            // `ask` 超 calls 预算仍挂起（B93：Pending 只由 ask 产生）
            if let Err(detail) = self.charge(1, 0.0) {
                return Err(Fault::Halt(Pending {
                    cause: "budget".into(),
                    key,
                    site: sp,
                    detail,
                }));
            }
            self.cost.asks += 1;
            self.cost.calls += 1;
            let input = CallInput::StateQuestion {
                state: (**state).clone(),
                question: (**q).clone(),
            };
            let a = match self.ports.call(s.id, input) {
                Ok(EffectOut::Answer(a)) => Ok(a),
                Ok(_) => Err(EffectError("问人端口返回的不是回答".into())),
                Err(e) => Err(e),
            }
            .map_err(|e| {
                Fault::Error(RtError::new(
                    Some("E-rt-client"),
                    format!("ask 失败：{}", e.0),
                    sp,
                ))
            })?;
            // 已问未答也入账（步 7）：重放照样以 Pending 结束；续跑得到答案时另起一条（只增）
            self.ledger.put_answer(Entry::Ask {
                key: key.clone(),
                ekey: self.effect_keys.get(&key).cloned(),
                answer: a.clone(),
            });
            a
        };
        match answer {
            Some(a) => {
                self.trace
                    .push(s.name, &key, false, 0.0, sp, format!("「{}」已答", q.text));
                let kind = match a {
                    Answer::Noul(p) => {
                        if p >= 0.5 {
                            ExitKind::Act
                        } else {
                            ExitKind::Ignore
                        }
                    }
                    Answer::Choice(v) => ExitKind::Pick(argmax(&v).0),
                    Answer::Score(v) => ExitKind::At(argmax(&v).0),
                };
                let e = self.new_exit(
                    kind,
                    None,
                    q.op,
                    &q.hash,
                    &state.hash,
                    out_taint(s.taint_rule, Taint::Trusted, None),
                    sp,
                );
                if let Value::Exit(x) = &e {
                    x.from_ask.set(true); // 12:265「或经 ask」；人答是 trusted（§2.11）
                }
                Ok(e)
            }
            None => {
                self.trace.push(
                    s.name,
                    &key,
                    false,
                    0.0,
                    sp,
                    format!("「{}」未答 → Pending", q.text),
                );
                Err(Fault::Halt(Pending {
                    cause: s.name.into(),
                    key,
                    site: sp,
                    detail: format!("等人回答「{}」", q.text),
                }))
            }
        }
    }

    pub(crate) fn transform(
        &mut self,
        s: &'static EffectSpec,
        f: &Rc<Closure>,
        args: &[Value],
        sp: Span,
    ) -> R<Value> {
        let mut mats = vec![];
        for a in args {
            mats.push(self.as_mat(a, s.name, sp)?);
        }
        let hashes: Vec<&str> = mats.iter().map(|m| m.hash.as_str()).collect();
        // 13 §4：可复用结果的身份不能只取代码正文——工厂造出来的两个方法正文相同、捕获不同时，
        // 只按正文就会把前一个的结果复用给后一个（**算错**）。身份 = 代码哈希 + 实际捕获状态的指纹。
        // 指纹取不到（捕获里有读数/出口/嵌套太深）时**禁用这一项的跨运行缓存**，照常执行；
        // 宁可不缓存，也不返回另一个方法的结果。
        let captured = self.env_fingerprint(&f.env, &referenced_names(&f.function), 3);
        let taint = mats
            .iter()
            .fold(Taint::Trusted, |t, m| Taint::join(t, m.taint));
        let taint = out_taint(s.taint_rule, taint, None);
        // 保一跳：`transform(f, m)` 的产物仍派生自 m 那道题。此前恒清零——
        // 一次恒等变换 `transform(fn(x){content(x)}, m)` 就洗掉 J-02 的禁自指。
        let derived: BTreeSet<String> = mats
            .iter()
            .flat_map(|m| m.derived_from.iter().cloned())
            .collect();
        // B59（步 17a）：变换的产物承接输入材料的来源出口键
        // B92（步 18c）：承接输入材料的来源边，种类不变
        let from = mats
            .iter()
            .fold(Sources::empty(), |s, m| s.union(&m.prov().sources));
        let Some(captured) = captured else {
            // **这句原来只说「不进跨运行缓存」，而它少说了一半**：这条路
            // **在 `ledger.put` 之前就返回了**，于是这份材料**根本不进账本**——
            // 而账本是今天唯一存着效应输出内容的地方。`12`:182 说「`gen`/`do`/`transform`
            // 的输出**默认入库**」，**对这条路是假的**，跨会话也就取不回来。
            self.trace.warn(format!(
                "W-no-cache: transform 的方法捕获环境指纹化不了（{}），这一项不进跨运行缓存；照常执行，不复用别人的结果。**它也不进账本**——账本是今天唯一存着效应输出内容的地方，所以这份材料跨会话取不回来（12:182「输出默认入库」对这条路不成立）",
                f.name.clone().unwrap_or_else(|| "匿名方法".into())
            ));
            let v = self.call_closure(
                f,
                mats.iter()
                    .map(|m| Value::Mat(Rc::new(m.clone())))
                    .collect(),
                sp,
            )?;
            // 惰性出口先解析（B94，审查修复 2）：闭包返回的出口要照改前报 J-11
            let v = self.检视(v)?;
            if matches!(
                v,
                Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)
            ) {
                return err(
                    Some("J-11"),
                    format!("transform 的输出要能成材料，收到 {}", v.type_name()),
                    sp,
                );
            }
            let content = match &v {
                Value::Mat(m) => m.content.clone(),
                other => other.to_json(),
            };
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    content,
                    s.name,
                    vec!["transform:uncached".to_string()],
                    taint,
                    derived,
                )
                .with_sources(&from),
            )));
        };
        // 12:189「键 (site, f_hash, args_hash)」。`captured` 是 13 §4 另加的（身份含捕获状态）。
        let key = self.effect_key_of(
            s.name,
            &[&sp.start.to_string(), &f.hash, &captured, &hashes.join(",")],
        );
        if let Some(Entry::Effect { output, .. }) = self.ledger.get(&key) {
            self.cost.replayed += 1;
            self.trace.push(s.name, &key, true, 0.0, sp, String::new());
            return Ok(Value::Mat(Rc::new(
                Mat::new(
                    output.clone(),
                    s.name,
                    vec![format!("transform:{key}")],
                    taint,
                    derived,
                )
                .with_sources(&from),
            )));
        }
        let v = self.call_closure(
            f,
            mats.iter()
                .map(|m| Value::Mat(Rc::new(m.clone())))
                .collect(),
            sp,
        )?;
        // 惰性出口先解析（B94，审查修复 2）：闭包返回的出口要照改前报 J-11
        let v = self.检视(v)?;
        if matches!(
            v,
            Value::Reading(_) | Value::Exit(_) | Value::Fn(_) | Value::State(_)
        ) {
            return err(
                Some("J-11"),
                format!("transform 的输出要能成材料，收到 {}", v.type_name()),
                sp,
            );
        }
        let content = match &v {
            Value::Mat(m) => m.content.clone(),
            other => other.to_json(),
        };
        self.ledger.put(Entry::Effect {
            key: key.clone(),
            ekey: self.effect_keys.get(&key).cloned(),
            output_mat: None,
            kind: s.name.into(),
            output: content.clone(),
            cost: 0.0,
        });
        self.trace.push(s.name, &key, false, 0.0, sp, String::new());
        Ok(Value::Mat(Rc::new(
            Mat::new(
                content,
                s.name,
                vec![format!("transform:{key}")],
                taint,
                derived,
            )
            .with_sources(&from),
        )))
    }
}

/// 预算停发的效应产出的失败值（B93，`12` J-12）：`budget: <报文>`，trusted（程序自己的记账，不来自外面）
fn 预算失败值(detail: &str) -> Value {
    Value::Fail(
        Rc::from(format!("budget: {detail}").as_str()),
        Provenance::trusted(),
    )
}
