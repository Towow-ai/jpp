//! B85：拆分认证（`--certify split`）的分半改为分层交替。
//! 依据：B85（地基/附注/2026-09-24-标注门槛裁定.md §四）；B75 按来源分层。
//! 与研究者 `compare.py::split_halves(stratified=True)` 同式的逐位对照在 `jpp-calib` 的单测
//! `交替分半与研究者实现同式`（分半函数是私有的）。

use jpp::effects::{CalibStore, CertGrade, LiteralMode, Sample};
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};

fn 样本(p: f64, label: bool) -> Sample {
    Sample {
        p: Some(p),
        label: Some(u8::from(label)),
        perms: 0,
        mode_share: None,
        mode: LiteralMode::default(),
        phys: "noul".into(),
        cluster: None,
        stratum: None,
    }
}

/// 96 条：48 条正例读数 0.90–0.99、48 条负例 0.01–0.10（零错两极，读数互不相同）。
fn 池() -> Vec<(f64, bool)> {
    (0..96)
        .map(|i| {
            let pos = i % 2 == 0;
            let r = (i / 2) as f64 * 0.002;
            if pos {
                (0.9 + r, true)
            } else {
                (0.01 + r, false)
            }
        })
        .collect()
}

fn 认证(rows: &[(f64, bool)]) -> (CalibStore, jpp::effects::Cert) {
    let mut c = CalibStore::new();
    for (p, l) in rows {
        c.absorb("k", 样本(*p, *l)).unwrap();
    }
    // 步 15d-2：按 δ 平移的认证（拆分族）要求记录已有 δ，调用方先 set_delta；noul 题式用 0.05。
    c.set_delta("k", 0.05).unwrap();
    let cert = c
        .commission_two_sided_split_stratified_graded("k", 0.1, 0.1, 20260923, CertGrade::Formal)
        .expect("96 条分层交替应上岗");
    (c, cert)
}

/// (a) 池 96 正负各半：选线半、认证半各 48 条，认证半上下两侧已决各 24（四格各 24）。
#[test]
fn 四格各24() {
    let (c, cert) = 认证(&池());
    let sel = cert.selection.clone().unwrap();
    assert_eq!(sel.method, "split-stratified");
    assert_eq!(sel.rule.as_deref(), Some("split-stratified/v1"));
    assert_eq!((sel.n_select, sel.n_certify), (48, 48));
    // 两极零错：认证半的上侧已决 = 认证半的正例数，下侧 = 负例数
    assert_eq!(cert.n_accepted, 24);
    assert_eq!(c.get("k").lower.unwrap().n_accepted, 24);
}

/// (c) 行序置换后证书相同（分半只看规范序下的段内序号）。
#[test]
fn 行序无关() {
    let rows = 池();
    let mut 反序 = rows.clone();
    反序.reverse();
    let (a, ca) = 认证(&rows);
    let (b, cb) = 认证(&反序);
    assert_eq!(ca, cb);
    assert_eq!(
        (a.get("k").hi, a.get("k").lo),
        (b.get("k").hi, b.get("k").lo)
    );
}

/// (b) 带来源分层（B75 类行，两个题式各 48 条）：各来源各段各自交替，方法 `split-strata-stratified`；
/// 每来源 24 正 24 负 → 每来源在认证半各 12 正 12 负，合计认证半上下各 24。
#[test]
fn 按来源再分层() {
    let rows: Vec<LabelRow> = (0..96)
        .map(|i| {
            let pos = i % 2 == 0;
            let tmpl = if i < 48 {
                "这段话提到{x}吗"
            } else {
                "这段话的主题是{x}吗"
            };
            let r = (i / 2) as f64 * 0.002;
            serde_json::from_value(serde_json::json!({
                "class": "c", "form": {"op": "test", "template": tmpl},
                "item": format!("m{i}"), "p": if pos { 0.9 + r } else { 0.01 + r },
                "label": pos, "source": "computed"
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
        batch: "b85".into(),
        seed: 20260923,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        certify: CertifyMethod::Split,
        step: None,
        sequential: None,
    };
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &opt).unwrap();
    let ck = CalibStore::class_key("c");
    assert_eq!(store.get(&ck).status, "上岗", "{}", rep[0].truth.gate);
    let r = &store.records[&ck];
    let cert = r.选中的证书().unwrap();
    let sel = cert.selection.as_ref().unwrap();
    assert_eq!(sel.method, "split-strata-stratified");
    assert_eq!((sel.n_select, sel.n_certify), (48, 48));
    assert_eq!(cert.n_accepted, 24);
    assert_eq!(r.lower.as_ref().unwrap().n_accepted, 24);
}
