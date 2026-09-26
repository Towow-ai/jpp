//! 库 `unobserved` 覆盖缺席类原因（步 25-3）。
//!
//! 步 25-3 之前 `lib/outcome.jpp` 的 `unobserved(o)` 只认 `budget`，判断器缺席（`absent`）与超时（`latency`）
//! 的项落在 `undecided(o)` 里，与「判过而拿不准」混在一起；只交出 `undecided(o)` 的程序因此能把缺席项当成
//! 拿不准带出去。现在库谓词 `unobserved_cause` 与运行时 B95 的缺席类集合相同：缺席项进 `unobserved`，只交
//! `undecided` 的程序在缺席时报运行期 J-05（B81 (c)：`undecided` 与 `unobserved` 都交出才算转交）。
//!
//! 依据：B95；B81 (c)；`过程记录/总账待补条目.md` 第 82 行；预注册 `地基/过程记录/工程-步25-3.md`。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断器始终报错：缺席策略 conservative → `Unsure(absent)`
fn 缺席端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |_s, _qs| {
        Err(EffectError("连不上".into()))
    }))
}

/// 是非题恒 0.5：落在线带内 → `Unsure(band)`（判过而拿不准）
fn 带内端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

/// 读法函数与 `lib/outcome.jpp` 同源（库文本接在 budget 行后面，同 `bypass_b81_b82.rs`）
fn 程序(body: &str) -> String {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/outcome.jpp"),
    )
    .expect("读得到 lib/outcome.jpp");
    let (budget, rest) = body.split_once('\n').expect("第一行是 budget");
    format!("{budget}\n{lib}\n{rest}")
}

fn 跑(body: &str, ports: Ports<'_>) -> Result<Json, String> {
    let program =
        lower(&parse(&程序(body)).map_err(|e| format!("{e:?}"))?).map_err(|e| format!("{e:?}"))?;
    let mut calib = CalibStore::new();
    calib.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    let mut l = Ledger::new();
    run(&program, ports, &calib, &ActionRegistry::new(), &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

const 缺席预算: &str = r#"budget {calls: 4, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "conservative"}};"#;

/// 两个直接 `cut` 的判断出口放进契约值的未决清单。不用 `sieve`：缺席策略 conservative 下 `sieve` 把没有
/// 答案的读数记成 `Unsure(budget)`（步 22-0 过程记录问题清单里的已知旧问题），拿不到 `absent`。
const 两个未决: &str = r#"let e1 = cut(judge(state(mat("甲")), test("行吗", "k")));
let e2 = cut(judge(state(mat("乙")), test("行吗", "k")));
let o = outcome({value: [], pending: [e1, e2], detail: {ignore: []}});"#;

fn 原因(v: &Json) -> Vec<String> {
    v.as_array()
        .expect("列表")
        .iter()
        .map(|p| p["cause"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn a_缺席项进unobserved不进undecided() {
    let body = format!("{缺席预算}\n{两个未决}\n{{u: undecided(o), n: unobserved(o)}}");
    let v = 跑(&body, 缺席端口()).expect("两份都交出，跑得完");
    assert_eq!(原因(&v["u"]), Vec::<String>::new());
    assert_eq!(原因(&v["n"]), vec!["absent", "absent"]);
}

#[test]
fn b_缺席时只交undecided报运行期j05() {
    let body = format!("{缺席预算}\n{两个未决}\n{{review: undecided(o)}}");
    let e = 跑(&body, 缺席端口()).expect_err("缺席项没有交出");
    assert!(e.contains("J-05"), "{e}");
}

#[test]
fn c_判过而拿不准的仍在undecided() {
    let body = format!(
        "budget {{calls: 4, cost: 0, depth: 16}};\n{两个未决}\n{{u: undecided(o), n: unobserved(o)}}"
    );
    let v = 跑(&body, 带内端口()).expect("跑得完");
    assert_eq!(原因(&v["u"]), vec!["band", "band"]);
    assert_eq!(原因(&v["n"]), Vec::<String>::new());
}

#[test]
fn d_unobserved_cause与运行时缺席类集合相同() {
    let body = "budget {calls: 0, cost: 0, depth: 16};\nmap([\"budget\", \"absent\", \"latency\", \"band\", \"cold\"], unobserved_cause)";
    let v = 跑(body, 带内端口()).expect("纯计算");
    assert_eq!(v, json!([true, true, true, false, false]));
}
