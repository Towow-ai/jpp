//! 绕过测试 步 20c（装载时按证书重跑认证）：`CalibStore::load` / `load_with` 对每条带证书的记录按证书方法重跑，
//! 决定出口的量（线、α、方法参数）不复现即降夹具；B104-1 旧证书复现后写回 δ；计数按重跑结果改写。
//! 依据：`20` v2 §2.3 J-03 文件面；B86、B87；B104-1（`地基/附注/2026-09-24-B104裁定.md` §一·3）；K-085；
//! 复现判据见 `地基/过程记录/工程-步20c.md`「复现判据」（主会话 2026-09-25 定）。

use std::path::PathBuf;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, SeqImport, import_labels};
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断恒给 0.97、不该生成、不该问人（步 15c 起用闭包端口）
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

fn 选项(certify: CertifyMethod, sequential: Option<SeqImport>) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "s20c".into(),
        seed: 20260925,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        certify,
        step: None,
        sequential,
    }
}

/// `n` 条两极构造真值（零错），正负各半，读数带一点梯度（候选不止一个），带运行材料同风格的文本。
fn 两极(n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let yes = i % 2 == 0;
            let p = if yes {
                0.9 + (i % 7) as f64 * 0.01
            } else {
                0.1 - (i % 7) as f64 * 0.01
            };
            serde_json::from_value(json!({
                "key": "k", "item": format!("m{i}"), "p": p, "label": yes,
                "source": "computed", "text": "顾客说要退款"
            }))
            .unwrap()
        })
        .collect()
}

fn 导入(certify: CertifyMethod, sq: Option<SeqImport>) -> CalibStore {
    导入n(certify, sq, 120)
}

fn 导入n(certify: CertifyMethod, sq: Option<SeqImport>, n: usize) -> CalibStore {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 之前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &两极(n), &选项(certify, sq)).unwrap();
    assert_eq!(store.get("k").status, "上岗", "{}", rep[0].truth.gate);
    store
}

/// 旧种子分半证书（B104-1 的「旧证书」：`selection` 在、`selection.delta` 缺）：在导入的样本上重新认证。
fn 旧证书库() -> CalibStore {
    旧证书库_带宽(0.05)
}

/// 同上，认证时的 δ 为 `d`（导入时画像的 δ；证书不记）
fn 旧证书库_带宽(d: f64) -> CalibStore {
    // 旧种子分半要选线半、认证半各侧都够 22 条：给 240 条
    let mut store = 导入n(CertifyMethod::FixedSequence, None, 240);
    // 步 15d-2：两侧拆分认证只读记录自带的 δ（`两侧前置`），画像的 δ 只在 import_labels 时当先验写进
    // 记录一次，之后改画像不会回头改记录；要让认证真的按 d 跑，得用 set_delta 直接改记录的 δ。
    store.set_delta("k", d).unwrap();
    let r = store.records.get_mut("k").unwrap();
    r.certs.clear();
    r.lower = None;
    r.status = "待真值".into();
    store
        .commission_legacy_seed_split_test_only(
            "k",
            0.1,
            0.1,
            20260923,
            true,
            jpp::effects::CertGrade::Formal,
        )
        .expect("旧种子分半认证");
    let c = store.records["k"].选中的证书().unwrap().clone();
    assert!(
        c.selection.as_ref().unwrap().delta.is_none(),
        "旧证书不记 δ"
    );
    store
}

fn 目录(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-s20c-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// 存盘、按 `改` 改写记录 JSON、再装载。
fn 改后装载(store: &CalibStore, tag: &str, 改: impl Fn(&mut Json)) -> CalibStore {
    改后装载2(store, tag, 改).0
}

/// 同上，另返回不重跑的装载（`load_raw`，比较「装载后记录不变」用）。
fn 改后装载2(
    store: &CalibStore,
    tag: &str,
    改: impl Fn(&mut Json),
) -> (CalibStore, CalibStore) {
    let d = 目录(tag);
    store.save(&d).unwrap();
    let f = d.join("k.json");
    let mut j: Json = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    改(&mut j);
    std::fs::write(&f, serde_json::to_string_pretty(&j).unwrap()).unwrap();
    let out = CalibStore::load(&d).unwrap();
    let raw = CalibStore::load_raw(&d).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    (out, raw)
}

fn 选中证书(j: &mut Json) -> &mut Json {
    let certs = j["certs"].as_object_mut().unwrap();
    assert_eq!(certs.len(), 1);
    certs.values_mut().next().unwrap()
}

const 放行: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("顾客说要退款")), test("该退吗", "k"))), {
    act: fn() { content(do("退款", [], 0)) },
    ignore: fn() { "没退" },
    unsure: fn(u) { consume(u, "drop"); "没退" }})
"#;

fn 跑(calib: &CalibStore) -> Result<jpp::Outcome, String> {
    let program = lower(&parse(放行).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    run(&program, 桩端口(), calib, &a, &mut Ledger::new()).map_err(|e| e.render())
}

/// (a) 伪造更宽的线（证书与记录的 hi 一起改低）→ 装载降夹具，报告带原因；出口 Fixture、J-08 拒。
#[test]
fn forged_line_is_demoted_to_fixture() {
    let store = 导入(CertifyMethod::FixedSequence, None);
    let s = 改后装载(&store, "forged", |j| {
        j["hi"] = json!(0.3);
        选中证书(j)["hi"] = json!(0.3);
    });
    assert!(s.records["k"].fixture, "伪造的线降为夹具");
    assert!(
        s.load_report
            .iter()
            .any(|l| l.starts_with("W-calib-rerun") && l.contains("不复现")),
        "{:?}",
        s.load_report
    );
    let e = 跑(&s).expect_err("夹具线不放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

/// (b) 固定序导入的记录落盘再装载：完整复现，记录逐字节不变，放行。
#[test]
fn fixed_sequence_record_reproduces_unchanged() {
    let store = 导入(CertifyMethod::FixedSequence, None);
    let (s, raw) = 改后装载2(&store, "fixed", |_| {});
    assert!(s.load_report.is_empty(), "{:?}", s.load_report);
    assert_eq!(s.records["k"], raw.records["k"]);
    let o = 跑(&s).expect("正式线放行");
    assert_eq!(o.value_json(), json!("已退"));
    assert_eq!(o.exits[0]["grade"], json!("Certified"));
}

/// (c) 旧证书缺 δ：装载用当前 δ 重跑，线复现即写回 δ，`delta_unknown` 消失；再存再装载不再改写（幂等）。
#[test]
fn old_certificate_gets_delta_written_back_idempotently() {
    let old = 旧证书库();
    let s = 改后装载(&old, "old", |_| {});
    assert!(!s.records["k"].fixture);
    let c = s.records["k"].选中的证书().unwrap();
    assert_eq!(
        c.selection.as_ref().unwrap().delta,
        Some(0.05),
        "兜底 δ 写回"
    );
    assert_eq!(
        s.records["k"]
            .lower
            .as_ref()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .delta,
        Some(0.05)
    );
    // B117 (b)：旧证书（上侧与下侧各一张）原样移入 certs_history
    assert_eq!(s.records["k"].certs_history.len(), 2);
    assert!(
        s.records["k"]
            .certs_history
            .iter()
            .all(|c| c.selection.as_ref().unwrap().delta.is_none())
    );
    assert!(
        s.load_report
            .iter()
            .any(|l| l.contains("写回解出的 δ=0.05")),
        "{:?}",
        s.load_report
    );
    let o = 跑(&s).expect("写回 δ 后放行");
    assert!(o.exits[0].get("delta_unknown").is_none(), "{:?}", o.exits);
    assert_eq!(o.value_json(), json!("已退"));
    // 未经 load 的同一记录（直接给运行时）仍是 δ 未知：不放行（B104-1）
    let e = 跑(&old).expect_err("δ 未知不放行");
    assert!(e.contains("J-08"), "{e}");
    // 幂等
    let (s2, raw2) = 改后装载2(&s, "old2", |_| {});
    assert!(s2.load_report.is_empty(), "{:?}", s2.load_report);
    assert_eq!(s2.records["k"], raw2.records["k"]);
}

/// (d) B117 (c)：旧证书的 δ 由样本与线解出，不取画像——认证时 δ = 0.04（画像值，证书没记），装载时不给画像，
/// 仍解出 0.04 并写回；线被改到任何网格 δ 都复现不了时降夹具。
#[test]
fn old_certificate_delta_is_solved_from_samples() {
    let s = 改后装载(&旧证书库_带宽(0.04), "d004", |_| {});
    assert!(!s.records["k"].fixture, "{:?}", s.load_report);
    let c = s.records["k"].选中的证书().unwrap();
    assert_eq!(c.selection.as_ref().unwrap().delta, Some(0.04));
    // 线被改：hi、lo 与两侧证书的线都挪到网格外，没有 δ 能复现
    let s = 改后装载(&旧证书库(), "nodelta", |j| {
        j["hi"] = json!(0.3333);
        j["lo"] = json!(0.2222);
        选中证书(j)["hi"] = json!(0.3333);
        j["lower"]["hi"] = json!(0.2222);
    });
    assert!(s.records["k"].fixture);
    assert!(s.load_report[0].contains("解不出"), "{:?}", s.load_report);
}

/// (e) 同批两侧选线的证书（无 `selection`，规则以「两侧联合选线：」开头）：没有重跑过程，降夹具。
#[test]
fn same_batch_two_sided_has_no_rerun_and_is_demoted() {
    let s = 改后装载(&旧证书库(), "samebatch", |j| {
        let c = 选中证书(j);
        c.as_object_mut().unwrap().remove("selection");
        c["resample"] = json!([
            0,
            "两侧联合选线：两区各自二项上界 ≤ α，取已决条数最多者（真值通道）"
        ]);
    });
    assert!(s.records["k"].fixture);
    assert!(
        s.load_report[0].contains("没有重跑过程"),
        "{:?}",
        s.load_report
    );
}

/// (f) 序贯导入（B87）的记录落盘再装载：按证书里的到达顺序与停时重跑，复现。
#[test]
fn sequential_record_reproduces() {
    let sq = SeqImport {
        batch: 10,
        weights: [0.8, 0.1, 0.05, 0.05],
        coverage_target: None,
        two_ends: false,
        frame: None,
    };
    let store = 导入(CertifyMethod::Sequential, Some(sq));
    assert_eq!(
        store.records["k"]
            .选中的证书()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .method,
        "sequential"
    );
    let s = 改后装载(&store, "seq", |_| {});
    assert!(s.load_report.is_empty(), "{:?}", s.load_report);
    assert!(!s.records["k"].fixture);
}

/// (g) 标注集对不上（删掉一条带标注样本）：降夹具。
#[test]
fn label_set_mismatch_is_demoted() {
    let store = 导入(CertifyMethod::FixedSequence, None);
    let s = 改后装载(&store, "labelset", |j| {
        j["samples"].as_array_mut().unwrap().pop();
    });
    assert!(s.records["k"].fixture);
    assert!(
        s.load_report[0].contains("标注集对不上"),
        "{:?}",
        s.load_report
    );
}

/// (h) 只改计数（`n_accepted` 改大）、线不变：复现（计数不参与判据），计数改写回重跑的值。
#[test]
fn inflated_counts_are_rewritten_not_demoted() {
    let store = 导入(CertifyMethod::FixedSequence, None);
    let n0 = store.records["k"].选中的证书().unwrap().n_accepted;
    let s = 改后装载(&store, "counts", |j| {
        let c = 选中证书(j);
        c["n_accepted"] = json!(c["n_accepted"].as_u64().unwrap() + 50);
    });
    assert!(!s.records["k"].fixture);
    assert_eq!(s.records["k"].选中的证书().unwrap().n_accepted, n0);
    assert!(
        s.load_report[0].contains("改写为重跑结果"),
        "{:?}",
        s.load_report
    );
}

/// (i) 带证书的记录手填 `unsure_rate`：被重跑测出的值覆盖（K-085）。
#[test]
fn hand_filled_unsure_rate_is_replaced_by_measured() {
    let store = 导入(CertifyMethod::FixedSequence, None);
    let u0 = store.records["k"].unsure_rate;
    let s = 改后装载(&store, "rate", |j| j["unsure_rate"] = json!(0.5));
    assert_eq!(s.records["k"].unsure_rate, u0);
    assert!(
        s.load_report[0].contains("unsure_rate"),
        "{:?}",
        s.load_report
    );
}
