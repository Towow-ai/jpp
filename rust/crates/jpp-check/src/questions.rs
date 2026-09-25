//! 程序里的字面题（步 20a-2b，B116 (5)）：`jpp check --questions-out` 的检查期一半。
//!
//! 校准键迁移（20a-2e）要把作者串记录（`test(题面, "cs-refund")` 一类）迁成零槽题式的主键记录，
//! 缺的只是题面——题面在程序里。这里只读语法与字面量，不执行、不解析名字：
//! - 收 `test(题面, 标签[, 记录])`、`select(题面, 标签[, 记录])`、`measure(题面, [档位…], 标签)`；
//! - 题面、标签、档位都须是字面文本；记录里取 `evidence`（字面文本列表）、`presupposition`、`request`
//!   （字面文本）三个字段，与运行时 `evidence_of`、`question_decl_of` 读的相同；`permute`、`over_kind`
//!   不进题式哈希，不取；
//! - 不是字面量的、题面含花括号的，跳过并给原因（不猜）。名字一律不解析：`let` 绑定不分作用域，
//!   形参同名时会解析成别的值；
//! - `form(…)`、`fill(…)` 不收：题式记录本来就以题式哈希为键。
//!
//! 零槽 `form_hash` 在 `jpp` 一侧用 `jpp_value::value::Form::new` 算（本 crate 不依赖值模型，`20` §2.2）。
//!
//! 依据：B116（地基/附注/2026-09-25-批量裁定.md §五 (c)）；`21` 步 20a-2；`过程记录/工程-步20a-2b.md`。

use crate::analysis::view::ExprKind;
use crate::*;

/// 一道字面题：迁移算零槽题式哈希所需的全部字段，外加站点。
#[derive(Clone, Debug, PartialEq)]
pub struct LiteralQuestion {
    /// 作者写的校准标签（B34：20a-2 后只是类别标签）
    pub label: String,
    /// `test` / `select` / `measure`
    pub op: &'static str,
    pub text: String,
    /// `measure` 的档位；其余为空
    pub scale: Vec<String>,
    pub evidence: Vec<String>,
    pub presupposition: Option<String>,
    pub request: Option<String>,
    pub span: Span,
}

/// 跳过的一处调用与原因。
#[derive(Clone, Debug, PartialEq)]
pub struct SkippedQuestion {
    pub call: &'static str,
    pub reason: String,
    pub span: Span,
}

/// 全程序的字面题与跳过项，都按源码顺序。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiteralQuestions {
    pub questions: Vec<LiteralQuestion>,
    pub skipped: Vec<SkippedQuestion>,
}

fn 字面文本(e: &Expr) -> Option<String> {
    match e.kind() {
        ExprKind::Text(t) => Some(t.clone()),
        _ => None,
    }
}

fn 字面文本列表(e: &Expr) -> Option<Vec<String>> {
    match e.kind() {
        ExprKind::List(items) => items.iter().map(字面文本).collect(),
        _ => None,
    }
}

/// 题的第三个实参（声明记录）里的三个字段。`Err` 是跳过原因。
type 声明 = (Vec<String>, Option<String>, Option<String>);

fn 声明记录(e: Option<&Expr>) -> Result<声明, String> {
    let Some(e) = e else {
        return Ok((vec![], None, None));
    };
    let ExprKind::Record(fields) = e.kind() else {
        return Err("第三个实参不是字面记录".into());
    };
    let 取 = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    let evidence = match 取("evidence") {
        None => vec![],
        Some(v) => 字面文本列表(v).ok_or("evidence 不是字面文本列表")?,
    };
    let 文本字段 = |k: &str| -> Result<Option<String>, String> {
        match 取(k) {
            None => Ok(None),
            Some(v) => 字面文本(v)
                .map(Some)
                .ok_or_else(|| format!("{k} 不是字面文本")),
        }
    };
    Ok((evidence, 文本字段("presupposition")?, 文本字段("request")?))
}

/// 一处调用 → 字面题或跳过原因。不是这三种调用返回 `None`。
fn 一处(e: &Expr) -> Option<(&'static str, Result<LiteralQuestion, String>)> {
    let op: &'static str = match call_name(e)? {
        "test" => "test",
        "select" => "select",
        "measure" => "measure",
        _ => return None,
    };
    let args = call_args(e);
    let 结果 = (|| {
        let text = args
            .first()
            .and_then(|a| 字面文本(a))
            .ok_or("题面不是字面文本")?;
        if text.contains('{') || text.contains('}') {
            // 零槽题式与同文本的带槽题式哈希相同，会撞键；不导出，迁移按缺映射处理（保守）
            return Err("题面含花括号，作零槽题式会与同文本的带槽题式撞键".to_string());
        }
        let (scale, 标签位, 记录位) = match op {
            "measure" => (
                args.get(1)
                    .and_then(|a| 字面文本列表(a))
                    .ok_or("档位不是字面文本列表")?,
                2,
                None,
            ),
            _ => (vec![], 1, args.get(2).copied()),
        };
        let label = args
            .get(标签位)
            .and_then(|a| 字面文本(a))
            .ok_or("标签不是字面文本")?;
        let (evidence, presupposition, request) = 声明记录(记录位)?;
        Ok(LiteralQuestion {
            label,
            op,
            text,
            scale,
            evidence,
            presupposition,
            request,
            span: e.span,
        })
    })();
    Some((op, 结果))
}

/// 程序里的全部字面题（B116 (5)，步 20a-2b）。只读语法与字面量，不执行、不解析名字。
pub fn literal_questions(p: &Program) -> LiteralQuestions {
    let mut out = LiteralQuestions::default();
    walk_block(&p.body, &mut |e| {
        if let Some((call, r)) = 一处(e) {
            match r {
                Ok(q) => out.questions.push(q),
                Err(reason) => out.skipped.push(SkippedQuestion {
                    call,
                    reason,
                    span: e.span,
                }),
            }
        }
    });
    out.questions.sort_by_key(|q| q.span.start);
    out.skipped.sort_by_key(|s| s.span.start);
    out
}
