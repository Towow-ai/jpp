//! `E-ledger-required`（步 18b，主会话 2026-09-25 裁定）：CLI 跑有不可逆 `do` 的程序，首跑与续接都要
//! `--ledger-out`，否则执行前报错、一个效应都不做；只凭账本重放不要求；没有不可逆 `do` 的程序不受影响。

use std::path::{Path, PathBuf};
use std::process::Command;

fn 目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-e2e-lr-{名}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(d: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(d)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

const 写文件: &str =
    "budget {calls: 2, cost: 0};\nis_fail(do(\"write_json\", [\"out.json\", 1], 0))\n";

#[test]
fn 不可逆动作_不给账本文件即停_给了照常() {
    let d = 目录("wj");
    std::fs::write(d.join("p.jpp"), 写文件).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(
        !ok && err.contains("E-ledger-required") && err.contains("--ledger-out"),
        "{err}"
    );
    assert!(err.contains("write_json"), "报文写出动作名：{err}");
    assert!(!d.join("out.json").exists(), "执行前停下，动作没做");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.jsonl"]);
    assert!(ok, "{err}");
    assert!(d.join("out.json").exists());
    // 续接同样要求
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--resume", "l.jsonl"]);
    assert!(!ok && err.contains("E-ledger-required"), "{err}");
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--resume",
            "l.jsonl",
            "--ledger-out",
            "l2.jsonl",
        ],
    );
    assert!(ok, "{err}");
    // 只凭账本重放不要求
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--replay", "l.jsonl"]);
    assert!(ok, "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 没有不可逆动作的程序不要求() {
    let d = 目录("rc");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\ncontent(do(\"record_check\", [{a: 1}], 0))\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(ok, "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn 动作名不是字面量按不可逆处理() {
    let d = 目录("dyn");
    std::fs::write(
        d.join("p.jpp"),
        "budget {calls: 2, cost: 0};\nlet n = \"record_check\";\ncontent(do(n, [{a: 1}], 0))\n",
    )
    .unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(!ok && err.contains("E-ledger-required"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}
