//! B64（步 15f）：置换是 `select` 站点的测量声明 `{permute: true}`，写在题或题式上，默认不开。
//!
//! 走真机端口 `JevPorts` 加本地替身传输，跑完整程序：
//! (a) 未声明 → 只发一次，出口 `unsure(untested)`，J-15 那一位是 `permutation`，告警给作者可改的修法；
//! (b) 声明（题上、题式上）→ 正逆两序各发一次，账本判断条目记 `perms = 2`，两序选同一候选时出 `pick`；
//! (c) 声明且判断器按位置选（两序选到不同候选）→ `unsure(tie)`；
//! (d) 声明位置与取值的错：`test` 上声明、`permute` 不是布尔，都报 `E-rt-question`。
//! 依据：B64（地基/附注/2026-09-24-探针首轮裁定.md，I-1(b)）；`12` §2.3 `Pick` 行、§2.11。

use std::cell::Cell;

use jpp::effects::{CalibStore, JevClient, JevPorts};
use jpp::ledger::{Entry, Ledger};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 稳定的判断器：不管候选怎么排，总把 0.9 给内容是「甲」的那一个。
fn 稳定(body: &Json) -> Result<Json, jpp::effects::EffectError> {
    let crit = body["questions"]["q0"]["criteria"]
        .as_object()
        .expect("有候选表");
    let mut probs = serde_json::Map::new();
    for (k, v) in crit {
        probs.insert(k.clone(), json!(if v == "甲" { 0.9 } else { 0.05 }));
    }
    Ok(json!({"answers": {"q0": {"probabilities": probs}}}))
}

/// 按位置选的判断器：不管内容，总把 0.9 给第一个位置（首位偏置）。
fn 按位置(body: &Json) -> Result<Json, jpp::effects::EffectError> {
    let crit = body["questions"]["q0"]["criteria"]
        .as_object()
        .expect("有候选表");
    let mut probs = serde_json::Map::new();
    for k in crit.keys() {
        probs.insert(k.clone(), json!(if k == "c0" { 0.9 } else { 0.05 }));
    }
    Ok(json!({"answers": {"q0": {"probabilities": probs}}}))
}

struct 结果 {
    出口: String,
    告警: Vec<String>,
    请求: u64,
    perms: Vec<usize>,
}

fn 跑(题: &str, 判断器: fn(&Json) -> Result<Json, jpp::effects::EffectError>) -> 结果 {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
let s = state(mat("对象"), {{over: [mat("甲"), mat("乙"), mat("丙")]}});
let e = cut(judge(s, {题}));
handle(e, {{
  pick: fn(k) {{ exit_kind(e) }},
  unsure: fn(u) {{ consume(u, "drop"); exit_kind(e) }}
}})
"#
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let n = std::rc::Rc::new(Cell::new(0u64));
    let n2 = n.clone();
    let mut jp = JevPorts::new(JevClient::with_transport(
        "jev-1.13.0",
        Box::new(move |b: &Json| {
            n2.set(n2.get() + 1);
            判断器(b)
        }),
    ));
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        jp.ports(),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let perms = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { perm: Some(p), .. } => Some(p.perms),
            _ => None,
        })
        .collect();
    结果 {
        出口: out.value_json().as_str().unwrap_or("").to_string(),
        告警: out.trace.warnings.clone(),
        请求: n.get(),
        perms,
    }
}

/// (a) 未声明：行为与现状相同，只发一次，`untested:permutation`，修法告诉作者怎么声明
#[test]
fn 未声明_未测且告诉作者怎么开() {
    let r = 跑(r#"select("挑一个", "k")"#, 稳定);
    assert_eq!(r.请求, 1, "默认不开置换：不替作者花双倍");
    assert!(r.出口.starts_with("unsure"), "{}", r.出口);
    assert!(r.perms.is_empty(), "没测置换，账本不记 perms");
    let w = r
        .告警
        .iter()
        .find(|w| w.starts_with("W-untested"))
        .unwrap_or_else(|| panic!("要留 W-untested：{:?}", r.告警));
    assert!(w.contains("permutation"), "{w}");
    assert!(
        w.contains("{permute: true}") && w.contains("作者可改"),
        "修法要是作者能改的写法：{w}"
    );
}

/// (b) 题上声明：正逆两序，账本记 perms = 2，两序一致 → pick
#[test]
fn 题上声明_两序一致给pick() {
    let r = 跑(r#"select("挑一个", "k", {permute: true})"#, 稳定);
    assert_eq!(r.请求, 2, "正逆两序各发一次（K = 2）");
    assert_eq!(
        r.perms,
        vec![2],
        "账本判断条目成对记 perms/mode_share（K-131）"
    );
    assert_eq!(r.出口, "pick(0)", "甲在原序第 0 位");
}

/// (b) 题式上声明：`fill` 把声明带到题上，效果与题上声明相同
#[test]
fn 题式上声明_由fill带到题上() {
    let r = 跑(
        r#"fill(form("select", "挑一个{x}", {calib: "k", permute: true}), {x: "吧"})"#,
        稳定,
    );
    assert_eq!(r.请求, 2);
    assert_eq!(r.perms, vec![2]);
    assert_eq!(r.出口, "pick(0)");
}

/// (c) 声明了、判断器按位置选：正序选甲、逆序选丙，众数占比 0.5 → tie（测了，不一致）
#[test]
fn 置换不一致_给tie() {
    let r = 跑(r#"select("挑一个", "k", {permute: true})"#, 按位置);
    assert_eq!(r.请求, 2);
    assert_eq!(r.perms, vec![2]);
    assert!(r.出口.contains("tie"), "测了、两序不一致是 tie：{}", r.出口);
}

fn 报错(题: &str) -> String {
    let src = format!("budget {{calls: 4, cost: 1, depth: 8}};\nlet q = {题};\n1\n");
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut jp = JevPorts::new(JevClient::with_transport("jev-1.13.0", Box::new(稳定)));
    run(
        &program,
        jp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .map(|_| String::new())
    .unwrap_or_else(|e| e.render())
}

/// (d) 声明位置与取值：只有 select 能声明；值要是布尔
#[test]
fn 声明位置与取值要对() {
    let e = 报错(r#"test("行吗", "k", {permute: true})"#);
    assert!(e.contains("E-rt-question") && e.contains("select"), "{e}");
    let e = 报错(r#"form("test", "行吗{x}", {calib: "k", permute: true})"#);
    assert!(e.contains("E-rt-question") && e.contains("select"), "{e}");
    let e = 报错(r#"select("挑一个", "k", {permute: "是"})"#);
    assert!(e.contains("E-rt-question") && e.contains("布尔"), "{e}");
}
