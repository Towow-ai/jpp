//! 步 24h：`W-legacy-fn-type`（K-052；`14-实施计划-把语言做完整-v1.md` §十二第 7 条）——形参标注
//! 旧式 `Fn(...) -> ...`（无效应行）且在函数体内被当调用者时报。预注册见 `地基/过程记录/工程-步24h.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn errors(src: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "W-legacy-fn-type")
        .map(|d| d.message.clone())
        .collect()
}

/// 旧式类型参数在体内被调用：报，点名参数。
#[test]
fn 旧式类型参数被调用_报() {
    let src = r#"budget {calls: 0, cost: 0};
fn apply(x: Int, method: Fn(Int) -> Int) -> Int { method(x) }
apply(1, fn(y: Int) -> Int { y })
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("method"), "{}", es[0]);
}

/// 新式（带效应行）类型参数：不报，即使被调用。
#[test]
fn 新式带效应行参数被调用_不报() {
    let src = r#"budget {calls: 0, cost: 0};
fn apply(x: Int, method: Fn(Int) -!{judge}-> Int) -> Int !{judge} { method(x) }
0
"#;
    assert!(errors(src).is_empty());
}

/// 空效应行（Fn(...) -!{}-> ...）也是新式（Type::Method），不报。
#[test]
fn 空效应行参数被调用_不报() {
    let src = r#"budget {calls: 0, cost: 0};
fn apply(x: Int, method: Fn(Int) -!{}-> Int) -> Int !{} { method(x) }
0
"#;
    assert!(errors(src).is_empty());
}

/// 旧式类型参数没有在体内被调用（只是原样返回）：不报——原文只管「被调用的方法参数位」。
#[test]
fn 旧式类型参数不被调用_不报() {
    let src = r#"budget {calls: 0, cost: 0};
fn pass_through(method: Fn(Int) -> Int) -> Fn(Int) -> Int { method }
0
"#;
    assert!(errors(src).is_empty());
}

/// 嵌套闭包捕获外层旧式类型参数并调用它（`compose` 的形状）：报——不能因为写成「返回一个闭包」
/// 就漏报，闭包体内确实调用了。
#[test]
fn 嵌套闭包调用外层旧式参数_报() {
    let src = r#"budget {calls: 0, cost: 0};
fn compose(f: Fn(Int) -> Int, g: Fn(Int) -> Int) -> Fn(Int) -> Int {
    fn(x: Int) -> Int { g(f(x)) }
}
0
"#;
    let es = errors(src);
    assert_eq!(es.len(), 2, "{es:?}");
}

/// 形参名被内层 `let` 遮蔽后不算：不报。
#[test]
fn 遮蔽后不报() {
    let src = r#"budget {calls: 0, cost: 0};
fn apply(x: Int, method: Fn(Int) -> Int) -> Int {
    let method = fn(y: Int) -> Int { y };
    method(x)
}
0
"#;
    assert!(errors(src).is_empty());
}

/// 未标注类型的参数（无 annotation）：不受本规则管，不报。
#[test]
fn 无标注参数_不报() {
    let src = r#"budget {calls: 0, cost: 0};
fn apply(x, method) { method(x) }
0
"#;
    assert!(errors(src).is_empty());
}
