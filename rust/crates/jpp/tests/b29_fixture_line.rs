//! B29：J-03 同时约束程序与宿主。宿主 `put` 只写夹具记录；夹具线给出的出口带
//! `W-fixture-line`，不算放行不可逆 `do` 的可信合取项。`cut(…, {cost: [fp, fn]})`
//! 只取按该代价矩阵认证过的证书的线，没有就是冷。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, Cert, EffectError, FnPort, JudgeResult, LabelSource, Ports};
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
                confidence: vec![],
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

fn 跑(src: &str, calib: &CalibStore, p: f64) -> Result<(Json, Vec<String>), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(jpp::value::Value::text("已发"))
    });
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    run(&program, 定值端口(p, &calls), calib, &a, &mut l)
        .map(|o| (o.value_json(), o.trace.warnings.clone()))
        .map_err(|e| e.render())
}

#[test]
fn 夹具线不放行不可逆do() {
    let mut 夹具 = CalibStore::new();
    夹具.put("k", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
    assert!(夹具.records["k"].fixture, "put 写的是夹具记录");
    let e = 跑(发信, &夹具, 0.95).expect_err("夹具线的 Act 不能单独放行不可逆 do");
    assert!(e.contains("J-08") && e.contains("夹具线"), "{e}");

    let mut 认证 = CalibStore::new();
    common::certified(&mut 认证, "k", 0.8, 0.2, 50);
    let (v, w) = 跑(发信, &认证, 0.95).expect("认证线的 Act 放行");
    assert_eq!(v, Json::from("已发"));
    assert!(!w.iter().any(|x| x.starts_with("W-fixture-line")), "{w:?}");
}

#[test]
fn 夹具线出口照常路由并告警() {
    let mut 夹具 = CalibStore::new();
    夹具.put("k", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { consume(u, "drop"); "un" }})
"#;
    let (v, w) = 跑(src, &夹具, 0.95).expect("可逆路径照常");
    assert_eq!(v, Json::from("act"));
    assert!(w.iter().any(|x| x.starts_with("W-fixture-line")), "{w:?}");
}

#[test]
fn cut代价参数只取同代价的证书() {
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")), {cost: [10, 1]});
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }})
"#;
    // 没有这个代价矩阵的证书 → 冷（不借无代价证书的线）
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.5, 0.2, 50);
    let (v, w) = 跑(src, &c, 0.6).expect("跑得完");
    assert_eq!(v, Json::from("cold"), "{w:?}");
    assert!(w.iter().any(|x| x.contains("cost_line")), "{w:?}");
    // 有 cost(10,1) 的证书：线 0.7，读数 0.6 → 未过线
    let cert = Cert {
        alpha: 0.10,
        conf_delta: 0.10,
        hi: 0.7,
        n_accepted: 40,
        n_errors: 0,
        ucb: 0.06,
        cluster_unit: "测试合成证书".into(),
        resample: None,
        cost: Some((10.0, 1.0)),
        bounded_side: "单侧".into(),
        label_fp: String::new(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: LabelSource::全体,
    };
    c.records
        .get_mut("k")
        .unwrap()
        .certs
        .insert(cert.addr(), cert);
    let (v, _) = 跑(src, &c, 0.6).expect("跑得完");
    assert_ne!(v, Json::from("act"), "代价线 0.7 高于读数 0.6");
    let (v, _) = 跑(src, &c, 0.9).expect("跑得完");
    assert_eq!(v, Json::from("act"));
}
