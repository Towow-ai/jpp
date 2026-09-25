//! 步 9a（评估①建议 6）：诊断的机读出口 `--json`、同码同址折叠、运行期编号 `E-rt-<名>`。
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn jpp(args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

fn scratch(name: &str, src: &str) -> String {
    let d = std::env::temp_dir().join(format!("jpp-9a-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let p = d.join("p.jpp");
    fs::write(&p, src).unwrap();
    p.to_string_lossy().into_owned()
}

/// stderr 里以 `{` 开头的行：`run --json` 的诊断
fn json_lines(err: &str) -> Vec<Value> {
    err.lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn check_json_是一个文档且字段齐() {
    let (ok, out, err) = jpp(&["check", "examples/errors/question-field-typo.jpp", "--json"]);
    assert!(!ok, "有错时退出码仍非零");
    assert!(err.is_empty(), "stderr 不出东西：{err}");
    let doc: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (
            doc["ok"].as_bool(),
            doc["errors"].as_u64(),
            doc["warnings"].as_u64()
        ),
        (Some(false), Some(1), Some(1))
    );
    let ds = doc["diagnostics"].as_array().unwrap();
    let e = ds.iter().find(|d| d["code"] == "E-field").unwrap();
    assert_eq!(e["level"], "error");
    assert_eq!(
        (e["span"]["line"].as_u64(), e["span"]["col"].as_u64()),
        (Some(3), Some(1))
    );
    assert!(
        e["span"]["file"]
            .as_str()
            .unwrap()
            .ends_with("question-field-typo.jpp")
    );
    assert!(
        e["fix"]
            .as_str()
            .unwrap()
            .starts_with("改成题的可读字段之一")
    );
    assert_eq!(
        (e["applicability"].as_str(), e["count"].as_u64()),
        (Some("manual"), Some(1))
    );
    for k in [
        "code",
        "level",
        "span",
        "message",
        "fix",
        "applicability",
        "count",
    ] {
        assert!(e.get(k).is_some(), "缺字段 {k}");
    }
}

#[test]
fn check_json_无错时_ok() {
    let (ok, out, _) = jpp(&["check", "examples/sieve.jpp", "--json"]);
    assert!(ok);
    let doc: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["ok"], true);
}

#[test]
fn check_json_降级错也出机读() {
    let p = scratch("lower", "1");
    let (ok, out, err) = jpp(&["check", &p, "--json"]);
    assert!(!ok && err.is_empty(), "{err}");
    let doc: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(doc["diagnostics"][0]["code"], "J-07a");
}

#[test]
fn run_json_同码同址折叠计数_报告不动() {
    let args = [
        "run",
        "examples/question-forms.jpp",
        "--fixtures",
        "examples/fixtures/question-forms.json",
    ];
    let (_, out_text, _) = jpp(&args);
    let mut with_json = args.to_vec();
    with_json.push("--json");
    let (ok, out_json, err) = jpp(&with_json);
    assert!(ok, "{err}");
    assert_eq!(out_text, out_json, "报告不因 --json 改变");
    let ds = json_lines(&err);
    let fixture: Vec<u64> = ds
        .iter()
        .filter(|d| d["code"] == "W-fixture-line")
        .map(|d| d["count"].as_u64().unwrap())
        .collect();
    // 同一站点两个键：form-mention 一条、form-listed 三条折成一条；键不同不合并
    assert_eq!(fixture, [1, 3]);
    let report: Value = serde_json::from_str(&out_json).unwrap();
    let raw = report["trace"]["warnings"].as_array().unwrap();
    assert_eq!(
        raw.iter()
            .filter(|w| w.as_str().unwrap().starts_with("W-fixture-line"))
            .count(),
        4,
        "trace.warnings 原样"
    );
}

#[test]
fn 运行期错误带编号与类型说明() {
    let p = scratch("rt", "budget {calls: 0, cost: 0};\nslice([1, 2], 1)");
    let (ok, _, err) = jpp(&["run", &p]);
    assert!(!ok);
    assert!(err.contains(": E-rt-arity: "), "文本形式带编号：{err}");
    let (ok, _, err) = jpp(&["run", &p, "--json"]);
    assert!(!ok);
    let ds = json_lines(&err);
    let e = ds
        .iter()
        .find(|d| d["level"] == "error")
        .unwrap_or_else(|| panic!("{err}"));
    assert_eq!(e["code"], "E-rt-arity");
    assert!(e["explain"].as_str().unwrap().contains("参数个数"));
    assert_eq!(e["span"]["line"].as_u64(), Some(2));
}

#[test]
fn json_只给_check_与_run() {
    let (ok, _, err) = jpp(&["parse", "examples/sieve.jpp", "--json"]);
    assert!(
        !ok && err.contains("--json is accepted only by check and run"),
        "{err}"
    );
    let (ok, _, err) = jpp(&["check", "examples/sieve.jpp", "--json", "--json"]);
    assert!(!ok && err.contains("twice"), "{err}");
}
