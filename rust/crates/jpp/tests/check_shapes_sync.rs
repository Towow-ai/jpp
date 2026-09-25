//! 步 12b：检查器（`jpp-check`）持有运行时名单与值形状的副本，因为它不依赖运行时与值模型
//! （`20` §2.2 第 4 条）。这里核对两边逐项相等：漏同步即变红。
use jpp::check::shapes;
use jpp::interp;

#[test]
fn 内置名单与运行时一致() {
    assert_eq!(shapes::BUILTINS, interp::BUILTINS);
}

#[test]
fn 字段表与运行时一致() {
    assert_eq!(shapes::QUESTION_FIELDS, interp::QUESTION_FIELDS);
    assert_eq!(shapes::FORM_FIELDS, interp::FORM_FIELDS);
    assert_eq!(shapes::OUTCOME_FIELDS, interp::OUTCOME_FIELDS);
}

#[test]
fn 模板槽解析与值模型一致() {
    for t in [
        "是否提到{city}？",
        "{a}与{b}是否{a}",
        "无槽",
        "{ x }",
        "未闭合{",
        "空槽{}",
        "",
    ] {
        assert_eq!(
            shapes::template_slots(t),
            jpp::value::Form::slots_of(t),
            "模板「{t}」"
        );
    }
}
