//! 绕过测试 B72（步 20f）：试用线出口可路由、报 `W-trial-line`（带 α、n 与「等级随材料传递」，步 17b 起），
//! 不放行不可逆 `do`（J-08 拒）；报告逐出口等级 `Trial`、`releases: false`；只凭账本重放 0 调用、
//! 逐出口等级与首跑逐字节相同（出口不进账本，等级随证书进账本的 `CalibUsed` 条目；v2 在头行 `calib_used`）。
//! 试用线经真值通道真实导入（80 条两极构造真值：正式 α 每格要 22 条不够，试用 α 每格 9 条够）。
//! 依据：B72（`地基/附注/2026-09-24-评估①裁定.md` §一；`12` §2.3 B72 条；`20` §3.4 等级表）；`21` 步 20f。
//! 21 写的路径是 `tests/bypass/b72_trial.rs`，本仓库约定为 `crates/jpp-core/tests/bypass_*.rs`。

use std::cell::Cell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, run, run_replay, syntax::parse};
use serde_json::json;

/// 步 15c：原 `impl Client for 桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
/// 改为一个 judge 闭包端口，调用计数借用处的 `Cell`。
fn 桩端口(calls: &Cell<u64>) -> Ports<'_> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        calls.set(calls.get() + 1);
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.97)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

pub fn 选项(alpha_trial: Option<f64>) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b72".into(),
        seed: 20260923,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial,
        certify: jpp::truth::CertifyMethod::Split,
        step: None,
        sequential: None,
    }
}

/// `n` 条两极构造真值（零错），正负各半。
fn 两极(key: &str, n: usize) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let yes = i % 2 == 0;
            serde_json::from_value(json!({
                "key": key, "item": format!("m{i}"), "p": if yes { 0.95 } else { 0.05 },
                "label": yes, "source": "computed",
                // B104-2（步 20h-1）：带材料文本，记录才有范围指纹（与运行材料同风格）
                "text": "顾客说要退款"
            }))
            .unwrap()
        })
        .collect()
}

fn 库(n: usize) -> CalibStore {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &两极("k", n), &选项(Some(0.25))).unwrap();
    assert_eq!(store.get("k").status, "上岗", "{}", rep[0].truth.gate);
    store
}

const 路由: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let e = cut(judge(state(mat("顾客说要退款")), test("该退吗", "k")));
handle(e, {
    act: fn() { {r: "act", 源: line_source(e)} },
    ignore: fn() { {r: "ignore", 源: line_source(e)} },
    unsure: fn(u) { consume(u, "drop"); {r: unsure_cause(u), 源: ""} }})
"#;

const 放行: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("顾客说要退款")), test("该退吗", "k"))), {
    act: fn() { content(do("退款", [], 0)) },
    ignore: fn() { "没退" },
    unsure: fn(u) { consume(u, "drop"); "没退" }})
"#;

fn 动作() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

fn 跑(src: &str, calib: &CalibStore, l: &mut Ledger) -> Result<jpp::Outcome, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calls = Cell::new(0u64);
    run(&program, 桩端口(&calls), calib, &动作(), l).map_err(|e| e.render())
}

/// 试用线出 act 供路由，报 W-trial-line（带 α、n、等级随材料传递，步 17b 前为「不随」）；line_source 说出「试用证书」；
/// 报告逐出口等级 Trial、releases false。
#[test]
fn 试用线可路由() {
    let mut l = Ledger::new();
    let o = 跑(路由, &库(80), &mut l).unwrap();
    assert_eq!(o.value_json()["r"], json!("act"));
    assert!(
        o.value_json()["源"]
            .as_str()
            .unwrap()
            .contains("试用证书:α=0.25"),
        "{}",
        o.value_json()
    );
    let w: Vec<&String> = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-trial-line"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(
        w[0].contains("α=0.25")
            && w[0].contains("n=80")
            && w[0].contains("等级随材料传递（B72-4）"),
        "{}",
        w[0]
    );
    assert_eq!(o.exits.len(), 1);
    assert_eq!(o.exits[0]["grade"], json!("Trial"));
    assert_eq!(o.exits[0]["releases"], json!(false));
    assert_eq!(o.exits[0]["exit"], json!("act"));
}

/// 试用线出口守卫不可逆 do → J-08 拒；同一程序在正式线（240 条）上放行。
#[test]
fn 试用线不放行不可逆do() {
    let e = 跑(放行, &库(80), &mut Ledger::new()).expect_err("试用线不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
    let o = 跑(放行, &库(240), &mut Ledger::new()).expect("正式线放行");
    assert_eq!(o.value_json(), json!("已退"));
    assert_eq!(o.exits[0]["grade"], json!("Certified"));
    assert_eq!(o.exits[0]["releases"], json!(true));
}

/// 只凭账本重放：0 调用，逐出口等级与首跑逐字节相同（校准库不再给，由账本头 calib_used 补回）。
#[test]
fn 只凭账本重放逐出口等级相同() {
    let mut l = Ledger::new();
    let first = 跑(路由, &库(80), &mut l).unwrap();
    let program = lower(&parse(路由).unwrap()).unwrap();
    // 与 CLI `--replay` 同一做法（run_io.rs）：校准记录只从账本头 calib_used 补回
    let mut 补回 = CalibStore::new();
    for (k, v) in &l.calib_used {
        补回.records.insert(
            k.clone(),
            serde_json::from_value(v["record"].clone()).unwrap(),
        );
    }
    assert!(
        补回.records.values().any(|r| r
            .certs
            .values()
            .any(|c| c.grade == jpp::effects::CertGrade::Trial)),
        "等级随证书进账本头"
    );
    let calls = Cell::new(0u64);
    let o = run_replay(
        &program,
        桩端口(&calls),
        &补回,
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| e.render())
    .unwrap();
    assert_eq!(calls.get(), 0, "重放不该再调用");
    assert_eq!(
        serde_json::to_string(&o.exits).unwrap(),
        serde_json::to_string(&first.exits).unwrap()
    );
    assert_eq!(o.value_json(), first.value_json());
}
