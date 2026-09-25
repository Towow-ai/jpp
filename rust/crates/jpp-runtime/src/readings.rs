//! 读数表（20 §2.3 `readings.rs`）：读答案的唯一入口与刷新点登记。
//!
//! 步 8b：`answer_of` 是运行时读答案的唯一入口（写答案的唯一入口是 `flush.rs::fill_answer`）；
//! 刷新点改为「判据入口 + 结构性刷新点表」：凡结果依赖答案的操作都在读答案前刷新（判据），
//! 下表把现有的每一处 `flush(原因)` 登记下来并标明是判据还是结构性（`12` §2.2 列出的
//! `if`、`handle`/`match`、程序结束、构造入口）。`flush` 在调试构建里核原因已登记，
//! 新增刷新点必须先在这里登记。
//! 依据：`12` §2.2 刷新点判据（「凡结果依赖于答案的操作，皆是刷新点」）；`20` §2.3 `readings.rs`。

use super::*;

impl<'a> Interp<'a> {
    /// 读答案的唯一入口：答案在运行时的私有读数表里（步 11b-3），读数只是句柄。
    /// 调用者必须已经刷新（判据：结果依赖答案的操作先 `flush`）。
    pub(crate) fn answer_of(&self, r: &Reading) -> Option<Answer> {
        self.answers.borrow().answer_of(r)
    }

    /// 新读数的句柄（本次运行内唯一）
    pub(crate) fn new_reading_id(&self) -> u64 {
        let id = self.next_reading.get();
        self.next_reading.set(id + 1);
        id
    }
}

/// 刷新点的种类
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshKind {
    /// 判据：这个操作的结果依赖答案（读答案前刷新）
    Criterion,
    /// 结构性：`12` §2.2 明列的控制流位置
    Structural,
}

/// 现有的全部刷新点：`flush(原因)` 的原因 → 种类与所在操作。
pub(crate) const REFRESH_POINTS: &[(&str, RefreshKind, &str)] = &[
    ("cut", RefreshKind::Criterion, "bridge.rs cut：读答案过线"),
    (
        "fit",
        RefreshKind::Criterion,
        "constructs/fit.rs：读答案拟合",
    ),
    ("order", RefreshKind::Criterion, "constructs：按读数排序"),
    (
        "allocate",
        RefreshKind::Criterion,
        "constructs/allocate.rs：按不确定性分配",
    ),
    (
        "unsure_bound",
        RefreshKind::Criterion,
        "constructs/allocate.rs：未决上界",
    ),
    (
        "repeat",
        RefreshKind::Criterion,
        "constructs/repeat.rs：合并重复读数",
    ),
    (
        "sieve",
        RefreshKind::Criterion,
        "constructs/sieve.rs：三路分流读出口",
    ),
    ("content", RefreshKind::Criterion, "eval.rs：宿主读材料内容"),
    ("if", RefreshKind::Structural, "eval.rs：条件"),
    ("end", RefreshKind::Structural, "outcome.rs：程序结束"),
];

/// 查一个刷新原因是否已登记。
pub(crate) fn refresh_point(reason: &str) -> Option<RefreshKind> {
    REFRESH_POINTS
        .iter()
        .find(|(r, _, _)| *r == reason)
        .map(|(_, k, _)| *k)
}
