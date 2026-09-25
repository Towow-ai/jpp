//! J-03：线不可字面。`cut`、`test`、`select`、`measure` 的 calib 位与 `form` 选项里的 `calib`
//! 只收校准记录的键（Text），不收数字字面量。文件面（`load` 重跑认证）在校准侧。
//!
//! 步 24d 补 H3（`12` §1.2，`calibration` 类假设）：`cut` 站点若画像给了、`calibration` 字段
//! 未测（判断器的校准测量还没过检），报 `W-untested`，点名字段——代价比线不可用只用保形线这条
//! 降级本身在运行期已生效（`bridge.rs` 代价线分支只取按代价矩阵认证过的证书），这里只是让作者
//! 在检查期就知道「这份画像没告诉我校准过检没有」。
//!
//! 步 24h 补代价记录形状预检（K-143/B29）：`cut` 的字面记录参数（`{cost: [fp, fn]}`，第二或第三
//! 位，取决于给没给 `calib_key`）里的 `cost` 字段不是恰好两个数字字面量的列表时报——运行期
//! （`host_builtins.rs`）已严格校验这一形状（`E-rt-arg`），这里只做能静态确定时的编译期镜像，
//! `cost` 字段值不是字面列表（名字/表达式）时判不出来，放过（静态报出 ⊆ 运行期拒绝）。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-03（线不可字面；calib 参数必须是校准记录的键）；H3（步 24d）。
pub(crate) const RULE: Rule = Rule {
    code: "J-03",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    match s.name {
        "cut" | "test" | "select" => {
            calib_literal(&mut out, s.name, s.args, 1);
            if s.name == "cut" {
                h3_calibration(cx, s, &mut out);
                cost_shape(&mut out, s.args);
            }
        }
        "measure" => calib_literal(&mut out, s.name, s.args, 2),
        // 题式上：calib 写在选项记录里，同样不能是数字字面量
        "form" => {
            if let Some(ExprKind::Record(fields)) = s.args.get(2).map(|a| a.kind()) {
                if let Some((_, v)) = fields.iter().find(|(k, _)| k == "calib") {
                    if matches!(
                        v.kind(),
                        ExprKind::Decimal | ExprKind::Integer(_) | ExprKind::Bool
                    ) {
                        // 依据：12 §5 J-03（线不可字面）
                        out.push(Diagnostic::error(
                            "J-03",
                            "form 的 calib 是数字字面量：线不可字面，这一位只收校准记录的键（Text）。修法：form(…, {calib: \"校准键\"})",
                            v.span,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn calib_literal(out: &mut Vec<Diagnostic>, name: &str, args: &[&Expr], idx: usize) {
    let Some(a) = args.get(idx) else { return };
    if matches!(
        a.kind(),
        ExprKind::Decimal | ExprKind::Integer(_) | ExprKind::Bool
    ) {
        // 依据：12 §5 J-03（线不可字面）
        out.push(Diagnostic::error(
            "J-03",
            format!("{name} 的第 {} 个参数是数字字面量：线不可字面，这一位只收校准记录的键（Text）。修法：{name}(…, \"校准键\")", idx + 1),
            a.span,
        ));
    }
}

/// 代价记录形状预检（步 24h，K-143/B29）：`cut(reading, {cost: [...]})` 或
/// `cut(reading, calib_key, {cost: [...]})`——`cost` 记录可能在第二或第三个实参位，
/// 看第一个是字面记录且含 `cost` 字段的那个（`calib_key` 位是 Text，不是 Record，两者不冲突）。
fn cost_shape(out: &mut Vec<Diagnostic>, args: &[&Expr]) {
    let Some(rec) = args.iter().skip(1).find_map(|a| match a.kind() {
        ExprKind::Record(fields) => Some((a, fields)),
        _ => None,
    }) else {
        return;
    };
    let (rec_expr, fields) = rec;
    let Some((_, v)) = fields.iter().find(|(k, _)| k == "cost") else {
        return;
    };
    let ExprKind::List(items) = v.kind() else {
        return; // 不是字面列表：判不出来，放过
    };
    let 两数 = items.len() == 2
        && items
            .iter()
            .all(|x| matches!(x.kind(), ExprKind::Integer(_) | ExprKind::Decimal));
    if !两数 {
        // 依据：12 §5 J-03（cost 参数形状；K-143/B29，步 24h）
        out.push(Diagnostic::error(
            "J-03",
            "cost 要是两个数字字面量 [fp, fn]：放错一条（假放行）与漏掉一条（假拒绝）的代价。修法：cut(…, {cost: [数, 数]})",
            rec_expr.span,
        ));
    }
}

/// H3（步 24d）：`cut` 站点，画像给了但 `calibration` 未测——代价比线不可用（只用保形线），
/// 检查期点名字段，不等运行期才发现代价证书查不到。
fn h3_calibration(cx: &Cx, s: &CallSite, out: &mut Vec<Diagnostic>) {
    let Some(p) = cx.profile else { return };
    // 依据：H3（12 §1.2，步 24d）
    if !p.calibration_tested() {
        out.push(Diagnostic::warning(
            "W-untested",
            "画像没有测过 calibration（H3）：判断器的校准测量还没过检，代价比线（cut 的 cost 参数）不可用，只用保形线",
            s.span,
        ));
    }
}
