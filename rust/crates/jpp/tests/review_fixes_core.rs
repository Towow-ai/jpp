//! 公开仓库 Codex 评审（Towow-ai/jpp PR #27 / #28）指出的问题，各一条回归测试。
//! Regression tests for the Codex review comments on Towow-ai/jpp PR #27 / #28.
use jpp::effects::{CalibStore, LiteralMode, Sample};
use jpp::truth::{ImportOptions, LabelRow, import_labels};

fn 样本(p: f64, label: u8, phys: &str, mode_share: Option<f64>) -> Sample {
    Sample {
        p: Some(p),
        label: Some(label),
        perms: 0,
        mode_share,
        mode: LiteralMode::default(),
        phys: phys.into(),
        cluster: None,
        stratum: None,
    }
}

/// 60 条、一个题型，走 absorb + commission 上岗。
fn 上岗(c: &mut CalibStore, key: &str, phys: &str, mode_share: Option<f64>) {
    for i in 0..60 {
        let p = 0.02 + i as f64 * 0.016;
        c.absorb(key, 样本(p, u8::from(p > 0.55), phys, mode_share))
            .expect("折得进");
    }
    c.commission(key, 0.10, 0.10, "条").expect("认得动");
}

fn 带unsure预算(src: &str, unsure: f64) -> jpp::Program {
    let mut p = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
    p.budget.unsure = Some(unsure);
    p
}

// ---------------------------------------------------------------- PR #27：J-10 静态界按读数计

/// 同一道 let 绑定的题被两处 judge 用到，运行期是两条读数，静态界要算 2u。
/// 原先按构造站点计一次（0.3 ≤ 0.5，不报），运行期却是 0.6。
#[test]
fn j10_静态界按每处判断计_let绑定的题() {
    let mut c = CalibStore::new();
    上岗(&mut c, "ka", "noul", None);
    c.set_unsure_rate("ka", 0.3).unwrap();
    let src = r#"
budget {calls: 4, cost: 1};
let q = test("甲行吗", "ka");
let a = judge(state(mat("一")), q);
let b = judge(state(mat("二")), q);
1
"#;
    let r = jpp::check_with_calib(&带unsure预算(src, 0.5), &c);
    let d = r
        .find("J-10")
        .unwrap_or_else(|| panic!("两条读数 Σ = 0.6 > 0.5，该报：{}", r.render()));
    assert!(d.message.contains("0.6000"), "{}", d.message);
}

/// 一道题问向字面状态列表：列表多长就是多少条读数。
#[test]
fn j10_静态界按每处判断计_字面状态列表() {
    let mut c = CalibStore::new();
    上岗(&mut c, "ka", "noul", None);
    c.set_unsure_rate("ka", 0.3).unwrap();
    let src = r#"
budget {calls: 4, cost: 1};
let rs = judge([state(mat("一")), state(mat("二"))], test("甲行吗", "ka"));
1
"#;
    let r = jpp::check_with_calib(&带unsure预算(src, 0.5), &c);
    let d = r
        .find("J-10")
        .unwrap_or_else(|| panic!("两条读数 Σ = 0.6 > 0.5，该报：{}", r.render()));
    assert!(d.message.contains("0.6000"), "{}", d.message);
}

// ---------------------------------------------------------------- PR #27 / #28：J-10 告警带出 run

/// `run` 里那次带校准记录的检查报出的 J-10 只是告警，原先随报告一起丢掉；
/// 现在进 `trace.warnings` 最前面。
#[test]
fn j10_告警随run带出() {
    let src = "budget {calls: 1, cost: 1};\nlet q = test(\"甲\", \"ka\");\nlet r = test(\"乙\", \"kb\");\n1";
    let p = 带unsure预算(src, 0.5);
    let c = CalibStore::new();
    // 步 15c：原 `jpp::FixedClient::new()` 改为 `jpp::effects::FixedPorts::new()`；
    // 这个程序从不真的 `judge`，固定观察表留空即可。
    let mut fixed = jpp::effects::FixedPorts::new();
    let mut l = jpp::ledger::Ledger::new();
    let out = jpp::run(
        &p,
        fixed.ports(),
        &c,
        &jpp::interp::ActionRegistry::new(),
        &mut l,
    )
    .expect("跑得完");
    assert!(
        out.trace
            .warnings
            .first()
            .is_some_and(|w| w.starts_with("J-10")),
        "{:?}",
        out.trace.warnings
    );

    let mut l = jpp::ledger::Ledger::new();
    let out = jpp::run_with_fits(
        &p,
        fixed.ports(),
        &c,
        &jpp::interp::ActionRegistry::new(),
        &jpp::effects::FitRegistry::new(),
        &mut l,
    )
    .expect("跑得完");
    assert!(
        out.trace.warnings.iter().any(|w| w.starts_with("J-10")),
        "{:?}",
        out.trace.warnings
    );
}

// ---------------------------------------------------------------- PR #27：unsure 率按题型的出口判据

/// choice：置换没测时 `cut` 一律给 Unsure(untested)，实测 unsure 率应为 1。
/// 原先套 noul 的 `hi + δ` 判据，把高 p 的样本算成会放行。
#[test]
fn choice_置换未测时unsure率为一() {
    let mut c = CalibStore::new();
    上岗(&mut c, "kc", "choice", None);
    assert_eq!(c.get("kc").unsure_rate, Some(1.0));
    // B63 起 K 元划分的出口也带 δ 迟滞，率绑认证时的 δ；
    // 步 15d-2：certify 线（`commission`）不按 δ 平移，认证时的 δ 固定为 0（批量裁定解读 (a)），
    // 不再取画像的 select 缺省值 0.15。
    assert_eq!(
        c.get("kc").unsure_rate_delta,
        Some(0.0),
        "choice 的率绑 δ（B63）；步 15d-2：certify 线 δ = 0"
    );
}

/// choice（置换一致）与 score：`p >= hi + δ` 放行（B63 带 δ 迟滞），没有低侧出口。
#[test]
fn choice与score_按p不小于hi计() {
    for phys in ["choice", "score"] {
        let mut c = CalibStore::new();
        上岗(&mut c, "k", phys, Some(1.0));
        let r = c.get("k");
        let d = r.unsure_rate_delta.expect("B63：K 元划分的率绑 δ");
        let 期望 = (0..60)
            .filter(|i| 0.02 + *i as f64 * 0.016 < r.hi + d)
            .count() as f64
            / 60.0;
        assert!(
            (r.unsure_rate.unwrap() - 期望).abs() < 1e-4,
            "{phys}: {:?} vs {期望}",
            r.unsure_rate
        );
    }
}

// ---------------------------------------------------------------- PR #27：unsure 率绑定认证时的 δ

/// unsure 率绑定认证时的 δ，存盘装回后仍然成立。步 15d-2：certify 线的 δ 是 0（批量裁定解读 (a)），
/// `cut` 不读画像的 δ，所以换画像不再改变 `cut` 的 δ，率仍可用（原断言「换档案后不可用」按预注册改）。
#[test]
fn unsure率绑定认证时的delta() {
    let mut c = CalibStore::new();
    上岗(&mut c, "k", "noul", None);
    let r = c.get("k");
    assert!(r.unsure_rate.is_some());
    assert_eq!(r.unsure_rate_delta, Some(0.0));
    assert_eq!(c.usable_unsure_rate(&r), r.unsure_rate);

    let dir = std::env::temp_dir().join(format!("jpp-review-delta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    c.save(&dir).unwrap();
    let mut 装回 = CalibStore::load(&dir).unwrap();
    assert_eq!(
        装回.get("k").unsure_rate_delta,
        r.unsure_rate_delta,
        "δ 随记录落盘"
    );
    装回.profile.delta = jpp::effects::Field::known((0.09, 0.15, 0.15), "换了画像");
    assert_eq!(
        装回.usable_unsure_rate(&装回.get("k")),
        r.unsure_rate,
        "画像的 δ 不进 cut，换画像不影响率"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- PR #28：拆分与行序无关

/// 同一批 (p, 真值) 换个行序，拆分认证的结论、线与两半条数都不变。
#[test]
fn 拆分认证与行序无关() {
    let rows: Vec<(f64, u8)> = (0..240)
        .map(|i| {
            let p = ((i * 37) % 100) as f64 / 100.0;
            // 中间带里掺一些错标，让分半真的影响结论
            let label = if (i % 11 == 0) && (0.3..0.7).contains(&p) {
                u8::from(p <= 0.5)
            } else {
                u8::from(p > 0.5)
            };
            (p, label)
        })
        .collect();
    let 认证 = |order: &[(f64, u8)]| {
        let mut c = CalibStore::new();
        for (p, l) in order {
            c.absorb("k", 样本(*p, *l, "noul", None)).unwrap();
        }
        let r = c.commission_two_sided_split("k", 0.10, 0.10, 20260923);
        let rec = c.get("k");
        (format!("{r:?}"), rec.status, rec.hi, rec.lo)
    };
    let 正 = 认证(&rows);
    let mut 反序 = rows.clone();
    反序.reverse();
    assert_eq!(正, 认证(&反序));
    let mut 交错: Vec<(f64, u8)> = rows
        .iter()
        .step_by(2)
        .chain(rows.iter().skip(1).step_by(2))
        .cloned()
        .collect();
    交错.rotate_left(17);
    assert_eq!(正, 认证(&交错));
}

// ---------------------------------------------------------------- PR #28：真值通道的门看整条记录

fn 行(item: String, p: f64, label: bool, source: &str) -> LabelRow {
    serde_json::from_value(
        serde_json::json!({"key": "k", "item": item, "p": p, "label": label, "source": source}),
    )
    .unwrap()
}

fn 导入选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.10,
        conf_delta: 0.10,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b".into(),
        seed: 20260923,
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

/// 第一次只导入模型标注（没有抽检）→ 待核；存盘、装回，第二次只导入人工行。
/// 认证用的是记录里的全部样本（含那批模型标注），所以门必须仍然挡住。
#[test]
fn 旧的待核模型标注不因新一批人工行而上岗() {
    let mut c = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    c.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let 模型: Vec<LabelRow> = (0..200)
        .map(|i| {
            行(
                format!("m{i}"),
                if i % 2 == 0 { 0.97 } else { 0.02 },
                i % 2 == 0,
                "model:x",
            )
        })
        .collect();
    let r = import_labels(&mut c, &模型, &导入选项()).unwrap();
    assert!(r[0].truth.gate.starts_with("待核"), "{}", r[0].truth.gate);

    let dir = std::env::temp_dir().join(format!("jpp-review-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    c.save(&dir).unwrap();
    let mut c = CalibStore::load(&dir).unwrap();
    let 人工: Vec<LabelRow> = (0..200)
        .map(|i| {
            行(
                format!("h{i}"),
                if i % 2 == 0 { 0.96 } else { 0.03 },
                i % 2 == 0,
                "human",
            )
        })
        .collect();
    let r = import_labels(&mut c, &人工, &导入选项()).unwrap();
    assert!(
        r[0].truth.gate.starts_with("待核"),
        "旧模型标注没有抽检，不能借新人工行上岗：{}",
        r[0].truth.gate
    );
    assert_ne!(c.get("k").status, "上岗");
    assert_eq!(
        c.get("k").truth.unwrap().sources.get("model:x"),
        Some(&200),
        "合并后的来源账写回记录"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
