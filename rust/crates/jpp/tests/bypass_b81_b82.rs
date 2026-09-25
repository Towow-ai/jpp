//! B81 (a)(c)、B82、A-2 旁路测试（步 25-0）。
//!
//! 依据：`附注/2026-09-24-仪表读数1裁定.md` §四（B81）、§五（B82）；`21` §四·6 步 25-0；
//! 预注册 `地基/过程记录/工程-步25-0.md`。`21` 写的 `tests/bypass/j05_projection_drop.rs`、
//! `j05_entry_return.rs` 两组用例按本仓库约定合在本文件（各 crate 的 `tests/bypass_*.rs`）。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 材料文字含「拿不准」的读 0.45（落在 0.3–0.6 的带里 → 未决），其余 0.9（接受）；
/// 不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口() -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", |s, qs| {
            let p = if s.on_text().contains("拿不准") {
                0.45
            } else {
                0.9
            };
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
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

/// 读法函数与 `lib/outcome.jpp` 同源（测试不经装载器，直接把库文本放在程序前面）。
fn 程序(body: &str) -> String {
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib/outcome.jpp"),
    )
    .expect("读得到 lib/outcome.jpp");
    // budget 声明在程序最前；库的函数定义接在它后面
    let (budget, rest) = body.split_once('\n').expect("第一行是 budget");
    format!("{budget}\n{lib}\n{rest}")
}

fn 跑(body: &str) -> Result<(Json, u64), String> {
    let program =
        lower(&parse(&程序(body)).map_err(|e| format!("{e:?}"))?).map_err(|e| format!("{e:?}"))?;
    let mut calib = CalibStore::new();
    for k in ["k1", "k2"] {
        calib.put(k, 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    }
    let mut l = Ledger::new();
    let out = run(&program, 定值端口(), &calib, &ActionRegistry::new(), &mut l)
        .map_err(|e| format!("{e:?}"))?;
    Ok((out.value_json(), out.cost.calls))
}

#[test]
fn j05_投影掉出口且不返回pending_运行期报错() {
    let e = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(["甲", "拿不准的乙"], test("行吗？", "k1"));
{review: map(undecided(o), fn(p) { {index: p.index, cause: p.cause} })}"#)
    .expect_err("投影掉 exit 的记录不是转移（B81 (c)、13 §3）");
    // 依据：B81 (c)；13 §3「只读原因文本不构成处理」
    assert!(e.contains("J-05"), "{e}");
}

#[test]
fn j05_返回条目本身即转移() {
    let (v, _) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(["甲", "拿不准的乙"], test("行吗？", "k1"));
{review: undecided(o), unobserved: unobserved(o)}"#)
    .expect("条目整体返回即转移");
    assert_eq!(v["review"][0]["index"], json!(1), "条目自带原始编号");
    assert_eq!(v["review"][0]["cause"], json!("band"));
    assert!(v["review"][0]["exit"].is_object(), "条目本身带出口");
}

#[test]
fn j05_预算停机下只返回undecided_运行期报错() {
    let body = |ret: &str| {
        format!(
            r#"budget {{calls: 1, cost: 1, depth: 64}};
let o = sieve(["甲", "乙", "丙"], test("行吗？", "k1"));
{ret}"#
        )
    };
    let e = 跑(&body("{review: undecided(o)}")).expect_err("未观察的项漏交");
    // 依据：B81 (c)（`undecided` 与 `unobserved` 都要交出）
    assert!(e.contains("J-05"), "{e}");
    let (v, _) =
        跑(&body("{review: undecided(o), unobserved: unobserved(o)}")).expect("两份都交出");
    assert!(
        !v["unobserved"].as_array().unwrap().is_empty(),
        "确实有预算停机没问到的项"
    );
}

#[test]
fn b81_链式过滤_编号跨层不变_pos是本层位置_source撤销() {
    let (v, _) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let first = sieve(["零", "一", "二"], test("第一问？", "k1"));
let second = sieve(first, test("第二问？", "k2"));
map(accepted(second), fn(e) { {index: e.index, pos: e.pos, has_source: has(e, "source")} })"#)
    .expect("跑得完");
    assert_eq!(
        v,
        json!([
            {"index": 0, "pos": 0, "has_source": false},
            {"index": 1, "pos": 1, "has_source": false},
            {"index": 2, "pos": 2, "has_source": false}
        ])
    );
}

#[test]
fn b81_配对产物经过滤保留left_right() {
    let (v, _) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(pair(["需求"], ["甲", "乙"]), test("相配吗？", "k1"));
map(accepted(o), fn(e) { {left: e.left, right: e.right, pos: e.pos, has_index: has(e, "index")} })"#)
    .expect("跑得完");
    assert_eq!(
        v,
        json!([
            {"left": "需求", "right": "甲", "pos": 0, "has_index": false},
            {"left": "需求", "right": "乙", "pos": 1, "has_index": false}
        ])
    );
}

#[test]
fn b82_多题返回一个契约值_元素是材料乘题() {
    let (v, calls) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(["甲", "乙"], [test("第一问？", "k1"), test("第二问？", "k2")]);
{kind: o.kind, n: len(accepted(o)), order: map(accepted(o), fn(e) { [e.index, e.qi] }),
 q1: map(accepted(by_q(o, 1)), fn(e) { e.index }), q1_text: by_q(o, 1).detail.question.text,
 evidence: len(o.evidence), sub_evidence: len(by_q(o, 1).evidence), calls: o.spent.calls, sub_calls: by_q(o, 1).spent.calls}"#)
    .expect("跑得完");
    assert_eq!(v["kind"], json!("sieve"), "多题产物是契约值，不是列表");
    assert_eq!(v["n"], json!(4));
    assert_eq!(
        v["order"],
        json!([[0, 0], [0, 1], [1, 0], [1, 1]]),
        "材料主序、题次序"
    );
    assert_eq!(v["q1"], json!([0, 1]));
    assert_eq!(v["q1_text"], json!("第二问？"));
    assert_eq!(
        (v["evidence"].clone(), v["sub_evidence"].clone()),
        (json!(4), json!(2))
    );
    assert_eq!(calls, 2, "两份材料两个状态，两道题融合：两次调用");
    assert_eq!(v["calls"], json!(2));
    assert_eq!(
        v["sub_calls"],
        json!(2),
        "融合的调用各题共享，子契约值的花费与整体相同"
    );
}

#[test]
fn b82_填法记录的非槽键进元素_不进题面() {
    let (v, _) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let f = form("test", "这段话提到{c}吗？", {calib: "k1"});
let o = sieve(["甲"], f, [{c: "A", parent: "根"}, {c: "B", parent: "根"}]);
map(accepted(o), fn(e) { {c: e.fill.c, parent: e.fill.parent, text: e.q.text} })"#)
    .expect("填法里多出的键不报错");
    assert_eq!(
        v,
        json!([
            {"c": "A", "parent": "根", "text": "这段话提到A吗？"},
            {"c": "B", "parent": "根", "text": "这段话提到B吗？"}
        ])
    );
}

#[test]
fn a2_outcome不收spent_花费由证据算() {
    let e = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
outcome({value: [], spent: {calls: 3}})"#)
    .expect_err("A-2");
    assert!(e.contains("spent"), "{e}");
    let (v, _) = 跑(r#"budget {calls: 10, cost: 1, depth: 64};
let o = sieve(["甲", "乙"], test("行吗？", "k1"));
let re = outcome({value: accepted(o), pending: o.pending, evidence: key_of(o)});
re.spent.calls"#)
    .expect("跑得完");
    assert_eq!(v, json!(2), "证据的两个键分属两次调用");
}
