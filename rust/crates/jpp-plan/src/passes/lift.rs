//! pass `lift`（`12` §4 序 1 + :610 修订记录 1 的推测提升）的静态一半：刚登记了一个 `judge`，
//! 后面**同状态**、中间无副作用的 `judge` 一起登记，免得各自等到下一个刷新点各成一层。
//!
//! 步 13a 从运行时的 `lift_followers` 搬来。停在：分支或循环（不跨分支，`12`:13）；会产生副作用或
//! 改状态的调用；重新绑定了状态表达式里用到的名字。按环境才判得出的两问（越过的这一句会不会触世界，
//! K-075；被提的这一句能不能提前求值，K-069）留给运行期经 [`crate::Hooks::may_effect`] 逐句问，
//! 因为前面被提的句子会改变环境。

use crate::analysis::{has_branch, has_impure, has_short_circuit, judged_state, names_in};
use jpp_effects::view::same_shape;
use jpp_ir::ir::{Block, Stmt};
use jpp_ir::plan::{LiftPlan, LiftStep};
use std::collections::BTreeSet;

/// 块 `b` 第 `from` 条语句（须是 `let x = judge(…)`）之后的提升步；不是则 `None`。
pub fn plan(b: &Block, from: usize) -> Option<LiftPlan> {
    let head_state = judged_state(match &b.statements[from] {
        Stmt::Let { value, .. } => value,
        _ => return None,
    })?;
    // 状态表达式用到的名字：谁被重新绑定，就不能再提了
    let mut used = BTreeSet::new();
    names_in(head_state, &mut used);

    let mut steps = vec![];
    for (j, st) in b.statements.iter().enumerate().skip(from + 1) {
        let Stmt::Let { name, value, .. } = st else {
            break;
        };
        // 中间有副作用 / 改状态的调用：停（运行期还要问「会不会触世界」，见 `may_effect`）
        if has_impure(value) || has_branch(value) {
            break;
        }
        let lift = match judged_state(value) {
            // 同状态（结构相同的表达式）才提；不同状态的层合并没有消费者，不做
            Some(s2) if same_shape(s2, head_state) => true,
            // 不是 judge、也不碰状态里的名字：跳过它继续往后看
            None if !used.contains(name.as_str()) => false,
            _ => break,
        };
        // 这一句重新绑定了状态里用到的名字：后面的同名状态已经不是同一个了
        let stop_after = used.contains(name.as_str());
        steps.push(LiftStep {
            index: j,
            node: value.id,
            name: name.clone(),
            lift,
            stop_after,
        });
        if stop_after {
            break;
        }
    }
    Some(LiftPlan { steps })
}

/// 直线段提升穿过函数调用（B94 下半，步 23c）的静态一半：块 `b` 从第 `from` 条语句起（含它自己）的
/// 直线段里，`judge` 站点与「被调者是名字、实参只有名字或字面量」的用户函数调用（按 13b 的条件，运行期
/// 由钩子判被调者是否方法值、实参会不会产生效应、形参与实参个数是否相等）。
///
/// 停在：本句之后第一条含副作用的语句（触世界，K-075）；任一句（含第 `from` 句）含分支、循环或短路
/// 运算（`&&`、`||` 的右侧不一定求值）时停在这一句之前（步 23c 审查修复 4，同 13a）。`if` 分支体与方法字面量里的站点不收（不跨分支，
/// `12`:13）；用到本段里才绑定的名字的站点不收（此刻算不出状态，含第 `from` 句自己绑定的名字）。
/// 同一状态的站点运行期才判得出，由运行时按状态分组、只登记一组里有两处以上的（B94「同一状态的多次 judge」；
/// B155 起按材料哈希分组，同材料上候选集不同的 `select` 算同一组）。
pub fn segment(b: &Block, from: usize) -> Vec<jpp_ir::plan::TargetSite> {
    use std::collections::HashSet;
    let mut out = vec![];
    let mut born: HashSet<String> = HashSet::new();
    for (j, st) in b.statements.iter().enumerate().skip(from) {
        let (value, name) = match st {
            Stmt::Let { name, value, .. } => (value, Some(name)),
            Stmt::Expr(e) => (e, None),
            Stmt::Function { name, .. } => {
                born.insert(name.clone());
                continue;
            }
        };
        if j > from && has_impure(value) {
            break;
        }
        // 审查修复 4：分支与短路右侧不一定走到，提前登记会多花调用、预算紧时多记缺席账
        if has_branch(value) || has_short_circuit(value) {
            break;
        }
        super::speculate::expr(value, &born, &mut out, true);
        if let Some(n) = name {
            born.insert(n.clone());
        }
    }
    out
}
