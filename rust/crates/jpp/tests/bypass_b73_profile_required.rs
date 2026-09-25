//! B73（`21` 步 15d-0）：真机运行必须带能力画像。
//! `21` 写作 `tests/bypass/b73_profile_required.rs`；本仓库的旁路测试按约定放在各 crate 的 `tests/bypass_*.rs`。
use std::{fs, path::PathBuf, process::Command};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-b73-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("p.jpp"), "budget {calls: 0, cost: 0};\n1").unwrap();
    d
}

/// HOME 指向空目录：没有 `~/.typesafe-key`，任何情况下都不会发出真实请求。
fn jpp(dir: &PathBuf, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .env("HOME", dir)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

fn shipped_profiles() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../profiles")
}

#[test]
fn 真机无画像_报e_profile_missing并列出试过的路径() {
    let d = scratch("missing");
    let empty = d.join("空目录");
    fs::create_dir_all(&empty).unwrap();
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--backend",
            "live",
            "--profiles-dir",
            empty.to_str().unwrap(),
        ],
    );
    assert!(!ok, "没有画像不该跑");
    assert!(err.contains("E-profile-missing"), "{err}");
    assert!(
        err.contains(&empty.join("jev-1.13.0.json").display().to_string()),
        "报文要写明试过的路径：{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 真机缺省找可执行文件旁的profiles() {
    let d = scratch("beside");
    let beside = PathBuf::from(env!("CARGO_BIN_EXE_jpp"))
        .parent()
        .unwrap()
        .join("profiles")
        .join("jev-1.13.0.json");
    if beside.exists() {
        return; // 构建目录里有人放了画像：这条前提不成立，不测
    }
    let (ok, _, err) = jpp(
        &d,
        &["run", "p.jpp", "--backend", "live", "--model", "jev-1.13.0"],
    );
    assert!(!ok);
    assert!(
        err.contains("E-profile-missing") && err.contains(&beside.display().to_string()),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 真机有画像_不报e_profile_missing() {
    let d = scratch("found");
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--backend",
            "live",
            "--profiles-dir",
            shipped_profiles().to_str().unwrap(),
        ],
    );
    // 画像解析通过后才轮到真机客户端：没开 `live` feature 或没有凭据时在那里失败，不是缺画像
    assert!(!ok);
    assert!(!err.contains("E-profile-missing"), "{err}");
    assert!(
        err.contains("--features live") || err.contains("typesafe-key"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 真机显式画像读不到_报错不回退() {
    let d = scratch("bad-path");
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--backend",
            "live",
            "--profile",
            "不存在.json",
        ],
    );
    assert!(!ok);
    assert!(err.contains("不存在.json"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 真机续接同样要画像() {
    let d = scratch("resume");
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.json"]);
    assert!(ok, "{err}");
    let empty = d.join("空目录");
    fs::create_dir_all(&empty).unwrap();
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--resume",
            "l.json",
            "--backend",
            "live",
            "--profiles-dir",
            empty.to_str().unwrap(),
        ],
    );
    assert!(!ok);
    assert!(err.contains("E-profile-missing"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 固定观察无画像_照跑并打印提示() {
    let d = scratch("fixed");
    let (ok, out, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(ok, "{err}");
    assert!(out.contains("\"value\": 1"), "{out}");
    assert!(
        // 步 15d-2：没有代码兜底了，提示改为「画像字段全部未测」
        err.contains("未加载") && err.contains("全部未测"),
        "提示要无条件打印：{err:?}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn 重放带backend_live不要求画像() {
    let d = scratch("replay");
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--ledger-out", "l.json"]);
    assert!(ok, "{err}");
    let (ok, _, err) = jpp(
        &d,
        &["run", "p.jpp", "--replay", "l.json", "--backend", "live"],
    );
    assert!(ok, "重放不发调用，不要求画像：{err}");
    let _ = fs::remove_dir_all(&d);
}
