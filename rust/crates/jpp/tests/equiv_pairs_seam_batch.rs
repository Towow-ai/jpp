//! 批调度缝的等价写法（步 13b；`21` §三·6 写 `tests/equiv_pairs/seam_batch.rs`，按 `jpp-core/tests/` 的放法落在这里）。
//!
//! 依据：`16` §一（同一程序两种写法调用数相差十四倍）；`20` §4.5 第 2、4 条与纸面推演（三种写法都为 1 层）；
//! `00-定位与方法论` §三推论：使用者不需要在每次调用时判断什么值得并行。
//! 断言写死值（`21` §三·6）。另有两条 K-069 形状的回归：穿过包装提前登记时，不得提前执行 `do`。
//! 预注册：`地基/过程记录/工程-步13b.md` §一。

mod common;
use std::cell::RefCell;
use std::rc::Rc;

use jpp::TaintOut;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 步 15c：原 `impl Client` 的计数桩改为三个闭包端口；`调用` 用 `RefCell` 借给判断端口计数
fn 计数端口(调用: &RefCell<u64>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *调用.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

struct 结果 {
    calls: u64,
    layers: usize,
    dos: usize,
    value: Json,
}

fn 跑(src: &str, passes: Passes) -> 结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 动作次数 = Rc::new(RefCell::new(0usize));
    let mut actions = ActionRegistry::new();
    let n = 动作次数.clone();
    actions.register("记一笔", 0.0, true, TaintOut::Trusted, move |_| {
        *n.borrow_mut() += 1;
        Ok(Value::text("记了"))
    });
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.65, 0.35, 100);
    let 调用 = RefCell::new(0u64);
    let mut ledger = Ledger::new();
    let budget = program.budget.clone();
    let mut it = Interp::new(计数端口(&调用), &mut ledger, &calib, &actions, budget);
    it.passes = passes;
    let out = it
        .run(&program)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    let dos = *动作次数.borrow();
    结果 {
        calls: out.cost.calls,
        layers: out.layers.len(),
        dos,
        value: out.value_json(),
    }
}

const 前言: &str = r#"
budget {calls: 20, cost: 1, depth: 64};
let m = mat("我上个月去了成都出差，顺便见了老朋友。");
let cities = ["成都", "北京", "上海", "杭州", "西安", "南京", "武汉", "重庆", "天津", "苏州", "长沙", "郑州", "青岛", "厦门"];
let qs = map(cities, fn(c) { test("这段话是否提到了" + c + "？", "k") });
fn 查(q: Question) !{judge} {
    handle(cut(judge(state(m), q)), { act: fn() { "是" }, ignore: fn() { "否" },
        unsure: fn(u) { consume(u, "drop"); "未决" } })
}
fn 外层(q: Question) !{judge} { 查(q) }
"#;

fn 程序(尾: &str) -> String {
    format!("{前言}\n{尾}")
}

/// `16` §一的写法二、写法三与两层包装：同一材料十四道题，都是 1 次调用、1 层。
/// 写法一（直接交给 `sieve`）的 1 次由 `examples/sieve-batch-new.jpp` 的金样与 `scripts/equiv_pairs.py` 钉住。
#[test]
fn 三种写法与多层包装调用数相同() {
    for (名, 尾) in [
        ("map(题组, 查)", "map(qs, 查)"),
        ("map(题组, fn(q){查(q)})", "map(qs, fn(q) { 查(q) })"),
        ("两层包装", "map(qs, fn(q) { 外层(q) })"),
    ] {
        let r = 跑(&程序(尾), Passes::default());
        assert_eq!((r.calls, r.layers), (1, 1), "{名}");
    }
}

/// 消融：关掉向量化，包装写法回到一题一层（14 次、14 层）；出口不变。
#[test]
fn 关掉向量化回到逐题() {
    let src = 程序("map(qs, fn(q) { 查(q) })");
    let 开 = 跑(&src, Passes::default());
    let 关 = 跑(
        &src,
        Passes {
            vectorize: false,
            ..Passes::default()
        },
    );
    assert_eq!((关.calls, 关.layers), (14, 14));
    assert_eq!(开.value, 关.value);
}

/// K-069 形状一：实参含调用（`side(x)` 里有 `do`）时不穿过包装；`do` 只在真走到的轮次执行，不被提前执行。
#[test]
fn 实参含调用不穿过包装() {
    let src = r#"
budget {calls: 20, cost: 1, depth: 64};
fn side(x) -> Mat !{do} { do("记一笔", [x], 0) }
fn 包(m: Mat) !{judge} {
    handle(cut(judge(state(m), test("提到了吗？", "k"))), { act: fn() { 1 }, ignore: fn() { 0 },
        unsure: fn(u) { consume(u, "drop"); 2 } })
}
map(["甲", "乙", "丙"], fn(x) { 包(side(x)) })
"#;
    let 开 = 跑(src, Passes::default());
    let 关 = 跑(
        src,
        Passes {
            vectorize: false,
            ..Passes::default()
        },
    );
    assert_eq!(开.dos, 3, "三轮各执行一次 do，提前登记不得多执行");
    assert_eq!(
        (开.calls, 开.layers),
        (关.calls, 关.layers),
        "不穿过：与关掉向量化相同"
    );
    assert_eq!(开.value, 关.value);
}

/// K-069 形状二：实参是体内含 `do` 的方法值时不穿过（按严格口径判为可能有效应）；`do` 次数与出口同现状。
#[test]
fn 实参是带副作用的方法不穿过包装() {
    let src = r#"
budget {calls: 20, cost: 1, depth: 64};
fn act(x) -> Mat !{do} { do("记一笔", [x], 0) }
fn 包(x, f) !{judge, do} {
    handle(cut(judge(state(mat(x)), test("提到了吗？", "k"))), { act: fn() { f(x) }, ignore: fn() { 0 },
        unsure: fn(u) { consume(u, "drop"); 2 } })
}
map(["甲", "乙", "丙"], fn(x) { 包(x, act) })
"#;
    let 开 = 跑(src, Passes::default());
    let 关 = 跑(
        src,
        Passes {
            vectorize: false,
            ..Passes::default()
        },
    );
    assert_eq!(
        (开.calls, 开.layers),
        (关.calls, 关.layers),
        "不穿过：与关掉向量化相同"
    );
    assert_eq!(开.dos, 关.dos);
    assert_eq!(开.value, 关.value);
}
