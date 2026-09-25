//! 同题重复读数 `repeat`（旧名 `agg`）与判断向量 `order`（B28）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_agg(
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
        if n == 0 || n > 2 {
            return err(
                Some("E-rt-arg"),
                format!("{name}(读数列表[, \"mean\" | \"median\"])"),
                sp,
            );
        }
        if name == "agg" {
            self.trace.warn(format!("W-deprecated: @{} agg 已改名 repeat（B28），语义改为只取均值 / 中位数、禁众数；下一版移除 agg", sp.start));
        }
        let method = match args.get(1) {
            None => "mean".to_string(),
            Some(Value::Text(t, _)) if t.as_ref() == "mean" || t.as_ref() == "median" => {
                t.to_string()
            }
            Some(Value::Text(t, _)) if t.as_ref() == "mode" => {
                return err(
                    Some("B28"),
                    "repeat 不许取众数：对出口或选项投票就是多数表决，错误持久时它不降错（B9 / B28）。修法：用 mean 或 median 压抖动",
                    sp,
                );
            }
            Some(other) => {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "repeat 的方式只收 \"mean\" 或 \"median\"，收到 {}",
                        other.type_name()
                    ),
                    sp,
                );
            }
        };
        self.flush("repeat")?;
        let rs = self.readings_of(&args[0], "repeat", sp)?;
        if rs.is_empty() {
            return err(Some("E-rt-arg"), "repeat 要至少一条读数", sp);
        }
        // 合并后**仍是读数**——所以还能 cut。拿 fold 求平均得到的是裸数，进不了 cut、也不带校准键。
        let first = &rs[0];
        if rs.iter().any(|r| r.q_hash != first.q_hash) {
            return err(
                Some("J-01"),
                "repeat 只合并**同一道题**跨运行的读数：收到的读数不是同一道题",
                sp,
            );
        }
        let merged = merge_runs(&*caps.read_answer().answers(self), &rs, &method, sp)?;
        let n_runs = rs.len();
        let 键 = format!("{}\u{1f}repeat(n={n_runs})", first.calib);
        let lk = format!("repeat(n={n_runs},{method}):{}", first.ledger_key);
        if caps.ledger_write().ledger_mut(self).get(&lk).is_none() {
            caps.ledger_write().ledger_mut(self).put(Entry::effect_keyed(lk.clone(), "repeat", serde_json::json!({"n": n_runs, "method": method, "calib": 键.replace('\u{1f}', ":")}), 0.0));
        }
        caps.ledger_write().trace_event(
            self,
            "repeat",
            &lk,
            false,
            0.0,
            sp,
            format!("n={n_runs} {method}"),
        );
        if !self
            .calib
            .line(&first.calib)
            .rerun_independent
            .unwrap_or(false)
        {
            self.trace.warn(format!(
                    "W-repeat-persistent: @{} 键 {} 未通过重跑分歧检验：重复读数只压抖动、不降错（错误持久，B9）。合并结果用独立键 {}，没有它的认证记录就是冷",
                    sp.start, first.calib, 键.replace('\u{1f}', ":")
                ));
        }
        let r = Rc::new(Reading {
            q_hash: first.q_hash.clone(),
            state_hash: first.state_hash.clone(),
            op: first.op,
            calib: 键,
            id: caps.issue_reading().new_reading_id(self),
            fail: None,
            model_id: first.model_id.clone(),
            ledger_key: lk,
            over_len: first.over_len,
            scale: first.scale.clone(),
            perms: std::cell::Cell::new(first.perms.get()),
            mode_share: std::cell::Cell::new(first.mode_share.get()),
            missing_evidence: first.missing_evidence.clone(),
            state_taint: rs
                .iter()
                .fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
            // 合并结果不借题式线：它有自己的含 n 的键（B28）
            form_hash: None,
            fp: None,
        });
        caps.issue_reading().fill_answer(self, &r, merged);
        Ok(Value::Reading(r))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_order(
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
        arity(1)?;
        self.flush("order")?;
        let rs = self.readings_of(&args[0], "order", sp)?;
        // J-04（12:255）：跨题、跨候选集、跨刻度或异锚的读数**不可比**。
        // order 是排序，排序就是比——两道题各有各的校准线，p 不在同一把尺子上，
        // 「问题一 0.9 高于问题二 0.5」这个比较本身不成立。
        if let Some(first) = rs.first() {
            for r in &rs {
                if r.q_hash != first.q_hash {
                    return err(
                        Some("J-04"),
                        "order 只排**同一道题**跨对象的读数：跨题的读数不可比（各有各的校准线，p 不在同一把尺子上）",
                        sp,
                    );
                }
                if r.over_len != first.over_len || r.scale != first.scale {
                    return err(
                        Some("J-04"),
                        "order 的读数候选集或刻度不同：指纹不同即不可比",
                        sp,
                    );
                }
            }
        }
        Ok(Value::list(
            self.order_tiers(caps.read_answer(), &rs)
                .into_iter()
                .map(|tier| {
                    Value::list(
                        tier.into_iter()
                            .map(|i| Value::Int(i as i64, Taint::Trusted.into()))
                            .collect(),
                    )
                })
                .collect(),
        ))
    }
}
