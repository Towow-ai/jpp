//! 宿主接口 `Session`（`20` §2.3 L6 `session/`，B74；`21` 步 14a）：`compile`、`explain`、`run`、
//! `replay`、`resume`、`feed_calib`。原 `jpp_core::run*` 与 CLI `run_io.rs` 里「头与补回」「出料」两段
//! 合并逻辑搬到这里，行为逐字节不变；`jpp::run*` 保留为它们的薄包装（测试与宿主不改）。
//!
//! 模块约束（`20` §2.3「禁止依赖」，`评估①裁定` §十第 12(c) 条，`deps.py` 核）：本模块不读写固定
//! 路径，不引用 `std::fs`、`std::env`。文件、目录与报文都归 `cli/`；校准记录合并策略（`--calib`
//! 与 `--fixtures` 谁优先）暂留 CLI，步 18 搬入 `store/`。
//!
//! 宿主入口（B105、B106，步 14b）：`compile(p, &EntryDecl)` 是 `Program.entry` 的唯一写入处，检查器从它
//! 知道入口名；`run`/`resume`/`replay` 收 [`EntryArgs`]（值条目、材料条目、`purpose`）。

use crate::check::{self, Report};
use crate::effects::{CalibRecord, CalibStore, FitRegistry, Sample};
use crate::interp::{ActionRegistry, EntryArgs, Interp, Outcome, RtError};
use crate::ir::EntryDecl;
use crate::ledger::Ledger;
use crate::{Error, Program};
use jpp_effects::Ports;

/// 一次会话：判断端口、校准视图、动作表，以及可选的 `fit` 表。
pub struct Session<'a> {
    /// 按效应实例索引的端口表（步 15b）
    ports: Ports<'a>,
    calib: &'a CalibStore,
    actions: &'a ActionRegistry,
    fits: Option<&'a FitRegistry>,
}

impl<'a> Session<'a> {
    /// 按端口表建会话（步 15b、15c，`20` §2.3 `Ports`）：宿主为程序用到的每个效应实例注册一个端口。
    pub fn new(
        ports: Ports<'a>,
        calib: &'a CalibStore,
        actions: &'a ActionRegistry,
    ) -> Session<'a> {
        Session {
            ports,
            calib,
            actions,
            fits: None,
        }
    }

    /// 带 `fit` 表（`12` §6.0 的 fit 桥要用它）
    pub fn with_fits(mut self, fits: &'a FitRegistry) -> Self {
        self.fits = Some(fits);
        self
    }

    /// 表层程序降到 IR（步 12d）：`jpp_syntax::lower` 配上外观层的名字表。缺预算等在这里报（`J-07a`）。
    ///
    /// 宿主入口声明（B106）：这里是 `Program.entry` 的唯一写入处；名字重复报 `E-entry-dup`，与内置名
    /// 相同报 `E-entry-name`（降级诊断，位置为整个程序）。无入口传 `&EntryDecl::default()`。
    /// 依据：B106（地基/附注/2026-09-25-B105-B106裁定.md §三）
    pub fn compile(
        p: &crate::syntax::ast::Program,
        entry: &EntryDecl,
    ) -> Result<Program, Vec<crate::syntax::Diagnostic>> {
        let mut prog = crate::syntax::lower(p, &crate::names::CurrentNames)?;
        let whole = crate::syntax::ast::Span {
            start: prog.span.start,
            end: prog.span.end,
        };
        let mut errs = vec![];
        let mut seen = std::collections::HashSet::new();
        for e in &entry.params {
            if !seen.insert(e.name.as_str()) {
                errs.push(crate::syntax::Diagnostic::new(
                    format!(
                        "E-entry-dup: 宿主入口参数 {} 给了两次（依据：B105 / B106）",
                        e.name
                    ),
                    whole,
                ));
            } else if crate::interp::BUILTINS.contains(&e.name.as_str()) {
                errs.push(crate::syntax::Diagnostic::new(
                    format!(
                        "E-entry-name: 宿主入口参数 {} 与内置名相同，程序里会遮蔽内置（依据：B105 / B106）",
                        e.name
                    ),
                    whole,
                ));
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        prog.entry = entry.clone();
        Ok(prog)
    }

    /// 执行前的静态检查（CLI `check` 与 `run` 执行前那次）：有画像带画像。入口名从 `Program.entry` 读（B106）。
    pub fn explain(program: &Program, profile: Option<&crate::effects::Profile>) -> Report {
        match profile {
            Some(p) => check::check_with_profile(program, p),
            None => check::check(program),
        }
    }

    /// [`Self::explain`] 的动作表增强版（步 24c，B108 已知限制的收口）：CLI `check`（无 `--input`
    /// 也照样有）与 `run` 的预跑诊断用它，把宿主已知的动作表交给检查器，让 J-08 静态子面对可逆
    /// 动作不报、对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`。
    pub fn explain_with_actions(
        program: &Program,
        profile: Option<&crate::effects::Profile>,
        actions: &check::ActionTable,
    ) -> Report {
        check::explain_with_actions(program, profile, actions)
    }

    /// 首跑：检查（带整本校准记录，J-10 静态面）无错再执行。`ledger` 为空账本。
    pub fn run(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut Ledger,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, false)
    }

    /// 续跑（`--resume`）：已记录的不付费、继续往下；与首跑同一路径，`ledger` 为上一趟的账本。
    pub fn resume(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut Ledger,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, false)
    }

    /// 审计重放（B35；21 步 3）：只凭账本重现首跑。账本里记过的调用照记录计入预算，缺的记录报
    /// `E-replay`（致命，不进 cause）。
    pub fn replay(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut Ledger,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, true)
    }

    /// 跳过静态检查直接执行——只给检查器本身的对照测试用。
    pub fn run_unchecked(self, program: &Program, ledger: &mut Ledger) -> Result<Outcome, RtError> {
        let budget = program.budget.clone();
        Interp::new(self.ports, ledger, self.calib, self.actions, budget).run(program)
    }

    fn go(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut Ledger,
        replay: bool,
    ) -> Result<Outcome, Error> {
        // **档案走到检查器**（`12` §1.2）。传 `check(program)` 会让降级规则永远够不着真实运行
        // ——那样管道就只在测试里通，而**一个只在测试里通的管道是构造，不是功能**。
        // **整本记录走到检查器**，不只是档案：J-10 的静态那一半要各题的 `unsure_rate`，
        // 而那住在 `CalibRecord` 里。**与 §1.2 那根「档案到不了检查器」的管道是同一种缺结构**，
        // 只是这次缺的是记录不是档案。
        // 入口名在 `Program.entry` 里（B106，`compile` 写入），检查器自己读，这里不再另传名字表
        // 动作表也走到检查器（B108，步 24-0）：J-08 静态子面在不可逆动作上报 error，必然被拦的程序
        // 在花调用之前停下
        let table = check::ActionTable {
            actions: (self.actions.actions.values())
                .map(|a| {
                    let facts = check::ActionFacts {
                        reversible: a.reversible,
                        output_untrusted: a.taint_out == crate::interp::TaintOut::Untrusted,
                    };
                    (a.name.clone(), facts)
                })
                .collect(),
            // `mat_shape`（B51-R2，步 24g，接入步 24h，主会话已批准）：`Action.mat_shape` 字段
            // 在 `jpp-runtime` 上本就公开可读，动作声明过形状的才进表；`W-diag-shape`（诊断层
            // 静态消费者，`jpp-check/src/diag/b13.rs::shape_check`）据此在检查期判断材料形状
            // 是否够回答。
            shapes: (self.actions.actions.values())
                .filter_map(|a| a.mat_shape.clone().map(|s| (a.name.clone(), s)))
                .collect(),
        };
        let report = check::check_with_calib_actions(program, self.calib, &table);
        if !report.is_ok() {
            return Err(Error::Check(report));
        }
        let budget = program.budget.clone();
        let mut it = match self.fits {
            Some(f) => Interp::with_fits(self.ports, ledger, self.calib, self.actions, f, budget),
            None => Interp::new(self.ports, ledger, self.calib, self.actions, budget),
        };
        // 宿主入口（B105）：运行入口绑定条目、`entry_hash` 进账本头；空入口与不设相同
        if !entry.is_empty() {
            it = it.with_entry(entry.clone());
        }
        if replay {
            it = it.audit_replay();
        }
        let out = it.run(program).map_err(Error::Runtime)?;
        Ok(带出静态告警(out, &report))
    }

    /// **只凭账本重放时补回当时的线**（头与补回；步 14a 自 CLI `run_io.rs` 搬来）：账本记着那一趟
    /// `cut` 实际查到的校准记录。`calib` 里已有的键（这次显式给了的）优先；没给的键才从账本补。
    /// 返回补回的键（`\u{1f}` 换成 `:`，按账本顺序），报文由宿主打印。
    pub fn restore_calib(calib: &mut CalibStore, ledger: &Ledger) -> Result<Vec<String>, String> {
        let mut 补回: Vec<String> = vec![];
        for (k, v) in &ledger.calib_used {
            if calib.records.contains_key(k) {
                continue;
            }
            let rec: CalibRecord = serde_json::from_value(v["record"].clone())
                .map_err(|e| format!("账本里的校准记录 {k:?} 读不成：{e}"))?;
            calib.records.insert(k.clone(), rec);
            补回.push(k.replace('\u{1f}', ":"));
        }
        Ok(补回)
    }

    /// **重放要用记录时那个 model_id**（头）：旧账本没有 `header` 时退回 `"fixed-0"`。
    pub fn replay_model_id(ledger: &Ledger) -> String {
        ledger
            .header
            .as_ref()
            .map(|h| h.model_id().to_string())
            .unwrap_or_else(|| "fixed-0".to_string())
    }

    /// **出料**：把这一趟的证据折进记录，标出停岗候选（`jpp_calib::feed::feed`）。落盘归宿主。
    pub fn feed_calib(
        calib: &CalibStore,
        evidence: &[(String, Sample)],
        suspend_candidates: &[&str],
    ) -> Result<(CalibStore, Vec<String>), String> {
        jpp_calib::feed::feed(calib, evidence, suspend_candidates)
    }
}

/// **J-10 的静态告警要带到调用者手里。** 它只是告警，`report.is_ok()` 仍为真，
/// 这份报告若在这里丢掉，调用者就永远看不到「跑之前就知道 unsure 预算不够」这句话。
/// 只带 J-10：别的静态告警 CLI 在执行前已经用 `check`/`check_with_profile` 打过，
/// 而只有要整本校准记录的 J-10 必须走这一处（`check_with_calib`）。
/// 放在 `trace.warnings` 最前面，表示它们先于任何调用成立。运行期出错时这份告警不随 `RtError` 带出。
fn 带出静态告警(mut out: Outcome, report: &Report) -> Outcome {
    let 静态: Vec<String> = report
        .warnings()
        .iter()
        .filter(|d| d.rule == "J-10")
        .map(|d| format!("{}: @{} {}", d.rule, d.span.start, d.message))
        .collect();
    if !静态.is_empty() {
        out.trace.warnings.splice(0..0, 静态);
    }
    out
}
