//! B92 绕过测试（步 18c；`20` 写的路径是 `tests/bypass/b92_derivation.rs`，按仓库惯例放在这里）。
//!
//! 来源边带种类：**值依赖**（内容由读数的值算出：`pick` 的 k、出口读出、`as_mat(Exit)`、`literalize`，
//! 以及由它们拼接、计算出的文本与材料）与**选择依赖**（`sieve` 元素的 `item`、按计算键取的下标与字段）。
//! J-02 的 `derived_from` 只取值依赖边的题哈希；跳数与谱系放行计全部边。同键两种边并存按值边计。
//! 依据：B92（地基/评估/2026-09-24-仪表读数4诊断与裁定.md §四·2、§八）；预注册 `地基/过程记录/工程-步18c.md`。

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Op, Question, State, Value};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 是非题 0.9（题面含「未决」的回 0.45，落在线带里切出 Unsure）；K 选一第一个候选 0.9、置换众数一致。
fn 端口<'a>() -> Ports<'a> {
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
                    _ if q.text.contains("未决") => Answer::Noul(0.45),
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
    for k in ["k1", "k2", "ks"] {
        c.put(k, 0.6, 0.3, 50, "上岗", Some(0.05)).expect("写得进");
    }
    c
}

fn 跑在(src: &str, l: &mut Ledger) -> Result<jpp::Outcome, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("记下", 0.0, true, jpp::TaintOut::Inherit, |args| {
        Ok(args.first().cloned().unwrap_or(Value::Unit))
    });
    run(&program, 端口(), &库(), &a, l).map_err(|e| e.render())
}

fn 跑(src: &str) -> Result<jpp::Outcome, String> {
    跑在(src, &mut Ledger::new())
}

fn 是j02(r: Result<jpp::Outcome, String>) {
    let Err(e) = r else { panic!("应被 J-02 拒") };
    assert!(e.contains("J-02"), "{e}");
}

#[test]
fn a_被选元素再问同题_通过() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let again = map(first.value, fn(e) { cut(judge(state(e.item), q)) });
len(again)
"#;
    let mut l = Ledger::new();
    let out = 跑在(src, &mut l).expect("选择边不算派生");
    // 再问的站点不同，账本键不同，本步仍各发一次（同一运行内按缓存键复用是步 19，B40）；这里只核不报 J-02
    assert_eq!(out.value_json(), serde_json::json!(2));
    let _ = l;
}

#[test]
fn b_literalize出口的值再问literalize那道题_j02() {
    // 解读登记见预注册 §一·3 (b)：材料由 literalize 出口的值派生，再问 literalize 的那道题
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let s = state(mat("一段话"));
let q2 = test("字面地说，提到成都吗？", "k2");
let e2 = handle(cut(judge(s, test("未决：提到成都吗？", "k1"))), {
    act: fn() { unit }, ignore: fn() { unit },
    unsure: fn(u) { literalize(u, s, q2) }
});
let m = mat("上一步说：" + text(exit_kind(e2)));
cut(judge(state(m), q2))
"#;
    是j02(跑(src));
}

#[test]
fn c_pick的k拼成文本再问同一select_j02() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = select("哪个最相关？", "ks");
let s = state(mat("一段话"), {over: ["甲", "乙", "丙"]});
let k = handle(cut(judge(s, q)), { pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); 0 } });
let s2 = state(mat("第" + text(k) + "项"), {over: ["甲", "乙", "丙"]});
cut(judge(s2, q))
"#;
    是j02(跑(src));
}

#[test]
fn d_over_k作另一题的材料_通过() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = select("哪个最相关？", "ks");
let cands = ["甲", "乙", "丙"];
let s = state(mat("一段话"), {over: cands});
let k = handle(cut(judge(s, q)), { pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); 0 } });
consume(cut(judge(state(mat(cands[k])), test("这是人名吗？", "k1"))), "drop")
"#;
    跑(src).expect("按 k 取出的候选是选择边，问另一道题通过");
    // 对照：按 k 取出的候选再问同一 select 也通过（选择边不算派生）
    let 同题 = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = select("哪个最相关？", "ks");
let cands = ["甲", "乙", "丙"];
let s = state(mat("一段话"), {over: cands});
let k = handle(cut(judge(s, q)), { pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); 0 } });
cut(judge(state(mat(cands[k]), {over: cands}), q))
"#;
    跑(同题).expect("over[k] 再问同一 select：选择边，通过");
}

#[test]
fn e_同一出口键既有值边又有选择边_按值边计_j02() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let e = first.value[0];
cut(judge(state(e.item, {ctx: [mat(e.exit)]}), q))
"#;
    是j02(跑(src));
    // 同一个值里两种边在 join 处相遇：先有选择边（item 的内容）再并入值边（出口读出），按值边计
    let 同值 = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let e = first.value[0];
cut(judge(state(mat(e.item + text(exit_kind(e.exit)))), q))
"#;
    是j02(跑(同值));
}

#[test]
fn f_效应边界保种类() {
    let 值边 = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let e = cut(judge(state(mat("去成都")), q));
let m = transform(fn(x) { content(x) }, mat(e));
cut(judge(state(m), q))
"#;
    是j02(跑(值边));
    let 选择边 = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let m = transform(fn(x) { content(x) }, first.value[0].item);
cut(judge(state(m), q))
"#;
    跑(选择边).expect("选择边过效应仍是选择边");
}

#[test]
fn g_跳数仍计选择边() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 64};
let q = test("提到成都吗？", "k1");
let first = sieve(["成都", "北京"], q);
let second = sieve(map(first.value, fn(e) { e.item }), test("是城市吗？", "k2"));
second
"#;
    let mut l = Ledger::new();
    跑在(src, &mut l).expect("跑得完");
    let 最大跳 = l
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { hop, .. } => Some(*hop),
            _ => None,
        })
        .max();
    assert_eq!(最大跳, Some(2), "选择边照样算一跳");
}
