use jpp::{lower, syntax::parse};

#[test]
fn lowering_keeps_explicit_budget_and_source_spans() {
    let source =
        "budget {calls: 10, cost: 0, depth: 64};\nfn twice(x: Int) -> Int !{} { x * 2 }\ntwice(21)";
    let core = lower(&parse(source).unwrap()).unwrap();
    assert_eq!(core.budget.calls, 10);
    let call = core.body.result.unwrap();
    assert_eq!(&source[call.span.start..call.span.end], "twice(21)");
    for source in [
        "budget {calls: 1}; 0",
        "budget {calls: 1, cost: 0, typo: 2}; 0",
        "budget {calls: dynamic, cost: 0}; 0",
    ] {
        assert!(lower(&parse(source).unwrap()).is_err());
    }
    // 缺预算在降级处报 J-07a（步 12d；此前降级放行、由检查器报 J-07）
    let e = lower(&parse("1").unwrap()).unwrap_err();
    assert!(e[0].message.starts_with("J-07a: "), "{e:?}");
}
