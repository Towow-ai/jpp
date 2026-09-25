//! 步 14c（B70，探针首轮 I-6）：宿主二元算术里 Int 与 Float 混用时统一提升为 Float，
//! `+ - * /` 与比较同一规则。探针实测 `3 * 1.0` 可而 `3.0 / 2` 报 `E-rt-type`，是同一规则
//! 在两处实现不一致。整数间运算（整数除法、溢出、除零）按 `13` §6 不变；`%` 不在 B70 范围内。
//! 依据：`地基/附注/2026-09-24-探针首轮裁定.md` B70；`地基/21-工程方案-v1.md` 步 14c。
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 跑一段无判断的程序，返回（成功否，报告里的 value，stderr）
fn run_src(name: &str, body: &str) -> (bool, Value, String) {
    let dir = std::env::temp_dir().join(format!("jpp-b70-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("b70.jpp");
    std::fs::write(
        &f,
        // B38（步 15d）：do 也计入 calls；g 用一次 read_json，其余用例没有效应，预算对它们无影响
        format!("budget {{calls: 1, cost: 0, depth: 8}};\n{body}\n"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(["run", f.to_str().unwrap()])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    (
        out.status.success(),
        v["value"].clone(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn value_of(name: &str, body: &str) -> Value {
    let (ok, v, stderr) = run_src(name, body);
    assert!(ok, "{name}: {stderr}");
    v
}

#[test]
fn a_混用除法提升为浮点() {
    let v = value_of("div", "{a: 3.0 / 2, b: 3 / 2.0}");
    assert_eq!(v, json!({"a": 1.5, "b": 1.5}));
}

#[test]
fn b_整数除法与除零不变() {
    let v = value_of("intdiv", "{q: 7 / 2}");
    assert_eq!(v, json!({"q": 3}));
    assert!(v["q"].is_i64());
    let (ok, _, stderr) = run_src("zero", "{q: 7 / 0}");
    assert!(!ok);
    assert!(stderr.contains("E-rt-int"), "{stderr}");
}

#[test]
fn c_混用比较按数值() {
    let v = value_of(
        "cmp",
        "{lt: 1 < 1.5, ge: 2 >= 2.0, gt: 2.5 > 2, le: 3 <= 2.0}",
    );
    assert_eq!(v, json!({"lt": true, "ge": true, "gt": true, "le": false}));
}

#[test]
fn d_已有的混用提升不变() {
    let v = value_of(
        "old",
        "{add: 1 + 2.5, sub: 3 - 0.5, mul: 3 * 1.0, rsub: 2.5 - 1, eq: 1.0 == 1, ne: 2 != 2.0}",
    );
    assert_eq!(
        v,
        json!({"add": 3.5, "sub": 2.5, "mul": 3.0, "rsub": 1.5, "eq": true, "ne": false})
    );
}

#[test]
fn e_混用除法的结果是浮点() {
    let v = value_of("kind", "{r: 2 / 1.0}");
    assert!(v["r"].is_f64(), "{v}");
    assert_eq!(v["r"].as_f64(), Some(2.0));
}

#[test]
fn f_取模不在范围内() {
    let (ok, _, stderr) = run_src("mod", "{m: 7 % 2.0}");
    assert!(!ok);
    assert!(stderr.contains("E-rt-type"), "{stderr}");
}

#[test]
fn g_提升不改来源标签() {
    // 不可信 Int（read_json 读出）与字面 Float 相除，结果转材料：taint 仍是 untrusted、origin 记 computed，
    // 与 Int/Int 同一规则（B33 的 ∨ 在 binop 一处算，提升只改数值）
    let dir = std::env::temp_dir().join(format!("jpp-b70-{}-taint", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let j = dir.join("n.json");
    std::fs::write(&j, r#"{"n": 3}"#).unwrap();
    let body = format!(
        "let d = content(do(\"read_json\", [{p:?}], 0));\nlet m = mat(d.n / 2.0);\nlet k = mat(d.n / 2);\n{{m: m, k: k}}",
        p = j.to_str().unwrap()
    );
    let v = value_of("taint", &body);
    for (k, want) in [("m", json!(1.5)), ("k", json!(1))] {
        let s = v[k].to_string();
        assert!(
            s.contains("Untrusted") || s.contains("untrusted"),
            "{k}: {s}"
        );
        assert!(s.contains("computed"), "{k}: {s}");
        assert!(s.contains(&want.to_string()), "{k}: {s}");
    }
}
