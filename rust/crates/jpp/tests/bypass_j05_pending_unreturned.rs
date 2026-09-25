//! 步 24a（B81(c) 补句、B95）：契约值绑定之后被判定「用过」（不然已是 J-05 error），但只碰了
//! 已决一侧（`.value`/`accepted()`/`ignored()`），从没碰未决一侧（`.pending`/`undecided()`/
//! `unobserved()`），也没整体转交、没显式 `consume`——报 `W-pending-unreturned`。
//! 依据：`12` §3 J-05 注（B81(c) 补句与 B95）；预注册见 `地基/过程记录/工程-步24a.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn warnings(src: &str, rule: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == rule)
        .map(|d| d.message.clone())
        .collect()
}

const 绑定: &str = "let r = sieve([\"a\", \"b\"], test(\"是否包含\", \"k\"), [1]);\n";

/// 只narrow引用：命中。
#[test]
fn 只用accepted_命中() {
    let src = format!("budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}accepted(r)\n");
    let ws = warnings(&src, "W-pending-unreturned");
    assert_eq!(ws.len(), 1, "{ws:?}");
    assert!(
        ws[0].contains("pending") || ws[0].contains("未决"),
        "{ws:?}"
    );
}

/// 同时给了 r.pending：未决感知信号，不命中。
#[test]
fn 也返回pending字段_不命中() {
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}{{a: accepted(r), p: r.pending}}\n"
    );
    assert!(warnings(&src, "W-pending-unreturned").is_empty());
}

/// 整体返回 r：不命中。
#[test]
fn 整体返回_不命中() {
    let src = format!("budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}r\n");
    assert!(warnings(&src, "W-pending-unreturned").is_empty());
}

/// 用了 undecided(r)：未决感知，不命中。
#[test]
fn 也用undecided_不命中() {
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}{{a: accepted(r), u: undecided(r)}}\n"
    );
    assert!(warnings(&src, "W-pending-unreturned").is_empty());
}

/// 显式 consume(r, "drop")：不命中（虽然 consume 契约值本身另有 J-05 判据，这里只测本条不额外报）。
#[test]
fn 显式consume_不额外命中本条() {
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}let a = accepted(r);\nconsume(r, \"drop\");\na\n"
    );
    assert!(warnings(&src, "W-pending-unreturned").is_empty());
}
