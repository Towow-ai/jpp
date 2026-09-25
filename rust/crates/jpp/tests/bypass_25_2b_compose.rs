//! 合成构造 `compose` 与元素构造 `element` 的内核一半（步 25-2b，B131、B133）。
//!
//! 本步 `tally`、`first_k` 的聚合出口改经 `compose` 签发、`sieve` 的元素改经 `element` 造；两者是内部构造，
//! 还不能在 `.jpp` 里按名字调用。这里钉住三件从返回值与报告看得见的事：(a) 聚合出口带 `parts`（有出口的元素
//! 各一个），仍按冷线不放行（25-1 保持到 25-9）；(b) `first_k` 出口的 `parts` 是全部元素的出口，不只读到的；
//! (c) 链式过滤的报告 `exits` 行由 `element` 按出口 id 写 `index`（原始编号）与 `pos`（本层位置）。
//!
//! 依据：B131、B133（地基/附注/2026-09-25-库层出口合成与待补批3裁定.md §一、§三）；预注册
//! `地基/过程记录/工程-步25-2b.md`。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, Outcome, run};
use jpp::{lower, syntax::parse};

/// 材料文字含「拿不准」的读 0.45（带内 → 未决）、含「不」的读 0.1（否定），其余 0.9（接受）
fn 定值端口() -> Ports<'static> {
    Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        let t = s.on_text();
        let p = if t.contains("拿不准") {
            0.45
        } else if t.contains("不") {
            0.1
        } else {
            0.9
        };
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }))
}

fn 跑(src: &str) -> Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    let mut l = Ledger::new();
    run(&program, 定值端口(), &calib, &ActionRegistry::new(), &mut l).expect("跑得完")
}

fn 出口(v: &Value, 路径: &[&str]) -> std::rc::Rc<jpp::value::Exit> {
    let mut cur = v.clone();
    for k in 路径 {
        cur = cur.get(k).unwrap_or_else(|| panic!("缺字段 {k}"));
    }
    match cur {
        Value::Exit(e) => e,
        other => panic!("{路径:?} 不是出口：{}", other.type_name()),
    }
}

const 三份材料: &str = r#"["可以", "不行", "拿不准"]"#;

#[test]
fn a_tally的聚合出口带parts且仍不放行() {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
tally(sieve({三份材料}, test("行吗", "k")))
"#
    );
    let out = 跑(&src);
    let v = out.value.expect("有返回值");
    for 路径 in [["value", "exists"], ["value", "all"]] {
        let e = 出口(&v, &路径);
        assert_eq!(e.parts.borrow().len(), 3, "{路径:?} 的分量是三个元素的出口");
        assert!(!e.releases(), "{路径:?}：合成出口到 25-9 前一律不放行");
    }
    // exists：有一个接受 → act；all：有一个否定 → ignore（与 25-2b 前相同）
    assert_eq!(出口(&v, &["value", "exists"]).label(), "act");
    assert_eq!(出口(&v, &["value", "all"]).label(), "ignore");
}

#[test]
fn b_first_k出口的parts是全部元素的出口() {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
first_k(sieve({三份材料}, test("行吗", "k")), 1)
"#
    );
    let out = 跑(&src);
    let v = out.value.expect("有返回值");
    let e = 出口(&v, &["value", "exit"]);
    // 第 0 个就凑够了 k = 1，但 parts 仍是全部三个元素的出口（B131 (2)）
    assert_eq!(e.label(), "act");
    assert_eq!(e.parts.borrow().len(), 3);
}

#[test]
fn c_链式过滤的报告行由element按出口写index与pos() {
    let src = format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
let r1 = sieve(["不行", "可以", "也可以"], test("行吗", "k"));
let r2 = sieve(r1, test("真的行吗", "k"));
{{a: map(r2.value, fn(e) {{ [e.index, e.pos] }}), p: r2.pending}}
"#
    );
    let out = 跑(&src);
    let rows = &out.exits;
    // 第一层三行：index = pos = 输入位置；第二层两行：index 沿用原始编号（1、2），pos 是本层位置（0、1）
    let 编号: Vec<(i64, i64)> = rows
        .iter()
        .map(|r| (r["index"].as_i64().unwrap(), r["pos"].as_i64().unwrap()))
        .collect();
    assert_eq!(编号, vec![(0, 0), (1, 1), (2, 2), (1, 0), (2, 1)]);
}
