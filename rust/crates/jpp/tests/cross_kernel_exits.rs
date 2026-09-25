//! **跨内核出口对照：给定同一批读数，两个内核给不给同一个出口。**
//!
//! **$0，而且比端到端真机更强**——用的正是那条规律：
//! **「一个可以用纯函数比的东西，不要用端到端跑来比。」**
//! 出口不是纯函数（它依赖模型读数），**但「给定读数 → 出口」是纯函数**。
//! 所以扫一整条 `p` 网格，**比一次真机采样彻底得多**，而且没有抖动要排除。
//!
//! **金标向量 `py_exits.json` 是 Python `runtime.py::_decide` 的 `test` 臂当场跑出来的**，
//! 签进仓库，**两边都读它**——一句注释跨不了两种语言，一个向量文件可以。
//!
//! # 它照出来的东西
//!
//! **Rust 的 `cut` 整条 δ 带缺失。** `12`:167 写着「再过线，再 `band`（**线附近 ±δ**）」，
//! 而 Rust 原来是裸的 `p >= hi` / `p <= lo`——**线的邻域里本该 `Unsure` 的读数拿到了强出口**。
//! **那是失败开放，而且是在 `cut` 这个最核心的判定上。**
//!
//! **E-JPP-LIVE 那次真机读数 `p = 0.56`、`lo = 0.56`、`δ = 0.04`**：
//! **修前 Rust 给 `Ignore`，Python 给 `Unsure(band)`——那一条实测数据本身就分岔。**

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 步 15c：原 `impl Client for 定值`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
/// 改为一个 judge 闭包端口，恒答 `p`。
fn 定值端口<'a>(p: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge(
        "jev-1.13.0",
        move |_s: &State, qs: &[&Question]| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        },
    ))
}

fn rust出口(p: f64, hi: f64, lo: f64, delta: f64) -> String {
    let program = lower(
        &parse(
            r#"
budget {calls: 2, cost: 0};
handle(cut(judge(state(mat("材料")), test("行吗", "k"))), {
    act: fn() { "act" }, ignore: fn() { "ignore" },
    unsure: fn(u) { consume(u, "drop"); unsure_cause(u) }})
"#,
        )
        .expect("解析"),
    )
    .expect("lower");
    let mut c = CalibStore::new();
    // 步 15d-2：δ 只从记录取（原写画像的 δ）
    c.put("k", hi, lo, 73, "上岗", Some(delta)).unwrap();
    let mut l = Ledger::new();
    let o = jpp::run(&program, 定值端口(p), &c, &ActionRegistry::new(), &mut l).expect("跑得完");
    o.value_json().as_str().expect("是文本").to_string()
}

/// **逐格对照 101 个 `p`**。
#[test]
fn 两内核对同一读数给同一出口() {
    let gold: Json = serde_json::from_str(include_str!("py_exits.json")).expect("金标向量");
    let (hi, lo, delta) = (
        gold["hi"].as_f64().unwrap(),
        gold["lo"].as_f64().unwrap(),
        gold["delta"].as_f64().unwrap(),
    );
    let mut 分岔 = vec![];
    for (k, v) in gold["exits"].as_object().expect("exits") {
        let p: f64 = k.parse().expect("p");
        let 我 = rust出口(p, hi, lo, delta);
        let 它 = v.as_str().expect("出口");
        if 我 != 它 {
            分岔.push(format!("p={k}: Rust={我} Python={它}"));
        }
    }
    分岔.sort();
    assert!(
        分岔.is_empty(),
        "**{} 格分岔**（这正是跨内核对照要找的东西）：\n  {}",
        分岔.len(),
        分岔.join("\n  ")
    );
    println!("101 格逐格相同（hi={hi} lo={lo} δ={delta}）——两内核对同一读数给同一出口 ✓");
}

/// **把 E-JPP-LIVE 那条实测读数单独钉住**：它恰好落在修前会分岔的那一格。
#[test]
fn 真机那条读数在修前是分岔的() {
    // hi=0.66 lo=0.56 δ=0.04，真机 p=0.56
    assert_eq!(
        rust出口(0.56, 0.66, 0.56, 0.04),
        "band",
        "**修前 Rust 给 ignore（p <= lo），Python 给 band（p > lo - δ）**"
    );
    // δ=0 时才回到修前那种裸比较
    assert_eq!(
        rust出口(0.56, 0.66, 0.56, 0.0),
        "ignore",
        "δ=0 时两种写法才一致"
    );
}
