//! 责任：`handle` 与未决分支的处置（20 §2.3 `duty.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    pub(crate) fn handle(&mut self, e: &Rc<Exit>, arms: &Value, sp: Span) -> R<Value> {
        let Value::Record(_) = arms else {
            return err(
                Some("J-05"),
                "handle 的第二个参数是记录 {act, ignore, pick, at, unsure, otherwise}",
                sp,
            );
        };
        let required: &[&str] = match e.op {
            Op::Test => &["act", "ignore", "unsure"],
            Op::Select => &["pick", "unsure"],
            Op::Measure => &["at", "unsure"],
        };
        let has_other = arms.get("otherwise").is_some();
        // 语法覆盖约束：Unsure 必须有显式的 unsure 臂，通配分支不能代替它。
        // 其余去向可以由 otherwise 兜底。
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|k| arms.get(k).is_none() && (*k == "unsure" || !has_other))
            .collect();
        if !missing.is_empty() {
            // 步 20j-3：`stat` 线的出口按 `cut` 的选项取臂（B153 (1)），`select` 读数上也可能是 test 型，报文照实说
            let 出处 = match self
                .exit_rows
                .get(&e.id)
                .and_then(|i| self.exit_grades.get(*i))
                .and_then(|row| row.get("stat"))
            {
                Some(s) => format!(
                    "stat={s} 线的出口是 {} 型（按 cut 的选项取臂：{{hi, lo}} 出 act / ignore / unsure，cuts 出 at / unsure，B153），",
                    if e.op == Op::Measure { "at" } else { "test" }
                ),
                None => format!("{} 题的出口", e.op.phys()),
            };
            return err(
                Some("J-05"),
                format!(
                    "handle 不穷尽：{出处}缺分支 {}。修法：补上；注意 unsure 必须自己写一臂，otherwise 兜不住未决责任",
                    missing.join(", ")
                ),
                sp,
            );
        }
        // J-08 的守卫证据只在这里产生（B121-1：`handle` 是出口消去为值的唯一形式）。
        // 同一个值两处用：压上所选臂的守卫栈（臂里的 `do`），并进臂返回值的 `Bool` 叶子
        // （`let ok = handle(…); if ok { do }`）。未决出口恒空（B121-2）。
        // 谱系放行（B72-4，步 17b）：未决出口本身无证据，不必查谱系
        let 谱系 = if e.is_unsure() { None } else { self.谱系(e)? };
        if let Some(说明) = &谱系 {
            self.谱系断.borrow_mut().insert(e.id, 说明.clone());
        }
        let ev = GuardEv::from_exit(e, 谱系.is_none());
        if e.is_unsure() {
            let arm = arms.get("unsure").unwrap();
            // **unsure 臂也要压守卫。** 那里拿到的是未决责任，**本来就不是一个放行判定**，
            // 所以它不提供可信合取项（`ev` 恒空）。但**不压就是空栈 → 整条 J-08 跳过 →
            // 臂里的不可逆 `do` 自由执行**，与这次修的是同一个洞。
            self.guards.push(ev);
            let r = self.handle_unsure(e, &arm, sp);
            self.guards.pop();
            return r.map(|v| v.stamp(ev));
        }
        let (name, arg): (&str, Value) = match &e.kind {
            ExitKind::Act => ("act", Value::Unit),
            ExitKind::Ignore => ("ignore", Value::Unit),
            // B84：k 继承出口 taint（`20` §3.10「Exit → Bool 继承出口」同一规则，修现状恒 Trusted 的漏），
            // 并带 sources = {出口键}；`over[k]` 经下标规则把它带给取出的候选或题
            ExitKind::Pick(k) => ("pick", Value::Int(*k as i64, 出口标签(e))),
            ExitKind::At(l) => ("at", Value::Int(*l as i64, 出口标签(e))),
            ExitKind::Unsure(_) => unreachable!("上面已经分流"),
        };
        let arm = arms.get(name).or_else(|| arms.get("otherwise")).unwrap();
        e.consumed.set(true);
        *e.consumed_by.borrow_mut() = format!("handle:{name}");
        match arm {
            Value::Fn(c) => {
                let n = c.function.parameters.len();
                let args = if n == 0 { vec![] } else { vec![arg] };
                // **这一臂之所以执行，正是因为那次 `cut` 切出了这个出口——那个出口就是守卫。**
                // 以前 `guards` 只在 `if` 处 push，于是写在 handler 臂里的不可逆 `do`
                // 在 J-08 眼里是「无条件执行」，完全不受管——**而那正是 §5 与 J-05 推荐的写法**。
                // `act`/`ignore`/`pick`/`at` 全走这里，**漏掉任何一个就是把同一个洞挪过去一个分支**。
                self.guards.push(ev);
                let r = self.call_closure(&c, args, sp);
                // **出错路径也要弹**：早返回会把脏栈留给这次运行的其余部分。
                self.guards.pop();
                r.map(|v| v.stamp(ev))
            }
            // 直接给值的臂（`act: true`）：同样盖值（B121-1 (f)）。这一支不求值，没有能写 `do`
            // 的地方，守卫栈无从起作用，所以只盖值。
            other => Ok(other.stamp(ev)),
        }
    }

    /// unsure 臂收到的是**未决责任本身**（`Value::Duty`），不是原因文本。臂体跑完再核它是不是真的
    /// 交出去了：escalate 给人、字面化重问、包装进返回值、或显式 drop。什么都不做就是静默丢弃（J-05）。
    pub(crate) fn handle_unsure(&mut self, e: &Rc<Exit>, arm: &Value, sp: Span) -> R<Value> {
        let Value::Fn(c) = arm else {
            return err(
                Some("J-05"),
                format!(
                    "unsure 的臂是个 {}，收不下未决责任。修法：写成 unsure: fn(u) {{ … }}，在体内先考虑转交——把 u 放进返回值（如 {{…, exit: u}}）交给调用者；或 escalate(u, …) 交给人、literalize(u, …) 重问；consume(u, \"drop\") 排最后，只用于不进入任何输出、不参与路由的题",
                    arm.type_name()
                ),
                sp,
            );
        };
        if c.function.parameters.is_empty() {
            return err(
                Some("J-05"),
                "unsure 的臂没有参数，接不到未决责任。修法：写成 fn(u) { … }",
                c.span,
            );
        }
        let site = c.span;
        let result = self.call_closure(c, vec![Value::Duty(e.clone())], sp)?;
        // escalate / literalize / consume 已经在臂体里销过账
        if e.consumed.get() {
            return Ok(result);
        }
        let mut carried = HashSet::new();
        collect_exit_ids(&result, &mut carried);
        if carried.contains(&e.id) {
            // 13 §3：包进返回值是**转交**，不是了结。这里不销账——责任继续挂着，
            // 交给函数返回检查与程序结束前检查去核。那两处把两件事分开：
            // 责任**真被丢了**是错；责任**如实交了出去、只是签名没说**是警告（见 call_closure）。
            *e.consumed_by.borrow_mut() = "handle:unsure(转交调用者)".into();
            return Ok(result);
        }
        err(
            Some("J-05"),
            format!(
                "unsure 的臂把未决责任丢了：{} 既没进返回值，也没 escalate、重问或 drop。进臂不等于销账。修法：转交——unsure: fn(u) {{ {{…, exit: u}} }} 把 u 放进返回值交给调用者；或 escalate(u, state, 题) 交给人、literalize(u, state, 更字面的题) 重问；consume(u, \"drop\") 排最后，只用于不进入任何输出、不参与路由的题",
                e.label()
            ),
            site,
        )
    }

    /// B162（步 25d）：登记一份未决责任的解除（`consume`、`escalate`、`literalize`）。它的键已由另一个出口解除时，
    /// 这次消费不再计账，报 `W-duty-twice`（写明首次解除处）。依据：B162
    pub(crate) fn 登记解除(&mut self, e: &Exit, how: &str, sp: Span) {
        let 键 = 责任键(e);
        if 键.is_empty() {
            return;
        }
        let 已解: Vec<&String> = 键.iter().filter(|k| self.解除.contains_key(*k)).collect();
        if !已解.is_empty() && 已解.len() == 键.len() {
            let 首次 = self.解除[已解[0]].clone();
            // 依据：B162（地基/附注/2026-09-26-批6裁定.md §十）
            self.trace.warn(format!(
                "W-duty-twice: @{} {} 的未决 {}（题 {}）已在 {首次} 解除；同一判断的责任只计一次（B162），这次消费不再计账。两处去向不同时请只留一处",
                sp.start,
                how,
                e.label(),
                头(&e.q_hash, 8)
            ));
            return;
        }
        for k in 键 {
            self.解除
                .entry(k)
                .or_insert_with(|| format!("@{} {how}", sp.start));
        }
    }

    /// B162：一份未消费的未决责任在返回处怎样有去向。`Some(true)`：它的键都已解除（另一个持有者消费过）；
    /// `Some(false)`：键都出现在返回值的某个未决出口上（含合成出口的分量）、但并非都已解除——随返回值交出；
    /// `None`：没有键或有键无处着落（仍按出口号核）
    pub(crate) fn 键的去向(&self, e: &Exit, 值键: &HashSet<String>) -> Option<bool> {
        let 键 = 责任键(e);
        if 键.is_empty() {
            return None;
        }
        if 键.iter().all(|k| self.解除.contains_key(k)) {
            return Some(true);
        }
        键.iter()
            .all(|k| self.解除.contains_key(k) || 值键.contains(k))
            .then_some(false)
    }

    /// B95（步 21）：返回值离开程序前核一遍显式 drop 过的未决——它本身、它所在的元素记录，或带它
    /// 账本键来源的投影（`item` 与由 `item` 算出的值，B84）仍在返回值里，就是「输出列了这一项，责任却丢了」。
    /// 只告警：drop 是合法去向（B31），这里只指出它与输出矛盾。只凭 `index`/`pos` 算出的编号不带来源，
    /// 运行期看不见，静态面留 `21` 步 24。依据：B95、`12` §3 J-05 注
    pub(crate) fn drop_then_return(&mut self, v: &Value, in_value: &HashSet<usize>) {
        if self.dropped.is_empty() {
            return;
        }
        let mut sources = BTreeSet::new();
        输出来源(v, &mut sources);
        let mut seen = HashSet::new();
        let mut hits: Vec<String> = vec![];
        for e in &self.dropped {
            if !seen.insert(e.id) {
                continue;
            }
            let key = e.ledger_key.borrow().clone();
            if in_value.contains(&e.id) || (!key.is_empty() && sources.contains(&key)) {
                hits.push(format!("{}（题 {}）", e.label(), 头(&e.q_hash, 8)));
            }
        }
        if hits.is_empty() {
            return;
        }
        let n = hits.len();
        let 例 = hits.iter().take(3).cloned().collect::<Vec<_>>().join("、");
        // 依据：B95（返回前可达性核的告警面）
        self.trace.warn(format!(
            "W-drop-then-return: {n} 个已 consume(…, \"drop\") 的未决仍在返回值里（{例}{}）：它所在的元素或由它算出的值被输出了，责任却丢了。修法：转交——投影里保留 exit 字段，或返回 undecided(o) 与 unobserved(o)；这一项确实不进入输出、不参与路由，才 drop 且不返回它",
            if n > 3 { "…" } else { "" }
        ));
    }
}

/// 返回值各叶子的来源键（B84）。**证据键本身不算**：`key_of(出口)` 得到的文本就是那条账本键、来源也是它，
/// 它是证据引用（`12` §2.12「证据只存账本键」），不是被输出的那一项。
fn 输出来源(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Text(t, p) if p.sources.iter().any(|k| k.as_str() == t.as_ref()) => {}
        Value::List(l) => l.iter().for_each(|x| 输出来源(x, out)),
        Value::Record(fs) => fs.iter().for_each(|(_, x)| 输出来源(x, out)),
        Value::Stop(x) => 输出来源(x, out),
        other => out.extend(other.prov().sources.to_set()),
    }
}

/// B162：这个出口是否只因被合成（`compose`、`tally`）吸收才记为已消费——它的责任转进了合成出口、并未解除，
/// 按它自己处理（drop、escalate、重问）仍要按账本键登记解除
pub(crate) fn 已被吸收(e: &Exit) -> bool {
    e.consumed.get() && matches!(e.consumed_by.borrow().as_str(), "compose" | "tally")
}

/// B162：一份未决责任的键：出口自己的账本键；合成出口没有键，取其未决分量的键（递归）。依据：B162
pub(crate) fn 责任键(e: &Exit) -> Vec<String> {
    let k = e.ledger_key.borrow().clone();
    if !k.is_empty() {
        return vec![k];
    }
    let mut out: Vec<String> = vec![];
    for p in e.parts.borrow().iter().filter(|p| p.is_unsure()) {
        for k in 责任键(p) {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// B162：返回值里未决出口（含未决合成出口的未决分量，不论是否已被吸收）带的账本键。已决出口不算持有者：
/// 同一读数经另一条线切出的已决出口，不替这份未决找去向
pub(crate) fn 值里的键(v: &Value) -> HashSet<String> {
    fn 收(e: &Exit, out: &mut HashSet<String>) {
        if !e.is_unsure() {
            return;
        }
        let k = e.ledger_key.borrow().clone();
        if !k.is_empty() {
            out.insert(k);
        }
        for p in e.parts.borrow().iter() {
            收(p, out);
        }
    }
    let mut out = HashSet::new();
    visit_exits(v, &mut |e| 收(e, &mut out));
    out
}

/// B162：返回值里每个账本键的持有者路径（`$.a[0].exit` 形）；合成出口按其分量的键计入
pub(crate) fn 键的持有者(
    v: &Value,
) -> std::collections::BTreeMap<String, (String, Vec<String>)> {
    fn 走(
        v: &Value,
        path: &str,
        out: &mut std::collections::BTreeMap<String, (String, Vec<String>)>,
    ) {
        match v {
            Value::Exit(e) | Value::Duty(e) if e.is_unsure() => {
                let mut 键 = vec![];
                let k = e.ledger_key.borrow().clone();
                if !k.is_empty() {
                    键.push(k);
                } else {
                    fn 分量键(e: &Exit, out: &mut Vec<String>) {
                        for p in e.parts.borrow().iter().filter(|p| p.is_unsure()) {
                            let k = p.ledger_key.borrow().clone();
                            if !k.is_empty() {
                                if !out.contains(&k) {
                                    out.push(k);
                                }
                            } else {
                                分量键(p, out);
                            }
                        }
                    }
                    分量键(e, &mut 键);
                }
                for k in 键 {
                    let slot = out.entry(k).or_insert_with(|| (e.label(), vec![]));
                    if !slot.1.contains(&path.to_string()) {
                        slot.1.push(path.to_string());
                    }
                }
            }
            Value::Cut(c) => {
                if let Some(e) = c.exit() {
                    走(&Value::Exit(e), path, out)
                }
            }
            Value::List(l) => l
                .iter()
                .enumerate()
                .for_each(|(i, x)| 走(x, &format!("{path}[{i}]"), out)),
            Value::Record(r) => r
                .iter()
                .for_each(|(k, x)| 走(x, &format!("{path}.{k}"), out)),
            Value::Stop(x) => 走(x, path, out),
            _ => {}
        }
    }
    let mut out = std::collections::BTreeMap::new();
    走(v, "$", &mut out);
    out
}
