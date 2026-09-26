//! 步 25d：`lib/compose/graph.jpp`（判出来的图 + 图算法）。真动作 `graph:*`（R2b），固定判断端口按
//! 材料给读数，库文本接在 budget 行后（与 `bypass_21_1_absent_runs.rs` 同一装配）。
//!
//! 依据：B148；`21` 步 25d；预注册 `地基/过程记录/工程-步25d.md` §一·3。

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes, TaintOut};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};

const 库: &str = include_str!("../../../lib/compose/graph.jpp");

/// 读数表：材料文字里同时含两个记号的那一对给定读数；其余题按题面给（含「不」0.05，否则 0.95）
fn 端口<'a>(边: &'a [(&'a str, &'a str, f64)], 每次: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        每次.borrow_mut().push(qs.len());
        let t = s.on_text();
        let p = 边
            .iter()
            .find(|(a, b, _)| t.contains(a) && t.contains(b))
            .map(|x| x.2);
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    Answer::Noul(match p {
                        Some(p) if q.text.contains("合作") => p,
                        _ if q.text.contains("不") => 0.05,
                        _ => 0.95,
                    })
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
            confidence: vec![],
        })
    }))
}

struct 跑结果 {
    r: Result<serde_json::Value, String>,
    每次: Vec<usize>,
    层: usize,
    do条目: usize,
    告警: Vec<String>,
    重放: Option<(Result<serde_json::Value, String>, Vec<usize>)>,
    /// 不可逆动作「发邮件」执行了几次
    发送: u32,
}

fn 跑(body: &str, 边: &[(&str, &str, f64)]) -> 跑结果 {
    跑一(body, 边, false)
}

fn 跑一(body: &str, 边: &[(&str, &str, f64)], 重放: bool) -> 跑结果 {
    let (budget, rest) = body.split_once('\n').expect("第一行是 budget");
    let src = format!("{budget}\n{库}\n{rest}");
    let program = lower(&parse(&src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let mut calib = CalibStore::new();
    for k in ["edge", "team", "merge"] {
        common::certified(&mut calib, k, 0.8, 0.2, 50);
    }
    let mut actions = ActionRegistry::new();
    jpp::actions::register_all(&mut actions, &jpp::actions::Ctx::default(), false);
    // 一个不可逆动作，核 J-08 放行用
    let 发送 = Rc::new(Cell::new(0u32));
    let c = 发送.clone();
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        c.set(c.get() + 1);
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let 每次 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let (r, 告警, 层) = {
        let mut it = jpp::interp::Interp::new(
            端口(边, &每次),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        );
        it.passes = Passes::default();
        match it.run(&program) {
            Ok(o) => (Ok(o.value_json()), o.trace.warnings.clone(), o.layers.len()),
            Err(e) => (Err(e.render()), vec![], 0),
        }
    };
    let 重放 = 重放.then(|| {
        let 每次2 = RefCell::new(vec![]);
        let again = {
            let mut it = jpp::interp::Interp::new(
                端口(边, &每次2),
                &mut ledger,
                &calib,
                &actions,
                program.budget.clone(),
            )
            .audit_replay();
            it.passes = Passes::default();
            it.run(&program)
                .map(|o| o.value_json())
                .map_err(|e| e.render())
        };
        let n = 每次2.borrow().clone();
        (again, n)
    });
    let do条目 = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Effect { .. }))
        .count();
    let 每次 = 每次.borrow().clone();
    eprintln!("每次={每次:?} 层={层} do条目={do条目} 告警={告警:?}");
    match &r {
        Ok(v) => eprintln!("值={}", serde_json::to_string(v).unwrap()),
        Err(e) => eprintln!("错={e}"),
    }
    跑结果 {
        r,
        每次,
        层,
        do条目,
        告警,
        重放,
        发送: 发送.get(),
    }
}

fn 值(x: &跑结果) -> &serde_json::Value {
    x.r.as_ref().unwrap_or_else(|e| panic!("程序应当跑完：{e}"))
}

fn j(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

/// 二部图 6 × 6：6 条已决有边、2 条已决无边、2 条未决（L4–R4、L5–R5）
const 二部边: &[(&str, &str, f64)] = &[
    ("L0", "R0", 0.95),
    ("L1", "R1", 0.95),
    ("L2", "R2", 0.95),
    ("L3", "R3", 0.95),
    ("L4", "R0", 0.95),
    ("L5", "R1", 0.95),
    ("L0", "R4", 0.05),
    ("L1", "R5", 0.05),
    ("L4", "R4", 0.5),
    ("L5", "R5", 0.5),
];

const 二部程序: &str = r#"budget {calls: 20, cost: 1, depth: 64};
let left = map(range(0, 6), fn(i) { mat(join(["L", text(i)], "")) });
let right = map(range(0, 6), fn(j) { mat(join(["R", text(j)], "")) });
let cand = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 0], [5, 1], [0, 4], [1, 5], [4, 4], [5, 5]];
let edge = test("这两方能合作吗？", "edge");
let g = judged_bipartite(left, right, edge, {prune: fn(l, r) { cand }});
let t = interval(g, "matching", {});
{lo: map(t.lo.value, fn(p) { p.members }), hi: map(t.hi.value, fn(p) { p.members }),
 hi_open: map(t.hi.pending, fn(p) { p.members }), differs: map(t.differs, fn(e) { e.ends }),
 same: t.same, alpha_bound: t.alpha_bound, n_unknown: t.n_unknown,
 counts: [len(g.edges), len(g.rejected), len(g.pending)],
 pending: concat(t.hi.pending, g.pending)}
"#;

#[test]
fn 二部图区间() {
    // 预注册 (2)：lo 4 对、hi 6 对（其中 2 对用到未决边进 hi.pending），differs 恰为两条未决边
    let x = 跑(二部程序, 二部边);
    let v = 值(&x);
    assert_eq!(v["counts"], j("[6, 2, 2]"));
    assert_eq!(v["lo"], j("[[0, 6], [1, 7], [2, 8], [3, 9]]"));
    assert_eq!(v["hi"], j("[[0, 6], [1, 7], [2, 8], [3, 9]]"));
    assert_eq!(v["hi_open"], j("[[4, 10], [5, 11]]"));
    assert_eq!(v["differs"], j("[[4, 10], [5, 11]]"));
    assert_eq!(v["same"], j("false"));
    // B161：乐观图 6 个产物各用 1 条边，认证线 α = 0.10，联合界 min(1, 6 × 0.10) = 0.6，没有未知
    let ab = v["alpha_bound"].as_f64().unwrap();
    assert!((ab - 0.6).abs() < 1e-9, "alpha_bound {ab}");
    assert_eq!(v["n_unknown"], j("0"));
    // 十条候选边各是一个状态，一层发出（不同状态不能合成一次调用）
    assert_eq!(x.每次, vec![1; 10]);
    assert_eq!(x.层, 1, "判边层内一次刷新");
    assert_eq!(x.do条目, 2, "decided、optimistic 各一次");
}

#[test]
fn 重放零调用() {
    // 预注册 (8)
    let x = 跑一(二部程序, 二部边, true);
    let (again, n) = x.重放.as_ref().unwrap();
    assert_eq!(again.as_ref().unwrap(), 值(&x));
    assert!(n.is_empty(), "重放不发判断：{n:?}");
}

const 单集合边: &[(&str, &str, f64)] = &[
    ("N0", "N1", 0.95),
    ("N0", "N2", 0.05),
    ("N1", "N2", 0.95),
    ("N2", "N3", 0.5),
    ("N3", "N4", 0.95),
    ("N4", "N5", 0.95),
    ("N3", "N5", 0.95),
];

#[test]
fn 单集合_over与prune一致() {
    // 预注册 (1)：over 只留 7 对；prune 给同样 7 对，边、无边、未决逐条相同
    let 程序 = |opts: &str| {
        format!(
            r#"budget {{calls: 20, cost: 1, depth: 64}};
let ns = map(range(0, 6), fn(i) {{ mat(join(["N", text(i)], "")) }});
let ok = ["N0-N1", "N0-N2", "N1-N2", "N2-N3", "N3-N4", "N4-N5", "N3-N5"];
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {opts});
{{edges: map(g.edges, fn(e) {{ e.ends }}), rejected: map(g.rejected, fn(e) {{ e.ends }}),
 pending: g.pending}}
"#
        )
    };
    let a = 跑(
        &程序(r#"{over: fn(a, b) { contains(ok, join([content(a), content(b)], "-")) }}"#),
        单集合边,
    );
    let b = 跑(
        &程序("{prune: fn(ns) { [[0, 1], [0, 2], [1, 2], [2, 3], [3, 4], [3, 5], [4, 5]] }}"),
        单集合边,
    );
    let (va, vb) = (值(&a), 值(&b));
    assert_eq!(va["edges"], j("[[0, 1], [1, 2], [3, 4], [3, 5], [4, 5]]"));
    assert_eq!(va["rejected"], j("[[0, 2]]"));
    assert_eq!(va["pending"].as_array().unwrap().len(), 1);
    assert_eq!(va, vb);
    assert_eq!((a.层, a.每次.len()), (1, 7));
}

#[test]
fn 最短路_已决不可达乐观可达() {
    // 预注册 (3)：0–1、2–3 已决有，1–2 未决；已决图 0 到 3 不可达，乐观图一条路径进 hi.pending
    let 边 = &[("N0", "N1", 0.95), ("N1", "N2", 0.5), ("N2", "N3", 0.95)];
    let x = 跑(
        r#"budget {calls: 20, cost: 1, depth: 64};
let ns = map(range(0, 4), fn(i) { mat(join(["N", text(i)], "")) });
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {prune: fn(ns) { [[0, 1], [1, 2], [2, 3]] }});
let t = interval(g, "shortest_path", {source: 0, target: 3});
{lo: len(t.lo.value), hi_open: map(t.hi.pending, fn(p) { p.members }),
 differs: map(t.differs, fn(e) { e.ends }), same: t.same, pending: concat(t.hi.pending, g.pending)}
"#,
        边,
    );
    let v = 值(&x);
    assert_eq!(v["lo"], j("0"));
    assert_eq!(v["hi_open"], j("[[0, 1, 2, 3]]"));
    assert_eq!(v["differs"], j("[[1, 2]]"));
    assert_eq!(v["same"], j("false"));
}

#[test]
fn 两层嵌套_产物再判() {
    // 预注册 (4)：已决匹配的每一对再判一道队级题（产物的 item 就是判边时的那份材料），一层发出
    let x = 跑(
        &二部程序.replace(
            "{lo: map(t.lo.value",
            "let teams = sieve(t.lo.value, test(\"这一对能组队吗？\", \"team\"));\n{ok: map(teams.value, fn(e) { e.members }), ok_edges: map(teams.value, fn(e) { len(e.edges) }), lo: map(t.lo.value",
        ),
        二部边,
    );
    let v = 值(&x);
    assert_eq!(v["ok"], j("[[0, 6], [1, 7], [2, 8], [3, 9]]"));
    assert_eq!(v["ok_edges"], j("[1, 1, 1, 1]"));
    assert_eq!(x.层, 2, "判边一层、队级题一层");
    assert_eq!(x.每次.len(), 14);
}

#[test]
fn 两层嵌套_产物作下一张图的节点() {
    // 预注册 (5)：第一张图的 4 个已决配对作第二张图的节点，判「两队能否合并」，再在第二层上匹配
    let x = 跑(
        &二部程序.replace(
            "{lo: map(t.lo.value",
            "let g2 = judged_graph(t.lo.value, test(\"这两队能合并吗？\", \"merge\"), {});\nlet t2 = interval(g2, \"matching\", {});\n{groups: map(t2.lo.value, fn(p) { map(p.nodes, fn(n) { n.members }) }), g2: [len(g2.edges), len(g2.pending)], same2: t2.same, lo: map(t.lo.value",
        ),
        二部边,
    );
    let v = 值(&x);
    assert_eq!(v["g2"], j("[6, 0]"), "4 个节点两两 6 对，全部已决有边");
    assert_eq!(
        v["groups"].as_array().unwrap().len(),
        2,
        "一般图匹配出 2 组：{}",
        v["groups"]
    );
    assert_eq!(v["same2"], j("true"));
    assert_eq!(x.层, 2);
}

#[test]
fn 不支持的算法与预算不够() {
    // 预注册 (7)：不支持的算法、预算只够判边时，on_graph 返回空产物与 detail.failed，不中止
    let x = 跑(
        &二部程序
            .replace("budget {calls: 20,", "budget {calls: 10,")
            .replace(
                "{lo: map(t.lo.value",
                "let bad = on_graph(g, \"set_cover\", {}, \"decided\");\n{bad: [len(bad.value), has(bad.detail, \"failed\")], complete: t.complete, lo: map(t.lo.value",
            ),
        二部边,
    );
    let v = 值(&x);
    assert_eq!(v["bad"], j("[0, true]"));
    assert_eq!(v["complete"], j("false"));
    assert_eq!(v["same"], j("false"), "没跑成不算结论不依赖未决边");
    assert_eq!(v["lo"], j("[]"));
    assert_eq!(x.do条目, 0);
}

#[test]
fn 未决的两种返回形状() {
    // 预注册 (6) 与 §四（B162）：责任按账本键计。乐观产物的合成出口吸收了它用到的未决边，interval 的 exit
    // 再吸收全部乐观产物；同一条边在 g.pending、产物 edges、合成出口的分量里是同一份责任的几个视图。
    // 两处都带、只带 g.pending、只带 hi.pending 都算转交；都不带报 J-05
    for (名, 尾, 过) in [
        ("两处", "pending: concat(t.hi.pending, g.pending)}", true),
        ("只g", "pending: g.pending}", true),
        ("只hi", "pending: t.hi.pending}", true),
        ("都不带", "n: 0}", false),
    ] {
        let src = 二部程序.replace("pending: concat(t.hi.pending, g.pending)}", 尾);
        let x = 跑(&src, 二部边);
        if 过 {
            assert!(x.r.is_ok(), "{名}：{:?}", x.r);
            assert!(
                x.告警.iter().any(|w| w.starts_with("returned_unsure: ")),
                "{名}：{:?}",
                x.告警
            );
        } else {
            assert!(
                x.r.as_ref().unwrap_err().contains("[J-05]"),
                "{名}：{:?}",
                x.r
            );
        }
    }
}

#[test]
fn 只带乐观产物会漏掉没用到的未决边() {
    // §四预测：0–1、2–3 已决有，1–2、3–4 未决；0 到 3 的乐观路径只用到 1–2。只带 t.hi.pending 时 3–4 那条
    // 没有持有者在返回值里，报 J-05；带上 g.pending 就过
    let 边 = &[
        ("N0", "N1", 0.95),
        ("N1", "N2", 0.5),
        ("N2", "N3", 0.95),
        ("N3", "N4", 0.5),
    ];
    let 程序 = |尾: &str| {
        format!(
            r#"budget {{calls: 20, cost: 1, depth: 64}};
let ns = map(range(0, 5), fn(i) {{ mat(join(["N", text(i)], "")) }});
let g = judged_graph(ns, test("这两方能合作吗？", "edge"), {{prune: fn(ns) {{ [[0, 1], [1, 2], [2, 3], [3, 4]] }}}});
let t = interval(g, "shortest_path", {{source: 0, target: 3}});
{{differs: map(t.differs, fn(e) {{ e.ends }}), {尾}}}
"#
        )
    };
    let 只hi = 跑(&程序("pending: t.hi.pending"), 边);
    let e = 只hi.r.as_ref().unwrap_err();
    assert!(e.contains("[J-05]"), "{e}");
    let 带g = 跑(&程序("pending: concat(t.hi.pending, g.pending)"), 边);
    assert_eq!(值(&带g)["differs"], j("[[1, 2]]"));
}

#[test]
fn 先处理_differs_的边() {
    // B162：手册说「differs 里的边先问人」。按边处理（drop 或 escalate）即解除这条边的责任，哪怕它已被乐观产物的
    // 合成出口吸收；乐观产物与 interval 的 exit 是同一责任的视图，不必再带回。图里其余的未决边（这里没有）仍要带回。
    // escalate 走同一个登记点（host_builtins 的 escalate 在销账前按键登记解除），这里的端口不带 ask，只测 drop
    let src = 二部程序
        .replace(
            "{lo: map(t.lo.value",
            "let routed = map(t.differs, fn(e) { handle(e.exit, {act: fn() { \"接\" }, ignore: fn() { \"不接\" }, unsure: fn(u) { consume(u, \"drop\"); \"丢\" }}) });\n{routed: len(routed), lo: map(t.lo.value",
        )
        .replace("pending: concat(t.hi.pending, g.pending)}", "n: 0}");
    let x = 跑(&src, 二部边);
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert_eq!(值(&x)["routed"], j("2"));
}

#[test]
fn differs_非空时乐观产物与区间出口不放行() {
    // 主会话 2026-09-26 第 3 问：differs 非空意味着乐观产物至少用到一条未决边；产物出口是 compose(…, "all")，
    // 用到的边只有 act 与未决两种，所以合成结果是未决，不会走 act 臂，不可逆动作不会被放行。interval 的 exit
    // 是全部乐观产物的 all 合成，同理。已决图的产物只用已决边，不受这一条约束（它的放行按自己的边定，25-9）
    let 发 = r#"{act: fn() { content(do("发邮件", [], 0)) }, ignore: fn() { "不发" }, unsure: fn(u) { consume(u, "drop"); "不发" }}"#;
    let src = 二部程序
        .replace(
            "{lo: map(t.lo.value",
            &format!(
                "let kinds = map(t.hi.pending, fn(p) {{ exit_kind(p.exit) }});\nlet whole = exit_kind(t.exit);\nlet sent = map(t.hi.pending, fn(p) {{ handle(p.exit, {发}) }});\nlet sent_all = handle(t.exit, {发});\n{{kinds: kinds, whole: whole, sent: sent, sent_all: sent_all, lo: map(t.lo.value"
            ),
        )
        .replace("pending: concat(t.hi.pending, g.pending)}", "pending: g.pending}");
    let x = 跑(&src, 二部边);
    let v = 值(&x);
    assert_eq!(v["differs"].as_array().unwrap().len(), 2);
    assert_eq!(v["kinds"], j(r#"["unsure(band)", "unsure(band)"]"#));
    assert_eq!(v["whole"], "unsure(band)");
    assert_eq!(v["sent"], j(r#"["不发", "不发"]"#));
    assert_eq!(v["sent_all"], "不发");
    assert_eq!(x.发送, 0, "未决的合成出口不放行不可逆动作");
}
