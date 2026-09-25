//! 步 3（B35）：只凭账本重放是审计重现。
//! 1. 账本里记过的调用照记录计入预算：首跑在哪里预算停机，重放就在哪里停，结果与首跑相同、新增调用 0；
//! 2. 账本缺记录报 `E-replay`（致命，不进 cause，不挂起）。
//!
//! 依据：12 §2.3 B35 注；21 §三·2 步 3。
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn jpp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-b35-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn replay_stops_where_the_first_run_stopped_on_budget() {
    let d = tmp("budget");
    let ledger = d.join("ledger.json");
    let first = json(&jpp(&[
        "run",
        "examples/sieve-budget.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--ledger-out",
        ledger.to_str().unwrap(),
    ]));
    let replay = json(&jpp(&[
        "run",
        "examples/sieve-budget.jpp",
        "--fixtures",
        "examples/fixtures/sieve.json",
        "--replay",
        ledger.to_str().unwrap(),
    ]));
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["value"], first["value"]);
    assert_eq!(replay["pending"], first["pending"]);
    assert_eq!(replay["status"], first["status"]);
}

#[test]
fn replay_missing_record_is_e_replay() {
    let d = tmp("missing");
    let src_a = d.join("a.jpp");
    let src_b = d.join("b.jpp");
    let head = "budget {calls: 5, cost: 0.01, depth: 256};\nlet s = state(mat(\"会员价比原价便宜了五十元。\"));\n";
    std::fs::write(
        &src_a,
        format!("{head}cut(judge(s, test(\"这段话给出了具体价格或折扣吗？\", \"price\")))\n"),
    )
    .unwrap();
    std::fs::write(
        &src_b,
        format!(
            "{head}{{a: cut(judge(s, test(\"这段话给出了具体价格或折扣吗？\", \"price\"))), b: cut(judge(s, test(\"这段话提到了会员吗？\", \"member\")))}}\n"
        ),
    )
    .unwrap();
    let ledger = d.join("ledger.json");
    json(&jpp(&[
        "run",
        src_a.to_str().unwrap(),
        "--fixtures",
        "examples/fixtures/tally.json",
        "--ledger-out",
        ledger.to_str().unwrap(),
    ]));
    let out = jpp(&[
        "run",
        src_b.to_str().unwrap(),
        "--replay",
        ledger.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "缺记录的重放应当失败");
    assert!(stderr.contains("E-replay"), "{stderr}");
}
