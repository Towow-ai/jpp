//! Host wiring only: fixed observations, local action registration and report I/O.
//! 动作的事实与实现在 lib 目标 `jpp::actions` 的表里（B150；比赛块 C-1、C-1b；R2b 六个
//! `graph:*` 动作已接入该表，见 `crates/jpp/src/actions/mod.rs::BUILTIN_ACTIONS`），这里只注册。
use jpp::{Program, effects::CalibStore, interp::ActionRegistry, ledger::Ledger};
use serde_json::{Value, json};

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
    let ctx = jpp::actions::Ctx::default();
    let mut actions = ActionRegistry::new();
    jpp::actions::register_all(&mut actions, &ctx, replay_only);
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
        "local_checks": *ctx.checks.borrow(),
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
