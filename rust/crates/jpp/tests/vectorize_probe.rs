//! **循环向量化**（宪法登记表第 47 行，pass `vectorize`）。
//!
//! **动手前先量了四个形状**——`speculate` 那次的教训是**一个形状不足以给一条 pass 定性**。
//! 量完发现：**四个里只有一个有余量。**
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Interp, Passes};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};
use std::cell::RefCell;

/// 步 15c：原 `impl Client` 的记账桩改为三个闭包端口；`每次题数` 用 `RefCell` 借给判断端口写
fn 记账端口(每次题数: &RefCell<Vec<usize>>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("m", move |_s, qs| {
            每次题数.borrow_mut().push(qs.len());
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

fn 跑(src: &str) -> (u64, usize, Vec<usize>) {
    跑p(src, Passes::default())
}

fn 跑p(src: &str, passes: Passes) -> (u64, usize, Vec<usize>) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let 每次题数 = RefCell::new(vec![]);
    let mut calib = CalibStore::new();
    calib.put("k", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
    let mut l = Ledger::new();
    let acts = ActionRegistry::new();
    let b = program.budget.clone();
    let mut it = Interp::new(记账端口(&每次题数), &mut l, &calib, &acts, b);
    it.passes = passes;
    let o = it.run(&program).expect("跑得完");
    (o.cost.calls, o.layers.len(), 每次题数.borrow().clone())
}

#[test]
fn 量四个形状() {
    let 异状态无cut = r#"
budget {calls: 20, cost: 1, depth: 64};
{r: map([mat("甲"), mat("乙"), mat("丙")], fn(m) { judge(state(m), test("行吗","k")) })}
"#;
    let 同状态无cut = r#"
budget {calls: 20, cost: 1, depth: 64};
let s = state(mat("同一份"));
{r: map([1,2,3], fn(i) { judge(s, test("行吗","k")) })}
"#;
    let 异状态带cut = r#"
budget {calls: 20, cost: 1, depth: 64};
{r: map([mat("甲"), mat("乙"), mat("丙")], fn(m) {
    handle(cut(judge(state(m), test("行吗","k"))), {
        act: fn(){"a"}, ignore: fn(){"i"}, unsure: fn(u){consume(u,"drop");"u"}}) })}
"#;
    let 同状态带cut = r#"
budget {calls: 20, cost: 1, depth: 64};
let s = state(mat("同一份"));
{r: map([1,2,3], fn(i) {
    handle(cut(judge(s, test("行吗","k"))), {
        act: fn(){"a"}, ignore: fn(){"i"}, unsure: fn(u){consume(u,"drop");"u"}}) })}
"#;
    let 关 = Passes {
        vectorize: false,
        ..Passes::default()
    };
    for (名, src) in [
        ("异状态·无 cut", 异状态无cut),
        ("同状态·无 cut", 同状态无cut),
        ("异状态·带 cut", 异状态带cut),
        ("同状态·带 cut", 同状态带cut),
    ] {
        let (c1, l1, q1) = 跑p(src, 关.clone());
        let (c2, l2, q2) = 跑(src);
        println!(
            "{名}: 关 calls={c1} layers={l1} 题数={q1:?} → 开 calls={c2} layers={l2} 题数={q2:?}"
        );
        assert!(c2 <= c1, "**向量化不许把调用数变多**：{名} {c1} → {c2}");
        assert!(l2 <= l1, "**也不许把层数变多**：{名} {l1} → {l2}");
        assert_eq!(
            q2.iter().sum::<usize>(),
            q1.iter().sum::<usize>().min(q2.iter().sum::<usize>()),
            "**也不许把问的题数变多**：{名} {q1:?} → {q2:?}"
        );
    }

    // **唯一有余量的那个形状，差值要真的出现**——否则这一包什么也没做
    let (_, 关层, _) = 跑p(异状态带cut, 关);
    let (_, 开层, _) = 跑(异状态带cut);
    assert_eq!(
        (关层, 开层),
        (3, 1),
        "**异状态·带 cut：三层压成一层**，这是 vectorize 唯一的余量"
    );
}

/// **同一个账本键只问一次。**
///
/// 提前登记（`speculate` / `vectorize`）与真站点会登记同一个键。实测 `vectorize` 刚接上时
/// `[1,1,1]` 变成 `[2,2,1]`——**同一道题付了两次钱**。
/// **这不是 `vectorize` 独有的，`speculate` 走同一条路，只是以前没量到。**
#[test]
fn 同键不许问两次() {
    let src = r#"
budget {calls: 20, cost: 1, depth: 64};
let s = state(mat("同一份"));
{r: map([1,2,3], fn(i) { judge(s, test("行吗","k")) })}
"#;
    let (calls, _, 题数) = 跑(src);
    assert_eq!(calls, 1);
    assert_eq!(
        题数,
        vec![1],
        "**三轮问的是同一道题（体里没用到 i），只该问一次**：{题数:?}"
    );
}
