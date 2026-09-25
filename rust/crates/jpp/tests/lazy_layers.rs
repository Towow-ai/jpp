//! 执行模型：惰性登记 + 刷新点 + 分层（`12` §2.2:129-130 与 §4:261）。
//!
//! 依据原文（自核过）：「**惰性**：登记后不发；被 `cut`、`fit`、`match`/`if`、或宿主读内容时刷新。
//! 刷新时把所有已登记且输入就绪的 `judge` 按状态分组、按依赖分层，一层一次发出」。
//!
//! Rust 现在是**逐语句即时发出**（`interp.rs` 模块注释自己写着「惰性融合是后续优化，未迁移」）。
//! 这份测试钉住惰性做出来之后该有的行为；在做出来之前它是红的。
//!
//! Python 侧是 oracle：`runtime.py:598 flush`、`:776 _run_layer`、`:841 _calls_from_plans`
//! （融合规则 = 同状态哈希 + 同段）。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::Passes;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 记下每次调用问了几道题——融合有没有发生，看的就是这个（步 15c：原 `impl Client` 的桩改为
/// 三个闭包端口；`每次题数` 用 `RefCell` 借给判断端口写，跑完后借用结束再读）
fn 记账端口(p: f64, 每次题数: &RefCell<Vec<usize>>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            每次题数.borrow_mut().push(qs.len());
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

fn 跑_带开关(src: &str, p: f64, passes: jpp::interp::Passes) -> (jpp::Outcome, Vec<usize>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次题数 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let actions = ActionRegistry::new();
    let budget = program.budget.clone();
    let mut it = jpp::interp::Interp::new(
        记账端口(p, &每次题数),
        &mut ledger,
        &calib,
        &actions,
        budget,
    );
    it.passes = passes;
    let out = it
        .run(&program)
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let 题数 = 每次题数.borrow().clone();
    (out, 题数)
}

fn 跑(src: &str, p: f64) -> (jpp::Outcome, Vec<usize>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次题数 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        记账端口(p, &每次题数),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let 题数 = 每次题数.borrow().clone();
    (out, 题数)
}

/// **省钱可见**：两次 `judge` 问的是**同一个状态**，中间没有刷新点。
/// 惰性登记 + 融合之后，它们应当在第一个 `cut` 处**一次发出**——这正是 P5 的意义。
///
/// 现在是逐语句即时发出，所以是两次调用；这条测试因此是红的。
#[test]
fn 同状态的两次登记应当融合成一次调用() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let m = mat("同一段文字");
let r1 = judge(state(m), test("含数字吗", "k"));
let r2 = judge(state(m), test("含日期吗", "k"));
let e1 = cut(r1);
let e2 = cut(r2);
consume([e1, e2], "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2)}
"#;
    let (out, 每次题数) = 跑(src, 0.9);
    assert_eq!(
        out.value_json(),
        serde_json::json!({"甲": "act", "乙": "act"}),
        "两道题都该判出来"
    );
    assert_eq!(
        每次题数,
        vec![2],
        "同一个状态上的两次登记，应当在第一个 cut（刷新点）处一次发出：期望一次调用问两道题，实际每次调用的题数是 {每次题数:?}"
    );
    assert_eq!(out.cost.calls, 1, "一次调用");
}

/// **依赖要分层**：第二道题的状态由第一道题的出口决定，两者不能同层。
/// 分层之后应当是**两层、两次调用**，每次一道题。
#[test]
fn 有依赖的判断必须分两层() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let r1 = judge(state(mat("第一段")), test("含数字吗", "k"));
let e1 = cut(r1);
let 下一段 = handle(e1, {act: fn() { "有数字的后续" }, ignore: fn() { "没数字的后续" },
                        unsure: fn(u) { consume(u, "drop"); "未决的后续" }});
let r2 = judge(state(mat(下一段)), test("含日期吗", "k"));
let e2 = cut(r2);
consume(e2, "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2)}
"#;
    let (out, 每次题数) = 跑(src, 0.9);
    assert_eq!(
        每次题数,
        vec![1, 1],
        "第二道题的状态依赖第一道的出口，必须分两层、各一次调用"
    );
    assert_eq!(out.cost.calls, 2);
}

/// **省钱可见**：同一个程序，融合前后调用数的差。
///
/// 三个状态、每个状态两道题。即时执行是 6 次调用；惰性 + 按状态融合是 3 次。
/// 这个差就是 P5 的意义——`12` §10 G2「一次调用 = 一状态多题」。
#[test]
fn 融合省下的调用数看得见() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 读一段 = fn(t) { [judge(state(mat(t)), test("含数字吗", "k")), judge(state(mat(t)), test("含日期吗", "k"))] };
let 三段 = map(["甲段", "乙段", "丙段"], 读一段);
let 出口 = map(三段, fn(rs) -> Exit { cut(rs[0]) });
let 种类 = map(出口, fn(e) { exit_kind(e) });
consume(出口, "drop");
{段数: len(种类)}
"#;
    let (out, 每次题数) = 跑(src, 0.9);
    assert_eq!(out.value_json(), serde_json::json!({"段数": 3}));
    assert_eq!(
        每次题数.len(),
        3,
        "三个状态三次调用，不是六次：每次问两题。实际 {每次题数:?}"
    );
    assert!(
        每次题数.iter().all(|n| *n == 2),
        "每次调用问两道题：{每次题数:?}"
    );
    println!(
        "省钱：即时执行 6 次调用 → 惰性 + 融合 {} 次，省 {} 次",
        out.cost.calls,
        6 - out.cost.calls
    );
}

// ---------------------------------------------------------------- 消融：开关得真的关得掉

/// `12` §4「每个一个开关，给消融留门」。**开关是消融的唯一载体**——没有它，
/// 「关掉融合成本涨多少」这种账没法再核一次。
///
/// 这条同时是那张表里「融合·不做会坏什么：E8 成本 +45%」那一栏的**量具**：
/// 同一个程序开关融合各跑一次，把调用数的差报出来。
#[test]
fn 关掉融合就逐题发_开关量得出成本差() {
    use jpp::interp::{Interp, Passes};

    let src = r#"
budget {calls: 20, cost: 1, depth: 8};
let 读一段 = fn(t) { judge(state(mat(t)), [test("含数字吗", "k"), test("含日期吗", "k")]) };
let 三段 = map(["甲段", "乙段", "丙段"], 读一段);
let 出口 = map(三段, fn(rs) -> Exit { cut(rs[0]) });
let 种类 = map(出口, fn(e) { exit_kind(e) });
consume(出口, "drop");
{段数: len(种类)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 跑一次 = |passes: Passes| {
        let mut calib = CalibStore::new();
        calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
        let 每次题数 = RefCell::new(vec![]);
        let mut ledger = jpp::ledger::Ledger::new();
        let budget = program.budget.clone();
        let actions = ActionRegistry::new();
        let mut it = Interp::new(
            记账端口(0.9, &每次题数),
            &mut ledger,
            &calib,
            &actions,
            budget,
        );
        it.passes = passes;
        let out = it
            .run(&program)
            .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
        (out.cost.calls, 每次题数.borrow().clone())
    };

    // **量融合就得把别的 pass 按住**：`vectorize` 落地之后，它会把后续各轮的题
    // 提前登记进本层——**融合关着时那些题各自一次调用，于是这个差值里混进了别人的账**。
    // 实测混进去之后关臂从 6 变 10。**一个没关干净的消融臂，差值是假的**——
    // 这次是反过来：**新 pass 污染了旧消融臂**，而旧臂的断言当场红，这是它该有的样子。
    let 基 = Passes {
        vectorize: false,
        ..Passes::default()
    };
    let (开, 开题数) = 跑一次(基.clone());
    let (关, 关题数) = 跑一次(Passes { fuse: false, ..基 });

    assert_eq!(
        开, 3,
        "融合开：三个状态三次调用，每次两题。实际每次题数 {开题数:?}"
    );
    assert!(开题数.iter().all(|n| *n == 2), "{开题数:?}");
    assert_eq!(关, 6, "融合关：逐题发，六次调用。实际每次题数 {关题数:?}");
    assert!(
        关题数.iter().all(|n| *n == 1),
        "关掉融合后每次只该问一道题：{关题数:?}"
    );
    println!(
        "融合的账：开 {开} 次调用 → 关 {关} 次，关掉成本涨 {:.0}%",
        (关 as f64 / 开 as f64 - 1.0) * 100.0
    );
}

/// 未落地的 pass，开关**开着也得报 false**——否则「开关开着」会被读成「这个 pass 在工作」。
/// 这正是「被包装成机制的 taste」那一类：一个开关若开了什么也不做，它就不是机制。
#[test]
fn 未落地的pass开关开着也不谎称在工作() {
    use jpp::interp::Passes;
    let 全开 = Passes {
        lift: true,
        fuse: true,
        fission: true,
        lower: true,
        schedule: true,
        plan: true,
        ledger: true,
        speculate: true,
        vectorize: true,
    };
    // 9 个不是 7 个：12 §4 的表写 7 行，v0.1.1 修订记录 1（:610）另增两个（推测提升、循环向量化），
    // 表从没改过。按 9 个算，文档不一致记在 INTERFACE。
    // `vectorize` 这一轮落地了，从这张表里移出去——**一个 pass 落地之后
    // 还留在「未落地」表里，这张表就开始说假话**，而它存在的全部理由是不说假话。
    for name in ["fission", "lower", "schedule", "plan"] {
        assert!(
            !全开.enabled(name),
            "{name} 还没落地，开关开着也不该说自己在工作"
        );
    }
    for name in Passes::landed() {
        assert!(全开.enabled(name), "{name} 已落地，开着就该在工作");
    }
    assert!(!Passes::none().enabled("fuse"), "全关时融合确实关掉");
}

// ---------------------------------------------------------------- lift：直线段内互不依赖的判断提到同一层

/// 这条测的是**惰性**，不是 `lift`：两次登记都发生在第一个 `cut` 之前，所以惰性本身
/// 就把它们攒到了一层。名字里没有 `lift`，免得它看起来像在给 `lift` 作证——
/// 一个不开 `lift` 也过的测试证不了 `lift`。
#[test]
fn 两次登记都在刷新点之前时惰性本身就同层() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let r1 = judge(state(mat("甲")), test("含数字吗", "k"));
let r2 = judge(state(mat("乙")), test("含日期吗", "k"));
let e1 = cut(r1);
let e2 = cut(r2);
consume([e1, e2], "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2)}
"#;
    // 两个状态不同，所以融合合并不了；但它们互不依赖，该在同一层发出
    let (out, 每次题数) = 跑(src, 0.9);
    assert_eq!(
        out.value_json(),
        serde_json::json!({"甲": "act", "乙": "act"})
    );
    assert_eq!(
        out.cost.calls, 2,
        "两个不同状态仍是两次调用——lift 省的是层数不是调用数"
    );
    assert_eq!(每次题数, vec![1, 1]);
    assert_eq!(
        out.layers.len(),
        1,
        "互不依赖，该在同一层发出；实际分了 {} 层：{:?}",
        out.layers.len(),
        out.layers
    );
}

/// **`lift` 真正要解决的形状**：源码写成「读一个、判一个」——`cut(r1)` **之后**才登记 `r2`，
/// 于是 `r2` 只能等下一个刷新点，分成两层。可它俩问的是**同一个状态**、互不依赖。
///
/// 依据是 `12`:610 修订记录 1 的推测提升：「**同状态**、静态可达、中间无 `do`/`gen`/`ask`/
/// `transform` 且不改写状态名的 `judge` 站点随首个站点一起发；只推测 `judge`，零成本零副作用
/// 故不回滚」。同状态是**依据里就有的限制**，不是我缩小范围。
///
/// **差值有消费者**：提上来之后已落地的 `fuse` 当场把它合掉，**两次调用变一次**。
/// （只提层数不省调用的那半——不同状态的层合并——要等 `schedule` 或 `budget.layers`
/// 有了消费者再说，见 INTERFACE。）
#[test]
fn lift_同状态的后续判断随首个站点一起发() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let m = mat("同一段文字");
let r1 = judge(state(m), test("含数字吗", "k"));
let e1 = cut(r1);
let r2 = judge(state(m), test("含日期吗", "k"));
let e2 = cut(r2);
consume([e1, e2], "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2)}
"#;
    let (out, 每次题数) = 跑(src, 0.9);
    assert_eq!(
        out.value_json(),
        serde_json::json!({"甲": "act", "乙": "act"})
    );
    assert_eq!(
        每次题数,
        vec![2],
        "同状态、互不依赖：该随首个站点一起发，fuse 再合成一次调用。实际 {每次题数:?}"
    );
    assert_eq!(out.cost.calls, 1, "两次调用应当变成一次");
    assert_eq!(out.layers.len(), 1, "一层：{:?}", out.layers);
}

/// **`lift` 不跨分支**（`12`:13 / :275）：`lift` 只在直线段内提升。
///
/// 注意与**推测执行**的分工：`speculate`（`12`:610 另立的 pass）**可以**把分支两侧的
/// judge 站点并入本层——那是另一条规则，成立的理由不同（推测的只有 judge，猜错不必回滚）。
/// 所以这条测试**关掉 speculate**，测的是 `lift` 自己的边界。
#[test]
fn lift_不跨分支() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let r1 = judge(state(mat("甲")), test("含数字吗", "k"));
let e1 = cut(r1);
let 走哪边 = exit_kind(e1) == "act";
consume(e1, "drop");
let 结果 = if 走哪边 {
    let r2 = judge(state(mat("乙")), test("含日期吗", "k"));
    let e2 = cut(r2);
    let k = exit_kind(e2);
    consume(e2, "drop");
    k
} else {
    "没走这边"
};
{结果: 结果}
"#;
    let (out, 每次题数) = 跑_带开关(
        src,
        0.9,
        Passes {
            speculate: false,
            ..Passes::default()
        },
    );
    assert_eq!(
        out.value_json(),
        serde_json::json!({"结果": "act"}),
        "p=0.9 过上线，走 act 那边"
    );
    assert_eq!(
        每次题数,
        vec![1, 1],
        "两次调用：lift 不许把分支里那个提到分支外"
    );
    assert!(
        out.layers.len() >= 2,
        "lift 不跨分支，所以分层，实际 {:?}",
        out.layers
    );
}

/// `lift` 的账：关掉它，调用数与层数各变成多少。
/// **报不出差值的 pass 不该留**——这是每个 pass 落地时的交付要求。
#[test]
fn 关掉lift的账() {
    use jpp::interp::{Interp, Passes};

    // 「读一个、判一个」写法：三道题问同一个状态，但各自登记在自己那次 cut 之后
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let m = mat("同一段文字");
let r1 = judge(state(m), test("含数字吗", "k"));
let e1 = cut(r1);
let r2 = judge(state(m), test("含日期吗", "k"));
let e2 = cut(r2);
let r3 = judge(state(m), test("含人名吗", "k"));
let e3 = cut(r3);
consume([e1, e2, e3], "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2), 丙: exit_kind(e3)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 跑一次 = |passes: Passes| {
        let mut calib = CalibStore::new();
        calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
        let 每次题数 = RefCell::new(vec![]);
        let mut ledger = jpp::ledger::Ledger::new();
        let actions = ActionRegistry::new();
        let budget = program.budget.clone();
        let mut it = Interp::new(
            记账端口(0.9, &每次题数),
            &mut ledger,
            &calib,
            &actions,
            budget,
        );
        it.passes = passes;
        let out = it
            .run(&program)
            .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
        (out.cost.calls, out.layers.len(), 每次题数.borrow().clone())
    };

    let (开调用, 开层, 开题数) = 跑一次(Passes::default());
    let (关调用, 关层, 关题数) = 跑一次(Passes {
        lift: false,
        ..Passes::default()
    });

    assert_eq!(
        (开调用, 开层),
        (1, 1),
        "lift 开：三道题随首个站点一起发，fuse 合成一次调用。实际每次题数 {开题数:?}"
    );
    assert_eq!(
        (关调用, 关层),
        (3, 3),
        "lift 关：各自等自己那次 cut，三层三次调用。实际 {关题数:?}"
    );
    println!("lift 的账：开 {开调用} 次调用 / {开层} 层 → 关 {关调用} 次 / {关层} 层");
}
