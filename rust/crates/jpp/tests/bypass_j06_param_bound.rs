//! 步 24a（B111，A-3b）：具名函数体内把自己的形参直接喂给 `loop`/`iterate` 的 `bound`——体内不报，
//! 改在每个调用点核实参；字面量或 B69 常量不报，否则在调用点报 `W-bound`；递归调用也只是一个
//! 调用点，同一条规则核，不特殊处理。依据：B111（`12` §3 J-06 行）；预注册见
//! `地基/过程记录/工程-步24a.md`。

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

const 定义: &str = "fn f(n) { loop(n, 0, fn(acc, i) { acc + i }) }\n";

/// 字面量实参：体内不报，调用点也不报。
#[test]
fn 字面量实参不报() {
    let src = format!("budget {{calls: 0, cost: 0}};\n{定义}f(5)\n");
    assert!(w_bound(&src).is_empty(), "{:?}", w_bound(&src));
}

/// 非字面量实参：体内仍不报（B111「体内不报」），但在调用点报。
#[test]
fn 非字面量在调用点报() {
    let src = format!("budget {{calls: 0, cost: 0}};\n{定义}let x = 3 + 1;\nf(x)\n");
    let ws = w_bound(&src);
    assert_eq!(ws.len(), 1, "{ws:?}");
    assert!(ws[0].contains("f") && ws[0].contains("bound"), "{ws:?}");
}

/// 递归调用：`f` 自己调 `f(n - 1)`，`n - 1` 不是字面量，递归调用点也报（不特殊处理递归边）。
#[test]
fn 递归报() {
    let src = "budget {calls: 0, cost: 0};\nfn f(n) { loop(n, 0, fn(acc, i) { acc + i }); if n > 0 { f(n - 1) } else { 0 } }\nf(5)\n";
    let ws = w_bound(src);
    assert_eq!(ws.len(), 1, "{ws:?}");
}
