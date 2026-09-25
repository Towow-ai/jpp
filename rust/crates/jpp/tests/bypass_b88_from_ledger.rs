//! B88：首跑账本作抽样框，导出待标清单（不带读数与出口），回填后按两端先标的顺序序贯导入（B87）。
//! 账本用固定观察现造（不花钱）。依据：`地基/附注/2026-09-24-标注门槛裁定.md` §三；`21` 步 20h。
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
    let d = std::env::temp_dir().join(format!("jpp-b88-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

const 题: &str = "这段话是否表达了同意？";

/// 60 段材料（30 段「同意」读数 0.97，30 段「拒绝」读数 0.03），固定观察跑一遍，留账本。
fn 首跑(d: &Path) -> Vec<String> {
    let mats: Vec<String> = (0..60)
        .map(|i| {
            if i % 2 == 0 {
                format!("同意第{i}号方案。")
            } else {
                format!("拒绝第{i}号方案。")
            }
        })
        .collect();
    let obs: Vec<Value> = mats
        .iter()
        .map(|m| {
            json!({"on": [m], "op": "test", "text": 题, "calib": "k",
                        "answer": {"Noul": if m.starts_with("同意") { 0.97 } else { 0.03 }}})
        })
        .collect();
    fs::write(d.join("fx.json"), json!({"observations": obs}).to_string()).unwrap();
    let list = mats
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        d.join("p.jpp"),
        format!(
            "budget {{calls: 100, cost: 1, depth: 256}};\nlet q = test({题:?}, \"k\");\nmap([{list}], fn(m) {{ handle(cut(judge(state(mat(m)), q)), {{act: fn() {{ 1 }}, ignore: fn() {{ 0 }}, unsure: fn(u) {{ consume(u, \"drop\"); -1 }}}}) }})\n"
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
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    fs::write(d.join("mats.json"), serde_json::to_string(&mats).unwrap()).unwrap();
    mats
}

fn 清单(d: &Path, out: &str, seed: &str) -> Vec<Value> {
    let o = 跑(
        d,
        &[
            "calib-import",
            "--from-ledger",
            "led.jsonl",
            "--key",
            "k",
            "--list-out",
            out,
            "--materials",
            "mats.json",
            "--seed",
            seed,
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    fs::read_to_string(d.join(out))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// (a)(b)(c) 清单：行只带 item、group、material；从两端向内分组；同账本同种子两次导出逐字节相同，换种子组内顺序变。
#[test]
fn 清单不带读数且可复现() {
    let d = 目录("list");
    首跑(&d);
    let rows = 清单(&d, "l1.jsonl", "7");
    assert_eq!(rows[0]["list"]["n"], 60);
    for r in &rows[1..] {
        let keys: Vec<&str> = r.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert!(
            keys.iter()
                .all(|k| ["item", "q", "group", "material"].contains(k)),
            "{r}"
        );
        assert!(r.get("p").is_none() && r.get("exit").is_none());
        assert!(r["material"].is_string(), "材料对上：{r}");
    }
    // 从两端向内：首组是上 1 / 下 1，且「上」组的材料全是高读数那一端
    assert!(rows[1]["group"].as_str().unwrap().ends_with('1'));
    for r in &rows[1..] {
        let g = r["group"].as_str().unwrap();
        let m = r["material"].as_str().unwrap();
        if g.starts_with('上') {
            assert!(m.starts_with("同意"), "{r}");
        }
        if g.starts_with('下') {
            assert!(m.starts_with("拒绝"), "{r}");
        }
    }
    清单(&d, "l2.jsonl", "7");
    assert_eq!(
        fs::read(d.join("l1.jsonl")).unwrap(),
        fs::read(d.join("l2.jsonl")).unwrap()
    );
    let other = 清单(&d, "l3.jsonl", "8");
    assert_ne!(rows, other, "换种子组内顺序变");
    let groups = |xs: &[Value]| {
        xs[1..]
            .iter()
            .map(|r| r["group"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(groups(&rows), groups(&other), "分组只由读数定");
}

fn 回填(rows: &[Value], take: std::ops::Range<usize>) -> String {
    rows[1..][take]
        .iter()
        .map(|r| {
            json!({"key": "k", "item": r["item"], "label": r["material"].as_str().unwrap().starts_with("同意"), "source": "computed"})
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// (d) 回填全部 → 缺省序贯、两端先标上岗；(e) 已标行不是清单前缀 → 拒收；(f) 行里读数与账本不符 → 拒收。
#[test]
fn 回填导入() {
    let d = 目录("fill");
    首跑(&d);
    let rows = 清单(&d, "l.jsonl", "7");
    fs::write(d.join("lab.jsonl"), 回填(&rows, 0..60)).unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--seed",
            "7",
            "--calib-out",
            "c",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rep: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rep[0]["status"], "上岗", "{rep}");
    let sel = &rep[0]["certification"]["selection"];
    assert_eq!(sel["method"], "sequential");
    assert_eq!(sel["sequential"]["order"], "two-ends");
    // (e) 只标清单后 30 条：不是前缀
    fs::write(d.join("lab2.jsonl"), 回填(&rows, 30..60)).unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab2.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--seed",
            "7",
            "--calib-out",
            "c2",
            "--profile",
            画像,
        ],
    );
    let rep: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        rep[0]["truth"]["gate"]
            .as_str()
            .unwrap()
            .starts_with("待核：两端先标的标注不是清单顺序的前缀"),
        "{rep}"
    );
    // (f) 读数与账本不符
    let mut bad: Value = serde_json::from_str(回填(&rows, 0..1).lines().next().unwrap()).unwrap();
    bad["p"] = json!(0.5);
    fs::write(d.join("lab3.jsonl"), bad.to_string()).unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "lab3.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--calib-out",
            "c3",
        ],
    );
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("与账本"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// PR35 评审修复（缺陷一）：`--from-ledger` 此前逐行反序列化跳过头行，从不校验版本、`seq` 序号与
/// `prev` 哈希链。篡改一条有后继的 Judge 读数（链没有重算）后，`--list-out` 导出与回填导入都必须
/// 报 `E-ledger-corrupt` 且不写任何输出——不能把改过的读数悄悄当合法抽样框用。
#[test]
fn 篡改账本被拒收() {
    let d = 目录("tamper");
    首跑(&d);
    let led = fs::read_to_string(d.join("led.jsonl")).unwrap();
    // 第一条 Judge 的读数是 0.97（i=0「同意」那条），后面还有 59 行：只改第一处，让下一行的
    // `prev` 对不上（链哈希记的是「上一行文本的哈希」，改了这一行内容后它自己的 prev 校验仍然
    // 通过，但它产出的新哈希和原链记的不一致，链断的判定发生在下一行）。
    assert!(
        led.contains("\"Noul\":0.97"),
        "前提：账本里有 0.97 的读数：{led}"
    );
    let tampered = led.replacen("\"Noul\":0.97", "\"Noul\":0.5", 1);
    assert_ne!(tampered, led, "确实改动了内容");
    fs::write(d.join("led.jsonl"), &tampered).unwrap();

    // --list-out 导出：要拒收，不写清单文件
    let o = 跑(
        &d,
        &[
            "calib-import",
            "--from-ledger",
            "led.jsonl",
            "--key",
            "k",
            "--list-out",
            "tampered-list.jsonl",
            "--seed",
            "7",
            "--profile",
            画像,
        ],
    );
    assert!(!o.status.success());
    assert!(
        o.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("E-ledger-corrupt"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        !d.join("tampered-list.jsonl").exists(),
        "拒收时不写清单文件"
    );

    // 回填导入：同样拒收，不写 --calib-out 目录（标签行内容本身不重要，账本先被拒收）
    fs::write(
        d.join("tampered-lab.jsonl"),
        r#"{"key": "k", "item": "x", "label": true, "source": "computed"}"#,
    )
    .unwrap();
    let o = 跑(
        &d,
        &[
            "calib-import",
            "tampered-lab.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--calib-out",
            "tampered-co",
            "--profile",
            画像,
        ],
    );
    assert!(!o.status.success());
    assert!(
        o.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("E-ledger-corrupt"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        !d.join("tampered-co").exists(),
        "拒收时不写 --calib-out 目录"
    );
}
