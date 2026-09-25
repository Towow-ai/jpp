//! 复核名额分配 `allocate` 与未决上界 `unsure_bound`。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_allocate(
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
        // 刷新点：它要读「离线多远」，那是答案上的量（12 §2.2 的「宿主读内容」同一类）
        self.flush("allocate")?;
        let rs = self.readings_of(&args[0], "allocate", sp)?;
        // **这里也在用线**（`uncertainty` 读 `lines_for`），所以漂移要在这里也报。
        self.报漂移_批(&rs, sp);
        let Value::Int(k, _) = &args[1] else {
            return err(
                Some("E-rt-arg"),
                "allocate(读数们, k: Int)：k 是复核名额，通常取 budget.escalate",
                sp,
            );
        };
        if *k < 0 {
            return err(
                Some("E-rt-arg"),
                format!("allocate: k 必须是非负整数（通常取 budget.escalate），收到 {k}"),
                sp,
            );
        }
        let rep = crate::strength::allocate_report(
            self.calib,
            &*caps.read_answer().answers(self),
            &rs,
            *k as usize,
        );
        // **算不出的那些要对程序可见，不能只进 trace**（J-15 那条纪律）。
        if !rep.算不出.is_empty() {
            self.trace.warn(format!(
                    "W-untested: @{} uncertainty 对 {} 条读数算不出（它们的校准键没有上岗记录，没有属于自己的线），                         这些读数不进 allocate 的榜。按 J-15 不阻塞。修法【作者可改】：按 `算不出` 那一栏另行处置（那一栏就在 allocate 的返回值里）；【需接线人】给那些键写上岗记录、或给 CLI 接上 --calib/--profile",
                    sp.start, rep.算不出.len()
                ));
        }
        // **返回记录而不是裸表**：`算不出` 那一栏必须对程序可见。
        // 以前返回一张下标表，而冷键上那张表恰好是 `[0, 1, …]`——
        // **一个与不确定性无关、却看起来像答案的答案**。
        Ok(Value::Record(Rc::new(vec![
            (
                "picked".into(),
                Value::List(Rc::new(
                    rep.picked
                        .into_iter()
                        .map(|i| Value::Int(i as i64, Taint::Trusted.into()))
                        .collect(),
                )),
            ),
            (
                "算不出".into(),
                Value::List(Rc::new(
                    rep.算不出
                        .into_iter()
                        .map(|i| Value::Int(i as i64, Taint::Trusted.into()))
                        .collect(),
                )),
            ),
        ])))
    }
    #[allow(unused_variables)]
    pub(crate) fn b_unsure_bound(
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
        arity(1)?;
        // 只读校准记录的 unsure_rate，不读答案——但读数要先就绪才谈得上「这批读数」
        self.flush("unsure_bound")?;
        let rs = self.readings_of(&args[0], "unsure_bound", sp)?;
        // **J-10 的上界建在 `unsure_rate` 上，而那是标注集上的实测值**——
        // 分布移开之后它不再成立。**漂了要报，否则上界静默失效。**
        self.报漂移_批(&rs, sp);
        let b = crate::strength::unsure_bound(self.calib, &rs);
        Ok(Value::Record(Rc::new(vec![
            ("n".into(), Value::Int(b.n as i64, Taint::Trusted.into())),
            (
                "union_bound".into(),
                Value::Float(b.union_bound, Taint::Trusted.into()),
            ),
            // **不给裸浮点。** Rust 侧的 `仅供参考` 类型闸只拦得住 Rust 调用者，
            // 而**这门语言唯一的用户拿到的是 `Value::Float`，可以直接当判据，没有任何东西会红**
            // ——「替身上成立、真机上失效」，只是这次的「替身」是宿主语言。
            // 交成 `Text`：看得见、打得出，**比不了大小、做不了算术**，
            // 与 Rust 侧那道闸是同一条纪律在同一侧生效。
            (
                "independent_any".into(),
                Value::text(&format!(
                    "{:.4}（仅供参考，不可作判据；判据是 union_bound）",
                    b.independent_any.as_reference_only()
                )),
            ),
            (
                "n_unknown".into(),
                Value::Int(b.n_unknown as i64, Taint::Trusted.into()),
            ),
        ])))
    }
}
