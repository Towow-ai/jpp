//! 步 24d（K-207，2026-09-24 设计收口盘点）：`test`/`select` 的 `{evidence: […]}` 声明，槽名必须
//! 是 `on`/`ctx`/`ref`/`over` 之一。运行期 `missing_evidence`（`jpp-runtime/src/lib.rs`）对不认识
//! 的名字静默当「不缺」处理，`insufficient` 的保护因此悄悄失效——这是检查器该拦的作者输入错误。
//! `measure` 现行实现严格三参、不支持 `evidence`，不检查它。
//! 依据：`地基/评估/2026-09-24-设计收口盘点.md`；预注册见 `地基/过程记录/工程-步24d.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn errors(src: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "J-09")
        .map(|d| d.message.clone())
        .collect()
}

/// 合法槽名（`on`）：不报。
#[test]
fn 合法槽名_不报() {
    let src = "budget {calls: 0, cost: 0};\ntest(\"行吗\", \"k\", {evidence: [\"on\"]})\n";
    assert!(errors(src).is_empty(), "{:?}", errors(src));
}

/// 全部四个合法槽名：不报。
#[test]
fn 全部四个合法槽名_不报() {
    let src = "budget {calls: 0, cost: 0};\ntest(\"行吗\", \"k\", {evidence: [\"on\", \"ctx\", \"ref\", \"over\"]})\n";
    assert!(errors(src).is_empty());
}

/// 拼写错误的槽名（`ctxx`）：报 J-09。
#[test]
fn 拼写错误的槽名_报j09() {
    let src = "budget {calls: 0, cost: 0};\ntest(\"行吗\", \"k\", {evidence: [\"ctxx\"]})\n";
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("ctxx"), "{}", es[0]);
}

/// 完全不存在的槽名（`material`）：报 J-09——这正是「静默失效」的典型写法：作者以为在声明
/// 决定性证据槽，实际上运行期从不检查它。
#[test]
fn 不存在的槽名_报j09() {
    let src =
        "budget {calls: 0, cost: 0};\nselect(\"选哪个\", \"k\", {evidence: [\"material\"]})\n";
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("material"), "{}", es[0]);
}

/// 多个槽名，一个错：只报那一个。
#[test]
fn 多个槽名只有一个错_只报那一个() {
    let src =
        "budget {calls: 0, cost: 0};\ntest(\"行吗\", \"k\", {evidence: [\"on\", \"typo\"]})\n";
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("typo"), "{}", es[0]);
}

/// 没有 `evidence` 声明：不报（第三参可以是别的选项，如 `presupposition`）。
#[test]
fn 没有evidence声明_不报() {
    let src = "budget {calls: 0, cost: 0};\ntest(\"行吗\", \"k\", {presupposition: \"前提\"})\n";
    assert!(errors(src).is_empty());
}

/// `measure` 现行实现不支持第四参，不检查它（写了也不会被这条规则触碰——运行期 `E-rt-arity` 会拦）。
#[test]
fn measure不检查() {
    let src = "budget {calls: 0, cost: 0};\nmeasure(\"档位\", [\"低\", \"高\"], \"k\")\n";
    assert!(errors(src).is_empty());
}
