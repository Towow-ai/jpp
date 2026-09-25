//! `fit` 桥。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_fit(
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
        arity(2)?;
        self.flush("fit")?;
        let Value::Text(name, _) = &args[0] else {
            return err(Some("J-16"), "fit(名字: Text, [读数…])", sp);
        };
        let rs = self.readings_of(&args[1], "fit", sp)?;
        // 依据：12 §3 J-16（fit 只认注册表签名，读数个数与特征数一致）
        let Some(rec) = self.fits.get(name.as_ref()).cloned() else {
            return err(
                Some("J-16"),
                format!(
                    "fit {name} 未注册：fit 只认注册表签名（12:274）。修法：用训练过程注册，或改用 cut"
                ),
                sp,
            );
        };
        if rs.len() != rec.features.len() {
            return err(
                Some("J-16"),
                format!(
                    "fit {name} 期望 {} 个读数，收到 {}",
                    rec.features.len(),
                    rs.len()
                ),
                sp,
            );
        }
        // J-04：输入指纹必须与注册特征**逐项相同**
        for (r, (ck, fk)) in rs.iter().zip(&rec.features) {
            if &r.calib != ck || r.op.phys() != fk {
                return err(
                    Some("J-04"),
                    format!(
                        "fit {name} 的输入指纹（{}, {}）与注册特征（{ck}, {fk}）不同：跨题读数不可比，喂错题就是错",
                        r.calib,
                        r.op.phys()
                    ),
                    sp,
                );
            }
        }
        let ps: Vec<f64> = {
            let ans = &*caps.read_answer().answers(self);
            rs.iter()
                .map(|r| rank_value(ans, r).unwrap_or(0.0))
                .collect()
        };
        let score = (rec.f)(&ps).clamp(0.0, 1.0);
        // 结果是读数：calib 位先留 fit 的名字，真正的线由 cut 的第二参给
        let r = Rc::new(Reading {
            q_hash: format!("fit:{name}"),
            state_hash: rs.first().map(|r| r.state_hash.clone()).unwrap_or_default(),
            op: Op::Test,
            calib: format!("fit:{name}"),
            id: caps.issue_reading().new_reading_id(self),
            fail: None,
            model_id: self.model_id.clone(),
            ledger_key: format!("fit:{name}"),
            over_len: 0,
            scale: vec![],
            perms: std::cell::Cell::new(0),
            mode_share: std::cell::Cell::new(None),
            missing_evidence: vec![],
            state_taint: rs
                .iter()
                .fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
            form_hash: None,
            fp: None,
        });
        // 写答案只经 fill_answer（fit 的结果是一个已答的读数）
        caps.issue_reading()
            .fill_answer(self, &r, Answer::Noul(score));
        Ok(Value::Reading(r))
    }
}
