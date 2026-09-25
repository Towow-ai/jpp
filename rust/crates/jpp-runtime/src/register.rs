//! 效应登记：`judge` 登记、提前登记（lift、speculate、vectorize）、窗口检查（20 §2.3 `register.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::plan_view::RtEnv;
use super::*;
use jpp_ir::plan::Reach;

impl<'a> Interp<'a> {
    /// 判断键：算出账本键并记下结构化键，写账本时附上（账本 v2，步 7）。键值与 `judge_key` 相同。
    pub(crate) fn judge_key_of(
        &mut self,
        state_hash: &str,
        q_hash: &str,
        phys: &str,
        site: usize,
    ) -> String {
        let k = JudgeKey::new(
            &self.model_id,
            state_hash,
            q_hash,
            phys,
            0,
            self.run_seq,
            site,
        );
        let d = k.digest();
        self.judge_keys.insert(d.clone(), k);
        d
    }

    /// 效应键：算出账本键并记下结构化键（账本 v2，步 7）。键值与 `effect_key` 相同。
    pub(crate) fn effect_key_of(&mut self, kind: &str, parts: &[&str]) -> String {
        let k = EffectKey::new(kind, parts);
        let d = k.digest();
        self.effect_keys.insert(d.clone(), k);
        d
    }

    pub(crate) fn judge(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        sp: Span,
    ) -> R<Vec<Value>> {
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return err(
                    Some("J-02"),
                    format!("禁自指：状态含由题「{}」派生的材料，不能再问同一题", q.text),
                    sp,
                );
            }
        }
        if state.has_fail {
            let rs: Vec<Rc<Reading>> = qs
                .iter()
                .map(|q| {
                    Rc::new(Reading {
                        q_hash: q.hash.clone(),
                        state_hash: state.hash.clone(),
                        op: q.op,
                        calib: q.calib.clone(),
                        id: self.new_reading_id(),
                        fail: Some("状态含 Fail 材料".into()),
                        model_id: self.model_id.clone(),
                        ledger_key: String::new(),
                        over_len: state.over.len(),
                        scale: q.scale.clone(),
                        perms: std::cell::Cell::new(0),
                        mode_share: std::cell::Cell::new(None),
                        missing_evidence: missing_evidence(state, q),
                        state_taint: Taint::join(state.taint, q.taint), // B58：读数 taint = 状态 ∨ 题
                        form_hash: q.form_hash.clone(),
                        fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
                    })
                })
                .collect();
            self.note_kinds(state, qs, &rs);
            return Ok(rs.into_iter().map(Value::Reading).collect());
        }
        let keys: Vec<String> = qs
            .iter()
            .map(|q| self.judge_key_of(&state.hash, &q.hash, q.op.phys(), sp.start))
            .collect();
        // 循环内键重复即停（J-06）
        if let Some(lc) = self.loops.last_mut() {
            for k in &keys {
                if !lc.seen_keys.insert(k.clone()) && lc.repeated.is_none() {
                    lc.repeated = Some(k.clone());
                }
            }
        }
        // 12 §2.2「**惰性**：登记后不发」。账本命中的当场填上（重放不花钱、也不必推迟）；
        // 缺的登记进 `pending`，等一个**刷新点**（`cut` / `if` / 程序结束）按状态分组一层发出。
        let readings: Vec<Rc<Reading>> = qs
            .iter()
            .zip(&keys)
            .map(|(q, k)| {
                Rc::new(Reading {
                    q_hash: q.hash.clone(),
                    state_hash: state.hash.clone(),
                    op: q.op,
                    calib: q.calib.clone(),
                    id: self.new_reading_id(),
                    fail: None,
                    model_id: self.model_id.clone(),
                    ledger_key: k.clone(),
                    over_len: state.over.len(),
                    scale: q.scale.clone(),
                    perms: std::cell::Cell::new(0),
                    mode_share: std::cell::Cell::new(None),
                    missing_evidence: missing_evidence(state, q),
                    state_taint: Taint::join(state.taint, q.taint), // B58：读数 taint = 状态 ∨ 题
                    form_hash: q.form_hash.clone(),
                    fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
                })
            })
            .collect();
        self.note_kinds(state, qs, &readings);
        let mut missing = vec![];
        for (i, k) in keys.iter().enumerate() {
            // 真站点走到了一个推测过的键：这次推测用上了
            if self.speculated.contains(k) {
                self.speculation_used.insert(k.clone());
            }
            if let Some(Entry::Judge {
                answer,
                cost,
                call,
                perm,
                ..
            }) = self.ledger.get(k)
            {
                let (answer, cost, call, perm) = (answer.clone(), *cost, *call, *perm);
                // 账本命中当场填：写答案只经 flush.rs 的 fill_answer（grep_fill 核）；
                // 置换测量随答案一起取回，否则 K 选一出口在重放处变成 `untested`
                self.fill_from_record(&readings[i], answer, perm);
                self.audit_account(call, cost, sp);
                self.cost.replayed += 1;
                self.trace
                    .push("judge", k, true, 0.0, sp, format!("「{}」", qs[i].text));
            } else if qs[i].op == Op::Select && state.over.is_empty() {
                // **没有候选**（B3）：K 选一的 over 槽是空的，问了也没有可选的——不发，出口 Unsure(no_candidate)，
                // 去向是调生成器补候选，不同于 tie / insufficient。
                self.absent_marks.insert(k.clone(), "no_candidate".into());
                self.trace.warn(format!("W-no-candidate: @{} 选择题「{}」没有候选（over 为空），出口 Unsure(no_candidate)", sp.start, qs[i].text));
            } else {
                missing.push(i);
            }
        }
        // 窗口检查（J-14 / 12:117）：对象槽内单段按 text_slots，槽间按 json_slots。
        // 超窗不报错只留痕——它不是算错，是**读数被语境接管而无人察觉**
        // （档案：「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」）。
        // 与 Python `_check_window` 同为 warn。
        self.check_window(state, sp);
        // J-14 运行期面（B62/I-10，步 24b）：单个 Mat 的 JSON 内容本身在顶层塞了一个多元素
        // 列表、题面又像是按编号或键引用其中之一——检查器看不见这种写法，静态面只查 `on` 实参
        // 个数。warn 级：不阻塞，只是「读数可能被语境接管（H8 串扰）而无人察觉」的提示，与上面
        // 窗口检查同一性质。
        self.check_multi_object(state, qs, sp);
        if !missing.is_empty() {
            self.pending.push(PendingJudge {
                state: state.clone(),
                items: missing
                    .iter()
                    .map(|i| (qs[*i].clone(), readings[*i].clone(), keys[*i].clone()))
                    .collect(),
                site: sp,
                speculative: false,
            });
        }
        Ok(readings.into_iter().map(Value::Reading).collect())
    }

    /// 记下每个读数的精化题类（B76，步 12e-2）：题 × 状态槽形。
    pub(crate) fn note_kinds(
        &mut self,
        state: &State,
        qs: &[Rc<Question>],
        readings: &[Rc<Reading>],
    ) {
        let shape = state.slot_shape();
        for (q, r) in qs.iter().zip(readings) {
            let kind = q.kind_on(&shape);
            self.reading_kinds.insert(r.id, kind);
            // B107、B120 (a)（步 20h-2）：报告 `questions` 表，每个不同题哈希一行，按首次登记的顺序
            if !self.questions.iter().any(|x| x["q"] == q.hash.as_str()) {
                let mut row = serde_json::json!({"q": q.hash, "kind": kind});
                if let Some(h) = &q.form_hash {
                    row["form_hash"] = serde_json::json!(h);
                }
                if let Some(t) = &q.template {
                    row["template"] = serde_json::json!(t);
                }
                if let Some(f) = &q.fill {
                    row["fill"] = serde_json::Value::Object(
                        f.iter()
                            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                            .collect(),
                    );
                }
                self.questions.push(row);
            }
        }
    }

    /// 读数的精化题类（B76）；合成读数（`fit`、`repeat`）没有记录，为 `None`。
    #[allow(dead_code)]
    pub(crate) fn reading_kind(&self, r: &Reading) -> Option<QuestionKind> {
        self.reading_kinds.get(&r.id).copied()
    }

    /// 提升（pass `lift`）的执行一半：把后面同状态、可安全提前登记的 `judge` 一起登记上来。
    ///
    /// 哪些语句可提、停在哪里由 `jpp-plan` 判（步 13a 从这里搬走，`jpp_plan::passes::lift`）：
    /// 计划给出逐句的提升步，按环境才判得出的两问（越过的这一句会不会触世界，K-075；被提的这一句
    /// 能不能提前求值，K-069）逐句问钩子，因为前面被提的句子会改变环境。这里只求值与绑定。
    pub(crate) fn lift_followers(
        &mut self,
        b: &Block,
        at: jpp_ir::key::NodeId,
        env: &Env,
        lifted: &mut HashSet<usize>,
    ) -> R<()> {
        let Some(lp) = self.plan.lifts.get(&at).cloned() else {
            return Ok(());
        };
        for step in &lp.steps {
            let Stmt::Let { name, value, .. } = &b.statements[step.index] else {
                break;
            };
            let view = RtEnv(env.clone());
            if self.hooks.may_effect(value, &view, Reach::World) {
                break;
            }
            if step.lift {
                if self.hooks.may_effect(value, &view, Reach::Strict) {
                    break;
                }
                let v = self.eval(value, env)?;
                env_define(env, name, v);
                lifted.insert(step.index);
            }
            if step.stop_after {
                break;
            }
        }
        Ok(())
    }

    /// 推测（pass `speculate`）的执行一半：从触发点 `at`（本块一条 `let` 的值表达式）起，
    /// 把 `if` 两侧分支体里此刻已能求值的站点推测登记。候选与许可由 `jpp-plan` 给（计划 + 钩子）。
    pub(crate) fn speculate_ahead(&mut self, b: &Block, at: jpp_ir::key::NodeId, env: &Env) {
        let view = RtEnv(env.clone());
        let judges = self.hooks.speculate(&self.plan, at, b, &view);
        for j in judges {
            self.speculate_judge(j, env);
        }
    }

    /// **循环向量化**（宪法登记表第 47 行，pass `vectorize`）的执行一半：把 `map`/`filter`
    /// **后续各轮**的 `judge` 站点提前登记进本层，这样体内有 `cut` 时不会一轮一层。
    ///
    /// **实测它只在一个形状上有余量**（四个形状各跑一遍）：
    /// | 形状 | 接之前 | 可省 |
    /// |---|---|---|
    /// | 异状态·无 `cut` | calls 3 / layers 1 | **无**——惰性已经把三轮并进一层 |
    /// | 同状态·无 `cut` | calls 1 / layers 1 | **无**——`fuse` 已经合成一次调用 |
    /// | **异状态·带 `cut`** | **calls 3 / layers 3** | **layers 3 → 1** |
    /// | 同状态·带 `cut` | calls 1 / layers 1 | **无**——后两轮同键，账本直接重放 |
    ///
    /// 每一轮：绑好形参，经钩子 `instantiate` 取这一轮可提前登记的站点（候选按函数体静态给出，
    /// 许可按这一轮的环境判，`jpp_plan::passes::vectorize`），再逐个求值登记。
    pub(crate) fn vectorize_ahead(&mut self, f: &Value, items: &[Value], _sp: Span) {
        if !self.plan.vectorize {
            return;
        }
        let Value::Fn(c) = f else { return };
        if c.function.parameters.len() != 1 {
            return;
        }
        // 第 0 轮也提前登记（步 13b-1）：刷新按登记先后分组发出，第 0 轮排在最前，各轮按程序顺序发；
        // 预算停发（B93）因此落在末尾的轮次上。第 0 轮随后的真站点命中同一键，刷新同键去重，不多花调用
        for it in items.iter() {
            let env = env_child(&c.env);
            env_define(&env, &c.function.parameters[0].name, it.clone());
            let view = RtEnv(env.clone());
            let targets = self.hooks.instantiate(&self.plan, &c.function, &view);
            self.run_targets(&c.function, &targets, &env);
        }
    }

    /// 执行钩子给的目标（步 13b）：`Site` 在当前环境里求状态与题并登记；`Enter` 求被调者与实参
    /// （钩子已核：实参只有名字或字面量、按环境不会产生效应），在被调者的捕获环境上绑好形参再往里执行。
    /// 这里只求值与登记，不判许可（`20` T3）；求不出来就放弃这一支，不报错（与推测同一口径）。
    fn run_targets(&mut self, f: &Function, targets: &[jpp_ir::plan::Target], env: &Env) {
        use jpp_ir::plan::Target;
        for t in targets {
            match t {
                Target::Site(id) => {
                    if let Some(e) = jpp_ir::ir::find_expr(&f.body, *id) {
                        self.speculate_judge(e, env);
                    }
                }
                Target::Enter { call, inner } => {
                    let Some(e) = jpp_ir::ir::find_expr(&f.body, *call) else {
                        continue;
                    };
                    let K::Call { callee, args } = kind(e) else {
                        continue;
                    };
                    let Some(Value::Fn(c2)) = callee.name().and_then(|n| env_lookup(env, n)) else {
                        continue;
                    };
                    let mut vals = vec![];
                    for a in &args {
                        match self.eval(a, env) {
                            Ok(v) => vals.push(v),
                            Err(_) => break,
                        }
                    }
                    if vals.len() != c2.function.parameters.len() {
                        continue;
                    }
                    let env2 = env_child(&c2.env);
                    for (p, v) in c2.function.parameters.iter().zip(vals) {
                        env_define(&env2, &p.name, v);
                    }
                    let c2 = c2.clone();
                    self.run_targets(&c2.function, inner, &env2);
                }
            }
        }
    }

    /// 把一个许可过的 `judge` 站点提前登记：在当前环境里求出状态与题，求不出就放弃这个站点（不报错）。
    ///
    /// **搬过去的站点，来源还是原来那份**：状态由 `make_state` 在**当前环境**里算出来，
    /// taint 与 `derived_from` 都跟着材料走——推测不新建材料，所以没有新边界。
    /// **这里搬的是站点不是值，值仍在原环境里算**。
    pub(crate) fn speculate_judge(&mut self, e: &Expr, env: &Env) {
        let K::Call {
            args: arguments, ..
        } = kind(e)
        else {
            return;
        };
        if let (Ok(Value::State(st)), Ok(q)) =
            (self.eval(arguments[0], env), self.eval(arguments[1], env))
        {
            let qs: Vec<Rc<Question>> = match q {
                Value::Question(q) => vec![q],
                Value::List(l) => l
                    .iter()
                    .filter_map(|x| {
                        if let Value::Question(q) = x {
                            Some(q.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => return,
            };
            if !qs.is_empty() {
                let _ = self.register_speculative(&st, &qs, e.span);
            }
        }
    }

    /// 登记一个推测站点：与真站点同一套键（`judge_key`），所以真站点走到时直接命中账本。
    pub(crate) fn register_speculative(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        sp: Span,
    ) -> Option<()> {
        if state.has_fail {
            return None;
        }
        for q in qs {
            if state.derived_from.contains(&q.hash) {
                return None; // 禁自指的站点不推
            }
        }
        let keys: Vec<String> = qs
            .iter()
            .map(|q| self.judge_key_of(&state.hash, &q.hash, q.op.phys(), sp.start))
            .collect();
        let mut items = vec![];
        for (q, k) in qs.iter().zip(&keys) {
            // 账本里已经有 = 不用推
            if self.ledger.get(k).is_some() {
                continue;
            }
            // 这一层已经登记过同一个键 = 不重复推
            if self
                .pending
                .iter()
                .any(|p| p.items.iter().any(|(_, _, kk)| kk == k))
            {
                continue;
            }
            let r = Rc::new(Reading {
                q_hash: q.hash.clone(),
                state_hash: state.hash.clone(),
                op: q.op,
                calib: q.calib.clone(),
                id: self.new_reading_id(),
                fail: None,
                model_id: self.model_id.clone(),
                ledger_key: k.clone(),
                over_len: state.over.len(),
                perms: std::cell::Cell::new(0),
                mode_share: std::cell::Cell::new(None),
                missing_evidence: missing_evidence(state, q),
                scale: q.scale.clone(),
                state_taint: Taint::join(state.taint, q.taint), // B58：读数 taint = 状态 ∨ 题
                form_hash: q.form_hash.clone(),
                fp: Some(jpp_value::stat::material_fingerprint(&state.on_text())),
            });
            items.push((q.clone(), r, k.clone()));
        }
        if items.is_empty() {
            return None;
        }
        for (_, _, k) in &items {
            self.speculated.insert(k.clone());
        }
        {
            let (qs2, rs2): (Vec<Rc<Question>>, Vec<Rc<Reading>>) =
                items.iter().map(|(q, r, _)| (q.clone(), r.clone())).unzip();
            self.note_kinds(state, &qs2, &rs2);
        }
        self.pending.push(PendingJudge {
            state: state.clone(),
            items,
            site: sp,
            speculative: true,
        });
        Some(())
    }

    /// 窗口检查（J-14 / `12`:117）。静态判不了大小，所以在**登记时**查。
    pub(crate) fn check_window(&mut self, state: &State, sp: Span) {
        // 窗口未测（步 15d，`20` §3.9 窗口行）：不核对象长度，每站点报一次 `W-window-untested`；
        // 裂变到每次一个对象属步 23b，建成前只报。依据：21 步 15d；20 §3.9
        let Some(w) = self.calib.profile().window() else {
            if self.unknown_reported.insert(format!("window@{}", sp.start)) {
                self.trace.warn(format!(
                    "W-window-untested: @{} 画像没有测过窗口（window），对象长度不核；超窗的语境可能接管读数而无痕",
                    sp.start
                ));
            }
            return;
        };
        // 对象槽内单段：每一段各自比，不是求和——依据说的是「单段材料」
        for m in &state.on {
            let t = m.tokens();
            if t > w.text {
                self.trace.warn(format!(
                    "W-window: @{} 对象槽内单段 {t} token 超已测窗口 {}（超窗的语境会接管读数，答案可能偏而无痕）",
                    sp.start, w.text
                ));
            }
        }
        // 槽间干扰：ctx 与 ref 求和
        let ctx: usize = state
            .ctx
            .iter()
            .chain(&state.r#ref)
            .map(|m| m.tokens())
            .sum();
        if ctx > w.json_ctx {
            self.trace.warn(format!(
                "W-window: @{} 语境槽 {ctx} token 超 JSON 槽已测窗口 {}（上限未测，超出即无依据）",
                sp.start, w.json_ctx
            ));
        }
    }

    /// J-14 运行期面（B62/I-10，步 24b）：`on` 槽某个材料的 JSON 顶层含长度大于 1 的列表，
    /// 且这次问的某道题的题面像是按编号或键引用其中的元素。静态面只查 `on` 实参个数（`jpp-check`），
    /// `mat({task, blocks})` 这种把列表塞进一个 `Mat` 的写法检查器看不见，只能在这里核实际内容。
    /// warn 级、不阻塞：H8（`multi_object_crosstalk`）未测按 `true`，但拦下会让原本合法的单对象
    /// JSON 材料受累，所以这条诊断从不改变出口，只留痕。
    pub(crate) fn check_multi_object(&mut self, state: &State, qs: &[Rc<Question>], sp: Span) {
        for m in &state.on {
            let Some(list) = top_level_list(&m.content) else {
                continue;
            };
            if list.len() <= 1 {
                continue;
            }
            let keys: std::collections::BTreeSet<&str> = list
                .iter()
                .filter_map(|v| v.as_object())
                .flat_map(|o| o.keys().map(|k| k.as_str()))
                .collect();
            for q in qs {
                if !references_element(&q.text, list.len(), &keys) {
                    continue;
                }
                let dedup = format!("multi_object@{}:{}", sp.start, m.hash);
                if self.unknown_reported.insert(dedup) {
                    self.trace.warn(format!(
                        "W-multi-object: @{} 材料的 JSON 顶层含 {} 个元素的列表，题面「{}」像是按编号或键引用其中之一：同状态多对象逐对象判存在串扰（H8，`multi_object_crosstalk`），线也不适用于这种材料分布（`12` §2.1、I-10）。修法：逐对象各建一个状态，用 judge([state(块, {{ctx: 任务}})…], 题) 或 sieve(chunks, 题式, 填法)",
                        sp.start,
                        list.len(),
                        q.text
                    ));
                }
            }
        }
    }
}

/// 一份 JSON 的顶层是否含一个列表：内容本身就是数组，或内容是对象且某个顶层字段的值是数组。
/// 只看**顶层**（B62/I-10 原文），不递归找更深的列表。
fn top_level_list(v: &Json) -> Option<&Vec<Json>> {
    match v {
        Json::Array(a) => Some(a),
        Json::Object(o) => o.values().find_map(|x| x.as_array()),
        _ => None,
    }
}

/// 题面文字是否像是按编号或键引用列表里的一个元素：文字里有一个数字串、解析后落在
/// `[0, len]`（同时覆盖 0-based 与 1-based 两种编号习惯）；或文字逐字包含元素对象的某个顶层
/// 字段名。两者任一命中即算引用（保守方向：只增不减，避免漏报常见写法）。
fn references_element(text: &str, len: usize, keys: &std::collections::BTreeSet<&str>) -> bool {
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once('\u{0}')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            if digits.parse::<usize>().is_ok_and(|n| n <= len) {
                return true;
            }
            digits.clear();
        }
    }
    keys.iter().any(|k| !k.is_empty() && text.contains(k))
}
