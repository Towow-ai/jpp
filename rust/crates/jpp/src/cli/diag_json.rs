//! 诊断的渲染层（步 9a；评估①建议 6）：文本与机读（`--json`）两种出口、同码同址折叠、
//! 运行期编号 `E-rt-<名>` 的类型说明表。
//!
//! 只管怎么呈现，不改诊断本身：`report.json` 与 `trace.warnings` 原样不动（主会话 2026-09-24 定），
//! 折叠只发生在这里。折叠键是（编号、源位置、报文）三者：编号与位置相同而报文不同的诊断，
//! 报文里带着不同的对象（例如两个校准键），不合并。
//!
//! 机读格式：每条诊断一个 JSON 对象
//! `{code, level, span: {file, line, col, start, end}, message, fix, applicability, count}`，
//! 运行期编号另带 `explain`（编号表里的类型说明）。`fix` 取报文里「修法」之后的文字；
//! `applicability` 为 `manual`（作者改源码）、`wiring`（修法标了【需接线人】）或 `null`（没有修法）。

use std::sync::atomic::{AtomicBool, Ordering};

use jpp_syntax::ast::Span;
use jpp_syntax::loader::LoadedProgram;
use serde_json::{Value, json};

/// 运行期编号与类型说明（步 9a）。`interp/` 里每个 `E-rt-*` 都必须在这里（单元测试核对）。
pub const RT_CODES: &[(&str, &str)] = &[
    ("E-rt-arity", "内置或构造收到的参数个数不对"),
    (
        "E-rt-arg",
        "实参的类型或形状不对；报文给出正确写法（如 slice(list, a, b)）",
    ),
    (
        "E-rt-type",
        "运算、条件或谓词的值类型不对（二元 / 一元运算、if 条件、filter 谓词、len）",
    ),
    ("E-rt-name", "未定义的名字、未知内置、调用了不可调用的值"),
    ("E-rt-field", "记录、材料、题、题式、出口上没有这个字段"),
    ("E-rt-index", "下标越界或值不可索引"),
    ("E-rt-int", "Int 溢出、除以零、取模零（13 §6）"),
    (
        "E-rt-question",
        "题或题式构造不合法（题型、档位、evidence 槽名、request、presupposition、fill 的槽）",
    ),
    ("E-rt-client", "外部组件报错（判断器客户端、gen、ask）"),
    ("E-rt-answer", "判断器答案的形状或条数与题不符"),
    ("E-rt-absent", "判断器缺席且缺席策略为 fail"),
];

/// 编号的类型说明；不是运行期编号返回 `None`。
pub fn explain(code: &str) -> Option<&'static str> {
    RT_CODES.iter().find(|(c, _)| *c == code).map(|(_, d)| *d)
}

static JSON: AtomicBool = AtomicBool::new(false);

/// `--json`：诊断改用机读出口。
pub fn set_json(on: bool) {
    JSON.store(on, Ordering::Relaxed);
}
pub fn json_mode() -> bool {
    JSON.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
        }
    }
}

/// 一条待呈现的诊断。`span` 是全程序偏移；运行期告警没有位置时为 `None`。
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub code: String,
    pub level: Level,
    pub span: Option<Span>,
    pub message: String,
}

impl Item {
    pub fn new(
        code: impl Into<String>,
        level: Level,
        span: Option<Span>,
        message: impl Into<String>,
    ) -> Self {
        Item {
            code: code.into(),
            level,
            span,
            message: message.into(),
        }
    }

    /// 从「编号: 报文」形式的文本拆出编号（降级诊断与运行期告警都是这个形式）。
    /// 没有可认的编号时整段作报文，编号记空串（文本照原样印）。
    pub fn from_prefixed(text: &str, level: Level, span: Option<Span>) -> Self {
        match split_code(text) {
            Some((code, msg)) => Item::new(code, level, span, msg),
            None => Item::new("", level, span, text),
        }
    }

    /// `trace.warnings` 里的一条：`编号: 报文`，位置取报文里第一个 `@<偏移>`。
    /// 不以编号开头的（例如 `returned_unsure: …` 这类记账行）不算诊断。
    pub fn from_warning(w: &str) -> Option<Self> {
        let (code, msg) = split_code(w)?;
        let span = at_offset(msg).map(|p| Span { start: p, end: p });
        Some(Item::new(code, Level::Warning, span, msg))
    }
}

/// `J-10: …`、`W-fixture-line: …`、`E-rt-arg: …` → (编号, 报文)
fn split_code(text: &str) -> Option<(&str, &str)> {
    let (code, rest) = text.split_once(": ")?;
    let ok = (code.starts_with("W-")
        || code.starts_with("E-")
        || code.starts_with("J-")
        || code.starts_with("I-"))
        && code.len() > 2
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    ok.then_some((code, rest))
}

fn at_offset(msg: &str) -> Option<usize> {
    let i = msg.find('@')?;
    let digits: String = msg[i + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 同码同址折叠：（编号、位置、报文）相同的合为一条并计数，保留首次出现的顺序。
pub fn fold(items: Vec<Item>) -> Vec<(Item, usize)> {
    let mut out: Vec<(Item, usize)> = vec![];
    for it in items {
        match out.iter_mut().find(|(x, _)| *x == it) {
            Some((_, n)) => *n += 1,
            None => out.push((it, 1)),
        }
    }
    out
}

/// 报文里的修法与它的适用方式。
pub fn fix_of(message: &str) -> (Option<String>, Option<&'static str>) {
    for (mark, how) in [("修法【需接线人】：", "wiring"), ("修法：", "manual")] {
        if let Some(i) = message.find(mark) {
            let fix = message[i + mark.len()..].trim();
            if !fix.is_empty() {
                return (Some(fix.to_string()), Some(how));
            }
        }
    }
    (None, None)
}

/// 文本形式：沿用 `文件:行:列: 编号: 报文` 与源码指示；折叠了多条时报文后加「（同码同址 ×N）」。
pub fn render_text(loaded: &LoadedProgram, item: &Item, count: usize) -> String {
    let times = if count >= 2 {
        format!("（同码同址 ×{count}）")
    } else {
        String::new()
    };
    let text = if item.code.is_empty() {
        format!("{}{times}", item.message)
    } else {
        format!("{}: {}{times}", item.code, item.message)
    };
    match item.span {
        Some(span) => loaded.render(&jpp_syntax::Diagnostic::new(text, span)),
        None => text,
    }
}

/// 机读形式：一个 JSON 对象。
pub fn to_json(loaded: &LoadedProgram, item: &Item, count: usize) -> Value {
    let (fix, applicability) = fix_of(&item.message);
    let span = item.span.and_then(|s| loaded.locate(s)).map(
        |l| json!({"file": l.file, "line": l.line, "col": l.col, "start": l.start, "end": l.end}),
    );
    let mut v = json!({
        "code": (!item.code.is_empty()).then_some(&item.code),
        "level": item.level.name(),
        "span": span,
        "message": item.message,
        "fix": fix,
        "applicability": applicability,
        "count": count,
    });
    if let Some(e) = explain(&item.code) {
        v["explain"] = json!(e);
    }
    v
}

/// 折叠后按当前模式呈现：文本模式每条一段（换行分隔），`--json` 模式每条一行 JSON。
pub fn render_all(loaded: &LoadedProgram, items: Vec<Item>) -> Vec<String> {
    fold(items)
        .iter()
        .map(|(it, n)| {
            if json_mode() {
                to_json(loaded, it, *n).to_string()
            } else {
                render_text(loaded, it, *n)
            }
        })
        .collect()
}

/// 检查器诊断 → 待呈现的诊断
pub fn from_check(d: &jpp::Diagnostic) -> Item {
    let level = match d.severity {
        jpp::Severity::Error => Level::Error,
        jpp::Severity::Warning => Level::Warning,
    };
    Item::new(
        d.rule.clone(),
        level,
        Some(Span {
            start: d.span.start,
            end: d.span.end,
        }),
        d.message.clone(),
    )
}

/// 降级诊断（报文以编号开头）→ 待呈现的诊断
pub fn from_lower(d: &jpp_syntax::Diagnostic) -> Item {
    Item::from_prefixed(&d.message, Level::Error, Some(d.span))
}

/// 运行期错误 → 待呈现的诊断。没有规则号的编号记空串：文本照旧只印报文，机读 `code` 为 `null`。
pub fn from_runtime(e: &jpp::RtError) -> Item {
    Item::new(
        e.rule.clone().unwrap_or_default(),
        Level::Error,
        Some(Span {
            start: e.span.start,
            end: e.span.end,
        }),
        e.message.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(code: &str, at: usize, msg: &str) -> Item {
        Item::new(code, Level::Warning, Some(Span { start: at, end: at }), msg)
    }

    #[test]
    fn 同码同址同报文才折叠() {
        // 依据：步 9a（评估①建议 6：同码同址折叠、机读出口、运行期编号）
        let items = vec![
            w("W-a", 1, "x"),
            w("W-a", 1, "x"),
            w("W-a", 1, "y"),
            w("W-a", 2, "x"),
            w("W-a", 1, "x"),
        ];
        let f = fold(items);
        assert_eq!(f.iter().map(|(_, n)| *n).collect::<Vec<_>>(), [3, 1, 1]);
        assert_eq!(f[1].0.message, "y");
    }

    #[test]
    fn 修法与适用方式() {
        assert_eq!(
            fix_of("错了。修法：改成 x"),
            (Some("改成 x".into()), Some("manual"))
        );
        assert_eq!(
            fix_of("夹具线。修法【需接线人】：用 calib-import"),
            (Some("用 calib-import".into()), Some("wiring"))
        );
        assert_eq!(fix_of("只是说明"), (None, None));
    }

    #[test]
    fn 告警文本拆编号与位置() {
        // 依据：步 9a（评估①建议 6：同码同址折叠、机读出口、运行期编号）
        let it = Item::from_warning("W-fixture-line: @779 键 k 的线是夹具线").unwrap();
        assert_eq!(
            (it.code.as_str(), it.span.map(|s| s.start)),
            ("W-fixture-line", Some(779))
        );
        assert!(Item::from_warning("returned_unsure: unsure(band)").is_none());
        // 依据：步 9a（评估①建议 6：同码同址折叠、机读出口、运行期编号）
        let it = Item::from_warning("J-10: 没有位置").unwrap();
        assert_eq!(it.span, None);
    }

    /// `interp/` 里用到的每个 `E-rt-*` 都在编号表里，表里每个编号都有人用
    #[test]
    fn 编号表覆盖运行期全部编号() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../jpp-runtime/src");
        let mut used = std::collections::BTreeSet::new();
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let t = std::fs::read_to_string(&p).unwrap();
                    // 依据：步 9a（评估①建议 6：同码同址折叠、机读出口、运行期编号）
                    for (i, _) in t.match_indices("\"E-rt-") {
                        let code: String = t[i + 1..].chars().take_while(|c| *c != '"').collect();
                        used.insert(code);
                    }
                }
            }
        }
        for c in &used {
            assert!(explain(c).is_some(), "interp/ 用了编号表里没有的 {c}");
        }
        for (c, _) in RT_CODES {
            assert!(used.contains(*c), "编号表里的 {c} 在 interp/ 里没有站点");
        }
    }
}
