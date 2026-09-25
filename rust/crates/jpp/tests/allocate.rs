//! 长处构件的**对照移植**验收：Rust 的 `allocate` / `unsure_bound` 必须与 Python 内核
//! 选出同一批下标、给出同一组界。
//!
//! 这两个构件在 Python 侧早已实现并有测试（`foundation/jv/runtime.py:1236` 与 `:1246`），
//! 所以 Rust 这边是移植不是重新设计，**Python 就是 oracle**。基准由
//! `tests/oracle/generate.py` 走真实的 `jv.judge` 路径跑出来，存在 `tests/oracle/oracle.json`；
//! 改了 Python 侧语义就重跑它，这里会跟着红。不要手改那个 json 里的数字。
//!
//! 用例故意挑会失败的形状：全部带内（分数全是 0，结果就是纯下标顺序）、并列（按下标升序）、
//! `k` 超长 / 为 0 / 空输入、记录自带 δ、分档题、冷记录（线不从记录来、从档案来）。

use std::rc::Rc;

use jpp::effects::CalibStore;
use jpp::strength::{allocate, unsure_bound};
use jpp::value::{Answer, Op, Reading};
use serde_json::Value as Json;

fn oracle() -> Json {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/oracle.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "读不到对照基准 {}：{e}（跑 tests/oracle/generate.py 生成）",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("对照基准是合法 JSON")
}

fn f(j: &Json, k: &str) -> f64 {
    j.get(k)
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| panic!("基准里缺 {k}"))
}

/// 按基准里记下的档案字段配一个校准库：这些数是**档案字段**，不是代码常数
fn store(case: &Json, profile: &Json) -> CalibStore {
    let mut calib = CalibStore::new();
    let s = profile["safety_lines"].as_array().expect("safety_lines");
    calib.profile.safety =
        jpp::effects::Field::known((s[0].as_f64().unwrap(), s[1].as_f64().unwrap()), "oracle");
    let d = &profile["delta"];
    calib.profile.delta =
        jpp::effects::Field::known((f(d, "noul"), f(d, "choice"), f(d, "score")), "oracle");
    // **这份夹具代表「档案加载过了」**：上面那两行填的是 oracle 里**测出来的**
    // 保守线与 δ，不是代码兜底。`hash` 为 `None` 的含义是「本次没有加载档案」，
    // 而这里显然加载了——不标上，`uncertainty` 会把这批测出来的线当成兜底而拒绝排序。
    calib.profile.hash = Some("oracle-fixture".into());

    let status = case["status"].as_str().unwrap();
    let n = if status == "上岗" { 100 } else { 0 };
    // 步 15d-2：δ 只从记录取。原来记录没写 δ 时用画像的 noul δ，现在把同一个值写进记录（Python 对照不变）
    calib
        .put(
            "k",
            f(case, "hi"),
            f(case, "lo"),
            n,
            status,
            Some(f(d, "noul")),
        )
        .expect("校准记录合法");
    if let Some(u) = case["unsure_rate"].as_f64() {
        calib.set_unsure_rate("k", u).expect("写得进 unsure_rate");
    }
    if let Some(delta) = case["delta"].as_f64() {
        calib.set_delta("k", delta).expect("写得进 delta");
    }
    calib
}

/// 按基准里的 p 造读数。分档题的概率向量与 generate.py 一致：第 0 档拿 p，其余平分。
fn readings(case: &Json) -> Vec<Rc<Reading>> {
    let phys = case["phys"].as_str().unwrap();
    let levels = case["scale"].as_array().map(|a| a.len()).unwrap_or(0);
    case["ps"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let p = p.as_f64().unwrap();
            let (op, answer) = match phys {
                "noul" => (Op::Test, Answer::Noul(p)),
                "score" => {
                    let rest = (1.0 - p) / (levels.max(2) - 1) as f64;
                    (
                        Op::Measure,
                        Answer::Score((0..levels).map(|l| if l == 0 { p } else { rest }).collect()),
                    )
                }
                other => panic!("基准里没预期的题式 {other}"),
            };
            let r = Rc::new(Reading {
                q_hash: format!("q{i}"),
                state_hash: format!("s{i}"),
                op,
                calib: "k".into(),
                id: 新句柄(),
                fail: None,
                model_id: "fixed-0".into(),
                ledger_key: format!("L{i}"),
                over_len: 0,
                scale: case["scale"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect(),
                perms: Default::default(),
                mode_share: Default::default(),
                missing_evidence: vec![],
                state_taint: Default::default(),
                form_hash: None,
                fp: None,
            });
            记答(&r, answer);
            r
        })
        .collect()
}
// 读数是句柄、答案在表里（步 11b-3）：测试自带一张答案表，经 `Answers` 交给 `strength`。
thread_local! {
    static 答: std::cell::RefCell<jpp::value::AnswerTable> = Default::default();
    static 号: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn 新句柄() -> u64 {
    号.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}
fn 记答(r: &Reading, a: Answer) {
    答.with(|t| t.borrow_mut().insert(r, a));
}
fn 答表() -> jpp::value::AnswerTable {
    答.with(|t| t.borrow().clone())
}

#[test]
fn 与python内核选出同一批下标() {
    let o = oracle();
    let profile = &o["profile"];
    let cases = o["cases"].as_array().expect("基准有用例");
    assert!(
        cases.len() >= 12,
        "基准至少该有十几条，实际 {}",
        cases.len()
    );
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let k = case["k"].as_u64().unwrap() as usize;
        let got = {
            let rs = readings(case);
            allocate(&store(case, profile), &答表(), &rs, k)
        };
        let want: Vec<usize> = case["picked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(
            got, want,
            "用例 {name}：Rust 选了 {got:?}，Python 选了 {want:?}"
        );
    }
}

#[test]
fn 与python内核给出同一组界() {
    let o = oracle();
    let profile = &o["profile"];
    for case in o["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = unsure_bound(&store(case, profile), &readings(case));
        let want = &case["bound"];
        assert_eq!(
            got.n as u64,
            want["n"].as_u64().unwrap(),
            "用例 {name} 的 n"
        );
        assert_eq!(
            got.n_unknown as u64,
            want["n_unknown"].as_u64().unwrap(),
            "用例 {name} 的 n_unknown"
        );
        // 两边都已经 round 到四位；比的是同一个数，不留 epsilon 的余地
        assert_eq!(
            got.union_bound,
            f(want, "union_bound"),
            "用例 {name} 的联合界"
        );
        assert_eq!(
            got.independent_any.as_reference_only(),
            f(want, "independent_any"),
            "用例 {name} 的独立估计（只作参考值）"
        );
    }
}

/// 冷记录（非上岗）的线**不从记录来**，从档案的保守线来：记录里写着 0.9/0.1 也不该被用上。
/// 这条在基准里是两个用例（`cold_record` 与 `cold_lines_ignored`），结果必须一样。
#[test]
fn 冷记录的线来自档案不来自记录() {
    let o = oracle();
    let cases = o["cases"].as_array().unwrap();
    let find = |n: &str| {
        cases
            .iter()
            .find(|c| c["name"] == n)
            .unwrap_or_else(|| panic!("基准里缺 {n}"))
    };
    let (a, b) = (find("cold_record"), find("cold_lines_ignored"));
    assert_ne!(
        a["hi"], b["hi"],
        "两个用例的记录线本来就该不同，否则这条测不出东西"
    );
    let pa = allocate(
        &store(a, &o["profile"]),
        &答表(),
        &readings(a),
        a["k"].as_u64().unwrap() as usize,
    );
    let pb = allocate(
        &store(b, &o["profile"]),
        &答表(),
        &readings(b),
        b["k"].as_u64().unwrap() as usize,
    );
    assert_eq!(pa, pb, "冷记录写什么线都一样：线只从档案的 safety_lines 来");
}
