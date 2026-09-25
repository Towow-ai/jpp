//! **差分网格法推广到其余纯函数。**
//!
//! 方法来自上一包：`cut` 的断言是照着实现写的，**而 Rust 的其余纯函数也全都是**
//! ——**同一个盲区覆盖它们全部**。上一包第一次用就在最核心的判定上照出一条失败开放
//! （9/101 格分岔），所以推广开。
//!
//! # 两边都有的纯函数清单（先列，再挑）
//!
//! | 函数 | Python 有 | 按「错了会不会失败开放」排 |
//! |---|---|---|
//! | `cut` 的过线判定 | ✓ `_decide` | **最高**——错了直接多给强出口。**上一包做了，照出 δ 带缺失** |
//! | `unsure_bound` | ✓ `runtime.py` | **高**——它是 J-10 的**上界**，报小了就是「预算够」的假放行 |
//! | `uncertainty` | ✓ `_uncertainty` | 中——喂 `allocate`，错了是复核名额分错，不直接放行 |
//! | `cost_line` | ✓ `calib.py` | 高——**它就是线本身**。**已在 `cost_line.rs` 逐值对过 oracle（0.590/0.780/0.385）** |
//! | `agg` / `order` | ✓ `ir.py:481/566` | 中——**本轮未做**，列在这里 |
//! | `binomial_upper` / `certify` / `drift` / `n_needed_zero_error` | **✗ 无对照** | **Python 没有**。它们只有单侧实现，**差分法用不上**——这本身是清单的一部分 |
//!
//! # 覆盖理由（上一包没说，这次说）
//!
//! - **`unsure_bound`**：0 条 / 1 条 / 多条；全已知 / 全未知 / 混合；`u = 0`（下界）、
//!   `u = 1`（上界）、以及 **Σu 会超过 n 的组合**（夹取那一步必须被走到）。
//! - **`uncertainty`**：五组线覆盖 **δ=0**（退化）、**δ>0**、**hi=lo**（零宽带）、
//!   **hi=1/lo=0**（兜底线，整段都在带内）；每组扫 21 个 `p`，含两端 0 与 1。
//!   **挑的是「会让分支边界翻面」的取值，不是「多跑几格」。**

use jpp::effects::CalibStore;
use jpp::strength::{uncertainty, unsure_bound};
use jpp::value::{Answer, Op, Reading};
use serde_json::Value as Json;
use std::rc::Rc;

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

fn 读数(calib: &str, p: f64) -> Rc<Reading> {
    let r = Reading {
        q_hash: "q".into(),
        state_hash: "s".into(),
        op: Op::Test,
        calib: calib.into(),
        id: 新句柄(),
        fail: None,
        model_id: "m".into(),
        ledger_key: "lk".into(),
        over_len: 0,
        scale: vec![],
        perms: Default::default(),
        mode_share: Default::default(),
        missing_evidence: vec![],
        state_taint: Default::default(),
        form_hash: None,
        fp: None,
    };
    记答(&r, Answer::Noul(p));
    Rc::new(r)
}

#[test]
fn unsure_bound逐例与python相同() {
    let gold: Json = serde_json::from_str(include_str!("py_pure.json")).expect("金标");
    let mut 分岔 = vec![];
    for c in gold["unsure_bound"].as_array().expect("用例") {
        let us: Vec<Option<f64>> = c["us"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                if v.is_null() {
                    None
                } else {
                    Some(v.as_f64().unwrap())
                }
            })
            .collect();
        let mut store = CalibStore::new();
        let mut rs = vec![];
        for (i, u) in us.iter().enumerate() {
            let k = format!("k{i}");
            match u {
                // **有 `unsure_rate` 的键必须是「上岗」**——两边都只采信上岗记录的那个值
                Some(v) => {
                    store.put(&k, 0.8, 0.2, 10, "上岗", Some(0.05)).unwrap();
                    store.set_unsure_rate(&k, *v).unwrap();
                }
                None => {
                    store.put(&k, 0.8, 0.2, 10, "上岗", Some(0.05)).unwrap();
                }
            }
            rs.push(读数(&k, 0.5));
        }
        let b = unsure_bound(&store, &rs);
        let e = &c["out"];
        let 我 = (
            b.n as u64,
            (b.union_bound * 10000.0).round(),
            b.n_unknown as u64,
        );
        let 它 = (
            e["n"].as_u64().unwrap(),
            (e["union_bound"].as_f64().unwrap() * 10000.0).round(),
            e["n_unknown"].as_u64().unwrap(),
        );
        if 我 != 它 {
            分岔.push(format!("us={:?}: Rust={:?} Python={:?}", c["us"], 我, 它));
        }
    }
    assert!(
        分岔.is_empty(),
        "**{} 例分岔**：\n  {}",
        分岔.len(),
        分岔.join("\n  ")
    );
    println!("unsure_bound：14 例逐例相同 ✓");
}

#[test]
fn uncertainty在上岗档上与python相同() {
    let gold: Json = serde_json::from_str(include_str!("py_pure.json")).expect("金标");
    let mut 分岔 = vec![];
    for g in gold["uncertainty"].as_array().expect("格") {
        let (p, hi, lo, delta) = (
            g["p"].as_f64().unwrap(),
            g["hi"].as_f64().unwrap(),
            g["lo"].as_f64().unwrap(),
            g["delta"].as_f64().unwrap(),
        );
        let mut store = CalibStore::new();
        // 步 15d-2：δ 只从记录取（原写画像的 δ）
        store.put("k", hi, lo, 10, "上岗", Some(delta)).unwrap();
        let 我 = {
            let r = 读数("k", p);
            uncertainty(&store, &答表(), &r)
        }
        .expect("上岗档一定算得出");
        let 它 = g["u"].as_f64().unwrap();
        if (我 - 它).abs() > 1e-9 {
            分岔.push(format!(
                "p={p} hi={hi} lo={lo} δ={delta}: Rust={我} Python={它}"
            ));
        }
    }
    assert!(
        分岔.is_empty(),
        "**{} 格分岔**：\n  {}",
        分岔.len(),
        分岔.join("\n  ")
    );
    println!("uncertainty（上岗档）：105 格逐格相同 ✓");
}

/// **一处有意分岔，钉住免得被当成 bug 再修一次。**
///
/// Python 的 `_uncertainty` 对冷键用档案保守线、**永远返回一个数**；
/// **Rust 在「记录不上岗且档案没加载」时返回 `None`**——那是 `allocate` 那一包有意改的：
/// **代码兜底线 `hi=1/lo=0` 让每个 `p` 都落在带内、全得 `0.0`，
/// 降序并列按下标 → `allocate` 退化成「按文件顺序取前 k 个」，零告警。**
///
/// **所以这条分岔的方向是「Rust 更保守」，而且是总控裁过的。**
#[test]
fn 冷键上的分岔是有意的() {
    let 空档 = CalibStore::new(); // 没加载档案
    assert_eq!(
        {
            let r = 读数("冷", 0.5);
            uncertainty(&空档, &答表(), &r)
        },
        None,
        "Rust：算不出"
    );
    // Python 在同样输入下会给 0.0（带 = 整个 [0,1]）——**那个 0.0 恰好排在最前面**
    let mut 有档 = CalibStore::new();
    有档.profile.hash = Some("装过了".into());
    有档.profile.safety = jpp::effects::Field::known((0.75, 0.25), "测试");
    有档.profile.delta = jpp::effects::Field::known((0.04, 0.04, 0.04), "测试");
    // 步 15d-2：冷键没有记录，也就没有 δ（δ 只从记录取，画像的 δ 只作认证先验）——装了画像也算不出，
    // 分岔从「没装画像」扩到「冷键」，方向仍是 Rust 更保守（原断言 is_some，按预注册改）
    assert!(
        {
            let r = 读数("冷", 0.95);
            uncertainty(&有档, &答表(), &r)
        }
        .is_none(),
        "冷键没有 δ，算不出"
    );
}
