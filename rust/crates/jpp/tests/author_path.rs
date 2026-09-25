//! **从外面走 `.jpp` 这条路的人撞到的四件。**
//!
//! 共同形状：**内核手里有答案，只是没说出口。**
//! 「说不准」与「说不出」是两回事——这几条全是后者，**而后者是免费可修的**。

mod common;
use std::cell::RefCell;

use jpp::effects::{
    CalibStore, CallInput, EffectError, EffectId, FixedPorts, FnPort, GenResult, JudgeResult, Ports,
};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, Op, Question, State, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// **一（最值钱，且免费）：固定观察未命中，要打出内核自己算的那份状态与题。**
///
/// 以前只给哈希，于是作者猜 `measure` 的档位字段名猜了四次
/// （`bands` / `levels` / `grades` / `options` 全不中，最后是 `scale`），
/// **每次拿到同一句话、同一个哈希，没有任何梯度**。
#[test]
fn 固定观察未命中要给出状态与题() {
    let mut fp = FixedPorts::new();
    let mut ports = fp.ports();
    let s = State::new(
        vec![jpp::value::Mat::literal(Json::String("材料甲".into()))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let q = Question::new(
        Op::Measure,
        "多大把握",
        "k",
        vec!["低".into(), "中".into(), "高".into()],
    );
    let Err(e) = ports.call(
        EffectId::Judge,
        CallInput::StateQuestions {
            state: s,
            questions: vec![q],
        },
    ) else {
        panic!("没喂记录就该报错")
    };

    assert!(
        e.0.contains("材料甲"),
        "**要打出状态里到底装了什么**：{}",
        e.0
    );
    assert!(
        e.0.contains("scale") && e.0.contains("高"),
        "**要打出档位字段叫什么、装了什么**：{}",
        e.0
    );
    assert!(e.0.contains("多大把握"), "{}", e.0);
    println!("{}", e.0);
}

/// **四：修法要标明谁能执行。**
/// `W-fixture-line` 的「走 `commission`」、`W-untested` 的「开置换」——
/// **读者是 `.jpp` 作者，执行者是 Rust 接线人。**
/// 而同一个前缀下 `W-untested(cold)` 的「给这个键写上岗记录」作者做得到——
/// **一条可执行、一条不可执行，长得一模一样。**
#[test]
fn 修法要标明谁能执行() {
    // 步 15c：原 `impl Client` 的桩改为三个闭包端口；`calls` 用 `RefCell` 记调用数（未被断言，保留计数是为了与原桩同纪律）
    let 跑 = |calib: &CalibStore| {
        let program = lower(
            &parse(
                r#"
budget {calls: 4, cost: 1};
handle(cut(judge(state(mat("材料")), test("行吗", "k"))), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#,
            )
            .expect("解析"),
        )
        .expect("lower");
        let mut l = Ledger::new();
        let calls = RefCell::new(0u64);
        run(
            &program,
            Ports::new()
                .with(FnPort::judge("m", |_s, qs| {
                    *calls.borrow_mut() += 1;
                    Ok(JudgeResult {
                        answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
                        tokens: 0,
                        cost: 0.0,
                        mode_share: vec![],
                        perms: vec![],
                    })
                }))
                .with(FnPort::generate("m", |_p, _c, _n, _r| {
                    Err(EffectError("x".into()))
                }))
                .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into())))),
            calib,
            &ActionRegistry::new(),
            &mut l,
        )
        .expect("跑得完")
        .trace
        .warnings
        .clone()
    };

    // 冷键 → 作者做得到（写一条上岗记录）
    let w = 跑(&CalibStore::new());
    let cold = w.iter().find(|x| x.contains("calib_line")).expect("有这条");
    assert!(
        cold.contains("【作者可改】"),
        "**这一条作者做得到，要说出来**：{cold}"
    );

    // 手填的线 + 强出口 → 接线人才做得到
    let mut 手填 = CalibStore::new();
    手填.put("k", 0.8, 0.2, 1, "上岗", Some(0.05)).unwrap();
    let w2 = 跑(&手填);
    let unc = w2
        .iter()
        .find(|x| x.starts_with("W-fixture-line"))
        .expect("有这条");
    assert!(
        unc.contains("【需接线人】"),
        "**`commission` 是 Rust API，作者调不到**：{unc}"
    );
    assert!(unc.contains("调不到") || unc.contains("接线人"), "{unc}");
}

/// **J-08 那条：核一下作者「一次都没见过 J-08」是哪个原因。**
///
/// 答案是**两个原因叠着**，而且都不是「taint 没跟到」：
/// 1. CLI 只登记了三个动作，**其中只有 `write_json` 不可逆**（J-08 只管不可逆）；
/// 2. **`gen` 的输出 taint = ∨ ctx.taint**（`12`:269），**没有 ctx 时就是 trusted**
///    ——那是**单位元不是兜底值**（`12`:282 亲口点名的正确折叠），所以守卫是可信的、J-08 照规矩放行。
#[test]
fn j08没触发的两个原因各自独立成立() {
    // 步 15c：原 `impl Client` 的桩改为三个闭包端口
    fn 端口() -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate("m", |_p, _c, n, _r| {
                Ok(GenResult {
                    outputs: (0..n).map(|_| Json::String("生成的材料".into())).collect(),
                    tokens: 0,
                    cost: 0.0,
                })
            }))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
    let 跑 = |可逆: bool| {
        let mut a = ActionRegistry::new();
        a.register("动作", 0.0, 可逆, TaintOut::Trusted, |_| {
            Ok(Value::Text(
                "做了".into(),
                jpp::value::Taint::Trusted.into(),
            ))
        });
        let program = lower(
            &parse(
                r#"
budget {calls: 4, cost: 1, depth: 8};
let 料 = gen("写一句", [], 1, 0);
handle(cut(judge(state(料[0]), test("该做吗", "k"))), {
    act: fn() { content(do("动作", [], 0)) },
    ignore: fn() { "没做" },
    unsure: fn(u) { consume(u, "drop"); "没做" }})
"#,
            )
            .expect("解析"),
        )
        .expect("lower");
        let mut calib = CalibStore::new();
        common::certified(&mut calib, "k", 0.8, 0.2, 50);
        let mut l = Ledger::new();
        run(&program, 端口(), &calib, &a, &mut l)
            .map(|o| o.value_json())
            .map_err(|e| e.render())
    };

    // 原因一：可逆的动作 J-08 压根不管——**这是设计，不是缺陷**
    assert!(跑(true).is_ok(), "可逆动作不受 J-08 管");
    // 原因二：**`gen` 无 ctx → trusted**，所以连不可逆动作也照常放行
    assert!(
        跑(false).is_ok(),
        "**gen 无 ctx 时输出是 trusted（∨ 的单位元），守卫可信，J-08 照规矩放行**"
    );
}

/// **作者要查得到哪些动作不可逆**——否则 J-08 保护的边界作者根本看不见。
/// **一个作者无法查询的安全边界，等于没有边界。**
#[test]
fn 作者查得到哪些动作不可逆() {
    // 步 15c：原 `impl Client` 的桩改为三个闭包端口，全部报错（这条程序不该发任何效应调用）
    let mut a = ActionRegistry::new();
    a.register("读一下", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::Text("x".into(), jpp::value::Taint::Trusted.into()))
    });
    a.register("发出去", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("x".into(), jpp::value::Taint::Trusted.into()))
    });
    let program =
        lower(&parse("budget {calls:0,cost:0}; do(\"打错的名字\", [], 0)").expect("解析"))
            .expect("lower");
    let mut l = Ledger::new();
    let e = run(
        &program,
        Ports::new()
            .with(FnPort::judge("m", |_s, _qs| Err(EffectError("x".into()))))
            .with(FnPort::generate("m", |_p, _c, _n, _r| {
                Err(EffectError("x".into()))
            }))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into())))),
        &CalibStore::new(),
        &a,
        &mut l,
    )
    .expect_err("未登记该报错")
    .render();
    assert!(e.contains("读一下（可逆）"), "{e}");
    assert!(
        e.contains("发出去（**不可逆**）"),
        "**不可逆的那些要看得见，那是 J-08 的边界**：{e}"
    );
}
