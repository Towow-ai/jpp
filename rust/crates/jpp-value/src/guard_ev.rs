//! 守卫证据 `GuardEv`（J-08 的正向证据，`20` v2 §3.1；`21` 步 16）。
//!
//! 随 `Value::Bool` 走，与来源标签 `Provenance` 正交：taint 说「内容来源可不可信」，
//! `GuardEv` 说「这个布尔是不是由一次可放行的判断（可信状态、线放行）或一次 `ask` 决定的」。
//! 字面量 `true` 的 taint 是 Trusted 却没有判断证据；可信判断 ∧ 不可信判断按 taint ∨ 是
//! Untrusted 却满足 J-08；夹具线切出的出口 taint 可信但等级不放行——所以两者不能合一。
//!
//! 产生与合并（B121-1）：只在 `handle` 分派处由 [`GuardEv::from_exit`] 产生；`&&` 取并，`||`、`!`、
//! 比较与其他运算为空；`if` 不把条件证据传给分支值；取字段、下标、函数返回、`let` 原样携带。
//!
//! 不参与值的相等、哈希与序列化：`Value::equals` 只比内容，`to_json` 不写它，
//! 报告、账本与 `entry_hash` 的字节因此不随它变。

use crate::value::Exit;

/// J-08 的两个析取项各占一位（`12` §5 J-08：至少一个合取项来自可信状态上的判断，或经 `ask`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuardEv {
    /// 有一个合取项来自可信状态上、线放行的判断（`Exit::guard_trusted`）。
    pub from_releasing_judgement: bool,
    /// 有一个合取项经 `ask`（人答，`12` §2.11）。
    pub via_ask: bool,
}

impl GuardEv {
    /// 空证据：字面量、运算结果、`||` 与 `!` 的输出。
    pub const EMPTY: GuardEv = GuardEv {
        from_releasing_judgement: false,
        via_ask: false,
    };

    /// 出口被 `handle` 分派时给出的证据（B121-1：本语言消去出口的唯一形式是 `handle`，
    /// 臂的守卫栈与臂返回值里的 `Bool` 叶子都取这一个值）。
    ///
    /// 未决出口两位恒空：unsure 臂拿到的是责任，不是判定（B121-2）。其余出口：
    /// `from_releasing_judgement` = [`Exit::guard_trusted`]（已决 ∧ taint 可信 ∧ `releases()`）
    /// ∧ `lineage_ok`（谱系放行，B72-4，步 17b：被判断材料经来源可达的每个祖先出口都已决且放行，
    /// 由运行时查本趟出口表与账本 `parents` 算出后传进来）；`via_ask` = 出口的 `from_ask`，
    /// 不从 taint 推，也不受谱系影响（B72-4 只收紧判断证据）。
    /// 依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md §一、§二）、B72-4
    pub fn from_exit(e: &Exit, lineage_ok: bool) -> GuardEv {
        if e.is_unsure() {
            return GuardEv::EMPTY;
        }
        GuardEv {
            from_releasing_judgement: e.guard_trusted() && lineage_ok,
            via_ask: e.from_ask.get(),
        }
    }

    /// 并：`a && b` 的两侧都是这个条件成立所保证的；嵌套 `handle` 的臂返回值取内外两层出口的并。
    pub fn join(self, other: GuardEv) -> GuardEv {
        GuardEv {
            from_releasing_judgement: self.from_releasing_judgement
                || other.from_releasing_judgement,
            via_ask: self.via_ask || other.via_ask,
        }
    }

    /// 这份证据能否放行不可逆 `do`。
    pub fn releases(self) -> bool {
        self.from_releasing_judgement || self.via_ask
    }

    pub fn is_empty(self) -> bool {
        !self.releases()
    }
}
