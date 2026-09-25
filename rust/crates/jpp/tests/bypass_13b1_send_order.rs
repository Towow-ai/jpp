//! 步 13b-1：向量化连第 0 轮一起提前登记，刷新按程序顺序发出，预算停发（B93）落在末尾的轮次上。
//!
//! 13b 只提前登记第 1..n 轮，第 0 轮最后才由真站点登记，刷新按登记先后分组，于是发出顺序是
//! 第 1..n 轮、第 0 轮；预算紧时停发的是程序里最靠前的那一项。本文件钉住：调用数与层数不变，
//! 发出顺序是程序顺序，预算停发的是最后几项。
//!
//! 依据：B93（`12` §3 J-07「停」指停发）；`21` 步 13b、22-0；主会话 2026-09-25 步 13b-1 指示；
//! 预注册 `地基/过程记录/工程-步13b-1.md`。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, run};
use jpp::{lower, syntax::parse};

/// 是非题恒 0.9（过线 → Act）；记下每次调用看到的材料文本
fn 端口<'a>(见: &'a RefCell<Vec<String>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |s, qs| {
        见.borrow_mut().push(s.on_text());
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }))
}

/// 四份材料各一道题，`map` 体内 `cut`（13b：向量化把它们并成一层）；出口随返回值转交
fn 源(calls: u32) -> String {
    format!(
        r#"
budget {{calls: {calls}, cost: 0, depth: 16}};
let rs = map(["甲", "乙", "丙", "丁"], fn(t) {{
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    {{k: exit_kind(e), e: e}}
}});
{{v: map(rs, fn(r) {{ r.k }}), pending: map(rs, fn(r) {{ r.e }})}}
"#
    )
}

fn 跑(src: &str, 见: &RefCell<Vec<String>>, l: &mut Ledger) -> Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    run(&program, 端口(见), &calib, &ActionRegistry::new(), l)
        .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()))
}

#[test]
fn 预算足够时按程序顺序发出_调用数与层数不变() {
    let 见 = RefCell::new(vec![]);
    let o = 跑(&源(10), &见, &mut Ledger::new());
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["act", "act", "act", "act"])
    );
    assert_eq!(o.cost.calls, 4, "每份材料一次调用");
    assert_eq!(o.layers.len(), 1, "向量化把四轮并成一层");
    assert_eq!(
        *见.borrow(),
        vec!["甲", "乙", "丙", "丁"],
        "发出顺序即程序顺序"
    );
}

#[test]
fn 预算停发落在末尾() {
    let 见 = RefCell::new(vec![]);
    let o = 跑(&源(2), &见, &mut Ledger::new());
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["act", "act", "unsure(budget)", "unsure(budget)"]),
        "停发的是程序里最后两项"
    );
    assert_eq!(*见.borrow(), vec!["甲", "乙"]);
    assert_eq!(o.budget.as_ref().map(|b| b.unsent), Some(2));
}
