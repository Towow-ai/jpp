//! 步 20a-2b：`jpp check --questions-out`（B116 (5)）。检查期把程序里的字面题按作者写的校准标签导出，每道题带零槽
//! `form_hash`，供 20a-2e 把作者串记录迁成主键记录。
//! (a) 导出内容：合并同题站点、同标签不同题各一项、`select`/`measure` 的声明与档位、四种跳过原因、题式与填法不导出；
//! (b) 零槽 `form_hash` 与运行时 `form(op, 同题面, {同声明}).hash` 逐字相等（B116 推翻条件 (1) 的正面检验）；
//! (c) V7 的 refund-do 程序导出 `cs-refund` 一项；(d) 降级失败不写文件、非零退出；参数的位置与错误。
//! 依据：B116（`地基/附注/2026-09-25-批量裁定.md` §五 (c)）；`地基/过程记录/工程-步20a-2b.md`。

use serde_json::{Value as Json, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn 目录(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-s20a2b-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn 读(p: &Path) -> Json {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

const 构造: &str = r#"budget {calls: 8, cost: 1};
let t1 = test("甲是否成立？", "lab-a");
let t2 = test("甲是否成立？", "lab-a");
let t3 = test("乙是否成立？", "lab-a", {presupposition: "前提", evidence: ["on"]});
let q4 = select("选哪个？", "lab-b", {request: "one"});
let m5 = measure("有多少？", ["少", "多"], "lab-c");
let x = "丙";
let t6 = test(x, "lab-d");
let t7 = test("丁？", x);
let t8 = test("含{括号}？", "lab-e");
let ev = ["on"];
let t9 = test("戊？", "lab-f", {evidence: ev});
let f = form("test", "己{x}？", {calib: "lab-g"});
let q10 = fill(f, {x: "庚"});
{a: t1.text, b: t2.text, c: t3.text, d: q4.text, e: m5.text, f: t6.text, g: t7.text, h: t8.text, i: t9.text, j: q10.text}
"#;

/// 与构造里四道导出题同题面、同声明的题式；程序返回它们的 `hash`（不判断、不发调用）
const 同声明题式: &str = r#"budget {calls: 1, cost: 0};
let f0 = form("test", "甲是否成立？", {calib: "z"});
let f1 = form("test", "乙是否成立？", {calib: "z", presupposition: "前提", evidence: ["on"]});
let f2 = form("select", "选哪个？", {calib: "z", request: "one"});
let f3 = form("measure", "有多少？", {calib: "z", scale: ["少", "多"]});
[f0.hash, f1.hash, f2.hash, f3.hash]
"#;

fn 导出(d: &Path) -> Json {
    fs::write(d.join("p.jpp"), 构造).unwrap();
    let o = jpp(d, &["check", "p.jpp", "--questions-out", "q.json"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        String::from_utf8_lossy(&o.stderr)
            .contains("题面导出（B116）：3 个标签、4 道题、跳过 4 处"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    读(&d.join("q.json"))
}

/// (a) 导出内容。
#[test]
fn a_导出字面题与跳过原因() {
    let d = 目录("a");
    let q = 导出(&d);
    let labels = q["labels"].as_object().unwrap();
    assert_eq!(
        labels.keys().collect::<Vec<_>>(),
        vec!["lab-a", "lab-b", "lab-c"]
    );
    let a = labels["lab-a"].as_array().unwrap();
    assert_eq!(a.len(), 2, "同标签两道不同的题各一项");
    assert_eq!(a[0]["text"], json!("甲是否成立？"));
    assert_eq!(a[0]["sites"].as_array().unwrap().len(), 2, "同题两站点合并");
    assert_eq!(a[0]["sites"][0]["line"], json!(2));
    assert_eq!(a[0]["sites"][1]["line"], json!(3));
    assert_eq!(a[1]["text"], json!("乙是否成立？"));
    assert_eq!(a[1]["presupposition"], json!("前提"));
    assert_eq!(a[1]["evidence"], json!(["on"]));
    let b = &labels["lab-b"][0];
    assert_eq!(
        (b["op"].clone(), b["request"].clone()),
        (json!("select"), json!("one"))
    );
    let c = &labels["lab-c"][0];
    assert_eq!(
        (c["op"].clone(), c["scale"].clone()),
        (json!("measure"), json!(["少", "多"]))
    );
    let 原因: Vec<&str> = q["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        原因,
        vec![
            "题面不是字面文本",
            "标签不是字面文本",
            "题面含花括号，作零槽题式会与同文本的带槽题式撞键",
            "evidence 不是字面文本列表",
        ]
    );
    let 全文 = q.to_string();
    assert!(
        !全文.contains("lab-g") && !全文.contains("己"),
        "题式与填法不导出"
    );
    let _ = fs::remove_dir_all(&d);
}

/// (b) 零槽 `form_hash` 与运行时同题面、同声明 `form()` 的 `hash` 逐字相等（B116 推翻条件 (1)）。
#[test]
fn b_零槽题式哈希与运行时form相等() {
    let d = 目录("b");
    let q = 导出(&d);
    fs::write(d.join("f.jpp"), 同声明题式).unwrap();
    let o = jpp(&d, &["run", "f.jpp", "--output", "r.json"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let 运行时 = 读(&d.join("r.json"))["value"].clone();
    let 导出的 = json!([
        q["labels"]["lab-a"][0]["form_hash"],
        q["labels"]["lab-a"][1]["form_hash"],
        q["labels"]["lab-b"][0]["form_hash"],
        q["labels"]["lab-c"][0]["form_hash"],
    ]);
    assert_eq!(导出的, 运行时);
    let _ = fs::remove_dir_all(&d);
}

/// (c) V7 的 refund-do 程序：导出 `cs-refund` 一项，题面与程序字面一致。
#[test]
fn c_v7_refund_do导出cs_refund() {
    let d = 目录("c");
    // 公开仓库副本：原路径指向研究工作区私有目录（评估/2026-09-24-V7固定序/），
    // 不在本仓库里；tools/sync-rust-from-research.sh 把这一行改写成仓库内夹具。
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/refund-do.jpp");
    let o = jpp(
        &d,
        &["check", src.to_str().unwrap(), "--questions-out", "q.json"],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let q = 读(&d.join("q.json"));
    let e = &q["labels"]["cs-refund"];
    assert_eq!(e.as_array().unwrap().len(), 1);
    assert_eq!(e[0]["op"], json!("test"));
    assert_eq!(
        e[0]["text"],
        json!("这段客服对话里，顾客是否明确提出要退款或退钱？")
    );
    assert_eq!(
        (
            e[0]["evidence"].clone(),
            e[0]["presupposition"].clone(),
            e[0]["request"].clone()
        ),
        (json!([]), Json::Null, Json::Null)
    );
    let _ = fs::remove_dir_all(&d);
}

/// (d) 降级失败不写；参数任意位置；`--questions-out` 缺值、给两次、给别的动词都是用法错误。
#[test]
fn d_降级失败不写与参数() {
    let d = 目录("d");
    fs::write(d.join("bad.jpp"), "let x = test(\"甲？\", \"k\");\n").unwrap();
    let o = jpp(&d, &["check", "bad.jpp", "--questions-out", "q.json"]);
    assert!(!o.status.success(), "缺预算应降级失败");
    assert!(!d.join("q.json").exists(), "降级失败不写");

    fs::write(d.join("p.jpp"), 构造).unwrap();
    fs::write(d.join("in.json"), "{}").unwrap();
    for argv in [
        vec![
            "check",
            "p.jpp",
            "--questions-out",
            "q1.json",
            "--input",
            "in.json",
        ],
        vec![
            "check",
            "p.jpp",
            "--input",
            "in.json",
            "--questions-out",
            "q2.json",
            "--input-trusted",
        ],
        vec!["check", "--questions-out", "q3.json", "p.jpp"],
    ] {
        let o = jpp(&d, &argv);
        assert!(
            o.status.success(),
            "{argv:?}：{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    for n in ["q1.json", "q2.json", "q3.json"] {
        assert_eq!(
            读(&d.join(n))["labels"],
            读(&d.join("q1.json"))["labels"],
            "{n}"
        );
    }
    for (argv, 报文) in [
        (
            vec!["check", "p.jpp", "--questions-out"],
            "--questions-out requires a value",
        ),
        (
            vec![
                "check",
                "p.jpp",
                "--questions-out",
                "a.json",
                "--questions-out",
                "b.json",
            ],
            "--questions-out was supplied twice",
        ),
        (
            vec!["run", "p.jpp", "--questions-out", "a.json"],
            "--questions-out is accepted only by check",
        ),
    ] {
        let o = jpp(&d, &argv);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success() && err.contains(报文), "{argv:?}：{err}");
    }
    let _ = fs::remove_dir_all(&d);
}
