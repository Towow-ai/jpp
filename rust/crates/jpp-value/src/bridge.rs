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
        host_accepts_declared: Cell::new(false),
        alpha: Cell::new(None),
        bound: Cell::new(None),
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
                    // 依据：B130（冷出口列四条出路；地基/附注/2026-09-25-作者主权与策略表达裁定.md §三）
                    "修法【作者可改】：这道题没有认证过的线（模式级记录不供线，B44）。四条出路，按成本从低到高：(1) 题库——用 bank/bank.json 里已认证的同题型题式，fill(题式, {…})，作者一条不标；(2) 真值可算——标签由程序算出（source: computed），经 calib-import 导入，零人工；(3) 标注或代标——calib-import 从带真值样本认证一条线（可由强模型代标加复核，B36、B89）；(4) 作者声明线——cut(r, {declare: {hi: …, lo: …}}) 按你写的数切，不作错误率保证，放行不可逆动作须 --release-on-declared（B128）".into(),
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

/// 作者声明线（B128；`cuts` 为 B153 的分桶，`closed` 为 B165 的端位；步 20j-1、20j-3）。
/// `cuts` 非空时是分桶线（出 `At(ℓ)`），`hi`/`lo` 不用；否则是 `{hi, lo}` 线，只给 `hi` 时 `lo = hi`。
#[derive(Clone, Debug, PartialEq)]
pub struct DeclaredLine {
    pub hi: f64,
    pub lo: f64,
    pub cuts: Vec<f64>,
    /// 作者写了 `lo`（单线时为假）
    pub lo_given: bool,
    pub closed_hi: bool,
    pub closed_lo: bool,
    pub closed_cuts: bool,
}

/// 缺省两端（与切点）全闭（B165 (2)：与认证线同约定）。手写而不派生：派生的 `bool` 缺省为假，会把端位静默变开。
impl Default for DeclaredLine {
    fn default() -> DeclaredLine {
        DeclaredLine {
            hi: Default::default(),
            lo: Default::default(),
            cuts: vec![],
            lo_given: false,
            closed_hi: true,
            closed_lo: true,
            closed_cuts: true,
        }
    }
}

impl DeclaredLine {
    /// `{hi, lo?}` 线，两端闭（B128 原样）
    pub fn two_sided(hi: f64, lo: f64, lo_given: bool) -> DeclaredLine {
        DeclaredLine {
            hi,
            lo,
            lo_given,
            ..Default::default()
        }
    }
    /// `{cuts: [c₁ < c₂ < …]}` 分桶线（B153），切点闭（B165 缺省）；`hi`/`lo` 不用
    pub fn with_cuts(cuts: Vec<f64>) -> DeclaredLine {
        DeclaredLine {
            cuts,
            ..Default::default()
        }
    }
    pub fn is_cuts(&self) -> bool {
        !self.cuts.is_empty()
    }
    /// 端位不是缺省全闭时的 `closed` 记录（B165 (3)）；全闭为 `None`，不写进记录与报告
    pub fn closed_json(&self) -> Option<serde_json::Value> {
        if self.is_cuts() {
            (!self.closed_cuts).then(|| serde_json::json!({"cuts": false}))
        } else if self.closed_hi && self.closed_lo {
            None
        } else {
            Some(serde_json::json!({"hi": self.closed_hi, "lo": self.closed_lo}))
        }
    }
    /// 线的数：`{hi, lo}` 或 `{cuts}`（B142 记录与报告 `declared` 共用）
    pub fn numbers_json(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        if self.is_cuts() {
            m.insert("cuts".into(), serde_json::json!(self.cuts));
        } else {
            m.insert("hi".into(), serde_json::json!(self.hi));
            m.insert("lo".into(), serde_json::json!(self.lo));
        }
        m
    }
    /// 告警与线源里的一段文字：`hi=0.7 lo=0.3`、`cuts=[0.5, 1.5]`，非缺省端位附在后面
    pub fn describe(&self) -> String {
        let 数 = if self.is_cuts() {
            format!("cuts={:?}", self.cuts)
        } else {
            format!("hi={} lo={}", self.hi, self.lo)
        };
        match self.closed_json() {
            Some(c) => format!("{数} closed={c}"),
            None => 数,
        }
    }
}

/// 统计量 `s` 过声明线（B128、B153、B165）：`{hi, lo}` 线出 test 型出口（act 当且仅当 s 在上侧已决区，ignore
/// 当且仅当在下侧已决区，其间 `Unsure(band)`）；`cuts` 线出 `At(ℓ)`，ℓ = 越过的切点数。闭端按现行容差
/// （`stat::decided_up/down`，δ = 0，与认证同一已决集合），开端按「严格越过线 ± ε」（`stat::beyond_up/down`）。
/// 依据：B153 (1)、B165 (2)(4)（地基/附注/2026-09-26-批6裁定.md §一、§十三）
pub fn past_declared(s: f64, l: &DeclaredLine) -> ExitKind {
    use crate::stat::{beyond_down, beyond_up, decided_down, decided_up};
    if l.is_cuts() {
        let 档 = l
            .cuts
            .iter()
            .filter(|c| {
                if l.closed_cuts {
                    decided_up(s, **c, crate::stat::DECLARED_DELTA)
                } else {
                    beyond_up(s, **c)
                }
            })
            .count();
        return ExitKind::At(档);
    }
    let 上 = if l.closed_hi {
        decided_up(s, l.hi, crate::stat::DECLARED_DELTA)
    } else {
        beyond_up(s, l.hi)
    };
    let 下 = if l.closed_lo {
        decided_down(s, l.lo, crate::stat::DECLARED_DELTA)
    } else {
        beyond_down(s, l.lo)
    };
    if 上 {
        ExitKind::Act
    } else if 下 {
        ExitKind::Ignore
    } else {
        ExitKind::Unsure("band".into())
    }
}

/// `stat` 不是 `max` 而没有 `declare`：冷（B153 (1)「认证在 p_max 上的线对别的统计量无效，不借」）。
pub fn cold_for_stat(stat: &crate::stat::Stat) -> (ExitKind, Untested) {
    (
        ExitKind::Unsure("cold".into()),
        Some((
            "calib_line".into(),
            format!(
                "修法【作者可改】：认证线在 p_max 上，对 stat: {} 无效（同键记录不借）；要按这个统计量切，写作者声明线 cut(r, {{stat: {}, declare: {{hi: …, lo: …}}}})，按你写的数切，不作错误率保证，放行不可逆动作须 --release-on-declared（B153、B128）",
                stat.name(),
                stat.to_json()
            ),
        )),
    )
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

    /// 步 20j-2（B128）：`Declared` 未经宿主接受不放行，接受后放行；taint 仍由 `guard_trusted` 合取
    #[test]
    fn declared_releases_only_when_host_accepts() {
        let d = 出口(Some(LineGrade::Declared), None);
        assert!(!d.releases() && !d.guard_trusted());
        d.host_accepts_declared.set(true);
        assert!(d.releases() && d.guard_trusted());
        // 接受位对其他等级无作用
        let t = 出口(Some(LineGrade::Trial), None);
        t.host_accepts_declared.set(true);
        assert!(!t.releases());
        // 正交位照样否决
        let u = 出口(Some(LineGrade::Declared), Some("permutation"));
        u.host_accepts_declared.set(true);
        assert!(!u.releases());
        let x = issue(部件(None, Taint::Untrusted));
        x.grade.set(Some(LineGrade::Declared));
        x.host_accepts_declared.set(true);
        assert!(x.releases() && !x.guard_trusted());
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
