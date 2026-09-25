//! 规划（`20` §2.3「L3 · `jpp-plan`」）：哪些站点可以合并、提前登记。本 crate 隐藏的设计决定是
//! 「批调度」缝的计划期决定——推测、提升、向量化的许可怎么判——以及它在运行期必须补算的那一半。
//!
//! 步 13a（R，`21` §三·6）：`Passes` 与推测、提升、向量化的分析从运行时搬到这里。
//! - 计划期：[`plan`] 遍历 IR，写 [`Plan`]：每个 `let` 触发点的推测候选、提升步、每个函数体的向量化候选。
//! - 运行期：[`Hooks`] 实现 [`PlanHooks`]，按运行期环境（[`jpp_ir::plan::EnvView`]，运行时实现）判
//!   「这段表达式会不会产生效应」；HEAD 运行时里「拿到方法值后看函数体」的那一半原样在这里。
//!
//! 运行时只读 `Plan`、只经钩子问，自己不再判许可（`20` T3、§4.5 第 2 条）。本步里解释器仍直接依赖本 crate
//! （为保住测试里 `Interp.passes` 的写法，计划在 `Interp::run` 入口由开关算出，钩子默认取 [`Hooks`]）；
//! 这条依赖在步 14a 拆出 `jpp-runtime` 时撤掉，改由宿主算计划、经 `Ports.hooks` 注入。
//!
//! 禁止依赖：客户端、账本、运行时、`jpp-value`。

mod analysis;
pub mod passes;
mod switches;

pub use switches::Passes;

use jpp_effects::view::{K, kind};
use jpp_ir::ir::{Block, Expr, Function, Program, Stmt, find_expr};
use jpp_ir::key::NodeId;
use jpp_ir::plan::{
    EnvView, Plan, PlanHooks, Reach, Target, TargetSite, TriggerKind, TriggerPlan, ValueSummary,
};
use std::collections::BTreeSet;
use std::rc::Rc;

/// 由开关算出一次运行的计划。未开的 pass 不写对应条目，运行时见不到条目即不提前。
pub fn plan(p: &Program, passes: &Passes) -> Plan {
    let mut out = Plan::empty();
    out.fuse = passes.enabled("fuse");
    out.vectorize = passes.enabled("vectorize");
    out.lazy_cut = passes.enabled("lazy_cut");
    let cx = Cx {
        speculate: passes.enabled("speculate"),
        lift: passes.enabled("lift"),
        vectorize: out.vectorize,
    };
    visit_block(&p.body, &cx, &mut out);
    out
}

struct Cx {
    speculate: bool,
    lift: bool,
    vectorize: bool,
}

fn visit_block(b: &Block, cx: &Cx, out: &mut Plan) {
    for (i, st) in b.statements.iter().enumerate() {
        match st {
            Stmt::Let { value, .. } => {
                if cx.speculate {
                    let targets = passes::speculate::block_from(b, i);
                    if !targets.is_empty() {
                        out.triggers.insert(
                            value.id,
                            TriggerPlan {
                                kind: TriggerKind::BlockFrom(i),
                                targets,
                            },
                        );
                    }
                }
                if cx.lift
                    && let Some(lp) = passes::lift::plan(b, i)
                {
                    out.lifts.insert(value.id, lp);
                }
                // B94 下半（步 23c）：直线段提升穿过函数调用，随 `lift` 开关
                if cx.lift {
                    let seg = passes::lift::segment(b, i);
                    if !seg.is_empty() {
                        out.segments.insert(value.id, seg);
                    }
                }
                visit_expr(value, cx, out);
            }
            Stmt::Expr(e) => visit_expr(e, cx, out),
            Stmt::Function { function, .. } => visit_function(function, cx, out),
        }
    }
    if let Some(r) = &b.result {
        visit_expr(r, cx, out);
    }
}

fn visit_function(f: &Function, cx: &Cx, out: &mut Plan) {
    if cx.vectorize {
        out.bodies.insert(f.id, passes::vectorize::body(f));
    }
    visit_block(&f.body, cx, out);
}

fn visit_expr(e: &Expr, cx: &Cx, out: &mut Plan) {
    for c in e.children() {
        visit_expr(c, cx, out);
    }
    match &e.node {
        jpp_ir::ir::Node::Host(jpp_ir::ir::Host::Function(f)) => visit_function(f, cx, out),
        _ => {
            for b in e.blocks() {
                visit_block(b, cx, out);
            }
        }
    }
}

/// 两个运行期钩子的实现（`20` §2.3），经 `Interp` 注入运行时。
pub struct Hooks;

impl Hooks {
    /// 目标站点此刻能否提前求值它的状态与题（原 `speculate_expr` 里的 K-069 那一问）：
    /// 推测要**提前求值状态与题**，这两段里只要可能有效应（包括藏在用户函数里的 `do`），就不推
    /// （K-069：`state(mat(side(1)))` 曾在不该走的分支里执行了 `side` 的 `do`）。
    fn permits(judge: &Expr, env: &dyn EnvView) -> bool {
        let K::Call { args, .. } = kind(judge) else {
            return false;
        };
        !args.iter().any(|a| analysis::may_effect(a, env))
    }

    fn resolve<'b>(targets: &[TargetSite], within: &'b Block, env: &dyn EnvView) -> Vec<&'b Expr> {
        targets
            .iter()
            .filter(|t| !t.call)
            .filter_map(|t| find_expr(within, t.node))
            .filter(|e| Self::permits(e, env))
            .collect()
    }
}

impl Hooks {
    /// 函数体 `f` 一轮里的目标（步 13b）：候选按计划（静态，遍历顺序），逐个按环境核。
    /// `judge` 候选照 13a 核；调用候选满足三条才穿进去（见 `过程记录/工程-步13b.md` 预注册）：
    /// 被调者按环境是方法值且不在本路径上（防递归）；每个实参按严格口径不会产生效应；形参与实参个数相等。
    /// 被调函数体里的名字按「被调者的捕获环境 + 形参绑实参摘要」解析，与运行时真调用时的环境同构。
    ///
    /// `seg` 为真（直线段提升穿进函数体，步 23c）时，函数体里 `&&`、`||` 右侧的候选不收：右侧不一定
    /// 求值（复核修复 9）。向量化（`instantiate`）传假，口径不变。
    fn targets_of(
        plan: &Plan,
        f: &Function,
        env: &dyn EnvView,
        seen: &mut BTreeSet<NodeId>,
        seg: bool,
    ) -> Vec<Target> {
        let computed;
        let cands = match plan.bodies.get(&f.id) {
            Some(t) => t,
            None => {
                computed = passes::vectorize::body(f);
                &computed
            }
        };
        let 短路右侧 = if seg {
            analysis::short_rhs_nodes(&f.body)
        } else {
            BTreeSet::new()
        };
        let mut out = vec![];
        for t in cands {
            if 短路右侧.contains(&t.node) {
                continue;
            }
            let Some(e) = find_expr(&f.body, t.node) else {
                continue;
            };
            if !t.call {
                if Self::permits(e, env) {
                    out.push(Target::Site(t.node));
                }
                continue;
            }
            if let Some(inner) = Self::enter(plan, e, env, seen, seg) {
                out.push(Target::Enter {
                    call: t.node,
                    inner,
                });
            }
        }
        out
    }

    fn enter(
        plan: &Plan,
        call: &Expr,
        env: &dyn EnvView,
        seen: &mut BTreeSet<NodeId>,
        seg: bool,
    ) -> Option<Vec<Target>> {
        let K::Call { callee, args } = kind(call) else {
            return None;
        };
        let Some(ValueSummary::Fn(fv)) = callee.name().and_then(|n| env.lookup(n)) else {
            return None;
        };
        let callee_fn = fv.function();
        if seen.contains(&callee_fn.id) || callee_fn.parameters.len() != args.len() {
            return None;
        }
        if args.iter().any(|a| analysis::may_effect(a, env)) {
            return None;
        }
        let mut binds = vec![];
        for (p, a) in callee_fn.parameters.iter().zip(&args) {
            let s = match kind(a) {
                K::Name(n) => env.lookup(n)?,
                _ => ValueSummary::Data,
            };
            binds.push((p.name.clone(), s));
        }
        let inner_env = Overlay {
            binds,
            parent: fv.env(),
        };
        seen.insert(callee_fn.id);
        let inner = Self::targets_of(plan, callee_fn, &inner_env, seen, seg);
        seen.remove(&callee_fn.id);
        if inner.is_empty() { None } else { Some(inner) }
    }
}

/// 被调函数体里的环境：形参绑实参摘要，其余名字查被调者的捕获环境（步 13b）
struct Overlay {
    binds: Vec<(String, ValueSummary)>,
    parent: Rc<dyn EnvView>,
}

impl EnvView for Overlay {
    fn lookup(&self, name: &str) -> Option<ValueSummary> {
        match self.binds.iter().rev().find(|(n, _)| n == name) {
            Some((_, v)) => Some(v.clone()),
            None => self.parent.lookup(name),
        }
    }
}

impl PlanHooks for Hooks {
    fn instantiate(&self, plan: &Plan, body: &Function, env: &dyn EnvView) -> Vec<Target> {
        if !plan.vectorize {
            return vec![];
        }
        let mut seen = BTreeSet::new();
        seen.insert(body.id);
        Self::targets_of(plan, body, env, &mut seen, false)
    }

    fn speculate<'b>(
        &self,
        plan: &Plan,
        at: NodeId,
        block: &'b Block,
        env: &dyn EnvView,
    ) -> Vec<&'b Expr> {
        match plan.triggers.get(&at) {
            Some(t) => Self::resolve(&t.targets, block, env),
            None => vec![],
        }
    }

    fn segment(&self, plan: &Plan, at: NodeId, block: &Block, env: &dyn EnvView) -> Vec<Target> {
        let Some(cands) = plan.segments.get(&at) else {
            return vec![];
        };
        let mut out = vec![];
        for t in cands {
            let Some(e) = find_expr(block, t.node) else {
                continue;
            };
            if !t.call {
                if Self::permits(e, env) {
                    out.push(Target::Site(t.node));
                }
                continue;
            }
            let mut seen = BTreeSet::new();
            if let Some(inner) = Self::enter(plan, e, env, &mut seen, true) {
                out.push(Target::Enter {
                    call: t.node,
                    inner,
                });
            }
        }
        out
    }

    fn may_effect(&self, e: &Expr, env: &dyn EnvView, reach: Reach) -> bool {
        match reach {
            Reach::Strict => analysis::may_effect(e, env),
            Reach::World => analysis::may_touch_world(e, env),
        }
    }
}
