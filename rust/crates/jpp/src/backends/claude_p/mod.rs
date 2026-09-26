//! 生成器端口 `claude -p`（B149；`21` 步 15h；`20` v2 §五 S12）：`gen` 的真机实例。
//!
//! 非阻塞：`submit` 把每个调用排进端口自己的工作线程池（线程数 = 画像 `gen.concurrency`），立即返回
//! 票据；`poll` 只查结果槽，没完成就 `Pending`。工作线程起子进程 `claude -p … --output-format json`，
//! 提示经 stdin 传入，外层 JSON 的 `result` 按「恰好 n 项的 JSON 数组」解析。失败（超时、退出码非零、
//! 非 JSON、项数不对、空）进 `GenResult.failure`，运行时产出 `Fail` 值（步 15h-1），不当运行期错误。
//! 输出 taint 由画像 `gen.taint_out` 声明，缺省 `untrusted`（B149）。
//!
//! 本模块不按 feature 门控编译（测试用假脚本代替 `claude` 在默认构建下跑）；CLI 的启用开关
//! `--gen-model` 只在 feature `live` 下接受。过程记录：`地基/过程记录/工程-步15h-1.md`。

use crate::backends::GenProfile;
use crate::effects::{
    CallInput, EffectCall, EffectError, EffectId, EffectInstance, EffectOut, EffectPort, GenResult,
    Ticket,
};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};

/// 注册表行（`backends::GENERATORS`）。
pub const SPEC: super::GenSpec = super::GenSpec {
    name: "claude-p",
    profile_file: "gen-claude-p.json",
    build,
};

fn build(model: &str, profile: &GenProfile) -> Box<dyn EffectPort> {
    Box::new(ClaudePPort::new(ClaudePConfig::from_profile(
        model, profile,
    )))
}

/// 生成时交给模型的系统说明：只要一个 JSON 数组。
const SYSTEM: &str = "You are a candidate generator called from inside a program. \
Reply with one JSON array only: no prose, no explanation, no code fences.";

/// 端口配置：可执行文件、模型、并发、超时、单次费用、声明的输出 taint。
#[derive(Clone, Debug)]
pub struct ClaudePConfig {
    /// `claude` 可执行文件（测试换成 `/bin/sh`）
    pub program: PathBuf,
    /// 放在 `claude` 参数前面的参数（测试用：`/bin/sh <假脚本>`；新建的可执行文件第一次执行时
    /// macOS 会先扫描数秒，不能直接执行临时脚本）
    pub pre_args: Vec<String>,
    pub model: String,
    pub concurrency: usize,
    pub timeout: Duration,
    pub cost_per_call: f64,
    pub taint_out: Option<jpp_value::value::Taint>,
}

impl ClaudePConfig {
    /// 按画像填：并发缺省 4（`21` 步 15h）；超时与单次费用必须由画像给（不回退代码兜底，B73 同一口径）；
    /// taint 缺省 `untrusted`（B149，画像装载时已定）。
    pub fn from_profile(model: &str, p: &GenProfile) -> ClaudePConfig {
        ClaudePConfig {
            program: PathBuf::from("claude"),
            pre_args: vec![],
            model: model.to_string(),
            concurrency: p.concurrency.unwrap_or(4).max(1),
            timeout: Duration::from_secs_f64(p.timeout_s),
            cost_per_call: p.cost_usd_per_call,
            taint_out: Some(p.taint_out),
        }
    }
}

type Slot = Arc<Mutex<Option<Result<GenResult, EffectError>>>>;

struct Job {
    prompt: String,
    ctx: Vec<Json>,
    n: usize,
    slot: Slot,
}

/// `gen@claude-p/<model>` 的端口。
pub struct ClaudePPort {
    cfg: Arc<ClaudePConfig>,
    tx: Option<Sender<Job>>,
    slots: HashMap<u64, Slot>,
    next: u64,
}

impl ClaudePPort {
    pub fn new(cfg: ClaudePConfig) -> ClaudePPort {
        ClaudePPort {
            cfg: Arc::new(cfg),
            tx: None,
            slots: HashMap::new(),
            next: 0,
        }
    }

    /// 第一次 `submit` 时起工作线程（数目 = 并发上限），之后复用。
    fn sender(&mut self) -> Sender<Job> {
        if let Some(tx) = &self.tx {
            return tx.clone();
        }
        let (tx, rx) = channel::<Job>();
        let rx: Arc<Mutex<Receiver<Job>>> = Arc::new(Mutex::new(rx));
        for _ in 0..self.cfg.concurrency {
            let (rx, cfg) = (rx.clone(), self.cfg.clone());
            std::thread::spawn(move || {
                loop {
                    let job = match rx.lock().map(|r| r.recv()) {
                        Ok(Ok(job)) => job,
                        _ => break,
                    };
                    let out = run_one(&cfg, &job.prompt, &job.ctx, job.n);
                    if let Ok(mut s) = job.slot.lock() {
                        *s = Some(Ok(out));
                    }
                }
            });
        }
        self.tx = Some(tx.clone());
        tx
    }
}

impl EffectPort for ClaudePPort {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: gen_effect(),
            model: format!("claude-p/{}", self.cfg.model),
        }
    }

    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let tx = self.sender();
        let mut tickets = Vec::with_capacity(calls.len());
        for call in calls {
            let CallInput::Prompt { prompt, ctx, n, .. } = call.input else {
                return Err(EffectError(
                    "claude-p 端口只服务 gen（提示加上下文）".into(),
                ));
            };
            let slot: Slot = Arc::new(Mutex::new(None));
            tx.send(Job {
                prompt,
                ctx,
                n,
                slot: slot.clone(),
            })
            .map_err(|_| EffectError("claude-p 工作线程已退出".into()))?;
            let t = self.next;
            self.next += 1;
            self.slots.insert(t, slot);
            tickets.push(Ticket(t));
        }
        Ok(tickets)
    }

    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        let Some(slot) = self.slots.get(&t.0) else {
            return Poll::Ready(Err(EffectError(format!("claude-p：票据 {} 不存在", t.0))));
        };
        let done = slot.lock().ok().and_then(|mut s| s.take());
        match done {
            Some(r) => {
                self.slots.remove(&t.0);
                Poll::Ready(r.map(EffectOut::Mats))
            }
            None => Poll::Pending,
        }
    }
}

/// 本端口服务的效应：注册表里出材料、不触世界、进效应行的那一个（与画像 `gen` 分表同一判据；
/// 效应名只在注册处，`20` §11.1）。
fn gen_effect() -> EffectId {
    jpp_effects::find(|s| {
        !s.produces_reading
            && s.in_effect_row
            && !s.side_effecting
            && s.output_shape == jpp_effects::OutputShape::Mats
    })
    .expect("注册表里有生成效应")
}

/// 交给模型的提示：作者的提示、上下文材料、输出格式要求。
fn render_prompt(prompt: &str, ctx: &[Json], n: usize) -> String {
    let mut s = String::from(prompt);
    if !ctx.is_empty() {
        s.push_str("\n\nContext materials (JSON):\n");
        for (i, c) in ctx.iter().enumerate() {
            s.push_str(&format!("[{i}] {c}\n"));
        }
    }
    s.push_str(&format!(
        "\nOutput exactly one JSON array with exactly {n} items and nothing else. \
         Each item is one candidate: a JSON string unless the request asks for a structured object."
    ));
    s
}

fn failure(kind: &str, detail: impl Into<String>, cfg: &ClaudePConfig) -> GenResult {
    GenResult {
        failure: Some(format!("{kind}: {}", detail.into())),
        cost: cfg.cost_per_call,
        taint_out: cfg.taint_out,
        ..Default::default()
    }
}

/// 起子进程要串行：macOS 没有 `pipe2`，管道先建后设 `CLOEXEC`，两条线程同时起子进程时，一个子进程会继承
/// 另一个的管道写端，对方的 stdin 等不到 EOF、stdout 读不到结尾（实测：假脚本本该立即返回却超时）。
/// 起进程本身很快，串行不影响并发执行。
static SPAWN: Mutex<()> = Mutex::new(());

/// 跑一次子进程并解析；所有失败都落成 `GenResult.failure`（四类：`timeout`、`failed`、`malformed`、`empty`）。
fn run_one(cfg: &ClaudePConfig, prompt: &str, ctx: &[Json], n: usize) -> GenResult {
    let guard = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    let spawned = Command::new(&cfg.program)
        .args(&cfg.pre_args)
        .args([
            "-p",
            "--model",
            &cfg.model,
            "--output-format",
            "json",
            "--no-session-persistence",
            "--tools",
            "",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--system-prompt",
            SYSTEM,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    drop(guard);
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            return failure(
                "failed",
                format!("起不了 {}：{e}", cfg.program.display()),
                cfg,
            );
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(render_prompt(prompt, ctx, n).as_bytes());
    }
    // 读 stdout/stderr 放在各自的线程里：子进程写满管道时不会卡住下面的超时等待
    let reader = |p: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut p) = p {
                let _ = p.read_to_string(&mut buf);
            }
            buf
        })
    };
    let out = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() >= cfg.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return failure(
                    "timeout",
                    format!("{:.0} 秒未返回", cfg.timeout.as_secs_f64()),
                    cfg,
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return failure("failed", e.to_string(), cfg),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let tail: String = stderr
            .chars()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return failure("failed", format!("退出码 {status}：{tail}"), cfg);
    }
    parse(&stdout, n, cfg)
}

/// 解析 `claude -p --output-format json` 的输出。
fn parse(stdout: &str, n: usize, cfg: &ClaudePConfig) -> GenResult {
    let Ok(outer) = serde_json::from_str::<Json>(stdout.trim()) else {
        return failure("malformed", "外层不是 JSON", cfg);
    };
    let tokens = ["input_tokens", "output_tokens"]
        .iter()
        .filter_map(|k| outer["usage"][k].as_u64())
        .sum();
    if outer["is_error"].as_bool() == Some(true) {
        let msg = outer["result"].as_str().unwrap_or("is_error").to_string();
        return GenResult {
            tokens,
            ..failure("failed", msg, cfg)
        };
    }
    let text = outer["result"].as_str().unwrap_or("").trim();
    let body = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .map(|t| t.trim_end().trim_end_matches("```").trim())
        .unwrap_or(text);
    let items = match serde_json::from_str::<Json>(body) {
        Ok(Json::Array(a)) => a,
        _ => {
            return GenResult {
                tokens,
                ..failure("malformed", "result 不是 JSON 数组", cfg)
            };
        }
    };
    let blank =
        |v: &Json| matches!(v, Json::Null) || v.as_str().is_some_and(|s| s.trim().is_empty());
    if items.is_empty() || items.iter().all(blank) {
        return GenResult {
            tokens,
            ..failure("empty", "没有候选", cfg)
        };
    }
    if items.len() != n {
        return GenResult {
            tokens,
            ..failure("malformed", format!("要 {n} 项，给了 {}", items.len()), cfg)
        };
    }
    GenResult {
        outputs: items,
        tokens,
        cost: cfg.cost_per_call,
        failure: None,
        taint_out: cfg.taint_out,
    }
}
