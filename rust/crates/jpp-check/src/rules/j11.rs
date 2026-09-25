//! J-11 静态面（步 24e）：`do` 的动作名是字面量、检查器拿到了宿主动作表、但这个名字没登记——
//! 检查期就报，不必等运行期真的触发它（同 `do_()` 的报文口径，`crates/jpp-runtime/src/effects_exec.rs`）。
//!
//! 本条是 J-11「材料来源」限制里唯一有静态触发面的部分。核实过其余两半在语言层面已经封闭，
//! 不需要另写检测器：
//! - **「写外层变量要用 `loop`」**：J++ 没有任何赋值语法（`jpp-ir::Stmt` 只有 `Let`/`Function`/
//!   `Expr`，`Host` 没有 `Assign` 变体，`jpp-syntax` 里没有 `set`/`:=`/`+=` 之类的写法）。闭包体
//!   只能用 `let` 遮蔽本地名字，结构上碰不到外层绑定——这条约束由语法保证，不是运行期或静态
//!   检查器的活。
//! - **「材料只能来自字面量、IR 形式输出、`transform` 输出」**（正面的来源限制）：J++ 是全表达式
//!   语言，值只能从字面量、名字（回溯到 `let`/形参/入口）、容器、取字段/下标、运算、`if`、六种
//!   效应形式（含 `transform`）逐层构造；没有任何语法通道能让一个"来路不明"的值绕过这些形式
//!   进入程序。这条正面限制因此也由语言的封闭构造集保证。
//!
//! `transform` 输出必须能成材料（运行期已核，`effects_exec.rs` 两处）不在本批做静态镜像：
//! 唯一可能的静态信号是方法体的结果字面上就是 `cut`/`judge`/`state`/`fn`，覆盖面很窄，价值低，
//! 登记进问题清单不做。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;
use jpp_effects::spec::{ProfileSchema, SlotKind};

/// 依据：12 §5 J-11（材料来源限制）；步 24e。
pub(crate) const RULE: Rule = Rule {
    code: "J-11",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let Some(sp) = jpp_effects::by_name(s.name) else {
        return out;
    };
    // 只管「动作」效应（do），按 profile_schema 泛化匹配，不写死 "do" 这个名字
    if sp.profile_schema != ProfileSchema::Action {
        return out;
    }
    let Some(pos) = sp
        .input_schema
        .iter()
        .position(|d| d.kind == SlotKind::Name)
    else {
        return out;
    };
    let Some(name_arg) = s.args.get(pos) else {
        return out;
    };
    let ExprKind::Text(name) = name_arg.kind() else {
        // 动作名不是字面量：静态判不了，交运行期
        return out;
    };
    // 没有动作表：检查期不知道注册了什么，不报（同 J-08 静态子面的口径，避免库调用方被误报）
    let Some(actions) = cx.actions else {
        return out;
    };
    if !actions.actions.contains_key(name.as_str()) {
        let mut 表: Vec<String> = actions
            .actions
            .iter()
            .map(|(n, a)| {
                format!(
                    "{n}（{}）",
                    if a.reversible {
                        "可逆"
                    } else {
                        "**不可逆**"
                    }
                )
            })
            .collect();
        表.sort();
        // 依据：12 §5 J-11（材料来源限制；do 只能触发登记过的动作）
        out.push(Diagnostic::error(
            "J-11",
            format!(
                "动作 {name} 未登记：do 只能触发登记过的动作（register）。本次登记了：{}",
                表.join("、")
            ),
            name_arg.span,
        ));
    }
    out
}
