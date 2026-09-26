//! B93 旁路测试（步 22-0）：预算停机在刷新点降级，不再让程序停在 Pending。
//!
//! `judge` 直接调用、`select` 超预算 → 出口 `Unsure(budget)`、程序照常返回；续跑（非审计）带更高预算
//! 重发这些站点；审计重放在同一站点同样停发、0 调用；`gen`/`do` 超预算产出失败值，动作不执行（不可逆 `do` 先过 J-08 放行，再核预算，同样不执行）。
//!
//! 依据：B93（`评估/2026-09-24-仪表读数4诊断与裁定.md` §三·1、§八）；`12` §3 J-07「停」指停发；
//! `21` 步 22-0；预注册 `地基/过程记录/工程-步22-0.md`。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jpp::effects::{CalibStore, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Op, Value};
use jpp::{ActionRegistry, Outcome, TaintOut, run, run_replay};
use jpp::{lower, syntax::parse};

/// 是非题恒 0.9（过线 → Act）；选择题第一个候选 0.9、置换众数一致
fn 端口<'a>(calls: &'a Cell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |s, qs| {
            calls.set(calls.get() + 1);
            let k = s.over.len().max(2);
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| match q.op {
                        Op::Select => {
                            let mut v = vec![0.1 / (k - 1) as f64; s.over.len()];
                            v[0] = 0.9;
                            Answer::Choice(v)
                        }
                        _ => Answer::Noul(0.9),
                    })
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: qs
                    .iter()
                    .map(|q| (q.op == Op::Select).then_some(1.0))
                    .collect(),
                perms: qs
                    .iter()
                    .map(|q| if q.op == Op::Select { 5 } else { 0 })
                    .collect(),
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", move |_p, _c, n, _r| {
            Ok(GenResult {
                outputs: (0..n)
                    .map(|i| serde_json::json!(format!("生成{i}")))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                ..Default::default()
            })
        }))
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k", "ks"] {
        c.put(k, 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    }
    c
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

fn 跑(src: &str, ledger: &mut Ledger, calls: &Cell<u64>) -> Outcome {
    run(
        &程序(src),
        端口(calls),
        &库(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()))
}

/// 三道是非题各在自己的状态上，逐个 `cut`（三层）；未决随返回值转交
const 三题: &str = r#"
budget {calls: 1, cost: 0, depth: 16};
fn 判(t) {
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    {k: exit_kind(e), e: e}
}
let rs = [判("甲"), 判("乙"), 判("丙")];
{v: map(rs, fn(r) { r.k }), pending: map(rs, fn(r) { r.e })}
"#;

#[test]
fn judge直接调用超预算给unsure_budget程序照常返回() {
    let calls = Cell::new(0);
    let o = 跑(三题, &mut Ledger::new(), &calls);
    assert!(o.pending.is_empty(), "不再挂起：{:?}", o.pending);
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["act", "unsure(budget)", "unsure(budget)"])
    );
    assert_eq!(calls.get(), 1);
    let b = o.budget.as_ref().expect("报告预算停机");
    assert!(b.exhausted);
    assert_eq!(b.unsent, 2);
    let w: Vec<_> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-budget"))
        .collect();
    assert_eq!(w.len(), 1, "W-budget 一条：{w:?}");
}

#[test]
fn select超预算同样降级() {
    let src = r#"
budget {calls: 1, cost: 0, depth: 16};
fn 挑(t) {
    let e = cut(judge(state(mat(t), {over: ["x", "y"]}), select("挑一个", "ks")));
    {k: exit_kind(e), e: e}
}
let rs = [挑("甲"), 挑("乙")];
{v: map(rs, fn(r) { r.k }), pending: map(rs, fn(r) { r.e })}
"#;
    let calls = Cell::new(0);
    let o = 跑(src, &mut Ledger::new(), &calls);
    assert!(o.pending.is_empty(), "{:?}", o.pending);
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["pick(0)", "unsure(budget)"])
    );
}

#[test]
fn 续跑带更高预算重发停发的站点() {
    let mut l = Ledger::new();
    let c1 = Cell::new(0);
    let _ = 跑(三题, &mut l, &c1);
    // 预算字面量等长替换：源码字节偏移（站点）不变，账本键不变
    let 更高 = 三题.replace("calls: 1,", "calls: 9,");
    let c2 = Cell::new(0);
    let o = 跑(&更高, &mut l, &c2);
    assert_eq!(c2.get(), 2, "只重发停发的两个站点");
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["act", "act", "act"])
    );
    assert!(o.budget.is_none());
}

#[test]
fn 审计重放在同一站点停发() {
    let mut l = Ledger::new();
    let c1 = Cell::new(0);
    let o1 = 跑(三题, &mut l, &c1);
    let n_absent = l
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { cause, .. } if cause == "budget"))
        .count();
    assert_eq!(n_absent, 2, "停发的两个站点记缺席账，首因 budget");
    let c2 = Cell::new(0);
    let o2 = run_replay(
        &程序(三题),
        端口(&c2),
        &库(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("审计重放：{}", e.render()));
    assert_eq!(c2.get(), 0, "重放不发");
    assert_eq!(o2.value_json(), o1.value_json());
    assert_eq!(o2.budget.as_ref().map(|b| b.unsent), Some(2));
}

#[test]
fn gen与do超预算产出失败值_do不执行() {
    let src = r#"
budget {calls: 1, cost: 0, depth: 16};
let a = gen("写一句", [], 1, 0);
let b = gen("再写一句", [], 1, 0);
let c = do("落盘", ["x"], 0);
{a: is_fail(a), b: is_fail(b), c: is_fail(c)}
"#;
    let 执行 = Rc::new(RefCell::new(0));
    let e2 = 执行.clone();
    let mut acts = ActionRegistry::new();
    acts.register("落盘", 0.0, true, TaintOut::Trusted, move |_| {
        *e2.borrow_mut() += 1;
        Ok(Value::text("写了"))
    });
    let calls = Cell::new(0);
    let o = run(&程序(src), 端口(&calls), &库(), &acts, &mut Ledger::new())
        .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()));
    assert_eq!(
        o.value_json(),
        serde_json::json!({"a": false, "b": true, "c": true})
    );
    assert_eq!(*执行.borrow(), 0, "预算耗尽时动作不执行");
    assert_eq!(o.budget.as_ref().map(|b| b.unsent), Some(2));
}
