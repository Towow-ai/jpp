//! 长处构件（清单 G4「发挥不确定性长处」；`12` §7「判断力花在哪」）。
//!
//! 前面几包做的都是**纪律**——不许把读数当值、不许静默丢未决、不许超预算。这一包做的是**长处**：
//! 读数是带校准线的随机变量，所以它有一个裸调用拿不到的量——**离决定带多远**。有了这个量，
//! 复核名额就能花在最不确定的那几条上，而不是随机抽或全抽。
//!
//! 这两个构件是**有对照的移植**，不是重新设计。Python 内核里它们已经实现并有测试：
//! `foundation/jv/runtime.py:1236` 的 `allocate`、`:1246` 的 `unsure_bound`。
//! `crates/jpp-core/tests/oracle/generate.py` 把 Python 侧跑出来存成基准，
//! `tests/allocate.rs` 断言这里给出同一批下标与同一组界。
//!
//! **合法性**（Python `allocate` 的 docstring 原话）：「它像 cut 一样只读校准线，不做跨题算术；
//! 返回宿主整数，不返回读数。」所以这两个都不发调用、不进账本、不动预算。

use std::rc::Rc;

use jpp_effects::views::CalibView;
use jpp_value::value::{Answer, Answers, Reading};

/// Python 的 `round(x, 4)` 是**四舍六入五成双**（banker's rounding），Rust 的 `f64::round`
/// 是五入远离零。对照移植要的是同一个数，所以照 Python 的规则来。
pub fn round4(x: f64) -> f64 {
    let scaled = x * 10_000.0;
    let lower = scaled.floor();
    let frac = scaled - lower;
    let r = if (frac - 0.5).abs() < 1e-9 {
        // 恰好在半整数上：取偶数那边
        if (lower as i64) % 2 == 0 {
            lower
        } else {
            lower + 1.0
        }
    } else {
        scaled.round()
    };
    r / 10_000.0
}

/// 一条读数拿来判断的那个概率：noul 是 p 本身，choice / score 是被选中那一档的概率。
fn probability(answers: &dyn Answers, r: &Reading) -> f64 {
    if r.fail.is_some() {
        return 0.0;
    }
    match answers.answer_of(r) {
        Some(Answer::Noul(p)) => p,
        Some(Answer::Choice(v)) | Some(Answer::Score(v)) => {
            v.iter().fold(f64::MIN, |a, b| if *b > a { *b } else { a })
        }
        None => 0.0,
    }
}

/// 与决定带的距离取负：**带内 = 0（最不确定）**，带外越远越确定。
///
/// **`None` = 算不出来**：这条读数的键没有上岗记录，**它没有属于自己的线**。
///
/// **为什么不返回 `0.0`。** 以前这里对冷键返回 `0.0`，理由是「取档案保守线」——
/// 而缺省保守线是 `hi=1 / lo=0`，**于是每一个 p 都落在带内、每一条读数都得 `0.0`**，
/// `allocate` 降序并列按下标 → **返回「前 k 个」，零告警**。
///
/// **`0.0` 在那里同时承担了两件事：「带内，很不确定」和「算不出来」。**
/// **而「算不出来」不是一个值，更不能是一个恰好排在最前面的值。**
///
/// **Python 侧同式**（`runtime.py:1243` 的 `_uncertainty`）——这不是移植分叉，
/// **是两边同一个洞**，Rust 这边先补。
pub fn uncertainty(calib: &dyn CalibView, answers: &dyn Answers, r: &Reading) -> Option<f64> {
    let rec = calib.line(&r.calib);
    // **拿不到任何来自测量的线** = 这条记录不上岗，**而且档案也没加载**。
    //
    // 退化的根不是「冷键」，是**没有档案**：差分夹具里 `safety_lines = [0.75, 0.25]`
    // 是一条**测出来的**保守线，冷键在它上面照样分得出 0.52 与 0.93（Python 给 `[1,0]`，
    // 不是下标序）。**而 `Profile::default()` 的兜底是 `hi=1 / lo=0`——带是整个 [0,1]，
    // 每一个 p 都在带内、每一条都得 `0.0`，降序并列按下标 → 「前 k 个」，零告警。**
    //
    // 判据还是那条：**「这个默认值在替谁说话」**。一条测出来的保守线是粗，但是真的；
    // 一条代码兜底线什么也没说，**而它说出来的那个 `0.0` 恰好排在最前面**。
    // `hash.is_none()` 是「本次没有加载档案」的既有判据（与账本头那条同源）。
    if rec.status != "上岗" && calib.profile().hash.is_none() {
        return None;
    }
    // δ 只从记录取（步 15d-2）；没有 δ 算不出强度
    let delta = calib.line_delta(&rec)?;
    // 判断用的线：上岗记录用自己的，其余一律用画像的保守线（与 `CalibStore::lines_for` 同口径）；
    // 画像没测保守线时算不出（步 15d-2）
    let (hi, lo) = if rec.status == "上岗" {
        (rec.hi, rec.lo)
    } else {
        *calib.profile().safety.get()?
    };
    let p = probability(answers, r);
    let (top, bot) = (hi + delta, lo - delta);
    Some(if bot <= p && p <= top {
        0.0
    } else {
        -(p - top).abs().min((p - bot).abs())
    })
}

/// `allocate` 的完整账：排得了的那些，**和排不了的那些**。
///
/// **排不了的不是「最不确定」也不是「最确定」——它没有位置。**
/// 把它们混进同一张榜，就是拿「离全局兜底多远」和「离这道题的线多远」比大小，
/// **而那是两把尺子**。
#[derive(Clone, Debug, PartialEq)]
pub struct Allocation {
    /// 按不确定度降序、并列按下标升序，取前 k 个
    pub picked: Vec<usize>,
    /// **算不出不确定度的那些读数的下标**（键没上岗）。它们不进榜，但必须点名。
    pub 算不出: Vec<usize>,
}

/// 把 `k` 份复核（人、第二传感器、更贵的执行）分给最不确定的读数：返回下标，按不确定度降序，
/// **并列按下标升序**。`k` 通常取 `budget.escalate`。`k` 大于读数条数时给全部。
pub fn allocate(
    calib: &dyn CalibView,
    answers: &dyn Answers,
    readings: &[Rc<Reading>],
    k: usize,
) -> Vec<usize> {
    allocate_report(calib, answers, readings, k).picked
}

/// 同上，外加「哪些排不了序」。**`allocate` 是它只取 `picked` 的特例。**
pub fn allocate_report(
    calib: &dyn CalibView,
    answers: &dyn Answers,
    readings: &[Rc<Reading>],
    k: usize,
) -> Allocation {
    let mut scored: Vec<(usize, f64)> = vec![];
    let mut 算不出: Vec<usize> = vec![];
    for (i, r) in readings.iter().enumerate() {
        match uncertainty(calib, answers, r) {
            Some(u) => scored.push((i, u)),
            None => 算不出.push(i),
        }
    }
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    Allocation {
        picked: scored[..k.min(scored.len())]
            .iter()
            .map(|(i, _)| *i)
            .collect(),
        算不出,
    }
}

/// 「只作参考值，不作判据」以前只写在注释里——而注释拦不住任何人。
/// 包一层：参考值**拿不出裸 f64**，要取得先调 `as_reference_only()`，那个名字在调用点就是一句话。
/// 它不实现 `PartialOrd`，所以 `est > budget.unsure` 这种比较**编译不过**。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct 仅供参考(f64);

impl 仅供参考 {
    /// 取出来看一眼可以；名字会在调用点提醒你它不能当判据用
    pub fn as_reference_only(self) -> f64 {
        self.0
    }
}

/// J-10 的三个数。**判据只有 `union_bound` 一个**（`12` §5 J-10：「无标注集时用联合界 Σuᵢ
/// 作上界；独立假设估计 1−(1−u)^k **只作参考值**」，理由是红队 06 B4——独立假设在题相关时
/// 方向不定）。所以这里让类型替纪律说话：`independent_any` 不是 `f64`，拿它做比较编译不过。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnsureBound {
    pub n: usize,
    /// 联合界 Σuᵢ（被 n 夹住）——**这才是判据**，超 `budget.unsure` 即报
    pub union_bound: f64,
    /// 独立假设估计 1−Π(1−uᵢ)——**只作参考值**，类型上就取不出来做比较
    pub independent_any: 仅供参考,
    /// 没有可用 unsure_rate 的题数（界里按 1 计，最保守）
    pub n_unknown: usize,
}

/// J-10：整批读数落入 unsure 的期望数。
///
/// 依据 `12` §5 J-10 原文：「unsure 预算：有标注集时用经验联合 unsure 率；无标注集时用联合界 Σuᵢ
/// 作上界；独立假设估计 1−(1−u)^k 只作参考值。超 `budget.unsure` 即报」。
/// 独立估计只作参考的理由是红队 06 B4：独立假设在题相关时方向不定。
///
/// `uᵢ` 取各题校准记录的 `unsure_rate`；记录不是「上岗」或没有这个值的，计进 `n_unknown`
/// 并在界里按 1 计。
pub fn unsure_bound(calib: &dyn CalibView, readings: &[Rc<Reading>]) -> UnsureBound {
    let mut us = vec![];
    let mut unknown = 0usize;
    for r in readings {
        // 与 J-10 静态那一半共用 `usable_unsure_rate`（经视图）：上岗、且认证时的 δ 与现在一致
        match calib.unsure_rate(&r.calib) {
            Some(u) => us.push(u),
            None => {
                unknown += 1;
                us.push(1.0);
            }
        }
    }
    let n = us.len();
    UnsureBound {
        n,
        union_bound: round4(us.iter().sum::<f64>().min(n as f64)),
        independent_any: 仅供参考(round4(1.0 - us.iter().fold(1.0, |acc, u| acc * (1.0 - u)))),
        n_unknown: unknown,
    }
}
