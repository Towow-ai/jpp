//! `JevClient`（唯一的真模型客户端）的测试。**它此前在整棵树里零覆盖。**
//!
//! 这份存在的理由不是「补一个遗漏的测试」，是一条通则：
//!
//! > **凡是真客户端要填、替身不填的字段，纪律就会在替身上成立、在真机上失效，而且是静默的。**
//!
//! `mode_share` 就是第一个实例：`12`:151 要求 `select` 的 `Pick` 置换众数一致，
//! `cut` 那半落地了而且往拒绝倒（`unwrap_or(false)`）——**但 `JevClient` 硬写
//! `mode_share: vec![]`**，于是用真模型跑，**每一道 `select` 都切成 `Unsure(tie)`**，
//! 而全绿的测试一盏灯都不会亮，因为绿灯全是替身给的。
//!
//! 用 `with_transport` 注入假 transport：不发网络、$0，但走的是**真客户端的全部代码路径**。

use std::cell::RefCell;
use std::rc::Rc;

use jpp::effects::JevClient;
use jpp::value::{Answer, Mat, Op, Question, State};
use serde_json::{Value as Json, json};

fn 三候选状态() -> Rc<State> {
    Rc::new(State::new(
        vec![Mat::literal(json!("对象"))],
        vec![],
        vec![],
        vec![
            Mat::literal(json!("甲")),
            Mat::literal(json!("乙")),
            Mat::literal(json!("丙")),
        ],
        false,
    ))
}

/// **`select` 要真的发置换，并按众数占比算 `mode_share`**（与 Python `runtime.py:949` 的
/// `perms = [正序, 逆序]` 和 `:1069` 的 `cnt / len(picks)` 同口径）。
#[test]
fn select要发正逆两个置换并算众数占比() {
    let 发过的: Rc<RefCell<Vec<Json>>> = Rc::new(RefCell::new(vec![]));
    let 记 = 发过的.clone();
    // **一个「稳定」的模型**：不管候选怎么排，它总选内容是 "甲" 的那一个。
    // 所以要看这次发出去的 criteria 里 "甲" 排在第几位，把 0.8 给那一位——
    // 这正是置换检验要测的东西：**换个顺序模型还选同一个吗**。
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(move |body: &Json| {
            记.borrow_mut().push(body.clone());
            let crit = body["questions"]["q0"]["criteria"]
                .as_object()
                .expect("有候选表");
            let 甲在第几位 = crit
                .iter()
                .find(|(_, v)| v.as_str() == Some("甲"))
                .map(|(k, _)| k.clone())
                .expect("候选里有甲");
            let mut probs = serde_json::Map::new();
            for k in crit.keys() {
                probs.insert(k.clone(), json!(if *k == 甲在第几位 { 0.8 } else { 0.1 }));
            }
            Ok(json!({"answers": {"q0": {"probabilities": probs}}}))
        }),
    );
    // **默认不发置换**（调用数 ×2 的钱不替作者花），要测它就显式开
    c.permute = true;
    let s = 三候选状态();
    let q = Question::new(Op::Select, "挑一个", "k", vec![]);
    let r = c.judge(&s, &[&q]).expect("跑得通");

    assert_eq!(
        发过的.borrow().len(),
        2,
        "select 要发**两个置换**（正序、逆序），实际发了 {} 次",
        发过的.borrow().len()
    );
    assert_eq!(r.mode_share.len(), 1, "一道题一个 mode_share");
    assert_eq!(
        r.mode_share[0],
        Some(1.0),
        "两次都选同一项：众数占比 2/2 = 1.0。实际 {:?}",
        r.mode_share[0]
    );

    // 两次请求的候选顺序要真的不同——否则「置换」是假的
    let 发过的 = 发过的.borrow();
    let 取候选 = |b: &Json| -> Vec<String> {
        b["questions"]["q0"]["criteria"]
            .as_object()
            .map(|m| m.values().map(|v| v.to_string()).collect())
            .unwrap_or_default()
    };
    assert_ne!(
        取候选(&发过的[0]),
        取候选(&发过的[1]),
        "两次的候选顺序必须不同，否则不是置换"
    );
}

/// **置换不一致时 `mode_share < 1.0`** —— 那正是 `cut` 用来不给 `Pick` 的依据。
#[test]
fn 两次选了不同候选时众数占比小于一() {
    let 第几次 = Rc::new(RefCell::new(0));
    let n = 第几次.clone();
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(move |_b: &Json| {
            let mut i = n.borrow_mut();
            *i += 1;
            // 第一次选 c0，第二次选 c1 —— 换个排列就选了别的
            Ok(if *i == 1 {
                json!({"answers": {"q0": {"probabilities": {"c0": 0.8, "c1": 0.1, "c2": 0.1}}}})
            } else {
                json!({"answers": {"q0": {"probabilities": {"c0": 0.1, "c1": 0.8, "c2": 0.1}}}})
            })
        }),
    );
    // **默认不发置换**（调用数 ×2 的钱不替作者花），要测它就显式开
    c.permute = true;
    let r = c
        .judge(
            &三候选状态(),
            &[&Question::new(Op::Select, "挑一个", "k", vec![])],
        )
        .expect("跑得通");
    assert_eq!(
        r.mode_share[0],
        Some(0.5),
        "两次选了不同候选：众数占比 1/2 = 0.5。实际 {:?}",
        r.mode_share[0]
    );
}

/// **非 select 不发置换**：noul / score 各发一次，`mode_share` 是 `None`（不适用，不是 0）。
#[test]
fn 非select不发置换() {
    let 次数 = Rc::new(RefCell::new(0));
    let n = 次数.clone();
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(move |_b: &Json| {
            *n.borrow_mut() += 1;
            Ok(json!({"answers": {"q0": {"noul": 0.9}}}))
        }),
    );
    // **默认不发置换**（调用数 ×2 的钱不替作者花），要测它就显式开
    c.permute = true;
    let s = Rc::new(State::new(
        vec![Mat::literal(json!("对象"))],
        vec![],
        vec![],
        vec![],
        false,
    ));
    let r = c
        .judge(&s, &[&Question::new(Op::Test, "行吗", "k", vec![])])
        .expect("跑得通");
    assert_eq!(*次数.borrow(), 1, "noul 只发一次");
    assert_eq!(
        r.mode_share[0], None,
        "noul 上没有「置换」这回事，是 None 不是 0"
    );
    assert!(matches!(r.answers[0], Answer::Noul(p) if (p - 0.9).abs() < 1e-9));
}

/// 费用与 token 照记（`13` §5：后端返回即记事实）。
#[test]
fn 费用与token照记() {
    let mut c = JevClient::with_transport(
        "jev-1.13.0",
        Box::new(|_b: &Json| {
            Ok(json!({"answers": {"q0": {"noul": 0.5}}, "usage": {"input_tokens": 1000}}))
        }),
    );
    // **默认不发置换**（调用数 ×2 的钱不替作者花），要测它就显式开
    c.permute = true;
    let s = Rc::new(State::new(
        vec![Mat::literal(json!("x"))],
        vec![],
        vec![],
        vec![],
        false,
    ));
    // 价格只住画像（B73，步 15d-0）：没给价格时不折钱，由宿主报 `W-cost-unknown`
    let r = c
        .judge(&s, &[&Question::new(Op::Test, "行吗", "k", vec![])])
        .expect("跑得通");
    assert_eq!(r.tokens, 1000);
    assert_eq!(r.cost, 0.0, "没有画像价格就没有代码里的兜底价格");
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    let price = jpp::effects::Profile::load(&p)
        .expect("发行画像")
        .price_per_input_token();
    let mut c = c.with_price(price);
    let r = c
        .judge(&s, &[&Question::new(Op::Test, "行吗", "k", vec![])])
        .expect("跑得通");
    assert_eq!(
        r.cost,
        1000.0 * price.expect("发行画像带价格"),
        "token 按画像价格折成钱"
    );
}

/// **跨内核对照：Rust 路径上跑出真的置换一致率，与 Python 那 8 条对得上。**
///
/// 这是总控改过的验收判据——不是「`mode_share` 不再是 `vec![]`」，是**那个量在 Rust 上
/// 第一次变得可测量，且与 oracle 一致**。
///
/// 为什么此前只能从 Python 捞：`JevClient` 从来没发过置换，**Rust 侧根本不产生这个量**。
/// 那个数被写错四次，根因之一就是这个。
///
/// 数据是 E-CAL 正式版里走**原生 choice** 路径的 8 条（K-noul 那 89 条的 `mode_share`
/// 是写死的 1.0，不是测量）。每条各跑正序与逆序两个置换，两次都选同一项 → 1.0。
#[test]
fn 置换一致率与python的八条对得上() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../foundation/experiments/raw/e_cal/readings.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("跳过：读不到 {}（发布目录不带实验数据）", path.display());
        return;
    };

    // 原生 choice 路径那 8 条：phys == "choice"
    let 原生: Vec<Json> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .filter(|d| d["type"] == "choice" && d["ans"]["phys"] == "choice")
        .collect();
    assert_eq!(原生.len(), 8, "E-CAL 里走原生 choice 的应当是 8 条");

    let mut 一致数 = 0;
    for d in &原生 {
        let probs = d["ans"]["probs"].as_object().expect("有概率表");
        let k = probs.len();
        assert!(k > 1, "候选要多于一个才谈得上置换");
        // 用那条读数的概率造一个「稳定的模型」：不管候选怎么排，它总选同一个内容
        let 原始最大 = probs
            .iter()
            .max_by(|a, b| {
                a.1.as_f64()
                    .unwrap_or(0.0)
                    .partial_cmp(&b.1.as_f64().unwrap_or(0.0))
                    .unwrap()
            })
            .map(|(kk, _)| kk.clone())
            .unwrap();
        let 赢家内容 = format!("候选{原始最大}");
        let mut c = JevClient::with_transport(
            "jev-1.13.0",
            Box::new(move |body: &Json| {
                let crit = body["questions"]["q0"]["criteria"]
                    .as_object()
                    .expect("有候选表");
                let 赢家位 = crit
                    .iter()
                    .find(|(_, v)| v.as_str() == Some(赢家内容.as_str()))
                    .map(|(kk, _)| kk.clone())
                    .expect("赢家在候选里");
                let mut out = serde_json::Map::new();
                for kk in crit.keys() {
                    out.insert(
                        kk.clone(),
                        json!(if *kk == 赢家位 {
                            0.9
                        } else {
                            0.1 / (crit.len() - 1) as f64
                        }),
                    );
                }
                Ok(json!({"answers": {"q0": {"probabilities": out}}}))
            }),
        );
        // **默认不发置换**（调用数 ×2 的钱不替作者花），要测它就显式开
        c.permute = true;
        let over: Vec<Mat> = (0..k)
            .map(|i| Mat::literal(json!(format!("候选{i}"))))
            .collect();
        let s = Rc::new(State::new(
            vec![Mat::literal(json!("对象"))],
            vec![],
            vec![],
            over,
            false,
        ));
        let r = c
            .judge(
                &s,
                &[&Question::new(Op::Select, "挑一个", "e_cal.choice", vec![])],
            )
            .expect("跑得通");
        assert_eq!(r.mode_share.len(), 1);
        if r.mode_share[0] == Some(1.0) {
            一致数 += 1;
        }
    }

    // Python 那 8 条的 mode_share 全是 1.0（各跑两个置换、两次都选同一项）
    let python一致数 = 原生
        .iter()
        .filter(|d| d["ans"]["mode_share"].as_f64() == Some(1.0))
        .count();
    assert_eq!(python一致数, 8, "Python 侧那 8 条应当全是 1.0");
    assert_eq!(
        一致数, python一致数,
        "Rust 路径上的置换一致率要与 Python 那 8 条对得上：Rust {一致数}/8，Python {python一致数}/8"
    );
    println!("跨内核置换一致率：Rust {一致数}/8，Python {python一致数}/8 —— 对得上");
    println!("（n=8 功效极低：若真实一致率是判据里的 0.75，8/8 出现的概率是 0.10）");
}
