//! R2a：`do("bm25_topk", [query, corpus, k], seq)` 端到端行为（纯 Rust，无外部依赖，
//! 不像 `embed_topk` 那样受机器上有没有装 Python 解释器影响）。
//! 预注册：`地基/过程记录/工程-比赛R2a.md`；实现：`crates/jpp/src/cli/actions_r2a.rs`。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-actions-bm25-{name}-{}", std::process::id()));
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

/// 首跑返回按分数降序的 `[{id, score}]`；只凭账本重放零调用、值逐字段相同。
#[test]
fn bm25_topk经do调用并且只凭账本重放零调用() {
    let d = tmp("basic");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("bm25_topk", ["苹果", ["苹果 香蕉 橙子", "今天天气很好", "苹果派配咖啡"], 2], 0))
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
    let hits = r1["value"].as_array().unwrap();
    assert_eq!(hits.len(), 2, "{r1}");
    let top_id = hits[0]["id"].as_i64().unwrap();
    assert!(top_id == 0 || top_id == 2, "含“苹果”的文档应排前：{r1}");

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

/// 空语料给空结果，不报错。
#[test]
fn bm25_topk空语料给空结果() {
    let d = tmp("empty");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("bm25_topk", ["query", [], 5], 0))
"#;
    fs::write(d.join("p.jpp"), src).unwrap();
    let (ok, err) = jpp(&d, &["run", "p.jpp", "--output", "r.json"]);
    assert!(ok, "{err}");
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    assert_eq!(r["value"], serde_json::json!([]), "{r}");
    let _ = fs::remove_dir_all(&d);
}
