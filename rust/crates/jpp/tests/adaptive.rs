//! 自适应选问：1000 个候选、固定目标 731、源码按前一次答案构造下一道二分题，至多 10 问。
//!
//! 要点：题是**值**（`bisect_asker` 返回一个造题的方法，`probe` 把题的构造方法当参数收），
//! 观察是固定记录不是模型，循环的 bound 写在源码里，停机由 `stop` 显式给出。

mod common;

use common::*;
use jpp::effects::{CalibStore, FixedPorts, NoCallPorts};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{Program, run};
use serde_json::json;

/// 题面的前缀：源码里用它拼题，测试用它算固定观察的键——只有一处定义
const PREFIX: &str = "目标是否不大于 ";
const CALIB: &str = "bisect";
const TARGET: i64 = 731;
const LOW: i64 = 1;
const HIGH: i64 = 1000;

/// 源码里的二分与这里的模拟必须一致：`mid = (lo + hi) / 2`，问「target ≤ mid ?」
fn steps() -> Vec<(i64, i64, i64, bool)> {
    let (mut lo, mut hi) = (LOW, HIGH);
    let mut out = vec![];
    while lo < hi {
        let mid = (lo + hi) / 2;
        let yes = TARGET <= mid;
        out.push((lo, hi, mid, yes));
        if yes { hi = mid } else { lo = mid + 1 }
    }
    out
}

fn fixed_client() -> FixedPorts {
    let mut client = FixedPorts::new();
    for (lo, hi, mid, yes) in steps() {
        let state = State::new(
            vec![Mat::literal(json!({"lo": lo, "hi": hi}))],
            vec![],
            vec![],
            vec![],
            false,
        );
        let question = Question::new(Op::Test, &format!("{PREFIX}{mid}"), CALIB, vec![]);
        // 线在 0.1 / 0.9；0.99 过上线是 act，0.01 过下线是 ignore
        client.observe(
            &state,
            &question,
            Answer::Noul(if yes { 0.99 } else { 0.01 }),
        );
    }
    client
}

fn calibrations() -> CalibStore {
    let mut calib = CalibStore::new();
    calib
        .put(CALIB, 0.9, 0.1, 120, "上岗", Some(0.05))
        .expect("校准记录合法");
    calib
}

/// ```text
/// budget {calls: 10, cost: 0, depth: 64};
/// fn bisect_asker(calib) { fn(bound) { test("目标是否不大于 " + text(bound), calib) } }
/// fn probe(lo, hi, make_q) -> Record !{judge} {
///     let mid = (lo + hi) / 2;
///     let st = state(mat({lo: lo, hi: hi}));
///     let e = cut(judge(st, make_q(mid)));
///     handle(e, {act: fn() {{lo: lo, hi: mid}},
///                ignore: fn() {{lo: mid + 1, hi: hi}},
///                unsure: fn(cause) {{lo: lo, hi: hi, stuck: cause}}})
/// }
/// fn search(start, make_q) -> Record !{judge} {
///     loop(10, start, fn(s, i) {
///         let n = probe(s.lo, s.hi, make_q);
///         let next = {lo: n.lo, hi: n.hi, asked: s.asked + 1};
///         if next.lo == next.hi { stop(next) } else { next }
///     })
/// }
/// let below = bisect_asker("bisect");
/// let found = search({lo: 1, hi: 1000, asked: 0}, below);
/// {target: found.lo, asked: found.asked}
/// ```
fn adaptive_program() -> Program {
    program(
        Some(budget(10, 64)),
        vec![
            // 方法返回方法：造题的方法本身是值
            func(
                "bisect_asker",
                &["calib"],
                body(
                    vec![],
                    lambda(
                        &["bound"],
                        body(
                            vec![],
                            call(
                                "test",
                                vec![
                                    bin("+", text(PREFIX), call("text", vec![name("bound")])),
                                    name("calib"),
                                ],
                            ),
                        ),
                    ),
                ),
            ),
            // 方法收方法：make_q 是参数
            func_eff(
                "probe",
                &["lo", "hi", "make_q"],
                &["judge"],
                body(
                    vec![
                        bind("mid", bin("/", bin("+", name("lo"), name("hi")), int(2))),
                        bind(
                            "st",
                            call(
                                "state",
                                vec![call(
                                    "mat",
                                    vec![rec(vec![("lo", name("lo")), ("hi", name("hi"))])],
                                )],
                            ),
                        ),
                        bind(
                            "e",
                            call(
                                "cut",
                                vec![call(
                                    "judge",
                                    vec![name("st"), call_of(name("make_q"), vec![name("mid")])],
                                )],
                            ),
                        ),
                    ],
                    call(
                        "handle",
                        vec![
                            name("e"),
                            rec(vec![
                                (
                                    "act",
                                    lambda(
                                        &[],
                                        body(
                                            vec![],
                                            rec(vec![("lo", name("lo")), ("hi", name("mid"))]),
                                        ),
                                    ),
                                ),
                                (
                                    "ignore",
                                    lambda(
                                        &[],
                                        body(
                                            vec![],
                                            rec(vec![
                                                ("lo", bin("+", name("mid"), int(1))),
                                                ("hi", name("hi")),
                                            ]),
                                        ),
                                    ),
                                ),
                                (
                                    "unsure",
                                    lambda(
                                        &["cause"],
                                        body(
                                            vec![],
                                            rec(vec![
                                                ("lo", name("lo")),
                                                ("hi", name("hi")),
                                                ("stuck", name("cause")),
                                            ]),
                                        ),
                                    ),
                                ),
                            ]),
                        ],
                    ),
                ),
            ),
            func_eff(
                "search",
                &["start", "make_q"],
                &["judge"],
                body(
                    vec![],
                    call(
                        "loop",
                        vec![
                            int(10),
                            name("start"),
                            lambda(
                                &["s", "i"],
                                body(
                                    vec![
                                        bind(
                                            "n",
                                            call(
                                                "probe",
                                                vec![
                                                    field(name("s"), "lo"),
                                                    field(name("s"), "hi"),
                                                    name("make_q"),
                                                ],
                                            ),
                                        ),
                                        bind(
                                            "next",
                                            rec(vec![
                                                ("lo", field(name("n"), "lo")),
                                                ("hi", field(name("n"), "hi")),
                                                (
                                                    "asked",
                                                    bin("+", field(name("s"), "asked"), int(1)),
                                                ),
                                            ]),
                                        ),
                                    ],
                                    if_(
                                        bin(
                                            "==",
                                            field(name("next"), "lo"),
                                            field(name("next"), "hi"),
                                        ),
                                        call("stop", vec![name("next")]),
                                        name("next"),
                                    ),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            bind("below", call("bisect_asker", vec![text(CALIB)])),
            bind(
                "found",
                call(
                    "search",
                    vec![
                        rec(vec![("lo", int(LOW)), ("hi", int(HIGH)), ("asked", int(0))]),
                        name("below"),
                    ],
                ),
            ),
        ],
        rec(vec![
            ("target", field(name("found"), "lo")),
            ("asked", field(name("found"), "asked")),
        ]),
    )
}

#[test]
fn 二分选问十题定位目标() {
    let program = adaptive_program();
    assert_clean(&program);
    assert_eq!(steps().len(), 10, "1000 个候选二分恰好十问");

    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        client.ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(outcome.value_json(), json!({"target": TARGET, "asked": 10}));
    assert!(outcome.pending.is_empty(), "没有未决出口");
    assert_eq!(
        outcome.cost.calls, 10,
        "十问就是十次调用，正好用完 budget.calls"
    );
    assert_eq!(client.judge.log.len(), 10, "十次固定观察全部命中");
    assert_eq!(outcome.trace.count("judge", false), 10);
    // **断的是它真正要断的那三条，不是「一条告警都没有」。**
    // 原来写的是 `warnings.is_empty()`，于是任何一条新告警都会把它打红——
    // 而这里的线是手填的 `n=120`、没有证书，`W-fixture-line` 本来就该响。
    // **一个「不许有任何告警」的断言，会逼着后来的人去掐掉正确的告警。**
    // 步 15d-2：没带画像时每个判断站点都会多一条 W-window-untested（§3.9），同样是常规记账。
    for w in &outcome.trace.warnings {
        assert!(
            w.starts_with("W-fixture-line") || w.starts_with("W-window-untested"),
            "不该有 W-bound / W-header / returned_unsure：{:?}",
            outcome.trace.warnings
        );
    }
}

#[test]
fn 同程序重放零调用() {
    let program = adaptive_program();
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let first = run(
        &program,
        client.ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("首跑");

    // 同一本账本 + 拒绝一切调用的端口表：命中即重放，发一次请求就是错
    let again = run(
        &program,
        NoCallPorts::ports(),
        &calibrations(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("重放不该发调用：{}", e.render()));

    assert_eq!(again.value_json(), first.value_json());
    assert_eq!(again.cost.calls, 0, "重放零调用");
    assert_eq!(again.cost.replayed, 10, "十次判断全部命中账本");
    assert_eq!(again.trace.count("judge", true), 10);
    // 步 15d-2：没带画像时每个判断站点都会多一条 W-window-untested（§3.9），同样是常规记账。
    for w in &again.trace.warnings {
        assert!(
            w.starts_with("W-fixture-line") || w.starts_with("W-window-untested"),
            "账本头一致，不该有 W-header：{:?}",
            again.trace.warnings
        );
    }
}

/// 错的效应标注要拦得住，位置指着那个方法的定义。
///
/// `probe` 的 `make_q` 是参数（题的构造方法从外面传进来），`search` 更是只在 loop 的回调里
/// 间接调 `probe`——首包这两处都因为「被调者解析不了」整条跳过检查，现在核得住。
#[test]
fn 错的效应标注拦得住() {
    let program = adaptive_program();
    for (name, real) in [("probe", &["judge"][..]), ("search", &["judge"][..])] {
        let (bad, at) = with_effects(&program, name, &[]);
        let report = jpp::check(&bad);
        let d = report
            .diagnostics
            .iter()
            .find(|d| d.rule == "E-effect" && d.message.starts_with(name))
            .unwrap_or_else(|| panic!("{name} 标成 !{{}} 应当报 E-effect：\n{}", report.render()));
        assert_eq!(d.span, at);
        assert!(d.message.contains("judge"), "{}", d.message);

        let (good, _) = with_effects(&program, name, real);
        assert!(
            jpp::check(&good).is_ok(),
            "标对了不该被报：{}",
            jpp::check(&good).render()
        );
    }
}
