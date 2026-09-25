//! 宿主动作表（B150；`20` v2 §2.1 L6、§五 S14）：`do` 能触发的每个动作在 [`BUILTIN_ACTIONS`] 里占一行。
//!
//! 注册（[`register_all`]）、检查期事实表（[`check_table`]，J-08 静态子面与 J-11 用）、帮助文本的
//! 动作清单（[`usage_list`]）都从这张表生成——步 24c「单一来源、避免漂移」的延续。加动作（S14）：
//! 在 `actions/<group>.rs` 写一个 `run` 函数，在表里加一行；需要宿主状态的放进 [`Ctx`]。
//! CLI（bin 目标的 `cli/runner.rs`）只引用这里。
//!
//! 过程记录：`地基/过程记录/工程-步C-1.md`（收成一张表）、`工程-步C-1b.md`（挪到 lib 目标）、
//! `工程-比赛R2b.md`（`graph:*` 六个精确算法动作，rebase 后接入本表）、
//! `工程-比赛R2a.md`（`exec_py`/`check_tests`/`embed_topk`/`bm25_topk`，rebase 后接入本表）。

mod exec;
mod graph;
mod io;
mod retrieval;
mod subprocess_util;
mod values_util;

use crate::interp::{ActionRegistry, TaintOut};
use crate::value::Value;
use serde_json::Value as Json;
use std::{cell::RefCell, rc::Rc};

/// 动作运行时能碰到的宿主状态。今天只有 `record_check` 的检查记录（进报告 `local_checks`）。
#[derive(Clone, Default)]
pub struct Ctx {
    pub checks: Rc<RefCell<Vec<Json>>>,
}

/// 表里的一行。
pub struct HostAction {
    /// `do` 的动作名
    pub name: &'static str,
    /// 帮助文本里的写法
    pub usage: &'static str,
    /// 可逆（J-08：不可逆动作要放行）
    pub reversible: bool,
    /// 输出 taint 声明（B37、`12` §2.11）
    pub taint_out: TaintOut,
    /// 每次执行的固定费用
    pub cost: f64,
    pub run: fn(&Ctx, &[Value]) -> Result<Value, String>,
}

/// 注册的全部动作（顺序即帮助文本里的顺序）。
pub const BUILTIN_ACTIONS: &[HostAction] = &[
    HostAction {
        name: "record_check",
        usage: "record_check",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: io::record_check,
    },
    HostAction {
        name: "read_json",
        usage: "read_json(path)",
        reversible: true,
        taint_out: TaintOut::Untrusted,
        cost: 0.0,
        run: io::read_json,
    },
    HostAction {
        name: "write_json",
        usage: "write_json(path,value)",
        reversible: false,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: io::write_json,
    },
    // 比赛 R2b（2026-09-25/26）：六个精确图算法动作，纯函数、可逆、taint 继承、cost 0
    // （附注 §五；实现与设计决定见 `地基/过程记录/工程-比赛R2b.md`）。
    HostAction {
        name: "graph:matching",
        usage: "graph:matching(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::matching,
    },
    HostAction {
        name: "graph:shortest_path",
        usage: "graph:shortest_path(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::shortest_path,
    },
    HostAction {
        name: "graph:max_clique",
        usage: "graph:max_clique(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::max_clique,
    },
    HostAction {
        name: "graph:components",
        usage: "graph:components(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::components,
    },
    HostAction {
        name: "graph:set_cover",
        usage: "graph:set_cover(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::set_cover,
    },
    HostAction {
        name: "graph:max_flow",
        usage: "graph:max_flow(graph)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: graph::max_flow,
    },
    // 比赛 R2a（2026-09-25/26）：执行器（`exec_py`/`check_tests`，子进程 + 超时 + 静态拒绝表
    // + 运行期网络禁用补丁，都不是沙箱）与检索（`embed_topk` 子进程调离线 MiniLM、
    // `bm25_topk` 纯 Rust）四个动作（附注 §五；实现与设计决定见 `地基/过程记录/工程-比赛R2a.md`）。
    HostAction {
        name: "exec_py",
        usage: "exec_py(code,stdin,timeout_s)",
        reversible: true,
        taint_out: TaintOut::Untrusted,
        cost: 0.0,
        run: exec::exec_py,
    },
    HostAction {
        name: "check_tests",
        usage: "check_tests(code,tests,timeout_s)",
        reversible: true,
        taint_out: TaintOut::Untrusted,
        cost: 0.0,
        run: exec::check_tests,
    },
    HostAction {
        name: "embed_topk",
        usage: "embed_topk(texts,query,k)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: retrieval::embed_topk,
    },
    HostAction {
        name: "bm25_topk",
        usage: "bm25_topk(query,corpus,k)",
        reversible: true,
        taint_out: TaintOut::Inherit,
        cost: 0.0,
        run: retrieval::bm25_topk,
    },
    // exec_sql 是 24e-1 同一块的补齐项（R2a 合入后追加，见
    // `地基/过程记录/工程-比赛R2a-exec_sql.md`）：只读连接、固定 3 秒超时、子进程 Python
    // 标准库 sqlite3（不加新 cargo 依赖），网络禁用补丁同 exec_py。
    HostAction {
        name: "exec_sql",
        usage: "exec_sql(db,sql)",
        reversible: true,
        taint_out: TaintOut::Untrusted,
        cost: 0.0,
        run: exec::exec_sql,
    },
];

/// 把表里的动作全部注册进 `registry`。只凭账本重放（`replay_only`）时动作一律不执行：
/// 账本里有记录的由运行时照记录给出，走不到这里；走到这里就是缺记录。
pub fn register_all(registry: &mut ActionRegistry, ctx: &Ctx, replay_only: bool) {
    for a in BUILTIN_ACTIONS {
        let (name, run, ctx) = (a.name, a.run, ctx.clone());
        registry.register(a.name, a.cost, a.reversible, a.taint_out, move |args| {
            if replay_only {
                return Err(format!("replay has no completed record for {name}"));
            }
            run(&ctx, args)
        });
    }
}

/// 已知动作的事实表（步 24c）：不依赖用户输入或实际 `ActionRegistry`（`check` 不构造它），
/// `check` 与 `run` 的预跑诊断都能随时拿到——J-08 静态子面据此对可逆动作不报、
/// 对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`。
pub fn check_table() -> crate::check::ActionTable {
    let mut t = crate::check::ActionTable::default();
    for a in BUILTIN_ACTIONS {
        t.actions.insert(
            a.name.to_string(),
            crate::check::ActionFacts {
                reversible: a.reversible,
                output_untrusted: a.taint_out == TaintOut::Untrusted,
            },
        );
    }
    t
}

/// 帮助文本里「Registered actions: …」的清单。
pub fn usage_list() -> String {
    BUILTIN_ACTIONS
        .iter()
        .map(|a| a.usage)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 核心的 JSON 适配器把整数读成 Int（i64）；表示不了的无符号大整数在这里拒绝。
pub fn validate_numbers(value: &Json) -> Result<(), String> {
    match value {
        Json::Number(n) if n.is_u64() && n.as_i64().is_none() => {
            Err("JSON integer exceeds J++ Int range".into())
        }
        Json::Array(items) => items.iter().try_for_each(validate_numbers),
        Json::Object(items) => items.values().try_for_each(validate_numbers),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 表内名字不重复且写法以名字开头() {
        let mut seen = std::collections::BTreeSet::new();
        for a in BUILTIN_ACTIONS {
            assert!(seen.insert(a.name), "动作 {} 重复登记", a.name);
            assert!(a.usage.starts_with(a.name), "{} 的写法 {}", a.name, a.usage);
        }
        assert_eq!(check_table().actions.len(), BUILTIN_ACTIONS.len());
    }
}
