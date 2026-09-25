//! PR #36 复核 P1 + B164：没有操作系统级沙箱时，`exec_py`/`check_tests`/`exec_sql` 一律拒绝
//! 执行，绝不在沙箱外跑。B164 把这条从「只在运行期报 Fail」升级为「宿主启动时探测，探测不到
//! 就把该动作的 `reversible` 置 false、`check` 报 `E-action-no-sandbox`」——`jpp run`/`check`
//! 因此在**执行前**（静态检查阶段）就失败，不会跑到「返回 `Fail(NoSandbox)` 的值」那一步
//! （实测确认，见过程记录 §六）；运行期那条 `Fail(NoSandbox)` 分支仍然存在（`exec_py_core`等
//! 的 `sandbox::tool().ok_or_else(...)`），是给绕过静态检查的调用路径（如库 API 的
//! `Session::run_unchecked`）留的兜底，CLI 的 `run`/`check` 走不到那里，本文件测不到那条分支——
//! 如实记录这个边界，不假装测过。
//!
//! 用 `JPP_FORCE_NO_SANDBOX` 只作用于**子进程**的环境（`Command::env`，不碰当前测试进程
//! 自己的环境变量）——不会和同一 `cargo test` 二进制里并发跑的其它测试互相干扰；`sandbox::tool()`
//! 每个进程只探测一次（`OnceLock`），子进程是全新进程，探测在其中正常发生。
//! 预注册：`地基/过程记录/工程-执行器动作安全修补.md`；实现：`crates/jpp/src/actions/sandbox.rs`、
//! `crates/jpp-check/src/rules/e_action_no_sandbox.rs`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-no-sandbox-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp_forced_no_sandbox(cwd: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .env("JPP_FORCE_NO_SANDBOX", "1")
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 三个执行器动作，`run` 在检查阶段就该失败、报 `E-action-no-sandbox`，不产生 `--output` 文件
/// （执行根本没开始）。
#[test]
fn 三个执行器动作没有沙箱时run在检查阶段就失败() {
    for (name, args_literal) in [
        ("exec_py", r#"["print(1)", "", 5]"#),
        ("check_tests", r#"["x = 1", ["assert x == 1"], 5]"#),
        ("exec_sql", r#"["nonexistent.db", "select 1"]"#),
    ] {
        let d = tmp(name);
        let src = format!(
            "budget {{calls: 2, cost: 0, depth: 8}};\ncontent(do(\"{name}\", {args_literal}, 0))\n"
        );
        fs::write(d.join("p.jpp"), src).unwrap();
        let (ok, err) = jpp_forced_no_sandbox(&d, &["run", "p.jpp", "--output", "r.json"]);
        assert!(!ok, "{name}: 没有沙箱时 run 应该失败");
        assert!(err.contains("E-action-no-sandbox"), "{name}: {err}");
        assert!(err.contains(name), "{name}: 报文应指名动作：{err}");
        assert!(
            !d.join("r.json").exists(),
            "{name}: 不该产生输出文件（没执行）"
        );
        let _ = fs::remove_dir_all(&d);
    }
}

/// `check`（不只是 `run`）同样在这个阶段报错——B164 (d) 「且 check 报错」。
#[test]
fn 没有沙箱时check也报错() {
    let d = tmp("check");
    let src = "budget {calls: 2, cost: 0, depth: 8};\ncontent(do(\"exec_py\", [\"print(1)\", \"\", 5], 0))\n";
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp_forced_no_sandbox(&d, &["check", "p.jpp"]);
    assert!(!ok, "{err}");
    assert!(err.contains("E-action-no-sandbox"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

/// 不管调用点有没有守卫都报——E-action-no-sandbox 是环境问题，不是放行策略能解决的（B164）。
#[test]
fn 没有沙箱时即使有守卫也报错() {
    let d = tmp("guarded");
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let ok = handle(cut(judge(state(mat("甲")), test("行吗", "k"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
if ok { content(do("exec_py", ["print(1)", "", 5], 0)) } else { "没做" }
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp_forced_no_sandbox(&d, &["check", "p.jpp"]);
    assert!(!ok, "{err}");
    assert!(
        err.contains("E-action-no-sandbox"),
        "有守卫也不该放行，环境问题不是放行策略能解决的：{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 反证：不设 `JPP_FORCE_NO_SANDBOX` 时，同样的 `exec_py` 调用应该正常跑通——确认上面几条
/// 测的是「没有沙箱」这个条件本身，不是别的东西碰巧总是失败。这条本身需要本机有真能用的
/// 沙箱（探测到且冒烟测试通过），公开仓库 CI 上可能没有，跳过时打印原因、不判失败——
/// 与上面几条「测无沙箱路径」的用例不同，那几条不受这条判断影响（主会话原话）。
#[test]
fn exec_py有沙箱时正常执行() {
    if !jpp::actions::sandbox_available() {
        eprintln!(
            "跳过 exec_py有沙箱时正常执行：本机没有可用（探测到且冒烟测试通过）的沙箱工具，环境依赖，非失败"
        );
        return;
    }
    let d = tmp("exec-py-with-sandbox");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("exec_py", ["print(1+1)", "", 5], 0))
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .args(["run", "p.jpp", "--output", "r.json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["stdout"], "2\n", "{r}");
    let _ = fs::remove_dir_all(&d);
}
