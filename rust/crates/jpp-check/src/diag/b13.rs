//! B13 诊断层第一批：只读题面字面量就能判的规则。
//!
//! 每条规则一个函数，返回 0 或 1 条 `W-diag-*` 告警，报文按「一句话说错在哪。修法：…」。
//! 这些是**提示**，不是错：题面的写法正确与否最终由校准与复核（B19、B36）判定；这里只在
//! 实验已经证实会出问题的写法上提前说出来。模式都按词面匹配，漏报优先于误报。
//!
//! | 码 | B13 条目 | 实测依据 |
//! |---|---|---|
//! | `W-diag-exclusion` | 藏了两个判断：题面含排除条款 | 第四轮 D 题（带排除条款）读数比 R 题更不两极（`实测/校准题式-2026-09-23/结果-第四轮-模型部分.md`） |
//! | `W-diag-two-judgments` | 藏了两个判断：一题两问 | P1「一跳一判」 |
//! | `W-diag-open-question` | 题式是否安全：是非题问的是开放问题，划分与题型不符 | 三种题型即三种划分（B1） |
//! | `W-diag-mention-scope` | 提到类题面须声明外延 | 第三轮人机一致 78%，分歧全部来自「提到」外延（`结果-人工抽检.md`） |
//! | `W-diag-abstract-direct` | 抽象概念配「直接写出」类谓词，外延未定 | 第四轮 D 题错判 34 条中抽象档 25 条 |
//! | `W-diag-meta` | 不用「有助于判断吗」这类元题 | P7 / E8：元题无区分力 |
//! | `W-diag-fill` | 条件不足或多余：填法缺槽或多槽 | 运行期 `fill` 会报错；静态提前说 |
//!
//! 需要判断器的两条（前提在材料里被做出了吗；问的东西在面前材料里吗）留到 `21` 步 26。

use crate::Diagnostic;
use jpp_ir::ir::Span;
use jpp_ir::key::Op;
use jpp_ir::question_kind::{
    KindSource, QuestionKind, Request, SlotDecls, SlotShape, question_kind,
};

/// 一道题的字面信息。`text` 是题面；`form` 为真时它是题式模板（含 `{槽}`）。
/// `kind` 是基础题类（不依赖状态，B76），由 [`jpp_ir::question_kind::question_kind`] 算出。
#[derive(Clone, Copy, Debug)]
pub struct QuestionLit<'a> {
    /// `test` / `select` / `measure`
    pub op: &'a str,
    pub text: &'a str,
    pub form: bool,
    pub span: Span,
    pub kind: QuestionKind,
    pub kind_source: KindSource,
}

impl<'a> QuestionLit<'a> {
    /// 没有请求与槽声明时的题字面量：基础类按 `op` 的缺省推出。
    pub fn new(op: &'a str, text: &'a str, form: bool, span: Span) -> Self {
        Self::with_decl(op, text, form, span, None, &SlotDecls::default())
    }

    /// 带请求与题式槽声明的题字面量。
    pub fn with_decl(
        op: &'a str,
        text: &'a str,
        form: bool,
        span: Span,
        request: Option<Request>,
        decl: &SlotDecls,
    ) -> Self {
        let (kind, kind_source) = question_kind(op_of(op), request, &SlotShape::UNKNOWN, decl);
        QuestionLit {
            op,
            text,
            form,
            span,
            kind,
            kind_source,
        }
    }
}

/// 题型名到 `Op`。认不得的名字按 `test`：检查器这里只作诊断，题型写错由运行期报。
pub(crate) fn op_of(op: &str) -> Op {
    match op {
        "select" => Op::Select,
        "measure" => Op::Measure,
        _ => Op::Test,
    }
}

/// 诊断的上下文。本批规则只读题面，还不需要上下文；步 26 需判断器的规则经它拿端口。
///
/// 步 24g 加三个字段，供 `shape_check`（B51-R2 静态消费者）用：判断站点的状态材料能静态确定来自
/// 哪个声明过 `mat_shape` 的动作时才给 `mat_shape`（判不出来源、或来源动作没声明，一律 `None`，
/// 按可判放过）；`one_hop`/`arithmetic_capable` 是画像 H4/H5 两个字段，未加载档案时按 `Tri::未测`
/// （与既有 H 字段访问器 `unwrap_or_default()` 同一口径）。
#[derive(Clone, Debug, Default)]
pub struct DiagCx {
    pub mat_shape: Option<jpp_effects::MatShape>,
    /// `mat_shape` 来自哪个动作（报文点名用；`mat_shape` 为 `None` 时本字段无意义）
    pub shape_action: Option<String>,
    pub one_hop: jpp_effects::Tri,
    pub arithmetic_capable: jpp_effects::Tri,
}

/// 对一道题（或一个题式模板）的题面跑全部静态规则。
pub fn diagnose_question(q: &QuestionLit, _cx: &DiagCx) -> Vec<Diagnostic> {
    // 模板里的槽不参与词面匹配：`{city}` 这样的槽名不是题面文字
    let text = strip_slots(q.text);
    let what = if q.form { "题式模板" } else { "题面" };
    [
        exclusion(&text, what, q.span),
        two_judgments(&text, what, q.span),
        open_question(q.op, &text, what, q.span),
        mention_scope(q.kind, &text, what, q.span),
        meta(&text, what, q.span),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// 对 `fill(题式, {槽: 值})` 跑填法相关的规则：缺槽 / 多槽，抽象概念配「直接写出」。
/// `fills` 里值为 `None` 的是非字面量填法（只参与槽名核对）。
pub fn diagnose_fill(
    op: &str,
    template: &str,
    fills: &[(String, Option<String>)],
    span: Span,
    _cx: &DiagCx,
) -> Vec<Diagnostic> {
    let _ = op;
    let mut out = vec![];
    if let Ok(slots) = crate::shapes::template_slots(template) {
        let 缺: Vec<&str> = slots
            .iter()
            .filter(|s| !fills.iter().any(|(k, _)| k == *s))
            .map(|s| s.as_str())
            .collect();
        let 多: Vec<&str> = fills
            .iter()
            .filter(|(k, _)| !slots.contains(k))
            .map(|(k, _)| k.as_str())
            .collect();
        if !缺.is_empty() || !多.is_empty() {
            let mut parts = vec![];
            if !缺.is_empty() {
                parts.push(format!("缺槽 {}", 缺.join("、")));
            }
            if !多.is_empty() {
                parts.push(format!("多出 {}", 多.join("、")));
            }
            // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
            out.push(Diagnostic::warning(
                "W-diag-fill",
                format!(
                    "填法与题式「{template}」的槽不符：{}（条件不足或多余，B13）。运行到这里 fill 会报错。修法：按题式的槽 {} 填",
                    parts.join("；"),
                    slots.join("、")
                ),
                span,
            ));
        }
    }
    let text = strip_slots(template);
    if direct_write(&text) {
        for (k, v) in fills {
            if let Some(v) = v {
                if looks_abstract(v) {
                    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
                    out.push(Diagnostic::warning(
                        "W-diag-abstract-direct",
                        format!(
                            "槽 {k} 填的「{v}」看起来是抽象概念，题式却问「直接写出」：抽象概念在材料里多以具体行为出现，「直接写出」对它没有判定边界，外延未定（B13；第四轮 D 题错判集中在抽象档）。修法：改问「是否与「{v}」这个话题相关」，或问「是否描述了体现{v}的具体行为」；若确是具体对象，在题式上声明接受的填法类型（B23）"
                        ),
                        span,
                    ));
                }
            }
        }
    }
    out
}

/// B51-R2 静态消费者（步 24g）：判断站点问的材料若能静态确定来自声明过 `mat_shape` 的动作
/// （`cx.mat_shape`），题类要求的确定性运算不在动作声明的 `settled` 清单里、且对应的画像 H4/H5
/// 字段不利（按 B39 方向，见下）时，报「静态判不了材料是否够回答」（B13「问的东西在面前材料里
/// 吗」的形状面）。
///
/// **题类 → 要求的运算、`settled` 名 → H 字段两张映射都是本条的提案，不是依据文本**——`附注/
/// 2026-09-24-评估①裁定.md` §六的 B76 映射表只给 `op × request × 槽形 → 九题类`，不含「题类要求
/// 哪种运算」；`12` §2.2 B51-R2 原文也没有给。范围刻意收窄到能从原文字面对应出的两类：`Enough`
/// （充分性，「材料由 `Question` 值渲染」＝ B51-R2「问的东西在面前材料里吗」的字面对应，要求
/// `count`）、`Subset`（子集，`all` 向量化，逐元素判断前先数/去重，要求 `count`/`dedup` 任一）；
/// 其余七类判不出运算需求，一律放过。见 `过程记录/工程-步24g.md` §二·2、§二·3。
///
/// H 字段方向：`12` B51-R2 原文字面「H4/H5 为假或未测」与 B39 的逐行方向矛盾（`one_hop: false`
/// 是 B39 定的放宽方向，不该触发警告）；本函数按 B39 方向实现——H4 类运算（`order`/`boundary`/
/// `dedup`）只在 `one_hop == 假` 时抑制；H5 类运算（`count`/`arithmetic`）只在
/// `arithmetic_capable == 真` 时抑制；未测或不利一律照常报。
pub fn shape_check(kind: QuestionKind, span: Span, cx: &DiagCx) -> Option<Diagnostic> {
    let shape = cx.mat_shape.as_ref()?;
    let required: &[&str] = match kind {
        QuestionKind::Enough => &["count"],
        QuestionKind::Subset => &["count", "dedup"],
        _ => return None,
    };
    if required
        .iter()
        .any(|op| shape.settled.iter().any(|s| s == op))
    {
        return None; // 已定案清单覆盖，不报
    }
    let h4_relevant = required
        .iter()
        .any(|op| matches!(*op, "order" | "boundary" | "dedup"));
    let h5_relevant = required
        .iter()
        .any(|op| matches!(*op, "count" | "arithmetic"));
    let h4_suppressed = !h4_relevant || cx.one_hop == jpp_effects::Tri::假;
    let h5_suppressed = !h5_relevant || cx.arithmetic_capable == jpp_effects::Tri::真;
    if h4_suppressed && h5_suppressed {
        return None;
    }
    let 运算 = required.join("/");
    let 已定案 = if shape.settled.is_empty() {
        "（空）".to_string()
    } else {
        shape.settled.join("、")
    };
    let h字段 = match (h4_relevant, h5_relevant) {
        (true, true) => "one_hop（H4）与 arithmetic_capable（H5）",
        (true, false) => "one_hop（H4）",
        _ => "arithmetic_capable（H5）",
    };
    let 动作 = cx.shape_action.as_deref().unwrap_or("（未知）");
    // 依据：12 §2.2 B51-R2（诊断层消费者三处之一）；B39（H4/H5 逐行方向）；步 24g
    Some(Diagnostic::warning(
        "W-diag-shape",
        format!(
            "这道题要求的运算（{运算}）不在动作 {动作} 声明的已定案清单里（{已定案}）：画像 {h字段} 未测或不利，静态判不了材料是否够回答（B51-R2 形状面）。修法：给动作补 `mat_shape.settled` 声明，或提供更完整的画像"
        ),
        span,
    ))
}

// ---------------------------------------------------------------- 规则

fn exclusion(text: &str, what: &str, span: Span) -> Option<Diagnostic> {
    const 排除: &[&str] = &[
        "不算",
        "不包括",
        "除了",
        "除外",
        "但不",
        "排除",
        "不计",
        "except",
        "excluding",
        "not counting",
    ];
    let hit = 排除.iter().find(|w| contains_ci(text, w))?;
    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
    Some(Diagnostic::warning(
        "W-diag-exclusion",
        format!(
            "{what}含排除条款「{hit}」：排除条款是嵌入的第二个判断，一跳要判两件事（P1），读数会更不两极（B13；第四轮带排除条款的题式实测如此）。修法：拆成两道题再用三值合取组合，或把要排除的情形在材料里单独标出"
        ),
        span,
    ))
}

fn two_judgments(text: &str, what: &str, span: Span) -> Option<Diagnostic> {
    let 是否 = text.matches("是否").count();
    let 连接 = ["并且", "而且"].iter().find(|w| text.contains(*w));
    let 既又 = text.contains('既') && text.contains('又');
    if 是否 < 2 && 连接.is_none() && !既又 {
        return None;
    }
    let why = if 是否 >= 2 {
        format!("出现 {是否} 处「是否」")
    } else if let Some(w) = 连接 {
        format!("用「{w}」连接两个条件")
    } else {
        "用「既…又…」连接两个条件".to_string()
    };
    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
    Some(Diagnostic::warning(
        "W-diag-two-judgments",
        format!(
            "{what}{why}：一道题藏了两个判断（B13），读数混着两件事，出口不知道在答哪一件。修法：拆成两道题，用三值合取（all / 逐元素 ∧）组合"
        ),
        span,
    ))
}

fn open_question(op: &str, text: &str, what: &str, span: Span) -> Option<Diagnostic> {
    if op != "test" {
        return None;
    }
    const 是非: &[&str] = &[
        "是否",
        "吗",
        "有没有",
        "是不是",
        "能否",
        "可否",
        "会不会",
        "对不对",
        "有无",
    ];
    if 是非.iter().any(|w| text.contains(w)) {
        return None;
    }
    const 开放: &[&str] = &[
        "为什么",
        "为何",
        "什么",
        "哪",
        "怎么",
        "怎样",
        "如何",
        "多少",
    ];
    let hit = 开放
        .iter()
        .find(|w| text.contains(*w))
        .map(|w| w.to_string())
        .or_else(|| {
            let first = text.trim_start().split_whitespace().next()?.to_lowercase();
            ["why", "what", "which", "how", "who", "where", "when"]
                .contains(&first.as_str())
                .then_some(first)
        })?;
    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
    Some(Diagnostic::warning(
        "W-diag-open-question",
        format!(
            "{what}是开放问句（含「{hit}」），题型却是是非题：是非题只划分「是 / 否」两块，开放问题的真答案不在这两块里，题式不安全（B13、B1）。修法：改写成「是否…」的是非问句；要从候选里挑用 select，要分档用 measure"
        ),
        span,
    ))
}

/// 提及类规则由 `kind == 提及 || 题面关键字启发` 触发（B76 改：原来只看关键字）。
/// `accepts`（B23）落地前 `kind` 不会是提及，触发面与原来相同。
fn mention_scope(kind: QuestionKind, text: &str, what: &str, span: Span) -> Option<Diagnostic> {
    const 提到: &[&str] = &["提到", "提及", "说到", "谈到", "涉及", "mention"];
    let hit = match 提到.iter().find(|w| contains_ci(text, w)) {
        Some(w) => *w,
        None if kind == QuestionKind::Mention => "提及类",
        None => return None,
    };
    const 外延: &[&str] = &[
        "直接",
        "代称",
        "明确",
        "字面",
        "指向",
        "话题",
        "相关",
        "同义",
        "包括",
        "也算",
        "不算",
        "directly",
        "explicitly",
        "refer",
        "topic",
    ];
    if 外延.iter().any(|w| contains_ci(text, w)) {
        return None;
    }
    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
    Some(Diagnostic::warning(
        "W-diag-mention-scope",
        format!(
            "{what}问「{hit}」却没有声明外延：代称、所属成员、联想相关算不算，读题的人各有口径（B13；第三轮人机一致 78%，分歧全部来自这里）。修法：写明口径，例如「是否直接写出了…，或用代称明确指向它（只是话题相关不算）」，或改问「是否与…这个话题相关」"
        ),
        span,
    ))
}

fn meta(text: &str, what: &str, span: Span) -> Option<Diagnostic> {
    const 元题: &[&str] = &[
        "有助于",
        "有帮助",
        "对判断",
        "对回答",
        "helpful",
        "useful for",
    ];
    let hit = 元题.iter().find(|w| contains_ci(text, w))?;
    // 依据：B13（12 §3 J-17 后「诊断层规则集第一批」）
    Some(Diagnostic::warning(
        "W-diag-meta",
        format!(
            "{what}是元题（含「{hit}」）：「这段材料有助于判断吗」这类题实测没有区分力（P7；E8 中 72% 读数低于 0.2）。修法：改问材料里字面可判的前提谓词，例如「这段有观点吗」"
        ),
        span,
    ))
}

// ---------------------------------------------------------------- 助手

/// 去掉模板里的 `{槽}`，留下题面文字
fn strip_slots(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn contains_ci(text: &str, w: &str) -> bool {
    if w.is_ascii() {
        text.to_lowercase().contains(w)
    } else {
        text.contains(w)
    }
}

/// 谓词是「直接写出」一类（按字面出现判定）
fn direct_write(text: &str) -> bool {
    ["直接写出", "直接提到", "字面出现", "写出了"]
        .iter()
        .any(|w| text.contains(w))
}

/// 按词形猜抽象概念：以常见的抽象名词词尾结尾。只按构词，不收任何具体题目里出现过的词，
/// 所以召回有限（例如「孤独」「理财」这类没有抽象词尾的概念认不出来），宁可漏报。
fn looks_abstract(v: &str) -> bool {
    const 词尾: &[&str] = &[
        "性", "感", "观", "主义", "关系", "问题", "意识", "文化", "精神", "理念", "状态", "趋势",
        "风险", "价值", "焦虑",
    ];
    let v = v.trim();
    v.chars().count() >= 2 && 词尾.iter().any(|s| v.ends_with(s) && v != *s)
}
