//! 步 24e：J-04 比较性指纹静态面的 `order` 半（`12` §5 J-04；运行期 `repeat.rs::b_order` 的
//! `q_hash` 不可比检查在**能静态确定**时的编译期镜像）。判据：`order(...)` 收字面列表，每项能
//! 解析成「产出读数的效应调用」（直接写或绑定给恰好出现一次的名字）的字面题式（`test`/`select`/
//! `measure`）比较键（构造名、题面文字、档位列表）时逐项比较，不同即报；判不出来源的项一律放过
//! （零假拒绝面，运行期兜底）。预注册见 `地基/过程记录/工程-步24e.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn errors(src: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "J-04")
        .map(|d| d.message.clone())
        .collect()
}

/// 两个字面绑定、题面不同：报 J-04（对应 `fit.rs::order不能跨题排序` 现在能在检查期而不是
/// 运行期拦住的同一形状）。
#[test]
fn order列表里两项题面不同_报j04() {
    let src = r#"budget {calls: 4, cost: 1};
let 甲 = judge(state(mat("x")), test("问题一", "k"));
let 乙 = judge(state(mat("x")), test("问题二", "k2"));
order([甲, 乙])
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(
        es[0].contains("问题一") && es[0].contains("问题二"),
        "{}",
        es[0]
    );
}

/// 两个字面绑定、同一题面：不报。
#[test]
fn order列表里两项同一题面_不报() {
    let src = r#"budget {calls: 4, cost: 1};
let 甲 = judge(state(mat("x")), test("问题一", "k"));
let 乙 = judge(state(mat("y")), test("问题一", "k"));
order([甲, 乙])
"#;
    assert!(errors(src).is_empty());
}

/// 内联写（不经 `let`），题面不同：同样报。
#[test]
fn order列表里内联题面不同_报j04() {
    let src = r#"budget {calls: 4, cost: 1};
order([judge(state(mat("x")), test("问题一", "k")), judge(state(mat("x")), test("问题二", "k2"))])
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}

/// `order` 单个 `judge(...)`（多对象、同一题），不是字面列表：不报——`order` 同题跨对象是正常用法
/// （对应 `fit.rs::order同题跨对象照常排`）。
#[test]
fn order单个judge调用不是字面列表_不报() {
    let src = r#"budget {calls: 4, cost: 1};
order(judge([state(mat("甲")), state(mat("乙"))], test("同一道题", "k")))
"#;
    assert!(errors(src).is_empty());
}

/// 名字绑定超过一次：判不出唯一来源，按可比放过，不报（宁可漏报，同 `j06.rs::binding_counts`
/// 的保守方向）。
#[test]
fn 名字绑定不止一次_判不出来源不报() {
    let src = r#"budget {calls: 4, cost: 1};
let 甲 = judge(state(mat("x")), test("问题一", "k"));
let 甲 = judge(state(mat("x")), test("问题二", "k2"));
order([甲, 甲])
"#;
    assert!(errors(src).is_empty());
}

/// `measure` 题式，档位列表不同：报。
#[test]
fn measure题式档位不同_报j04() {
    let src = r#"budget {calls: 4, cost: 1};
let 甲 = judge(state(mat("x")), measure("多高", ["低", "高"], "k"));
let 乙 = judge(state(mat("x")), measure("多高", ["低", "中", "高"], "k2"));
order([甲, 乙])
"#;
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}

/// 少于两项：不比，不报。
#[test]
fn 单项列表_不报() {
    let src = r#"budget {calls: 4, cost: 1};
let 甲 = judge(state(mat("x")), test("问题一", "k"));
order([甲])
"#;
    assert!(errors(src).is_empty());
}
