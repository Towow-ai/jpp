//! B13 诊断层第一批：每条规则一条命中、一条不命中（`21` 步 5）。
//! 依据：B13（`12` §3 J-17 后「诊断层规则集第一批」）。

use jpp::Span;
use jpp::check::diag::{DiagCx, QuestionLit, diagnose_fill, diagnose_question};

fn q(op: &'static str, text: &'static str) -> Vec<String> {
    let span = Span { start: 0, end: 0 };
    diagnose_question(&QuestionLit::new(op, text, false, span), &DiagCx::default())
        .into_iter()
        .map(|d| d.rule)
        .collect()
}
fn f(template: &str, fills: &[(&str, &str)]) -> Vec<String> {
    let span = Span { start: 0, end: 0 };
    let fills: Vec<(String, Option<String>)> = fills
        .iter()
        .map(|(k, v)| (k.to_string(), Some(v.to_string())))
        .collect();
    diagnose_fill("test", template, &fills, span, &DiagCx::default())
        .into_iter()
        .map(|d| d.rule)
        .collect()
}

#[test]
fn 排除条款_命中与不命中() {
    assert!(
        q("test", "这段话是否直接写出了钢琴？（只是话题相关不算）")
            .contains(&"W-diag-exclusion".to_string())
    );
    assert!(
        !q("test", "这段话的内容是否与钢琴这个话题相关？")
            .contains(&"W-diag-exclusion".to_string())
    );
}

#[test]
fn 一题两问_命中与不命中() {
    assert!(
        q("test", "候选人是否会写网页，并且是否懂数据库？")
            .contains(&"W-diag-two-judgments".to_string())
    );
    assert!(!q("test", "候选人是否会写网页？").contains(&"W-diag-two-judgments".to_string()));
}

#[test]
fn 开放问句配是非题_命中与不命中() {
    assert!(q("test", "这段话讲的是什么城市？").contains(&"W-diag-open-question".to_string()));
    assert!(q("test", "Why did the build fail?").contains(&"W-diag-open-question".to_string()));
    assert!(
        !q("test", "这段话是否提到了什么城市名？").contains(&"W-diag-open-question".to_string())
    );
    // select 本来就是在候选里挑，开放问句合法
    assert!(!q("select", "哪个候选最合适？").contains(&"W-diag-open-question".to_string()));
}

#[test]
fn 提到类须声明外延_命中与不命中() {
    assert!(q("test", "这段话是否提到了钢琴？").contains(&"W-diag-mention-scope".to_string()));
    assert!(
        !q("test", "这段话是否直接写出了钢琴，或用代称明确指向它？")
            .contains(&"W-diag-mention-scope".to_string())
    );
}

#[test]
fn 元题_命中与不命中() {
    assert!(q("test", "这段材料是否有助于判断作者立场？").contains(&"W-diag-meta".to_string()));
    assert!(!q("test", "这段有观点吗？").contains(&"W-diag-meta".to_string()));
}

#[test]
fn 抽象概念配直接写出_命中与不命中() {
    assert!(
        f("这段话是否直接写出了{c}？", &[("c", "职业焦虑")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
    assert!(
        !f("这段话是否直接写出了{c}？", &[("c", "钢琴")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
    // 话题相关类谓词配抽象概念不报
    assert!(
        !f("这段话是否与「{c}」这个话题相关？", &[("c", "职业焦虑")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
}

#[test]
fn 填法缺槽多槽_命中与不命中() {
    assert!(f("{a}是否在{b}里？", &[("a", "苹果")]).contains(&"W-diag-fill".to_string()));
    assert!(
        f("{a}是否在清单里？", &[("a", "苹果"), ("x", "多余")])
            .contains(&"W-diag-fill".to_string())
    );
    assert!(
        !f("{a}是否在{b}里？", &[("a", "苹果"), ("b", "清单")])
            .contains(&"W-diag-fill".to_string())
    );
}

#[test]
fn 模板槽名不参与词面匹配() {
    // 槽名 mention 不应触发「提到」规则
    let span = Span { start: 0, end: 0 };
    let r: Vec<String> = diagnose_question(
        &QuestionLit::new("test", "{mention}是否成立？", true, span),
        &DiagCx::default(),
    )
    .into_iter()
    .map(|d| d.rule)
    .collect();
    assert!(r.is_empty(), "{r:?}");
}
