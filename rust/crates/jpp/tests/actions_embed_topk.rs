//! R2a：`do("embed_topk", [texts, query, k], seq)` 端到端行为。
//! 预注册：`地基/过程记录/工程-比赛R2a.md`；实现：`crates/jpp/src/cli/actions_r2a.rs`。
//! 依赖装了 `sentence-transformers` 的离线 Python，只认环境变量 `JPP_EMBED_PYTHON`
//! （这份代码会同步到公开仓库，不能内置任何一台开发机的本地路径）；没设，或设了但不是可执行
//! 文件时跳过（打印原因，不判失败），不拖垮没有这个环境的机器上的全量门禁。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn embed_python_available() -> bool {
    match std::env::var("JPP_EMBED_PYTHON") {
        Ok(python) => Path::new(&python).is_file(),
        Err(_) => false,
    }
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-actions-embed-{name}-{}", std::process::id()));
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

/// 首跑做真机语义检索；只凭账本重放零调用、值逐字段相同（向量本身不进账本，
/// 但动作输出 `[{id, score}]` 进账本，重放时原样取回，不重跑 Python）。
#[test]
fn embed_topk经do调用并且只凭账本重放零调用() {
    if !embed_python_available() {
        eprintln!(
            "跳过 embed_topk 集成测试：找不到已装 sentence-transformers 的解释器（环境依赖，非失败）"
        );
        return;
    }
    let d = tmp("basic");
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
content(do("embed_topk", [["苹果", "香蕉", "橙子", "汽车"], "水果", 2], 0))
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
    for h in hits {
        let id = h["id"].as_i64().unwrap();
        assert!(id != 3, "“水果”查询的前 2 名不该包含“汽车”（id 3）：{r1}");
    }

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
