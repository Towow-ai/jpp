//! 三路过滤 `sieve`（施工件 c）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::constructs::element::元素上下文;
use crate::*;

impl<'a> Interp<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sieve(
        &mut self,
        caps: &Caps,
        items: &[Value],
        qs: &[Rc<Question>],
        carried_pending: &[Value],
        carried_evidence: &[Value],
        fills: Option<&[Value]>,
        sp: Span,
    ) -> R<Value> {
        let m0 = caps.key_collect().mark(self);
        for q in qs {
            if q.op != Op::Test {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "sieve 只收是非题（test）：题「{}」是 {}。K 选一与打分的分流待后续件",
                        q.text,
                        q.op.fixture_name()
                    ),
                    sp,
                );
            }
        }
        // B93（步 22-0）：预算停机不再以 `Halt` 冒到这里，入口不必先把此前的登记单独发成一层
        let mut prepared: Vec<(Value, Value, Vec<Rc<Reading>>, Value)> = vec![];
        for it in items {
            let (material, trail) = element_parts(it);
            let state = match &material {
                Value::State(s) => s.clone(),
                other => match self.make_state(&[other.clone()], sp)? {
                    Value::State(s) => s,
                    _ => return err(Some("E-rt-arg"), "sieve 无法把元素变成状态", sp),
                },
            };
            // B59（步 17a）：由元素记录构造的状态，来源读数是元素的出口（结构通道）
            let lineage = element_lineage(it);
            let state = if lineage.is_empty() {
                state
            } else {
                Rc::new((*state).clone().with_parents(lineage))
            };
            let rs = self.judge(&state, qs, sp)?;
            let rs: Vec<Rc<Reading>> = rs
                .into_iter()
                .map(|v| match v {
                    Value::Reading(r) => r,
                    _ => unreachable!("judge 只回读数"),
                })
                .collect();
            prepared.push((material, trail, rs, it.clone()));
        }
        self.flush("sieve")?;
        // B93：预算停发的读数由刷新标了 `budget`、记了缺席账；停止说明从缺席账取（首个停发的读数）
        let 停发 = |me: &Self, r: &Reading| {
            caps.ledger_read().absent_mark(me, &r.ledger_key) == Some("budget")
        };
        let stopped: Option<String> = prepared
            .iter()
            .flat_map(|(_, _, rs, _)| rs.iter())
            .find(|r| 停发(self, r))
            .map(|r| {
                match caps
                    .ledger_read()
                    .ledger(self)
                    .get(&format!("absent:{}", r.ledger_key))
                {
                    Some(Entry::Absent { detail, .. }) => detail.clone(),
                    _ => String::new(),
                }
            });
        let (_, carried_spent) = caps.key_collect().since(self, m0);
        // B82（步 25-0）：一个契约值，元素集 = 材料 × 题（材料主序、题次序）；未决一份、证据取并、花费一份。
        let (mut act, mut ignore, mut pending, mut evidence) = (vec![], vec![], vec![], vec![]);
        // 未观察的元素（材料 × 题）总数：先数出来，告警仍在第一道题切完时报（与步 25-0 前同一位置）
        let n_unobserved = prepared
            .iter()
            .flat_map(|(_, _, rs, _)| rs.iter())
            .filter(|r| caps.read_answer().answer_of(self, r).is_none() && r.fail.is_none())
            .count();
        // 切（与出口编号、告警）仍按题主序进行，与步 25-0 之前逐题造契约值时同一顺序；
        // 元素按材料主序放进产物（B82）。格子里记（流，元素）。
        let mut 格: Vec<Vec<Option<(u8, Value)>>> = vec![vec![None; qs.len()]; prepared.len()];
        for (j, q) in qs.iter().enumerate() {
            for (i, (_, _, rs, source)) in prepared.iter().enumerate() {
                let r = &rs[j];
                // B81 (a)、B133（步 25-2b）：输出元素经元素构造 `element` 造——保留输入元素的全部字段，
                // `index` 只在输入不是元素记录时赋为输入位置，`pos` 是本次调用里的位置；选择边与报告行也在那里写。
                let 上下文 = || 元素上下文 {
                    pos: i,
                    q: q.clone(),
                    qi: j,
                    fill: fills.map(|fs| fs[j].clone()),
                    key: r.ledger_key.clone(),
                };
                if caps.read_answer().answer_of(self, r).is_none() && r.fail.is_none() {
                    // 预算停机没问到：记为 Unsure(budget) 进未决清单（B17 取舍），不混进 ignore。
                    // 出口在这里直接造，与步 22-0 前相同（不进报告 `exits` 表）。已知旧问题（不在本步改）：
                    // 缺席策略 conservative 下没有答案的读数也走这里、记成 budget（过程记录 22-0 问题清单）
                    let ex = caps.issue_unsure().new_unsure(
                        self,
                        "budget",
                        Op::Test,
                        &q.hash,
                        jpp_value::value::Taint::Trusted,
                        sp,
                    );
                    let Value::Exit(e) = &ex else {
                        unreachable!("new_unsure 只给出口")
                    };
                    // B81 (c)：未决条目就是带 exit 的元素记录（这个出口没有账本键与报告行，元素构造不加边、不写行）
                    格[i][j] = Some((2, self.调元素(source, e, 上下文())));
                    continue;
                }
                if !r.ledger_key.is_empty()
                    && !evidence.iter().any(
                        |k: &Value| matches!(k, Value::Text(t, _) if t.as_ref() == r.ledger_key),
                    )
                {
                    evidence.push(Value::text(&r.ledger_key));
                }
                let exit = self.cut(r, None, Default::default(), sp)?;
                let Value::Exit(e) = &exit else {
                    return err(Some("E-rt-arg"), "cut 没有给出出口", sp);
                };
                // 元素构造写报告 `exits` 行的 `index`/`pos`（B120 (b)）与 `item` 的选择边（B84、B92：元素内容先于
                // 读数存在，读数只决定选中了它）；元素记录的 exit 照 17a 保留
                let 流 = match &e.kind {
                    ExitKind::Act => {
                        caps.duty().settle(e, "sieve:act");
                        0
                    }
                    ExitKind::Ignore => {
                        caps.duty().settle(e, "sieve:ignore");
                        1
                    }
                    // B81 (c)：未决条目就是带 exit 的元素记录
                    ExitKind::Unsure(_) => 2,
                    _ => return err(Some("E-rt-arg"), "是非题给出了非是非出口", sp),
                };
                格[i][j] = Some((流, self.调元素(source, e, 上下文())));
            }
            if let Some(detail) = &stopped {
                if j == 0 && n_unobserved > 0 {
                    // 数的是未观察的元素（材料 × 题），步 25-0 起不再只数第一道题
                    // 依据：B17（预算未观察项记 Unsure(budget) 进未决清单）、B93（停止说明取自缺席账）
                    self.trace.warn(format!("W-sieve-budget: 三路过滤在预算处停止，{} 个元素未观察，记为 Unsure(budget) 进未决清单（未计入 ignore）：{}", n_unobserved, detail));
                }
            }
        }
        for (流, e) in 格.into_iter().flatten().flatten() {
            match 流 {
                0 => act.push(e),
                1 => ignore.push(e),
                _ => pending.push(e),
            }
        }
        let resume = match &stopped {
            Some(detail) => Value::record(vec![
                ("reason".into(), Value::text("budget")),
                ("detail".into(), Value::text(detail)),
                (
                    "unobserved".into(),
                    Value::Int(n_unobserved as i64, Taint::Trusted.into()),
                ),
            ]),
            None => Value::Unit,
        };
        // 单题 `{question, ignore}` 与此前相同；多题 `{questions, ignore}`（步 25-0 解释登记）
        let detail = if qs.len() == 1 {
            Value::record(vec![
                ("question".into(), Value::Question(qs[0].clone())),
                ("ignore".into(), Value::list(ignore)),
            ])
        } else {
            Value::record(vec![
                (
                    "questions".into(),
                    Value::list(qs.iter().map(|q| Value::Question(q.clone())).collect()),
                ),
                ("ignore".into(), Value::list(ignore)),
            ])
        };
        pending.extend(carried_pending.iter().cloned());
        for k in carried_evidence.iter() {
            push_key(&mut evidence, k.clone());
        }
        Self::outcome_value(
            "sieve",
            Value::list(act),
            pending,
            evidence,
            resume,
            carried_spent,
            detail,
            Value::Unit,
            sp,
        )
    }
    #[allow(unused_variables)]
    pub(crate) fn b_sieve(
        &mut self,
        caps: &Caps,
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
        // 三路过滤（05 §1 `filter(S, q)`，施工件 c）：一组材料 × 一道题（或题列表 / 题式 + 填法）
        // → 三条流 act / ignore / unsure，外加 unobserved（预算提前停止时没问到的）。
        // 直接吃题：全部登记完再一次刷新，同状态的题由融合合成一次调用——
        // 「14 倍」那种绕过批处理的写法在这里没有可写的位置。
        if n != 2 && n != 3 {
            return err(
                Some("E-rt-arg"),
                "sieve(材料列表, 题 | [题…]) 或 sieve(材料列表, 题式, [填法…])",
                sp,
            );
        }
        // 输入可以是列表，也可以是上一个构造的契约值（取它的产出；它的未决与证据带进新契约）
        let (items, carried_pending, carried_evidence) = self.unpack(&args[0], "sieve", sp)?;
        let mut fill_records: Option<Vec<Value>> = None;
        let (qs, many) = if n == 3 {
            let Value::List(fills) = &args[2] else {
                return err(
                    Some("E-rt-arg"),
                    "sieve(材料, 题式, [填法…]) 的第三个参数要是填法记录的列表",
                    sp,
                );
            };
            // B82：填法记录可以带槽以外的键（元素的 `fill` 原样保留整条记录）；交给 `fill()` 的只有槽键
            let slots: Vec<String> = match &args[1] {
                Value::Form(f) => f.slots.clone(),
                _ => vec![],
            };
            let mut qs = vec![];
            for f in fills.iter() {
                let 槽键 = match f {
                    Value::Record(fs) if !slots.is_empty() => Value::record(
                        fs.iter()
                            .filter(|(k, _)| slots.contains(k))
                            .cloned()
                            .collect(),
                    ),
                    other => other.clone(),
                };
                match self.builtin("fill", vec![args[1].clone(), 槽键], sp)? {
                    Value::Question(q) => qs.push(q),
                    _ => return err(Some("E-rt-question"), "fill 没有给出题", sp),
                }
            }
            fill_records = Some(fills.iter().cloned().collect::<Vec<Value>>());
            (qs, true)
        } else {
            match &args[1] {
                Value::Question(q) => (vec![q.clone()], false),
                Value::List(l) => {
                    let mut qs = vec![];
                    for q in l.iter() {
                        match q {
                            Value::Question(q) => qs.push(q.clone()),
                            other => {
                                return err(
                                    Some("E-rt-arg"),
                                    format!("sieve 的题列表里有 {}", other.type_name()),
                                    sp,
                                );
                            }
                        }
                    }
                    (qs, true)
                }
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "sieve 的第二个参数要是题或题列表，收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        };
        // B82：多题与多填法也返回一个契约值（元素 = 材料 × 题），单题是 K = 1 的特例
        let _ = many;
        self.sieve(
            caps,
            &items,
            &qs,
            &carried_pending,
            &carried_evidence,
            fill_records.as_deref(),
            sp,
        )
    }
}
