//! 绕过测试 B91（步 20d-2）：范围扩展按 α 分档。在新风格材料上只验记录已有的线对，9 + 9 零错并入试用级扩展
//! （扩展内出口等级 `Trial`、不放行），22 + 22 零错并入与记录同级的扩展（放行）；10 条复核级的批次清不掉 `scope_out`。
//! 依据：`地基/附注/2026-09-24-标注门槛裁定.md` §七；`21` §四·8「步 20d-2 追加」。

use jpp::effects::{CalibStore, EffectError, ExtendOptions, ExtendRow, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::json;

fn 桩端口() -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("m", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.97)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

const 短: &str = "顾客说要退款";

fn 长() -> String {
    "顾客在电话里说，上周买的这台洗衣机第二次用就漏水了，师傅上门看过说是密封圈的问题，但他不想再修，希望直接退款。".repeat(3)
}

/// 正式认证的线：120 条两极构造真值，材料都是短句（范围指纹只覆盖短句）
fn 库() -> CalibStore {
    let rows: Vec<LabelRow> = (0..120)
        .map(|i| {
            let yes = i % 2 == 0;
            serde_json::from_value(json!({
                "key": "k", "item": format!("m{i}"), "p": if yes { 0.95 } else { 0.05 },
                "label": yes, "source": "computed", "text": 短
            }))
            .unwrap()
        })
        .collect();
    let opt = ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b91".into(),
        seed: 20260925,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        certify: CertifyMethod::FixedSequence,
        step: None,
        sequential: None,
    };
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 没有记录自带 δ 时取画像先验；用步 15d-2 前的代码兜底值让这条线保持原样
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    import_labels(&mut store, &rows, &opt).unwrap();
    assert_eq!(store.get("k").status, "上岗");
    store
}

/// 新风格批：`pos` 条真（读数 0.97）、`neg` 条假（读数 0.03），`err_up` 条真值为假却读数 0.97
fn 批(pos: usize, neg: usize, err_up: usize) -> Vec<ExtendRow> {
    let t = 长();
    let mut v: Vec<ExtendRow> = (0..pos)
        .map(|i| ExtendRow {
            p: 0.97,
            label: i >= err_up,
            text: t.clone(),
        })
        .collect();
    v.extend((0..neg).map(|_| ExtendRow {
        p: 0.03,
        label: false,
        text: t.clone(),
    }));
    v
}

fn 选项() -> ExtendOptions {
    ExtendOptions {
        alpha_trial: Some(0.25),
        quantiles: (0.01, 0.99),
        margins: Default::default(),
        batch: "新风格".into(),
    }
}

fn 跑(calib: &CalibStore) -> Result<jpp::Outcome, String> {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
handle(cut(judge(state(mat({:?})), test("该退吗", "k"))), {{
    act: fn() {{ content(do("退款", [], 0)) }},
    ignore: fn() {{ "没退" }},
    unsure: fn(u) {{ consume(u, "drop"); "没退" }}}})
"#,
        长()
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    run(&program, 桩端口(), calib, &a, &mut Ledger::new()).map_err(|e| e.render())
}

/// 没有扩展时：长材料在范围外，J-08 拒（本测试的前提）。
fn 范围外被拒(s: &CalibStore) {
    let e = 跑(s).expect_err("范围外不放行");
    assert!(e.contains("J-08"), "{e}");
}

/// (a) 9 + 9 零错 → 试用级扩展；扩展内出口等级 Trial、不 scope_out、报 W-scope-extension，J-08 仍拒。
#[test]
fn trial_extension_routes_but_does_not_release() {
    let mut s = 库();
    范围外被拒(&s);
    let ext = s.extend_scope("k", &批(9, 9, 0), &选项()).unwrap();
    assert_eq!((ext.alpha, ext.n_up, ext.n_down), (0.25, 9, 9));
    let e = 跑(&s).expect_err("试用级扩展不放行");
    assert!(e.contains("J-08"), "{e}");
    // 路由照常：不接动作，只看出口表
    let src = format!(
        "budget {{calls: 4, cost: 1, depth: 8}};\nlet e = cut(judge(state(mat({:?})), test(\"该退吗\", \"k\")));\nlet r = exit_kind(e);\nconsume(e, \"drop\");\nr\n",
        长()
    );
    let program = lower(&parse(&src).unwrap()).unwrap();
    let o = run(
        &program,
        桩端口(),
        &s,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap();
    assert_eq!(o.exits[0]["grade"], json!("Trial"), "{:?}", o.exits);
    assert_eq!(o.exits[0]["releases"], json!(false));
    assert!(o.exits[0].get("scope_out").is_none());
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-scope-extension"))
    );
}

/// (b) 22 + 22 零错 → 与记录同级的扩展；扩展内出口 Certified、放行。
#[test]
fn formal_extension_releases() {
    let mut s = 库();
    let ext = s.extend_scope("k", &批(22, 22, 0), &选项()).unwrap();
    assert_eq!(ext.alpha, 0.1);
    let o = 跑(&s).expect("正式扩展放行");
    assert_eq!(o.value_json(), json!("已退"));
    assert_eq!(o.exits[0]["grade"], json!("Certified"));
}

/// (c) 10 条零错（5 + 5，复核级批次）→ 样本不足，记录不动，仍范围外。
#[test]
fn ten_row_review_does_not_clear_scope_out() {
    let mut s = 库();
    let before = s.records["k"].clone();
    let e = s.extend_scope("k", &批(5, 5, 0), &选项()).unwrap_err();
    assert!(e.contains("样本不足"), "{e}");
    assert_eq!(s.records["k"], before);
    范围外被拒(&s);
}

/// (d) 上侧有错：正式上界超 α、试用也不过 → 拒，记录不动。
#[test]
fn errors_on_the_new_style_are_refused() {
    let mut s = 库();
    let before = s.records["k"].clone();
    let e = s.extend_scope("k", &批(12, 12, 4), &选项()).unwrap_err();
    assert!(e.contains("扩展认证不过"), "{e}");
    assert_eq!(s.records["k"], before);
}

/// (e) CLI：扩展行缺 text → 拒收。
#[test]
fn cli_rows_without_text_are_refused() {
    let d = std::env::temp_dir().join(format!("jpp-b91-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    库().save(&d.join("c")).unwrap();
    std::fs::write(d.join("x.jsonl"), "{\"p\": 0.97, \"label\": true}\n").unwrap();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .args([
            "calib-import",
            "x.jsonl",
            "--calib",
            "c",
            "--extend-scope",
            "k",
            "--calib-out",
            "c2",
        ])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("text"));
    let _ = std::fs::remove_dir_all(&d);
}

/// (f) 带扩展的记录 save 再 load：步 20c 的重跑复现，扩展保留，仍放行。
#[test]
fn extension_survives_save_and_load() {
    let mut s = 库();
    s.extend_scope("k", &批(22, 22, 0), &选项()).unwrap();
    let d = std::env::temp_dir().join(format!("jpp-b91-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    s.save(&d).unwrap();
    let back = CalibStore::load(&d).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    assert!(back.load_report.is_empty(), "{:?}", back.load_report);
    assert_eq!(
        back.records["k"].scope.as_ref().unwrap().extensions.len(),
        1
    );
    assert_eq!(跑(&back).unwrap().value_json(), json!("已退"));
}
