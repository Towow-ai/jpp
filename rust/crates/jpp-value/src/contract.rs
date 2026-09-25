//! 组合封闭性契约（B17；`20` §2.3 L1 `jpp-value` 的 `contract`、§3.6）：契约值的唯一构造。
//!
//! 步 8a-3（R）：运行时的 `outcome_value` / `pending_entry` 原样搬来，并在构造时核两条不变量：
//! 未决清单的每一项带承担责任的出口（责任随包），证据只存账本键（Text，非空）。
//! 内置构造（sieve、pair、tally、first_k、iterate、outcome）都经 [`build`]；调用者构造的
//! `outcome({...})` 由运行时先做输入规整与面向作者的报错，再经 [`build`]。
//! 依据：B17 三条不变量；`12` §2.12。

use crate::value::{Taint, Value};

/// 契约值的各部分（字段顺序在 [`build`] 里固定）。
pub struct Parts {
    pub kind: String,
    pub value: Value,
    pub pending: Vec<Value>,
    pub evidence: Vec<Value>,
    pub resume: Value,
    pub spent: (i64, f64),
    pub detail: Value,
    pub purpose: Value,
}

/// 构造被拒：不变量不成立（内置构造不会走到；走到就是运行时的缺陷）。
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    pub rule: &'static str,
    pub message: String,
}

/// 未决清单的一项（B81 (c)，步 25-0）：带 `exit` 的元素记录本身（缺 `cause` 时由出口补上）；
/// 没有元素时是裸出口包成的 `{exit, cause}`。此前是 `{element, exit, cause}` 两层。
pub fn pending_entry(element: Value, exit: &Value) -> Value {
    let cause = match exit {
        Value::Exit(e) | Value::Duty(e) => Value::text(&e.cause()),
        _ => Value::Unit,
    };
    match element {
        Value::Record(fs) if fs.iter().any(|(k, _)| k == "exit") => {
            if fs.iter().any(|(k, _)| k == "cause") {
                Value::Record(fs)
            } else {
                let mut v: Vec<(String, Value)> = fs.iter().cloned().collect();
                v.push(("cause".into(), cause));
                Value::record(v)
            }
        }
        _ => Value::record(vec![("exit".into(), exit.clone()), ("cause".into(), cause)]),
    }
}

/// 契约值的唯一构造：`{kind, value, pending, evidence, resume, spent, detail, purpose}`。
pub fn build(p: Parts) -> Result<Value, Rejected> {
    // 依据：B17 不变量 2（未决责任随包转移）：每一项都要带出口
    for (i, e) in p.pending.iter().enumerate() {
        if !matches!(e.get("exit"), Some(Value::Exit(_) | Value::Duty(_))) {
            return Err(Rejected {
                rule: "J-05",
                message: format!("契约值的 pending 第 {i} 项没有承担责任的出口"),
            });
        }
    }
    // 依据：B17 不变量 3（证据只存账本键）
    for (i, k) in p.evidence.iter().enumerate() {
        if !matches!(k, Value::Text(t, _) if !t.is_empty()) {
            return Err(Rejected {
                rule: "E-evidence",
                message: format!("契约值的 evidence 第 {i} 项不是账本键"),
            });
        }
    }
    // 续接总是记录（空记录 = 没有停在半路、也没有继续方法），调用者可以用 has 查
    let resume = if matches!(p.resume, Value::Unit) {
        Value::record(vec![])
    } else {
        p.resume
    };
    Ok(Value::record(vec![
        ("kind".into(), Value::text(&p.kind)),
        ("value".into(), p.value),
        ("pending".into(), Value::list(p.pending)),
        ("evidence".into(), Value::list(p.evidence)),
        ("resume".into(), resume),
        (
            "spent".into(),
            Value::record(vec![
                ("calls".into(), Value::Int(p.spent.0, Taint::Trusted.into())),
                ("usd".into(), Value::Float(p.spent.1, Taint::Trusted.into())),
            ]),
        ),
        ("detail".into(), p.detail),
        ("purpose".into(), p.purpose),
    ]))
}
