//! B25：停岗候选。漂移信号超线时系统自动把这条线标成停岗候选：出口照常路由，
//! 但不算放行不可逆 `do` 的可信合取项；正式停岗由人确认（CLI `calib-confirm`）。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, LiteralMode, Ports, Sample};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, TaintOut, run};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 判断恒给 `p`，问人恒「还没答」，不该生成（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口<'a>(p: f64, calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(None)))
}

const 发信: &str = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("内部材料")), test("可以发吗", "k")));
handle(e, {act: fn() { content(do("发邮件", [], 0)) },
           ignore: fn() { "没发" },
           unsure: fn(u) { consume(u, "drop"); "没发" }})
"#;

fn 跑(calib: &CalibStore) -> Result<(Json, Vec<String>, Vec<String>), String> {
    let program = lower(&parse(发信).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(jpp::value::Value::text("已发"))
    });
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    run(&program, 定值端口(0.95, &calls), calib, &a, &mut l)
        .map(|o| {
            (
                o.value_json(),
                o.trace.warnings.clone(),
                o.suspend_candidates.clone(),
            )
        })
        .map_err(|e| e.render())
}

fn 观察(c: &mut CalibStore, ps: &[f64], label: Option<u8>) {
    for p in ps {
        c.absorb(
            "k",
            Sample {
                p: Some(*p),
                label,
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
}

#[test]
fn 记录已是候选时不放行不可逆do() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    assert!(跑(&c).is_ok(), "上岗认证线照常放行");
    c.records.get_mut("k").unwrap().status = "停岗候选".into();
    let e = 跑(&c).expect_err("候选线的 Act 不能放行不可逆 do");
    assert!(e.contains("J-08") && e.contains("停岗候选"), "{e}");
}

#[test]
fn 漂移信号把线自动标成候选() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    let 标注: Vec<f64> = (0..40).map(|i| 0.10 + i as f64 * 0.02).collect();
    观察(&mut c, &标注, Some(1));
    let 近期: Vec<f64> = (0..40).map(|i| 0.50 + i as f64 * 0.012).collect();
    观察(&mut c, &近期, None);
    assert!(
        c.drift_of("k").expect("算得出").可停岗(),
        "构造的漂移足够强"
    );
    let e = 跑(&c).expect_err("漂移后同一趟里这条线已是候选");
    assert!(e.contains("J-08"), "{e}");
    assert_eq!(
        c.records["k"].status, "上岗",
        "内核不改记录，候选经 Outcome 交给宿主落盘"
    );
}

#[test]
fn 候选照常路由并交出候选键() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    let 标注: Vec<f64> = (0..40).map(|i| 0.10 + i as f64 * 0.02).collect();
    观察(&mut c, &标注, Some(1));
    let 近期: Vec<f64> = (0..40).map(|i| 0.50 + i as f64 * 0.012).collect();
    观察(&mut c, &近期, None);
    let program = lower(
        &parse(
            r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { consume(u, "drop"); "un" }})
"#,
        )
        .expect("解析"),
    )
    .expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = run(
        &program,
        定值端口(0.95, &calls),
        &c,
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("可逆路径照常");
    assert_eq!(o.value_json(), Json::from("act"));
    assert_eq!(o.suspend_candidates, vec!["k".to_string()]);
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-suspend-candidate")),
        "{:?}",
        o.trace.warnings
    );
}
