//! R2a：`do("check_tests", [code, tests, timeout_s], seq)` 端到端行为。
//! 预注册：`地基/过程记录/工程-比赛R2a.md`；实现：`crates/jpp/src/cli/actions_r2a.rs`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jpp-actions-check-tests-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(cwd: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 首跑记 `passed`/`failed`；只凭账本重放零调用、值逐字段相同。
#[test]
fn check_tests经do调用并且只凭账本重放零调用() {
    let d = tmp("basic");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("check_tests", ["x = 2", ["assert x == 2", "assert x == 3"], 5], 0))
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--output",
            "r1.json",
            "--ledger-out",
            "l.jsonl",
        ],
    );
    assert!(ok, "{err}");
    let r1: Value = serde_json::from_str(&fs::read_to_string(d.join("r1.json")).unwrap()).unwrap();
    assert_eq!(r1["value"]["passed"], 1, "{r1}");
    assert_eq!(r1["value"]["failed"], 1, "{r1}");

    let (ok, err) = jpp(
        &d,
        &["run", "p.jpp", "--replay", "l.jsonl", "--output", "r2.json"],
    );
    assert!(ok, "{err}");
    let r2: Value = serde_json::from_str(&fs::read_to_string(d.join("r2.json")).unwrap()).unwrap();
    assert_eq!(r2["cost"]["calls"], 0, "重放不应新增调用");
    assert_eq!(r2["value"], r1["value"]);

    let _ = fs::remove_dir_all(&d);
}

/// 静态拒绝表对 `code`+`tests` 合并文本同样生效。
#[test]
fn check_tests静态拒绝命中tests参数() {
    let d = tmp("reject");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let r = do("check_tests", ["x = 1", ["import socket"], 5], 0);
is_fail(r)
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"], true, "{r}");
    let _ = fs::remove_dir_all(&d);
}
