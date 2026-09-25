//! 步 1（K1，放行方向）：调用者用 `outcome` 重包 sieve 结果时漏写 `detail.ignore`，
//! `tally` 曾把 `all` 从 ignore 静默翻成 act（四骨架原型的最小复现，
//! `地基/过程记录/2026-09-23-阶段4-四骨架.md` 语言缺口 1）。修后报 `E-tally-missing-rejected`；
//! 显式抄入 `detail.ignore` 时与直接 `tally(sieve 结果)` 相同。
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_src(name: &str, src: &str) -> (bool, Value, String) {
    let dir = std::env::temp_dir().join(format!("jpp-k1-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("k1.jpp");
    std::fs::write(&f, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "run",
            f.to_str().unwrap(),
            "--fixtures",
            "examples/fixtures/tally.json",
        ])
        .output()
        .unwrap();
    let v = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    (
        out.status.success(),
        v,
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

const HEAD: &str = r#"budget {calls: 10, cost: 0.01, depth: 256};
let notes = ["这款耳机售价三百九十九元。", "新店开业，全场八折。", "今天天气晴朗，适合出门。", "会员价比原价便宜了五十元。", "价格还没公布，据说下个月发布。"];
let price = test("这段话给出了具体价格或折扣吗？", "price");
let r = sieve(notes, price);
"#;

#[test]
fn repacked_contract_without_ignore_is_rejected() {
    let src = format!(
        "{HEAD}let re = outcome({{value: r.value, pending: r.pending, evidence: r.evidence}});\nlet t = tally(re);\n{{all: t.value.all, pending: t.pending}}\n"
    );
    let (ok, _, stderr) = run_src("missing", &src);
    assert!(!ok, "漏写 detail.ignore 应当报错");
    assert!(stderr.contains("E-tally-missing-rejected"), "{stderr}");
}

#[test]
fn repacked_contract_with_ignore_matches_direct_tally() {
    let direct = format!(
        "{HEAD}let t = tally(r);\n{{count: t.value.count, all: t.value.all, exists: t.value.exists, pending: t.pending}}\n"
    );
    let repack = format!(
        "{HEAD}let re = outcome({{value: r.value, pending: r.pending, evidence: r.evidence, detail: {{ignore: r.detail.ignore}}}});\nlet t = tally(re);\n{{count: t.value.count, all: t.value.all, exists: t.value.exists, pending: t.pending}}\n"
    );
    let (ok1, a, e1) = run_src("direct", &direct);
    let (ok2, b, e2) = run_src("repack", &repack);
    assert!(ok1 && ok2, "{e1}\n{e2}");
    assert_eq!(a["value"]["count"], b["value"]["count"]);
    assert_eq!(a["value"]["all"], b["value"]["all"]);
    assert_eq!(a["value"]["exists"], b["value"]["exists"]);
}
