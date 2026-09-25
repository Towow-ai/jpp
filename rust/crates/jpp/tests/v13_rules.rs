//! `13-Rust实践反馈设计修订-v0.2.md` 的验收行为。
//!
//! `13` 声明「本文只替换下述冲突点，其余沿用 `12-IR与类契约-v0.1.md`」，所以现行依据是 `12` + `13`
//! 两份，`13` 优先。每条测试的名字对应 `13` 的一节，断言照该节**验收**那一段的原话写，
//! 不以测试条数代替设计验收。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, NoCallPorts, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

// ---------------------------------------------------------------- 测试替身

/// 固定答案 + **可指定费用**的本地替身。`13` §5 的验收明说「用本地替身返回高于预估的费用」，
/// 不需要付费实验。（步 15c：原 `impl Client` 的替身改为三个闭包端口，计数搬到调用处的 `RefCell`）
fn costly_ports<'a>(p: f64, cost_each: f64, calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 7,
                cost: cost_each,
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

fn program(src: &str) -> jpp::Program {
    let parsed = parse(src).unwrap_or_else(|d| panic!("解析失败：{}", d.render("t.jpp", src)));
    lower(&parsed).unwrap_or_else(|d| panic!("lower 失败：{}", d[0].render("t.jpp", src)))
}

fn calib() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.65, 0.35, 100, "上岗", Some(0.05))
        .expect("校准记录合法");
    c
}

// ---------------------------------------------------------------- §4 方法身份包括实际捕获状态

/// `13` §4 验收：「工厂返回正文相同而捕获值不同的方法，对同一输入产生各自正确结果；
/// 相同程序按已有记录重放不发生新的外部调用。」
///
/// 缺陷（PR #12 审查）：闭包只按**正文**哈希，而 `transform` 的账本键用它，
/// 于是工厂造出来的两个方法正文相同、捕获不同时，第二次 `transform` 会复用第一次的结果——**算错**。
#[test]
fn 第四条_工厂造的同正文不同捕获的方法各得各的结果() {
    let src = r#"
budget {calls: 0, cost: 0, depth: 16};
fn 工厂(k) { fn(old) -> Record { with(content(old), "k", k) } }
let m = mat({base: 1});
let 甲 = transform(工厂(1), m);
let 乙 = transform(工厂(2), m);
{甲: content(甲).k, 乙: content(乙).k}
"#;
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program(src),
        costly_ports(0.9, 0.0, &calls),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    assert_eq!(
        out.value_json(),
        serde_json::json!({"甲": 1, "乙": 2}),
        "捕获值不同的两个方法必须各得各的结果，不能把前一个的结果复用给后一个"
    );
}

/// `13` §4 验收的后半句：相同程序按已有记录重放，不发生新的外部调用。
#[test]
fn 第四条_同程序重放不发新调用() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 16};
fn 工厂(k) { fn(old) -> Record { with(content(old), "k", k) } }
let m = mat({base: 1});
let 甲 = transform(工厂(1), m);
let 乙 = transform(工厂(2), m);
let r = judge(state(m), test("行吗", "k"));
let e = cut(r);
consume(e, "drop");
{甲: content(甲).k, 乙: content(乙).k, 出口: exit_kind(e)}
"#;
    let p = program(src);
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let first = run(
        &p,
        costly_ports(0.9, 0.0, &calls),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("首跑");
    assert_eq!(*calls.borrow(), 1, "一道题一次调用");

    let again = run(
        &p,
        NoCallPorts::ports(),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("重放不该发调用：{}", e.render()));
    assert_eq!(
        again.value_json(),
        first.value_json(),
        "重放结果要与首跑一致"
    );
    assert_eq!(again.cost.calls, 0, "重放零新调用");
}

// ---------------------------------------------------------------- §5 调用前预算与调用后事实记账分开

/// `13` §5 验收：「用本地替身返回高于预估的费用：结果、费用被保存，程序停止新增调用；
/// 带该记录恢复不会再次请求。」
///
/// 缺陷（PR #12 审查）：旧顺序在调用返回后先核预算再记账，超了就直接退出——**钱花了，结果扔了**。
#[test]
fn 第五条_超预算的已完成调用要先记账再停() {
    let src = r#"
budget {calls: 5, cost: 0.001, depth: 8};
let r = judge(state(mat("a")), test("行吗", "k"));
let e = cut(r);
let r2 = judge(state(mat("b")), test("行吗", "k"));
let e2 = cut(r2);
{a: exit_kind(e), b: exit_kind(e2), 待: [e, e2]}
"#;
    let p = program(src);
    // 替身报的费用比预算高一个量级：调用已经发生了，钱已经花了
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &p,
        costly_ports(0.9, 0.01, &calls),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("超预算是停发不是错");

    assert_eq!(*calls.borrow(), 1, "调用确实发生了，第二次不再发");
    // 步 22-0（B93）：停的是下一步，而且是停发不是停程序——第二道题出口 Unsure(budget)
    assert_eq!(out.value_json()["b"], serde_json::json!("unsure(budget)"));
    assert!(out.budget.is_some(), "程序该因预算停发");
    assert!(
        !ledger.entries.is_empty(),
        "已完成调用的结果必须被保存，不能连同费用一起丢掉"
    );
    assert!(
        out.cost.usd >= 0.01 - 1e-9,
        "实际费用要记进来（记的是事实，不是预估），实际 {}",
        out.cost.usd
    );

    // 带该记录恢复：已完成的那次不再请求；停发的第二道题续跑时重发（B93）
    ledger.rebuild_index();
    let calls2 = RefCell::new(0);
    let again = run(
        &p,
        costly_ports(0.9, 0.0, &calls2),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("带记录恢复：{}", e.render()));
    assert_eq!(*calls2.borrow(), 1, "只重发停发的那道题");
    assert_eq!(again.cost.replayed, 1, "恢复时复用已完成记录，不重复付费");
}

// ---------------------------------------------------------------- §6 整数行为不随构建模式改变

/// `13` §6 验收：「最大整数加一、最小整数取负及除以负一、除零等相应路径在调试和优化构建下
/// 遵守同一规则。」——都要变成**指向 `.jpp` 源码的运行错误**，不能是 Rust panic，也不能悄悄回绕。
#[test]
fn 第六条_整数越界是指向源码的运行错误() {
    let mut calib = calib();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 跑 = |src: &str| {
        let p = program(src);
        let calls = RefCell::new(0);
        let mut ledger = Ledger::new();
        run(
            &p,
            costly_ports(0.9, 0.0, &calls),
            &CalibStore::new(),
            &ActionRegistry::new(),
            &mut ledger,
        )
    };
    let 该报错 = |src: &str, 说明: &str| {
        let src = format!("budget {{calls: 0, cost: 0, depth: 8}};\n{src}");
        match 跑(&src) {
            Err(e) => {
                let t = e.render();
                assert!(
                    t.contains("溢出")
                        || t.contains("除以零")
                        || t.contains("越界")
                        || t.contains("取模"),
                    "{说明}：报文要说清是什么边界，实际「{t}」"
                );
                // 位置要落在源码里
                let at = format!("{t}");
                assert!(at.contains("@"), "{说明}：诊断要带 Span，实际「{t}」");
            }
            Ok(o) => panic!("{说明}：本该是运行错误，却跑出了 {:?}", o.value_json()),
        }
    };
    该报错("9223372036854775807 + 1", "最大整数加一");
    该报错("(0 - 9223372036854775807 - 1) - 1", "最小整数减一");
    该报错("9223372036854775807 * 2", "最大整数乘二");
    该报错(
        "(0 - 9223372036854775807 - 1) / (0 - 1)",
        "最小整数除以负一",
    );
    该报错("1 / 0", "除以零");
    该报错("1 % 0", "模零");
}

// ---------------------------------------------------------------- §1 方法的构造与调用分开

/// `13` §1 验收：「返回一个未来调用 judge 的方法，返回步骤不产生 judge 调用；随后调用该方法才发生
/// judge。把它存进记录也得到相同区分。」——这是**运行期**的事实，不只是检查器的判断。
#[test]
fn 第一条_造方法不执行方法体() {
    let 造 = r#"
budget {calls: 3, cost: 1, depth: 8};
fn 造一个() { fn(t) -> Record { handle(cut(judge(state(mat(t)), test("行吗", "k"))), {
    act: fn() { {ok: true} }, ignore: fn() { {ok: false} }, unsure: fn(u) { consume(u, "drop"); {ok: unit} }}) } }
"#;
    let 跑 = |tail: &str| {
        let src = format!("{造}{tail}");
        let p = program(&src);
        let calls = RefCell::new(0);
        let mut ledger = Ledger::new();
        let out = run(
            &p,
            costly_ports(0.9, 0.0, &calls),
            &calib(),
            &ActionRegistry::new(),
            &mut ledger,
        )
        .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
        (out.cost.calls, *calls.borrow())
    };

    // 只造、只返回：一次调用都不该发生
    assert_eq!(跑("{有个方法: 1}").0, 0, "只定义方法，不该有 judge");
    assert_eq!(
        跑("let f = 造一个(); {有: 1}").1,
        0,
        "返回一个将来会 judge 的方法，返回这一步不产生 judge"
    );
    // 存进记录同样
    assert_eq!(
        跑("let 盒 = {go: 造一个()}; {有: 1}").1,
        0,
        "把它存进记录也得到相同区分"
    );
    // 真调用了才发生
    assert_eq!(
        跑("let f = 造一个(); f(\"a\").ok").1,
        1,
        "随后调用该方法才发生 judge"
    );
    assert_eq!(
        跑("let 盒 = {go: 造一个()}; 盒.go(\"a\").ok").1,
        1,
        "经记录字段调用也发生"
    );
}

// ---------------------------------------------------------------- §2 效应在调用处实例化

/// `13` §2 验收的后半句是一条**硬要求**，不是我先前报的「边界」：
/// 「显式效应集合是调用者可依赖的上界，**不能因为某次实参恰为纯而悄悄缩小作者声明**。」
///
/// 所以 `fn apply(m, f) -> Record !{judge}` 之下，哪怕某个调用点只传纯方法，
/// 调用者仍要按 `{judge}` 算——这不是误报，是依据规定的语义。这条钉住它，防止以后被「优化」掉。
#[test]
fn 第二条_显式效应集合是上界不能被实参缩小() {
    use jpp::check;
    let base = r#"
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "k"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                    unsure: fn(u) { consume(u, "drop"); {ok: unit} }})
}
fn plain(m) -> Record !{} { {ok: true} }
"#;
    // 作者显式声明了上界 !{judge}：纯调用点也得按上界算
    let 声明了上界 = format!(
        "budget {{calls: 2, cost: 0, depth: 8}};{base}
fn apply(m, f) -> Record !{{judge}} {{ f(m) }}
fn 纯用法(m) -> Record !{{}} {{ apply(m, plain) }}
{{a: 纯用法(mat({{x: 1}})).ok}}"
    );
    let report = check(&program(&声明了上界));
    let d = report
        .find("E-effect")
        .unwrap_or_else(|| panic!("上界不该被实参缩小：\n{}", report.render()));
    assert!(d.message.starts_with("纯用法"), "{}", d.message);

    // 作者没有声明上界（缺省 = 推断）：这时才按调用点实例化，纯调用点不背 judge
    let 没声明 = 声明了上界.replace(
        "fn apply(m, f) -> Record !{judge} { f(m) }",
        "fn apply(m, f) { f(m) }",
    );
    let report = check(&program(&没声明));
    assert!(
        report.is_ok(),
        "没声明上界时按调用点实例化，纯调用点不该被报：\n{}",
        report.render()
    );
}

// ---------------------------------------------------------------- §3 未决结果按实际去向传递

/// `13` §3 验收：「未决可包在记录或继续方法中返回，再由调用者接续」。
#[test]
fn 第三条_未决包进记录返回再由调用者接续() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
fn 看(m) -> Record { handle(cut(judge(state(m), test("行吗", "k"))), {
    act: fn() { {状态: "act", 待: unit} },
    ignore: fn() { {状态: "ignore", 待: unit} },
    unsure: fn(u) { {状态: "unsure", 待: unsure(u)} }}) }
let 包 = 看(mat("a"));
if 包.状态 == "unsure" { consume(包.待, "drop") } else { unit };
{状态: 包.状态}
"#;
    let p = program(src);
    // p = 0.5 落在带内 → unsure；责任包进记录返回，调用者接续
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &p,
        costly_ports(0.5, 0.0, &calls),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("包进记录返回、调用者消费，应当跑完：{}", e.render()));
    assert_eq!(out.value_json(), serde_json::json!({"状态": "unsure"}));
    assert!(
        out.returned_unsure.is_empty(),
        "已经被调用者消费了，不该还挂着：{:?}",
        out.returned_unsure
    );
}

/// `13` §3 验收：「取字段/过滤导致最后一份承接信息丢失时应显式处理或报错」。
///
/// 两个案例的粒度在中间，不是都放过也不是都拦下（总控 2026-09-21 裁定）：
/// **责任没出现在返回值里 = 最后一份承接信息被丢了 → 错**（这条测的就是它）；
/// **责任如实出现在返回值里、只是返回类型没提 Exit → 警告**（`W-untyped-transfer`，
/// 报文直接给修法），样例标注补齐后升为错。
#[test]
fn 第三条_取字段丢掉最后一份承接信息要报错() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
fn 看(m) -> Record { handle(cut(judge(state(m), test("行吗", "k"))), {
    act: fn() { {状态: "act", 待: unit} },
    ignore: fn() { {状态: "ignore", 待: unit} },
    unsure: fn(u) { {状态: "unsure", 待: unsure(u)} }}) }
let 包 = 看(mat("a"));
{只要状态: 包.状态}
"#;
    let p = program(src);
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    match run(
        &p,
        costly_ports(0.5, 0.0, &calls),
        &calib(),
        &ActionRegistry::new(),
        &mut ledger,
    ) {
        Err(e) => assert!(e.render().contains("J-05"), "该是 J-05：{}", e.render()),
        Ok(out) => panic!(
            "只取了 状态 字段、把带着未决的 待 字段丢了，这是最后一份承接信息：{:?} / returned_unsure={:?}",
            out.value_json(),
            out.returned_unsure
        ),
    }
}
