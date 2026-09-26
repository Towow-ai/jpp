//! PR #31 Codex P1（放行方向）：认证范围指纹只在认证真正发生时写，且与认证用同一批样本。
//! 起因：`calib-import` 原来不看门控与认证结果、只用本批行，一律覆盖 `scope`——被拦的一批会把在岗旧线
//! 的范围换成本批材料。依据：B68（`12` §2.3）；`过程记录/工程-步20g-1.md` §二。

use jpp::effects::CalibStore;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use serde_json::json;

/// 步 15d-2：import_labels 没有记录自带 δ 时取画像先验；用步 15d-2 前的代码兜底值（noul 0.05 /
/// choice、score 0.15）让这些测试保持原来的线。
fn 新库() -> CalibStore {
    let mut store = CalibStore::new();
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    store
}

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b1".into(),
        seed: 20260923,
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

/// `n` 条行（正负各半、两极），`src` 为来源，`文本` 给每行材料（`None` 不带）。
fn 行(n: usize, 起: usize, src: &str, 文本: Option<&str>) -> Vec<LabelRow> {
    (起..起 + n)
        .map(|i| {
            let yes = i % 2 == 0;
            let mut r = json!({"key": "k", "item": format!("m{i}"), "p": if yes { 0.95 } else { 0.05 },
                               "label": yes, "source": src});
            if let Some(t) = 文本 {
                r["text"] = json!(format!("{t}{}", "。".repeat(i % 3)));
            }
            serde_json::from_value(r).unwrap()
        })
        .collect()
}

const 甲: &str = "今天下午的会议推迟到三点，大家准时到";
const 乙: &str = "ERROR: build failed at src/main.rs:42 expected `;` found `}` error: aborting";

fn 指纹(store: &CalibStore) -> Option<jpp::conformal::ScopeRanges> {
    store.get("k").scope.and_then(|s| s.fingerprint)
}

/// (i) 被拦的一批（只有模型标注、无复核 → 待核）不改在岗线的范围；(ii) 认证不过的一批也不改。
#[test]
fn 没有认证就不改范围() {
    let mut store = 新库();
    import_labels(&mut store, &行(60, 0, "computed", Some(甲)), &选项()).unwrap();
    let 原 = store.get("k").scope.clone().expect("认证上岗 → 写范围");
    assert_eq!(原.fingerprint.as_ref().unwrap().n, 60);
    // (i) 被拦：模型标注没有复核
    let rep = import_labels(&mut store, &行(40, 1000, "model:sonnet", Some(乙)), &选项()).unwrap();
    assert!(
        rep[0].truth.gate.starts_with("待核"),
        "{}",
        rep[0].truth.gate
    );
    assert_eq!(
        store.get("k").scope.as_ref(),
        Some(&原),
        "被拦的一批不得改范围"
    );
    assert_eq!(store.get("k").status, "上岗");
    // (ii) 认证不过：高读数全判「否」
    let mut 坏 = 行(40, 2000, "computed", Some(乙));
    for r in 坏.iter_mut() {
        r.label = json!(r.p < 0.5);
    }
    let rep = import_labels(&mut store, &坏, &选项()).unwrap();
    assert!(
        !rep[0].truth.gate.starts_with("上岗"),
        "{}",
        rep[0].truth.gate
    );
    assert_eq!(
        store.get("k").scope.as_ref(),
        Some(&原),
        "认证不过的一批不得改范围"
    );
}

/// (iii) 连续两批都带文本、都可认证：第二次认证后指纹按合并后的全部样本算（n = 120），两批材料都在范围内。
#[test]
fn 追加导入按合并样本写范围() {
    let mut store = 新库();
    import_labels(&mut store, &行(60, 0, "computed", Some(甲)), &选项()).unwrap();
    let mut o = 选项();
    o.batch = "b2".into();
    import_labels(&mut store, &行(60, 100, "computed", Some(乙)), &o).unwrap();
    let f = 指纹(&store).expect("两批都带文本 → 写指纹");
    assert_eq!(f.n, 120);
    for t in [甲, 乙] {
        assert!(
            f.outside(&jpp::conformal::material_fingerprint(t))
                .is_none(),
            "{t}"
        );
    }
    assert_eq!(store.get("k").scope.unwrap().batches, vec!["b1", "b2"]);
}

/// (iv) 第二批不带文本：认证集部分带文本 → 用带文本的子集写指纹（B104-2，步 20h-1），记 n_text = 60/120，
/// 报 W-scope-partial。
#[test]
fn 认证集部分无文本写子集指纹() {
    let mut store = 新库();
    import_labels(&mut store, &行(60, 0, "computed", Some(甲)), &选项()).unwrap();
    let rep = import_labels(&mut store, &行(60, 100, "computed", None), &选项()).unwrap();
    assert_eq!(指纹(&store).expect("子集指纹").n, 60);
    assert_eq!(store.get("k").scope.unwrap().n_text, Some(60));
    assert!(
        rep[0]
            .warnings
            .iter()
            .any(|w| w.starts_with("W-scope-partial")),
        "{:?}",
        rep[0].warnings
    );
}

/// 固定读数 0.97 的判断端口（(v) 用）。步 15c：原 `impl Client` 的桩改为三个闭包端口。
fn 桩端口() -> jpp::effects::Ports<'static> {
    jpp::effects::Ports::new()
        .with(jpp::effects::FnPort::judge("m", |_s, qs| {
            Ok(jpp::effects::JudgeResult {
                answers: qs.iter().map(|_| jpp::value::Answer::Noul(0.97)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(jpp::effects::FnPort::generate("m", |_p, _c, _n, _r| {
            Err(jpp::effects::EffectError("x".into()))
        }))
        .with(jpp::effects::FnPort::ask("m", |_s, _q| {
            Err(jpp::effects::EffectError("x".into()))
        }))
}

/// (v) 认证集全部没有文本：范围未知（B104-2）。运行时出口照常路由、报 W-scope-unknown、`releases: false`，
/// 不可逆 do 被 J-08 拒。
#[test]
fn 全无文本范围未知不放行() {
    let mut store = 新库();
    import_labels(&mut store, &行(60, 0, "computed", None), &选项()).unwrap();
    assert_eq!(store.get("k").status, "上岗");
    assert!(指纹(&store).is_none());
    let 跑 = |src: &str| {
        let program = jpp::lower(&jpp::syntax::parse(src).unwrap()).unwrap();
        let mut a = jpp::interp::ActionRegistry::new();
        a.register("发", 0.0, false, jpp::interp::TaintOut::Trusted, |_| {
            Ok(jpp::value::Value::Text(
                "已发".into(),
                jpp::value::Taint::Trusted.into(),
            ))
        });
        jpp::run(
            &program,
            桩端口(),
            &store,
            &a,
            &mut jpp::ledger::Ledger::new(),
        )
    };
    let o = 跑(r#"budget {calls: 2, cost: 1, depth: 8};
handle(cut(judge(state(mat("今天下午的会议推迟到三点")), test("行吗", "k"))), {
    act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }})"#)
    .unwrap();
    assert_eq!(o.value_json(), json!("act"));
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-scope-unknown")),
        "{:?}",
        o.trace.warnings
    );
    assert_eq!(o.exits[0]["releases"], json!(false));
    let e = 跑(r#"budget {calls: 2, cost: 1, depth: 8};
handle(cut(judge(state(mat("今天下午的会议推迟到三点")), test("行吗", "k"))), {
    act: fn() { content(do("发", [], 0)) }, ignore: fn() { "不" }, unsure: fn(u) { consume(u, "drop"); "不" }})"#)
    .map(|_| ())
    .expect_err("范围未知不放行不可逆 do");
    assert!(e.render().contains("J-08"), "{}", e.render());
}

/// 带文本导入的记录（有 `material_fps`）存盘后能装回（步 20h-1 发现 20g-1 漏了装载器的字段表）。
#[test]
fn 带材料指纹的记录存盘可装回() {
    let mut store = 新库();
    import_labels(&mut store, &行(60, 0, "computed", Some(甲)), &选项()).unwrap();
    let d = std::env::temp_dir().join(format!("jpp-pr31-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    store.save(&d).unwrap();
    let back = CalibStore::load(&d).expect("装回");
    assert_eq!(back.get("k").material_fps.len(), 60);
    let _ = std::fs::remove_dir_all(&d);
}
