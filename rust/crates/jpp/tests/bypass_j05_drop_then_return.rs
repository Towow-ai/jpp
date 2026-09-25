//! 步 24a（B95 静态半）：`handle(B.exit, {…, unsure: fn(u) { consume(u, "drop"); … } })` 的
//! `unsure` 臂里，若又提到同一个裸名字 `B` 的 `.index`/`.pos`，报 `W-drop-then-return`——运行期
//! `duty.rs::drop_then_return`（步 21）自认看不见「只凭 index/pos 算出的编号」，这里补语法面。
//! 依据：`jpp-runtime/src/duty.rs::drop_then_return` 函数头注释；主会话 2026-09-25 确认范围；
//! 预注册见 `地基/过程记录/工程-步24a.md`。

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

/// drop 之后又返回 e.index：命中。
#[test]
fn drop后返回index_命中() {
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}map(accepted(r), fn(e) {{ handle(e.exit, {{act: fn() {{ e.index }}, ignore: fn() {{ -1 }}, unsure: fn(u) {{ consume(u, \"drop\"); e.index }}}}) }})\n"
    );
    let ws = warnings(&src, "W-drop-then-return");
    assert_eq!(ws.len(), 1, "{ws:?}");
    assert!(ws[0].contains("index") || ws[0].contains("pos"), "{ws:?}");
}

/// drop 但不返回 index/pos：不命中。
#[test]
fn drop但不返回index_不命中() {
    let src = format!(
        "budget {{calls: 0, cost: 0, depth: 8}};\n{绑定}map(accepted(r), fn(e) {{ handle(e.exit, {{act: fn() {{ e.index }}, ignore: fn() {{ -1 }}, unsure: fn(u) {{ consume(u, \"drop\"); -1 }}}}) }})\n"
    );
    assert!(warnings(&src, "W-drop-then-return").is_empty());
}

/// handle 的 exit 实参不是 `.exit` 字段（裸名字本身就是出口），不满足「必须是 Field{.., "exit"}」
/// 的形状要求，即便 unsure 臂也 drop 又返回某个 `.index`，也不命中（范围收窄，登记见预注册）。
#[test]
fn exit实参不是字段形式_不命中() {
    let src = "budget {calls: 0, cost: 0, depth: 8};\nlet exit = cut(judge(state(mat(\"x\")), test(\"行吗\", \"k\")));\nhandle(exit, {act: fn() { \"是\" }, ignore: fn() { \"否\" }, unsure: fn(u) { consume(u, \"drop\"); \"未决\" }})\n";
    assert!(warnings(src, "W-drop-then-return").is_empty());
}
