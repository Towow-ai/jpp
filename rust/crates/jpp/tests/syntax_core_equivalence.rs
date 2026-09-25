//! The same method composition expressed as source and as a directly built program.
//! 步 12d 前「直接构造」指核心语法树；核心语法树删除后改用表层 AST 建造器直接构造，再降到 IR。
mod common;

use common::*;
use jpp::{
    Program,
    effects::{CalibStore, NoCallPorts},
    interp::ActionRegistry,
    ledger::Ledger,
};

fn execute(program: &Program) -> String {
    let mut ledger = Ledger::new();
    let calibrations = CalibStore::new();
    let actions = ActionRegistry::new();
    let outcome = jpp::run(
        program,
        NoCallPorts::ports(),
        &calibrations,
        &actions,
        &mut ledger,
    )
    .unwrap();
    assert!(outcome.pending.is_empty());
    assert_eq!(outcome.cost.calls, 0);
    outcome.value_json().to_string()
}

#[test]
fn nested_method_composition_matches_direct_core_construction() {
    let source = jpp::syntax::parse(include_str!("../../../examples/composition.jpp")).unwrap();
    let lowered = jpp::lower(&source).unwrap();
    let returned_method = lambda(
        &["x"],
        body(vec![], call("g", vec![call("f", vec![name("x")])])),
    );
    let compose = lambda(&["f", "g"], body(vec![], returned_method));
    let increment = lambda(&["x"], body(vec![], bin("+", name("x"), int(1))));
    let twice = lambda(&["x"], body(vec![], bin("*", name("x"), int(2))));
    let direct = program(
        Some(budget(0, 256)),
        vec![
            bind("compose", compose),
            bind("increment", increment),
            bind("twice", twice),
            bind(
                "method",
                call("compose", vec![name("increment"), name("twice")]),
            ),
            bind(
                "larger",
                call("compose", vec![name("method"), name("increment")]),
            ),
        ],
        rec(vec![
            ("result", call("larger", vec![int(20)])),
            ("expected", int(43)),
        ]),
    );
    let result = execute(&lowered);
    assert_eq!(result, execute(&direct));
    assert_eq!(result, r#"{"expected":43,"result":43}"#);
    let typed_source = include_str!("../../../examples/composition.jpp")
        .replace("Fn(Int) -> Int", "Fn(Int) -!{}-> Int");
    let typed = jpp::lower(&jpp::syntax::parse(&typed_source).unwrap()).unwrap();
    assert_eq!(execute(&typed), execute(&direct));
}
