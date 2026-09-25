//! 未决责任（`U(q)`）的去向：进 unsure 臂不等于销账。
//!
//! `handle` 把责任本身交给 unsure 臂，臂体跑完再核它是不是真的交出去了。合法去向四条：
//! escalate 给人、literalize 按更字面的题重问、包进返回值交给调用者、consume 显式丢并记账
//! （`12` §6 允许 drop，core 会留一条 `W-drop-vs-escalate`）。什么都不做就是静默丢弃，J-05 错。

mod common;

use common::program as build;
use common::*;
use jpp::effects::{CalibStore, FixedPorts};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{Budget, Program};
use jpp::{Error, check, run, run_unchecked};
use serde_json::json;

const ASK: &str = "这个对吗？";
const LITERAL: &str = "字面前提成立吗？";
const CALIB: &str = "k";

fn spend(escalate: u64) -> Budget {
    Budget {
        calls: 4,
        cost: 0.0,
        depth: Some(32),
        escalate: Some(escalate),
        unsure: None,
        absent: None,
        latency_p95: None,
    }
}

fn calibrations() -> CalibStore {
    let mut calib = CalibStore::new();
    calib
        .put(CALIB, 0.8, 0.2, 100, "上岗", Some(0.05))
        .expect("校准记录合法");
    calib
}

fn material() -> serde_json::Value {
    json!({"x": 1})
}

fn client() -> FixedPorts {
    let mut client = FixedPorts::new();
    let state = State::new(
        vec![Mat::literal(material())],
        vec![],
        vec![],
        vec![],
        false,
    );
    client.fix_ask(
        &state,
        &Question::new(Op::Test, ASK, CALIB, vec![]),
        Some(Answer::Noul(0.9)),
    );
    client.observe(
        &state,
        &Question::new(Op::Test, LITERAL, CALIB, vec![]),
        Answer::Noul(0.95),
    );
    client
}

/// `let q = …; let s = …; handle(unsure("材料不够"), {act: 1, ignore: 0, unsure: <臂>})`
fn duty_program(arm: Expr, escalate: u64) -> Program {
    build(
        Some(spend(escalate)),
        vec![
            bind("q", call("test", vec![text(ASK), text(CALIB)])),
            bind("q2", call("test", vec![text(LITERAL), text(CALIB)])),
            bind(
                "s",
                call("state", vec![call("mat", vec![rec(vec![("x", int(1))])])]),
            ),
        ],
        call(
            "handle",
            vec![
                call("unsure", vec![text("材料不够")]),
                rec(vec![("act", int(1)), ("ignore", int(0)), ("unsure", arm)]),
            ],
        ),
    )
}

fn go(program: &Program) -> Result<jpp::Outcome, Error> {
    let mut c = client();
    let mut ledger = Ledger::new();
    run(
        program,
        c.ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
}

/// 臂体什么都不对责任做：进臂不等于销账。
#[test]
fn 忽略未决责任的臂是错() {
    let arm = lambda(&["u"], body(vec![], int(7)));
    let at = match &arm.kind {
        ExprKind::Function(_) => arm.span,
        _ => unreachable!(),
    };
    let program = duty_program(arm, 0);
    // 静态判不出（臂确实是个收一个参数的方法），由运行期把关
    assert!(check(&program).is_ok(), "{}", check(&program).render());
    let Err(Error::Runtime(e)) = go(&program) else {
        panic!("应当报 J-05")
    };
    assert_eq!(e.rule.as_deref(), Some("J-05"));
    assert_eq!(e.span, at, "出错位置指着那条臂");
    assert!(
        e.message.contains("escalate"),
        "报文要列出合法去向：{}",
        e.message
    );
}

/// 只读原因不算处理：`unsure_cause(u)` 是读展示快照，不转移责任。
#[test]
fn 只读原因不算处理() {
    let program = duty_program(
        lambda(&["u"], body(vec![], call("unsure_cause", vec![name("u")]))),
        0,
    );
    let Err(Error::Runtime(e)) = go(&program) else {
        panic!("应当报 J-05")
    };
    assert_eq!(e.rule.as_deref(), Some("J-05"));
}

/// 字面量臂根本收不下责任——这条静态就能判。
#[test]
fn 字面量臂静态就被拦() {
    let program = duty_program(boolean(false), 0);
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert!(d.message.contains("字面量"), "{}", d.message);
    // 运行期也拦得住：两道防线不互相替代
    let mut c = client();
    let mut ledger = Ledger::new();
    let e = run_unchecked(
        &program,
        c.ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_err();
    assert_eq!(e.rule.as_deref(), Some("J-05"));
}

/// 通配分支兜不住未决责任：Unsure 必须有自己的一臂。
#[test]
fn otherwise兜不住未决责任() {
    let program = build(
        Some(spend(0)),
        vec![],
        call(
            "handle",
            vec![
                call("unsure", vec![text("材料不够")]),
                rec(vec![("otherwise", int(1))]),
            ],
        ),
    );
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert!(d.message.contains("otherwise"), "{}", d.message);

    let mut c = client();
    let mut ledger = Ledger::new();
    let e = run_unchecked(
        &program,
        c.ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_err();
    assert_eq!(e.rule.as_deref(), Some("J-05"));
    assert!(e.message.contains("otherwise"), "{}", e.message);
}

/// 合法去向一：包进返回值，责任转交给调用者。
#[test]
fn 包进返回值是合法去向() {
    let program = duty_program(
        lambda(
            &["u"],
            body(
                vec![],
                rec(vec![("待办", name("u")), ("下一步", text("补材料"))]),
            ),
        ),
        0,
    );
    let outcome = go(&program).unwrap_or_else(|e| panic!("{}", e.render()));
    let value = outcome.value_json();
    assert_eq!(value["下一步"], json!("补材料"));
    assert_eq!(
        value["待办"]["unsure"],
        json!("材料不够"),
        "责任带着原因一起交出去"
    );
    // 13 §3：包进返回值是**转交**不是了结，责任一路挂到程序结束；带到最外层要留账。
    // 断言落在结构化字段上——`Outcome.returned_unsure` 是契约，trace 里那条是给人看的文本，
    // 措辞早晚会改，拿字符串当契约会让测试在无关改动上碎。
    assert_eq!(
        outcome.returned_unsure.len(),
        1,
        "带到最外层的未决要记在 returned_unsure 里：{:?}",
        outcome.returned_unsure
    );
    assert!(
        outcome.returned_unsure[0].contains("unsure"),
        "记的就是这份责任：{:?}",
        outcome.returned_unsure
    );
}

/// 合法去向二：escalate 交给人。
#[test]
fn escalate交给人是合法去向() {
    let program = duty_program(
        lambda(
            &["u"],
            body(
                vec![],
                call("escalate", vec![name("u"), name("s"), name("q")]),
            ),
        ),
        1,
    );
    assert!(check(&program).is_ok(), "{}", check(&program).render());
    let outcome = go(&program).unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(
        outcome.value_json()["exit"],
        json!("act"),
        "人答了，换来一个已决出口"
    );
    assert_eq!(outcome.cost.asks, 1);

    // 没给 escalate 预算就是 E10：问出去只会直接挂起
    let no_budget = duty_program(
        lambda(
            &["u"],
            body(
                vec![],
                call("escalate", vec![name("u"), name("s"), name("q")]),
            ),
        ),
        0,
    );
    assert!(
        check(&no_budget).find("J-07").is_some(),
        "{}",
        check(&no_budget).render()
    );
}

/// 合法去向三：literalize 按更字面的题重问，换来一个新的、仍要处理的出口。
#[test]
fn 字面化重问是合法去向() {
    let program = duty_program(
        lambda(
            &["u"],
            body(
                vec![],
                call(
                    "handle",
                    vec![
                        call("literalize", vec![name("u"), name("s"), name("q2")]),
                        rec(vec![
                            ("act", text("收")),
                            ("ignore", text("弃")),
                            (
                                "unsure",
                                lambda(&["v"], body(vec![], call("unsure", vec![name("v")]))),
                            ),
                        ]),
                    ],
                ),
            ),
        ),
        0,
    );
    let outcome = go(&program).unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(outcome.value_json(), json!("收"));
    assert_eq!(outcome.cost.calls, 1, "重问是一次真的判断");
}

/// 合法去向四：显式 drop。`12` §6 允许，但 core 会留一条提示。
#[test]
fn 显式丢弃合法但留痕() {
    let program = duty_program(
        lambda(
            &["u"],
            body(
                vec![discard(call("consume", vec![name("u"), text("drop")]))],
                text("放着不管"),
            ),
        ),
        0,
    );
    let outcome = go(&program).unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(outcome.value_json(), json!("放着不管"));
    assert_eq!(outcome.trace.warnings.len(), 1);
    assert!(
        outcome.trace.warnings[0].starts_with("W-drop-vs-escalate"),
        "{:?}",
        outcome.trace.warnings
    );
}

/// 责任不能换个壳就消失：变成材料也不行。
#[test]
fn 责任不能变成材料() {
    let program = duty_program(
        lambda(&["u"], body(vec![], call("mat", vec![name("u")]))),
        0,
    );
    let Err(Error::Runtime(e)) = go(&program) else {
        panic!("应当报 J-05")
    };
    assert_eq!(e.rule.as_deref(), Some("J-05"));
    assert!(e.message.contains("材料"), "{}", e.message);
}

/// 表层的方法类型 `Fn(params) -!{effects}-> ret`（`linear` 即 `Fn¹`，捕获了未决责任）
fn method(params: Vec<Type>, ret: Type, effects: &[&str], linear: bool) -> Type {
    Type::Method {
        parameters: params,
        result: Box::new(ret),
        effects: Some(effects.iter().map(|e| e.to_string()).collect()),
        captures_responsibility: linear,
    }
}

/// 捕获了责任的 `Fn¹` 不能交给 map：它可能被调用任意次，也可能一次都不调。
#[test]
fn 捕获责任的方法不能交给map() {
    use Statement as S;
    let linear = method(
        vec![Type::Named("Int".into())],
        Type::Named("Int".into()),
        &[],
        true,
    );
    let apply_all = S::Function {
        name: "apply_all".into(),
        function: Function {
            parameters: vec![
                Parameter {
                    name: "f".into(),
                    annotation: Some(linear),
                    span: sp(),
                },
                Parameter {
                    name: "xs".into(),
                    annotation: None,
                    span: sp(),
                },
            ],
            result_type: None,
            effects: None,
            body: body(vec![], call("map", vec![name("xs"), name("f")])),
        },
        span: sp(),
    };
    let program = build(Some(spend(0)), vec![apply_all], int(0));
    let report = check(&program);
    let d = report
        .find("J-05")
        .unwrap_or_else(|| panic!("应当报 J-05：\n{}", report.render()));
    assert!(d.message.contains("Fn¹"), "{}", d.message);
}

/// 方法类型带效应行时，经参数调用的效应也能核——这正是 `Type::Function` 补不上的那一半。
#[test]
fn 方法类型带着效应行走() {
    use Statement as S;
    let judging = method(
        vec![Type::Named("Mat".into())],
        Type::Named("Record".into()),
        &["judge"],
        false,
    );
    let driver = S::Function {
        name: "drive".into(),
        function: Function {
            parameters: vec![
                Parameter {
                    name: "method".into(),
                    annotation: Some(judging),
                    span: sp(),
                },
                Parameter {
                    name: "m".into(),
                    annotation: None,
                    span: sp(),
                },
            ],
            result_type: None,
            // 标成纯的，但参数的类型说它会 judge
            effects: Some(vec![]),
            body: body(vec![], call_of(name("method"), vec![name("m")])),
        },
        span: sp(),
    };
    let program = build(Some(spend(0)), vec![driver], int(0));
    let report = check(&program);
    let d = report
        .find("E-effect")
        .unwrap_or_else(|| panic!("应当报 E-effect：\n{}", report.render()));
    assert!(d.message.contains("judge"), "{}", d.message);

    // 换成没有效应行的旧式 Fn 类型，静态就判不了，按「宁可漏报也不误报」跳过
    let opaque = S::Function {
        name: "drive".into(),
        function: Function {
            parameters: vec![
                Parameter {
                    name: "method".into(),
                    annotation: Some(Type::Function(
                        vec![Type::Named("Mat".into())],
                        Box::new(Type::Named("Record".into())),
                    )),
                    span: sp(),
                },
                Parameter {
                    name: "m".into(),
                    annotation: None,
                    span: sp(),
                },
            ],
            result_type: None,
            effects: Some(vec![]),
            body: body(vec![], call_of(name("method"), vec![name("m")])),
        },
        span: sp(),
    };
    let loose = build(Some(spend(0)), vec![opaque], int(0));
    assert!(
        check(&loose).find("E-effect").is_none(),
        "{}",
        check(&loose).render()
    );
}
