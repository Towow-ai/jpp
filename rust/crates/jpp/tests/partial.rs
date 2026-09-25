//! 部分候选检验 → 精确组合 → 续解。
//!
//! A(4, web)、B(5, db) 一问即收；C(2, web+db)、D(20, web) 落在线中间是未决。
//! 第一轮用 A+B 交付成本 9；给 C 补材料（新材料 = 新观察身份）后只补 C 得成本 2，D 仍未决；
//! 再换一种问法（分档题 + 另一套校准）处理 D。共 6 次固定观察、3 次本地检查，旧检查不重做。
//!
//! 组合是源码里写的：幂集用 fold 展开，约束用 filter，最省用 fold；内核没有候选求解命令。
//! 「接着做」是语言里的方法值：`packet` 把算法状态和下一步策略封在一个记录里返回。

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jpp::effects::{CalibStore, FixedPorts, NoCallPorts};
use jpp::interp::{ActionRegistry, Interp, Passes, TaintOut};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{Program, run};
use serde_json::{Value as Json, json};

const ASK: &str = "这个候选现在可用吗？";
const GRADE: &str = "这个候选的把握有多大？";
const ACCEPT: &str = "accept";
const CONF: &str = "confidence";
const LEVELS: [&str; 3] = ["low", "mid", "high"];

fn candidate(name: &str, cost: i64, skills: &[&str]) -> Json {
    json!({"name": name, "cost": cost, "skills": skills})
}

fn seed_test(client: &mut FixedPorts, on: Json, p: f64) {
    let state = State::new(vec![Mat::literal(on)], vec![], vec![], vec![], false);
    client.observe(
        &state,
        &Question::new(Op::Test, ASK, ACCEPT, vec![]),
        Answer::Noul(p),
    );
}

fn fixed_client() -> FixedPorts {
    let mut client = FixedPorts::new();
    // 第一轮：四个候选各一次观察。A、B 过上线；C、D 落在线中间 → Unsure(band)
    seed_test(&mut client, candidate("A", 4, &["web"]), 0.95);
    seed_test(&mut client, candidate("B", 5, &["db"]), 0.92);
    seed_test(&mut client, candidate("C", 2, &["web", "db"]), 0.50);
    seed_test(&mut client, candidate("D", 20, &["web"]), 0.50);
    // 第二轮：C 有了补充材料，是另一个观察身份
    seed_test(
        &mut client,
        json!({"name": "C", "cost": 2, "skills": ["web", "db"], "evidence": "supplement"}),
        0.97,
    );
    // 第三轮：D 换成分档题 + 另一套校准。同一份材料、换一道题，仍是新的观察身份
    let state = State::new(
        vec![Mat::literal(candidate("D", 20, &["web"]))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let question = Question::new(
        Op::Measure,
        GRADE,
        CONF,
        LEVELS.iter().map(|s| s.to_string()).collect(),
    );
    client.observe(&state, &question, Answer::Score(vec![0.85, 0.10, 0.05]));
    client
}

fn calibrations() -> CalibStore {
    let mut calib = CalibStore::new();
    calib
        .put(ACCEPT, 0.8, 0.2, 150, "上岗", Some(0.05))
        .expect("校准记录合法");
    calib
        .put(CONF, 0.7, 0.3, 90, "上岗", Some(0.05))
        .expect("校准记录合法");
    calib
}

/// 登记 `do` 能触发的本地检查动作：它只记录并回传源码已经算好的值，不含任何候选算法。
fn actions(log: Rc<RefCell<Vec<Json>>>) -> ActionRegistry {
    let mut actions = ActionRegistry::new();
    actions.register("record_check", 0.0, true, TaintOut::Inherit, move |args| {
        if args.len() != 1 {
            return Err("record_check 收一条源码已算好的检查记录".into());
        }
        log.borrow_mut().push(args[0].to_json());
        Ok(args[0].clone())
    });
    actions
}

/// ```text
/// budget {calls: 9, cost: 0, depth: 64};
/// fn look(m, q) -> Record !{judge} {
///     let e = cut(judge(state(m), q));
///     handle(e, {act: fn() {{status: "accepted"}}, ignore: fn() {{status: "rejected"}},
///                unsure: fn(cause) {{status: "pending", cause: cause}}})
/// }
/// fn screen(cands, q) -> List !{judge} { map(cands, fn(c) {{cand: c, status: look(c, q).status}}) }
/// fn pick(items, want) { map(filter(items, fn(x) { x.status == want }), fn(x) { x.cand }) }
/// fn note(c, seq) -> Record !{do} {
///     let ok = c.cost > 0 && len(c.skills) > 0;
///     content(do("record_check", [{name: c.name, cost: c.cost, ok: ok}], seq))
/// }
/// fn record_all(cs, from) -> List !{do} { map(range(0, len(cs)), fn(i) { note(cs[i], from + i) }) }
/// fn subsets(cs) { fold(cs, [[]], fn(gs, c) { concat(gs, map(gs, fn(g) { append(g, c) })) }) }
/// fn total(g) { fold(g, 0, fn(t, c) { t + c.cost }) }
/// fn covers(g, s) { fold(g, false, fn(f, c) { f || contains(c.skills, s) }) }
/// fn usable(g) { covers(g, "web") && covers(g, "db") }
/// fn best(cs) { fold(filter(subsets(cs), usable), {members: [], cost: 999},
///                    fn(b, g) { if total(g) < b.cost { {members: map(g, fn(c) { c.name }), cost: total(g)} } else { b } }) }
/// fn packet(acc, und, checks) {
///     {plan: best(acc), pending: map(und, fn(c) { c.name }), checks: checks,
///      more: fn(strategy) { strategy(acc, und, checks) }}
/// }
/// fn first_round(cands) -> Record !{judge, do} { … }
/// fn supplement(acc, und, checks) -> Record !{judge, do} { … }   // 策略一：补材料再问一次
/// fn grade(acc, und, checks) -> Record !{judge} { … }            // 策略二：换分档题
/// let first = first_round([A, B, C, D]);
/// let second = first.more(supplement);
/// let third = second.more(grade);
/// ```
fn partial_program() -> Program {
    let cand_of = || lambda(&["x"], body(vec![], field(name("x"), "cand")));
    let names_of = || lambda(&["c"], body(vec![], field(name("c"), "name")));

    program(
        Some(budget(9, 64)), // B38（步 15d）：6 次判断 + 3 次 do
        vec![
            // 一次观察：判断 → 切出口 → 穷尽处理。读数不出现在返回值里
            func_eff(
                "look",
                &["m", "q"],
                &["judge"],
                body(
                    vec![bind("e", call("cut", vec![call("judge", vec![call("state", vec![name("m")]), name("q")])]))],
                    call(
                        "handle",
                        vec![
                            name("e"),
                            rec(vec![
                                ("act", lambda(&[], body(vec![], rec(vec![("status", text("accepted"))])))),
                                ("ignore", lambda(&[], body(vec![], rec(vec![("status", text("rejected"))])))),
                                // 13 §3 + 总控 2026-09-21 裁定：下游的 `screen` 只取 `.status`、把这条记录的其余
                                // 字段丢了，所以「把责任放进 cause 字段」在这个程序里**不是真的转交**——
                                // 最后一份承接信息会在下一步消失。改成**显式丢弃并记账**（`13` §3 明写
                                // 「允许显式丢弃」），让程序说出它本来就在做的事。原来的写法是无声消失。
                                (
                                    "unsure",
                                    lambda(
                                        &["u"],
                                        body(
                                            vec![discard(call("consume", vec![name("u"), text("drop")]))],
                                            rec(vec![("status", text("pending"))]),
                                        ),
                                    ),
                                ),
                            ]),
                        ],
                    ),
                ),
            ),
            func_eff(
                "screen",
                &["cands", "q"],
                &["judge"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            name("cands"),
                            lambda(
                                &["c"],
                                body(
                                    vec![],
                                    rec(vec![
                                        ("cand", name("c")),
                                        ("status", field(call("look", vec![name("c"), name("q")]), "status")),
                                    ]),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "pick",
                &["items", "want"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            call("filter", vec![name("items"), lambda(&["x"], body(vec![], bin("==", field(name("x"), "status"), name("want"))))]),
                            cand_of(),
                        ],
                    ),
                ),
            ),
            // 本地检查：有效性由源码算，动作只记录并回传
            func_eff(
                "note",
                &["c", "seq"],
                &["do"],
                body(
                    vec![bind(
                        "ok",
                        bin(
                            "&&",
                            bin(">", field(name("c"), "cost"), int(0)),
                            bin(">", call("len", vec![field(name("c"), "skills")]), int(0)),
                        ),
                    )],
                    call(
                        "content",
                        vec![call(
                            "do",
                            vec![
                                text("record_check"),
                                list(vec![rec(vec![
                                    ("name", field(name("c"), "name")),
                                    ("cost", field(name("c"), "cost")),
                                    ("ok", name("ok")),
                                ])]),
                                name("seq"),
                            ],
                        )],
                    ),
                ),
            ),
            func_eff(
                "record_all",
                &["cs", "from"],
                &["do"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            call("range", vec![int(0), call("len", vec![name("cs")])]),
                            lambda(
                                &["i"],
                                body(vec![], call("note", vec![index(name("cs"), name("i")), bin("+", name("from"), name("i"))])),
                            ),
                        ],
                    ),
                ),
            ),
            // —— 精确组合：幂集、约束、最省，全写在语言里
            func(
                "subsets",
                &["cs"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            name("cs"),
                            list(vec![list(vec![])]),
                            lambda(
                                &["gs", "c"],
                                body(
                                    vec![],
                                    call(
                                        "concat",
                                        vec![
                                            name("gs"),
                                            call(
                                                "map",
                                                vec![name("gs"), lambda(&["g"], body(vec![], call("append", vec![name("g"), name("c")])))],
                                            ),
                                        ],
                                    ),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "total",
                &["g"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![name("g"), int(0), lambda(&["t", "c"], body(vec![], bin("+", name("t"), field(name("c"), "cost"))))],
                    ),
                ),
            ),
            func(
                "covers",
                &["g", "s"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            name("g"),
                            boolean(false),
                            lambda(
                                &["f", "c"],
                                body(vec![], bin("||", name("f"), call("contains", vec![field(name("c"), "skills"), name("s")]))),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "usable",
                &["g"],
                body(
                    vec![],
                    bin(
                        "&&",
                        call("covers", vec![name("g"), text("web")]),
                        call("covers", vec![name("g"), text("db")]),
                    ),
                ),
            ),
            func(
                "best",
                &["cs"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            call("filter", vec![call("subsets", vec![name("cs")]), name("usable")]),
                            rec(vec![("members", list(vec![])), ("cost", int(999))]),
                            lambda(
                                &["b", "g"],
                                body(
                                    vec![],
                                    if_(
                                        bin("<", call("total", vec![name("g")]), field(name("b"), "cost")),
                                        rec(vec![
                                            ("members", call("map", vec![name("g"), names_of()])),
                                            ("cost", call("total", vec![name("g")])),
                                        ]),
                                        name("b"),
                                    ),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            // 部分结果：可用方案 + 未决项 + 一个「接着做」的方法值
            func(
                "packet",
                &["acc", "und", "checks"],
                body(
                    vec![],
                    rec(vec![
                        ("plan", call("best", vec![name("acc")])),
                        ("pending", call("map", vec![name("und"), names_of()])),
                        ("checks", name("checks")),
                        (
                            "more",
                            lambda(
                                &["strategy"],
                                body(vec![], call_of(name("strategy"), vec![name("acc"), name("und"), name("checks")])),
                            ),
                        ),
                    ]),
                ),
            ),
            func_eff(
                "first_round",
                &["cands"],
                &["judge", "do"],
                body(
                    vec![
                        bind("seen", call("screen", vec![name("cands"), call("test", vec![text(ASK), text(ACCEPT)])])),
                        bind("acc", call("pick", vec![name("seen"), text("accepted")])),
                        bind("checks", call("record_all", vec![name("acc"), int(0)])),
                    ],
                    call(
                        "packet",
                        vec![name("acc"), call("pick", vec![name("seen"), text("pending")]), call("len", vec![name("checks")])],
                    ),
                ),
            ),
            // 策略一：给便宜的未决项补材料，再问同一道题——新材料就是新的观察身份
            func_eff(
                "supplement",
                &["acc", "und", "checks"],
                &["judge", "do"],
                body(
                    vec![
                        bind("q", call("test", vec![text(ASK), text(ACCEPT)])),
                        bind(
                            "seen",
                            call(
                                "map",
                                vec![
                                    name("und"),
                                    lambda(
                                        &["c"],
                                        body(
                                            vec![],
                                            if_(
                                                bin("<=", field(name("c"), "cost"), int(2)),
                                                rec(vec![
                                                    ("cand", name("c")),
                                                    (
                                                        "status",
                                                        field(
                                                            call(
                                                                "look",
                                                                vec![
                                                                    call(
                                                                        "transform",
                                                                        vec![
                                                                            lambda(
                                                                                &["old"],
                                                                                body(
                                                                                    vec![],
                                                                                    call(
                                                                                        "with",
                                                                                        vec![
                                                                                            call("content", vec![name("old")]),
                                                                                            text("evidence"),
                                                                                            text("supplement"),
                                                                                        ],
                                                                                    ),
                                                                                ),
                                                                            ),
                                                                            name("c"),
                                                                        ],
                                                                    ),
                                                                    name("q"),
                                                                ],
                                                            ),
                                                            "status",
                                                        ),
                                                    ),
                                                ]),
                                                rec(vec![("cand", name("c")), ("status", text("pending"))]),
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                        ),
                        bind("more", call("pick", vec![name("seen"), text("accepted")])),
                    ],
                    call(
                        "packet",
                        vec![
                            call("concat", vec![name("acc"), name("more")]),
                            call("pick", vec![name("seen"), text("pending")]),
                            bin(
                                "+",
                                name("checks"),
                                call("len", vec![call("record_all", vec![name("more"), name("checks")])]),
                            ),
                        ],
                    ),
                ),
            ),
            // 策略二：换一道分档题、换一套校准，处理剩下的未决项
            func_eff(
                "grade",
                &["acc", "und", "checks"],
                &["judge"],
                body(
                    vec![
                        bind(
                            "q",
                            call(
                                "measure",
                                vec![text(GRADE), list(LEVELS.iter().map(|s| text(s)).collect()), text(CONF)],
                            ),
                        ),
                        bind(
                            "seen",
                            call(
                                "map",
                                vec![
                                    name("und"),
                                    lambda(
                                        &["c"],
                                        body(
                                            vec![],
                                            call(
                                                "handle",
                                                vec![
                                                    call("cut", vec![call("judge", vec![call("state", vec![name("c")]), name("q")])]),
                                                    rec(vec![
                                                        (
                                                            "at",
                                                            lambda(
                                                                &["level"],
                                                                body(
                                                                    vec![],
                                                                    rec(vec![
                                                                        ("cand", name("c")),
                                                                        (
                                                                            "status",
                                                                            if_(
                                                                                bin(">=", name("level"), int(2)),
                                                                                text("accepted"),
                                                                                text("rejected"),
                                                                            ),
                                                                        ),
                                                                    ]),
                                                                ),
                                                            ),
                                                        ),
                                                        (
                                                            "unsure",
                                                            // 未决责任要带着走，不能只写一句 pending 就算完
                                                            lambda(
                                                                &["u"],
                                                                body(
                                                                    vec![],
                                                                    rec(vec![
                                                                        ("cand", name("c")),
                                                                        ("status", text("pending")),
                                                                        ("待办", name("u")),
                                                                    ]),
                                                                ),
                                                            ),
                                                        ),
                                                    ]),
                                                ],
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                        ),
                    ],
                    call(
                        "packet",
                        vec![
                            call("concat", vec![name("acc"), call("pick", vec![name("seen"), text("accepted")])]),
                            call("pick", vec![name("seen"), text("pending")]),
                            name("checks"),
                        ],
                    ),
                ),
            ),
            bind(
                "first",
                call(
                    "first_round",
                    vec![list(vec![
                        rec(vec![("name", text("A")), ("cost", int(4)), ("skills", list(vec![text("web")]))]),
                        rec(vec![("name", text("B")), ("cost", int(5)), ("skills", list(vec![text("db")]))]),
                        rec(vec![("name", text("C")), ("cost", int(2)), ("skills", list(vec![text("web"), text("db")]))]),
                        rec(vec![("name", text("D")), ("cost", int(20)), ("skills", list(vec![text("web")]))]),
                    ])],
                ),
            ),
            bind("second", call_of(field(name("first"), "more"), vec![name("supplement")])),
            bind("third", call_of(field(name("second"), "more"), vec![name("grade")])),
        ],
        rec(vec![
            ("first", snapshot("first")),
            ("second", snapshot("second")),
            ("third", snapshot("third")),
            ("done", bin("==", call("len", vec![field(name("third"), "pending")]), int(0))),
        ]),
    )
}

/// 取一个部分结果里可展示的三项（`more` 是方法值，不进对照）
fn snapshot(which: &str) -> Expr {
    rec(vec![
        ("plan", field(name(which), "plan")),
        ("pending", field(name(which), "pending")),
        ("checks", field(name(which), "checks")),
    ])
}

#[test]
fn 部分候选先交付再续解() {
    let program = partial_program();
    assert_clean(&program);

    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        client.ports(),
        &calibrations(),
        &actions(log.clone()),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(
        outcome.value_json(),
        json!({
            "first":  {"plan": {"members": ["A", "B"], "cost": 9}, "pending": ["C", "D"], "checks": 2},
            "second": {"plan": {"members": ["C"],      "cost": 2}, "pending": ["D"],      "checks": 3},
            "third":  {"plan": {"members": ["C"],      "cost": 2}, "pending": [],         "checks": 3},
            "done": true
        })
    );
    assert!(
        outcome.pending.is_empty(),
        "程序没有被挂起：未决是算法的返回值，不是程序级出口"
    );
    assert_eq!(client.judge.log.len(), 6, "六次固定观察");
    assert_eq!(outcome.cost.calls, 9, "6 次判断 + 3 次 do（B38）");
    assert_eq!(outcome.trace.count("judge", false), 6);
    assert_eq!(
        log.borrow().len(),
        3,
        "三次本地检查：A、B 各一次，C 补材料后一次"
    );
    assert_eq!(outcome.trace.count("do", false), 3);
    assert_eq!(
        outcome.trace.count("transform", false),
        1,
        "只给 C 补了材料"
    );
    // 步 13b（事后补登，见 `地基/过程记录/工程-步13b.md`）：`screen` 的 `map` 体经 `look(c, q)` 包装判断，
    // 向量化穿过包装后，后续各轮在第 0 轮刷新前已登记；真走到时命中本趟账本，计入 `replayed`。
    // 第一次跑没有旧账本，这 3 条都是本趟提前登记的命中，调用数仍是 6。
    assert_eq!(outcome.cost.replayed, 3, "本趟提前登记的站点被真走到时命中");
    // `W-drop-vs-escalate` 是**常规记账**（显式丢弃已记账），不是「可能有问题」。它和
    // `W-bound` / `W-header` 挤在同一个列表里，所以这里不能再断言整个列表为空——
    // 断言落在「有没有那两条真正的体检项」上。把常规记账从 warnings 分出去的提议见
    // COORDINATION.md，等谁下次动 Trace 时一起做。
    // `W-fixture-line` 与 `W-drop-vs-escalate` 同族：常规记账，不是体检项。
    // 这里的线是手填的（n=150 / n=90）、没有保形证书，**它本来就该响**。
    // 步 15d-2：没带画像时每个判断站点都会多一条 W-window-untested（§3.9），同样不是体检项。
    let 体检 = |ws: &[String]| {
        ws.iter()
            .filter(|w| {
                !w.starts_with("W-drop-vs-escalate")
                    && !w.starts_with("W-fixture-line")
                    && !w.starts_with("W-window-untested")
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    assert!(
        体检(&outcome.trace.warnings).is_empty(),
        "不该有 W-bound / W-header：{:?}",
        outcome.trace.warnings
    );

    // 旧检查不重做：三条 do 记录的键互不相同，A、B 的那两条在后两轮没有再出现
    let keys: Vec<&str> = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Effect { key, kind, .. } if kind == "do" => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(
        keys.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "三条检查是三个不同的账本键"
    );
    let names: Vec<String> = log
        .borrow()
        .iter()
        .map(|v| v["name"].as_str().unwrap_or("?").to_string())
        .collect();
    assert_eq!(names, vec!["A", "B", "C"]);
}

#[test]
fn 部分候选程序重放零调用() {
    let program = partial_program();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let first = run(
        &program,
        client.ports(),
        &calibrations(),
        &actions(log.clone()),
        &mut ledger,
    )
    .expect("首跑");

    let replay_log = Rc::new(RefCell::new(Vec::new()));
    let again = run(
        &program,
        NoCallPorts::ports(),
        &calibrations(),
        &actions(replay_log.clone()),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("重放不该发调用：{}", e.render()));

    assert_eq!(again.value_json(), first.value_json());
    assert_eq!(again.cost.calls, 0, "重放零调用");
    assert_eq!(
        again.cost.replayed, 10,
        "6 次判断 + 3 次动作 + 1 次变换全部命中账本"
    );
    assert!(
        replay_log.borrow().is_empty(),
        "重放不重新执行动作，只取账本里的输出"
    );
    // `W-fixture-line` 与 `W-drop-vs-escalate` 同族：常规记账，不是体检项。
    // 这里的线是手填的（n=150 / n=90）、没有保形证书，**它本来就该响**。
    // 步 15d-2：没带画像时每个判断站点都会多一条 W-window-untested（§3.9），同样不是体检项。
    let 体检 = |ws: &[String]| {
        ws.iter()
            .filter(|w| {
                !w.starts_with("W-drop-vs-escalate")
                    && !w.starts_with("W-fixture-line")
                    && !w.starts_with("W-window-untested")
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    assert!(
        体检(&again.trace.warnings).is_empty(),
        "{:?}",
        again.trace.warnings
    );
}

/// 错的效应标注要拦得住：两条策略方法都是经 `packet` 里的方法值间接被调用的。
#[test]
fn 错的效应标注拦得住() {
    let program = partial_program();
    let cases: [(&str, &[&str]); 5] = [
        ("supplement", &["judge", "do"]),
        ("grade", &["judge"]),
        ("first_round", &["judge", "do"]),
        ("record_all", &["do"]),
        ("screen", &["judge"]),
    ];
    for (name, real) in cases {
        let (bad, at) = with_effects(&program, name, &[]);
        let report = jpp::check(&bad);
        let d = report
            .diagnostics
            .iter()
            .find(|d| d.rule == "E-effect" && d.message.starts_with(name))
            .unwrap_or_else(|| panic!("{name} 标成 !{{}} 应当报 E-effect：\n{}", report.render()));
        assert_eq!(d.span, at, "位置要指着 {name} 的定义");
        for effect in real {
            assert!(
                d.message.contains(effect),
                "{name} 应当报出少了 {effect}：{}",
                d.message
            );
        }

        let (good, _) = with_effects(&program, name, real);
        let report = jpp::check(&good);
        assert!(report.is_ok(), "{name} 标对了不该被报：{}", report.render());
    }
}

/// 步 13b（主会话要求）：穿过 `look` 包装的向量化省的是层数，不多花调用；预算紧时也不多花、出口不变。
fn 跑_开关(program: &Program, passes: Passes) -> jpp::Outcome {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let calib = calibrations();
    let acts = actions(log);
    let mut it = Interp::new(
        client.ports(),
        &mut ledger,
        &calib,
        &acts,
        program.budget.clone(),
    );
    it.passes = passes;
    it.run(program).unwrap_or_else(|e| panic!("{}", e.render()))
}

#[test]
fn 部分候选_穿过包装只省层数() {
    let program = partial_program();
    let 开 = 跑_开关(&program, Passes::default());
    let 关 = 跑_开关(
        &program,
        Passes {
            vectorize: false,
            ..Passes::default()
        },
    );
    assert_eq!(开.cost.calls, 关.cost.calls, "调用数不变");
    assert_eq!(开.value_json(), 关.value_json(), "出口不变");
    assert!(
        开.layers.len() < 关.layers.len(),
        "层数变少：开 {} 关 {}",
        开.layers.len(),
        关.layers.len()
    );
    assert_eq!((开.cost.calls, 开.layers.len(), 关.layers.len()), (9, 3, 6));
}

#[test]
fn 部分候选_预算紧时提前登记不多花() {
    // 步 22-0（B93）：预算停发后 `do` 产出失败值，本程序对它 `content` 报运行期错（J-12：失败值要先 is_fail）；
    // 两臂结果（值或错误）相同、判断调用数相同即说明提前登记没多花
    let mut program = partial_program();
    program.budget.calls = 4;
    let 开 = 跑_开关_结果(&program, Passes::default());
    let 关 = 跑_开关_结果(
        &program,
        Passes {
            vectorize: false,
            ..Passes::default()
        },
    );
    assert!(开.1 <= 4, "不超预算：{}", 开.1);
    assert!(关.1 < 6, "预算确实吃紧（原程序要 6 次）：{}", 关.1);
    assert_eq!(开.1, 关.1, "预算紧时调用数不因提前登记而增加");
    assert_eq!(开.0, 关.0, "结果不变");
}

/// 同 `跑_开关`，但运行期错误也收下：返回（值或错误的渲染，账本里判断调用的个数）
fn 跑_开关_结果(
    program: &Program,
    passes: Passes,
) -> (Result<serde_json::Value, String>, u64) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let calib = calibrations();
    let acts = actions(log);
    let mut it = Interp::new(
        client.ports(),
        &mut ledger,
        &calib,
        &acts,
        program.budget.clone(),
    );
    it.passes = passes;
    let r = it
        .run(program)
        .map(|o| o.value_json())
        .map_err(|e| e.render());
    let 调用: std::collections::BTreeSet<u64> = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            jpp::ledger::Entry::Judge { call, .. } => Some(*call),
            _ => None,
        })
        .collect();
    (r, 调用.len() as u64)
}
