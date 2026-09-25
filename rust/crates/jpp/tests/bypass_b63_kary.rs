//! B63：K 元划分（select / measure）的真值通道与单侧认证。
//! 依据：B63（地基/附注/2026-09-24-探针首轮裁定.md）；B24 上侧同形；B36 取序。

use jpp::effects::CalibStore;
use jpp::truth::{ImportOptions, LabelRow, import_labels};

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b63".into(),
        seed: 20260924,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        certify: jpp::truth::CertifyMethod::FixedSequence,
        step: None,
        sequential: None,
    }
}

/// n 条计算真值的 K 元行：高 p_max（≥ 0.9）的 argmax 全对，低 p_max（≤ 0.5）的全挑错。
fn 行(op: &str, n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let 高 = i % 3 != 0;
            let p = if 高 {
                0.9 + (i % 10) as f64 * 0.009
            } else {
                0.3 + (i % 5) as f64 * 0.04
            };
            let truth = i % 4;
            let pick = if 高 { truth } else { (truth + 1) % 4 };
            serde_json::from_value(serde_json::json!({
                "key": format!("k-{op}"), "op": op, "item": format!("m{i}"),
                "p": p, "pick": pick, "label": truth, "source": "computed"
            }))
            .unwrap()
        })
        .collect()
}

/// select 与 measure 都能经真值通道认证上岗；hi + δ 落在高 p_max 区间内，记录没有低侧线。
#[test]
fn k元划分经真值通道单侧认证上岗() {
    for op in ["select", "measure"] {
        let mut store = CalibStore::new();
        // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 之前的代码兜底值。
        store.profile.delta =
            jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        let rep = import_labels(&mut store, &行(op, 400), &选项()).unwrap();
        assert_eq!(rep[0].status, "上岗", "{op}: {}", rep[0].truth.gate);
        let r = store.get(&format!("k-{op}"));
        let o = if op == "select" {
            jpp::value::Op::Select
        } else {
            jpp::value::Op::Measure
        };
        let d = store.delta_for(&r, o).expect("导入写进了记录的 δ");
        assert!(
            r.hi + d > 0.5 && r.hi + d <= 0.9 + 1e-9,
            "{op}: hi {} δ {d}",
            r.hi
        );
        assert_eq!(r.lo, 0.0);
        assert!(r.lower.is_none());
    }
}

/// K 元行必须给 pick；label 只能是索引；同一键不许混题型。
#[test]
fn k元行格式错误拒收() {
    let mut store = CalibStore::new();
    let 缺pick: LabelRow = serde_json::from_value(serde_json::json!({"key": "k", "op": "select", "item": "a", "p": 0.9, "label": 1, "source": "computed"})).unwrap();
    assert!(
        import_labels(&mut store, &[缺pick], &选项())
            .unwrap_err()
            .contains("pick")
    );
    let 布尔标签: LabelRow = serde_json::from_value(serde_json::json!({"key": "k", "op": "measure", "item": "a", "p": 0.9, "pick": 0, "label": true, "source": "computed"})).unwrap();
    assert!(import_labels(&mut store, &[布尔标签], &选项()).is_err());
    let a: LabelRow = serde_json::from_value(
        serde_json::json!({"key": "k", "item": "a", "p": 0.9, "label": true, "source": "computed"}),
    )
    .unwrap();
    let b: LabelRow = serde_json::from_value(serde_json::json!({"key": "k", "op": "select", "item": "b", "p": 0.9, "pick": 0, "label": 0, "source": "computed"})).unwrap();
    assert!(
        import_labels(&mut store, &[a, b], &选项())
            .unwrap_err()
            .contains("混了")
    );
}

/// 是非题的真值通道不受影响：两侧线、有低侧证书。
#[test]
fn test行照旧两侧认证() {
    let rows: Vec<LabelRow> = (0..400)
        .map(|i| {
            let yes = i % 2 == 0;
            let p = if yes { 0.9 + (i % 10) as f64 * 0.009 } else { 0.02 + (i % 10) as f64 * 0.009 };
            serde_json::from_value(serde_json::json!({"key": "t", "item": format!("m{i}"), "p": p, "label": yes, "source": "computed"})).unwrap()
        })
        .collect();
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项()).unwrap();
    assert_eq!(rep[0].status, "上岗");
    assert!(store.get("t").lower.is_some());
}

/// 桥：K 元划分 p_max ≥ hi + δ 才出 Pick / At，否则 Unsure(band)；select 仍要求置换一致。
#[test]
fn 桥按hi加delta出pick与at() {
    use jpp_value::bridge::{CutInput, decide};
    use jpp_value::value::{Answer, ExitKind};
    let 判 = |a: Answer, ms: Option<f64>| {
        decide(&CutInput {
            fail: None,
            absent: None,
            suspended: false,
            line: Some((0.6, 0.0)),
            cost_requested: false,
            answer: Some(a),
            delta: Some(0.15),
            mode_share: ms,
        })
        .0
    };
    assert_eq!(
        判(Answer::Score(vec![0.1, 0.8, 0.1]), None),
        ExitKind::At(1)
    );
    assert_eq!(
        判(Answer::Score(vec![0.3, 0.7, 0.0]), None),
        ExitKind::Unsure("band".into())
    );
    assert_eq!(
        判(Answer::Choice(vec![0.8, 0.2]), Some(1.0)),
        ExitKind::Pick(0)
    );
    assert_eq!(
        判(Answer::Choice(vec![0.7, 0.3]), Some(1.0)),
        ExitKind::Unsure("band".into())
    );
    assert_eq!(
        判(Answer::Choice(vec![0.9, 0.1]), None),
        ExitKind::Unsure("untested".into())
    );
}
