//! 真值通道（件 a，B19）：标注导入 → 题式级线 → 只凭账本重放出口一致。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// 步 15d-2：δ 只从校准记录取，不装画像时用测试画像（noul 0.05 / choice、score 0.15，
// 即步 15d-2 之前代码兜底的值）让这些测试保持原来的线。
const 画像: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn jpp(args: &[&str]) -> (Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    (
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
        stderr,
    )
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-truth-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_rows(path: &Path, rows: &[Value]) {
    fs::write(
        path,
        rows.iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
}

/// 题式 `这段话是否提到了{city}？`（与 examples/sieve.jpp 同一规格，所以落在同一个题式键上）
fn mention() -> Value {
    json!({"op": "test", "template": "这段话是否提到了{city}？"})
}

#[test]
fn computed_truth_certifies_both_sides_and_replay_needs_only_the_ledger() {
    let d = scratch("replay");
    let mut rows = vec![];
    // B24 拆分认证：认证半每侧要有零错误所需的条数（α=δ=0.1 时约 22），所以每侧 100 条
    for i in 0..100 {
        rows.push(json!({"form": mention(), "item": format!("pos{i}"), "p": 0.97 + (i % 3) as f64 * 0.01, "label": true, "source": "computed"}));
        rows.push(json!({"form": mention(), "item": format!("neg{i}"), "p": 0.01 + (i % 3) as f64 * 0.01, "label": false, "source": "computed"}));
    }
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let calib = d.join("calib");
    let (report, _) = jpp(&[
        "calib-import",
        labels.to_str().unwrap(),
        "--calib-out",
        calib.to_str().unwrap(),
        "--profile",
        画像,
    ]);
    assert_eq!(report[0]["status"], "上岗");
    assert!(
        report[0]["lo"].as_f64().unwrap() > 0.0,
        "两侧认证：Ignore 出口可达"
    );
    assert!(report[0]["lo"].as_f64().unwrap() < report[0]["hi"].as_f64().unwrap());

    // 夹具里没有 form-mention 的线：出口只能来自题式级回退，且要留痕
    let ledger = d.join("ledger.json");
    let first = d.join("first.json");
    jpp(&[
        "run",
        "examples/sieve.jpp",
        "--fixtures",
        "examples/fixtures/sieve-truth.json",
        "--calib",
        calib.to_str().unwrap(),
        "--ledger-out",
        ledger.to_str().unwrap(),
        "--output",
        first.to_str().unwrap(),
    ]);
    let first: Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    let warnings = first["trace"]["warnings"].to_string();
    assert!(
        warnings.contains("W-form-line"),
        "用了题式级线就要说出来：{warnings}"
    );
    assert_eq!(first["value"]["streams"][0]["act"], json!([0, 3, 8, 9]));

    // 只凭账本重放：不给 --calib、不给 --fixtures，出口与首跑逐字节一致
    let again = d.join("again.json");
    let (_, stderr) = jpp(&[
        "run",
        "examples/sieve.jpp",
        "--replay",
        ledger.to_str().unwrap(),
        "--output",
        again.to_str().unwrap(),
    ]);
    assert!(stderr.contains("从账本补回"), "{stderr}");
    let again: Value = serde_json::from_slice(&fs::read(&again).unwrap()).unwrap();
    assert_eq!(again["cost"]["calls"], 0);
    assert_eq!(again["value"], first["value"]);
}

#[test]
fn model_truth_waits_for_a_spot_check_above_the_gate() {
    let d = scratch("gate");
    let mut rows = vec![];
    for i in 0..40 {
        let truth = i % 2 == 0;
        rows.push(json!({"form": mention(), "item": format!("m{i}"), "p": if truth { 0.95 } else { 0.05 }, "label": truth, "source": "model:test"}));
    }
    // 人工抽检 10 条，其中 3 条与模型相反：一致率 0.7 < 0.9
    for i in 0..10 {
        let truth = i % 2 == 0;
        rows.push(json!({"form": mention(), "item": format!("m{i}"), "label": if i < 3 { !truth } else { truth }, "source": "human", "spot_check": "b1"}));
    }
    rows.push(json!({"form": mention(), "item": "amb", "p": 0.5, "label": "ambiguous", "source": "model:test"}));
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let calib = d.join("calib");
    let (report, _) = jpp(&[
        "calib-import",
        labels.to_str().unwrap(),
        "--calib-out",
        calib.to_str().unwrap(),
        "--profile",
        画像,
    ]);
    assert_eq!(report[0]["status"], "待真值");
    let gate = report[0]["truth"]["gate"].as_str().unwrap();
    assert!(gate.starts_with("待核") && gate.contains("0.70"), "{gate}");
    assert_eq!(report[0]["truth"]["spot_check"]["n"], 10);
    assert_eq!(report[0]["truth"]["ambiguous"], 1);
}

/// B36：Sonnet 标注 + Fable 复核批次。每侧 100 条标注；复核 30 条全一致（下界 0.905）。
fn sonnet_rows() -> Vec<Value> {
    let mut rows = vec![];
    for i in 0..100 {
        rows.push(json!({"form": mention(), "item": format!("pos{i}"), "p": 0.97, "label": true, "source": "model:claude-sonnet-5", "generator": "model:claude-sonnet-5"}));
        rows.push(json!({"form": mention(), "item": format!("neg{i}"), "p": 0.02, "label": false, "source": "model:claude-sonnet-5", "generator": "model:claude-sonnet-5"}));
    }
    rows
}

#[test]
fn b36_model_review_batch_certifies_model_labels_and_names_the_reviewer() {
    let d = scratch("b36-review");
    let mut rows = sonnet_rows();
    for i in 0..15 {
        rows.push(json!({"form": mention(), "item": format!("pos{i}"), "label": true, "source": "model:fable-5.1", "spot_check": "fable-r1-M"}));
        rows.push(json!({"form": mention(), "item": format!("neg{i}"), "label": false, "source": "model:fable-5.1", "spot_check": "fable-r1-M"}));
    }
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let (report, _) = jpp(&[
        "calib-import",
        labels.to_str().unwrap(),
        "--calib-out",
        d.join("calib").to_str().unwrap(),
        "--profile",
        画像,
    ]);
    assert_eq!(report[0]["status"], "上岗", "{report}");
    // B89（步 20i）：复核 30 条只覆盖认证集的一部分，a_lb(A) ≈ 0.905 → alpha_eff ≈ 0.195 > α，降为试用
    let gate = report[0]["truth"]["gate"].as_str().unwrap();
    assert!(
        gate.starts_with(
            "上岗（复核者 model:fable-5.1）；alpha_eff=0.195 > α=0.1，按 B89 降为试用"
        ),
        "{gate}"
    );
    assert_eq!(report[0]["certification"]["grade"], "trial");
    assert_eq!(report[0]["truth"]["spot_check"]["n"], 30);
    assert_eq!(
        report[0]["truth"]["spot_check"]["model_reviewers"],
        json!(["model:fable-5.1"])
    );
}

#[test]
fn b36_review_from_the_same_source_as_the_annotation_is_rejected() {
    let d = scratch("b36-same");
    let mut rows = sonnet_rows();
    for i in 0..30 {
        rows.push(json!({"form": mention(), "item": format!("pos{i}"), "label": true, "source": "model:claude-sonnet-5", "spot_check": "self-r1"}));
    }
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let (report, _) = jpp(&[
        "calib-import",
        labels.to_str().unwrap(),
        "--calib-out",
        d.join("calib").to_str().unwrap(),
        "--profile",
        画像,
    ]);
    assert_eq!(report[0]["status"], "待真值");
    assert!(
        report[0]["truth"]["spot_check"].is_null(),
        "同源复核不计入一致率"
    );
    assert!(
        report[0]["truth"]["gate"]
            .as_str()
            .unwrap()
            .starts_with("待核")
    );
    assert!(
        report[0]["warnings"]
            .to_string()
            .contains("W-review-same-source")
    );
}

#[test]
fn b36_truth_order_is_computed_then_human_then_review_then_annotation() {
    let d = scratch("b36-order");
    let rows = vec![
        // a：标注 true、复核 false → 真值取复核行
        json!({"form": mention(), "item": "a", "p": 0.5, "label": true, "source": "model:claude-sonnet-5"}),
        json!({"form": mention(), "item": "a", "label": false, "source": "model:fable-5.1", "spot_check": "fable-r1-M"}),
        // b：标注、复核、人工 → 取人工
        json!({"form": mention(), "item": "b", "p": 0.5, "label": true, "source": "model:claude-sonnet-5"}),
        json!({"form": mention(), "item": "b", "label": true, "source": "model:fable-5.1", "spot_check": "fable-r1-M"}),
        json!({"form": mention(), "item": "b", "p": 0.5, "label": false, "source": "human"}),
        // c：人工在前、构造在后 → 取构造
        json!({"form": mention(), "item": "c", "p": 0.5, "label": false, "source": "human"}),
        json!({"form": mention(), "item": "c", "p": 0.5, "label": true, "source": "computed"}),
    ];
    let labels = d.join("labels.jsonl");
    write_rows(&labels, &rows);
    let (report, _) = jpp(&[
        "calib-import",
        labels.to_str().unwrap(),
        "--calib-out",
        d.join("calib").to_str().unwrap(),
        "--profile",
        画像,
    ]);
    assert_eq!(
        report[0]["truth"]["sources"],
        json!({"computed": 1, "human": 1, "model:fable-5.1": 1})
    );
    assert_eq!(report[0]["truth"]["spot_check"]["n"], 2);
    assert_eq!(report[0]["truth"]["spot_check"]["agree"], 1);
}

/// PR #30 补丁请求 3：题级上岗但没有这个代价矩阵的证书，代价线取自题式级证书。
/// 题式键要记进账本 `calib_used`，只凭账本重放才能补回同一条线、给出同一个出口。
#[test]
fn cost_line_form_cert_replays_from_ledger_only() {
    use jpp::effects::{CalibStore, Cert, LabelSource};
    let d = scratch("cost-form");
    let 头 = "budget {calls: 2, cost: 0, depth: 8};\nlet f = form(\"test\", \"这段话是否提到了{city}？\", {calib: \"k\"});\n";
    let hash_src = d.join("hash.jpp");
    fs::write(&hash_src, format!("{头}f.hash\n")).unwrap();
    let hash_out = d.join("hash.json");
    jpp(&[
        "run",
        hash_src.to_str().unwrap(),
        "--output",
        hash_out.to_str().unwrap(),
    ]);
    let hash: Value = serde_json::from_slice(&fs::read(&hash_out).unwrap()).unwrap();
    let fk = CalibStore::form_key(hash["value"].as_str().expect("hash 是文本"));

    // 题级 k：上岗，只有无代价证书；题式键：上岗，带 cost(1,5) 证书（线 0.7）
    let mut c = CalibStore::new();
    for (key, cost) in [("k", None), (fk.as_str(), Some((1.0, 5.0)))] {
        c.put(key, 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
        let cert = Cert {
            alpha: 0.10,
            conf_delta: 0.10,
            hi: 0.7,
            n_accepted: 50,
            n_errors: 0,
            ucb: 0.05,
            cluster_unit: "测试合成证书".into(),
            resample: None,
            cost,
            bounded_side: "单侧".into(),
            label_fp: String::new(),
            selection: None,
            grade: Default::default(),
            eff: None,
            label_source: LabelSource::全体,
        };
        let r = c.records.get_mut(key).unwrap();
        r.certs.insert(cert.addr(), cert);
        r.fixture = false;
    }
    let calib = d.join("calib");
    c.save(&calib).unwrap();
    let fixtures = d.join("fixtures.json");
    let obs = json!({"observations": [{"on": ["我去了上海"], "op": "test", "text": "这段话是否提到了上海？",
        "calib": "k", "answer": {"Noul": 0.9}}]});
    fs::write(&fixtures, obs.to_string()).unwrap();

    let src = d.join("prog.jpp");
    fs::write(&src, format!("{头}let q = fill(f, {{city: \"上海\"}});\nlet e = cut(judge(state(mat(\"我去了上海\")), q), {{cost: [1, 5]}});\n\
        handle(e, {{act: fn() {{ \"act\" }}, ignore: fn() {{ \"ig\" }}, unsure: fn(u) {{ let c = unsure_cause(u); consume(u, \"drop\"); c }}}})\n")).unwrap();
    let ledger = d.join("ledger.json");
    let first = d.join("first.json");
    jpp(&[
        "run",
        src.to_str().unwrap(),
        "--fixtures",
        fixtures.to_str().unwrap(),
        "--calib",
        calib.to_str().unwrap(),
        "--ledger-out",
        ledger.to_str().unwrap(),
        "--output",
        first.to_str().unwrap(),
    ]);
    let first: Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    assert_eq!(first["value"], json!("act"), "{first}");

    // 账本 v3（步 18a）：命中记录是 `CalibUsed` 条目（原为头行 `calib_used`）
    let 命中: Vec<String> = fs::read_to_string(&ledger)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v["entry"]["CalibUsed"]["key"].as_str().map(String::from))
        .collect();
    assert!(命中.contains(&fk), "题式键要入账：{命中:?}");

    let again = d.join("again.json");
    jpp(&[
        "run",
        src.to_str().unwrap(),
        "--replay",
        ledger.to_str().unwrap(),
        "--output",
        again.to_str().unwrap(),
    ]);
    let again: Value = serde_json::from_slice(&fs::read(&again).unwrap()).unwrap();
    assert_eq!(again["cost"]["calls"], 0);
    assert_eq!(again["value"], first["value"]);
}
