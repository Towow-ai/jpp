//! B32：判断力缺席与时延。判断器调用失败时按 `budget.absent` 重试、退避，然后
//! escalate（挂起）/ conservative（出口 Unsure(absent)）/ fail（运行期错误）；连续缺席熔断；
//! 时延预算用完后的判断站点转 Unsure(latency)；静态估计超预算的计划在检查阶段拒绝。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports, Profile};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 前 `坏` 次调用失败，之后返回 0.9；每次调用睡 `睡` 毫秒
/// （步 15c：原 `impl Client` 的桩改为三个闭包端口；`calls` 用 `RefCell` 借给判断端口计数）
fn 时好时坏(坏: u64, 睡: u64, calls: &RefCell<u64>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            if 睡 > 0 {
                std::thread::sleep(std::time::Duration::from_millis(睡));
            }
            if *calls.borrow() <= 坏 {
                return Err(EffectError("连接中断".into()));
            }
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(None)))
}

fn 源(budget: &str) -> String {
    format!(
        r#"
budget {budget};
fn 判(t) {{
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    {{k: exit_kind(e), e: e}}
}}
// 缺席类出口不能 drop（B95，步 21）：随返回值转交
let rs = [判("甲"), 判("乙"), 判("丙")];
{{v: map(rs, fn(r) {{ r.k }}), pending: map(rs, fn(r) {{ r.e }})}}
"#
    )
}

fn 跑(src: &str, 坏: u64, 睡: u64, ledger: &mut Ledger) -> (Result<jpp::Outcome, String>, u64) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0u64);
    let o = run(
        &program,
        时好时坏(坏, 睡, &calls),
        &calib,
        &ActionRegistry::new(),
        ledger,
    )
    .map_err(|e| e.render());
    let n = *calls.borrow();
    (o, n)
}

#[test]
fn 保守策略出口转未决并可重放() {
    let src = 源(
        r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 1, backoff: 0, then: "conservative", breaker: 5}}"#,
    );
    let mut l = Ledger::new();
    let (o, calls) = 跑(&src, 100, 0, &mut l);
    let o = o.expect("conservative 下程序继续");
    let v = o.value_json()["v"].clone();
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .all(|x| x.as_str().unwrap().contains("absent")),
        "{v}"
    );
    assert_eq!(calls, 6, "三次调用各重试一次");
    assert_eq!(
        o.cost.calls, 6,
        "逐次计费（选项 A）：3 站点 ×（首发 + 重试 1）"
    );
    // 只凭账本重放：不再发，出口一致
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    let calls2 = RefCell::new(0u64);
    let o2 = run(
        &program,
        时好时坏(0, 0, &calls2),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("重放");
    assert_eq!(o2.value_json()["v"], v);
    assert_eq!(*calls2.borrow(), 0, "缺席事件已记账，重放不发");
}

#[test]
fn 重试成功就照常出口() {
    let src = 源(
        r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative", breaker: 5}}"#,
    );
    let (o, _) = 跑(&src, 1, 0, &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(
        o.value_json()["v"],
        serde_json::json!(["act", "act", "act"])
    );
    assert_eq!(o.cost.calls, 4, "第 1 站点失败 1 次后成功（2），其余各 1");
}

#[test]
fn 升级策略挂起待续跑() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0}}"#);
    let (o, _) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("挂起不是错误");
    assert_eq!(
        o.pending.first().map(|p| p.cause.as_str()),
        Some("absent"),
        "默认 then = escalate"
    );
    assert_eq!(o.cost.calls, 1, "失败的那一次也计");
}

#[test]
fn 失败策略是运行期错误() {
    let src =
        源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "fail"}}"#);
    let (o, _) = 跑(&src, 100, 0, &mut Ledger::new());
    assert!(o.expect_err("fail").contains("缺席"));
}

#[test]
fn 连续缺席熔断后不再发() {
    let src = 源(
        r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "conservative", breaker: 2}}"#,
    );
    let (o, calls) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(calls, 2, "第三个站点熔断，不再发");
    assert_eq!(o.cost.calls, 2, "熔断不发、不计费");
}

#[test]
fn 时延预算用完转超时未决() {
    let src = 源(r#"{calls: 20, cost: 0, depth: 16, latency_p95: 0.001}"#);
    let (o, _) = 跑(&src, 0, 5, &mut Ledger::new());
    let v = o.expect("跑完").value_json()["v"].clone();
    let a = v.as_array().unwrap();
    assert!(
        a.iter().all(|x| x.as_str().unwrap().contains("latency")),
        "每次调用 5ms，预算 1ms：{v}"
    );
}

#[test]
fn 静态估计超时延预算即拒绝() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 8, latency_p95: 0.5};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
consume(e, "drop");
1
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let profile = Profile {
        hash: Some("测试".into()),
        ..Profile::untested()
    }
    .with_latency_p95(1.0, "测试");
    let r = jpp::check::check_with_profile(&program, &profile);
    assert!(
        r.diagnostics.iter().any(|d| d.rule == "E-latency"),
        "{:?}",
        r.diagnostics
    );
    let r = jpp::check::check(&program);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.rule == "W-untested" && d.message.contains("latency")),
        "{:?}",
        r.diagnostics
    );
}

/// 失败策略报错时，失败的那一次也在账面上（选项 A）：用账本看（运行期错误没有 Outcome）
#[test]
fn 失败策略的尝试入账() {
    let src =
        源(r#"{calls: 20, cost: 0, depth: 16, absent: {retry: 0, backoff: 0, then: "fail"}}"#);
    let mut l = Ledger::new();
    let (o, calls) = 跑(&src, 100, 0, &mut l);
    assert!(o.is_err());
    assert_eq!(calls, 1);
    let n: u64 = l
        .entries
        .iter()
        .map(|e| match e {
            jpp::ledger::Entry::Absent { attempts, .. } => *attempts,
            _ => 0,
        })
        .sum();
    assert_eq!(n, 1, "账本记下 1 次尝试");
}

fn 单站点(budget: &str) -> String {
    format!(
        r#"
budget {budget};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
{{v: exit_kind(e), pending: e}}
"#
    )
}

/// 补丁请求 1：`calls: 1, retry: 5` 且始终失败 → 第 2 次尝试前预算停机，后端只收到 1 次。
/// 步 22-0（B93）起预算停机是停发不是挂起：这一站点出口 `Unsure(budget)`，程序照常返回
#[test]
fn retry_charges_each_attempt() {
    let src = 单站点(
        r#"{calls: 1, cost: 0, depth: 16, absent: {retry: 5, backoff: 0.01, then: "conservative"}}"#,
    );
    let (o, calls) = 跑(&src, 100, 0, &mut Ledger::new());
    let o = o.expect("预算停机是停发，程序照常返回");
    assert!(o.pending.is_empty(), "{:?}", o.pending);
    assert_eq!(o.value_json()["v"], serde_json::json!("unsure(budget)"));
    assert_eq!(calls, 1, "后端恰好收到 1 次");
    assert_eq!(o.cost.calls, 1);
}

/// 前两次失败、第三次成功 → 该站点计 3 次
#[test]
fn retry_success_counts_all_attempts() {
    let src = 单站点(
        r#"{calls: 10, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative"}}"#,
    );
    let (o, calls) = 跑(&src, 2, 0, &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(o.value_json()["v"], serde_json::json!("act"));
    assert_eq!(calls, 3);
    assert_eq!(o.cost.calls, 3);
}

/// 全失败也计时延：退避 0.01 + 0.02 = 0.03s 进时延预算，于是下一个站点转 Unsure(latency)
#[test]
fn terminal_failure_counts_latency() {
    // 第 1 站点 3 次全失败（首发 + 重试 2），之后成功；预算 0.025s 被第 1 站点的退避用完
    let src = 源(
        r#"{calls: 20, cost: 0, depth: 16, latency_p95: 0.025, absent: {retry: 2, backoff: 0.01, then: "conservative", breaker: 5}}"#,
    );
    let (o, calls) = 跑(&src, 3, 0, &mut Ledger::new());
    let v = o.expect("跑完").value_json()["v"].clone();
    let a = v.as_array().unwrap();
    assert!(a[0].as_str().unwrap().contains("absent"), "{v}");
    assert!(
        a[1].as_str().unwrap().contains("latency"),
        "失败路径的退避计入时延：{v}"
    );
    assert_eq!(calls, 3, "时延用完后不再发");
}

/// 首跑在重试中途因预算停机；只凭账本审计重放停在同一站点、同样的原因。
/// 步 22-0（B93）起停机是停发：首跑与重放都返回，第 2、3 站点出口 `Unsure(budget)`，停发的首个站点相同
#[test]
fn absent_replay_stops_where_first_run_stopped() {
    let src = 源(
        r#"{calls: 3, cost: 0, depth: 16, absent: {retry: 2, backoff: 0, then: "conservative", breaker: 5}}"#,
    );
    let mut l = Ledger::new();
    // 第 1 站点首发成功；第 2 站点首发失败，重试 1 次失败，重试 2 时预算不够（1+2 已花）
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    // 第二站点坏：判断从第 2 次调用起失败（步 15c：原 `impl Client` 改为闭包端口）
    let calls = RefCell::new(0u64);
    let o1 = run(
        &program,
        Ports::new()
            .with(FnPort::judge("fixed-0", |_s, qs| {
                *calls.borrow_mut() += 1;
                if *calls.borrow() >= 2 {
                    return Err(EffectError("连接中断".into()));
                }
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
                Err(EffectError("不该 gen".into()))
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| Ok(None))),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("首跑返回");
    assert_eq!(
        o1.value_json()["v"],
        serde_json::json!(["act", "unsure(budget)", "unsure(budget)"])
    );
    let 首停 = o1.budget.as_ref().expect("首跑预算停发").first_site;
    assert_eq!(o1.cost.calls, 3);
    let calls2 = RefCell::new(0u64);
    let o2 = jpp::run_replay(
        &program,
        时好时坏(0, 0, &calls2),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("重放返回");
    assert_eq!(o2.value_json(), o1.value_json());
    assert_eq!(
        o2.budget.as_ref().map(|b| b.first_site),
        Some(首停),
        "停在同一站点"
    );
    assert_eq!(*calls2.borrow(), 0, "重放不发");
}
