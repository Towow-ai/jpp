//! 有界迭代 `iterate`（施工件 g）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_iterate(
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
        // 有界迭代（05 §1 `iterate(f, S, bound)`，施工件 g）：三条终止线并存——
        // 步数到上限（bound）、每层材料严格变少（measure 不再下降即停，noshrink）、
        // 账本键在本循环内重复即停（repeat，J-06）。step 返回 stop(v) 也停。
        // 终止原因写进结果：{value, reason, rounds, measures}。
        // measure：fn(acc) -> Int，或 "tokens"（按渲染后的 token 估算，与窗口检查同一估法）。
        arity(4)?;
        let Value::Int(b, _) = &args[0] else {
            return err(Some("J-06"), "iterate 的 bound 必须是整数", sp);
        };
        let bound = *b;
        // 依据：12 §3 J-06（bound 必填、为正整数）
        if bound <= 0 {
            return err(
                Some("J-06"),
                format!("iterate 的 bound 必须是正整数，收到 {bound}"),
                sp,
            );
        }
        let Value::Fn(step) = &args[2] else {
            return err(
                Some("E-rt-arg"),
                "iterate(bound, 初值, fn(acc, i), measure) 的 step 要是函数",
                sp,
            );
        };
        let step = step.clone();
        let measure = args[3].clone();
        let measure_of = |me: &mut Self, v: &Value| -> R<i64> {
            match &measure {
                Value::Text(t, _) if t.as_ref() == "tokens" => {
                    Ok((canon(&v.to_json()).chars().count() as f64 / 1.3) as i64 + 1)
                }
                Value::Fn(_) | Value::Builtin(_) => {
                    match me.apply(measure.clone(), vec![v.clone()], sp)? {
                        Value::Int(i, _) => Ok(i),
                        other => err(
                            Some("E-rt-arg"),
                            format!("iterate 的 measure 要返回 Int，收到 {}", other.type_name()),
                            sp,
                        ),
                    }
                }
                other => err(
                    Some("E-rt-arg"),
                    format!(
                        "iterate 的 measure 要是 fn(acc) -> Int 或 \"tokens\"，收到 {}",
                        other.type_name()
                    ),
                    sp,
                ),
            }
        };
        let m0 = caps.key_collect().mark(self);
        let mut acc = args[1].clone();
        let mut prev = measure_of(self, &acc)?;
        let mut measures = vec![Value::Int(prev, Taint::Trusted.into())];
        let mut reason = "bound";
        let mut rounds = 0i64;
        caps.loop_context().loops(self).push(LoopCtx {
            seen_keys: HashSet::new(),
            repeated: None,
        });
        for i in 0..bound {
            let out = match self.call_closure(
                &step,
                vec![acc.clone(), Value::Int(i, Taint::Trusted.into())],
                sp,
            ) {
                Ok(v) => v,
                Err(e) => {
                    caps.loop_context().loops(self).pop();
                    return Err(e);
                }
            };
            rounds = i + 1;
            if let Value::Stop(v) = out {
                acc = (*v).clone();
                reason = "stop";
                break;
            }
            acc = out;
            if caps
                .loop_context()
                .loops(self)
                .last()
                .and_then(|l| l.repeated.clone())
                .is_some()
            {
                reason = "repeat";
                break;
            }
            let m = match measure_of(self, &acc) {
                Ok(m) => m,
                Err(e) => {
                    caps.loop_context().loops(self).pop();
                    return Err(e);
                }
            };
            measures.push(Value::Int(m, Taint::Trusted.into()));
            if m >= prev {
                reason = "noshrink";
                break;
            }
            prev = m;
        }
        caps.loop_context().loops(self).pop();
        // 停止原因与轮次进续接（B17 取舍）；产出是最后的累积值
        let (evidence, spent) = caps.key_collect().since(self, m0);
        let resume = Value::record(vec![
            ("reason".into(), Value::text(reason)),
            ("rounds".into(), Value::Int(rounds, Taint::Trusted.into())),
            ("measures".into(), Value::list(measures)),
        ]);
        Self::outcome_value(
            "iterate",
            acc,
            vec![],
            evidence,
            resume,
            spent,
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
}
