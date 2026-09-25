//! J-01：读数只经 `cut` 离开。读数不能当条件、不能比、不能算、不能读字段、不能放进要材料、
//! 要出口、要数的槽。哪些表达式是读数由名字趟推出（`analysis/names.rs`）；本文件判位置并出报文。
//! 算术那一面依 H5（`12` §1.3）：档案说模型会算术则降 warn，未测或无档案则照常报错并另报
//! `W-untested`（J-15）。

#![allow(unused_imports)]
use super::{AnalysisId, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-01（读数只经 cut 或 fit 离开；依赖假设 H5，§1.3 降级）。
pub(crate) const RULE: Rule = Rule {
    code: "J-01",
    requires: &[AnalysisId::Names],
    hooks: Hooks {
        reading: Some(reading_err_h5),
        scan_call: Some(builtin_call),
        ..Hooks::NONE
    },
};

/// 读数被放到了要材料 / 要出口 / 要数的位置。
fn builtin_call(cx: &Cx, name: &str, args: &[&Expr]) -> Vec<Diagnostic> {
    let names = cx.names.expect("名字趟的钩子点给出名字视图");
    let inside = |e: &Expr| names.is_reading(e);
    let mut out = vec![];
    // 记账变换（`EffectSpec.in_effect_row` 为假：宿主纯函数作用在材料上，`12` §2.8）的实参都是材料槽；
    // 按注册表字段认它，不写效应名（步 15a，`20` A2）
    let host_transform = jpp_effects::by_name(name).is_some_and(|s| !s.in_effect_row);
    match name {
        _ if host_transform => {
            for a in args {
                if inside(a) {
                    reading_err(
                        &mut out,
                        a.span,
                        format!("读数不能放进 {name} 的槽：读数不是材料"),
                    );
                }
            }
        }
        "state" | "mat" => {
            for a in args {
                if inside(a) {
                    reading_err(
                        &mut out,
                        a.span,
                        format!("读数不能放进 {name} 的槽：读数不是材料"),
                    );
                }
            }
        }
        "content" | "text" | "len" | "sum" => {
            if args.first().map(|a| inside(a)).unwrap_or(false) {
                reading_err(
                    &mut out,
                    args[0].span,
                    format!("读数不能进 {name}：读数没有可读的值"),
                );
            }
        }
        "handle" | "consume" | "exit_kind" | "untested" => {
            if args.first().map(|a| inside(a)).unwrap_or(false) {
                reading_err(
                    &mut out,
                    args[0].span,
                    format!("{name} 的第一个参数要是出口，这里是读数"),
                );
            }
        }
        "contains" | "append" => {
            if args.get(1).map(|a| inside(a)).unwrap_or(false) {
                reading_err(&mut out, args[1].span, "读数不可比、不可存");
            }
        }
        _ => {}
    }
    out
}

/// 算术面按 H5 的档案字段决定 error 还是 warn（`12` §1.3）；其余用法照常是错。
fn reading_err_h5(cx: &Cx, span: Span, msg: &str, 归h5管: bool) -> Vec<Diagnostic> {
    let mut out = vec![];
    if !归h5管 {
        reading_err(&mut out, span, msg);
        return out;
    }
    // H5 的档案字段（`12` §1.2）读 `check` 的输入（B71）。**`None` = 本次没有加载档案**，与
    // `Some(Tri::未测)`（有档案、这项没测）**不是一回事**。
    match cx.profile.map(|p| p.arithmetic_capable()) {
        // 档案明说模型会算术 → H5 不成立 → 降 warn，**并说出是哪个字段让它降的**
        Some(jpp_effects::Tri::真) => {
            out.push(Diagnostic::warning(
                "J-01",
                format!("{msg}。档案 arithmetic_capable: true → H5 不成立，按 12 §1.3 降 warn（fit 仍是唯一跨题合成，I3 不变）"),
                span,
            ));
        }
        // 档案明说不会 → H5 成立 → 照常是错，**不报未测**
        Some(jpp_effects::Tri::假) => reading_err(&mut out, span, msg),
        // 档案说「未测」，或**根本没有档案** → 取该假设为真（保守项），另报 W-untested（J-15）
        other => {
            reading_err(&mut out, span, msg);
            let 载体 = if other.is_some() {
                "档案里 arithmetic_capable 未测"
            } else {
                "本次没有加载档案，arithmetic_capable 无任何测量支持"
            };
            out.push(Diagnostic::warning(
                "W-untested",
                format!("{载体}：按 J-15 取 H5 成立（保守项），J-01 的算术面仍是错。修法：跑 foundation/profile 的 literal_probe 把这个字段测出来"),
                span,
            ));
        }
    }
    out
}

fn reading_err(out: &mut Vec<Diagnostic>, span: Span, msg: impl Into<String>) {
    out.push(Diagnostic::error(
        "J-01",
        format!("{}。修法：cut(读数) 得到出口，再 handle 它", msg.into()),
        span,
    ));
}
