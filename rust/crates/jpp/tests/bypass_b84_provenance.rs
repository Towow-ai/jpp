//! B84 旁路测试（步 17c）：值级来源标签 `Provenance = (taint, sources)`。
//!
//! 依据：`附注/2026-09-24-B83续接缺口裁定.md` §二（B84）；`21` §四·6 步 17c；主会话 2026-09-24 选 (c)
//!（J-02 不变，`derived_from ⊆ sources 的 q_hash 投影`；步 18c 起 `derived_from` = 值依赖边的投影，B92）。预注册：`地基/过程记录/工程-步17c.md` §一第 6 条。

use std::collections::BTreeSet;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Mat, Op, Question, State, Taint, Value};
use jpp::{ActionRegistry, TaintOut, run};
use jpp::{lower, syntax::parse};
use serde_json::json;

/// 是非题一律 0.9；K 选一给第一个候选 0.9，并报置换众数一致（否则 `cut` 不给 `Pick`）。
/// 步 15c：原 `impl Client for 定值` 改为一个 judge 闭包端口（程序里不发 gen/ask，不注册）。
fn 定值端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |s: &State, qs: &[&Question]| {
        let k = s.over.len();
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| match q.op {
                    Op::Select => {
                        let mut v = vec![0.1 / (k.max(2) - 1) as f64; k];
                        v[0] = 0.9;
                        Answer::Choice(v)
                    }
                    _ => Answer::Noul(0.9),
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs
                .iter()
                .map(|q| (q.op == Op::Select).then_some(1.0))
                .collect(),
            perms: qs
                .iter()
                .map(|q| if q.op == Op::Select { 5 } else { 0 })
                .collect(),
        })
    }))
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    for k in ["k1", "k2", "k3", "ks"] {
        c.put(k, 0.6, 0.3, 50, "上岗", Some(0.05)).expect("写得进");
    }
    c
}

fn 动作() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::text("外面的"))
    });
    a
}

fn 跑(src: &str, ledger: &mut Ledger) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    run(&program, 定值端口(), &库(), &动作(), ledger).expect("跑得完")
}

/// （账本键，hop，parents，题哈希）
fn 条目(l: &Ledger) -> Vec<(String, u32, Vec<String>, String)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key,
                hop,
                parents,
                jkey,
                ..
            } => Some((
                key.clone(),
                *hop,
                parents.clone(),
                jkey.as_ref().map(|k| k.q.clone()).unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

fn 题哈希(l: &Ledger, keys: &BTreeSet<String>) -> BTreeSet<String> {
    let es = 条目(l);
    keys.iter()
        .filter_map(|k| es.iter().find(|e| &e.0 == k).map(|e| e.3.clone()))
        .collect()
}

#[test]
fn a_经_e_item_取出再过滤_两跳() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let first = sieve(["甲", "乙"], test("第一问？", "k1"));
let second = sieve(map(first.value, fn(e) { e.item }), test("第二问？", "k2"));
second.kind
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    let es = 条目(&l);
    let 一: Vec<&String> = es.iter().filter(|e| e.1 == 1).map(|e| &e.0).collect();
    let 二: Vec<_> = es.iter().filter(|e| e.1 == 2).collect();
    assert_eq!((一.len(), 二.len()), (2, 2));
    for e in 二 {
        assert_eq!(e.2.len(), 1);
        assert!(一.contains(&&e.2[0]));
    }
}

#[test]
fn b_content_拼接后再成材料_两跳() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let e = cut(judge(state(mat("材料")), test("第一问？", "k1")));
consume(e, "drop");
let m2 = mat(text(content(mat(e))) + "（附注）");
let e2 = cut(judge(state(m2), test("第二问？", "k2")));
consume(e2, "drop");
1
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    let mut hops: Vec<u32> = 条目(&l).iter().map(|e| e.1).collect();
    hops.sort();
    assert_eq!(hops, vec![1, 2]);
}

const 选题: &str = r#"
budget {calls: 10, cost: 1, depth: 64};
let qs = [test("甲对吗？", "k1"), test("乙对吗？", "k2")];
let s = state(mat("对象"), {over: [mat("甲"), mat("乙")]});
let k = handle(cut(judge(s, select("挑一个", "ks"))), {pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); 1 }});
let e = cut(judge(state(mat("对象")), qs[k]));
consume(e, "drop");
k
"#;

#[test]
fn c_select_后_over_k_取题再判_父含_select_键() {
    let mut l = Ledger::new();
    let out = 跑(选题, &mut l);
    assert_eq!(
        out.value_json(),
        json!(0),
        "选中了第一个（否则本测试测不出什么）"
    );
    let es = 条目(&l);
    assert_eq!(es.len(), 2);
    let (选, 判) = (&es[0], &es[1]);
    assert_eq!(
        判.2,
        vec![选.0.clone()],
        "题经 qs[k] 取出，带上 select 的出口键"
    );
    assert_eq!(判.1, 2);
}

#[test]
fn d_控制流不传播() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let e = cut(judge(state(mat("材料")), test("第一问？", "k1")));
let r = handle(e, {act: fn() {
    let e2 = cut(judge(state(mat("别的材料")), test("第二问？", "k2")));
    consume(e2, "drop");
    1
  }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 0 }});
r
"#;
    let mut l = Ledger::new();
    let out = 跑(src, &mut l);
    assert_eq!(out.value_json(), json!(1), "走进了 act 分支");
    assert!(
        条目(&l).iter().all(|e| e.1 == 1 && e.2.is_empty()),
        "分支里的判断不因条件而多一跳"
    );
}

#[test]
fn e_字面下标不带来源() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let qs = [test("甲对吗？", "k1"), test("乙对吗？", "k2")];
let e = cut(judge(state(mat("对象")), qs[0]));
consume(e, "drop");
1
"#;
    let mut l = Ledger::new();
    跑(src, &mut l);
    assert!(条目(&l).iter().all(|e| e.1 == 1 && e.2.is_empty()));
}

#[test]
fn f_来源不进哈希与账本键() {
    let a = Mat::literal(json!("x"));
    let b = Mat::literal(json!("x")).with_from_key(["某键".to_string()]);
    let sa = State::new(vec![a], vec![], vec![], vec![], false);
    let sb = State::new(vec![b], vec![], vec![], vec![], false);
    assert_eq!(sa.hash, sb.hash);
    let v1 = Value::text("同一段");
    let v2 = Value::text("同一段").with_prov(&jpp::value::Provenance::new(
        Taint::Trusted,
        jpp::value::Sources::from_key("某键"),
    ));
    assert_eq!(v1.to_json(), v2.to_json(), "报告里的值不带标签");
    assert_eq!(v1.equals(&v2), Some(true));
}

#[test]
fn g_不可信状态上_pick_的_k_为不可信() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let 脏 = do("取外部", [], 0);
let s = state(mat("对象"), {over: [mat("甲"), mat("乙")], ctx: [脏]});
handle(cut(judge(s, select("挑一个", "ks"))), {pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); 9 }})
"#;
    let mut l = Ledger::new();
    let out = 跑(src, &mut l);
    match out.value.as_ref().expect("有值") {
        Value::Int(0, p) => assert_eq!(p.taint, Taint::Untrusted, "k 继承出口 taint"),
        v => panic!("该是 pick 的 0，得 {v:?}"),
    }
    // 同一程序在可信状态上：k 可信
    let mut l2 = Ledger::new();
    let out2 = 跑(&src.replace(", ctx: [脏]", ""), &mut l2);
    match out2.value.as_ref().expect("有值") {
        Value::Int(0, p) => assert_eq!(p.taint, Taint::Trusted),
        v => panic!("{v:?}"),
    }
}

#[test]
fn h_derived_from_是_sources_投影的子集() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let e = cut(judge(state(mat("材料")), test("第一问？", "k1")));
consume(e, "drop");
let m = mat(e);
{m: m, t: transform(fn(x) { {包: content(x)} }, m), s: state(m)}
"#;
    let mut l = Ledger::new();
    let out = 跑(src, &mut l);
    let v = out.value.as_ref().expect("有值");
    let 取 = |k: &str| v.get(k).expect(k);
    let 核 = |名: &str, derived: &BTreeSet<String>, sources: &BTreeSet<String>| {
        assert!(!derived.is_empty(), "{名}：派生自那道题");
        assert!(
            derived.is_subset(&题哈希(&l, sources)),
            "{名}：derived_from ⊆ sources 的题哈希投影"
        );
    };
    match 取("m") {
        Value::Mat(m) => 核("as_mat", &m.derived_from, &m.from_key),
        x => panic!("{x:?}"),
    }
    match 取("t") {
        Value::Mat(m) => 核("transform", &m.derived_from, &m.from_key),
        x => panic!("{x:?}"),
    }
    match 取("s") {
        Value::State(s) => 核("State::new", &s.derived_from, &s.parents),
        x => panic!("{x:?}"),
    }
}

#[test]
fn i_记录一次重放一次_账本逐字节() {
    let mut l = Ledger::new();
    跑(选题, &mut l);
    let 首 = l.encode();
    let 再 = 跑(选题, &mut l);
    assert_eq!(再.cost.calls, 0);
    assert_eq!(l.encode(), 首);
}

#[test]
fn j02_现状_同题重问被选元素仍放行() {
    // 选 (c)：投影化留到步 18。这两例在「derived_from = sources 投影」下会变成 J-02 假拒绝，
    // 本步锁住现状（过程记录 17c「J-02 与投影」）
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let second = sieve(first, q);
let third = sieve(map(first.value, fn(e) { e.item }), q);
{a: second.kind, b: third.kind}
"#;
    let mut l = Ledger::new();
    let out = 跑(src, &mut l);
    assert_eq!(out.value_json()["a"], json!("sieve"));
}
