//! R2a：`do("exec_py", [code, stdin, timeout_s], seq)` 作为 CLI 宿主动作的端到端行为。
//! 预注册：`地基/过程记录/工程-比赛R2a.md`；实现：`crates/jpp/src/cli/actions_r2a.rs`。
//! 核心逻辑的单测在该文件的 `#[cfg(test)]`（不依赖注册表）；这里只钉住「经 `do` 真的能调用、
//! 一切进账本、只凭账本重放零调用」——设计附注 §二·6「一切进账本」的验收面。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-actions-exec-py-{name}-{}", std::process::id()));
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

/// 首跑：`do("exec_py", …)` 返回 `{stdout, stderr, exit_code, timed_out}`；`cost.calls` 记 1 次。
/// 重放：只给账本，新增调用为 0，`value` 与首跑逐字段相同。
#[test]
fn exec_py经do调用并且只凭账本重放零调用() {
    let d = tmp("basic");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("exec_py", ["print('六加六等于', 6+6)", "", 5], 0))
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
    assert_eq!(r1["status"], "returned");
    assert_eq!(r1["cost"]["calls"], 1);
    assert_eq!(r1["value"]["exit_code"], 0);
    assert_eq!(r1["value"]["timed_out"], false);
    assert!(
        r1["value"]["stdout"]
            .as_str()
            .unwrap()
            .contains("六加六等于 12"),
        "{r1}"
    );

    let (ok, err) = jpp(
        &d,
        &["run", "p.jpp", "--replay", "l.jsonl", "--output", "r2.json"],
    );
    assert!(ok, "{err}");
    let r2: Value = serde_json::from_str(&fs::read_to_string(d.join("r2.json")).unwrap()).unwrap();
    assert_eq!(r2["cost"]["calls"], 0, "重放不应新增调用");
    assert_eq!(r2["value"], r1["value"], "重放的值应与首跑逐字段相同");
    assert_eq!(r2["status"], r1["status"]);

    let _ = fs::remove_dir_all(&d);
}

/// 静态拒绝表命中时，`do` 返回失败值（J-12），程序照常往下走，不是运行期报错、不起子进程。
#[test]
fn exec_py静态拒绝返回失败值而不是运行期错误() {
    let d = tmp("reject");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let r = do("exec_py", ["import os\nos.system('echo boom')", "", 5], 0);
{是失败值: is_fail(r), 理由: if is_fail(r) { text(r) } else { "不该走到这里" }}
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["是失败值"], true, "{r}");
    let reason = r["value"]["理由"].as_str().unwrap_or_default();
    assert!(reason.contains("import os"), "{r}");
    let _ = fs::remove_dir_all(&d);
}

/// 超时是结构化失败（`timed_out: true`），不是拒绝、不是运行期错误。
#[test]
fn exec_py超时给出结构化结果() {
    let d = tmp("timeout");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("exec_py", ["import time\ntime.sleep(2)", "", 0.2], 0))
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["timed_out"], true, "{r}");
    let _ = fs::remove_dir_all(&d);
}
