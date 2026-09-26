//! 绕过测试 B75（步 20f）：类记录的混合样本与类线的放行。
//! 来源 = 题式（`form_hash`），同一题式的不同填法是同一来源；不同来源 ≥ `class_min_sources`；
//! 每来源进线条数 ≥ 该档 `n_needed`；分半按来源分层（证书 `split-strata`）；记录写 `sources`；
//! 类线出口守卫不可逆 `do` → J-08 拒。
//! 依据：B75（`地基/附注/2026-09-24-评估①裁定.md` §五；`12` §2.2 B34 条、§2.3 混合样本条；`20` §3.4、§3.8）；`21` 步 20f。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断恒给 0.97，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
fn 桩端口() -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("m", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.97)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c: &[Json], _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

fn 选项(alpha_trial: Option<f64>) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b75".into(),
        seed: 20260923,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial,
        certify: jpp::truth::CertifyMethod::Split,
        step: None,
        sequential: None,
    }
}

/// 类行：题式 `模板`、填法 `填法`，`n` 条两极零错真值（正负各半），条目名加前缀避免撞。
fn 类行(模板: &str, 填法: &[&str], n: usize, 前缀: &str) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let yes = i % 2 == 0;
            let fill = 填法[i % 填法.len()];
            serde_json::from_value(json!({
                "class": "c", "form": {"op": "test", "template": 模板},
                "question": 模板.replace("{x}", fill),
                "item": format!("{前缀}{i}"), "p": if yes { 0.95 } else { 0.05 },
                "label": yes, "source": "computed"
            }))
            .unwrap()
        })
        .collect()
}

/// 一个题式、三个填法 → 一个来源 → 待真值，门控写来源不足（20b 的临时口径下这会上岗）。
#[test]
fn 同题式不同填法是同一来源() {
    let rows = 类行("这段话提到{x}吗", &["苹果", "地铁", "风筝"], 240, "a");
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项(Some(0.25))).unwrap();
    assert_eq!(
        rep[0].truth.gate,
        "待核：类记录来源不足（1 个来源，需 ≥ 2）"
    );
    assert_ne!(store.get(&CalibStore::class_key("c")).status, "上岗");
}

/// 两个题式各 120 条 → 上岗；证书 split-strata；记录 sources 两个来源各 120。
#[test]
fn 两来源分层上岗() {
    let mut rows = 类行("这段话提到{x}吗", &["苹果", "地铁"], 120, "a");
    rows.extend(类行("这段话的主题是{x}吗", &["饮食", "交通"], 120, "b"));
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项(Some(0.25))).unwrap();
    let ck = CalibStore::class_key("c");
    assert_eq!(store.get(&ck).status, "上岗", "{}", rep[0].truth.gate);
    let r = &store.records[&ck];
    assert_eq!(
        r.sources.values().copied().collect::<Vec<_>>(),
        vec![120, 120]
    );
    assert_eq!(
        r.选中的证书().unwrap().selection.as_ref().unwrap().method,
        "split-strata-stratified"
    );
}

/// 两个来源，其一只有 6 条：正式档（22）与试用档（9）都不足 → 待真值，门控点名该来源。
#[test]
fn 来源不足n条() {
    let mut rows = 类行("这段话提到{x}吗", &["苹果"], 200, "a");
    rows.extend(类行("这段话的主题是{x}吗", &["饮食"], 6, "b"));
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项(Some(0.25))).unwrap();
    let g = &rep[0].truth.gate;
    assert!(
        g.starts_with("待核：类记录来源 ") && g.contains("不足 22 条（有 6 条）"),
        "{g}"
    );
    assert!(
        rep[0]
            .trial
            .as_deref()
            .unwrap()
            .contains("不足 9 条（有 6 条）"),
        "{:?}",
        rep[0].trial
    );
    assert_ne!(store.get(&CalibStore::class_key("c")).status, "上岗");
}

/// 类线出口守卫不可逆 do → J-08 拒（B75：Class 不放行）；报告等级 Class、releases false。
#[test]
fn 类线不放行不可逆do() {
    let mut rows = 类行("这段话提到{x}吗", &["苹果", "地铁"], 120, "a");
    rows.extend(类行("这段话的主题是{x}吗", &["饮食", "交通"], 120, "b"));
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    import_labels(&mut store, &rows, &选项(None)).unwrap();
    let mut a = ActionRegistry::new();
    a.register("发", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let 放行 = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("一段话")), test("该发吗", "c"))), {
    act: fn() { content(do("发", [], 0)) },
    ignore: fn() { "没发" },
    unsure: fn(u) { consume(u, "drop"); "没发" }})
"#;
    let program = lower(&parse(放行).unwrap()).unwrap();
    let e = run(&program, 桩端口(), &store, &a, &mut Ledger::new()).expect_err("类线不放行");
    assert!(e.render().contains("J-08"), "{}", e.render());
    let 路由 = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("一段话")), test("该发吗", "c"))), {
    act: fn() { "act" }, ignore: fn() { "ignore" },
    unsure: fn(u) { consume(u, "drop"); "unsure" }})
"#;
    let program = lower(&parse(路由).unwrap()).unwrap();
    let o = run(&program, 桩端口(), &store, &a, &mut Ledger::new()).unwrap();
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.exits[0]["grade"], json!("Class"));
    assert_eq!(o.exits[0]["releases"], json!(false));
}

/// PR #31 Codex P2：只有模棱两可行的来源不进线，不计入来源数。来源甲 240 条已决、来源乙全是
/// `ambiguous` → 有效来源 1 个 → 待真值（修前按分组时的来源数算作 2 个，会上岗）。
#[test]
fn 全模棱两可的来源不计入来源数() {
    let mut rows = 类行("这段话提到{x}吗", &["苹果", "地铁"], 240, "a");
    let mut 乙 = 类行("这段话的主题是{x}吗", &["饮食"], 30, "b");
    for r in 乙.iter_mut() {
        r.label = json!("ambiguous");
    }
    rows.extend(乙);
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项(Some(0.25))).unwrap();
    assert_eq!(
        rep[0].truth.gate,
        "待核：类记录来源不足（1 个来源，需 ≥ 2）"
    );
    assert_ne!(store.get(&CalibStore::class_key("c")).status, "上岗");
}
