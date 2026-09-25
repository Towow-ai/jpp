//! 效应行写在**类型位**上（`Fn(A) -!{judge}-> B`、`Fn1(…)` 是 `Fn¹`）。
//!
//! 这份单独成文件，因为它依赖前端的类型文法——那是 Codex 归口，落地时间与 core 这边不同步。
//! `tests/effect_rows.rs` 里的推断那半不依赖它，任何时候都能跑。

use jpp::{Report, check};
use jpp::{lower, syntax::parse};

fn check_source(what: &str, source: &str) -> Report {
    let parsed =
        parse(source).unwrap_or_else(|d| panic!("{what} 解析失败：{}", d.render(what, source)));
    let core = lower(&parsed)
        .unwrap_or_else(|d| panic!("{what} lower 失败：{}", d[0].render(what, source)));
    check(&core)
}

fn effect_error<'a>(report: &'a Report, what: &str) -> &'a jpp::Diagnostic {
    report
        .find("E-effect")
        .unwrap_or_else(|| panic!("{what} 应当报 E-effect，实际诊断：\n{}", report.render()))
}

fn assert_clean(what: &str, report: &Report) {
    assert!(report.is_ok(), "{what} 不该报错：\n{}", report.render());
}

const POLY_BASE: &str = r#"
fn peek(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("行吗", "fixture"))), {act: fn() { {ok: true} }, ignore: fn() { {ok: false} },
                                                          unsure: fn(cause) { {ok: unit, cause: cause} }})
}
fn plain(m) -> Record !{} { {ok: true} }
"#;

// ---------------------------------------------------------------- 效应行写在类型位上

/// 前端已经支持把效应行写进**类型**（`Fn(A) -!{judge}-> B`，`Fn1(…)` 是捕获了责任的 `Fn¹`）。
/// 标了之后契约跟着类型走：不靠推断、不靠调用点实例化，当场就有约束。
#[test]
fn 类型位上的效应行当场生效() {
    // 参数标了会 judge，函数却标成纯的：不需要任何调用点就该被拦下
    let src = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn apply(m, f: Fn(Record) -!{{judge}}-> Record) -> Record !{{}} {{ f(m) }}
{{a: 1}}"
    );
    let report = check_source("类型位(函数标成纯的)", &src);
    assert!(
        effect_error(&report, "参数类型说会 judge")
            .message
            .starts_with("apply")
    );

    let ok = src.replace(
        "fn apply(m, f: Fn(Record) -!{judge}-> Record) -> Record !{}",
        "fn apply(m, f: Fn(Record) -!{judge}-> Record) -> Record !{judge}",
    );
    assert_clean("标对了不该报", &check_source("类型位(标对)", &ok));
}

/// 参数类型上的效应行是**对实参的契约**：类型说只收纯方法，传一个会 judge 的进来就是错，
/// 诊断指着**传错东西的那个实参**，不是函数定义。
#[test]
fn 实参必须在参数类型的效应行之内() {
    let src = format!(
        "budget {{calls: 1, cost: 0, depth: 8}};{POLY_BASE}
fn apply(m, f: Fn(Record) -!{{}}-> Record) -> Record !{{}} {{ f(m) }}
{{a: apply(mat({{x: 1}}), peek).ok}}"
    );
    let report = check_source("实参超出参数类型的效应行", &src);
    let d = effect_error(&report, "peek 会 judge，而参数类型只收纯方法");
    assert!(
        d.message.contains("peek"),
        "报文要点出是哪个实参：{}",
        d.message
    );
    assert!(
        &src[d.span.start..d.span.end] == "peek",
        "位置要指着实参本身，实际指着「{}」",
        &src[d.span.start..d.span.end]
    );

    // 传纯方法就没事
    let ok = src.replace("apply(mat({x: 1}), peek)", "apply(mat({x: 1}), plain)");
    assert_clean("传纯方法不该报", &check_source("实参合契约", &ok));

    // 把参数类型放宽到 judge 也没事
    let widened = src.replace(
        "f: Fn(Record) -!{}-> Record) -> Record !{}",
        "f: Fn(Record) -!{judge}-> Record) -> Record !{judge}",
    );
    assert_clean(
        "参数类型放宽后不该报",
        &check_source("参数类型放宽", &widened),
    );
}
