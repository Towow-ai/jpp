//! B89：模型标注作真值的记录写有效 α（alpha_eff）与真值基准；等级按 alpha_eff 定；复核行不得带读数与出口。
//! 依据：`地基/附注/2026-09-24-标注门槛裁定.md` §五；`21` 步 20i。

use jpp::effects::{CalibStore, CertGrade};
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use jpp_effects::views::CalibView;
use serde_json::{Value, json};

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b89".into(),
        seed: 20260924,
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
    }
}

fn 行(v: Vec<Value>) -> Vec<LabelRow> {
    v.into_iter()
        .map(|x| serde_json::from_value(x).unwrap())
        .collect()
}

/// Sonnet 标注：`n` 条两极（偶数真 0.97、奇数假 0.02），条目名 `m{i}`
fn 标注(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            let t = i % 2 == 0;
            json!({"key": "k", "item": format!("m{i}"), "p": if t { 0.97 } else { 0.02 }, "label": t, "source": "model:claude-sonnet-5"})
        })
        .collect()
}

/// Fable 复核行（不带读数）：条目 `m{i}`，i ∈ 给定范围，判断与标注一致
fn 复核(items: impl Iterator<Item = usize>) -> Vec<Value> {
    items
        .map(|i| json!({"key": "k", "item": format!("m{i}"), "label": i % 2 == 0, "source": "model:fable-5.1", "spot_check": "fable-b89"}))
        .collect()
}

fn 导入(v: Vec<Value>) -> (CalibStore, jpp::truth::KeyReport) {
    let mut s = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    s.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let r = import_labels(&mut s, &行(v), &选项()).unwrap();
    (s, r.into_iter().next().unwrap())
}

/// (a) 裁定原例：复核 30 条全在已决区外（读数 0.5 附近的条目），本批 A 内没有复核行 → 按 (1 − a_lb)/c → 试用。
#[test]
fn 复核落在已决区外按c() {
    let mut v = 标注(200);
    for i in 0..30 {
        v.push(json!({"key": "k", "item": format!("z{i}"), "p": 0.5, "label": i % 2 == 0, "source": "model:claude-sonnet-5"}));
        v.push(json!({"key": "k", "item": format!("z{i}"), "label": i % 2 == 0, "source": "model:fable-5.1", "spot_check": "fable-b89"}));
    }
    let (s, rep) = 导入(v);
    let c = s.records["k"].选中的证书().unwrap();
    let e = c.eff.as_ref().expect("模型真值写 alpha_eff");
    assert_eq!(e.basis, "review-over-c");
    assert!(e.alpha_eff > 0.19 && e.alpha_eff < 0.23, "{}", e.alpha_eff);
    assert_eq!(c.grade, CertGrade::Trial);
    assert!(
        rep.truth.gate.contains("按 B89 降为试用"),
        "{}",
        rep.truth.gate
    );
    assert!(s.lookup("k").unwrap().selected.unwrap().trial);
}

/// (b) 复核全覆盖已决区：alpha_eff = α，等级正式，真值基准 model:fable-5.1。
#[test]
fn 复核全覆盖为正式() {
    let mut v = 标注(60);
    v.extend(复核(0..60));
    let (s, rep) = 导入(v);
    assert_eq!(rep.status, "上岗", "{}", rep.truth.gate);
    let c = s.records["k"].选中的证书().unwrap();
    let e = c.eff.as_ref().unwrap();
    assert_eq!(
        (e.basis.as_str(), e.alpha_eff, e.truth_baseline.as_str()),
        ("full-review", 0.1, "model:fable-5.1")
    );
    assert!((e.conf - 0.9).abs() < 1e-12);
    assert_eq!(c.grade, CertGrade::Formal);
    let v = s.lookup("k").unwrap().selected.unwrap();
    assert!(!v.trial && v.alpha_eff == 0.1);
}

/// (c) computed 真值：证书没有 eff，序列化逐字节同旧（不含 eff 键）。
#[test]
fn 构造真值不变() {
    let v: Vec<Value> = (0..60)
        .map(|i| json!({"key": "k", "item": format!("m{i}"), "p": if i % 2 == 0 { 0.97 } else { 0.02 }, "label": i % 2 == 0, "source": "computed"}))
        .collect();
    let (s, _) = 导入(v);
    let c = s.records["k"].选中的证书().unwrap();
    assert!(c.eff.is_none());
    assert!(!serde_json::to_string(c).unwrap().contains("\"eff\""));
}

/// (d) 复核行带读数或出口 → E-review-leak，整批拒收。
#[test]
fn 复核行带读数拒收() {
    let mut v = 标注(10);
    v.push(json!({"key": "k", "item": "m0", "p": 0.97, "label": true, "source": "model:fable-5.1", "spot_check": "r"}));
    let e = import_labels(&mut CalibStore::new(), &行(v), &选项()).unwrap_err();
    assert!(e.starts_with("E-review-leak"), "{e}");
    let mut v = 标注(10);
    v.push(json!({"key": "k", "item": "m0", "exit": "act", "label": true, "source": "model:fable-5.1", "spot_check": "r"}));
    let e = import_labels(&mut CalibStore::new(), &行(v), &选项()).unwrap_err();
    assert!(e.starts_with("E-review-leak") && e.contains("exit"), "{e}");
    // 复核行没有同题的标注行可回接读数
    let v = vec![
        json!({"key": "k", "item": "q", "label": true, "source": "model:fable-5.1", "spot_check": "r"}),
    ];
    let e = import_labels(&mut CalibStore::new(), &行(v), &选项()).unwrap_err();
    assert!(e.contains("没有同一道题的标注行可回接读数"), "{e}");
}

/// (e) 旧记录（证书无 eff、真值账有模型来源与抽检账）：视图按 α + (1 − a_lb)/c 补算，降为试用。
#[test]
fn 旧记录运行时降级() {
    let mut v = 标注(60);
    v.extend(复核(0..60));
    let (mut s, _) = 导入(v);
    let r = s.records.get_mut("k").unwrap();
    for c in r.certs.values_mut() {
        c.eff = None;
    }
    let t = r.truth.as_mut().unwrap();
    t.spot_check.as_mut().unwrap().lower = None;
    let sel = s.lookup("k").unwrap().selected.unwrap();
    assert!(sel.alpha_eff > 0.1 && sel.trial, "{}", sel.alpha_eff);
}

/// (f) 复核部分覆盖、落在已决区内（20 条全一致）：a_lb(A) = 0.861 → alpha_eff ≈ 0.239 → 试用。
#[test]
fn 区内部分复核() {
    let mut v = 标注(60);
    v.extend(复核(0..20));
    let (s, _) = 导入(v);
    let c = s.records["k"].选中的证书().unwrap();
    let e = c.eff.as_ref().unwrap();
    assert_eq!(e.basis, "review-in-A");
    assert!((e.alpha_eff - 0.2393).abs() < 0.002, "{}", e.alpha_eff);
    assert!((e.conf - 0.85).abs() < 1e-9, "{}", e.conf);
    assert_eq!(c.grade, CertGrade::Trial);
}

/// (g) B89 解读 (b)（步 20c）：alpha_eff 超过试用 α → 临时上岗（`Provisional`），不是试用；证书等级不改，
/// 门控写明，视图 `provisional` 为真、`trial` 为假；证书记下导入时的试用上限。
#[test]
fn alpha_eff_above_trial_alpha_is_provisional() {
    // 200 条两极 + 150 条中间读数（0.5，不进已决区）的 Sonnet 标注；复核 30 条全在中间：c = 200/350 ≈ 0.57，
    // a_lb ≈ 0.905 → alpha_eff ≈ 0.1 + 0.095 / 0.57 ≈ 0.27 > 0.25
    let mut v = 标注(200);
    for i in 0..150 {
        v.push(json!({"key": "k", "item": format!("z{i}"), "p": 0.5, "label": i % 2 == 0, "source": "model:claude-sonnet-5"}));
        if i < 30 {
            v.push(json!({"key": "k", "item": format!("z{i}"), "label": i % 2 == 0, "source": "model:fable-5.1", "spot_check": "fable-b89"}));
        }
    }
    let (s, rep) = 导入(v);
    let c = s.records["k"].选中的证书().unwrap();
    let e = c.eff.as_ref().unwrap();
    assert!(e.alpha_eff > 0.25, "{}", e.alpha_eff);
    assert_eq!(e.trial_alpha, Some(0.25));
    assert_eq!(c.grade, CertGrade::Formal, "等级不改为试用");
    assert!(
        rep.truth.gate.contains("为临时上岗（Provisional）"),
        "{}",
        rep.truth.gate
    );
    let view = s.lookup("k").unwrap().selected.unwrap();
    assert!(view.provisional && !view.trial);
}
