//! 跳数登记（步 17a，B59）：`Judge.parents`/`hop` 按结构通道计。
//!
//! 依据：`20` v2 附录 B59、§3.1 边界表；`21` §四·6 步 17a；主会话 2026-09-24 裁定 C（只走结构通道，
//! 值级来源另开，候选 B84）。预注册：`地基/过程记录/工程-步17a.md` §一。
//! 结构通道：`sieve`/`pair` 元素记录的出口、出口转材料（`mat(exit)`）、元素记录转材料（`mat(e)`）、
//! 效应输出承接输入材料的来源。经普通值（`e.item` 取出的文本）的依赖不计——下界。

use std::collections::BTreeSet;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger, depth_profile};
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::json;

/// 判断恒给 0.9，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
fn 定值端口() -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k1", "k2"] {
        c.put(k, 0.6, 0.3, 50, "上岗", Some(0.05)).expect("写得进");
    }
    c
}

fn 跑(src: &str, ledger: &mut Ledger) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    run(&program, 定值端口(), &库(), &ActionRegistry::new(), ledger).expect("跑得完")
}

/// 每条判断条目：（题面前缀可辨的站点无关）→ (hop, parents)
fn 条目(l: &Ledger) -> Vec<(String, u32, Vec<String>)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key, hop, parents, ..
            } => Some((key.clone(), *hop, parents.clone())),
            _ => None,
        })
        .collect()
}

const 两层: &str = r#"
budget {calls: 10, cost: 1, depth: 64};
let q1 = test("第一问？", "k1");
let q2 = test("第二问？", "k2");
let first = sieve(["甲", "乙"], q1);
let second = sieve(first, q2);
second.kind
"#;

#[test]
fn 契约值再过滤_第二层两跳_父为同一元素的第一层条目() {
    let mut l = Ledger::new();
    跑(两层, &mut l);
    let es = 条目(&l);
    assert_eq!(es.len(), 4);
    let 第一层: Vec<&String> = es.iter().filter(|e| e.1 == 1).map(|e| &e.0).collect();
    let 第二层: Vec<_> = es.iter().filter(|e| e.1 == 2).collect();
    assert_eq!(第一层.len(), 2);
    assert_eq!(第二层.len(), 2);
    for (_, _, ps) in &第二层 {
        assert_eq!(ps.len(), 1);
        assert!(第一层.contains(&&ps[0]), "父键是第一层的条目");
    }
    // 两个第二层条目的父各不相同（各自元素的出口）
    assert_ne!(第二层[0].2, 第二层[1].2);
    assert_eq!(depth_profile(&l), vec![(1, 2, 0), (2, 2, 0)]);
}

#[test]
fn 配对元素经过滤_承接左右元素的出口() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q1 = test("第一问？", "k1");
let q2 = test("a 与 b 相配吗？", "k2");
let people = sieve(["甲"], q1);
let rel = sieve(pair(["需求"], people), q2);
rel.kind
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    let es = 条目(&l);
    let 一 = es.iter().find(|e| e.1 == 1).expect("第一层");
    let 二 = es.iter().find(|e| e.1 == 2).expect("配对后的一层是两跳");
    assert_eq!(
        二.2,
        vec![一.0.clone()],
        "左侧是普通文本，只有右侧元素的出口"
    );
}

#[test]
fn 出口转材料与变换输出_都算一跳依赖() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q1 = test("第一问？", "k1");
let q2 = test("第二问？", "k2");
let q3 = test("第三问？", "k2");
let e = cut(judge(state(mat("材料")), q1));
let e2 = cut(judge(state(mat(e)), q2));
let m = transform(fn(x) { {wrapped: content(x)} }, mat(e2));
let e3 = cut(judge(state(m), q3));
exit_kind(e3)
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    let mut hops: Vec<u32> = 条目(&l).iter().map(|e| e.1).collect();
    hops.sort();
    assert_eq!(
        hops,
        vec![1, 2, 3],
        "mat(exit) 一跳，transform 承接来源再一跳"
    );
}

#[test]
fn 经普通值取出的依赖_17c起计入() {
    // 17a 时这里锁的是结构通道下界（hop 1、parents 空）；步 17c（B84 值级来源）起 `e.item` 取出的文本
    // 带着选中它的出口键，第二层 hop 2、parents 为第一层对应键（预注册 `过程记录/工程-步17c.md` 第 5 条）
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q1 = test("第一问？", "k1");
let q2 = test("第二问？", "k2");
let first = sieve(["甲", "乙"], q1);
let second = sieve(map(first.value, fn(e) { e.item }), q2);
second.kind
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    let es = 条目(&l);
    let 第一层: Vec<&String> = es.iter().filter(|e| e.1 == 1).map(|e| &e.0).collect();
    let 第二层: Vec<_> = es.iter().filter(|e| e.1 == 2).collect();
    assert_eq!((第一层.len(), 第二层.len()), (2, 2));
    for (_, _, ps) in &第二层 {
        assert_eq!(ps.len(), 1);
        assert!(第一层.contains(&&ps[0]));
    }
}

#[test]
fn 来源不进材料与状态哈希() {
    let a = Mat::literal(json!("x"));
    let b = Mat::literal(json!("x")).with_from_key(["某键".to_string()]);
    assert_eq!(a.hash, b.hash);
    let sa = State::new(vec![a], vec![], vec![], vec![], false);
    let sb = State::new(vec![b], vec![], vec![], vec![], false);
    assert_eq!(sa.hash, sb.hash, "StateHash 不含 parents，账本键不变");
    assert_eq!(sb.parents, BTreeSet::from(["某键".to_string()]));
    // 空来源不序列化：旧形状逐字节不变
    assert!(!serde_json::to_string(&sa).unwrap().contains("parents"));
    assert!(
        !serde_json::to_string(&Mat::literal(json!("x")))
            .unwrap()
            .contains("from_key")
    );
    // 带来源的材料能往返（MatRaw 收这个字段）
    let back: Mat = serde_json::from_value(serde_json::to_value(&sb.on[0]).unwrap()).unwrap();
    assert_eq!(back.from_key, sb.on[0].from_key);
}

#[test]
fn 题的来源不进题哈希() {
    // 本步只落消费一侧：没有结构通道产生 Question.from_key（select 的 pick 臂只交出下标）
    let q = Question::new(Op::Test, "题", "k1", vec![]);
    let mut q2 = q.clone();
    q2.from_key.insert("来源".into());
    assert_eq!(q.hash, q2.hash);
    assert!(!serde_json::to_string(&q).unwrap().contains("from_key"));
}

#[test]
fn 同一账本重放_不写新条目_账本逐字节不变() {
    let mut l = Ledger::new();
    跑(两层, &mut l);
    let 首 = l.encode();
    let 再 = 跑(两层, &mut l);
    assert_eq!(再.cost.calls, 0);
    assert_eq!(l.encode(), 首);
}
