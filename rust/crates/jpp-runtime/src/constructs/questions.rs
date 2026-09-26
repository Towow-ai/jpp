//! 题与题式：`test`、`select`、`measure`、`form`、`fill`。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。
//! 步 25-2c（B138）：题值是内核值，构造与其 taint 等字段的初始化经 `IssueQuestion` 令牌；本文件只解析参数。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_test(
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
        if n != 2 && n != 3 {
            return err(
                Some("E-rt-arity"),
                format!(
                    "{name} 需要 2 或 3 个参数（题面, calib[, {{evidence: [槽名…]}}]），收到 {n}"
                ),
                sp,
            );
        }
        let (Value::Text(t, _), Value::Text(c, _)) = (&args[0], &args[1]) else {
            return err(
                Some("J-03"),
                format!("{name}(题面: Text, calib: Text) — calib 是校准记录的键，不是线"),
                sp,
            );
        };
        let op = if name == "test" { Op::Test } else { Op::Select };
        let evidence = evidence_of(args.get(2), sp)?;
        let (presupposition, request) = question_decl_of(args.get(2), op, sp)?;
        let permute = permute_of(args.get(2), op, sp)?;
        let labels = labels_of(args.get(2), op, sp)?;
        Ok(caps.issue_question().question(
            op,
            t,
            c,
            evidence,
            presupposition,
            request,
            permute,
            labels,
        ))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_measure(
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
        arity(3)?;
        let (Value::Text(t, _), Value::List(scale), Value::Text(c, _)) =
            (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-question"), "measure(题面, [档位…], calib)", sp);
        };
        let mut sc = vec![];
        for s in scale.iter() {
            match s {
                Value::Text(x, _) => sc.push(x.to_string()),
                _ => return err(Some("E-rt-question"), "档位要是 Text", sp),
            }
        }
        if sc.len() < 2 {
            return err(Some("E-rt-question"), "measure 至少两档", sp);
        }
        Ok(caps.issue_question().measure_question(t, c, sc))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_form(
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
        // form(题型, 模板题面, {calib, scale?, evidence?, presupposition?, request?}) → 题式
        arity(3)?;
        let (Value::Text(opname, _), Value::Text(template, _)) = (&args[0], &args[1]) else {
            return err(
                Some("E-rt-question"),
                "form(题型: \"test\" | \"select\" | \"measure\", 模板题面: Text, {calib: \"校准键\", …})",
                sp,
            );
        };
        let op = match opname.as_ref() {
            "test" => Op::Test,
            "select" => Op::Select,
            "measure" => Op::Measure,
            other => {
                return err(
                    Some("E-rt-question"),
                    format!("form 的题型要是 test / select / measure，收到 {other}"),
                    sp,
                );
            }
        };
        let Value::Record(_) = &args[2] else {
            return err(
                Some("E-rt-question"),
                "form 的第三个参数要是记录：{calib: \"校准键\", …}",
                sp,
            );
        };
        let calib = match args[2].get("calib") {
            Some(Value::Text(c, _)) => c.to_string(),
            Some(Value::Int(_, _)) | Some(Value::Float(_, _)) => {
                return err(
                    Some("J-03"),
                    "form 的 calib 是数字：线不可字面，这一位只收校准记录的键（Text）",
                    sp,
                );
            }
            _ => {
                return err(
                    Some("J-03"),
                    "form 需要 calib：{calib: \"校准键\"}。线只从校准记录来",
                    sp,
                );
            }
        };
        let mut scale = vec![];
        if let Some(v) = args[2].get("scale") {
            let Value::List(l) = v else {
                return err(Some("E-rt-question"), "scale 要是档位列表", sp);
            };
            for x in l.iter() {
                match x {
                    Value::Text(t, _) => scale.push(t.to_string()),
                    _ => return err(Some("E-rt-question"), "档位要是 Text", sp),
                }
            }
        }
        match (op, scale.len()) {
            (Op::Measure, n) if n < 2 => {
                return err(
                    Some("E-rt-question"),
                    "measure 题式至少两档：{scale: [\"低\", \"高\"]}",
                    sp,
                );
            }
            (Op::Test | Op::Select, n) if n > 0 => {
                return err(Some("E-rt-question"), "只有 measure 题式带 scale", sp);
            }
            _ => {}
        }
        let evidence = evidence_of(Some(&args[2]), sp)?;
        let (presupposition, request) = question_decl_of(Some(&args[2]), op, sp)?;
        let f = caps
            .issue_question()
            .form(
                op,
                template,
                &calib,
                scale,
                evidence,
                presupposition,
                request,
            )
            .map_err(|m| Fault::Error(RtError::new(Some("E-rt-question"), m, sp)))?;
        // B64（步 15f）：置换声明，不进 form_hash，由 fill 带到题上
        let permute = permute_of(Some(&args[2]), op, sp)?;
        // B76（步 12e-2）：题式的 `over` 声明，只决定题类，不进 form_hash
        let over_kind = match args[2].get("over_kind") {
            None | Some(Value::Unit) => None,
            Some(Value::Text(t, _)) => Some(jpp_value::value::OverKind::parse(&t).ok_or_else(|| {
                Fault::Error(RtError::new(
                    Some("E-rt-question"),
                    format!("over_kind 「{t}」不认识：只收 labels、candidates、questions、actions（B76）"),
                    sp,
                ))
            })?),
            Some(other) => {
                return err(
                    Some("E-rt-question"),
                    format!("over_kind 要是文本，收到 {}", other.type_name()),
                    sp,
                );
            }
        };
        let labels = labels_of(Some(&args[2]), op, sp)?;
        Ok(caps
            .issue_question()
            .finish_form(f, permute, over_kind, labels))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_fill(
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
        // fill(题式, {槽: 值, …}) → 题。值按 text() 渲染；Int/Float/Bool/Text 以外的值不能填进题面。
        arity(2)?;
        let Value::Form(f) = &args[0] else {
            return err(
                Some("E-rt-question"),
                format!(
                    "fill 的第一个参数要是题式（form(…) 的结果），收到 {}",
                    args[0].type_name()
                ),
                sp,
            );
        };
        let Value::Record(fields) = &args[1] else {
            return err(
                Some("E-rt-question"),
                "fill 的第二个参数要是记录：{槽名: 值}",
                sp,
            );
        };
        let mut fill = vec![];
        // B58（步 17b）：题面 taint = 模板 ∨ 各填入值。在这里并而不只在分派处并，
        // 因为 `sieve(材料, 题式, [填法…])` 直接调本内置、不经分派。依据：B58
        let mut 题面taint = f.taint;
        for (k, v) in fields.iter() {
            题面taint = Taint::join(题面taint, v.taint());
            let t = match v {
                Value::Text(t, _) => t.to_string(),
                Value::Int(i, _) => i.to_string(),
                Value::Float(x, _) => x.to_string(),
                Value::Bool(b, _, _) => b.to_string(),
                Value::Reading(_) => {
                    return err(
                        Some("J-01"),
                        format!("槽 {k} 填的是读数：读数不能进题面（它不是材料，也不可渲染）"),
                        sp,
                    );
                }
                other => {
                    return err(
                        Some("E-rt-question"),
                        format!(
                            "槽 {k} 要填 Text / Int / Float / Bool，收到 {}",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            };
            fill.push((k.clone(), t));
        }
        caps.issue_question()
            .fill_form(f, &fill, 题面taint)
            .map_err(|m| Fault::Error(RtError::new(Some("E-rt-question"), m, sp)))
    }
}
