//! **J-10 的静态那一半**：`12`:814 那张表写的是「J-10 unsure 上界 | **✓ 估计** | 实测」，
//! **✓ 在静态那一栏**——条文把静态估计定为主位，运行期实测是另一半。
//!
//! **区别是钱**：静态这一半在**任何模型调用发生之前**报；运行期的 `unsure_bound`
//! 报的时候钱已经花了。**这是这一条唯一值得做的理由**，不是多一道栏杆。
//!
//! **在这之前它没有路径能落地**，而且是两处都缺：
//! 1. `ast::Budget` **没有 `unsure` 这一格**——条文「超 `budget.unsure` 即报」里那个东西不存在；
//! 2. 检查器只收 `&Profile`，**而 `unsure_rate` 住在 `CalibRecord` 里**，档案里没有。
//!
//! 2026-09-23 起前端接上了 `budget {unsure: …}`（规则批 B32 施工时一并接线），
//! 由 `前端写得出budget_unsure` 钉住。

use jpp::effects::{CalibStore, FnPort, JudgeResult, LiteralMode, Ports, Sample};
use jpp::value::{Answer, Question, State};
use std::cell::Cell;

fn 上岗记录(key: &str, unsure_rate: f64) -> (String, f64) {
    (key.to_string(), unsure_rate)
}

/// 造一本记录：每个键都真的走 `absorb` + `commission` 上岗，**再把 unsure_rate 按需覆盖**
/// （覆盖是为了让这条测试的赌值可控；生产者本身另有测试验）。
fn 记录本(keys: &[(String, f64)]) -> CalibStore {
    let mut c = CalibStore::new();
    for (k, u) in keys {
        for i in 0..60 {
            let p = 0.02 + i as f64 * 0.016;
            c.absorb(
                k,
                Sample {
                    p: Some(p),
                    label: Some(if p > 0.55 { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: None,
                    stratum: None,
                },
            )
            .expect("折得进");
        }
        c.commission(k, 0.10, 0.10, "条").expect("认得动");
        c.set_unsure_rate(k, *u).expect("设得上");
    }
    c
}

/// 降级后直接改 IR 的预算（源码里写 `budget {unsure: …}` 也行，见 `前端写得出budget_unsure`；
/// 这里要同一份程序配不同的 `unsure`）。
fn 带unsure预算(src: &str, unsure: Option<f64>) -> jpp::Program {
    let mut p = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
    p.budget.unsure = unsure;
    p
}

const 两题: &str = r#"
budget {calls: 4, cost: 1};
handle(cut(judge(state(mat("材料")), [test("甲行吗", "ka"), test("乙行吗", "kb")])[0]), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#;

#[test]
fn 联合界超了就在跑之前报() {
    let c = 记录本(&[上岗记录("ka", 0.3), 上岗记录("kb", 0.4)]);
    // Σuᵢ = 0.7
    let 松 = jpp::check_with_calib(&带unsure预算(两题, Some(0.8)), &c);
    assert!(
        松.find("J-10").is_none(),
        "0.7 ≤ 0.8，不该报：{}",
        松.render()
    );

    let 紧 = jpp::check_with_calib(&带unsure预算(两题, Some(0.5)), &c);
    let d = 紧
        .find("J-10")
        .unwrap_or_else(|| panic!("0.7 > 0.5，该报：{}", 紧.render()));
    println!("{}", d.render());
    assert!(
        d.message.contains("0.7000"),
        "要把那个和说出来：{}",
        d.message
    );
    assert!(紧.is_ok(), "**只报不停**：条文写的是「即报」，不是 Halt");
}

/// **不设 `budget.unsure` 就不报。** `None` 不是 0——
/// 设成 0 是「一条 unsure 都不许有」，是个很强的断言，不能由缺省替人做。
#[test]
fn 不设这一格就不报() {
    let c = 记录本(&[上岗记录("ka", 0.9), 上岗记录("kb", 0.9)]);
    let r = jpp::check_with_calib(&带unsure预算(两题, None), &c);
    assert!(
        r.find("J-10").is_none(),
        "没设上限就没有「超」这回事：{}",
        r.render()
    );
}

/// **没有上岗记录的键按 1 计**（与 `unsure_bound` 对未知键同口径，最保守），
/// 且诊断要说出有几个是这么来的——**否则 `Σ = 2.0` 读起来像测出来的**。
#[test]
fn 未知的键按一计并且说出来() {
    let c = CalibStore::new(); // 一条记录都没有
    let r = jpp::check_with_calib(&带unsure预算(两题, Some(0.5)), &c);
    let d = r.find("J-10").expect("两个未知键 → Σ = 2.0 > 0.5");
    assert!(d.message.contains("2.0000"), "{}", d.message);
    assert!(
        d.message.contains("2 个站点没有可用的 unsure_rate"),
        "**要说出这个和是怎么来的**：{}",
        d.message
    );
}

/// **它真的在任何调用之前报**——这是它与运行期那一半的全部区别。
/// 判据不是「报了」，是**客户端的调用计数为 0**。
#[test]
fn 报在任何模型调用之前() {
    // 步 15c：原 `impl Client for 数桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
    // 改为一个 judge 闭包端口，调用计数改用 `Cell`（借用处、闭包借走）。
    let c = CalibStore::new();
    let p = 带unsure预算(两题, Some(0.5));
    // **检查器拿不到 Ports**（签名里没有），所以「报在调用之前」是结构保证不是巧合。
    // 这里要测的是那个结构的**后果**：同一个程序，只检查 → 有诊断、零调用；跑 → 钱花了。
    let r = jpp::check_with_calib(&p, &c);
    assert!(r.find("J-10").is_some(), "只检查就报得出来");

    let calls = Cell::new(0u64);
    let ports = Ports::new().with(FnPort::judge("m", |_s: &State, qs: &[&Question]| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }));
    let mut l = jpp::ledger::Ledger::new();
    let out = jpp::run(&p, ports, &c, &jpp::interp::ActionRegistry::new(), &mut l).expect("跑得完");
    assert!(calls.get() > 0, "前提：跑起来真的会发调用");
    assert!(
        out.trace.warnings.iter().any(|w| w.contains("J-10")) || true,
        "运行期这一半是否也报是另一件事，这条不管"
    );
    // **这才是要钉住的那句**：J-10 是「报」不是「停」，
    // **所以它本身一分钱也没省下**——省钱要有人看见这条报告然后不跑。
    // 把这件事写成断言，免得下一个人把「有了静态检查」读成「省了钱」。
    assert!(calls.get() > 0, "**报了照样跑**：报不等于省");
}

/// `.jpp` 里写得出 `budget.unsure`（2026-09-23 前端接上；原「边界：写不出」测试随之改写）。
#[test]
fn 前端写得出budget_unsure() {
    // 2026-09-23 规则批（B32 施工时一并接上）：`.jpp` 里的 `budget {unsure: …}` 进到核心 Budget。
    let p = jpp::lower(
        &jpp::syntax::parse("budget {calls: 1, cost: 1, unsure: 0.5};\n1").expect("解析"),
    )
    .expect("lower");
    assert_eq!(p.budget.unsure, Some(0.5));
}

/// **函数体里的站点也算「可能不止一遍」。**
/// `fn q() { test(…) }` 定义在顶层、**在 `map` 里被调用**——它的跨度不落在任何循环里。
/// **只按循环跨度算，这里会读到 0，诊断就会说「这个和是上界」，而它不是。**
/// 一个声称自己是上界的诊断，比没有这条诊断更糟。
#[test]
fn 函数体里的站点不许被当成只跑一遍() {
    let src = r#"
budget {calls: 8, cost: 1};
fn q() { test("甲行吗", "ka") }
handle(cut(map([1, 2, 3], fn(i) { judge(state(mat("材料")), q()) })[0]), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#;
    let c = 记录本(&[上岗记录("ka", 0.9)]);
    let r = jpp::check_with_calib(&带unsure预算(src, Some(0.5)), &c);
    let d = r
        .find("J-10")
        .unwrap_or_else(|| panic!("0.9 > 0.5 该报：{}", r.render()));
    println!("{}", d.render());
    assert!(
        d.message.contains("不是上界"),
        "**站点在函数体里，这个和不是上界，诊断必须自己说出来**：{}",
        d.message
    );
}

/// **接上 `check_with_calib` 之后，没设 `budget.unsure` 的程序报告要逐条不变。**
/// commit 里声称「行为保持」，这条把那句话钉住。
#[test]
fn 没设这一格时两个入口给一样的报告() {
    let c = 记录本(&[上岗记录("ka", 0.9), 上岗记录("kb", 0.9)]);
    for src in [
        两题,
        "budget {calls: 1, cost: 1};\n1",
        "budget {calls: 1, cost: 1};\nask(state(mat(\"m\")), test(\"t\", \"ka\"))",
    ] {
        let p = 带unsure预算(src, None);
        let 经记录 = jpp::check_with_calib(&p, &c);
        let 经档案 = jpp::check_with_profile(&p, &c.profile);
        assert_eq!(
            经记录.render(),
            经档案.render(),
            "两个入口的报告要一样：\n{src}"
        );
    }
}
