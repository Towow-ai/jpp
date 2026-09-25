//! `jpp check --questions-out <file.json>`（步 20a-2b，B116 (5)）：把程序里的字面题按作者写的校准标签分组导出，
//! 每道题带零槽题式哈希，供校准键迁移（20a-2e）把作者串记录迁成主键记录。
//!
//! 字面题的收集在检查器（`jpp_check::questions`，只读语法与字面量）；零槽 `form_hash` 在这里用
//! `jpp::value::Form::new` 算——与运行时 `form(op, 题面, {…})` 同一个构造函数，题面已排除花括号，所以槽为空。
//! 同一标签下同一道题合并、列全部站点；不同的题各一项，按首次出现排序。
//!
//! 依据：B116（地基/附注/2026-09-25-批量裁定.md §五 (c)）；`过程记录/工程-步20a-2b.md`。

use jpp::value::{Form, Op};
use jpp_syntax::loader::LoadedProgram;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn 站点(loaded: &LoadedProgram, span: jpp::ir::Span) -> Value {
    match loaded.locate(jpp_syntax::ast::Span {
        start: span.start,
        end: span.end,
    }) {
        Some(l) => json!({"file": l.file, "line": l.line, "col": l.col, "start": l.start}),
        None => json!({"start": span.start}),
    }
}

fn op_of(op: &str) -> Op {
    match op {
        "select" => Op::Select,
        "measure" => Op::Measure,
        _ => Op::Test,
    }
}

/// 导出并写文件；返回（标签数、题数、跳过数）供宿主打一行提示。
pub fn write(
    program: &jpp::Program,
    loaded: &LoadedProgram,
    source: &Path,
    out: &Path,
) -> Result<(usize, usize, usize), String> {
    let lq = jpp_check::questions::literal_questions(program);
    // 标签 → [(题的身份, 条目)]，按首次出现
    let mut labels: BTreeMap<String, Vec<(Value, Value)>> = BTreeMap::new();
    for q in &lq.questions {
        let form = Form::new(
            op_of(q.op),
            &q.text,
            "",
            q.scale.clone(),
            q.evidence.clone(),
            q.presupposition.clone(),
            q.request.clone(),
        )
        .map_err(|e| format!("题面「{}」算不出题式哈希：{e}", q.text))?;
        // 依据：B116 推翻条件 (1)（零槽题式：没有槽）；花括号题面已在检查器一侧跳过
        if !form.slots.is_empty() {
            return Err(format!(
                "题面「{}」解析出槽 {:?}，不是零槽题式",
                q.text, form.slots
            ));
        }
        let 身份 = json!([
            q.op,
            q.text,
            q.scale,
            q.evidence,
            q.presupposition,
            q.request
        ]);
        let 条目们 = labels.entry(q.label.clone()).or_default();
        match 条目们.iter_mut().find(|(id, _)| *id == 身份) {
            Some((_, e)) => e["sites"]
                .as_array_mut()
                .expect("sites 是数组")
                .push(站点(loaded, q.span)),
            None => 条目们.push((
                身份,
                json!({
                    "op": q.op, "text": q.text, "scale": q.scale, "evidence": q.evidence,
                    "presupposition": q.presupposition, "request": q.request,
                    "form_hash": form.hash, "sites": [站点(loaded, q.span)],
                }),
            )),
        }
    }
    let 题数: usize = labels.values().map(Vec::len).sum();
    let doc = json!({
        "about": "jpp check --questions-out（步 20a-2b，B116 (5)）：程序里的字面题，按作者写的校准标签分组；form_hash 是零槽题式的题式哈希，与 form(op, 题面, {同声明}) 的 form_hash 同算法。只读语法与字面量：题面、标签或声明不是字面量的调用列在 skipped，给出原因。",
        "source": source.display().to_string(),
        "labels": labels
            .iter()
            .map(|(k, v)| (k.clone(), Value::Array(v.iter().map(|(_, e)| e.clone()).collect())))
            .collect::<serde_json::Map<String, Value>>(),
        "skipped": lq
            .skipped
            .iter()
            .map(|s| json!({"call": s.call, "reason": s.reason, "site": 站点(loaded, s.span)}))
            .collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n";
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok((labels.len(), 题数, lq.skipped.len()))
}
