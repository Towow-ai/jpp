//! 检查器对运行时名单与值形状的只读副本（步 12b）。
//!
//! 检查器不依赖运行时与值模型（`20` §2.2 第 4 条），而名字解析要知道哪些是内置、字段核对要知道题、
//! 题式、契约值各有哪些字段。这四张表是运行时同名常量的副本；`jpp-core` 的测试
//! `tests/check_shapes_sync.rs` 核对两边逐项相等，漏同步即变红。步 12d 起内置名单改由降级读
//! 注册表视图，字段表随 `jpp-ir::contract` 的形状类型收拢（`20` §2.3）。

/// 根环境里的内置名（运行时 `interp::BUILTINS` 的副本）。
pub const BUILTINS: &[&str] = &[
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "key_of",
    "element",
    "cut",
    "handle",
    "consume",
    "gen",
    "do",
    "ask",
    "transform",
    "mat",
    "content",
    "unsure",
    "pending",
    "fail",
    "is_fail",
    "loop",
    "stop",
    "unsure_cause",
    "untested",
    "line_source",
    "cert",
    "compose",
    "taint",
    "escalate",
    "literalize",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
    "fit",
    "len",
    "map",
    "filter",
    "fold",
    "range",
    "append",
    "concat",
    "slice",
    "contains",
    "sum",
    "reverse",
    "keys",
    "with",
    "has",
    "text",
    "join",
    "print",
    "min",
    "max",
    "abs",
    "floor",
    "exit_kind",
    // 文本与数据内置（B157，步 7t）
    "split",
    "lower",
    "upper",
    "trim",
    "replace",
    "starts_with",
    "ends_with",
    "index_of",
    "chars",
    "regex_match",
    "regex_find",
    "sort",
    "sort_by",
    "parse_json",
    "to_json",
    "hash",
    "date_parse",
    "date_format",
    "date_add",
    // 带种子伪随机（B158，步 7t）
    "rand",
    "rand_int",
    "shuffle",
];

/// 题的可读字段（运行时 `interp::QUESTION_FIELDS` 的副本）。
pub const QUESTION_FIELDS: &[&str] = &[
    "text",
    "op",
    "calib",
    "scale",
    "evidence",
    "hash",
    "subject",
    "predicate",
    "partition",
    "request",
    "presupposition",
    "form",
    "template",
    "fill",
];

/// 题式的可读字段（运行时 `interp::FORM_FIELDS` 的副本）。
pub const FORM_FIELDS: &[&str] = &[
    "template",
    "op",
    "slots",
    "calib",
    "scale",
    "evidence",
    "presupposition",
    "request",
    "partition",
    "subject",
    "hash",
];

/// 组合封闭性契约的字段（运行时 `interp::OUTCOME_FIELDS` 的副本，B17）。
pub const OUTCOME_FIELDS: &[&str] = &[
    "kind", "value", "pending", "evidence", "resume", "spent", "detail", "purpose",
];

/// 模板里的槽名（按首次出现顺序去重）；与值模型 `Form::slots_of` 同口径，`jpp-core` 的测试核对。
pub fn template_slots(template: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = vec![];
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('}') else {
            return Err(format!("模板「{template}」里有未闭合的 {{"));
        };
        let name = after[..j].trim();
        if name.is_empty() {
            return Err(format!("模板「{template}」里有空槽 {{}}"));
        }
        if !out.iter().any(|x| x == name) {
            out.push(name.to_string());
        }
        rest = &after[j + 1..];
    }
    Ok(out)
}
