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

/// 读：正常 `select`，`columns`/`rows` 正确，`error` 为空。
#[test]
fn exec_sql读() {
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

/// 写被拒：只读连接上的 `insert` 报错进 `error`，不 panic；库文件内容不变（真的没写进去，
/// 不是应用层假装拒绝）。
#[test]
fn exec_sql写被只读连接拒绝() {
    let d = tmp("write");
    let db = make_test_db(&d);
    let before = fs::read(&db).unwrap();
    write_program(&d, &db, "insert into t values (3, 'c')");
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    let error = r["value"]["error"].as_str().unwrap_or_default();
    assert!(!error.is_empty(), "{r}");
    assert!(
        error.to_lowercase().contains("readonly") || error.to_lowercase().contains("read-only"),
        "{r}"
    );
    let after = fs::read(&db).unwrap();
    assert_eq!(before, after, "只读连接上的写语句不该改动库文件");
    let _ = fs::remove_dir_all(&d);
}

/// 坏 SQL：语法错，`error` 非空，不 panic。
#[test]
fn exec_sql坏sql报错() {
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
