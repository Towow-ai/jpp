//! B83 旁路测试（步 7c）：命中记录（账本 v3 的 `CalibUsed` 条目；原为头行 `calib_used`）是本趟实际命中的集合，续接后只凭账本重放复现续接趟。
//! 账本 v3（步 18a，B124）起命中记录是 `CalibUsed` 条目、按键取最后一条，续接换线追加一条；
//! 本文件的取值帮手随之改读条目，(c) 的一条断言按条目形状改写，其余断言不变。
//!
//! 依据：`附注/2026-09-24-B83续接缺口裁定.md`；`21` §四·6 步 7c。预注册：`地基/过程记录/工程-步7c.md` §一。
//! 程序 `examples/sieve.jpp` 在 `_calib/b24` 上只命中一条题式记录 K。A′ = 把 K 的线抬高，出口随之变化。
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-b83-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str], out: &Path) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .arg("run")
        .args(args)
        .args(["--output", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&fs::read(out).unwrap()).unwrap()
}

const K文件: &str = "_form_38e0207a4cbe0a56b8b472b7.json";
const K哈希: &str = "38e0207a4cbe0a56b8b472b7";

fn 库a(d: &Path) -> PathBuf {
    let a = d.join("A");
    fs::create_dir_all(&a).unwrap();
    for e in fs::read_dir(root().join("tests/golden/_calib/b24")).unwrap() {
        let p = e.unwrap().path();
        fs::copy(&p, a.join(p.file_name().unwrap())).unwrap();
    }
    a
}

/// A′：K 的线（记录与证书上的 hi）从 0.455 抬到 0.98
fn 库a撇(d: &Path) -> PathBuf {
    let a2 = d.join("A2");
    fs::create_dir_all(&a2).unwrap();
    let a = 库a(d);
    for e in fs::read_dir(&a).unwrap() {
        let p = e.unwrap().path();
        fs::copy(&p, a2.join(p.file_name().unwrap())).unwrap();
    }
    let t = fs::read_to_string(a.join(K文件)).unwrap();
    let t2 = t.replace("\"hi\": 0.455", "\"hi\": 0.98");
    assert_ne!(t, t2, "夹具记录的写法变了，改这里");
    fs::write(a2.join(K文件), t2).unwrap();
    a2
}

/// 命中记录：`CalibUsed` 条目按键取最后一条（账本 v3，B124；原为头行 `calib_used`）
fn 命中(l: &Path) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    for line in fs::read_to_string(l).unwrap().lines().skip(1) {
        let v: Value = serde_json::from_str(line).unwrap();
        if let Some(c) = v["entry"].get("CalibUsed") {
            m.insert(
                c["key"].as_str().unwrap().to_string(),
                serde_json::json!({"hash": c["hash"], "record": c["record"]}),
            );
        }
    }
    m
}

/// 账本里某键的 `CalibUsed` 条目条数
fn 命中条数(l: &Path, 含: &str) -> usize {
    fs::read_to_string(l)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| {
            v["entry"]
                .get("CalibUsed")
                .is_some_and(|c| c["key"].as_str().unwrap_or("").contains(含))
        })
        .count()
}

fn w_header(r: &Value) -> Vec<String> {
    r["trace"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w.as_str())
        .filter(|w| w.starts_with("W-header"))
        .map(String::from)
        .collect()
}

fn 去掉头告警(r: &Value) -> Vec<String> {
    r["trace"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w.as_str())
        .filter(|w| !w.starts_with("W-header"))
        .map(String::from)
        .collect()
}

const 程序: &str = "examples/sieve.jpp";
const 夹具: &str = "examples/fixtures/sieve-truth.json";

fn 首跑(lib: &Path, ledger: &Path, out: &Path) -> Value {
    run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            lib.to_str().unwrap(),
            "--ledger-out",
            ledger.to_str().unwrap(),
        ],
        out,
    )
}

fn k记录(l: &Path) -> Option<Value> {
    命中(l)
        .iter()
        .find(|(k, _)| k.contains(K哈希))
        .map(|(_, v)| v["record"].clone())
}

#[test]
fn a_换线续接_报变化的键_续接后账本复现续接趟() {
    let d = scratch("a");
    let (a, a2) = (库a(&d), 库a撇(&d));
    let (l1, l2) = (d.join("L1.jsonl"), d.join("L2.jsonl"));
    let first = 首跑(&a, &l1, &d.join("first.json"));
    let resumed = run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            a2.to_str().unwrap(),
            "--resume",
            l1.to_str().unwrap(),
            "--ledger-out",
            l2.to_str().unwrap(),
        ],
        &d.join("resumed.json"),
    );
    let w = w_header(&resumed);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("calib_used_hash 旧"), "{w:?}");
    assert!(
        w[0].contains("变化的键") && w[0].contains(K哈希),
        "报文列出 K：{w:?}"
    );
    assert_ne!(
        resumed["value"], first["value"],
        "A′ 的线让出口变了（否则本测试测不出什么）"
    );
    // L2 里 K 的记录（按键取最后一条）= 只用 A′ 首跑时记下的那一条
    let 参照 = d.join("ref.jsonl");
    首跑(&a2, &参照, &d.join("ref.json"));
    assert_eq!(k记录(&l2), k记录(&参照), "续接后账本记的是续接趟用的线");
    // 只凭 L2 重放：0 调用、无 W-header、复现续接趟
    let rep = run(
        &[程序, "--replay", l2.to_str().unwrap()],
        &d.join("replay.json"),
    );
    assert_eq!(rep["cost"]["calls"], 0);
    assert!(w_header(&rep).is_empty(), "{:?}", w_header(&rep));
    for k in ["status", "value", "returned_unsure", "pending"] {
        assert_eq!(rep[k], resumed[k], "字段 {k}");
    }
    assert_eq!(去掉头告警(&rep), 去掉头告警(&resumed));
}

#[test]
fn c_续接趟不走k_账本不含k_重放无告警() {
    let d = scratch("c");
    let (a, a2) = (库a(&d), 库a撇(&d));
    let (l1, l2) = (d.join("L1.jsonl"), d.join("L2.jsonl"));
    首跑(&a, &l1, &d.join("first.json"));
    // 续接时换程序：tally 不走 K 那条路
    let resumed = run(
        &[
            "examples/tally.jpp",
            "--fixtures",
            "examples/fixtures/tally.json",
            "--calib",
            a2.to_str().unwrap(),
            "--resume",
            l1.to_str().unwrap(),
            "--ledger-out",
            l2.to_str().unwrap(),
        ],
        &d.join("resumed.json"),
    );
    let w = w_header(&resumed);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("calib_used_hash 旧") && w[0].contains(K哈希),
        "旧头有 K，K 换了线：{w:?}"
    );
    // 账本 v3：命中记录是条目、保留历史（B83 第 6 条、B124）。续接趟没命中 K，就不为 K 追加条目：
    // L2 里 K 仍只有 L1 那一条（原断言「L2 不留 K」是头行按趟清空时的形状）
    assert_eq!(命中条数(&l2, K哈希), 1, "续接趟没命中 K，不追加");
    assert_eq!(k记录(&l2), k记录(&l1), "L2 里 K 仍是 L1 那一条");
    let rep = run(
        &["examples/tally.jpp", "--replay", l2.to_str().unwrap()],
        &d.join("replay.json"),
    );
    assert_eq!(rep["cost"]["calls"], 0);
    assert!(w_header(&rep).is_empty(), "{:?}", w_header(&rep));
}

#[test]
fn d_同库续接_无告警_命中集合不变() {
    let d = scratch("d");
    let a = 库a(&d);
    let (l1, l2) = (d.join("L1.jsonl"), d.join("L2.jsonl"));
    首跑(&a, &l1, &d.join("first.json"));
    let resumed = run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            a.to_str().unwrap(),
            "--resume",
            l1.to_str().unwrap(),
            "--ledger-out",
            l2.to_str().unwrap(),
        ],
        &d.join("resumed.json"),
    );
    assert!(w_header(&resumed).is_empty(), "{:?}", w_header(&resumed));
    assert_eq!(命中(&l1), 命中(&l2));
}
