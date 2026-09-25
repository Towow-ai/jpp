//! 效应行在**一等方法值**处还管不管用：给真正的高阶函数写一个错的 `!{…}`，检查器必须拦下。
//!
//! 首包拦不住。首包的 `E-effect` 一遇到「被调者是参数里的方法值」就把整条推断退成 ⊤ 并跳过检查，
//! 于是 `solve` / `advance` / `probe` / `supplement` / `grade` 这些真正带效应的高阶函数一个都没被核，
//! 被核的只有叶子函数（见 `INTERFACE.md` §七第 5 条）。这份测试就是钉住那条洞被补上了。
//!
//! 每条都配一个正向对照：**原样的程序仍然一条诊断都不报**。只证明「错的被拦下」不够，
//! 那样把检查器改成见谁咬谁也能通过；要同时证明没有误杀。
//!
//! `solve` 与 `advance` 直接用 `examples/*.jpp` 的**原文**，只在内存里替换那一行标注——
//! 样例归前端（Codex），core 不改它们。`probe` / `search` / `supplement` / `grade` 是
//! `tests/adaptive.rs` 与 `tests/partial.rs` 里手工构造的那两个程序的源码写法。
//!
//! 与 `tests/examples.rs`、`tests/adaptive.rs`、`tests/partial.rs` 里那三条同向的测试是**独立交叉**：
//! 那几条改 lower 之后的 core AST，这里走完整的 `parse -> lower -> check`，报文与 Span 都落在源码上。
//! 这一份还多两个方向：**不能见谁咬谁**（创建 / 存起 / 遮蔽的方法不算发生效应）与**多调用点取并集**。

use std::path::Path;

use jpp::{Report, check};
use jpp::{lower, syntax::parse};

fn check_source(what: &str, source: &str) -> Report {
    let parsed =
        parse(source).unwrap_or_else(|d| panic!("{what} 解析失败：{}", d.render(what, source)));
    let core = lower(&parsed)
        .unwrap_or_else(|d| panic!("{what} lower 失败：{}", d[0].render(what, source)));
    check(&core)
}

fn example(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()))
}

/// 换掉一处标注；换不到就是样例改了形状，测试要当场说清楚，不能悄悄测了个空
fn retag(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "「{from}」在样例里应当恰好出现一次"
    );
    source.replace(from, to)
}

fn effect_error<'a>(report: &'a Report, what: &str) -> &'a jpp::Diagnostic {
    report
        .find("E-effect")
        .unwrap_or_else(|| panic!("{what} 应当报 E-effect，实际诊断：\n{}", report.render()))
}

fn assert_clean(what: &str, report: &Report) {
    assert!(report.is_ok(), "{what} 不该报错：\n{}", report.render());
}

// ---------------------------------------------------------------- examples/*.jpp 里的两个

/// `solve` 自己一个效应都不产：它只调用传进来的 `method`。所以它的标注只能在**调用点**核——
/// `solve(mat({target: 731}), step)` 把 `step`（`!{judge}`）放进方法位，`solve` 就得认 judge。
#[test]
fn solve_的错标注在调用点被拦下() {
    let source = example("adaptive.jpp");
    assert_clean("adaptive.jpp 原样", &check_source("adaptive.jpp", &source));

    let wrong = retag(
        &source,
        "fn solve(input: Mat, method: Fn(Record) -> Record) -> Record !{judge}",
        "fn solve(input: Mat, method: Fn(Record) -> Record) -> Record !{}",
    );
    let report = check_source("adaptive.jpp(solve 标成纯的)", &wrong);
    let d = effect_error(&report, "solve 标成 !{} 而实参 step 会 judge");
    assert!(
        d.message.contains("judge"),
        "报文要点出少了哪个效应：{}",
        d.message
    );
    assert!(d.span.end <= wrong.len(), "位置要落在源码里");
}

/// `advance` 的 judge 与 do 藏在 `map` 的匿名方法体里（`observe` / `inspect`），
/// 而它同时又调用参数 `strategy`。首包因为后者把前者也一起放过了。
#[test]
fn advance_的错标注在定义处被拦下() {
    let source = example("partial.jpp");
    assert_clean("partial.jpp 原样", &check_source("partial.jpp", &source));

    let missing_do = retag(
        &source,
        "fn advance(s: Record, strategy) -> Record !{judge, do}",
        "fn advance(s: Record, strategy) -> Record !{judge}",
    );
    let report = check_source("partial.jpp(advance 漏标 do)", &missing_do);
    let d = effect_error(&report, "advance 漏标 do");
    assert!(d.message.contains("do"), "{}", d.message);

    let missing_judge = retag(
        &source,
        "fn advance(s: Record, strategy) -> Record !{judge, do}",
        "fn advance(s: Record, strategy) -> Record !{do}",
    );
    let report = check_source("partial.jpp(advance 漏标 judge)", &missing_judge);
    let d = effect_error(&report, "advance 漏标 judge");
    assert!(d.message.contains("judge"), "{}", d.message);
}

// ---------------------------------------------------------------- tests/adaptive.rs 的那个程序

/// `tests/adaptive.rs` 里手工构造的二分选问程序的源码写法：题是值，`make_q` 是参数。
const BISECT: &str = r#"
budget {calls: 10, cost: 0, depth: 64};
fn bisect_asker(calib) { fn(bound) { test("目标是否不大于 " + text(bound), calib) } }
fn probe(lo, hi, make_q) -> Record !{judge} {
    let mid = (lo + hi) / 2;
    let st = state(mat({lo: lo, hi: hi}));
    let e = cut(judge(st, make_q(mid)));
    handle(e, {act: fn() { {lo: lo, hi: mid} },
               ignore: fn() { {lo: mid + 1, hi: hi} },
               unsure: fn(cause) { {lo: lo, hi: hi, stuck: cause} }})
}
fn search(start, make_q) -> Record !{judge} {
    loop(10, start, fn(s, i) {
        let n = probe(s.lo, s.hi, make_q);
        let next = {lo: n.lo, hi: n.hi, asked: s.asked + 1};
        if next.lo == next.hi { stop(next) } else { next }
    })
}
let below = bisect_asker("bisect");
let found = search({lo: 1, hi: 1000, asked: 0}, below);
{target: found.lo, asked: found.asked}
"#;

/// `probe` 体内既调用参数 `make_q`，又自己 `judge`。确定的那一半不该因为前者被放过。
#[test]
fn probe_的错标注被拦下() {
    assert_clean("二分选问程序原样", &check_source("bisect", BISECT));

    let wrong = retag(
        BISECT,
        "fn probe(lo, hi, make_q) -> Record !{judge}",
        "fn probe(lo, hi, make_q) -> Record !{}",
    );
    let report = check_source("bisect(probe 标成纯的)", &wrong);
    assert!(
        effect_error(&report, "probe 标成 !{}")
            .message
            .contains("judge")
    );
}

/// `search` 自己不 judge：它的 judge 是 `loop` 体内调用 `probe` 带来的。
/// 调用点在**另一个函数体的 lambda 里**，实例化要能走到那儿。
#[test]
fn search_的错标注被拦下_效应来自嵌套调用点() {
    let wrong = retag(
        BISECT,
        "fn search(start, make_q) -> Record !{judge}",
        "fn search(start, make_q) -> Record !{}",
    );
    let report = check_source("bisect(search 标成纯的)", &wrong);
    assert!(
        effect_error(&report, "search 标成 !{}")
            .message
            .contains("judge")
    );
}

// ---------------------------------------------------------------- tests/partial.rs 的那个程序

/// `tests/partial.rs` 里手工构造的部分结果程序的源码写法：`packet` 捕获算法状态，
/// `more` 是个收策略的方法值，`supplement` / `grade` 是两个策略。
const PARTIAL: &str = r#"
budget {calls: 6, cost: 0, depth: 64};
fn look(m, q) -> Record !{judge} {
    let e = cut(judge(state(m), q));
    handle(e, {act: fn() { {status: "accepted"} }, ignore: fn() { {status: "rejected"} },
               unsure: fn(cause) { {status: "pending", cause: cause} }})
}
fn screen(cands, q) -> List !{judge} { map(cands, fn(c) { {cand: c, status: look(c, q).status} }) }
fn pick(items, want) { map(filter(items, fn(x) { x.status == want }), fn(x) { x.cand }) }
fn note(c, seq) -> Record !{do} {
    let ok = c.cost > 0 && len(c.skills) > 0;
    content(do("record_check", [{name: c.name, cost: c.cost, ok: ok}], seq))
}
fn record_all(cs, from) -> List !{do} { map(range(0, len(cs)), fn(i) { note(cs[i], from + i) }) }
fn packet(acc, und, checks) {
    {plan: len(acc), pending: map(und, fn(c) { c.name }), checks: checks,
     more: fn(strategy) { strategy(acc, und, checks) }}
}
fn supplement(acc, und, checks) -> Record !{judge, do} {
    let q = test("候选合适吗", "fixture");
    let seen = map(und, fn(c) {
        if c.cost <= 2 {
            {cand: c, status: look(transform(fn(old) { with(content(old), "evidence", "supplement") }, c), q).status}
        } else {
            {cand: c, status: "pending"}
        }
    });
    let more = pick(seen, "accepted");
    packet(concat(acc, more), pick(seen, "pending"), checks + len(record_all(more, checks)))
}
fn grade(acc, und, checks) -> Record !{judge} {
    let q = measure("有多合适", ["low", "high"], "conf");
    let seen = map(und, fn(c) {
        handle(cut(judge(state(c), q)), {
            at: fn(level) { {cand: c, status: if level > 0 { "accepted" } else { "rejected" }} },
            unsure: fn(cause) { {cand: c, status: "pending"} }})
    });
    packet(acc, pick(seen, "pending"), checks)
}
let first = packet([], [], 0);
let second = first.more(supplement);
let third = second.more(grade);
{first: first.checks, second: second.checks, third: third.checks}
"#;

/// `supplement` 的 judge 来自 `map` 体内的 `look`，do 来自 `record_all`。两个都在匿名方法里。
#[test]
fn supplement_的错标注被拦下() {
    assert_clean("部分结果程序原样", &check_source("partial", PARTIAL));

    let missing_do = retag(
        PARTIAL,
        "fn supplement(acc, und, checks) -> Record !{judge, do}",
        "fn supplement(acc, und, checks) -> Record !{judge}",
    );
    let report = check_source("partial(supplement 漏标 do)", &missing_do);
    assert!(
        effect_error(&report, "supplement 漏标 do")
            .message
            .contains("do")
    );

    let missing_judge = retag(
        PARTIAL,
        "fn supplement(acc, und, checks) -> Record !{judge, do}",
        "fn supplement(acc, und, checks) -> Record !{do}",
    );
    let report = check_source("partial(supplement 漏标 judge)", &missing_judge);
    assert!(
        effect_error(&report, "supplement 漏标 judge")
            .message
            .contains("judge")
    );
}

/// `grade` 是经 `first.more(grade)` 传进去的策略：被调者是字段，静态认不出，
/// 所以它只能靠自己体内那半被核——这一半以前也没被核。
#[test]
fn grade_的错标注被拦下() {
    let wrong = retag(
        PARTIAL,
        "fn grade(acc, und, checks) -> Record !{judge}",
        "fn grade(acc, und, checks) -> Record !{}",
    );
    let report = check_source("partial(grade 标成纯的)", &wrong);
    assert!(
        effect_error(&report, "grade 标成 !{}")
            .message
            .contains("judge")
    );
}

// ---------------------------------------------------------------- 反方向：不能见谁咬谁

/// 造一个方法不等于执行它。只被返回、只被存进记录的方法体，效应不算在外层头上
/// （Codex 答 (c) 第 3 条）。这条要是破了，`packet` 那种续接方法会当场造出假错。
#[test]
fn 创建或传递方法不算发生效应() {
    let source = r#"
budget {calls: 1, cost: 0, depth: 8};
fn asker(calib) -> Record !{} {
    {make: fn(m, q) { cut(judge(state(m), q)) }, name: "asker"}
}
fn keep(fs, f) -> List !{} { append(fs, f) }
fn peek(m, q) -> Record !{judge} {
    handle(cut(judge(state(m), q)), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                     unsure: fn(cause) { {ok: unit, cause: cause} }})
}
{a: len(keep([], peek)), b: asker("fixture").name}
"#;
    assert_clean(
        "只创建、只存起方法值",
        &check_source("创建不等于执行", source),
    );
}

/// 同一个参数在不同调用点收到不同的方法：`!{…}` 是**上界**，要盖住所有调用点的并集，
/// 盖住了就不该报。注意这与「行按调用点传播」是两件事，分开算：核标注取并集，
/// 传给调用者的行按每个调用点各自实例化（见下面那条多态测试）。
#[test]
fn 一个参数收到多个实参时按并集核() {
    let source = r#"
budget {calls: 2, cost: 0, depth: 8};
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "fixture"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                          unsure: fn(cause) { {ok: unit, cause: cause} }})
}
fn plain(m) -> Record !{} { {ok: true} }
fn apply(m, f) -> Record !{judge} { f(m) }
{a: apply(mat({x: 1}), peek).ok, b: apply(mat({x: 2}), plain).ok}
"#;
    let report = check_source("并集", source);
    assert_clean("apply 标了 judge，盖得住两个调用点的并集", &report);
    assert!(
        report.find("W-effect").is_none(),
        "judge 确实从 peek 那个调用点来，不该提示「标了却看不到」：\n{}",
        report.render()
    );

    // 反过来：标成纯的就该被拦下
    let wrong = source.replace(
        "fn apply(m, f) -> Record !{judge}",
        "fn apply(m, f) -> Record !{}",
    );
    let report = check_source("并集（apply 标成纯的）", &wrong);
    assert!(
        effect_error(&report, "apply 标成 !{} 而有一个调用点传了 peek")
            .message
            .contains("judge")
    );
}

/// 形参被块内同名的 `let` / `fn` 盖住之后，调的是新绑定，不是那个参数。
/// 这条以前会误报：效应行是按**名字**灌给形参的，往下走时没摘掉被盖住的名字，
/// 于是 `f(peek)` 那次调用点实例化出来的 judge 被算到了一个纯函数头上。
#[test]
fn 形参被同名局部绑定盖住时不算它的效应() {
    let peek = r#"
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "fixture"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                          unsure: fn(cause) { {ok: unit, cause: cause} }})
}
"#;
    // let 遮蔽
    let by_let = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{peek}
fn f(cb) -> Record !{{}} {{ let cb = fn() {{ {{ok: true}} }}; cb() }}
{{a: f(peek).ok}}"
    );
    assert_clean("形参被 let 盖住", &check_source("遮蔽(let)", &by_let));

    // fn 语句遮蔽
    let by_fn = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{peek}
fn f(cb) -> Record !{{}} {{ fn cb() -> Record !{{}} {{ {{ok: true}} }} cb() }}
{{a: f(peek).ok}}"
    );
    assert_clean("形参被 fn 语句盖住", &check_source("遮蔽(fn)", &by_fn));

    // 没有遮蔽时仍然该被拦下——证明上面两条不是把检查关掉换来的
    let no_shadow = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{peek}
fn f(cb) -> Record !{{}} {{ cb(mat({{x: 1}})) }}
{{a: f(peek).ok}}"
    );
    let report = check_source("没遮蔽", &no_shadow);
    assert!(
        effect_error(&report, "没有遮蔽时 f 调的就是那个会 judge 的形参")
            .message
            .contains("judge")
    );
}

// ---------------------------------------------------------------- 效应多态：按调用点实例化

const POLY_BASE: &str = r#"
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "fixture"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                          unsure: fn(cause) { {ok: unit, cause: cause} }})
}
fn plain(m) -> Record !{} { {ok: true} }
"#;

/// 同一个高阶函数用在两处、一处传纯方法一处传带 judge 的方法：**纯的那处不该背 judge**。
///
/// 这是效应多态与单态并集的分水岭。单态实现把一个形参的效应行取所有调用点的并集，
/// 于是 `apply(m, plain)` 这个调用点也按 `{judge}` 算，`pure_user` 被误报「标注少了 judge」。
/// 三份样例里每个高阶函数只有一种用法，碰不到；复用一多就会炸。
#[test]
fn 效应多态_同一高阶函数两处用法互不污染() {
    let src = format!(
        "budget {{calls: 2, cost: 0, depth: 8}};{POLY_BASE}
fn apply(m, f) {{ f(m) }}
fn pure_user(m) -> Record !{{}} {{ apply(m, plain) }}
fn judgy_user(m) -> Record !{{judge}} {{ apply(m, peek) }}
{{a: pure_user(mat({{x: 1}})).ok, b: judgy_user(mat({{x: 2}})).ok}}"
    );
    assert_clean(
        "传纯方法的那个调用点不该背 judge",
        &check_source("多态", &src),
    );

    // 反过来：真的传了会 judge 的方法，标成纯的就得被拦下——证明上面那条不是把检查关掉换来的
    let wrong = retag(
        &src,
        "fn judgy_user(m) -> Record !{judge}",
        "fn judgy_user(m) -> Record !{}",
    );
    let report = check_source("多态(judgy_user 标成纯的)", &wrong);
    let d = effect_error(&report, "judgy_user 经 apply 调用了 peek");
    assert!(
        d.message.starts_with("judgy_user"),
        "诊断要指着出错的那个函数：{}",
        d.message
    );
    assert!(d.message.contains("judge"), "{}", d.message);

    // 高阶函数自己标了上界也一样核：apply 标成 !{} 而有调用点传 peek
    let wrong = retag(&src, "fn apply(m, f) {", "fn apply(m, f) -> Record !{} {");
    let report = check_source("多态(apply 标成纯的)", &wrong);
    assert!(
        effect_error(&report, "apply 标成 !{} 而有调用点传 peek")
            .message
            .starts_with("apply")
    );
}

/// 方法经**返回值**传递：`maker()` 返回 `peek`，`maker()(m)` 就会 judge。
#[test]
fn 方法经返回值传递时契约不丢() {
    let src = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn maker() -> Fn(Record) -> Record !{{}} {{ peek }}
fn use_maker(m) -> Record !{{judge}} {{ maker()(m) }}
{{a: use_maker(mat({{x: 1}})).ok}}"
    );
    assert_clean("标对了不该报", &check_source("返回方法", &src));

    let wrong = retag(
        &src,
        "fn use_maker(m) -> Record !{judge}",
        "fn use_maker(m) -> Record !{}",
    );
    let report = check_source("返回方法(标成纯的)", &wrong);
    assert!(
        effect_error(&report, "调用了 maker 返回的方法")
            .message
            .starts_with("use_maker")
    );
}

/// 方法经**记录字段**传递：`boxed()` 返回 `{go: peek}`，`boxed().go(m)` 就会 judge。
#[test]
fn 方法经记录字段传递时契约不丢() {
    let src = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn boxed() -> Record !{{}} {{ {{go: peek}} }}
fn use_boxed(m) -> Record !{{judge}} {{ boxed().go(m) }}
{{a: use_boxed(mat({{x: 1}})).ok}}"
    );
    assert_clean("标对了不该报", &check_source("记录字段", &src));

    let wrong = retag(
        &src,
        "fn use_boxed(m) -> Record !{judge}",
        "fn use_boxed(m) -> Record !{}",
    );
    let report = check_source("记录字段(标成纯的)", &wrong);
    assert!(
        effect_error(&report, "调用了记录字段上的方法")
            .message
            .starts_with("use_boxed")
    );
}

/// 递归把方法参数自己传回去：以前一个认不出的调用点就把这个形参的行毒成未知，
/// 顶层那次真实参的实例化被一起冲掉。按调用点实例化之后，顶层那次照样算数。
#[test]
fn 递归传递方法参数也核得住() {
    let src = format!(
        "budget {{calls: 3, cost: 0, depth: 8}};{POLY_BASE}
fn walk(ms, f) -> List !{{judge}} {{ if len(ms) == 0 {{ [] }} else {{ append(walk(slice(ms, 1, len(ms)), f), f(ms[0])) }} }}
{{a: len(walk([mat({{x: 1}})], peek))}}"
    );
    assert_clean("标对了不该报", &check_source("递归", &src));

    let wrong = retag(
        &src,
        "fn walk(ms, f) -> List !{judge}",
        "fn walk(ms, f) -> List !{}",
    );
    let report = check_source("递归(标成纯的)", &wrong);
    assert!(
        effect_error(&report, "递归函数的方法参数在顶层收到 peek")
            .message
            .starts_with("walk")
    );
}

/// 反方向的提示（标了却看不到）要**完整信息**才能发：只要有一个调用点的实参认不出，
/// 就不能因为别的调用点认得出、就把「认不出」抹掉。两个调用点都认得出且都是纯的，才该提示。
#[test]
fn 有认不出的调用点时不发标了却看不到的提示() {
    let partly_unknown = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn apply(m, f) -> Record !{{judge}} {{ f(m) }}
let g = peek;
{{a: apply(mat({{x: 1}}), g).ok, b: apply(mat({{x: 2}}), plain).ok}}"
    );
    let report = check_source("一处认不出", &partly_unknown);
    assert!(
        report.find("W-effect").is_none(),
        "有认不出的调用点就不该提示「标了却看不到」：\n{}",
        report.render()
    );
    assert_clean("也不该报错", &report);

    let all_known = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn apply(m, f) -> Record !{{judge}} {{ f(m) }}
{{a: apply(mat({{x: 1}}), plain).ok, b: apply(mat({{x: 2}}), plain).ok}}"
    );
    let report = check_source("全认得出且都纯", &all_known);
    assert!(
        report.find("W-effect").is_some(),
        "两个调用点都传纯方法，judge 确实用不上：\n{}",
        report.render()
    );
    assert!(
        report.is_ok(),
        "这只是提示，不拦程序：\n{}",
        report.render()
    );
}

/// 参数**类型位**上的效应行是对实参的契约：类型说只收纯方法，传一个会 judge 的进来就是错，
/// 诊断指着**传错东西的那个实参**。
///
/// 这条直接拿表层 AST 搭，不走文法（原先搭核心语法树，步 12d 删除后改搭表层 AST 再降级）；
/// `tests/effect_rows_typed.rs` 是它的源码级对照。
#[test]
fn 实参必须在参数类型的效应行之内() {
    use jpp::syntax::ast::{
        Block, Expr, ExprKind, Function, Parameter, Program, Span, Statement, Type,
    };

    let sp = Span { start: 0, end: 1 };
    let at = Span { start: 40, end: 44 }; // 实参 peek 的位置
    let record = || Type::Named("Record".into());
    let method = |effects: &[&str]| Type::Method {
        parameters: vec![record()],
        result: Box::new(record()),
        effects: Some(effects.iter().map(|e| e.to_string()).collect()),
        captures_responsibility: false,
    };
    // f 的类型：只收纯方法
    let pure_method = method(&[]);
    let int = |v| Expr {
        kind: ExprKind::Integer(v),
        span: sp,
    };
    let call = |f: &str, arg: Expr| Expr {
        kind: ExprKind::Call {
            function: Box::new(Expr {
                kind: ExprKind::Name(f.into()),
                span: sp,
            }),
            arguments: vec![arg],
        },
        span: sp,
    };
    let block = |statements, result| Block {
        statements,
        result: Some(Box::new(result)),
        span: sp,
    };

    let leaf = |name: &str, effects: &[&str]| Statement::Function {
        name: name.into(),
        function: Function {
            parameters: vec![Parameter {
                name: "m".into(),
                annotation: None,
                span: sp,
            }],
            result_type: Some(record()),
            effects: Some(effects.iter().map(|e| e.to_string()).collect()),
            body: block(vec![], int(0)),
        },
        span: sp,
    };

    let program = |arg: &str, param_type: Type, apply_effects: &[&str]| {
        let src = Program {
            budget: jpp::syntax::parse("budget {calls: 1, cost: 0}; 0")
                .unwrap()
                .budget,
            body: block(
                vec![
                    leaf("peek", &["judge"]),
                    leaf("plain", &[]),
                    Statement::Function {
                        name: "apply".into(),
                        function: Function {
                            parameters: vec![Parameter {
                                name: "f".into(),
                                annotation: Some(param_type),
                                span: sp,
                            }],
                            result_type: Some(record()),
                            effects: Some(apply_effects.iter().map(|e| e.to_string()).collect()),
                            body: block(vec![], call("f", int(1))),
                        },
                        span: sp,
                    },
                ],
                call(
                    "apply",
                    Expr {
                        kind: ExprKind::Name(arg.into()),
                        span: at,
                    },
                ),
            ),
        };
        jpp::lower(&src).expect("降级")
    };

    // 传会 judge 的方法给只收纯方法的位置：报在实参上
    let report = check(&program("peek", pure_method.clone(), &[]));
    let d = effect_error(&report, "peek 会 judge，而参数类型只收纯方法");
    assert!(
        d.message.contains("peek"),
        "报文要点出是哪个实参：{}",
        d.message
    );
    assert_eq!(d.span, at, "位置要指着实参本身");

    // 传纯方法就没事
    assert_clean("传纯方法", &check(&program("plain", pure_method, &[])));

    // 把参数类型放宽到 judge 就没事了——注意 apply 自己的标注也得跟着放宽：
    // 参数类型说这个位置会 judge，那 apply 的函数体就真的会 judge
    let wide = method(&["judge"]);
    let report = check(&program("peek", wide, &["judge"]));
    assert!(
        report.find("E-effect").is_none(),
        "参数类型与 apply 的标注都放宽后不该报：\n{}",
        report.render()
    );
}

// ---------------------------------------------------------------- 效应名本身要认得

/// 效应名今天完全不校验：`EFFECT_NAMES`（`check.rs`）定义了却全树零引用，
/// 而前端的词法用的是 Unicode `is_alphabetic()`，于是 `!{ε}` 会被**当成一个叫 ε 的具体效应**
/// 解析成功，再报一条「少了 judge」——**比不写标注还糟**，作者拿到的是指向错误方向的诊断。
///
/// 要的行为：认不得的名字得到一条**说清楚是名字不认识**的诊断，带 Span，并列出认得的四个。
#[test]
fn 认不得的效应名要当场说清楚() {
    let judging = r#"
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "k"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                    unsure: fn(u) { consume(u, "drop"); {ok: unit} }})
}
"#;
    let 查 = |标注: &str| {
        let src = format!(
            "budget {{calls: 1, cost: 0, depth: 8}};{judging}
fn 包一层(m) -> Record !{{{标注}}} {{ peek(m) }}
{{a: 1}}"
        );
        check_source("效应名", &src)
    };

    // 1. 希腊字母 ε：作者想写效应变量，语言还没有这个东西——要说「不认识这个名字」
    let report = 查("ε");
    let d = report.find("E-effect-name").unwrap_or_else(|| {
        panic!(
            "`!{{ε}}` 该报「认不得的效应名」，实际诊断：\n{}",
            report.render()
        )
    });
    assert!(
        d.message.contains('ε'),
        "报文要点出是哪个名字：{}",
        d.message
    );
    assert!(
        d.message.contains("judge") && d.message.contains("ask"),
        "报文要列出认得的名字：{}",
        d.message
    );
    assert!(
        report.find("E-effect").is_none(),
        "名字都不认识，就别再拿它做差集报「少了 judge」——那是指向错误方向的诊断：\n{}",
        report.render()
    );

    // 2. 拼错的名字混在认得的里面
    let report = 查("judge, 拼错的名字");
    let d = report
        .find("E-effect-name")
        .unwrap_or_else(|| panic!("拼错的名字该被认出来：\n{}", report.render()));
    assert!(d.message.contains("拼错的名字"), "{}", d.message);

    // 3. 大小写变体：效应名是小写的四个，`Judge` 不是其中之一
    let report = 查("Judge");
    assert!(
        report.find("E-effect-name").is_some(),
        "`Judge` 不是认得的名字：\n{}",
        report.render()
    );

    // 4. 正向对照：四个都认得，不该报这条
    let report = 查("judge, gen, do, ask");
    assert!(
        report.find("E-effect-name").is_none(),
        "四个都是认得的名字：\n{}",
        report.render()
    );
}
