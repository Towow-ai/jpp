//! B38（步 15d）：`budget.calls` 计所有效应调用——`judge`、`gen`、`do`、`ask` 每次实际执行计一次；
//! `transform` 是宿主记账变换，不计；`ask` 同时受 `budget.escalate` 约束，两道限制并存。
//! 依据：B38（`20` 附录 A）；`21` §四·8「步 15d 拆分」。

use jpp::effects::{CalibStore, FixedPorts};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, TaintOut, run};
use jpp::{lower, syntax::parse};

fn 动作() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("记一笔", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::text("好"))
    });
    a
}

fn 跑(src: &str, fp: &mut FixedPorts) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    run(
        &program,
        fp.ports(),
        &CalibStore::new(),
        &动作(),
        &mut Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("跑得完：{}", e.render()))
}

/// do 每次执行计一次调用；预算恰好够用时跑完，少一次则第二次 do 不执行、产出失败值（步 22-0，B93）
#[test]
fn do计入调用与预算() {
    let src = |n: u32| {
        format!(
            "budget {{calls: {n}, cost: 0, depth: 8}};\nlet a = do(\"记一笔\", [1], 0);\nlet b = do(\"记一笔\", [2], 0);\n{{a: is_fail(a), b: is_fail(b)}}"
        )
    };
    let o = 跑(&src(2), &mut FixedPorts::new());
    assert_eq!(o.cost.calls, 2, "两次 do 计两次调用");
    assert!(o.pending.is_empty());
    let o = 跑(&src(1), &mut FixedPorts::new());
    assert!(o.pending.is_empty(), "{:?}", o.pending);
    assert_eq!(o.value_json(), serde_json::json!({"a": false, "b": true}));
    assert_eq!(o.cost.calls, 1);
}

/// ask 计入调用，同时受 escalate 约束
#[test]
fn ask计入调用且受escalate约束() {
    let src = "budget {calls: 1, cost: 0, depth: 8, escalate: 1};\nlet s = state(mat(\"x\"));\nlet q = test(\"行吗\", \"k\");\nhandle(ask(s, q), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, \"drop\"); -1 }})";
    let mut fp = FixedPorts::new();
    let s = jpp::value::State::new(
        vec![jpp::value::Mat::literal(serde_json::json!("x"))],
        vec![],
        vec![],
        vec![],
        false,
    );
    fp.fix_ask(
        &s,
        &jpp::value::Question::new(jpp::value::Op::Test, "行吗", "k", vec![]),
        Some(Answer::Noul(0.9)),
    );
    let o = 跑(src, &mut fp);
    assert_eq!(
        (o.cost.calls, o.cost.asks),
        (1, 1),
        "ask 计入 calls 与 asks"
    );
    let 零 = src.replace("calls: 1", "calls: 0");
    let o = 跑(&零, &mut fp);
    assert_eq!(o.pending.first().map(|p| p.cause.as_str()), Some("budget"));
}

/// transform 不计调用
#[test]
fn transform不计() {
    let src = "budget {calls: 0, cost: 0, depth: 8};\nlet m = transform(fn(x) { content(x) }, mat(\"甲\"));\ncontent(m)";
    let o = 跑(src, &mut FixedPorts::new());
    assert_eq!(o.cost.calls, 0);
    assert!(o.pending.is_empty(), "{:?}", o.pending);
}
