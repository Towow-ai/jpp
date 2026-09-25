//! `do` 实参 `Value` 与普通 Rust 类型之间的薄适配，`exec.rs`/`retrieval.rs` 共用。
//! 从 `cli/actions_r2a.rs`（R2a）原样搬来。

use crate::value::Value;

pub(super) fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Text(s, _) => Some(s.to_string()),
        Value::Mat(m) => Some(
            m.content
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| m.content.to_string()),
        ),
        _ => None,
    }
}

pub(super) fn value_number(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i, _) => Some(*i as f64),
        Value::Float(f, _) => Some(*f),
        _ => None,
    }
}

pub(super) fn value_to_texts(v: &Value) -> Result<Vec<String>, String> {
    match v {
        Value::List(items) => items
            .iter()
            .map(|x| value_text(x).ok_or_else(|| "列表的每个元素必须是文本或材料".to_string()))
            .collect(),
        _ => Err("必须是列表".into()),
    }
}

pub(super) fn value_to_assertions(v: &Value) -> Result<Vec<String>, String> {
    match v {
        Value::List(items) => items
            .iter()
            .map(|x| value_text(x).ok_or_else(|| "tests 列表的每个元素必须是文本".to_string()))
            .collect(),
        Value::Text(s, _) => Ok(s
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()),
        _ => Err("tests 必须是文本列表或按行分隔的文本".into()),
    }
}

pub(super) fn hits_to_value(hits: Vec<(usize, f64)>) -> Value {
    Value::list(
        hits.into_iter()
            .map(|(id, score)| {
                Value::record(vec![
                    ("id".to_string(), Value::int(id as i64)),
                    ("score".to_string(), Value::float(score)),
                ])
            })
            .collect(),
    )
}
