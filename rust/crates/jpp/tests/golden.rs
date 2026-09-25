//! 步 0 门禁：示例金样、重放一致性与语义投影（21 §二·1、§二·2、§二·4；20 §11.5）。
//!
//! 依据：21 §二·1 第 2、3 条——每个 `examples/*.jpp` 在固定观察下运行一次，报告与账本
//! 与 `tests/golden/<用例>/` 逐字节相同；用该账本重放，新增调用 0、值相同、重放报告逐字节相同。
//! 21 §二·2——语义投影在步 0 写好并对 HEAD 自检，供步 7、20a 的格式步比对。
//!
//! 金样只证明「重构没有改变行为」，不证明行为正确（21 §六·2），不计入任何总账条目。
//! 录制或按 S 步预注册更新：`JPP_GOLDEN_UPDATE=1 cargo test -p jpp --test golden`。
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

fn updating() -> bool {
    std::env::var("JPP_GOLDEN_UPDATE").is_ok_and(|v| v == "1")
}

/// 去掉与运行环境有关、与行为无关的部分：绝对路径换成占位符。清单见 `tests/golden/NORMALIZE.md`。
fn normalize(text: &str, tmp: &Path) -> String {
    let tmp_s = tmp.canonicalize().unwrap_or(tmp.to_path_buf());
    text.replace(&tmp_s.display().to_string(), "<TMP>")
        .replace(&tmp.display().to_string(), "<TMP>")
        .replace(&root().display().to_string(), "<ROOT>")
}

/// 语义投影（21 §二·2）：跨账本或校准键格式仍应相等的那部分行为。
/// 现行报告里取不到的两项（每个 `cut` 站点的出口、层数）记在 `tests/golden/NORMALIZE.md`，
/// 步 7 账本 v2 带上 `layer` 后补入。
pub fn project(report: &Value) -> Value {
    let events = report["trace"]["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let effects: Vec<Value> = events
        .iter()
        .filter(|e| matches!(e["kind"].as_str(), Some("do" | "ask" | "gen")))
        .map(|e| json!([e["kind"], e["note"], e["key"], e["replayed"]]))
        .collect();
    let judges = events.iter().filter(|e| e["kind"] == "judge").count();
    let pending: Vec<Value> = report["pending"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| json!([p["cause"], p["site"]]))
        .collect();
    json!({
        "status": report["status"],
        "value": report["value"],
        "returned_unsure": report["returned_unsure"],
        "pending": pending,
        "effects": effects,
        "judge_registrations": judges,
        "calls": report["cost"]["calls"],
        "asks": report["cost"]["asks"],
        "usd": report["cost"]["usd"],
    })
}

struct Case {
    name: String,
    source: String,
    fixtures: Option<String>,
    calib: Option<String>,
    files: Vec<(String, String)>,
    resume_from: Option<String>,
    expect_error: bool,
    /// 清单里登记的重放不成功（停机或分歧）及原因；未登记的用例重放必须成功。
    replay_exception: Option<String>,
}

fn cases() -> Vec<Case> {
    let m: Value =
        serde_json::from_slice(&fs::read(root().join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    m["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Case {
            name: c["name"].as_str().unwrap().into(),
            source: c["source"].as_str().unwrap().into(),
            fixtures: c["fixtures"].as_str().map(String::from),
            calib: c["calib"].as_str().map(String::from),
            files: c["files"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            resume_from: c["resume_from"].as_str().map(String::from),
            expect_error: c["expect"] == "error",
            replay_exception: c["replay"].as_str().map(String::from),
        })
        .collect()
}

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-golden-{}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(cwd: &Path, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

fn base_args(c: &Case) -> Vec<String> {
    let r = root();
    let mut a = vec!["run".into(), r.join(&c.source).display().to_string()];
    if let Some(f) = &c.fixtures {
        a.push("--fixtures".into());
        a.push(r.join(f).display().to_string());
    }
    a
}

/// 与金样逐字节比较；更新模式下写入。返回差异说明（空即一致）。
fn check(dir: &Path, file: &str, actual: &str, diffs: &mut Vec<String>) {
    let path = dir.join(file);
    if updating() {
        fs::create_dir_all(dir).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    match fs::read_to_string(&path) {
        Ok(expected) if expected == actual => {}
        Ok(expected) => {
            let line = expected
                .lines()
                .zip(actual.lines())
                .position(|(a, b)| a != b)
                .map(|i| i + 1)
                .unwrap_or(expected.lines().count().min(actual.lines().count()) + 1);
            diffs.push(format!("{}：与金样不同（第 {line} 行起）", path.display()));
        }
        Err(_) => diffs.push(format!(
            "{}：缺金样。修法：JPP_GOLDEN_UPDATE=1 cargo test -p jpp --test golden 录制，并在提交说明里写明原因",
            path.display()
        )),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap() + "\n"
}

/// 21 §二·1 第 2、3 条：全部示例的金样与重放。
#[test]
fn examples_match_golden_and_replay() {
    let base = scratch();
    let golden = root().join("tests/golden");
    let mut diffs = vec![];
    for c in cases() {
        let tmp = base.join(&c.name);
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        for (name, body) in &c.files {
            fs::write(tmp.join(name), body).unwrap();
        }
        let gdir = golden.join(&c.name);
        let mut args = base_args(&c);
        if c.expect_error {
            let out = jpp(&tmp, &args);
            assert!(!out.status.success(), "{}：预期失败却成功", c.name);
            let err = normalize(&String::from_utf8_lossy(&out.stderr), &tmp);
            check(&gdir, "stderr.txt", &err, &mut diffs);
            continue;
        }
        if let Some(cal) = &c.calib {
            args.push("--calib".into());
            args.push(root().join(cal).display().to_string());
        }
        if let Some(from) = &c.resume_from {
            args.push("--resume".into());
            args.push(base.join(from).join("ledger.json").display().to_string());
        }
        args.extend([
            "--ledger-out".into(),
            "ledger.json".into(),
            "--output".into(),
            "report.json".into(),
        ]);
        let out = jpp(&tmp, &args);
        assert!(
            out.status.success(),
            "{}：运行失败：{}",
            c.name,
            String::from_utf8_lossy(&out.stderr)
        );
        let report_text = normalize(&fs::read_to_string(tmp.join("report.json")).unwrap(), &tmp);
        let ledger_text = normalize(&fs::read_to_string(tmp.join("ledger.json")).unwrap(), &tmp);
        let report: Value = serde_json::from_str(&report_text).unwrap();
        check(&gdir, "report.json", &report_text, &mut diffs);
        check(&gdir, "ledger.json", &ledger_text, &mut diffs);
        check(
            &gdir,
            "projection.json",
            &pretty(&project(&report)),
            &mut diffs,
        );

        // 重放：只凭账本（加同一份固定观察，供 ask 回答与生成夹具）；新增调用 0、值相同。
        let mut rargs = base_args(&c);
        rargs.extend([
            "--replay".into(),
            tmp.join("ledger.json").display().to_string(),
            "--output".into(),
            "replay-report.json".into(),
        ]);
        let rtmp = tmp.join("replay");
        fs::create_dir_all(&rtmp).unwrap();
        for (name, body) in &c.files {
            fs::write(rtmp.join(name), body).unwrap();
        }
        let rout = jpp(&rtmp, &rargs);
        if let Some(why) = &c.replay_exception {
            // 清单登记的例外：重放必须照登记的样子失败，报文进金样；登记的原因写在 manifest。
            assert!(
                !rout.status.success(),
                "{}：登记为重放例外（{why}），却重放成功；请删去登记",
                c.name
            );
            let err = normalize(&String::from_utf8_lossy(&rout.stderr), &tmp);
            check(&gdir, "replay-stderr.txt", &err, &mut diffs);
            continue;
        }
        assert!(
            rout.status.success(),
            "{}：重放失败：{}",
            c.name,
            String::from_utf8_lossy(&rout.stderr)
        );
        let rtext = normalize(
            &fs::read_to_string(rtmp.join("replay-report.json")).unwrap(),
            &tmp,
        );
        let replay: Value = serde_json::from_str(&rtext).unwrap();
        assert_eq!(replay["cost"]["calls"], 0, "{}：重放新增了调用", c.name);
        assert_eq!(replay["cost"]["asks"], 0, "{}：重放新增了提问", c.name);
        assert_eq!(
            replay["value"], report["value"],
            "{}：重放值与首跑不同",
            c.name
        );
        check(&gdir, "replay-report.json", &rtext, &mut diffs);
    }
    let _ = fs::remove_dir_all(&base);
    assert!(
        diffs.is_empty(),
        "金样门禁变红（{} 处）：\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// 21 步 12a：每个示例的 IR 通过 `wellformed`，两次打印逐字节相同，打印存入金样 `ir.txt`。
/// `ir.txt` 是新增产物，只要求与自身一致，不参与行为判定。步 12d 起 IR 由 `jpp::lower` 直接产出。
/// 预期报错的示例：能降级的照样打印；降级报带规则号的诊断（缺预算 `J-07a`）的打印诊断；语法错的不产出。
#[test]
fn ir_is_wellformed_and_prints_stably() {
    use jpp::ir;
    use jpp::names::CurrentNames;
    let golden = root().join("tests/golden");
    let mut diffs = vec![];
    for c in cases() {
        let path = root().join(&c.source);
        let Ok(loaded) = jpp_syntax::loader::load(&path) else {
            assert!(c.expect_error, "{}：前端装载失败", c.name);
            continue;
        };
        let (first, annotated) = match jpp::lower(&loaded.program) {
            Ok(prog) => {
                let built = ir::wellformed(&prog, &CurrentNames);
                if !c.expect_error {
                    assert!(built.is_ok(), "{}：IR 不良构：{:?}", c.name, built.err());
                }
                let first = match &built {
                    Ok(()) => ir::print(&prog, None),
                    Err(ds) => ds
                        .iter()
                        .map(|d| {
                            format!(
                                "{} {}..{} {}\n",
                                d.code, d.span.start, d.span.end, d.message
                            )
                        })
                        .collect(),
                };
                assert_eq!(
                    ir::print(&prog, None),
                    ir::print(&prog, None),
                    "{}：IR 两次打印不同",
                    c.name
                );
                let annotated = match &built {
                    Ok(()) => {
                        let (_, annot) = jpp::check::check_annotated(&prog, None, None);
                        let text = ir::print(&prog, Some(&annot));
                        let (_, again) = jpp::check::check_annotated(&prog, None, None);
                        assert_eq!(
                            text,
                            ir::print(&prog, Some(&again)),
                            "{}：标注打印两次不同",
                            c.name
                        );
                        text
                    }
                    Err(_) => first.clone(),
                };
                (first, annotated)
            }
            // 降级报的带规则号诊断（`J-07a: …`）照 IR 诊断的格式打印；不带规则号的是表层错，不产出
            Err(ds) if ds[0].message.starts_with("J-") || ds[0].message.starts_with("E-") => {
                assert!(c.expect_error, "{}：降级失败：{ds:?}", c.name);
                let text: String = ds
                    .iter()
                    .map(|d| {
                        let (code, msg) = d.message.split_once(": ").unwrap_or(("", &d.message));
                        format!("{} {}..{} {}\n", code, d.span.start, d.span.end, msg)
                    })
                    .collect();
                (text.clone(), text)
            }
            Err(ds) => {
                assert!(c.expect_error, "{}：lower 失败：{ds:?}", c.name);
                continue;
            }
        };
        check(&golden.join(&c.name), "ir.txt", &first, &mut diffs);
        check(&golden.join(&c.name), "annot.txt", &annotated, &mut diffs);
    }
    assert!(
        diffs.is_empty(),
        "IR 金样变红（{} 处）：\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// 21 §二·2 投影自检：同一格式上投影是恒等式；且投影对它声称覆盖的每一类字段都敏感，
/// 不会因为丢字段而把不同的行为投成相同。
#[test]
fn projection_is_sensitive_to_each_field() {
    let report = json!({
        "status": "returned", "value": {"x": 1}, "returned_unsure": [],
        "pending": [{"cause": "band", "site": {"start": 1, "end": 2}, "key": "k", "detail": "d"}],
        "cost": {"calls": 3, "asks": 1, "usd": 0.5},
        "trace": {"events": [
            {"kind": "judge", "key": "a", "note": "q", "replayed": false},
            {"kind": "do", "key": "b", "note": "write_json", "replayed": false},
        ]}
    });
    let p = project(&report);
    assert_eq!(p, project(&report.clone()));
    type Mutation = (&'static str, Box<dyn Fn(&mut Value)>);
    let mutations: Vec<Mutation> = vec![
        ("value", Box::new(|r| r["value"]["x"] = json!(2))),
        ("status", Box::new(|r| r["status"] = json!("pending"))),
        (
            "pending cause",
            Box::new(|r| r["pending"][0]["cause"] = json!("cold")),
        ),
        (
            "pending site",
            Box::new(|r| r["pending"][0]["site"]["start"] = json!(9)),
        ),
        (
            "do 序列",
            Box::new(|r| r["trace"]["events"][1]["note"] = json!("read_json")),
        ),
        (
            "判断登记数",
            Box::new(|r| r["trace"]["events"][0]["kind"] = json!("gen")),
        ),
        ("调用数", Box::new(|r| r["cost"]["calls"] = json!(4))),
        ("费用", Box::new(|r| r["cost"]["usd"] = json!(0.6))),
        (
            "returned_unsure",
            Box::new(|r| r["returned_unsure"] = json!(["unsure(band)"])),
        ),
    ];
    for (what, m) in mutations {
        let mut r = report.clone();
        m(&mut r);
        assert_ne!(project(&r), p, "投影对「{what}」不敏感");
    }
    // 不属于行为的字段（报文细节、账本键以外的说明）变化时投影不变。
    let mut r = report.clone();
    r["pending"][0]["detail"] = json!("另一段说明");
    assert_eq!(project(&r), p);
}
