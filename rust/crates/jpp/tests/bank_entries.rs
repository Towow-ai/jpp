//! 题库条目完整性（步 27a；B48 入库闸门：「至少一条在 CI 上重跑认证通过的记录」）。
//!
//! 对 `bank/entries/<form_hash>/` 下每个条目核四件：
//! (a) `lib/bank/<slug>.jpp` 求值得到的题式哈希 = 目录名 = 记录键里的题式哈希（改模板一个字即新题式）；
//! (b) `CalibStore::load` 按证书重跑认证后没有降级、也没有写回（`load_report` 为空、记录与原文件相同，20c / B117）；
//! (c) `bank.json` 的索引与目录一一对应，状态取值属于规范 §三的两档（B118）；
//! (d) 记录的样本数 = `labels.jsonl` 里非复核行的条数（复核行带 `spot_check`，B36 / B89）。
//! 依据：地基/题库/规范.md v2 §一、§三；地基/过程记录/工程-步27a.md §一·2。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::effects::CalibStore;
use serde_json::Value as Json;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bank() -> Json {
    serde_json::from_slice(&fs::read(root().join("bank/bank.json")).unwrap()).unwrap()
}

fn entry_dirs() -> BTreeSet<String> {
    fs::read_dir(root().join("bank/entries"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// 用 `jpp run` 求题式哈希：程序只 import 条目的题式文件并返回 `<slug>.hash`（0 调用）。
fn form_hash_of(slug: &str) -> String {
    // import 只收相对路径：程序放在仓库根下两层（target/<临时目录>/），相对引用 lib/bank/
    let dir = root().join(format!("target/bank-hash-{}-{slug}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = format!(
        "import \"../../lib/bank/{slug}.jpp\";\nbudget {{calls: 1, cost: 0.01, depth: 8}};\n{{h: {slug}.hash}}\n"
    );
    let prog = dir.join("h.jpp");
    fs::write(&prog, src).unwrap();
    let out = dir.join("r.json");
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&dir)
        .args([
            "run",
            prog.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Json = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
    let _ = fs::remove_dir_all(&dir);
    r["value"]["h"].as_str().unwrap().to_string()
}

fn entry(hash: &str) -> Json {
    bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["form_hash"] == hash)
        .unwrap_or_else(|| panic!("bank.json 没有索引条目 {hash}"))
        .clone()
}

#[test]
fn bank_json_indexes_exactly_the_entry_dirs() {
    let idx: BTreeSet<String> = bank()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["form_hash"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(idx, entry_dirs());
    for e in bank()["entries"].as_array().unwrap() {
        let s = e["status"].as_str().unwrap();
        assert!(
            s == "已认证·未复用" || s == "共享",
            "状态 {s} 不在规范 §三的两档里（B118）"
        );
    }
}

#[test]
fn form_hash_from_lib_matches_dir_and_record_key() {
    for h in entry_dirs() {
        let slug = entry(&h)["slug"].as_str().unwrap().to_string();
        assert_eq!(
            form_hash_of(&slug),
            h,
            "lib/bank/{slug}.jpp 的题式哈希与条目目录不符"
        );
        let raw = CalibStore::load_raw(&root().join(format!("bank/entries/{h}/calib"))).unwrap();
        let keys: Vec<&String> = raw.records.keys().collect();
        assert_eq!(keys.len(), 1, "条目 {h} 应恰有一条记录：{keys:?}");
        assert!(keys[0].ends_with(&h), "记录键 {:?} 不是题式 {h}", keys[0]);
    }
}

#[test]
fn every_entry_record_reruns_certification_without_downgrade() {
    for h in entry_dirs() {
        let dir = root().join(format!("bank/entries/{h}/calib"));
        let raw = CalibStore::load_raw(&dir).unwrap();
        let s = CalibStore::load(&dir).unwrap();
        assert!(s.load_report.is_empty(), "条目 {h}：{:?}", s.load_report);
        for (k, r) in &s.records {
            assert!(!r.fixture, "条目 {h} 的记录 {k:?} 降为夹具");
            assert_eq!(r, &raw.records[k], "条目 {h}：装载写回了记录（应逐位复现）");
        }
    }
}

fn label_rows(p: &Path) -> Vec<Json> {
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn record_sample_count_matches_label_set() {
    for h in entry_dirs() {
        let rows = label_rows(&root().join(format!("bank/entries/{h}/labels.jsonl")));
        let annot = rows
            .iter()
            .filter(|r| r.get("spot_check").is_none())
            .count();
        let raw = CalibStore::load_raw(&root().join(format!("bank/entries/{h}/calib"))).unwrap();
        let r = raw.records.values().next().unwrap();
        assert_eq!(
            r.n as usize, annot,
            "条目 {h}：记录 n 与标注集非复核行数不符"
        );
    }
}
