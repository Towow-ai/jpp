//! 绕过测试 步 20a-1（`LineGrade` 与唯一放行判定点）：
//! (a) 临时上岗（B19 修订）的线出口照常路由，等级 `Provisional`、`releases: false`，守卫不可逆 `do` → J-08 拒，
//!     报文点名临时上岗；同一记录去掉临时门控 → `Certified`、放行（步 20f 登记的缺陷，本步按等级表收紧）；
//! (c) 校准记录 `kind` 分类字段（B76）：导入写基础题类，不进 `calib_hash`，`save` / `load` 往返。
//! `Exit::releases` 的等级一项与正交位（含 `untested`）的单元测试在 `jpp-value/src/bridge.rs::line_grade_tests`。
//! 依据：`附注/2026-09-24-评估①裁定.md` §五 各等级一致表、§十第 12(a) 条；B76；`21` 步 20a-1。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports, calib_hash};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::json;

/// 判断恒给 0.97、不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
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
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

fn 选项() -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "s20a1".into(),
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
    }
}

/// `n` 条两极构造真值（零错），正负各半，带与运行材料同风格的文本（有范围指纹）。
fn 两极(key: &str, n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let yes = i % 2 == 0;
            serde_json::from_value(json!({
                "key": key, "item": format!("m{i}"), "p": if yes { 0.95 } else { 0.05 },
                "label": yes, "source": "computed", "text": "顾客说要退款"
            }))
            .unwrap()
        })
        .collect()
}

/// 正式认证的题级线；`临时` 为真时把门控改成临时上岗（B19 修订：点估计过、下界不过）。
fn 库(临时: bool) -> CalibStore {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 之前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &两极("k", 120), &选项()).unwrap();
    assert_eq!(store.get("k").status, "上岗", "{}", rep[0].truth.gate);
    if 临时 {
        let rec = store.records.get_mut("k").unwrap();
        rec.truth.as_mut().unwrap().gate =
            "临时上岗：人工抽检一致率 28/30，单侧 95% 置信下界 0.80 < 门槛 0.90".into();
    }
    store
}

const 路由: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("顾客说要退款")), test("该退吗", "k"))), {
    act: fn() { "act" },
    ignore: fn() { "ignore" },
    unsure: fn(u) { consume(u, "drop"); "unsure" }})
"#;

const 放行: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("顾客说要退款")), test("该退吗", "k"))), {
    act: fn() { content(do("退款", [], 0)) },
    ignore: fn() { "没退" },
    unsure: fn(u) { consume(u, "drop"); "没退" }})
"#;

fn 跑(src: &str, calib: &CalibStore) -> Result<jpp::Outcome, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    run(&program, 桩端口(), calib, &a, &mut Ledger::new()).map_err(|e| e.render())
}

/// (a) 临时上岗：路由照常，等级 Provisional、不放行；J-08 拒并点名临时上岗。
#[test]
fn provisional_routes_but_never_releases() {
    let o = 跑(路由, &库(true)).unwrap();
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.exits.len(), 1);
    assert_eq!(o.exits[0]["grade"], json!("Provisional"));
    assert_eq!(o.exits[0]["releases"], json!(false));
    let e = 跑(放行, &库(true)).expect_err("临时上岗线不该放行不可逆 do");
    assert!(e.contains("J-08") && e.contains("临时上岗"), "{e}");
}

/// (a) 对照：同一记录去掉临时门控 → Certified、放行。
#[test]
fn same_record_without_provisional_gate_releases() {
    let o = 跑(放行, &库(false)).expect("正式线放行");
    assert_eq!(o.value_json(), json!("已退"));
    assert_eq!(o.exits[0]["grade"], json!("Certified"));
    assert_eq!(o.exits[0]["releases"], json!(true));
}

fn 题式行(op: &str, n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let yes = i % 2 == 0;
            let (label, pick) = if op == "test" {
                (json!(yes), None)
            } else {
                (json!(if yes { 0 } else { 1 }), Some(if yes { 0 } else { 1 }))
            };
            let mut r = json!({
                "form": {"op": op, "template": "这段话属于哪一类？", "scale": if op == "test" { json!([]) } else { json!(["甲", "乙"]) }},
                "item": format!("m{i}"), "p": 0.95, "label": label, "source": "computed"
            });
            if let Some(k) = pick {
                r["pick"] = json!(k);
            }
            serde_json::from_value(r).unwrap()
        })
        .collect()
}

/// (c) 校准记录 `kind`：test 题式 → attr，select 题式 → class（缺省 over_kind = labels）；
/// 不进 `calib_hash`；`save` / `load` 往返；未导入的记录不带这个字段。
#[test]
fn record_kind_written_by_import_and_not_hashed() {
    for (op, want) in [("test", "attr"), ("select", "class")] {
        let mut store = CalibStore::new();
        store.profile.delta =
            jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        import_labels(&mut store, &题式行(op, 60), &选项()).unwrap();
        assert_eq!(store.records.len(), 1);
        let key = store.records.keys().next().unwrap().clone();
        let j = serde_json::to_value(&store.records[&key]).unwrap();
        assert_eq!(j["kind"], json!(want), "{op}");
        let h = calib_hash(&store);
        store.records.get_mut(&key).unwrap().kind = None;
        assert_eq!(calib_hash(&store), h, "kind 不进哈希（B76）");
        assert!(
            serde_json::to_value(&store.records[&key])
                .unwrap()
                .get("kind")
                .is_none(),
            "为空不序列化"
        );
    }
    let mut store = CalibStore::new();
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    import_labels(&mut store, &题式行("test", 60), &选项()).unwrap();
    let dir = std::env::temp_dir().join(format!("jpp-s20a1-kind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    store.save(&dir).unwrap();
    let back = CalibStore::load(&dir).expect("带 kind 的记录读得回来");
    assert_eq!(back.records, store.records);
    let _ = std::fs::remove_dir_all(&dir);
}
