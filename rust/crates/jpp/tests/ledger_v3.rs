//! 账本 v3（步 18a，格式步；B83 第 6 条、B84、B92、B124）。
//!
//! (a) 首跑每个命中键一条 `CalibUsed`，同一账本再跑一趟逐字节不变；(b) 续接换线追加一条、只凭续接后的
//! 账本重放用最后一条；(c) `output_mat` 往返来源边与种类；(d) `calib_ref` 留位为空不序列化、给值能往返；
//! (e) v2 → v3 迁移：头行、`calib_used`、`__mat` 三项转换、链重算、截断与断链；(f) CLI 读 v2 账本在内存里
//! 迁移并提示，`jpp ledger-migrate` 改写文件。预注册：`地基/过程记录/工程-步18a.md` §一。
use jpp::ledger::{CalibRef, Entry, Header, Ledger, MatMeta, SourceEdge};
use jpp::value::{EdgeKind, Taint};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-v3-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn run(args: &[&str], out: &Path) -> Value {
    let mut a = vec!["run"];
    a.extend_from_slice(args);
    a.extend_from_slice(&["--output", out.to_str().unwrap()]);
    let o = jpp(&a);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&fs::read(out).unwrap()).unwrap()
}

fn 命中条目(l: &Path) -> Vec<Value> {
    fs::read_to_string(l)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|x| serde_json::from_str::<Value>(x).ok())
        .filter_map(|v| v["entry"].get("CalibUsed").cloned())
        .collect()
}

fn 库(d: &Path, 抬高: bool) -> PathBuf {
    let a = d.join(if 抬高 { "A2" } else { "A" });
    fs::create_dir_all(&a).unwrap();
    for e in fs::read_dir(root().join("tests/golden/_calib/b24")).unwrap() {
        let p = e.unwrap().path();
        let mut t = fs::read_to_string(&p).unwrap();
        if 抬高 {
            t = t.replace("\"hi\": 0.455", "\"hi\": 0.98");
        }
        fs::write(a.join(p.file_name().unwrap()), t).unwrap();
    }
    a
}

const 程序: &str = "examples/sieve.jpp";
const 夹具: &str = "examples/fixtures/sieve-truth.json";

#[test]
fn a_首跑每键一条_同账本再跑逐字节不变() {
    let d = scratch("a");
    let a = 库(&d, false);
    let (l1, l2) = (d.join("L1.jsonl"), d.join("L2.jsonl"));
    let c = a.to_str().unwrap();
    run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            c,
            "--ledger-out",
            l1.to_str().unwrap(),
        ],
        &d.join("1.json"),
    );
    let n = 命中条目(&l1);
    assert_eq!(n.len(), 1, "只命中一条题式记录：{n:?}");
    let head: Value =
        serde_json::from_str(fs::read_to_string(&l1).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(head["version"], 3);
    assert!(
        head.get("calib_used").is_none()
            && head["header"]["compared"].get("calib_used_hash").is_none()
    );
    run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            c,
            "--resume",
            l1.to_str().unwrap(),
            "--ledger-out",
            l2.to_str().unwrap(),
        ],
        &d.join("2.json"),
    );
    assert_eq!(
        fs::read(&l1).unwrap(),
        fs::read(&l2).unwrap(),
        "同库再跑一趟账本逐字节不变"
    );
}

#[test]
fn b_续接换线追加一条_重放取最后一条() {
    let d = scratch("b");
    let (a, a2) = (库(&d, false), 库(&d, true));
    let (l1, l2) = (d.join("L1.jsonl"), d.join("L2.jsonl"));
    run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            a.to_str().unwrap(),
            "--ledger-out",
            l1.to_str().unwrap(),
        ],
        &d.join("1.json"),
    );
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
        &d.join("2.json"),
    );
    let n = 命中条目(&l2);
    assert_eq!(n.len(), 2, "换线追加一条：{n:?}");
    assert_ne!(n[0]["hash"], n[1]["hash"]);
    let (l, _) = Ledger::decode(&fs::read_to_string(&l2).unwrap()).unwrap();
    assert_eq!(
        l.calib_used.values().next().unwrap()["hash"],
        n[1]["hash"],
        "派生视图取最后一条"
    );
    let rep = run(&[程序, "--replay", l2.to_str().unwrap()], &d.join("r.json"));
    assert_eq!(rep["cost"]["calls"], 0);
    assert_eq!(rep["value"], resumed["value"], "只凭账本重放复现续接趟");
}

#[test]
fn c_output_mat往返来源边与种类() {
    let mut l = Ledger::new();
    l.set_header(Header::new(1, 1.0, "m", "r1", "h"));
    let meta = MatMeta {
        addr: "do:x".into(),
        origin: vec!["do:k".into()],
        taint: Taint::Untrusted,
        sources: vec![
            SourceEdge {
                key: "甲".into(),
                kind: EdgeKind::Value,
                q: "q甲".into(),
            },
            SourceEdge {
                key: "乙".into(),
                kind: EdgeKind::Select,
                q: String::new(),
            },
        ],
    };
    l.put(Entry::Effect {
        key: "k".into(),
        ekey: None,
        kind: "do".into(),
        output: json!({"x": 1}),
        output_mat: Some(Box::new(meta.clone())),
        cost: 0.0,
    });
    let text = l.encode();
    assert!(
        text.contains("\"kind\":\"value\"") && text.contains("\"kind\":\"select\""),
        "{text}"
    );
    assert!(
        !text.contains("derived_from"),
        "v3 不写 derived_from：{text}"
    );
    let (back, _) = Ledger::decode(&text).unwrap();
    let Some(Entry::Effect { output_mat, .. }) = back.get("k") else {
        panic!("该有效应条目")
    };
    assert_eq!(output_mat.as_deref(), Some(&meta));
    assert_eq!(back.encode(), text);
}

#[test]
fn d_calib_ref留位为空不序列化_给值能往返() {
    let mut l = Ledger::new();
    let mut e = Entry::judge("k1", jpp::value::Answer::Noul(0.9), 0, 0.0, "m", 1);
    if let Entry::Judge { calib_ref, .. } = &mut e {
        *calib_ref = Some(Box::new(CalibRef::declared("k")));
    }
    l.put(e.clone());
    let text = l.encode();
    assert!(
        text.contains("\"calib_ref\":{\"declared\":\"k\"}"),
        "{text}"
    );
    let mut e2 = Entry::judge("k2", jpp::value::Answer::Noul(0.9), 0, 0.0, "m", 1);
    if let Entry::Judge { calib_ref, .. } = &mut e2 {
        *calib_ref = Some(Box::new(CalibRef {
            declared: "k".into(),
            key: Some("元组".into()),
            kind: Some("rel".into()),
            fill: Some(vec![("city".into(), "北京".into())]),
            line: Some("declared".into()),
            hi: Some(0.7),
            lo: Some(0.3),
            site: Some(42),
        }));
    }
    l.put(e2.clone());
    let (back, _) = Ledger::decode(&l.encode()).unwrap();
    assert_eq!(back.get("k2"), Some(&e2));
}

/// 手写一份 v2 账本：头行 calib_used 一键、一条判断、一条 `__mat` 效应输出（按 v2 规则成链）。
fn v2样本() -> String {
    fn h(s: &str) -> String {
        jpp::value::hash_of(&["ledger-line", s])
    }
    let head = r#"{"version":2,"header":{"budget":{"calls":1,"cost":1.0},"compared":{"model_id":"m","render_version":"r1","handler_version":"h","profile_hash":null,"behavior_hash":null,"calib_hash":"c","calib_used_hash":"u","lib_version":null,"bank_version":null,"ir_version":null,"entry_hash":null}},"calib_used":{"k":{"hash":"hk","record":{"hi":0.5}}}}"#;
    let judge = Entry::judge("j1", jpp::value::Answer::Noul(0.9), 0, 0.0, "m", 1);
    let l1 = format!(
        "{{\"seq\":1,\"prev\":\"{}\",\"entry\":{}}}",
        h(head),
        serde_json::to_string(&judge).unwrap()
    );
    let eff = r#"{"Effect":{"key":"e1","ekey":null,"kind":"do","output":{"__mat":{"x":1},"taint":"Untrusted","addr":"do:w","origin":["do:e1"],"derived_from":["q1"]},"output_mat":null,"cost":0.0}}"#;
    let l2 = format!("{{\"seq\":2,\"prev\":\"{}\",\"entry\":{eff}}}", h(&l1));
    format!("{head}\n{l1}\n{l2}\n")
}

#[test]
fn e_v2迁移为v3() {
    let v2 = v2样本();
    let m = jpp::store::migrations::ledger_v2::migrate(&v2).expect("迁移");
    assert_eq!(m.calib_used_keys, 1);
    let (l, t) = Ledger::decode(&m.text).expect("v3 可读");
    assert!(t.is_none());
    assert!(
        !m.text.contains("calib_used_hash") && !m.text.contains("derived_from"),
        "{}",
        m.text
    );
    assert_eq!(l.entries.len(), 3, "判断、效应、一条 CalibUsed");
    assert_eq!(l.calib_used["k"]["record"], json!({"hi": 0.5}));
    let Some(Entry::Effect {
        output, output_mat, ..
    }) = l.get("e1")
    else {
        panic!("该有效应条目")
    };
    assert_eq!(output, &json!({"x": 1}));
    let meta = output_mat.as_deref().unwrap();
    assert_eq!((meta.addr.as_str(), meta.taint), ("do:w", Taint::Untrusted));
    assert!(meta.sources.is_empty());
    // 末行半写：截掉并报告
    let 尾 = "{\"seq\":3,\"pr";
    let 半写 = format!("{v2}{尾}");
    let m2 = jpp::store::migrations::ledger_v2::migrate(&半写).expect("截断照迁");
    assert_eq!(m2.truncated_bytes, Some(尾.len()));
    // 断链：拒绝
    let 断 = v2.replacen("\"seq\":2", "\"seq\":5", 1);
    let e = jpp::store::migrations::ledger_v2::migrate(&断)
        .err()
        .expect("断链要拒");
    assert!(e.contains("链断"), "{e}");
    // v3 解码器直接读 v2：报迁移
    let e = Ledger::decode(&v2).expect_err("v2 不直接读");
    assert!(e.starts_with("E-ledger-v2"), "{e}");
}

#[test]
fn f_cli读v2自动迁移_ledger_migrate改写文件() {
    let d = scratch("f");
    let a = 库(&d, false);
    let l3 = d.join("L3.jsonl");
    run(
        &[
            程序,
            "--fixtures",
            夹具,
            "--calib",
            a.to_str().unwrap(),
            "--ledger-out",
            l3.to_str().unwrap(),
        ],
        &d.join("1.json"),
    );
    // 用手写的 v2 样本验证 CLI 的两条路（改写文件；读入时在内存里迁移）
    let v2 = d.join("v2.jsonl");
    fs::write(&v2, v2样本()).unwrap();
    let out = d.join("v3.jsonl");
    let o = jpp(&[
        "ledger-migrate",
        v2.to_str().unwrap(),
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("CalibUsed"));
    let (l, _) = Ledger::decode(&fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(l.entries.len(), 3);
    // 读 v2 账本做 --replay：在内存里迁移，stderr 提示，文件不改写
    let 前 = fs::read(&v2).unwrap();
    let o = jpp(&[
        "run",
        "examples/composition.jpp",
        "--replay",
        v2.to_str().unwrap(),
        "--output",
        d.join("r.json").to_str().unwrap(),
    ]);
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("账本 v2 已按 v3 读入"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(fs::read(&v2).unwrap(), 前, "文件不改写");
    let o = jpp(&[
        "ledger-migrate",
        l3.to_str().unwrap(),
        d.join("x").to_str().unwrap(),
    ]);
    assert!(!o.status.success(), "v3 不能再按 v2 迁移");
}
