//! 桥的判序（`12` §2.3；`20` §2.3 L1 `jpp-value` 的 `bridge`）：读数 + 线 → 出口种类。
//!
//! 步 8a-2（R）：从 `jpp-core` 运行时 `cut_inner` 的判序部分原样抽出，成纯函数。
//! 运行时只保留查找（线从哪一级来）、登记与留痕；判序在这里，一处。
//! 依据：`12` §2.3 判序（insufficient 在运行时查线之前已处理）、J-15（未测取保守并带修法）、B32、B29。

use std::cell::{Cell, RefCell};

use jpp_ir::ir::Span;

use crate::value::{Answer, Exit, ExitKind, Op, Taint};

/// 出口的各部分（运行时查好的事实）。
pub struct ExitParts {
    pub id: usize,
    pub kind: ExitKind,
    pub untested: Option<String>,
    pub op: Op,
    pub q_hash: String,
    pub state_hash: String,
    pub taint: Taint,
    pub line_source: String,
    pub site: Span,
}

/// **出口的唯一构造处**（`20` A3、§2.4「出口只由桥产生」）。`Exit` 标了 `#[non_exhaustive]`，
/// 本 crate 以外写不出 `Exit { .. }`；运行时的所有出口（`cut`、`ask`、构造里的派生出口）都经这里。
///
/// ```compile_fail
/// // 依据：20 §2.4 `bypass/exit_ctor`：外部 crate 不能用结构体字面量造出口
/// use jpp_value::value::{Exit, ExitKind, Op, Taint};
/// let _ = Exit { id: 0, op: Op::Test, kind: ExitKind::Act, q_hash: String::new(), state_hash: String::new(),
///     taint: Taint::Trusted, site: Default::default(), from_ask: Default::default(), consumed: Default::default(),
///     consumed_by: Default::default(), line_source: String::new(), untested: None, ledger_key: Default::default(),
///     grade: Default::default(), suspend_candidate: Default::default(), scope_out: Default::default(),
///     delta_unknown: Default::default(), scope_unknown: Default::default(), parts: Default::default() };
/// ```
pub fn issue(p: ExitParts) -> Exit {
    Exit {
        id: p.id,
        op: p.op,
        kind: p.kind,
        q_hash: p.q_hash,
        state_hash: p.state_hash,
        taint: p.taint,
        site: p.site,
        from_ask: Cell::new(false),
        consumed: Cell::new(false),
        consumed_by: RefCell::new(String::new()),
        untested: p.untested,
        line_source: p.line_source,
        ledger_key: RefCell::new(String::new()),
        grade: Cell::new(None),
        suspend_candidate: Cell::new(false),
        scope_out: Cell::new(false),
        delta_unknown: Cell::new(false),
        scope_unknown: Cell::new(false),
        parts: RefCell::new(Vec::new()),
    }
}

/// 判序的输入：运行时查好线之后的全部事实。
pub struct CutInput<'a> {
    /// 读数本身是 Fail（状态含 Fail 材料）
    pub fail: Option<&'a str>,
    /// 判断器缺席或超时的标记（B32）
    pub absent: Option<&'a str>,
    /// 记录状态是停岗
    pub suspended: bool,
    /// 查到的线 `(hi, lo)`；`None` = 冷
    pub line: Option<(f64, f64)>,
    /// 调用者给了代价矩阵（找不到同代价的证书线时，冷的修法不同）
    pub cost_requested: bool,
    /// 刷新之后的答案（只在需要比线时读）
    pub answer: Option<Answer>,
    /// 这条线的 δ（线附近 ±δ 为 band）。`None` = 记录没有 δ（`20` §3.9：出口一律 `Unsure(untested)`，
    /// 载体 `Delta`；步 15d-2 起 δ 只从校准记录取，没有代码兜底）
    pub delta: Option<f64>,
    /// 置换众数占比（`None` = 本次路径上没测过置换）
    pub mode_share: Option<f64>,
}

/// 未测载体与修法提示（J-15）：由判序产生，运行时统一出告警。
pub type Untested = Option<(String, String)>;

/// 取最大分量：返回 `(下标, 值)`。
pub fn argmax(v: &[f64]) -> (usize, f64) {
    let mut best = (0usize, f64::MIN);
    for (i, p) in v.iter().enumerate() {
        if *p > best.1 {
            best = (i, *p);
        }
    }
    best
}

/// 判序：Fail → 缺席 → 停岗 → 冷 → 按题型过线（是非题带 ±δ 的 band；选择题要求置换众数一致）。
pub fn decide(i: &CutInput) -> (ExitKind, Untested) {
    if let Some(f) = i.fail {
        return (ExitKind::Unsure(format!("fail:{f}")), None);
    }
    if let Some(c) = i.absent {
        // B32：判断器缺席或超时，出口按 J-05 四条去向路由，不加新去向
        return (ExitKind::Unsure(c.to_string()), None);
    }
    if i.suspended {
        // `drift` 不是未测：停岗是「测过、而且测出漂了」。停岗在回退之前返回，类级先验放行不了它。
        return (ExitKind::Unsure("drift".into()), None);
    }
    let Some((hi, lo)) = i.line else {
        // 题级没上岗、回退层也没上岗 → 冷
        return if i.cost_requested {
            (
                ExitKind::Unsure("cold".into()),
                Some((
                    "cost_line".into(),
                    "修法【需接线人】：用 commission_costed（或 calib-import）为这个代价矩阵从带真值样本认证一条线；线只来自记录".into(),
                )),
            )
        } else {
            (
                ExitKind::Unsure("cold".into()),
                Some((
                    "calib_line".into(),
                    "修法【作者可改】：用 calib-import 从带真值样本为这道题或它的题式认证一条线（模式级记录不供线，B44）".into(),
                )),
            )
        };
    };
    // 依据：`20` §3.9 数值字段未测行为表「记录的 delta」行（步 15d-2）
    let Some(delta) = i.delta else {
        return (
            ExitKind::Unsure("untested".into()),
            Some((
                "Delta".into(),
                "修法【需接线人】：这条线的校准记录没有 δ；用 calib-import 带画像重新导入，或在夹具的 calibrations 里给出 delta（δ 只从记录取，B104、步 15d-2）".into(),
            )),
        );
    };
    match i.answer.as_ref().expect("刷新之后答案必然在") {
        // `12`:167「再过线，再 band（线附近 ±δ）」。裸的 `p >= hi` 是失败开放（跨内核对照照出，E-JPP-LIVE）。
        Answer::Noul(p) => {
            // 边界按容差比较，与认证同一已决集合（步 15d-2，`stat::decided_up/down`）
            if crate::stat::decided_up(*p, hi, delta) {
                (ExitKind::Act, None)
            } else if crate::stat::decided_down(*p, lo, delta) {
                (ExitKind::Ignore, None)
            } else {
                (ExitKind::Unsure("band".into()), None)
            }
        }
        // 12:151：Pick 要求置换众数一致；没测过（J-15）不是 tie（测了、不一致）。
        Answer::Choice(v) => {
            let (k, p) = argmax(v);
            match i.mode_share {
                None => (
                    ExitKind::Unsure("untested".into()),
                    Some((
                        "permutation".into(),
                        // 依据：B64（步 15f：置换是 select 站点的测量声明）
                        "修法【作者可改】：在这道 select 题或它的题式上声明 {permute: true}（正逆两序，select 的调用数 ×2；B64）".into(),
                    )),
                ),
                Some(ms) if ms < 1.0 => (ExitKind::Unsure("tie".into()), None),
                // 依据：B63（K 元划分的单侧线带 δ 迟滞：p_max ≥ hi + δ 才出 Pick）
                Some(_) => {
                    if crate::stat::decided_up(p, hi, delta) {
                        (ExitKind::Pick(k), None)
                    } else {
                        (ExitKind::Unsure("band".into()), None)
                    }
                }
            }
        }
        Answer::Score(v) => {
            let (l, p) = argmax(v);
            // 依据：B63（同上；档位即动作不是免线的理由）
            if crate::stat::decided_up(p, hi, delta) {
                (ExitKind::At(l), None)
            } else {
                (ExitKind::Unsure("band".into()), None)
            }
        }
    }
}

#[cfg(test)]
mod line_grade_tests {
    //! 步 20a-1：`Exit::releases` 是唯一放行判定点（`附注/2026-09-24-评估①裁定.md` §十第 12(a) 条）。
    use super::*;
    use crate::value::LineGrade;

    fn 部件(untested: Option<&str>, taint: Taint) -> ExitParts {
        ExitParts {
            id: 0,
            kind: ExitKind::Act,
            untested: untested.map(str::to_string),
            op: Op::Test,
            q_hash: String::new(),
            state_hash: String::new(),
            taint,
            line_source: "题级·证书:α=0.10".into(),
            site: Span::default(),
        }
    }

    fn 出口(g: Option<LineGrade>, untested: Option<&str>) -> Exit {
        let e = issue(部件(untested, Taint::Trusted));
        e.grade.set(g);
        e
    }

    /// 等级一项：只有 `Certified`、`Form` 放行；变体名与报告 `exits` 表的字符串一致。
    #[test]
    fn grade_releases_only_certified_and_form() {
        let all = [
            (LineGrade::Cold, "Cold", false),
            (LineGrade::Fixture, "Fixture", false),
            (LineGrade::Class, "Class", false),
            (LineGrade::Trial, "Trial", false),
            (LineGrade::Provisional, "Provisional", false),
            (LineGrade::Form, "Form", true),
            (LineGrade::Certified, "Certified", true),
        ];
        for (g, name, rel) in all {
            assert_eq!(g.name(), name);
            assert_eq!(g.releases(), rel, "{name}");
            let e = 出口(Some(g), None);
            assert_eq!(e.releases(), rel, "{name}");
            assert_eq!(e.guard_trusted(), rel, "{name}");
        }
    }

    /// 正交位：任一为真即不放行，可与放行等级叠加；`untested`（J-15）自步 20a-1 起计入。
    #[test]
    fn orthogonal_bits_block_release() {
        assert!(出口(Some(LineGrade::Certified), None).releases());
        let u = 出口(Some(LineGrade::Certified), Some("permutation"));
        assert!(!u.releases() && !u.guard_trusted(), "判据未测不放行");
        let sets: [fn(&Exit); 4] = [
            |e| e.scope_out.set(true),
            |e| e.suspend_candidate.set(true),
            |e| e.delta_unknown.set(true),
            |e| e.scope_unknown.set(true),
        ];
        for set in sets {
            let x = 出口(Some(LineGrade::Form), None);
            set(&x);
            assert!(!x.releases());
        }
        // 不来自 `cut` 的出口没有等级：只看正交位
        assert!(出口(None, None).releases());
        // taint 由 guard_trusted 合取，不进 releases
        let t = issue(部件(None, Taint::Untrusted));
        t.grade.set(Some(LineGrade::Certified));
        assert!(t.releases() && !t.guard_trusted());
    }
}
