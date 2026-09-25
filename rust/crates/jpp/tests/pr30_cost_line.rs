//! PR #30 补丁请求 2：`cut(…, {cost: …})` 取到的代价证书要看**它所在记录自己的状态**（B25、B29）。
//! 题式级记录「停岗」→ 与题级停岗同路由（drift）；记录非上岗却带证书 → 冷；「停岗候选」→ 照常路由但标候选。

mod common;

use jpp::effects::{CalibStore, Cert, EffectError, FnPort, JudgeResult, LabelSource, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 判断恒给 `p`、不该生成、问人恒答「还没答」（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
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

const 题式头: &str = r#"let f = form("test", "这段话是否提到了{city}？", {calib: "k"});"#;

const 程序: &str = r#"
budget {calls: 2, cost: 0, depth: 8};
let f = form("test", "这段话是否提到了{city}？", {calib: "k"});
let q = fill(f, {city: "上海"});
let e = cut(judge(state(mat("我去了上海")), q), {cost: [1, 5]});
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" }, unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }})
"#;

fn 跑(src: &str, calib: &CalibStore, p: f64) -> (Json, Vec<String>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let o = run(&program, 定值端口(p), calib, &ActionRegistry::new(), &mut l).expect("跑得完");
    (o.value_json(), o.trace.warnings.clone())
}

/// 题式键：跑一段只取 `f.hash` 的程序
fn 题式键() -> String {
    let (v, _) = 跑(
        &format!("budget {{calls: 0, cost: 0, depth: 8}};\n{题式头}\nf.hash"),
        &CalibStore::new(),
        0.5,
    );
    CalibStore::form_key(v.as_str().expect("hash 是文本"))
}

/// 在 `key` 上写一条认证记录，挂一张 cost(1,5)、线 0.7 的证书，再把状态改成 `status`
fn 带代价证书(c: &mut CalibStore, key: &str, status: &str) {
    common::certified(c, key, 0.8, 0.2, 50);
    let cert = Cert {
        alpha: 0.10,
        conf_delta: 0.10,
        hi: 0.7,
        n_accepted: 40,
        n_errors: 0,
        ucb: 0.06,
        cluster_unit: "测试合成证书".into(),
        resample: None,
        cost: Some((1.0, 5.0)),
        bounded_side: "单侧".into(),
        label_fp: String::new(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: LabelSource::全体,
    };
    let r = c.records.get_mut(key).unwrap();
    r.certs.insert(cert.addr(), cert);
    r.status = status.into();
}

#[test]
fn cost_line_form_level_suspended_is_drift() {
    let mut c = CalibStore::new();
    let fk = 题式键();
    带代价证书(&mut c, &fk, "上岗");
    let (v, w) = 跑(程序, &c, 0.9);
    assert_eq!(v, Json::from("act"), "对照：题式级上岗时代价线放行 {w:?}");
    c.records.get_mut(&fk).unwrap().status = "停岗".into();
    let (v, w) = 跑(程序, &c, 0.9);
    assert_eq!(v, Json::from("drift"), "题式级停岗与题级停岗同路由 {w:?}");
}

#[test]
fn cost_line_cold_record_with_cert_is_cold() {
    let mut c = CalibStore::new();
    带代价证书(&mut c, "k", "冷");
    let (v, w) = 跑(程序, &c, 0.9);
    assert_eq!(v, Json::from("cold"), "{w:?}");
    assert!(w.iter().any(|x| x.contains("cost_line")), "{w:?}");
}

#[test]
fn cost_line_form_level_candidate_routes_but_not_trusted() {
    let mut c = CalibStore::new();
    let fk = 题式键();
    带代价证书(&mut c, &fk, "停岗候选");
    let (v, w) = 跑(程序, &c, 0.9);
    assert_eq!(v, Json::from("act"), "候选线照常路由 {w:?}");
    assert!(
        w.iter().any(|x| x.starts_with("W-suspend-candidate")),
        "候选要标出来 {w:?}"
    );
}
