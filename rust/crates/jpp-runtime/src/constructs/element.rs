//! 元素记录构造 `element`（B133，步 25-2b）：把 `sieve` 的三处旁写收进一个内核构造。
//!
//! `element(输入, 出口, 上下文)` 做三件 `.jpp` 拿不到的事：(1) 输出 `item` = 输入材料并入选择边
//! {出口的账本键}（B92 选择依赖；键为空不加边）；(2) 报告 `exits` 表里该出口那一行写 `index`（原始编号：
//! 输入是元素记录时沿用它的，否则取 `pos`）与 `pos`（B120 (b)）；(3) 按 B81/B82 形状造元素记录。
//! 上下文 `{pos, q, qi, fill?, key}` 比 B133 多一个 `key`：预算未观察的元素的出口没有账本键，元素记录的
//! `key` 是它本该有的键，只能由调用者给（施工解读，`过程记录/工程-步25-2b.md`）。
//!
//! 本步它是**内部构造**：只经 `caps.rs` 的入口 `调元素` 被 `sieve` 调用；开放成 `.jpp` 名字在步 25-11。
//!
//! 依据：B133（地基/附注/2026-09-25-库层出口合成与待补批3裁定.md §三；12 §2.12 element 注）；B81；B82；B92

use crate::caps::Caps;
use crate::*;

/// 输入是不是元素记录（过滤或配对的产物：带 `item` 与 `trail`）。
pub(crate) fn is_element(it: &Value) -> bool {
    matches!(it, Value::Record(_)) && it.get("item").is_some() && it.get("trail").is_some()
}

/// 过滤的输出元素（B81 (a)，步 25-0）：输入是元素记录时保留它的全部字段、同名键原位更新；
/// 否则从空记录起，并把 `index` 赋为输入位置。`index` 此后经任何构造不变；`sets` 依次写入或更新。
pub(crate) fn element_out(it: &Value, i: usize, sets: Vec<(&str, Value)>) -> Value {
    let mut fs: Vec<(String, Value)> = match it {
        Value::Record(r) if is_element(it) => {
            r.iter().filter(|(k, _)| k != "source").cloned().collect()
        }
        _ => vec![(
            "index".to_string(),
            Value::Int(i as i64, Taint::Trusted.into()),
        )],
    };
    for (k, v) in sets {
        match fs.iter_mut().find(|(kk, _)| kk == k) {
            Some(slot) => slot.1 = v,
            None => fs.push((k.to_string(), v)),
        }
    }
    Value::record(fs)
}

/// 元素构造的上下文（B133 的 `ctx`，另加 `key`）。
pub(crate) struct 元素上下文 {
    /// 本次调用输入列表里的位置
    pub pos: usize,
    pub q: Rc<Question>,
    /// 题在列表里的位置（单题为 0）
    pub qi: usize,
    /// 填法形式：调用者给的整条填法记录
    pub fill: Option<Value>,
    /// 该元素读数的账本键（预算未观察的也记它本该有的键）
    pub key: String,
}

impl<'a> Interp<'a> {
    /// 元素构造的实现（只经 `caps.rs::调元素` 进来，令牌按 `element` 的声明发放）。
    pub(crate) fn 元素(
        &mut self,
        caps: &Caps,
        input: &Value,
        exit: &Rc<Exit>,
        ctx: 元素上下文,
    ) -> Value {
        let (material, trail) = element_parts(input);
        // B120 (b)：报告 `exits` 行补元素编号（出口没有行——预算未观察——则不写）
        let 原号 = match input.get("index") {
            Some(Value::Int(n, _)) if is_element(input) => n,
            _ => ctx.pos as i64,
        };
        caps.exit_row().stamp_row(self, exit.id, 原号, ctx.pos);
        // B92：选择依赖边——元素内容先于读数存在，读数只决定选中了它
        let item = caps.source_select().select_edge(material, exit);
        let cause = if exit.is_unsure() {
            Value::text(&exit.cause())
        } else {
            Value::Unit
        };
        let mut sets = vec![
            ("item", item),
            ("pos", Value::Int(ctx.pos as i64, Taint::Trusted.into())),
            ("trail", trail),
            ("exit", Value::Exit(exit.clone())),
            ("cause", cause),
            ("q", Value::Question(ctx.q)),
            ("qi", Value::Int(ctx.qi as i64, Taint::Trusted.into())),
            ("key", Value::text(&ctx.key)),
        ];
        if let Some(f) = ctx.fill {
            sets.push(("fill", f));
        }
        element_out(input, ctx.pos, sets)
    }
}
