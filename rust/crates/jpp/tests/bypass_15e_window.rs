//! 步 15e：刷新按窗口发出一层（`21` 步 15e；`20` §2.3 `schedule.rs` 行；主会话 2026-09-25 裁定）。
//!
//! 窗口 W 取画像 `concurrency`（未测 1，即原串行路径）。钉住五条性质（`过程记录/工程-步15e.md` (P2)）：
//! (i) 无失败时账本与报告与串行逐字节相同；(ii) 第一窗串行；(iii) cost 预算按已观察的单次最大费用限窗，
//! 超额与串行相同；(iv) calls 预算把已排队的调用算进去、硬截断，停的组与串行相同；(v) 失败的重试在批后串行发，
//! 账本 `call` 编号与串行相同。

mod common;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::task::Poll;

use jpp::effects::{
    CalibStore, CallInput, EffectCall, EffectError, EffectInstance, EffectOut, EffectPort,
    JudgeResult, Ports, Profile, Ticket,
};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 记录每次 `submit` 批大小的判断端口：每次调用费用 `费`；全局第 `坏` 次调用（从 1 数）失败一次。
struct 记批 {
    批: Rc<RefCell<Vec<usize>>>,
    费: f64,
    坏: Option<u64>,
    n: u64,
    next: u64,
    done: BTreeMap<u64, Result<EffectOut, EffectError>>,
}

impl EffectPort for 记批 {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: jpp_effects::find(|s| s.produces_reading).unwrap(),
            model: "m".into(),
        }
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        self.批.borrow_mut().push(calls.len());
        Ok(calls
            .into_iter()
            .map(|c| {
                self.n += 1;
                let r = match c.input {
                    CallInput::StateQuestions { questions, .. } if self.坏 != Some(self.n) => {
                        Ok(EffectOut::Readings(JudgeResult {
                            answers: questions.iter().map(|_| Answer::Noul(0.9)).collect(),
                            tokens: 10,
                            cost: self.费,
                            mode_share: vec![None; questions.len()],
                            perms: vec![0; questions.len()],
                        }))
                    }
                    CallInput::StateQuestions { .. } => Err(EffectError("连接中断".into())),
                    _ => Err(EffectError("只收判断".into())),
                };
                let t = self.next;
                self.next += 1;
                self.done.insert(t, r);
                Ticket(t)
            })
            .collect())
    }
    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        Poll::Ready(
            self.done
                .remove(&t.0)
                .unwrap_or_else(|| Err(EffectError("票据不存在".into()))),
        )
    }
}

/// 一层 `n` 组（`n` 段不同的材料各登记一道题，第一次 `cut` 时一次刷新）
fn 源(budget: &str, n: usize) -> String {
    let 材料: Vec<String> = (0..n).map(|i| format!("\"材料{i}\"")).collect();
    format!(
        r#"
budget {budget};
fn 判(r) {{
    let e = cut(r);
    {{k: exit_kind(e), e: e}}
}}
let rs = map([{}], fn(t) {{ judge(state(mat(t)), test("行吗", "k")) }});
// 未决（含预算停发的 Unsure(budget)）随返回值转交（B95，步 21；B93，步 22-0）
let ks = map(rs, 判);
{{v: map(ks, fn(x) {{ x.k }}), pending: map(ks, fn(x) {{ x.e }})}}
"#,
        材料.join(", ")
    )
}

struct 结果 {
    批: Vec<usize>,
    账本: String,
    值: String,
    usd: f64,
    calls: u64,
}

fn 跑(src: &str, w: u32, 费: f64, 坏: Option<u64>) -> 结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    calib.profile = Profile::untested().with_concurrency(w, "测试");
    let 批 = Rc::new(RefCell::new(vec![]));
    let port = 记批 {
        批: 批.clone(),
        费,
        坏,
        n: 0,
        next: 0,
        done: BTreeMap::new(),
    };
    let mut l = Ledger::new();
    let o = run(
        &program,
        Ports::new().with(port),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    );
    let (值, usd, calls) = match o {
        Ok(o) => (
            format!("{} {:?}", o.value_json(), o.pending),
            o.cost.usd,
            o.cost.calls,
        ),
        Err(e) => (e.render(), f64::NAN, 0),
    };
    let 批 = 批.borrow().clone();
    结果 {
        批,
        账本: l.encode(),
        值,
        usd,
        calls,
    }
}

const 宽: &str = "{calls: 30, cost: 0.01, depth: 16}";

/// (ii) 第一窗串行：还没观察到单次费用时 W_eff = 1；之后 min(W, floor(余额 / c_max))
#[test]
fn 第一窗串行() {
    let r = 跑(&源(宽, 9), 4, 0.001, None);
    assert_eq!(r.批, vec![1, 4, 4]);
    let s = 跑(&源(宽, 9), 1, 0.001, None);
    assert_eq!(s.批, vec![1; 9]);
    assert_eq!(r.账本, s.账本, "无失败时账本与串行逐字节相同");
    assert_eq!(r.值, s.值);
}

/// (iii) cost 限窗：floor((0.0045 − 0.001) / 0.001) = 3；再往后 floor(0.0005 / 0.001) = 0 → 1，
/// 与串行一样多发一次、超 0.0005 后停
#[test]
fn cost预算按已观察的单次最大费用限窗() {
    let src = 源("{calls: 30, cost: 0.0045, depth: 16}", 9);
    let r = 跑(&src, 4, 0.001, None);
    assert_eq!(r.批, vec![1, 3, 1]);
    let s = 跑(&src, 1, 0.001, None);
    assert_eq!(s.批, vec![1; 5]);
    assert_eq!(r.账本, s.账本);
    assert_eq!(r.值, s.值);
    assert_eq!(r.calls, 5);
    let 超额 = r.usd - 0.0045;
    assert!((超额 - 0.0005).abs() < 1e-12, "实际超额 {超额}");
}

/// (iv) calls 硬截断：已排队的调用算进去，停的组与串行相同
#[test]
fn calls预算硬截断() {
    let src = 源("{calls: 5, cost: 0, depth: 16}", 9);
    let r = 跑(&src, 8, 0.0, None);
    assert_eq!(r.批, vec![1, 4]);
    let s = 跑(&src, 1, 0.0, None);
    assert_eq!(s.批, vec![1; 5]);
    assert_eq!(r.账本, s.账本);
    assert_eq!(r.值, s.值);
    assert_eq!(r.calls, 5);
}

/// (v) 失败后重试在批后串行发；账本 `call` 编号与串行相同
#[test]
fn 失败的重试在批后串行发() {
    let src = 源(
        r#"{calls: 30, cost: 0, depth: 16, absent: {retry: 1, backoff: 0, then: "conservative", breaker: 5}}"#,
        4,
    );
    let r = 跑(&src, 4, 0.0, Some(2));
    assert_eq!(r.批, vec![1, 3, 1]);
    let s = 跑(&src, 1, 0.0, Some(2));
    assert_eq!(s.批, vec![1; 5]);
    assert_eq!(r.账本, s.账本);
    assert_eq!(r.值, s.值);
}

// ---------- (i) 全部金样：只差并发的两份画像，账本与报告相同 ----------

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn jpp(cwd: &Path, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

/// 账本逐行：去掉头行的画像哈希（两份画像文件本来就不同）与各行的 `prev` 链（随头行变）
fn 账本去哈希(text: &str) -> Vec<Json> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut v: Json = serde_json::from_str(l).unwrap();
            去键(&mut v, &["prev", "profile_hash", "behavior_hash"]);
            v
        })
        .collect()
}

fn 去键(v: &mut Json, ks: &[&str]) {
    match v {
        Json::Object(o) => {
            for k in ks {
                o.remove(*k);
            }
            o.values_mut().for_each(|x| 去键(x, ks));
        }
        Json::Array(a) => a.iter_mut().for_each(|x| 去键(x, ks)),
        _ => {}
    }
}

/// (i) 无失败时账本除时间外与串行一致：全部金样示例在固定观察下，用只差 `concurrency`（未测与 8）的
/// 两份画像各跑一次，账本（去掉画像哈希与 `prev` 链）与报告逐项相同。
#[test]
fn 全部金样并发与串行一致() {
    let m: Json =
        serde_json::from_slice(&std::fs::read(root().join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    let base = std::env::temp_dir().join(format!("jpp-15e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mut 报告: HashMap<&str, BTreeMap<String, (Json, Vec<Json>)>> = HashMap::new();
    let mut 比过 = 0;
    for (名, 画像) in [
        ("c1", "{}"),
        ("c8", r#"{"concurrency": {"lower_bound_ok": 8}}"#),
    ] {
        let d = base.join(名);
        std::fs::create_dir_all(&d).unwrap();
        let pf = d.join("profile.json");
        std::fs::write(&pf, 画像).unwrap();
        for c in m["cases"].as_array().unwrap() {
            if c["expect"] == "error" {
                continue;
            }
            let name = c["name"].as_str().unwrap();
            let tmp = d.join(name);
            std::fs::create_dir_all(&tmp).unwrap();
            if let Some(o) = c["files"].as_object() {
                for (k, v) in o {
                    std::fs::write(tmp.join(k), v.as_str().unwrap()).unwrap();
                }
            }
            let mut a: Vec<String> = vec![
                "run".into(),
                root()
                    .join(c["source"].as_str().unwrap())
                    .display()
                    .to_string(),
            ];
            for (flag, key) in [("--fixtures", "fixtures"), ("--calib", "calib")] {
                if let Some(f) = c[key].as_str() {
                    a.push(flag.into());
                    a.push(root().join(f).display().to_string());
                }
            }
            if let Some(from) = c["resume_from"].as_str() {
                a.push("--resume".into());
                a.push(d.join(from).join("ledger.json").display().to_string());
            }
            a.extend([
                "--profile".into(),
                pf.display().to_string(),
                "--ledger-out".into(),
                "ledger.json".into(),
                "--output".into(),
                "report.json".into(),
            ]);
            let out = jpp(&tmp, &a);
            assert!(
                out.status.success(),
                "{name}（{名}）：{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let rep: Json =
                serde_json::from_slice(&std::fs::read(tmp.join("report.json")).unwrap()).unwrap();
            let led = 账本去哈希(&std::fs::read_to_string(tmp.join("ledger.json")).unwrap());
            报告
                .entry(名)
                .or_default()
                .insert(name.to_string(), (rep, led));
        }
    }
    let (串, 并) = (&报告["c1"], &报告["c8"]);
    assert_eq!(串.len(), 并.len());
    for (name, (r1, l1)) in 串 {
        let (r8, l8) = &并[name];
        assert_eq!(l1, l8, "{name}：账本不同");
        assert_eq!(r1, r8, "{name}：报告不同");
        比过 += 1;
    }
    assert!(比过 >= 25, "比过的示例太少：{比过}");
    let _ = std::fs::remove_dir_all(&base);
}
