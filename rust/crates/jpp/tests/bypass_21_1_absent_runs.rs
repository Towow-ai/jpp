//! 步 21-1：仓库里在 `unsure` 臂或逐项 `consume(…, "drop")` 的设计者程序，判断器缺席时要能跑完。
//! 步 21（B95）起缺席类出口（`budget`、`absent`、`latency`）不能 drop；这些程序改为「先看原因，判过的才丢，
//! 没观察到的带出口转交」。本文件把程序源码原样读进来，只做三处测试装配：去掉 `import`、把库文本接在
//! budget 行后（同 `bypass_b81_b82.rs`）、budget 加缺席策略 conservative；再用恒缺席的判断端口跑。
//!
//! 依据：B95；`21` 步 21 追加；主会话 2026-09-25 步 21-1 指示；预注册 `地基/过程记录/工程-步21-1.md`。

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome, run};
use jpp::{lower, syntax::parse};

fn 读(rel: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(rel),
    )
    .unwrap_or_else(|e| panic!("读 {rel}：{e}"))
}

/// 去 import、budget 加缺席策略、库文本接在 budget 行后；`替换` 是测试装配（如把 `input.x` 换成字面量）
fn 装配(src: &str, libs: &[&str], 替换: &[(&str, String)]) -> String {
    let mut body: Vec<&str> = vec![];
    let mut budget = String::new();
    for line in src.lines() {
        if line.starts_with("import ") {
            continue;
        }
        if line.starts_with("budget {") && budget.is_empty() {
            budget = line.replacen(
                "};",
                r#", absent: {retry: 0, backoff: 0, then: "conservative"}};"#,
                1,
            );
            continue;
        }
        body.push(line);
    }
    assert!(!budget.is_empty(), "程序第一处 budget 行");
    let lib: String = libs.iter().map(|l| 读(l) + "\n").collect();
    let mut out = format!("{budget}\n{lib}\n{}\n", body.join("\n"));
    for (a, b) in 替换 {
        assert!(out.contains(a), "装配点 {a} 不在源码里");
        out = out.replace(a, b);
    }
    out
}

/// 判断器恒缺席（每次调用都报错）
fn 缺席<'a>() -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", |_s, _qs| {
            Err(EffectError("连不上".into()))
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, n, _r| {
            Ok(GenResult {
                outputs: (0..n)
                    .map(|i| serde_json::json!(format!("补的候选{i}")))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                ..Default::default()
            })
        }))
}

fn 判过而拿不准<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |s, qs| {
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| {
                    if q.op == jpp::value::Op::Measure {
                        Answer::Score(vec![1.0 / 3.0; 3])
                    } else if q.op == jpp::value::Op::Select {
                        Answer::Choice(vec![1.0 / s.over.len().max(1) as f64; s.over.len().max(1)])
                    } else {
                        Answer::Noul(0.5)
                    }
                })
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

fn 跑(src: &str, ports: Ports<'_>, keys: &[&str]) -> Outcome {
    let program = lower(&parse(src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let mut calib = CalibStore::new();
    for k in keys {
        calib.put(k, 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    }
    run(
        &program,
        ports,
        &calib,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("应当跑完：{}", e.render()))
}

fn 目录() -> Vec<(&'static str, String)> {
    let m: serde_json::Value =
        serde_json::from_str(&读("probes/entity-align/baseline/materials.json")).unwrap();
    vec![
        ("input.catalog_a", m["catalog_a"].to_string()),
        ("input.catalog_b", m["catalog_b"].to_string()),
    ]
}

fn 未决原因(o: &Outcome) -> Vec<String> {
    o.returned_unsure.clone()
}

/// 缺席类原因（B95）：判断器缺席在预算里计费，连续缺席后余下的站点转 `budget`，两种都是没观察到的项
fn 没观察到(c: &str) -> bool {
    ["budget", "absent", "latency"]
        .iter()
        .any(|k| c.contains(k))
}

#[test]
fn align_判断器缺席时跑完并转交() {
    for f in [
        "probes/entity-align/align.jpp",
        "probes/entity-align/t1/align_t1.jpp",
    ] {
        let src = 装配(&读(f), &["lib/outcome.jpp"], &目录());
        let o = 跑(
            &src,
            缺席(),
            &["align-link", "align-name", "align-brewery", "align-style"],
        );
        let u = 未决原因(&o);
        assert!(!u.is_empty() && u.iter().all(|c| 没观察到(c)), "{f}：{u:?}");
        assert!(
            !o.trace.warnings.iter().any(|w| w.starts_with("W-drop")),
            "{f}：缺席项不该被丢：{:?}",
            o.trace.warnings
        );
    }
}

#[test]
fn align_判过而拿不准的字段题照旧丢() {
    let src = 装配(
        &读("probes/entity-align/align.jpp"),
        &["lib/outcome.jpp"],
        &目录(),
    );
    let o = 跑(
        &src,
        判过而拿不准(),
        &["align-link", "align-name", "align-brewery", "align-style"],
    );
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-drop-vs-escalate")),
        "{:?}",
        o.trace.warnings
    );
    assert!(
        !o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-drop-then-return")),
        "{:?}",
        o.trace.warnings
    );
}

#[test]
fn unsure_causes_判断器缺席时跑完并转交() {
    let src = 装配(
        &读("examples/unsure-causes.jpp"),
        &["lib/outcome.jpp", "lib/handlers.jpp"],
        &[],
    );
    let o = 跑(&src, 缺席(), &["cand"]);
    assert!(
        !未决原因(&o).is_empty() && 未决原因(&o).iter().all(|c| 没观察到(c)),
        "{:?}",
        o.returned_unsure
    );
}

#[test]
fn iterate_判断器缺席时跑完并转交() {
    let src = 装配(&读("examples/iterate.jpp"), &["lib/outcome.jpp"], &[]);
    let o = 跑(&src, 缺席(), &["layer"]);
    assert!(
        !未决原因(&o).is_empty() && 未决原因(&o).iter().all(|c| 没观察到(c)),
        "{:?}",
        o.returned_unsure
    );
}
