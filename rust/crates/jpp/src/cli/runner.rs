//! Host wiring only: fixed observations, local action registration and report I/O.
use jpp::{
    Program,
    effects::CalibStore,
    interp::{ActionRegistry, TaintOut, json_to_value},
    ledger::Ledger,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

/// CLI 唯一注册的三个内置动作：名字、是否可逆、输出 taint 规则（步 24c，B108 已知限制收口）。
/// 单一来源：`execute()` 的三处 `.register()` 调用与 [`builtin_action_table`] 都从这里取事实，
/// 不各写一份、避免漂移。
pub(crate) const BUILTIN_ACTIONS: &[(&str, bool, TaintOut)] = &[
    ("record_check", true, TaintOut::Inherit),
    ("read_json", true, TaintOut::Untrusted),
    ("write_json", false, TaintOut::Inherit),
];

fn action_fact(name: &str) -> (bool, TaintOut) {
    BUILTIN_ACTIONS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, r, t)| (*r, *t))
        .expect("action registered below BUILTIN_ACTIONS")
}

/// CLI 已知动作表（步 24c）：不依赖用户输入或实际 `ActionRegistry`（`check` 不构造它），
/// `check` 与 `run` 的预跑诊断都能随时拿到——J-08 静态子面据此对可逆动作不报、
/// 对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`。
pub(crate) fn builtin_action_table() -> jpp::check::ActionTable {
    let mut t = jpp::check::ActionTable::default();
    for (name, reversible, taint_out) in BUILTIN_ACTIONS {
        t.actions.insert(
            (*name).to_string(),
            jpp::check::ActionFacts {
                reversible: *reversible,
                output_untrusted: *taint_out == TaintOut::Untrusted,
            },
        );
    }
    t
}

pub fn execute(
    program: &Program,
    // 端口表（步 15b、15c）：固定观察、真机或重放
    ports: jpp_effects::Ports<'_>,
    calibrations: &CalibStore,
    ledger: &mut Ledger,
    replay_only: bool,
    // 越界接线：把这一趟的证据带出去，供 `--calib-out` 折进记录。
    // **J-03 决定了程序永远写不了线**，所以「跑程序 → 积累证据 → 认证 → 用上」这条环
    // **只能靠宿主/CLI 闭合**——而这是出料那一半。
    evidence_out: &mut Vec<(String, jpp::effects::Sample)>,
    // 宿主入口（B105；步 14b-0 起 `--input` 产一条值条目）；空入口与不设相同，逐字节不变
    entry: &jpp::EntryArgs,
) -> Result<Value, jpp::Error> {
    let checks = Rc::new(RefCell::new(Vec::new()));
    let log = checks.clone();
    let mut actions = ActionRegistry::new();
    let (rc_reversible, rc_taint) = action_fact("record_check");
    let (rj_reversible, rj_taint) = action_fact("read_json");
    let (wj_reversible, wj_taint) = action_fact("write_json");
    // The source computes validity. This action only records and returns its value.
    actions.register("record_check", 0.0, rc_reversible, rc_taint, move |args| {
        if replay_only {
            return Err("replay has no completed record for record_check".into());
        }
        if args.len() != 1 {
            return Err("record_check expects one source-computed record".into());
        }
        log.borrow_mut().push(args[0].to_json());
        Ok(args[0].clone())
    });
    actions.register("read_json", 0.0, rj_reversible, rj_taint, move |args| {
        if replay_only {
            return Err("replay has no completed record for read_json".into());
        }
        let [jpp::value::Value::Text(path, _)] = args else {
            return Err("read_json expects one file path".into());
        };
        let bytes = std::fs::read(path.as_ref()).map_err(|e| format!("{path}: {e}"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}"))?;
        // Reject unsigned integers that the core's JSON adapter cannot represent as Int.
        validate_numbers(&value)?;
        Ok(json_to_value(&value))
    });
    actions.register("write_json", 0.0, wj_reversible, wj_taint, move |args| {
        if replay_only {
            return Err("replay has no completed record for write_json".into());
        }
        let [jpp::value::Value::Text(path, _), value] = args else {
            return Err("write_json expects a file path and a value".into());
        };
        let bytes = serde_json::to_vec_pretty(&value.to_json()).map_err(|e| e.to_string())?;
        std::fs::write(path.as_ref(), bytes).map_err(|e| format!("{path}: {e}"))?;
        Ok(value.clone())
    });
    // 重放是审计重现（B35）：账本记过的调用照记录计预算，缺记录即 E-replay；续跑与首跑走 run
    let session = jpp::Session::new(ports, calibrations, &actions);
    let outcome = if replay_only {
        session.replay(program, entry, ledger)?
    } else {
        // 首跑与续跑同一路径：`ledger` 为空即首跑，为上一趟账本即续跑
        session.resume(program, entry, ledger)?
    };
    evidence_out.extend(outcome.evidence.iter().cloned());
    // J-10 的静态告警只有带校准记录的那次检查报得出来（在 `jpp::run` 里），
    // CLI 执行前那次检查没有记录、报不出它；这里打到 stderr，`--output` 时终端也看得见。
    for w in outcome
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("J-10"))
    {
        eprintln!("warning: {w}");
    }
    let mut report = json!({
        "mode": "fixed observations; no model API requests",
        "status": if outcome.pending.is_empty() { "returned" } else { "pending" },
        "value": outcome.value_json(),
        "pending": outcome.pending,
        "returned_unsure": outcome.returned_unsure,
        "cost": {"calls": outcome.cost.calls, "replayed": outcome.cost.replayed,
                 "tokens": outcome.cost.tokens, "usd": outcome.cost.usd, "asks": outcome.cost.asks},
        "trace": outcome.trace,
        "local_checks": *checks.borrow(),
    });
    // 停岗候选（B25）只在有时出现，默认输出逐字节不变
    if !outcome.suspend_candidates.is_empty() {
        report["suspend_candidates"] = json!(outcome.suspend_candidates);
    }
    // 逐出口记线等级（步 20f）：只在运行里有 `cut` 出口时出现。出口不进账本（`20` §3.7(1)），
    // 只凭账本重放时按账本 `CalibUsed` 条目（账本 v3；v2 在头行 `calib_used`）的记录重算出同一张表
    // B107、B120 (a)（步 20h-2）：本趟问过的题（题面、填法、精化题类；不带读数）
    if !outcome.questions.is_empty() {
        report["questions"] = json!(outcome.questions);
    }
    if !outcome.exits.is_empty() {
        report["exits"] = json!(outcome.exits);
    }
    // 预算停机（B93，步 22-0）：只在耗尽时出现，默认输出逐字节不变
    if let Some(b) = &outcome.budget {
        report["budget"] =
            json!({"exhausted": b.exhausted, "unsent": b.unsent, "first_site": b.first_site});
    }
    // 宿主入口段（步 14b-1，B108）：直接复用 `Program.entry.params` 的既有序列化（`EntryParam`
    // 的 name/kind/taint），无入口为空数组；`purpose`（若给）已经是 params[0]（B58/17b）。
    report["entry"] = json!(program.entry.params);
    Ok(report)
}

pub fn validate_numbers(value: &Value) -> Result<(), String> {
    match value {
        Value::Number(n) if n.is_u64() && n.as_i64().is_none() => {
            Err("JSON integer exceeds J++ Int range".into())
        }
        Value::Array(items) => items.iter().try_for_each(validate_numbers),
        Value::Object(items) => items.values().try_for_each(validate_numbers),
        _ => Ok(()),
    }
}
