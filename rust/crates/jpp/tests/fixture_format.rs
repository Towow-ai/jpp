//! 步 4d：夹具格式补两项（原型 K6、K7）。旧夹具缺这两项时行为与改前相同（金样覆盖）。
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
    let d = std::env::temp_dir().join(format!("jpp-fixfmt-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn run(program: &str, fixture: &Value, d: &Path) -> Value {
    let src = d.join("p.jpp");
    let fx = d.join("f.json");
    let out = d.join("out.json");
    fs::write(&src, program).unwrap();
    fs::write(&fx, fixture.to_string()).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "run",
            src.to_str().unwrap(),
            "--fixtures",
            fx.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&fs::read(&out).unwrap()).unwrap()
}

const SELECT: &str = r#"budget {calls: 2, cost: 0, depth: 64};
let q = select("哪个选项与目标词完全相同？", "k-sel");
let s = state([mat("目标词：编译器")], {over: [mat("数据库"), mat("编译器")]});
handle(cut(judge(s, q)), {pick: fn(k) { {picked: k} }, unsure: fn(u) { {unsure: unsure_cause(u), pending: u} }})
"#;

fn select_fixture(measured: bool) -> Value {
    let mut o = json!({"on": ["目标词：编译器"], "over": ["数据库", "编译器"], "op": "select",
                       "text": "哪个选项与目标词完全相同？", "calib": "k-sel", "answer": {"Choice": [0.02, 0.98]}});
    if measured {
        o["perms"] = json!(2);
        o["mode_share"] = json!(1.0);
    }
    // 步 15d-2：上岗夹具线要显式给 δ，否则出口 Unsure(untested)；select 题式补代码兜底值 0.15
    json!({"calibrations": [{"key": "k-sel", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗", "delta": 0.15}], "observations": [o]})
}

/// K6：观察带置换测量时，固定观察下 `select` 走到 pick 分支；不带时与改前相同（未测 → unsure）。
#[test]
fn select_reaches_pick_when_the_fixture_records_permutations() {
    let d = scratch("select");
    let with = run(SELECT, &select_fixture(true), &d);
    assert_eq!(with["value"]["picked"], 1, "{with}");
    let without = run(SELECT, &select_fixture(false), &d);
    assert_eq!(without["value"]["unsure"], "untested", "{without}");
}

/// K7：同一提示、不同 ctx 的两次 `gen` 各得各的输出；不带 ctx 的旧写法仍按提示命中。
#[test]
fn gen_fixture_can_key_on_context() {
    let d = scratch("gen");
    let program = r#"budget {calls: 3, cost: 0, depth: 64};
let a = gen("写一句摘要", [mat("甲文")], 1, 0);
let b = gen("写一句摘要", [mat("乙文")], 1, 0);
let c = gen("写一句标题", [mat("丙文")], 1, 0);
{a: content(a[0]), b: content(b[0]), c: content(c[0])}
"#;
    let fixture = json!({"generations": [
        {"prompt": "写一句摘要", "retry_seq": 0, "ctx": ["甲文"], "output": ["甲的摘要"]},
        {"prompt": "写一句摘要", "retry_seq": 0, "ctx": ["乙文"], "output": ["乙的摘要"]},
        {"prompt": "写一句标题", "retry_seq": 0, "output": ["通用标题"]}
    ]});
    let out = run(program, &fixture, &d);
    assert_eq!(
        out["value"],
        json!({"a": "甲的摘要", "b": "乙的摘要", "c": "通用标题"}),
        "{out}"
    );
}
