//! 步 24c：CLI `check`（不带 `--input` 也一样）与 `run` 的预跑诊断从此带上 CLI 唯一注册的
//! 三个内置动作（`record_check`/`read_json`/`write_json`），不再是「没有动作表」的降级路径。
//! 两个方向：可逆动作（`record_check`）本不该被 J-08 管，不再误报 `W-guard-untrusted`；
//! 不可逆动作（`write_json`）在检查期就该报 `J-08`（error），不必等运行期真的拦下。
//! 依据：24-0 完成记录「已知限制」一节；预注册见 `地基/过程记录/工程-步24c.md`。

use std::{fs, path::PathBuf, process::Command};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jpp-j08-check-actions-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

fn 守卫程序(动作: &str, 实参: &str) -> String {
    format!(
        "budget {{calls: 2, cost: 1}};\nlet raw = do(\"read_json\", [\"x.json\"], 0);\nlet ok = handle(cut(judge(state(raw), test(\"行吗\", \"k\"))), {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ content(do(\"{动作}\", [{实参}], 0)) }} else {{ \"没做\" }}\n"
    )
}

/// 不可逆动作（`write_json`）：`jpp check`（不带 `--input`）现在就能在检查期报 `J-08`，
/// 不必等运行期真的拦下——这是本批要修的「已知限制」的直接验证。
#[test]
fn check不带input时不可逆动作报j08() {
    let d = scratch("wj");
    fs::write(d.join("p.jpp"), 守卫程序("write_json", "\"y.json\", {}")).unwrap();
    let (ok, err) = jpp(&d, &["check", "p.jpp"]);
    assert!(!ok && err.contains("J-08"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

/// 可逆动作（`record_check`）：`jpp check`（不带 `--input`）现在知道它可逆，不再误报
/// `W-guard-untrusted`（修前：没有表时一律降级警告，不看实际可逆与否）。
#[test]
fn check不带input时可逆动作不报() {
    let d = scratch("rc");
    fs::write(d.join("p.jpp"), 守卫程序("record_check", "{a: 1}")).unwrap();
    let (ok, err) = jpp(&d, &["check", "p.jpp"]);
    assert!(ok, "{err}");
    assert!(
        !err.contains("W-guard-untrusted") && !err.contains("J-08"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// `jpp run`（不带 `--input`、不给夹具）对不可逆动作同样在检查期就报 `J-08`，
/// 一次判断都不花——与 `check` 一致，且比修前的「先打 W-guard-untrusted 再到运行期真正拦下」更早。
#[test]
fn run不带input时不可逆动作在检查期就报() {
    let d = scratch("run-wj");
    fs::write(d.join("p.jpp"), 守卫程序("write_json", "\"y.json\", {}")).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(!ok && err.contains("J-08"), "{err}");
    assert!(!d.join("y.json").exists(), "不可逆 do 不该被执行");
    let _ = fs::remove_dir_all(&d);
}
