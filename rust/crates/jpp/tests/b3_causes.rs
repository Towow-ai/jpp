//! B3：未决原因 `rejected_all`（候选全被否决）与 `no_candidate`（没有候选）。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 判断恒给 `p`（select 题按均分），问人恒「还没答」，不该生成（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口<'a>(p: f64, calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| {
                        if q.op == jpp::value::Op::Select {
                            Answer::Choice(vec![1.0 / s.over.len() as f64; s.over.len()])
                        } else {
                            Answer::Noul(p)
                        }
                    })
                    .collect(),
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

fn 跑(src: &str, p: f64) -> (Json, u64) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = run(
        &program,
        定值端口(p, &calls),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    (o.value_json(), *calls.borrow())
}

const 找第一个: &str = r#"
budget {calls: 8, cost: 0, depth: 16};
fn 第一个(pool) {
    let r = first_k(sieve(pool, test("行吗", "k")), 1);
    let c = handle(r.value.exit, {act: fn() { "act" }, ignore: fn() { "ignore" },
                   unsure: fn(u) { let c = unsure_cause(u); consume(u, "drop"); c }});
    // 契约值不能整份 drop（B95，步 21）：逐项丢
    consume(map(r.pending, fn(p) { p.exit }), "drop");
    c
}
POOL
"#;

#[test]
fn 全被否决与没有候选分开() {
    let (v, _) = 跑(&找第一个.replace("POOL", r#"第一个(["甲", "乙"])"#), 0.05);
    assert_eq!(v, Json::from("rejected_all"));
    let (v, calls) = 跑(&找第一个.replace("POOL", "第一个([])"), 0.05);
    assert_eq!(v, Json::from("no_candidate"));
    assert_eq!(calls, 0);
    let (v, _) = 跑(&找第一个.replace("POOL", r#"第一个(["甲", "乙"])"#), 0.95);
    assert_eq!(v, Json::from("act"));
}

#[test]
fn 选择题没有候选不发并给no_candidate() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 8};
let e = cut(judge(state(mat("哪个好"), {over: []}), select("选一个", "k")));
let c = exit_kind(e);
consume(e, "drop");
c
"#;
    let (v, calls) = 跑(src, 0.5);
    assert!(v.as_str().unwrap().contains("no_candidate"), "{v}");
    assert_eq!(calls, 0, "没有候选不发");
}
