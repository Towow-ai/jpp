//! 集合聚合 `tally` 与输入顺序中的前 k 个 `first_k`（施工件 f）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。
//! 步 25-2b（B131）：聚合出口改经合成构造 `compose` 签发（`any`/`all`/`first`），这两个构造自己不再需要
//! 任何内核能力；25-1 的「聚合出口不作放行证据」随之移进 `compose`（保持到步 25-9）。

use crate::constructs::compose::{分量, 合成请求, 规则};
use crate::*;

/// 契约值条目的出口（有则取）：合成的分量出口与被吸收的未决都从这里取
fn 条目出口(e: &Value) -> Option<Rc<Exit>> {
    match e.get("exit") {
        Some(Value::Exit(x)) => Some(x),
        _ => None,
    }
}

/// 未决条目的原因（缺或不是文本时取 `band`，与步 25-2b 前 `tally`、`first_k` 相同）
fn 条目原因(e: &Value) -> String {
    match e.get("cause") {
        Some(Value::Text(t, _)) => t.to_string(),
        _ => "band".into(),
    }
}

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_tally(
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
        // 集合聚合（05 §1 `agg(S, op)` 的存在 / 全部 / 计数，施工件 f）：吃一个契约值（通常是 sieve 的）。
        // 精确计算，不是概率：计数给区间 [act, act + 未决]，未决里 cause=budget 的是未观察项；
        // 存在、全部是三值出口——结论取决于未决元素时给 unsure。
        // 输入的未决：结论被它们挡住时并入聚合出口（记为已消费），聚合出口进新的未决清单；
        // 没挡住结论时原样带进新契约（13 §3：不许无声消失）。
        arity(1)?;
        let r = &args[0];
        if !is_outcome(r) {
            return err(
                Some("E-rt-arg"),
                format!(
                    "tally 收一个契约值（sieve / pair / outcome 的结果），收到 {}",
                    r.type_name()
                ),
                sp,
            );
        }
        // 步 1（K1，放行方向）：调用者用 outcome 重包契约值时若漏写 detail.ignore，
        // 已决否定会被当成「没有」，all 从 ignore 静默翻成 act。算不出来的必须有人明确说
        // 调用者构造的契约值没有 ignore 键即报错；内置构造各有定义，不受影响。
        // 依据：20 §11.6 第 19 条；21 §三·2 步 1；12 §2.12（B17 契约）。
        if matches!(r.get("kind"), Some(Value::Text(k, _)) if k.as_ref() == "outcome")
            && r.get("detail").and_then(|d| d.get("ignore")).is_none()
        {
            return err(
                Some("E-tally-missing-rejected"),
                "tally 收到调用者构造的契约值，但 detail 里没有 ignore（已决否定）：不写会把「全部」算成成立。修法：抄入原构造的 detail.ignore；确实没有已决否定就显式写 detail: {ignore: []}",
                sp,
            );
        }
        let act = list_of(r.get("value"));
        let ignore = list_of(r.get("detail").and_then(|d| d.get("ignore")));
        let pend = list_of(r.get("pending"));
        let is_budget =
            |e: &Value| matches!(e.get("cause"), Some(Value::Text(t, _)) if t.as_ref() == "budget");
        let no = pend.iter().filter(|e| is_budget(e)).count() as i64;
        let nu = pend.len() as i64 - no;
        let (na, ni) = (act.len() as i64, ignore.len() as i64);
        // B131：分量种类按列表成员定（接受 Act、否定 Ignore、未决 Unsure(原因)），有出口的进 `parts`；
        // 调用者自造的契约值里没有出口的普通值也照样计数（`过程记录/工程-步25-2b.md` Q14）
        let 分量们 = || -> Vec<分量> {
            act.iter()
                .map(|e| (ExitKind::Act, e))
                .chain(ignore.iter().map(|e| (ExitKind::Ignore, e)))
                .chain(pend.iter().map(|e| (ExitKind::Unsure(条目原因(e)), e)))
                .map(|(种类, e)| 分量 {
                    种类,
                    出口: 条目出口(e),
                })
                .collect()
        };
        // 结论被未决挡住时，元素的未决责任并入聚合出口（B131 (3)，与步 25-2b 前同）
        let 吸收: Vec<Rc<Exit>> = pend.iter().filter_map(条目出口).collect();
        let 合成 = |me: &mut Self, 规则, 题键| {
            me.调合成(
                合成请求 {
                    规则,
                    分量: 分量们(),
                    吸收: &吸收,
                    op: Op::Test,
                    题键,
                    已决标签: "tally:decided",
                    吸收标签: "tally",
                },
                sp,
            )
        };
        let ex = 合成(self, 规则::Any, "tally:exists")?;
        let al = 合成(self, 规则::All, "tally:all")?;
        let 未决的 = |x: &Value| matches!(x, Value::Exit(e) if e.is_unsure());
        let pending = if 未决的(&ex) || 未决的(&al) {
            [&ex, &al]
                .into_iter()
                .filter(|x| 未决的(x))
                .map(|x| Self::pending_entry(Value::Unit, x))
                .collect()
        } else {
            pend.clone()
        };
        let value = Value::record(vec![
            (
                "n".into(),
                Value::Int(na + ni + nu + no, Taint::Trusted.into()),
            ),
            ("act".into(), Value::Int(na, Taint::Trusted.into())),
            ("ignore".into(), Value::Int(ni, Taint::Trusted.into())),
            ("unsure".into(), Value::Int(nu, Taint::Trusted.into())),
            ("unobserved".into(), Value::Int(no, Taint::Trusted.into())),
            (
                "count".into(),
                Value::list(vec![
                    Value::Int(na, Taint::Trusted.into()),
                    Value::Int(na + nu + no, Taint::Trusted.into()),
                ]),
            ),
            (
                "complete".into(),
                Value::Bool(nu == 0 && no == 0, Taint::Trusted.into(), GuardEv::EMPTY),
            ),
            ("exists".into(), ex),
            ("all".into(), al),
        ]);
        Self::outcome_value(
            "tally",
            value,
            pending,
            list_of(r.get("evidence")),
            r.get("resume").unwrap_or(Value::Unit),
            (0, 0.0),
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
    #[allow(unused_variables)]
    pub(crate) fn b_first_k(
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
        // 输入顺序中的前 k 个接受项（交接首包「第一个」语义；05 §1 前 k）：
        // 按原顺序走，遇到未决（含 cause=budget 的未观察项）而还没凑够 k 个，就不能宣称后面的接受项是「前 k 个」。
        // 产出 `{items, exit}`：act = 凑够了 k 个且之前没有挡路的；ignore = 全部观察完、确定不足 k 个；
        // unsure = 被挡住。被挡的位置与原因进续接 `resume`（B17 取舍）。
        // 输入的未决原样带进新契约；被挡住时的 unsure 出口也进未决清单。
        arity(2)?;
        let (r, Value::Int(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "first_k(契约值, k: Int)", sp);
        };
        let k = *k;
        if k <= 0 {
            return err(Some("E-rt-arg"), "first_k 的 k 要是正整数", sp);
        }
        if !is_outcome(r) {
            return err(
                Some("E-rt-arg"),
                format!(
                    "first_k 收一个契约值（sieve 的结果），收到 {}",
                    r.type_name()
                ),
                sp,
            );
        }
        let pend = list_of(r.get("pending"));
        let mut all: Vec<(i64, String, Value)> = vec![];
        for e in list_of(r.get("value")) {
            all.push((0, "act".into(), e));
        }
        for e in list_of(r.get("detail").and_then(|d| d.get("ignore"))) {
            all.push((0, "ignore".into(), e));
        }
        for p in &pend {
            let c = 条目原因(p);
            // B81 (c)（步 25-0）：未决条目就是元素记录本身
            all.push((0, format!("pending:{c}"), p.clone()));
        }
        // 按本次过滤的输入顺序（`pos`）排：链式过滤后 `index` 是原始编号（B81 (a)），输入顺序看 `pos`
        for x in all.iter_mut() {
            x.0 = match x.2.get("pos") {
                Some(Value::Int(i, _)) => i,
                _ => {
                    return err(
                        Some("E-rt-arg"),
                        "first_k：元素缺 pos（只收 sieve 产物的元素）",
                        sp,
                    );
                }
            };
        }
        all.sort_by_key(|x| x.0);
        // 产出与续接：按序读出前 k 个接受项，被挡住时记位置与原因（B17 取舍）。出口的种类由 `compose` 的
        // `first` 规则从同一序列算出（B131；B3 的 no_candidate、rejected_all 在规则里）。
        let mut items = vec![];
        let mut resume = Value::Unit;
        for (idx, tag, e) in &all {
            match tag.as_str() {
                "act" => {
                    items.push(e.clone());
                    if items.len() as i64 == k {
                        break;
                    }
                }
                "ignore" => {}
                t => {
                    let c = t.trim_start_matches("pending:").to_string();
                    resume = Value::record(vec![
                        ("reason".into(), Value::text("blocked")),
                        ("at".into(), Value::Int(*idx, Taint::Trusted.into())),
                        ("cause".into(), Value::text(&c)),
                    ]);
                    break;
                }
            }
        }
        let 分量们: Vec<分量> = all
            .iter()
            .map(|(_, tag, e)| 分量 {
                种类: match tag.as_str() {
                    "act" => ExitKind::Act,
                    "ignore" => ExitKind::Ignore,
                    t => ExitKind::Unsure(t.trim_start_matches("pending:").to_string()),
                },
                出口: 条目出口(e),
            })
            .collect();
        // `first_k` 被挡住时不吸收元素的未决（与步 25-2b 前同；B131 (3) 是否适用待定，Q15）
        let ex = self.调合成(
            合成请求 {
                规则: 规则::First(k as usize),
                分量: 分量们,
                吸收: &[],
                op: Op::Test,
                题键: "first_k",
                已决标签: "first_k:decided",
                吸收标签: "first_k",
            },
            sp,
        )?;
        let mut pending = pend.clone();
        if let Value::Exit(e) = &ex {
            if e.is_unsure() {
                pending.push(Self::pending_entry(Value::Unit, &ex));
            }
        }
        let value = Value::record(vec![
            ("items".into(), Value::list(items)),
            ("exit".into(), ex),
            ("k".into(), Value::Int(k, Taint::Trusted.into())),
        ]);
        Self::outcome_value(
            "first_k",
            value,
            pending,
            list_of(r.get("evidence")),
            resume,
            (0, 0.0),
            Value::record(vec![]),
            Value::Unit,
            sp,
        )
    }
}
