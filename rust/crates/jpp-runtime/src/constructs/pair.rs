//! 配对 `pair`（施工件 e）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_pair(
        &mut self,
        _caps: &Caps,
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
        // 配对（05 §1 `pair(S, T)`，施工件 e）：两组材料 → 关系记录，返回契约值（B17）。
        // 候选怎么枚举由调用者定：全配对 `pair(左, 右)`；按调用者的方法取舍
        // `pair(左, 右, fn(a, b) -> Bool)`；或直接给候选对 `pair([[a, b], …])`。
        // 这里只构造关系，不判断；关系交给 sieve 按关系题分流。
        // 关系记录：`item` 是交给判断器的一份状态材料，两端对象段结构化标为 a / b；
        // `left` / `right` 原样保留调用者给的元素。左右可以是契约值：取产出，未决与证据带进新契约。
        let mut cands: Vec<(Value, Value, i64, i64)> = vec![];
        let mut carried_pending = vec![];
        let mut carried_evidence: Vec<Value> = vec![];
        match n {
            1 => {
                let Value::List(ps) = &args[0] else {
                    return err(
                        Some("E-rt-arg"),
                        "pair([[a, b], …]) 的参数要是候选对的列表",
                        sp,
                    );
                };
                for (k, p) in ps.iter().enumerate() {
                    match p {
                        Value::List(ab) if ab.len() == 2 => {
                            cands.push((ab[0].clone(), ab[1].clone(), k as i64, k as i64))
                        }
                        other => {
                            return err(
                                Some("E-rt-arg"),
                                format!(
                                    "pair 的候选对要是两个元素的列表，第 {k} 个是 {}",
                                    other.type_name()
                                ),
                                sp,
                            );
                        }
                    }
                }
            }
            2 | 3 => {
                let (l, lp, le) = self.unpack(&args[0], "pair", sp)?;
                let (r, rp, re) = self.unpack(&args[1], "pair", sp)?;
                carried_pending.extend(lp);
                carried_pending.extend(rp);
                for k in le.into_iter().chain(re) {
                    push_key(&mut carried_evidence, k);
                }
                for (i, a) in l.iter().enumerate() {
                    for (j, b) in r.iter().enumerate() {
                        if n == 3 {
                            match self.apply(args[2].clone(), vec![a.clone(), b.clone()], sp)? {
                                Value::Bool(true, _, _) => {}
                                Value::Bool(false, _, _) => continue,
                                other => {
                                    return err(
                                        Some("E-rt-type"),
                                        format!(
                                            "pair 的取舍方法要返回 Bool，收到 {}",
                                            other.type_name()
                                        ),
                                        sp,
                                    );
                                }
                            }
                        }
                        cands.push((a.clone(), b.clone(), i as i64, j as i64));
                    }
                }
            }
            _ => {
                return err(
                    Some("E-rt-arg"),
                    "pair(左, 右) / pair(左, 右, fn(a, b)) / pair([[a, b], …])",
                    sp,
                );
            }
        }
        let rels = cands
            .into_iter()
            .enumerate()
            .map(|(pos, (a, b, i, j))| {
                let (ma, _) = element_parts(&a);
                let (mb, _) = element_parts(&b);
                Value::record(vec![
                    (
                        "item".into(),
                        Value::record(vec![("a".into(), ma), ("b".into(), mb)]),
                    ),
                    ("trail".into(), Value::list(vec![])),
                    ("left".into(), a),
                    ("right".into(), b),
                    (
                        "at".into(),
                        Value::list(vec![
                            Value::Int(i, Taint::Trusted.into()),
                            Value::Int(j, Taint::Trusted.into()),
                        ]),
                    ),
                    // B81 (a)（步 25-0）：本次产物里的位置；配对产物不赋 index
                    ("pos".into(), Value::Int(pos as i64, Taint::Trusted.into())),
                ])
            })
            .collect();
        Self::outcome_value(
            "pair",
            Value::list(rels),
            carried_pending,
            carried_evidence,
            Value::Unit,
            (0, 0.0),
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
}
