//! `element` 开放为 `.jpp` 名字（步 25-8a，B133、B148）：手写 `element(输入, 出口, 上下文)` 造的元素与
//! `sieve` 内部造的同形同值（选择边、报告 `exits` 行、B81/B82 形状），实参错报 `E-rt-arg`；附带 `order`
//! 对打分读数的分档核对。固定端口，不发请求。
//!
//! 依据：B133（`12` §2.12）；B148；预注册 `地基/过程记录/工程-步25-8a.md` (a)–(f)。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, NoCallPorts, Ports};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 是非题按材料定读数：含「北京」0.9，含「待定」0.5，其余 0.1；打分题按材料给三档分布
fn 端口<'a>() -> Ports<'a> {
    let mut p = NoCallPorts::ports();
    p.replace(Box::new(FnPort::judge("fixed-0", |s, qs| {
        let text = s.on_text();
        Ok::<_, EffectError>(JudgeResult {
            answers: qs
                .iter()
                .map(|q| match q.scale.is_empty() {
                    false if text.contains("高") => Answer::Score(vec![0.05, 0.05, 0.9]),
                    false if text.contains("中") => Answer::Score(vec![0.1, 0.8, 0.1]),
                    false => Answer::Score(vec![0.9, 0.05, 0.05]),
                    true if text.contains("北京") => Answer::Noul(0.9),
                    true if text.contains("待定") => Answer::Noul(0.5),
                    true => Answer::Noul(0.1),
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    })));
    p
}

fn 跑(src: &str) -> Result<Outcome, String> {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    let calib = 库();
    let acts = ActionRegistry::new();
    Session::new(端口(), &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

fn 字段(v: &Value, k: &str) -> Value {
    v.get(k).unwrap_or_else(|| panic!("缺字段 {k}"))
}

fn 名单(v: &Value) -> Vec<String> {
    let Value::Record(r) = v else {
        panic!("不是记录")
    };
    let mut ks: Vec<String> = r.iter().map(|(k, _)| k.clone()).collect();
    ks.sort();
    ks
}

/// item 的来源边里有这个出口的账本键，且是选择依赖边（B92：读数只选中了它，不是内容派生自题）
fn 有选择边(item: &Value, key: &str) -> bool {
    let Value::Mat(m) = item else {
        return false;
    };
    m.prov()
        .sources
        .edges()
        .any(|(k, e)| k == key && format!("{e:?}").contains("Select"))
}

const 对照: &str = r#"budget {calls: 4, cost: 0, depth: 64};
let m = mat("我去了北京");
let q = test("这段话提到北京了吗？", "k");
let s = sieve([m], q);
let e = cut(judge(state(m), q));
let mine = element(m, e, {pos: 0, q: q});
{s: s.value[0], mine: mine, kind: exit_kind(e)}"#;

/// (a) 手写 element 与 sieve 内部造的元素同形同值；item 带自己出口的选择边；key 缺省取出口账本键
#[test]
fn a_与_sieve_的元素同形同值() {
    let o = 跑(对照).unwrap();
    let v = o.value.clone().unwrap();
    let (s, mine) = (字段(&v, "s"), 字段(&v, "mine"));
    assert_eq!(名单(&s), 名单(&mine), "字段集相同");
    for k in ["index", "pos", "qi", "trail", "cause"] {
        assert_eq!(字段(&s, k).to_json(), 字段(&mine, k).to_json(), "{k}");
    }
    assert_eq!(
        字段(&s, "item").to_json()["content"],
        字段(&mine, "item").to_json()["content"]
    );
    assert_eq!(字段(&s, "q").to_json(), 字段(&mine, "q").to_json());
    assert_eq!(字段(&mine, "exit").to_json()["exit"], "act");
    let Value::Text(key, _) = 字段(&mine, "key") else {
        panic!("key 要是 Text")
    };
    assert!(!key.is_empty(), "key 缺省取出口的账本键");
    assert!(有选择边(&字段(&mine, "item"), &key), "item 带选择边");
    let Value::Text(skey, _) = 字段(&s, "key") else {
        panic!()
    };
    assert!(有选择边(&字段(&s, "item"), &skey));
}

/// (b) 报告 exits 行：手写 element 后那一行带 index 与 pos
#[test]
fn b_报告行补编号() {
    let src = r#"budget {calls: 2, cost: 0, depth: 64};
let m = mat("我去了北京");
let q = test("这段话提到北京了吗？", "k");
let e = cut(judge(state(m), q));
let x = element(m, e, {pos: 3, q: q});
{pos: x.pos, index: x.index}"#;
    let o = 跑(src).unwrap();
    assert_eq!(o.value_json(), json!({"pos": 3, "index": 3}));
    let rows: Vec<&Json> = o.exits.iter().filter(|r| r.get("pos").is_some()).collect();
    assert_eq!(rows.len(), 1, "{:?}", o.exits);
    assert_eq!(rows[0]["pos"], 3);
    assert_eq!(rows[0]["index"], 3);
}

/// (c) 输入是元素记录：沿用它的 index、保留其余字段，trail 追加上一层出口（B81 (a)）
#[test]
fn c_输入是元素记录() {
    let src = r#"budget {calls: 4, cost: 0, depth: 64};
let m = mat("我去了北京");
let q = test("这段话提到北京了吗？", "k");
let first = sieve([mat("别的"), m], q).value[0];
let e = cut(judge(state(first.item), test("这段话提到北京了吗？再看一次", "k")));
let x = element(first, e, {pos: 0, q: q});
{index: x.index, pos: x.pos, trail: len(x.trail)}"#;
    let o = 跑(src).unwrap();
    assert_eq!(o.value_json(), json!({"index": 1, "pos": 0, "trail": 1}));
}

/// (d) 未决出口：cause 是原因文本，key 缺省取出口的账本键，未决随元素交出
#[test]
fn d_未决出口() {
    let src = r#"budget {calls: 2, cost: 0, depth: 64};
let m = mat("地点待定");
let q = test("这段话提到北京了吗？", "k");
let e = cut(judge(state(m), q));
let x = element(m, e, {pos: 0, q: q});
{cause: x.cause, has_key: len(x.key) > 0, pending: [x]}"#;
    let o = 跑(src).unwrap();
    let v = o.value_json();
    assert_eq!(v["cause"], "band");
    assert_eq!(v["has_key"], true);
}

/// (e) 实参错报 E-rt-arg
#[test]
fn e_实参错() {
    let 出口位给整数 = r#"budget {calls: 2, cost: 0, depth: 64};
let q = test("这段话提到北京了吗？", "k");
element(mat("x"), 1, {pos: 0, q: q})"#;
    let 缺题 = r#"budget {calls: 2, cost: 0, depth: 64};
let m = mat("我去了北京");
let q = test("这段话提到北京了吗？", "k");
let e = cut(judge(state(m), q));
let x = element(m, e, {pos: 0});
{k: exit_kind(e)}"#;
    for src in [出口位给整数, 缺题] {
        let e = 跑(src).expect_err("应当报错");
        assert!(e.contains("E-rt-arg") && e.contains("element("), "{e}");
    }
}

/// (f) order 对打分读数（同题同刻度、跨对象）应按档位分：高 > 中 > 低。
///
/// 核对结果（步 25-8a）：当时 `rank_value` 对打分读数取**众数档的概率**（`argmax(v).1`），不取档位，
/// 所以「低 0.9」与「高 0.9」并列、「中 0.8」排后，得 `[[0, 1], [2]]`，本条挂起（过程记录 工程-步25-8a.md Q1）。
/// B167 裁定 `measure` 读数缺省按档位排，步 15k 实现后本条转为运行（过程记录 工程-步15k.md）。
#[test]
fn f_order_对打分读数分档() {
    let src = r#"budget {calls: 4, cost: 0, depth: 64};
let q = measure("这个方案的完成度", ["低", "中", "高"], "k");
order(judge([state(mat("低")), state(mat("高")), state(mat("中"))], q))"#;
    let o = 跑(src).unwrap();
    assert_eq!(o.value_json(), json!([[1], [2], [0]]));
}
