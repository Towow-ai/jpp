//! 检查器对着前端已经写好的 `.jpp` 源码跑一遍。
//!
//! 这是检查器最要紧的验收：**不误杀**。三份样例（组合、自适应选问、部分候选续解）都必须一条错都不报；
//! 报了就是检查器的问题，不是样例的问题——样例归前端，core 不改它们。
//! 诊断在这里按文件打印出来，前端据此对齐报文格式。

mod common;

use std::path::Path;

use common::with_effects;
use jpp::check;
use jpp::{lower, syntax::parse};

fn lowered(file: &str) -> jpp::Program {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(file);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
    lower(&parse(&source).expect("解析通过")).expect("lower 通过")
}

fn report_for(file: &str) -> (String, jpp::Report) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(file);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
    let parsed =
        parse(&source).unwrap_or_else(|d| panic!("{} 解析失败：{}", file, d.render(file, &source)));
    let core = lower(&parsed)
        .unwrap_or_else(|d| panic!("{} lower 失败：{}", file, d[0].render(file, &source)));
    (source, check(&core))
}

#[test]
fn 样例源码一条错都不该报() {
    for file in ["composition.jpp", "adaptive.jpp", "partial.jpp"] {
        let (source, report) = report_for(file);
        let errors: Vec<String> = report
            .errors()
            .iter()
            .map(|d| {
                format!(
                    "  {} @ {}",
                    d.render(),
                    &source[d.span.start..d.span.end.min(source.len())]
                )
            })
            .collect();
        assert!(
            errors.is_empty(),
            "{file} 被检查器误杀：\n{}",
            errors.join("\n")
        );
        if !report.warnings().is_empty() {
            println!("{file} 的提示：");
            for w in report.warnings() {
                println!(
                    "  {} @ {}",
                    w.render(),
                    &source[w.span.start..w.span.end.min(source.len())]
                );
            }
        }
    }
}

/// 前端给的三个错误样例：检查器或解析器要认出来，且位置落在源码里。
#[test]
fn 错误样例要被认出来() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/errors");
    let path = dir.join("missing-budget.jpp");
    let source = std::fs::read_to_string(&path).expect("读得到缺预算样例");
    // 步 12d 起缺预算在降级处报 J-07a（IR 的预算必填），不再到检查器
    let ds = lower(&parse(&source).expect("语法本身没问题")).expect_err("缺预算应当降级失败");
    let d = &ds[0];
    assert!(
        d.message.starts_with("J-07a: "),
        "缺预算应当报 J-07a：{ds:?}"
    );
    assert!(d.span.end <= source.len(), "位置要落在源码里");
}

/// 高阶函数的**错**标注必须拦得住，而且位置要精确。
///
/// 这是这一包的验收判据：首包做不到——被调者只要是参数，整条推断就退成未知、检查直接跳过，
/// 于是 `solve`、`advance` 这些真正带效应的高阶函数一个都核不了。现在两条改动把它们捞回来：
/// 创建方法不等于执行方法；解析不了的被调者只让推断变成下界，缺漏照报。
#[test]
fn 高阶函数的错标注拦得住() {
    let cases: [(&str, &[(&str, &[&str])]); 2] = [
        (
            "adaptive.jpp",
            // solve 的效应只能从「调用点把 step 实例化给 method 参数」得到
            &[
                ("solve", &["judge"]),
                ("step", &["judge"]),
                ("observe", &["judge"]),
            ],
        ),
        (
            "partial.jpp",
            &[
                ("advance", &["judge", "do"]),
                ("validate", &["judge", "do"]),
                ("read", &["judge"]),
                ("inspect", &["do"]),
            ],
        ),
    ];
    for (file, targets) in cases {
        let (source, clean) = report_for(file);
        assert!(clean.is_ok(), "{file} 本身应当是干净的：{}", clean.render());
        for (name, real) in targets {
            let (bad, at) = with_effects(&lowered(file), name, &[]);
            let report = check(&bad);
            let d = report
                .diagnostics
                .iter()
                .find(|d| d.rule == "E-effect" && d.message.starts_with(name))
                .unwrap_or_else(|| {
                    panic!(
                        "{file} 的 {name} 标成 !{{}} 应当报 E-effect：\n{}",
                        report.render()
                    )
                });
            assert_eq!(d.span, at, "位置要指着 {name} 的定义");
            for effect in *real {
                assert!(
                    d.message.contains(effect),
                    "{name} 应当报出少了 {effect}：{}",
                    d.message
                );
            }
            assert!(d.span.end <= source.len(), "位置要落在源码里");

            // 正确标注不得被误杀
            let (good, _) = with_effects(&lowered(file), name, real);
            let report = check(&good);
            assert!(
                !report
                    .diagnostics
                    .iter()
                    .any(|d| d.rule == "E-effect" && d.message.starts_with(name)),
                "{file} 的 {name} 标对了却被报：\n{}",
                report.render()
            );
        }
    }
}
