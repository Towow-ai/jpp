//! B86：固定序检验、不拆分（`calib-import` 的缺省认证方式）。
//! 依据：B86（地基/附注/2026-09-24-标注门槛裁定.md §一）；B63 同形；B72 两档。

use jpp::effects::{
    CalibStore, CertGrade, FIXED_SEQUENCE_RULE, LiteralMode, Refusal, Sample,
    fixed_sequence_candidates, fixed_sequence_step,
};
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};

fn 样本(p: f64, label: bool, phys: &str) -> Sample {
    Sample {
        p: Some(p),
        label: Some(u8::from(label)),
        perms: 0,
        mode_share: None,
        mode: LiteralMode::default(),
        phys: phys.into(),
        cluster: None,
        stratum: None,
    }
}

fn 库(rows: &[(f64, bool)]) -> CalibStore {
    let mut c = CalibStore::new();
    for (p, l) in rows {
        c.absorb("k", 样本(*p, *l, "noul")).unwrap();
    }
    // 步 15d-2：按 δ 平移的认证（固定序族）要求记录已有 δ，调用方先 set_delta；这里都是 noul 题式，用 0.05。
    c.set_delta("k", 0.05).unwrap();
    c
}

/// `pos` 条读数 0.95 的正例与 `neg` 条读数 0.05 的负例（零错两极）。
fn 两极(pos: usize, neg: usize) -> Vec<(f64, bool)> {
    let mut v = vec![(0.95, true); pos];
    v.extend(vec![(0.05, false); neg]);
    v
}

fn 选项(certify: CertifyMethod) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b86".into(),
        seed: 20260923,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: Some(0.25),
        certify,
        step: None,
        sequential: None,
    }
}

fn 行(rows: &[(f64, bool)]) -> Vec<LabelRow> {
    rows.iter()
        .enumerate()
        .map(|(i, (p, l))| {
            serde_json::from_value(serde_json::json!({
                "key": "k", "item": format!("m{i}"), "p": p, "label": l, "source": "computed"
            }))
            .unwrap()
        })
        .collect()
}

/// (a) 候选只由读数生成：同一批读数换标签、换行序，候选序列逐位相同；证书的生成数与步长也相同。
#[test]
fn 候选序列只读读数() {
    let ps: Vec<f64> = (0..80).map(|i| ((i * 37) % 97) as f64 / 97.0).collect();
    let mut 反序 = ps.clone();
    反序.reverse();
    let s = fixed_sequence_step(ps.len());
    assert_eq!(s, 4);
    let a = fixed_sequence_candidates(&ps, 0.1, 0.1, 0.05, s);
    assert_eq!(a, fixed_sequence_candidates(&反序, 0.1, 0.1, 0.05, s));
    assert!(!a.0.is_empty() && !a.1.is_empty());
    // 上侧从严到宽（阈值降），下侧从严到宽（阈值升）
    assert!(a.0.windows(2).all(|w| w[0] > w[1]));
    assert!(a.1.windows(2).all(|w| w[0] < w[1]));
    // 同读数、两套标签：证书的生成数、步长、δ 相同（标签只决定停在哪）
    let 甲: Vec<(f64, bool)> = ps.iter().map(|p| (*p, *p > 0.5)).collect();
    let 乙: Vec<(f64, bool)> = ps.iter().map(|p| (*p, *p > 0.3)).collect();
    let sel = |rows: &[(f64, bool)]| {
        let mut c = 库(rows);
        let _ =
            c.commission_two_sided_fixed_sequence_graded("k", 0.25, 0.1, None, CertGrade::Formal);
        c
    };
    let (ca, cb) = (sel(&甲), sel(&乙));
    let g = |c: &CalibStore| {
        c.records["k"]
            .选中的证书()
            .and_then(|x| x.selection.clone())
            .map(|s| (s.generated, s.step, s.delta))
    };
    let (x, y) = (g(&ca).expect("甲应上岗"), g(&cb).expect("乙应上岗"));
    assert_eq!(x, y);
}

/// (b) 22 + 22 零错两极 → 正式上岗；证书写方法、规则版本、步长、δ、生成数与停点。
#[test]
fn 零错22加22正式上岗() {
    let mut c = 库(&两极(22, 22));
    let cert = c
        .commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal)
        .expect("22 + 22 零错应上岗");
    let sel = cert.selection.clone().unwrap();
    assert_eq!(sel.method, "fixed-sequence");
    assert_eq!(sel.rule.as_deref(), Some(FIXED_SEQUENCE_RULE));
    assert_eq!(sel.step, Some(2));
    assert_eq!(sel.delta, Some(0.05));
    assert_eq!(sel.n_certify, 44);
    assert_eq!(sel.generated, Some((3, 3)));
    assert_eq!(sel.stop_index, Some((2, 2)));
    let rec = c.get("k");
    assert_eq!(rec.status, "上岗");
    assert!(
        (rec.hi - 0.90).abs() < 1e-9 && (rec.lo - 0.10).abs() < 1e-9,
        "{} {}",
        rec.hi,
        rec.lo
    );
    assert_eq!((cert.n_accepted, cert.n_errors), (22, 0));
    assert_eq!(rec.lower.unwrap().n_accepted, 22);
    // 证书带规则版本 → 地址多一段 rule(…)，与旧拆分证书不同址
    assert!(cert.addr().contains("rule("));
}

/// (c) 21 + 22：正例不到 22 条 → 正式停在待核（样本不足），不降低门槛。
#[test]
fn 正例21条正式待核() {
    let mut c = 库(&两极(21, 22));
    match c.commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal) {
        Err(Refusal::跑不成(w)) => assert!(w.starts_with("待核：样本不足"), "{w}"),
        other => panic!("应待核：{other:?}"),
    }
    assert_ne!(c.get("k").status, "上岗");
}

/// (d) 9 + 9 经真值通道（缺省固定序、带试用 α）：正式待核 → 试用上岗，证书 trial、方法 fixed-sequence。
#[test]
fn 九加九试用上岗() {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(
        &mut store,
        &行(&两极(9, 9)),
        &选项(CertifyMethod::FixedSequence),
    )
    .unwrap();
    assert!(
        rep[0]
            .truth
            .gate
            .starts_with("试用上岗（α=0.25）：正式 α=0.1 未过：待核"),
        "{}",
        rep[0].truth.gate
    );
    let c = store.records["k"].选中的证书().unwrap();
    assert_eq!((c.grade, c.alpha), (CertGrade::Trial, 0.25));
    assert_eq!(c.selection.as_ref().unwrap().method, "fixed-sequence");
}

/// (e) 第一格不过即停：读数最高的 22 条里有 1 条错（上界约 0.17 > α），
/// 更宽的候选（读数 ≥ 0.95 的 60 条里 1 错，上界约 0.064）若被检验会通过——固定序不去检验它。
#[test]
fn 第一格不过即停() {
    let mut rows = vec![(0.99, true); 21];
    rows.push((0.99, false));
    rows.extend(vec![(0.95, true); 38]);
    rows.extend(vec![(0.05, false); 30]);
    let mut c = 库(&rows);
    match c.commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal) {
        Err(Refusal::认证不过(jpp::conformal::Certificate::Refused {
            best_n_accepted,
            best_hi,
            best_ucb,
            ..
        })) => {
            assert_eq!(best_n_accepted, 22);
            assert_eq!(best_hi, 0.99);
            assert!(best_ucb > 0.1);
        }
        other => panic!("应认证不过：{other:?}"),
    }
    // 对照：那个更宽的候选单独看是过的（同批不校正会选它）
    assert!(jpp::conformal::binomial_upper(1, 60, 0.1) <= 0.1);
    assert_ne!(c.get("k").status, "上岗");
}

/// (f) T1 型 60 条两极（30 + 30）：`--certify split` 待核，`fixed-sequence` 正式上岗。
#[test]
fn 六十条拆分待核固定序上岗() {
    let rows = 两极(30, 30);
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let mut opt = 选项(CertifyMethod::Split);
    opt.alpha_trial = None;
    let rep = import_labels(&mut store, &行(&rows), &opt).unwrap();
    assert!(
        rep[0].truth.gate.starts_with("待核：选线半样本不足"),
        "{}",
        rep[0].truth.gate
    );
    let mut store = CalibStore::new();
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &行(&rows), &选项(CertifyMethod::FixedSequence)).unwrap();
    assert_eq!(rep[0].truth.gate, "上岗");
    let c = store.records["k"].选中的证书().unwrap();
    assert_eq!(
        (c.grade, c.selection.as_ref().unwrap().method.as_str()),
        (CertGrade::Formal, "fixed-sequence")
    );
}

/// (g) 旧的种子拆分证书不带新字段：序列化不含 rule / step / delta / generated / stop_index，地址不含 rule 段。
#[test]
fn 旧拆分证书逐字节不变() {
    let mut c = 库(&两极(100, 100));
    let cert = c
        .commission_legacy_seed_split_test_only(
            "k",
            0.1,
            0.1,
            7,
            true,
            jpp::effects::CertGrade::Formal,
        )
        .unwrap();
    let j = serde_json::to_value(&cert).unwrap();
    let sel = j["selection"].as_object().unwrap();
    let keys: Vec<&str> = sel.keys().map(|k| k.as_str()).collect();
    assert_eq!(
        keys,
        ["candidates", "method", "n_certify", "n_select", "seed"]
    );
    assert!(!cert.addr().contains("rule("));
}

/// (h) K 元同形：select 行固定序上岗，hi = h − δ（choice 的 δ），没有下侧证书。
#[test]
fn k元固定序上岗() {
    let mut c = CalibStore::new();
    for _ in 0..30 {
        c.absorb("k", 样本(0.9, true, "choice")).unwrap();
    }
    // 步 15d-2：按 δ 平移的认证要求记录已有 δ，调用方先 set_delta；choice 题式用 0.15。
    c.set_delta("k", 0.15).unwrap();
    let cert = c
        .commission_upper_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal)
        .expect("30 条全对应上岗");
    let rec = c.get("k");
    assert_eq!(rec.status, "上岗");
    assert!(rec.lower.is_none());
    assert_eq!(rec.lo, 0.0);
    let d = c
        .delta_for(&rec, jpp::value::Op::Select)
        .expect("导入写进了记录的 δ");
    assert!((rec.hi + d - 0.9).abs() < 1e-9);
    assert_eq!(cert.selection.unwrap().method, "fixed-sequence");
}
