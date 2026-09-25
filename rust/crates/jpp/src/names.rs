//! 降级用的名字表（步 12d，`20` §2.3 `lower`「表层名到节点的三条规则」）：外观层在这里把三张表
//! 组装成 [`NameTable`] 交给 `jpp_syntax::lower`。
//!
//! - 效应表：`jpp-effects` 的注册表（名字与输入槽都从 `EffectSpec` 读）；
//! - 构造表：内核构造的名字（`20` §五 S3；`ConstructSig` 在步 25 进 `jpp-ir`，届时改读它）；
//! - 宿主内置表：运行时根环境里其余的名字（[`crate::interp::BUILTINS`]），其中 `map`/`filter`/`fold`
//!   带站点。
//!
//! 语言形式（`state`、`cut`、`fit`、`loop`、`handle` 与三条消费路）按名字映射到各自的节点。
//! 三张表与运行时根环境的一致性由本文件的测试钉住。

use jpp_effects::{ALL, EffectId, spec};
use jpp_ir::ir::{ConsumeHow, NameClass, NameTable};

/// 内核构造（`20` §五 S3）：现行运行时的构造臂与题的构造。
pub const CONSTRUCTS: &[&str] = &[
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "key_of",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
];

/// 带站点的高阶宿主内置（推测与向量化的触发点）。
pub const HIGHER_ORDER: &[&str] = &["map", "filter", "fold"];

/// 语言形式的名字（不是效应、不是构造，各有自己的 IR 节点）。
pub const FORMS: &[&str] = &[
    "state",
    "cut",
    "fit",
    "loop",
    "handle",
    "consume",
    "escalate",
    "literalize",
];

/// 现行名字表：效应读注册表，构造与宿主内置读上面两张表。
pub struct CurrentNames;

impl NameTable for CurrentNames {
    fn classify(&self, name: &str) -> NameClass {
        if let Some(id) = ALL.into_iter().find(|i| spec(*i).name == name) {
            return NameClass::Effect(id);
        }
        match name {
            "state" => NameClass::State,
            "cut" => NameClass::Cut,
            "fit" => NameClass::Fit,
            "loop" => NameClass::Loop,
            "handle" => NameClass::Handle,
            "consume" => NameClass::Consume(ConsumeHow::Consume),
            "escalate" => NameClass::Consume(ConsumeHow::Escalate),
            "literalize" => NameClass::Consume(ConsumeHow::Literalize),
            n if CONSTRUCTS.contains(&n) => NameClass::Construct,
            n if HIGHER_ORDER.contains(&n) => NameClass::HigherOrder,
            _ => NameClass::Plain,
        }
    }
    fn slots(&self, effect: EffectId) -> Vec<&'static str> {
        spec(effect).input_schema.iter().map(|d| d.name).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分类表里的每个名字都在运行时根环境与检查器的内置名单里：新增或改名内置时这里提醒同步
    #[test]
    fn 分类表的名字都是现行内置() {
        for n in CONSTRUCTS.iter().chain(HIGHER_ORDER).chain(FORMS) {
            assert!(crate::interp::BUILTINS.contains(n), "{n} 不是运行时内置");
            assert!(
                crate::check::shapes::BUILTINS.contains(n),
                "{n} 不是检查器内置"
            );
        }
        for id in ALL {
            assert!(crate::interp::BUILTINS.contains(&spec(id).name));
            assert!(crate::check::shapes::BUILTINS.contains(&spec(id).name));
        }
    }
}
