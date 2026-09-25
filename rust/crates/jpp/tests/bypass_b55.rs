//! 绕行测试（`20` §4 表「B56 效应形式不作值」行，文件名沿用该表的 `bypass/b55.rs`）：效应形式、
//! 语言形式与内核构造不作一等值。`let j = judge; j(s, q)` 这类写法会走宿主调用绕开效应节点——没有
//! `SiteId`、没有标注、没有计划；降级在名字出现在调用位置以外时报 `E-form-as-value`，这条路不存在。

use jpp::{lower, syntax::parse};

const B: &str = "budget {calls: 4, cost: 0};\n";

/// 降级报的 `E-form-as-value`，每条给出（名字在源码里的原文）
fn form_values(body: &str) -> Vec<String> {
    let src = format!("{B}{body}");
    match lower(&parse(&src).expect("解析")) {
        Ok(_) => vec![],
        Err(ds) => ds
            .iter()
            .filter(|d| d.message.starts_with("E-form-as-value: "))
            .map(|d| src[d.span.start..d.span.end].to_string())
            .collect(),
    }
}

#[test]
fn 效应名绑给变量再调用被拦() {
    let hits =
        form_values("let s = state(mat(\"材料\"));\nlet j = judge;\nj(s, test(\"行吗\", \"k\"))");
    assert_eq!(hits, ["judge"]);
}

#[test]
fn 效应名与构造名当实参被拦() {
    assert_eq!(form_values("map([\"甲\", \"乙\"], test)"), ["test"]);
    assert_eq!(form_values("fn apply(f, x) { f(x) }\napply(do, 1)"), ["do"]);
}

#[test]
fn 放进容器与记录字段被拦() {
    assert_eq!(form_values("[gen, 1]"), ["gen"]);
    assert_eq!(form_values("{via: sieve}"), ["sieve"]);
}

#[test]
fn 语言形式名同样不作值() {
    assert_eq!(form_values("let c = cut;\n1"), ["cut"]);
    assert_eq!(
        form_values("[state, handle, consume]"),
        ["state", "handle", "consume"]
    );
}

#[test]
fn 调用位置与遮蔽的名字照常() {
    // 调用位置：不报
    assert!(
        form_values("let s = state(mat(\"材料\"));\njudge(s, test(\"行吗\", \"k\"))").is_empty()
    );
    // 用户绑定遮蔽了形式名：那是用户名字
    assert!(form_values("let fit = test(\"行吗\", \"k\");\nfit").is_empty());
    assert!(form_values("fn g(judge) { judge }\ng(1)").is_empty());
}

#[test]
fn 宿主内置可以作值() {
    // `map`/`filter`/`fold` 与其余宿主内置在宿主内置表里，可以作值（`20` §2.3 `lower` 不变量）
    assert!(form_values("let m = map;\nm([1, 2], fn(x) { x })").is_empty());
    assert!(form_values("map([[1], [2, 3]], len)").is_empty());
}

#[test]
fn 缺预算的程序也照样报体内的形式作值() {
    // 库文件没有预算：降级先报 J-07a，体内的 E-form-as-value 一并报出
    let ds = lower(&parse("let j = judge;\n1").expect("解析")).expect_err("应当降级失败");
    assert!(ds[0].message.starts_with("J-07a: "), "{ds:?}");
    assert!(ds[1].message.starts_with("E-form-as-value: "), "{ds:?}");
}
