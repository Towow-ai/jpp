//! 宿主动作表（B150；`20` v2 §2.1 L6、§五 S14）：`do` 能触发的每个动作在 [`builtin_actions`]
//! 里占一行。
//!
//! 注册（[`register_all`]）、检查期事实表（[`check_table`]，J-08 静态子面与 J-11 用）、帮助文本的
//! 动作清单（[`usage_list`]）都从这张表生成——步 24c「单一来源、避免漂移」的延续。加动作（S14）：
//! 在 `actions/<group>.rs` 写一个 `run` 函数，在表里加一行；需要宿主状态的放进 [`Ctx`]。
//! CLI（bin 目标的 `cli/runner.rs`）只引用这里。
//!
//! **B164（2026-09-26）**：`exec_py`/`check_tests`/`exec_sql` 三个执行器动作的 `reversible`
//! 不再是编译期写死的常量——宿主启动时探测一次操作系统沙箱（`sandbox::kind()`，缓存），
//! `reversible` 由 `sandbox.kind != none` 派生；因此表从 `const` 改成 [`builtin_actions`]
//! （`OnceLock` 缓存，只在第一次访问时按探测结果建表，之后不变）。
//!
//! 过程记录：`地基/过程记录/工程-步C-1.md`（收成一张表）、`工程-步C-1b.md`（挪到 lib 目标）、
//! `工程-比赛R2b.md`（`graph:*` 六个精确算法动作，rebase 后接入本表）、
//! `工程-比赛R2a.md`（`exec_py`/`check_tests`/`embed_topk`/`bm25_topk`，rebase 后接入本表）、
//! `工程-比赛R2a-exec_sql.md`（`exec_sql`）、`工程-执行器动作安全修补.md`（B164：沙箱、
//! 动态 `reversible`、`E-action-no-sandbox`、`exec_sql` 的 `Fail(Denied)`）。

mod exec;
mod graph;
mod io;
mod retrieval;
mod sandbox;
mod subprocess_util;
mod values_util;

use crate::interp::{ActionRegistry, TaintOut};
use crate::value::Value;
use serde_json::Value as Json;
use std::sync::OnceLock;
use std::{cell::RefCell, rc::Rc};

/// 动作运行时能碰到的宿主状态。今天只有 `record_check` 的检查记录（进报告 `local_checks`）。
#[derive(Clone, Default)]
pub struct Ctx {
    pub checks: Rc<RefCell<Vec<Json>>>,
}

/// 画像 `actions` 分表里执行器动作的 `sandbox` 描述（B164）：`kind` 是
/// `"sandbox-exec"|"bwrap"|"none"`（宿主启动时探测得到，`none` 表示两者都没有）；`fs`/`net`/
/// `undo` 是这套隔离方案本身固定的性质，不随探测结果变——写限定在每次调用新建的临时目录
/// （`fs: "tmpdir"`）、网络一律拒绝（`net: false`）、没有额外的撤销动作，调用结束整目录丢弃
/// 就是全部状态清理（`undo: "discard-tmpdir"`）。
pub struct SandboxProfile {
    pub kind: &'static str,
    pub fs: &'static str,
    pub net: bool,
    pub undo: &'static str,
}

/// 表里的一行。
pub struct HostAction {
    /// `do` 的动作名
    pub name: &'static str,
    /// 帮助文本里的写法
    pub usage: &'static str,
    /// 可逆（J-08：不可逆动作要放行）。执行器动作（`sandbox.is_some()`）的这一位是派生值
    /// （`sandbox.kind != "none"`），不再单独声明；其余动作照常手写。
    pub reversible: bool,
    /// 输出 taint 声明（B37、`12` §2.11）
    pub taint_out: TaintOut,
    /// 每次执行的固定费用
    pub cost: f64,
    pub run: fn(&Ctx, &[Value]) -> Result<Value, String>,
    /// 只有执行器动作（`exec_py`/`check_tests`/`exec_sql`）有；见 [`SandboxProfile`]。
    pub sandbox: Option<SandboxProfile>,
}

fn executor_sandbox_profile() -> SandboxProfile {
    SandboxProfile {
        kind: sandbox::kind().as_str(),
        fs: "tmpdir",
        net: false,
        undo: "discard-tmpdir",
    }
}

fn executor_reversible() -> bool {
    sandbox::kind() != sandbox::SandboxKind::None
}

static BUILTIN_ACTIONS_CELL: OnceLock<Vec<HostAction>> = OnceLock::new();

/// 注册的全部动作（顺序即帮助文本里的顺序）。第一次调用时按宿主启动探测到的沙箱结果建表
/// （B164），此后缓存不变——`register_all`/`check_table`/`usage_list` 都从这里取，单一来源。
pub fn builtin_actions() -> &'static [HostAction] {
    BUILTIN_ACTIONS_CELL.get_or_init(|| {
        vec![
            HostAction {
                name: "record_check",
                usage: "record_check",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: io::record_check,
                sandbox: None,
            },
            HostAction {
                name: "read_json",
                usage: "read_json(path)",
                reversible: true,
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: io::read_json,
                sandbox: None,
            },
            HostAction {
                name: "write_json",
                usage: "write_json(path,value)",
                reversible: false,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: io::write_json,
                sandbox: None,
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
                sandbox: None,
            },
            HostAction {
                name: "graph:shortest_path",
                usage: "graph:shortest_path(graph)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::shortest_path,
                sandbox: None,
            },
            HostAction {
                name: "graph:max_clique",
                usage: "graph:max_clique(graph)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::max_clique,
                sandbox: None,
            },
            HostAction {
                name: "graph:components",
                usage: "graph:components(graph)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::components,
                sandbox: None,
            },
            HostAction {
                name: "graph:set_cover",
                usage: "graph:set_cover(graph)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::set_cover,
                sandbox: None,
            },
            HostAction {
                name: "graph:max_flow",
                usage: "graph:max_flow(graph)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: graph::max_flow,
                sandbox: None,
            },
            // 比赛 R2a（2026-09-25/26）：执行器（`exec_py`/`check_tests`，B164 起 `reversible`
            // 由沙箱探测派生）与检索（`embed_topk` 子进程调离线 MiniLM、`bm25_topk` 纯 Rust，
            // 不需要沙箱——不执行任意代码）（附注 §五；实现见 `工程-比赛R2a.md`）。
            HostAction {
                name: "exec_py",
                usage: "exec_py(code,stdin,timeout_s)",
                reversible: executor_reversible(),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::exec_py,
                sandbox: Some(executor_sandbox_profile()),
            },
            HostAction {
                name: "check_tests",
                usage: "check_tests(code,tests,timeout_s)",
                reversible: executor_reversible(),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::check_tests,
                sandbox: Some(executor_sandbox_profile()),
            },
            HostAction {
                name: "embed_topk",
                usage: "embed_topk(texts,query,k)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: retrieval::embed_topk,
                sandbox: None,
            },
            HostAction {
                name: "bm25_topk",
                usage: "bm25_topk(query,corpus,k)",
                reversible: true,
                taint_out: TaintOut::Inherit,
                cost: 0.0,
                run: retrieval::bm25_topk,
                sandbox: None,
            },
            // exec_sql 是 24e-1 同一块的补齐项（R2a 合入后追加，见
            // `地基/过程记录/工程-比赛R2a-exec_sql.md`）：只读连接 + `PRAGMA query_only` +
            // 授权回调 + B164 起同一套沙箱，四层。
            HostAction {
                name: "exec_sql",
                usage: "exec_sql(db,sql)",
                reversible: executor_reversible(),
                taint_out: TaintOut::Untrusted,
                cost: 0.0,
                run: exec::exec_sql,
                sandbox: Some(executor_sandbox_profile()),
            },
        ]
    })
}

/// 把表里的动作全部注册进 `registry`。只凭账本重放（`replay_only`）时动作一律不执行：
/// 账本里有记录的由运行时照记录给出，走不到这里；走到这里就是缺记录。
pub fn register_all(registry: &mut ActionRegistry, ctx: &Ctx, replay_only: bool) {
    for a in builtin_actions() {
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
/// 对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`；`no_sandbox`
/// （B164）供 `E-action-no-sandbox` 用，与调用点有没有守卫无关。
pub fn check_table() -> crate::check::ActionTable {
    let mut t = crate::check::ActionTable::default();
    for a in builtin_actions() {
        t.actions.insert(
            a.name.to_string(),
            crate::check::ActionFacts {
                reversible: a.reversible,
                output_untrusted: a.taint_out == TaintOut::Untrusted,
                no_sandbox: a
                    .sandbox
                    .as_ref()
                    .is_some_and(|s| s.kind == sandbox::SandboxKind::None.as_str()),
            },
        );
    }
    t
}

/// 本机能不能真的跑执行器动作（`exec_py`/`check_tests`/`exec_sql`）——供集成测试复用同一套
/// 探测逻辑（主会话复核追加：公开仓库 CI 上沙箱工具可能存在但跑不起来，`sandbox::probe()`
/// 已经把「跑一次冒烟测试」算进探测结果，这里只是把答案暴露成公开只读函数，不是另一套逻辑）。
/// 集成测试（`crates/jpp/tests/*.rs`）在需要真沙箱跑通的用例开头调用它，探测不到就跳过、
/// 打印原因，不判失败；测 `JPP_FORCE_NO_SANDBOX` 本身（无沙箱路径）的用例不需要它。
pub fn sandbox_available() -> bool {
    sandbox::kind() != sandbox::SandboxKind::None
}

/// 帮助文本里「Registered actions: …」的清单。
pub fn usage_list() -> String {
    builtin_actions()
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
        for a in builtin_actions() {
            assert!(seen.insert(a.name), "动作 {} 重复登记", a.name);
            assert!(a.usage.starts_with(a.name), "{} 的写法 {}", a.name, a.usage);
        }
        assert_eq!(check_table().actions.len(), builtin_actions().len());
    }

    /// B164：执行器动作的 `reversible` 与 `sandbox.kind` 一致派生；本机（macOS）预期探测到
    /// `sandbox-exec`，三个执行器动作应该是可逆的、`sandbox` 字段齐全。
    #[test]
    fn 执行器动作的reversible由sandbox_kind派生() {
        for name in ["exec_py", "check_tests", "exec_sql"] {
            let a = builtin_actions()
                .iter()
                .find(|a| a.name == name)
                .unwrap_or_else(|| panic!("{name} 应在表里"));
            let sb = a
                .sandbox
                .as_ref()
                .unwrap_or_else(|| panic!("{name} 应有 sandbox 画像"));
            assert_eq!(
                a.reversible,
                sb.kind != sandbox::SandboxKind::None.as_str(),
                "{name}: reversible 应等于 kind != \"none\""
            );
            assert_eq!(sb.fs, "tmpdir");
            assert!(!sb.net);
            assert_eq!(sb.undo, "discard-tmpdir");
        }
    }

    /// `no_sandbox` 与 `reversible` 的一致性：探测到沙箱时两者都该是「正常」（可逆、不报无沙箱）；
    /// 探测不到时两者都该翻转（不可逆、报无沙箱）——不管本机实际探测结果是哪种，这条关系恒成立。
    #[test]
    fn no_sandbox与reversible互为反面() {
        let t = check_table();
        for name in ["exec_py", "check_tests", "exec_sql"] {
            let facts = t.actions[name];
            assert_eq!(
                facts.no_sandbox, !facts.reversible,
                "{name}: no_sandbox 应与 reversible 相反"
            );
        }
    }
}
