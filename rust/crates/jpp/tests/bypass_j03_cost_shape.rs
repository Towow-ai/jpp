//! 步 24h：`cut` 代价记录 `{cost: [fp, fn]}` 的形状预检（K-143/B29）——运行期已严格校验（两个
//! 正数），这里做能静态确定时的编译期镜像：`cost` 是字面列表但不是恰好两个数字元素时报 J-03。
//! `cost` 字段值不是字面列表（名字/表达式）时判不出来，放过。预注册见 `地基/过程记录/工程-步24h.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn errors(src: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "J-03")
        .map(|d| d.message.clone())
        .collect()
}

/// 合法形状（两个数字）：不报。
#[test]
fn 两个数字_不报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), {cost: [1, 2]})
"#;
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}

/// 三元素列表：报。
#[test]
fn 三个元素_报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), {cost: [1, 2, 3]})
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("cost"), "{}", es[0]);
}

/// 一个元素：报。
#[test]
fn 一个元素_报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), {cost: [1]})
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}

/// 元素不是数字：报。
#[test]
fn 元素非数字_报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), {cost: ["a", "b"]})
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}

/// 带 calib_key 的三参形式（cost 在第三位）：同样核对。
#[test]
fn 带calib_key时cost在第三位_也核() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), "k", {cost: [1, 2, 3]})
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}

/// `cost` 不是字面列表（名字）：判不出来，不报。
#[test]
fn cost非字面列表_不报() {
    let src = r#"budget {calls: 2, cost: 1};
let c = [1, 2, 3];
cut(judge(state(mat("x")), test("行吗", "k")), {cost: c})
"#;
    assert!(errors(src).is_empty());
}

/// 没有 `cost` 字段的记录（例如只有别的选项）：不报——这条不是本规则的事。
#[test]
fn 没有cost字段_不报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), {other: 1})
"#;
    assert!(errors(src).is_empty());
}

/// 没有代价记录，只有 calib_key：不报（这条不该被本函数触发）。
#[test]
fn 只有calib_key_不报() {
    let src = r#"budget {calls: 2, cost: 1};
cut(judge(state(mat("x")), test("行吗", "k")), "k")
"#;
    assert!(errors(src).is_empty());
}
