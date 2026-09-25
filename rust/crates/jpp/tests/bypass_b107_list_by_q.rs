//! B107（步 20h-2）：待标清单与回接按 `(item, q)`。一道题式三种填法问同一批材料（`sieve` 的多题产物，B82），
//! 账本里一个键下每段材料有三个读数：清单要 108 行、行带 `q` 与题面（`--report`），回填按 `(item, q)` 回接；
//! 标签行不带 `q` 时报 `E-list-ambiguous`；带 `q` 的 108 行得 108 条样本、与直接导入同一条线。K 元读数的框同形。
//! 另：报告 `questions` 表的题类随回填进记录（B120 (a)）；报告 `exits` 行带 `item`、`index`、`pos`（B120 (b)）。
//! 账本用固定观察现造（不花钱）。依据：`地基/附注/2026-09-25-批量裁定.md` §一、§十；`21` 步 20h-2。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

// 步 15d-2：δ 只从校准记录取，不装画像时用测试画像（noul 0.05 / choice、score 0.15，
// 即步 15d-2 之前代码兜底的值）让这些测试保持原来的线。
const 画像: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json");

fn 跑(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn 目录(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-b107-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

const 填法: [&str; 3] = ["物流", "价格", "服务"];

fn 真值(i: usize, f: usize) -> bool {
    (i + f).is_multiple_of(2)
}

/// 36 段评论 × 三种填法，固定观察跑一遍：读数按真值取两极（真 0.97、假 0.03）。留账本与报告。
fn 首跑(d: &Path) -> Vec<String> {
    let mats: Vec<String> = (0..36).map(|i| format!("第{i}条评论：还行。")).collect();
    let mut obs = vec![];
    for (i, m) in mats.iter().enumerate() {
        for (f, t) in 填法.iter().enumerate() {
            obs.push(json!({"on": [m], "op": "test", "text": format!("这段评论是否提到了{t}？"), "calib": "k",
                            "answer": {"Noul": if 真值(i, f) { 0.97 } else { 0.03 }}}));
        }
    }
    fs::write(d.join("fx.json"), json!({"observations": obs}).to_string()).unwrap();
    let list = mats
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        d.join("p.jpp"),
        format!(
            "budget {{calls: 200, cost: 1, depth: 256}};\nlet f = form(\"test\", \"这段评论是否提到了{{t}}？\", {{calib: \"k\"}});\nsieve([{list}], map([\"物流\", \"价格\", \"服务\"], fn(t) {{ fill(f, {{t: t}}) }}))\n"
        ),
    )
    .unwrap();
    let o = 跑(
        d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "led.jsonl",
            "--output",
            "report.json",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    mats
}

fn 读(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn 清单(d: &Path, report: bool) -> (Vec<Value>, String) {
    let mut args = vec![
        "calib-import",
        "--from-ledger",
        "led.jsonl",
        "--key",
        "k",
        "--list-out",
        "list.jsonl",
        "--profile",
        画像,
    ];
    if report {
        args.extend(["--report", "report.json"]);
    }
    let o = 跑(d, &args);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    (
        读(&d.join("list.jsonl")),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

/// 报告 `questions` 表：题哈希 → 填法
fn 题表(d: &Path) -> std::collections::BTreeMap<String, String> {
    let r: Value =
        serde_json::from_str(&fs::read_to_string(d.join("report.json")).unwrap()).unwrap();
    r["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["q"].as_str().unwrap().to_string(),
                x["fill"]["t"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// (a) 清单 108 行，每行 (item, q) 各不相同、带题面与填法，不带读数；报告的 exits 行带 item、index、pos。
#[test]
fn list_has_one_row_per_item_and_question() {
    let d = 目录("list");
    首跑(&d);
    let (rows, err) = 清单(&d, true);
    assert_eq!(rows[0]["list"]["n"], 108);
    let body = &rows[1..];
    assert_eq!(body.len(), 108);
    let ids: std::collections::BTreeSet<(String, String)> = body
        .iter()
        .map(|r| {
            (
                r["item"].as_str().unwrap().into(),
                r["q"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(ids.len(), 108, "(item, q) 各不相同");
    for r in body {
        assert!(r.get("p").is_none() && r.get("exit").is_none(), "{r}");
        assert!(r["template"].as_str().unwrap().contains("{t}"), "{r}");
        assert!(填法.contains(&r["fill"]["t"].as_str().unwrap()), "{r}");
    }
    assert!(!err.contains("W-list-no-question"), "{err}");
    // 不带 --report：多题键报 W-list-no-question，行里只有题哈希
    let (rows2, err2) = 清单(&d, false);
    assert!(err2.contains("W-list-no-question"), "{err2}");
    assert!(rows2[1].get("template").is_none());
    // B120 (b)：exits 行带 item（状态哈希前 12 位）、index、pos
    let r: Value =
        serde_json::from_str(&fs::read_to_string(d.join("report.json")).unwrap()).unwrap();
    let ex = r["exits"].as_array().unwrap();
    assert_eq!(ex.len(), 108);
    assert!(ex.iter().all(|e| e["item"].as_str().unwrap().len() == 12
        && e["index"].is_i64()
        && e["pos"] == e["index"]));
    let _ = fs::remove_dir_all(&d);
}

fn 标签行(d: &Path, rows: &[Value], with_q: bool) -> String {
    let 填 = 题表(d);
    let mats: Vec<String> = (0..36).map(|i| format!("第{i}条评论：还行。")).collect();
    // 材料 → 编号：清单行只有状态哈希；这里按首跑同算法对回（与 --materials 同）
    let 号: std::collections::BTreeMap<String, usize> = {
        fs::write(d.join("mats.json"), serde_json::to_string(&mats).unwrap()).unwrap();
        let o = 跑(
            d,
            &[
                "calib-import",
                "--from-ledger",
                "led.jsonl",
                "--key",
                "k",
                "--list-out",
                "list-m.jsonl",
                "--materials",
                "mats.json",
                "--report",
                "report.json",
                "--profile",
                画像,
            ],
        );
        assert!(o.status.success());
        读(&d.join("list-m.jsonl"))[1..]
            .iter()
            .map(|r| {
                let m = r["material"].as_str().unwrap();
                (
                    r["item"].as_str().unwrap().to_string(),
                    mats.iter().position(|x| x == m).unwrap(),
                )
            })
            .collect()
    };
    let lines: Vec<String> = rows[1..]
        .iter()
        .map(|r| {
            let item = r["item"].as_str().unwrap();
            let q = r["q"].as_str().unwrap();
            let f = 填法.iter().position(|t| *t == 填[q]).unwrap();
            let mut v =
                json!({"key": "k", "item": item, "label": 真值(号[item], f), "source": "computed"});
            if with_q {
                v["q"] = json!(q);
            }
            v.to_string()
        })
        .collect();
    lines.join("\n") + "\n"
}

/// (b) 标签行不带 q：同一材料在该键下有三个读数 → E-list-ambiguous，不合并、不认证。
#[test]
fn labels_without_q_are_ambiguous() {
    let d = 目录("amb");
    首跑(&d);
    let (rows, _) = 清单(&d, true);
    fs::write(d.join("lab.jsonl"), 标签行(&d, &rows, false)).unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--calib-out",
            "c",
            "--profile",
            画像,
        ],
    );
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("E-list-ambiguous"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

/// (c) 带 q 的 108 行 → 108 条样本；与直接导入（每行带 form 与填法、各自编号）同一条线；记录题类取报告的精化类。
#[test]
fn labels_with_q_give_one_sample_each_and_match_direct_import() {
    let d = 目录("q");
    首跑(&d);
    let (rows, _) = 清单(&d, true);
    fs::write(d.join("lab.jsonl"), 标签行(&d, &rows, true)).unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--report",
            "report.json",
            "--certify",
            "fixed-sequence",
            "--calib-out",
            "c1",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r1: Value =
        serde_json::from_str(&fs::read_to_string(d.join("c1/k.json")).unwrap()).unwrap();
    assert_eq!(r1["status"], "上岗");
    let n_lab = r1["samples"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| !s["label"].is_null())
        .count();
    assert_eq!(n_lab, 108, "每个 (item, q) 一条样本");
    assert_eq!(r1["kind"], "attr", "题类取报告的精化类");
    // 直接导入：同一批读数与真值，每行 key k、编号各不相同
    let direct: Vec<String> = (0..36)
        .flat_map(|i| {
            (0..3).map(move |f| {
                let t = 真值(i, f);
                json!({"key": "k", "item": format!("m{i}-{f}"), "p": if t { 0.97 } else { 0.03 }, "label": t, "source": "computed"})
                    .to_string()
            })
        })
        .collect();
    fs::write(d.join("direct.jsonl"), direct.join("\n") + "\n").unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "direct.jsonl",
            "--calib-out",
            "c2",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r2: Value =
        serde_json::from_str(&fs::read_to_string(d.join("c2/k.json")).unwrap()).unwrap();
    assert_eq!((&r1["hi"], &r1["lo"]), (&r2["hi"], &r2["lo"]), "同一条线");
    let _ = fs::remove_dir_all(&d);
}

/// (d) K 元读数进框：清单行带 q、不带读数与 argmax；回填按 (item, q) 接回 p_max 与 pick，认证单侧线。
#[test]
fn kary_readings_enter_the_frame() {
    let d = 目录("kary");
    let mats: Vec<String> = (0..60).map(|i| format!("第{i}份合同。")).collect();
    let obs: Vec<Value> = mats
        .iter()
        .map(|m| {
            json!({"on": [m], "over": ["租赁", "买卖"], "op": "select", "text": "这是哪类合同？", "calib": "k",
                   "perms": 2, "mode_share": 1.0, "answer": {"Choice": [0.97, 0.03]}})
        })
        .collect();
    fs::write(d.join("fx.json"), json!({"observations": obs}).to_string()).unwrap();
    let list = mats
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(d.join("p.jpp"), format!(
        "budget {{calls: 100, cost: 1, depth: 256}};\nmap([{list}], fn(m) {{ let e = cut(judge(state(mat(m), {{over: [\"租赁\", \"买卖\"]}}), select(\"这是哪类合同？\", \"k\"))); consume(e, \"drop\"); 0 }})\n"
    )).unwrap();
    let o = 跑(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "led.jsonl",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let (rows, _) = 清单(&d, false);
    assert_eq!(rows[0]["list"]["n"], 60);
    for r in &rows[1..] {
        assert!(r.get("p").is_none() && r.get("pick").is_none(), "{r}");
        assert!(r["q"].is_string());
    }
    // 真值全是候选 0（读数都把 0.97 给候选 0），argmax 全对
    let lab: Vec<String> = rows[1..]
        .iter()
        .map(|r| {
            json!({"key": "k", "op": "select", "item": r["item"], "q": r["q"], "label": 0, "source": "computed"}).to_string()
        })
        .collect();
    fs::write(d.join("lab.jsonl"), lab.join("\n") + "\n").unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--certify",
            "fixed-sequence",
            "--calib-out",
            "c",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("c/k.json")).unwrap()).unwrap();
    assert_eq!(r["status"], "上岗", "{r}");
    assert!(
        r["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["phys"] == "choice")
    );
    let _ = fs::remove_dir_all(&d);
}

/// (e) B120 (a)：标签行的题类——`slot_shape: pair` 算出关系类进记录；同键各行题类矛盾 → `E-kind-conflict`。
#[test]
fn row_kind_and_conflict() {
    let d = 目录("kind");
    let rows = |extra: &dyn Fn(usize) -> Value| -> String {
        (0..60)
            .map(|i| {
                let t = i % 2 == 0;
                let mut v = json!({"key": "k", "item": format!("m{i}"), "p": if t { 0.97 } else { 0.03 }, "label": t, "source": "computed"});
                for (k, x) in extra(i).as_object().unwrap() {
                    v[k] = x.clone();
                }
                v.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    fs::write(
        d.join("pair.jsonl"),
        rows(&|_| json!({"slot_shape": "pair"})),
    )
    .unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "pair.jsonl",
            "--calib-out",
            "c1",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_str(&fs::read_to_string(d.join("c1/k.json")).unwrap()).unwrap();
    assert_eq!(r["kind"], "rel");
    fs::write(
        d.join("bad.jsonl"),
        rows(&|i| json!({"kind": if i == 0 { "rel" } else { "attr" }})),
    )
    .unwrap();
    let o = 跑(&d, &["calib-import", "bad.jsonl", "--calib-out", "c2"]);
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("E-kind-conflict"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let _ = fs::remove_dir_all(&d);
}
