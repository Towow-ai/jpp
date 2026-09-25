//! R2a 续：`do("exec_sql", [db, sql], seq)` 端到端行为（24e-1 同一块的补齐项）。
//! 预注册：`地基/过程记录/工程-比赛R2a-exec_sql.md`；实现：`crates/jpp/src/actions/exec.rs`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "jpp-actions-exec-sql-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 建一个带两行数据的 sqlite 库（用系统 `python3` 的标准库 `sqlite3`，不引入新依赖）。
fn make_test_db(dir: &Path) -> PathBuf {
    let db = dir.join("t.db");
    let code = format!(
        "import sqlite3\n\
         conn = sqlite3.connect({db:?})\n\
         conn.execute('create table t (id integer, name text)')\n\
         conn.execute('insert into t values (1, \"a\")')\n\
         conn.execute('insert into t values (2, \"b\")')\n\
         conn.commit()\n\
         conn.close()\n",
        db = db.to_string_lossy()
    );
    let out = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .output()
        .expect("python3 应可用");
    assert!(
        out.status.success(),
        "建测试库失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    db
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

fn write_program(dir: &Path, db: &Path, sql: &str) {
    let src = format!(
        "budget {{calls: 2, cost: 0, depth: 8}};\ncontent(do(\"exec_sql\", [{db:?}, {sql:?}], 0))\n",
        db = db.to_string_lossy(),
        sql = sql,
    );
    fs::write(dir.join("p.jpp"), src).unwrap();
}

/// B164：授权回调拒绝是 `Fail(Denied)`，`content()` 只收材料——不能像成功路径那样直接
/// `content(do(...))`，要走 `is_fail`/`text` 那条惯用法（同 `exec_py静态拒绝返回失败值`）。
fn write_program_expect_fail(dir: &Path, db: &Path, sql: &str) {
    let src = format!(
        "budget {{calls: 2, cost: 0, depth: 8}};\n\
         let r = do(\"exec_sql\", [{db:?}, {sql:?}], 0);\n\
         {{是失败值: is_fail(r), 理由: if is_fail(r) {{ text(r) }} else {{ \"不该走到这里\" }}}}\n",
        db = db.to_string_lossy(),
        sql = sql,
    );
    fs::write(dir.join("p.jpp"), src).unwrap();
}

/// `do("exec_sql", …)` 字面出现在程序里，检查期就要求本机有真的能跑通的沙箱
/// （`E-action-no-sandbox`，B164；连 `坏SQL` 这种只想测内容性错误的用例也要先过这一关，
/// 因为检查期不看参数内容，只看动作名字面量）。复用生产同一套探测（内含真实冒烟测试，
/// 不是只看文件存不存在；主会话复核追加，公开仓库 CI 上沙箱工具可能存在但跑不起来）。
macro_rules! require_sandbox_or_skip {
    () => {
        if !jpp::actions::sandbox_available() {
            eprintln!(
                "跳过：本机没有可用（探测到且冒烟测试通过）的沙箱工具，环境依赖，非失败"
            );
            return;
        }
    };
}

/// 读：正常 `select`，`columns`/`rows` 正确，`error` 为空。
#[test]
fn exec_sql读() {
    require_sandbox_or_skip!();
    let d = tmp("read");
    let db = make_test_db(&d);
    write_program(&d, &db, "select id, name from t order by id");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["error"], Value::Null, "{r}");
    assert_eq!(
        r["value"]["columns"],
        serde_json::json!(["id", "name"]),
        "{r}"
    );
    assert_eq!(
        r["value"]["rows"],
        serde_json::json!([[1, "a"], [2, "b"]]),
        "{r}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 写被拒：只读连接上的 `insert` 报 `Fail(Denied)`（B164：安全边界，不是内容性错误），不
/// panic；库文件内容不变（真的没写进去，不是应用层假装拒绝）。
#[test]
fn exec_sql写被授权回调拒绝() {
    require_sandbox_or_skip!();
    let d = tmp("write");
    let db = make_test_db(&d);
    let before = fs::read(&db).unwrap();
    write_program_expect_fail(&d, &db, "insert into t values (3, 'c')");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["是失败值"], true, "{r}");
    let reason = r["value"]["理由"].as_str().unwrap_or_default();
    assert!(reason.contains("Denied"), "{r}");
    let after = fs::read(&db).unwrap();
    assert_eq!(before, after, "只读连接上的写语句不该改动库文件");
    let _ = fs::remove_dir_all(&d);
}

/// 严重项（PR #36 复核，主会话实测）：`mode=ro` 只读打开位挡不住 `VACUUM INTO` 在磁盘生成
/// 新文件——授权回调修好之后应该报 `Fail(Denied)`、不产生目标文件（B164）。
#[test]
fn exec_sql的vacuum_into被拒且不留文件() {
    require_sandbox_or_skip!();
    let d = tmp("vacuum-into");
    let db = make_test_db(&d);
    let out = d.join("vacuum_out.db");
    let _ = fs::remove_file(&out);
    write_program_expect_fail(&d, &db, &format!("VACUUM INTO '{}'", out.to_string_lossy()));
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["是失败值"], true, "{r}");
    assert!(
        r["value"]["理由"]
            .as_str()
            .unwrap_or_default()
            .contains("Denied"),
        "{r}"
    );
    assert!(!out.exists(), "VACUUM INTO 不该在磁盘上留下文件：{out:?}");
    let _ = fs::remove_dir_all(&d);
}

/// 同上，`ATTACH DATABASE` 路径：`ATTACH` 本身不写「原库文件」，`mode=ro` 管不到（B164）。
#[test]
fn exec_sql的attach_database被拒且不留文件() {
    require_sandbox_or_skip!();
    let d = tmp("attach");
    let db = make_test_db(&d);
    let att = d.join("att_out.db");
    let _ = fs::remove_file(&att);
    write_program_expect_fail(
        &d,
        &db,
        &format!("ATTACH DATABASE '{}' AS a", att.to_string_lossy()),
    );
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"]["是失败值"], true, "{r}");
    assert!(
        r["value"]["理由"]
            .as_str()
            .unwrap_or_default()
            .contains("Denied"),
        "{r}"
    );
    assert!(
        !att.exists(),
        "ATTACH DATABASE 不该在磁盘上留下文件：{att:?}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 坏 SQL：语法错，`error` 非空，不 panic。
#[test]
fn exec_sql坏sql报错() {
    require_sandbox_or_skip!();
    let d = tmp("bad-sql");
    let db = make_test_db(&d);
    write_program(&d, &db, "select * from 不存在的表 where");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert!(
        !r["value"]["error"].as_str().unwrap_or_default().is_empty(),
        "{r}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 只凭账本重放零调用：首跑（带 `--ledger-out`）与只凭账本重放，`value` 逐字段相同、
/// 重放不新增调用。
#[test]
fn exec_sql只凭账本重放零调用() {
    require_sandbox_or_skip!();
    let d = tmp("replay");
    let db = make_test_db(&d);
    write_program(&d, &db, "select id, name from t order by id");
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
