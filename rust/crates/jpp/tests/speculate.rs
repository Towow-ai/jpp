//! 推测执行（pass `speculate`，`12`:610 修订记录 1「judge 推测提升」）。
//!
//! **为什么这条在 Jev 上成立、在 CPU 上要回滚**（`00-宪法.md` 第 46 行的借用登记）：
//! CPU 分支预测必须回滚，因为分支两侧都有副作用；**我们推测的只有 `judge`——模型只分配概率、
//! 不触世界（I1），题近乎免费（P5），读数记账可重放（P3）**。所以猜错不必回滚，只多花一个调用。
//! 这个能力直接来自 Jev 的公理，不是从 CPU 那边抄的。
//!
//! **不推测 `do` / `gen`**（红队 06 A1）：它们有代价或触世界，猜错要回滚，而我们没有回滚。

use std::cell::RefCell;

use jpp::ActionRegistry;
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};

/// 数调用的判断端口：每次调用把问了几道题记进共享计数器；不该生成、不该问人
/// （步 15c：原 `impl Client` 的桩改为闭包端口，`每次题数` 借调用处的 `RefCell` 而不是自带字段）
fn 记账端口<'a>(p: f64, 每次题数: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            每次题数.borrow_mut().push(qs.len());
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

fn 跑(src: &str, p: f64, passes: Passes) -> (jpp::Outcome, Vec<usize>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次题数 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let actions = ActionRegistry::new();
    let budget = program.budget.clone();
    let mut it = Interp::new(
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

/// **验收：省下的调用次数。** 分支两侧各有一个 judge，两侧的状态在分支前就已定。
/// 推测前三层（条件一层、两个分支体各一层）；推测后应当**一层**。
#[test]
fn 分支两侧的judge能并入本层() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
let 条件 = handle(cut(judge(state(mat("条件料")), q)), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    handle(cut(judge(甲, q)), {act: fn() { "甲act" }, ignore: fn() { "甲ignore" },
                               unsure: fn(u) { consume(u, "drop"); "甲unsure" }})
} else {
    handle(cut(judge(乙, q)), {act: fn() { "乙act" }, ignore: fn() { "乙ignore" },
                               unsure: fn(u) { consume(u, "drop"); "乙unsure" }})
};
{r: 结果}
"#;
    let (关, 关题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: false,
            ..Passes::default()
        },
    );
    let (开, 开题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );

    assert_eq!(
        开.value_json()["r"],
        serde_json::json!("甲act"),
        "结果不该被推测改变"
    );
    assert_eq!(
        关.value_json()["r"],
        开.value_json()["r"],
        "开关推测，程序的值必须一样"
    );

    assert_eq!(
        关.layers.len(),
        2,
        "关推测：条件一层、走到的那个分支体一层。实际 {:?}",
        关.layers
    );
    assert_eq!(
        开.layers.len(),
        1,
        "开推测：两侧的 judge 并入本层，一层发完。实际 {:?}",
        开.layers
    );
    println!(
        "推测的账：关 {} 层 / {} 次调用（{关题数:?}） → 开 {} 层 / {} 次调用（{开题数:?}）",
        关.layers.len(),
        关.cost.calls,
        开.layers.len(),
        开.cost.calls
    );
}

/// 推错的那一侧要记 `W-spec-unused`——**推测花了调用，花掉的必须留痕**。
#[test]
fn 推错记w_spec_unused() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
let 条件 = handle(cut(judge(state(mat("条件料")), q)), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    handle(cut(judge(甲, q)), {act: fn() { "甲" }, ignore: fn() { "甲" },
                               unsure: fn(u) { consume(u, "drop"); "甲" }})
} else {
    handle(cut(judge(乙, q)), {act: fn() { "乙" }, ignore: fn() { "乙" },
                               unsure: fn(u) { consume(u, "drop"); "乙" }})
};
{r: 结果}
"#;
    let (out, _) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-spec-unused")),
        "走了甲那侧，乙那侧的推测没用上，要留痕：{:?}",
        out.trace.warnings
    );
}

/// **不跨分支推测 `do` / `gen`**（红队 06 A1）：它们有代价或触世界，猜错要回滚，而我们没有回滚。
#[test]
fn 不跨分支推测do和gen() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "k");
let 条件 = handle(cut(judge(state(mat("条件料")), q)), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 { "走了这边" } else { content(do("不该触发", [], 0)) };
{r: 结果}
"#;
    // 动作不登记：一旦推测执行了它，会报 J-11「动作未登记」
    let (out, _) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );
    assert_eq!(
        out.value_json()["r"],
        serde_json::json!("走了这边"),
        "没走的分支里的 do 不该被执行"
    );
}

/// **分支体内才产生的名字**：那一侧的站点依赖它，推测时还不存在，不推。
/// 这是「看起来可推测但决定不推」的第一类——**名字不在，状态就算不出来**。
#[test]
fn 依赖分支内名字的站点不推测() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let q = test("行吗", "k");
let 条件 = handle(cut(judge(state(mat("条件料")), q)), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    let 分支内的料 = mat("分支里才造出来的");
    handle(cut(judge(state(分支内的料), q)), {act: fn() { "甲" }, ignore: fn() { "甲" },
                                             unsure: fn(u) { consume(u, "drop"); "甲" }})
} else { "没走" };
{r: 结果}
"#;
    let (开, _) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );
    assert_eq!(开.value_json()["r"], serde_json::json!("甲"), "程序照常跑");
    assert_eq!(
        开.layers.len(),
        2,
        "那个站点依赖分支内的名字，推不了，仍是两层：{:?}",
        开.layers
    );
}

/// **共状态时推测是不是零边际？** 总控提的问题，比上一条的数字更要紧。
///
/// P5 是「**状态收费、题免费**」。如果被推测的那道题**骑在一个本来就要发的状态上**，
/// 它应该是零边际的——不是多花一次调用，是白搭。上一条实测多花了一次，
/// 是因为两个分支体的 judge 各自需要**不同的状态**。
///
/// 这条测共状态：分支两侧的 judge 与条件**问同一个状态、不同的题**。
#[test]
fn 共状态时推测是否零边际() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 同一个 = state(mat("同一份材料"));
let 条件 = handle(cut(judge(同一个, test("该继续吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    handle(cut(judge(同一个, test("甲问题", "k"))), {act: fn() { "甲" }, ignore: fn() { "甲" },
                                                    unsure: fn(u) { consume(u, "drop"); "甲" }})
} else {
    handle(cut(judge(同一个, test("乙问题", "k"))), {act: fn() { "乙" }, ignore: fn() { "乙" },
                                                    unsure: fn(u) { consume(u, "drop"); "乙" }})
};
{r: 结果}
"#;
    let (关, 关题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: false,
            ..Passes::default()
        },
    );
    let (开, 开题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );

    assert_eq!(
        关.value_json()["r"],
        开.value_json()["r"],
        "开关推测程序的值必须一样"
    );
    println!(
        "共状态推测的账：关 {} 层 / {} 次调用（每次题数 {关题数:?}） → 开 {} 层 / {} 次调用（{开题数:?}）",
        关.layers.len(),
        关.cost.calls,
        开.layers.len(),
        开.cost.calls
    );

    // 共状态 + 融合：两侧的题应当与条件那道题合成**一次**调用
    assert_eq!(开.layers.len(), 1, "共状态该并成一层：{:?}", 开.layers);
    assert_eq!(
        开.cost.calls, 1,
        "共状态时推测应当是**零边际**：题免费、状态收费（P5）。实际 {} 次",
        开.cost.calls
    );
}

/// **推测的账有两栏：省了多少、白花了多少。只报第一栏是在报一半。**
/// `12` §4 第 8 行写着「推错记 `W-spec-unused`；超预算先丢推测」。
#[test]
fn 推错那一栏的账() {
    // 两侧状态不同：走了甲那侧，乙那侧的推测就白花了
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 甲 = state(mat("甲料"));
let 乙 = state(mat("乙料"));
let q = test("行吗", "k");
let 条件 = handle(cut(judge(state(mat("条件料")), q)), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    handle(cut(judge(甲, q)), {act: fn() { "甲" }, ignore: fn() { "甲" },
                               unsure: fn(u) { consume(u, "drop"); "甲" }})
} else {
    handle(cut(judge(乙, q)), {act: fn() { "乙" }, ignore: fn() { "乙" },
                               unsure: fn(u) { consume(u, "drop"); "乙" }})
};
{r: 结果}
"#;
    let (关, _) = 跑(
        src,
        0.9,
        Passes {
            speculate: false,
            ..Passes::default()
        },
    );
    let (开, 开题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: true,
            ..Passes::default()
        },
    );

    // 第一栏：省了多少层
    assert_eq!((关.layers.len(), 开.layers.len()), (2, 1), "省了一层");
    // 第二栏：白花了多少
    let 白花 = 开.cost.calls as i64 - 关.cost.calls as i64;
    assert!(白花 > 0, "走了甲那侧，乙那侧的推测白花了：{白花}");
    assert!(
        开.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-spec-unused")),
        "白花的那次要留痕（12 §4 第 8 行）：{:?}",
        开.trace.warnings
    );
    println!(
        "推测的两栏账：省 {} 层；白花 {白花} 次调用（关 {} 次 → 开 {} 次，每次题数 {开题数:?}）",
        关.layers.len() - 开.layers.len(),
        关.cost.calls,
        开.cost.calls
    );
    println!("**这个数不能跟 fuse 那笔比：程序形状不同**（那边是同状态多题，这边是异状态分支）。");
}

/// **消融臂必须关干净**：关推测时不能只停「跨分支提升」而让同一次登记的多题还在融合。
/// 一个没关干净的消融臂，差值是假的，**而且假得看不出来**。
#[test]
fn 消融臂关得干净() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 8};
let 同一个 = state(mat("同一份材料"));
let 条件 = handle(cut(judge(同一个, test("该继续吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 条件 {
    handle(cut(judge(同一个, test("甲问题", "k"))), {act: fn() { "甲" }, ignore: fn() { "甲" },
                                                    unsure: fn(u) { consume(u, "drop"); "甲" }})
} else { "没走" };
{r: 结果}
"#;
    // 关推测时：条件那题与分支里那题**必须分两次调用**——它们不在同一层，融合不该跨层
    let (关, 关题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: false,
            ..Passes::default()
        },
    );
    assert_eq!(
        关题数,
        vec![1, 1],
        "关推测时两道题各自一次调用，融合不跨层：{关题数:?}"
    );
    assert_eq!(关.layers.len(), 2);

    // 同时关推测与融合：仍是两次，且每次一题（证明这两个开关各管各的）
    let (全关, 全关题数) = 跑(
        src,
        0.9,
        Passes {
            speculate: false,
            fuse: false,
            ..Passes::default()
        },
    );
    assert_eq!(全关题数, vec![1, 1], "{全关题数:?}");
    let _ = 全关;
}
