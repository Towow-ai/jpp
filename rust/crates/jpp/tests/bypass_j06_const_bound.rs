//! 步 24a（B69）：`loop`/`iterate` 的 `bound` 若是名字，且这个名字在整个程序里恰好绑定一次、且
//! 这一次绑定是程序顶层块的 `let name = <正整数字面量>;`，判定为常量，不报 `W-bound`；真正被
//! 遮蔽（名字在别处又绑定一次）时判定失败，仍报。依据：B69（`地基/附注/2026-09-24-探针首轮裁定.md`
//! §五·1）；预注册见 `地基/过程记录/工程-步24a.md`。

use jpp::check;
use jpp::{lower, syntax::parse};

fn w_bound(src: &str) -> Vec<String> {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "W-bound")
        .map(|d| d.message.clone())
        .collect()
}

/// 命中：顶层常量、恰好绑定一次，喂给 loop 的 bound 不报。
#[test]
fn 顶层常量命中_不报() {
    let src = "budget {calls: 0, cost: 0};\nlet max_depth = 6;\nloop(max_depth, 0, fn(acc, i) { acc + i })\n";
    assert!(w_bound(src).is_empty(), "{:?}", w_bound(src));
}

/// 遮蔽：顶层 `let N = 5;`，函数体内又绑定一次同名 `N`——整个程序里 `N` 绑定两次，B69 判定失败，
/// 仍报 `W-bound`（保守方向：宁可少报不误判遮蔽为命中）。
#[test]
fn 遮蔽时仍报() {
    let src = "budget {calls: 0, cost: 0};\nlet N = 5;\nfn f() { let N = 3; loop(N, 0, fn(acc, i) { acc + i }) }\nf()\n";
    let ws = w_bound(src);
    assert_eq!(ws.len(), 1, "{ws:?}");
    assert!(ws[0].contains("bound"), "{ws:?}");
}

/// 非顶层的整数 `let`（函数体内）不算 B69 常量，即使从未被遮蔽也仍报（范围只认顶层）。
#[test]
fn 非顶层常量不命中() {
    let src = "budget {calls: 0, cost: 0};\nfn f() { let n = 4; loop(n, 0, fn(acc, i) { acc + i }) }\nf()\n";
    assert_eq!(w_bound(src).len(), 1);
}

/// iterate 同样吃 B69。
#[test]
fn iterate也吃常量传播() {
    let src = "budget {calls: 0, cost: 0};\nlet max_depth = 3;\niterate(max_depth, [], fn(acc, i) { acc }, \"tokens\")\n";
    assert!(w_bound(src).is_empty());
}
