//! 调用者构造契约值 `outcome`（B17）。账本键取用 `key_of` 自步 25-2c 起是宿主内置（B138 (3)，`host_builtins.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_outcome(
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
        // 调用者自己构造一个契约值（B17）：`outcome({value, pending?, evidence?, resume?, purpose?, detail?})`。
        // 与内置构造返回同一类型，可再交给 sieve / pair / tally 等。
        // - pending：出口，或 `{element?, exit, cause?}` 记录；每项必须带出口（责任载体）；
        // - evidence：只收账本键（Text，由 key_of 取得），不收读数或材料副本（不变量 3）；
        // - resume：记录；或一个方法，记为 `{reason: "continue", next: 方法}`。
        arity(1)?;
        let r = &args[0];
        if !matches!(r, Value::Record(_)) {
            return err(
                Some("E-rt-arg"),
                "outcome({value, pending?, evidence?, resume?, purpose?, detail?})",
                sp,
            );
        }
        if let Value::Record(fs) = r {
            for (k, _) in fs.iter() {
                if k == "spent" {
                    // A-2（B81，步 25-0）：花费由运行时从 evidence 的账本键算，程序不给
                    return err(
                        Some("E-rt-arg"),
                        "outcome 不收 spent：契约值的花费由运行时按 evidence 里的账本键算（A-2）。修法：删掉 spent，把证据键放进 evidence",
                        sp,
                    );
                }
                if ![
                    "value", "pending", "evidence", "resume", "purpose", "detail",
                ]
                .contains(&k.as_str())
                {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "outcome 不认得字段 {k}：可给 value、pending、evidence、resume、purpose、detail"
                        ),
                        sp,
                    );
                }
            }
        }
        let Some(value) = r.get("value") else {
            return err(Some("E-rt-arg"), "outcome 必须给 value（产出）", sp);
        };
        let mut pending = vec![];
        for (i, p) in list_of(r.get("pending")).into_iter().enumerate() {
            match &p {
                Value::Exit(_) | Value::Duty(_) => {
                    pending.push(Self::pending_entry(Value::Unit, &p))
                }
                Value::Record(_) => match p.get("exit") {
                    Some(x @ (Value::Exit(_) | Value::Duty(_))) => pending.push(
                        // B81 (c)：带 exit 的记录原样收（元素记录本身就是未决条目）
                        Self::pending_entry(p.clone(), &x),
                    ),
                    _ => {
                        return err(
                            Some("J-05"),
                            format!(
                                "outcome 的 pending 第 {i} 项没有出口：未决清单的每一项都要带承担责任的出口（exit）。修法：把 cut / handle 前的出口放进来"
                            ),
                            sp,
                        );
                    }
                },
                other => {
                    return err(
                        Some("J-05"),
                        format!(
                            "outcome 的 pending 第 {i} 项是 {}：只收出口或带 exit 的记录",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        let mut evidence = vec![];
        for (i, k) in list_of(r.get("evidence")).into_iter().enumerate() {
            match &k {
                Value::Text(t, _) if !t.is_empty() => push_key(&mut evidence, k.clone()),
                other => {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "E-evidence: outcome 的 evidence 第 {i} 项是 {}：证据只存账本键（Text），不存读数、观察或材料的副本。修法：用 key_of(出口) 取键",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        let resume = match r.get("resume") {
            None | Some(Value::Unit) => Value::Unit,
            Some(f @ (Value::Fn(_) | Value::Builtin(_))) => Value::record(vec![
                ("reason".into(), Value::text("continue")),
                ("next".into(), f),
            ]),
            Some(rec @ Value::Record(_)) => rec,
            Some(other) => {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "outcome 的 resume 要是记录或方法，收到 {}",
                        other.type_name()
                    ),
                    sp,
                );
            }
        };
        // A-2：花费 = 证据键对应的判断条目的不同调用数与这些调用的费用和（融合的多道题同属一次调用，只计一次）
        let spent = self.spent_of_evidence(caps.ledger_read(), &evidence);
        Self::outcome_value(
            "outcome",
            value,
            pending,
            evidence,
            resume,
            spent,
            r.get("detail").unwrap_or(Value::record(vec![])),
            r.get("purpose").unwrap_or(Value::Unit),
            sp,
        )
    }
    /// A-2（步 25-0）：由证据的账本键算花费。键对应的判断条目按 `call` 去重，费用取每次调用记一次。
    pub(crate) fn spent_of_evidence(
        &self,
        cap: &crate::caps::Cap<crate::caps::LedgerRead>,
        evidence: &[Value],
    ) -> (i64, f64) {
        let mut calls: std::collections::BTreeMap<u64, f64> = Default::default();
        for k in evidence {
            if let Value::Text(t, _) = k {
                if let Some(Entry::Judge { call, cost, .. }) = cap.ledger(self).get(t) {
                    if *call > 0 {
                        calls.entry(*call).or_insert(*cost);
                    }
                }
            }
        }
        (calls.len() as i64, calls.values().sum())
    }
}
