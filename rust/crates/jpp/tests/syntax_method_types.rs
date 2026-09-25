use jpp::{Stmt as Statement, Type};
use jpp::{lower, syntax::parse};

#[test]
fn method_rows_and_capture_annotations_lower_without_changing_old_fn_types() {
    let src = "budget {calls:0,cost:0}; fn f(old: Fn(Int) -> Int, new: Fn(Int) -!{judge,do}-> Int, duty: Fn1(Int) -!{}-> Int) -> Fn(Int) -!{}-> Int !{} { old } 0";
    let program = lower(&parse(src).unwrap()).unwrap();
    let Statement::Function { function, .. } = &program.body.statements[0] else {
        panic!()
    };
    assert!(matches!(
        function.parameters[0].annotation,
        Some(Type::Function(..))
    ));
    let Some(Type::Method(m)) = &function.parameters[1].annotation else {
        panic!()
    };
    assert_eq!(m.effects.as_deref().unwrap(), ["judge", "do"]);
    assert!(!m.captures_responsibility);
    let Some(Type::Method(m)) = &function.parameters[2].annotation else {
        panic!()
    };
    assert!(m.captures_responsibility);
    assert_eq!(m.effects, Some(vec![]));
    assert_eq!(function.effects, Some(vec![]));
    assert!(matches!(function.result_type, Some(Type::Method(_))));
}

#[test]
fn declared_method_effects_reach_the_common_checker() {
    let src =
        "budget {calls:1,cost:0}; fn apply(m: Mat, f: Fn(Mat) -!{judge}-> Exit) !{} { f(m) } 0";
    let bad = lower(&parse(src).unwrap()).unwrap();
    let report = jpp::check::check(&bad);
    assert!(report.find("E-effect").is_some(), "{}", report.render());
    let good = lower(&parse(&src.replace(") !{}", ") !{judge}")).unwrap()).unwrap();
    assert!(jpp::check::check(&good).is_ok());
}

#[test]
fn capture_contract_reaches_higher_order_checks() {
    let src = "budget {calls:0,cost:0}; fn use_many(f: Fn1(Int) -!{}-> Int) { map([1,2], f) } 0";
    let report = jpp::check::check(&lower(&parse(src).unwrap()).unwrap());
    assert!(!report.is_ok(), "linear capture must reach core checks");
}

#[test]
fn malformed_rows_have_a_source_location() {
    let src = "budget {calls:0,cost:0};\nfn f(cb: Fn(Int) -!{judge} Int) { 0 }";
    let error = parse(src).unwrap_err();
    assert!(error.render("broken.jpp", src).contains("broken.jpp:2:"));
}

#[test]
fn effectful_argument_cannot_satisfy_a_pure_method_parameter() {
    let src = "budget {calls:1,cost:0}; fn effect() -> Unit !{judge} { unit } fn apply(f: Fn() -!{}-> Unit) !{} { f() } apply(effect)";
    let report = jpp::check::check(&lower(&parse(src).unwrap()).unwrap());
    let error = report
        .find("E-effect")
        .expect("the actual method must respect the parameter row");
    assert_eq!(&src[error.span.start..error.span.end], "effect");
    let pure = src.replace("!{judge}", "!{}");
    assert!(jpp::check::check(&lower(&parse(&pure).unwrap()).unwrap()).is_ok());
}
