//! 公开仓库 Codex 评审（Towow-ai/jpp PR #28）指出的 CLI 问题的回归测试。
//! CLI regression tests for the Codex review comments on Towow-ai/jpp PR #28.
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-review-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &PathBuf, args: &[&str]) -> std::process::Output {
    // HOME 指向空目录：没有 ~/.typesafe-key，保证测到的是「重放不碰真实后端」
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .env("HOME", dir)
        .args(args)
        .output()
        .unwrap()
}

/// `--replay` 同时带 `--backend live`：重放不发调用，不应初始化真实后端
/// （没开 `live` feature 或没有凭据时原先直接失败）。
#[test]
fn replay_with_backend_live_does_not_initialize_the_live_client() {
    let d = scratch("replay-live");
    fs::write(d.join("p.jpp"), "budget {calls: 0, cost: 0};\n1").unwrap();
    let out = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = jpp(
        &d,
        &["run", "p.jpp", "--replay", "l.json", "--backend", "live"],
    );
    assert!(
        out.status.success(),
        "重放不该需要真实后端：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["replay"], true);
    assert_eq!(report["cost"]["calls"], 0);
    let _ = fs::remove_dir_all(&d);
}

/// 带校准记录的 J-10 静态告警：进报告的 `trace.warnings`，也打到 stderr（`--output` 时终端看得见）。
#[test]
fn j10_static_warning_reaches_the_report_and_stderr() {
    let d = scratch("j10");
    fs::write(
        d.join("p.jpp"),
        "budget {calls: 1, cost: 1, unsure: 0.5};\nlet q = test(\"甲\", \"ka\");\nlet r = test(\"乙\", \"kb\");\n1",
    )
    .unwrap();
    let out = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("J-10"), "{stderr}");
    let report: Value = serde_json::from_slice(&fs::read(d.join("r.json")).unwrap()).unwrap();
    assert!(
        report["trace"]["warnings"].to_string().contains("J-10"),
        "{report}"
    );
    let _ = fs::remove_dir_all(&d);
}
