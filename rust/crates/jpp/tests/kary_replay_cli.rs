//! CLI 只凭账本重放 K 元出口（修复 folio 重放，2026-09-24）。
//!
//! 重放时不给 `--fixtures`、不给 `--calib`：线从账本头补回，置换测量从判断条目取回。
//! 首跑与重放的值、交出的未决、未决原因逐项相同，重放 0 调用。
//! 覆盖 `probes/folio`（缺陷现场：K 选一首跑 `band`，修前重放成 `untested`）与一份内联夹具
//! （pick、tie、band、untested:permutation 四种 select 出口）。其余 K 元出口见 `jpp-core/tests/kary_replay.rs`。
//! 依据：`jpp-core/INTERFACE.md` §四·二·七·五；过程记录 `工程-修复-folio重放.md`。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-kary-replay-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(cwd: &Path, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "jpp {args:?} 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn 取(report: &Value) -> Value {
    let pending: Vec<Value> = report["pending"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| json!([p["cause"], p["site"]]))
        .collect();
    json!({"value": report["value"], "returned_unsure": report["returned_unsure"], "pending": pending, "status": report["status"]})
}

/// 首跑（带夹具）→ 只凭账本重放；返回 (首跑报告, 重放报告)。
fn 首跑与重放(name: &str, src: &Path, fixtures: &Path) -> (Value, Value) {
    首跑与重放_带输入(name, src, fixtures, None)
}

/// 步 14b-0：探针的材料经 `--input` 交给程序，首跑与重放给同一份。
fn 首跑与重放_带输入(
    name: &str,
    src: &Path,
    fixtures: &Path,
    input: Option<&Path>,
) -> (Value, Value) {
    let d = tmp(name);
    let (src, fx) = (src.display().to_string(), fixtures.display().to_string());
    let inp: Vec<String> = input
        .map(|p| vec!["--input".to_string(), p.display().to_string()])
        .unwrap_or_default();
    let inp: Vec<&str> = inp.iter().map(String::as_str).collect();
    let first: Vec<&str> = [
        "run",
        &src,
        "--fixtures",
        &fx,
        "--output",
        "r1.json",
        "--ledger-out",
        "l.jsonl",
    ]
    .into_iter()
    .chain(inp.iter().copied())
    .collect();
    jpp(&d, &first);
    let second: Vec<&str> = ["run", &src, "--replay", "l.jsonl", "--output", "r2.json"]
        .into_iter()
        .chain(inp.iter().copied())
        .collect();
    jpp(&d, &second);
    let r1: Value = serde_json::from_str(&fs::read_to_string(d.join("r1.json")).unwrap()).unwrap();
    let r2: Value = serde_json::from_str(&fs::read_to_string(d.join("r2.json")).unwrap()).unwrap();
    let _ = fs::remove_dir_all(&d);
    (r1, r2)
}

#[test]
fn folio_只凭账本重放与首跑相同() {
    let r = root();
    for fx in [
        "probes/folio/fixture.json",
        "probes/scope/fixture-folio.json",
    ] {
        let (r1, r2) = 首跑与重放_带输入(
            "folio",
            &r.join("probes/folio/folio.jpp"),
            &r.join(fx),
            Some(&r.join("probes/folio/baseline/materials.json")),
        );
        assert_eq!(r2["cost"]["calls"], 0, "{fx}：重放新增了调用");
        assert_eq!(取(&r2), 取(&r1), "{fx}：重放与首跑不同");
        // 缺陷现场：K 选一出口首跑是 band，不能在重放里变成 untested
        assert_eq!(r1["returned_unsure"][0], "unsure(band)", "{fx}");
    }
}

#[test]
fn select_四种出口只凭账本重放与首跑相同() {
    let d = tmp("inline");
    let src = r#"
budget {calls: 12, cost: 1, depth: 64};
let d = mat("一份租赁合同");
fn sel(t) {
    handle(cut(judge(state(d, {over: ["租赁", "买卖"]}), select(t, "k"))), {
        pick: fn(k) { "pick" },
        unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c } })
}
[sel("甲"), sel("乙"), sel("丙"), sel("丁")]
"#;
    let obs = |t: &str, p: f64, perm: Option<f64>| {
        let mut o = json!({"on": ["一份租赁合同"], "over": ["租赁", "买卖"], "op": "select", "text": t,
                           "calib": "k", "answer": {"Choice": [p, 1.0 - p]}});
        if let Some(ms) = perm {
            o["perms"] = json!(2);
            o["mode_share"] = json!(ms);
        }
        o
    };
    let fixture = json!({
        "description": "kary_replay_cli",
        // 步 15d-2：上岗夹具线要显式给 δ，否则出口 Unsure(untested)；select 题式补代码兜底值 0.15
        "calibrations": [{"key": "k", "hi": 0.6, "lo": 0.0, "n": 1, "status": "上岗", "delta": 0.15}],
        "observations": [obs("甲", 0.99, Some(1.0)), obs("乙", 0.99, Some(0.5)), obs("丙", 0.62, Some(1.0)), obs("丁", 0.99, None)]
    });
    fs::write(d.join("p.jpp"), src).unwrap();
    fs::write(d.join("fx.json"), fixture.to_string()).unwrap();
    let (r1, r2) = 首跑与重放("inline-run", &d.join("p.jpp"), &d.join("fx.json"));
    assert_eq!(r1["value"], json!(["pick", "tie", "band", "untested"]));
    assert_eq!(r2["cost"]["calls"], 0);
    assert_eq!(取(&r2), 取(&r1));
    let _ = fs::remove_dir_all(&d);
}
