//! 真机传输超时（`地基/过程记录/工程-传输超时.md` §四第 9 条）：假传输层注入挂起。
//! 超时属 absent（`12` B35），之后走 `budget.absent` 的既有路由：重试逐次计费，全部失败按 `then`；
//! 不声明 `absent` 即 `E-rt-client`。传输层不重试、不给兜底读数。
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use jpp::effects::{CalibStore, JevClient, JevPorts};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, lower, run, syntax::parse};
use serde_json::{Value as Json, json};

/// 前 `挂` 次请求各睡 `睡` 毫秒（模拟挂起），之后立刻答 0.9
fn 传输(挂: u64, 睡: u64, n: Arc<AtomicU64>) -> jpp::effects::Attempt {
    Arc::new(move |_body: &Json| {
        let k = n.fetch_add(1, Ordering::SeqCst) + 1;
        if k <= 挂 {
            std::thread::sleep(Duration::from_millis(睡));
        }
        Ok(json!({"answers": {"q0": {"noul": 0.9}}}))
    })
}

fn 源(absent: &str) -> String {
    format!(
        "budget {{calls: 10, cost: 0, depth: 8{absent}}};\nlet e = cut(judge(state(mat(\"材料\")), test(\"行吗\", \"k\")));\nhandle(e, {{act: fn() {{ \"act\" }}, ignore: fn() {{ \"ignore\" }}, unsure: fn(u) {{ {{c: unsure_cause(u), exit: u}} }}}})\n"
    )
}

fn 跑(
    src: &str,
    挂: u64,
    睡: u64,
    timeout: Option<Duration>,
    ledger: &mut Ledger,
) -> (Result<jpp::Outcome, String>, u64, Duration) {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    let n = Arc::new(AtomicU64::new(0));
    let mut c = JevPorts::new(JevClient::with_timed_transport(
        "jev-1.13.0",
        timeout,
        传输(挂, 睡, n.clone()),
    ));
    let t0 = Instant::now();
    let o =
        run(&program, c.ports(), &calib, &ActionRegistry::new(), ledger).map_err(|e| e.render());
    (o, n.load(Ordering::SeqCst), t0.elapsed())
}

const 超时: Option<Duration> = Some(Duration::from_millis(200));

/// (a) 挂起一次，重试成功：两次都计费，出口照常，不等挂起结束。
#[test]
fn 挂起一次_重试成功且逐次计费() {
    let src = 源(r#", absent: {retry: 1, backoff: 0, then: "conservative", breaker: 5}"#);
    let (o, 请求, 用时) = 跑(&src, 1, 3000, 超时, &mut Ledger::new());
    let o = o.expect("跑完");
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.cost.calls, 2, "挂起的那次也计费（B32 逐次计费）");
    assert_eq!(请求, 2);
    assert!(
        用时 < Duration::from_millis(2500),
        "超时后就该返回，不等挂起结束：{用时:?}"
    );
}

/// (b) 每次都挂起、conservative：出口 Unsure(absent)，原因带 E-timeout；只凭账本重放 0 调用、出口相同。
#[test]
fn 一直挂起_保守策略出口转absent_可重放() {
    let src = 源(r#", absent: {retry: 2, backoff: 0, then: "conservative", breaker: 5}"#);
    let mut l = Ledger::new();
    let (o, 请求, _) = 跑(&src, 100, 3000, 超时, &mut l);
    let o = o.expect("conservative 下程序继续");
    assert_eq!(o.value_json()["c"], json!("absent"));
    assert_eq!(o.cost.calls, 3, "首发 + 重试 2，各计一次");
    assert_eq!(请求, 3);
    assert!(l.encode().contains("E-timeout"), "缺席账记下超时原因");
    // 重放：账本里已记缺席，不再发请求
    let (o2, 请求2, _) = 跑(&src, 0, 0, 超时, &mut l);
    assert_eq!(o2.expect("重放").value_json()["c"], json!("absent"));
    assert_eq!(请求2, 0);
}

/// (c) then: fail → E-rt-absent，报文带 E-timeout。
#[test]
fn 一直挂起_fail策略报e_rt_absent() {
    let src = 源(r#", absent: {retry: 1, backoff: 0, then: "fail", breaker: 5}"#);
    let (o, _, _) = 跑(&src, 100, 3000, 超时, &mut Ledger::new());
    let e = o.expect_err("fail 策略即运行期错误");
    assert!(e.contains("E-rt-absent") && e.contains("E-timeout"), "{e}");
}

/// (d) 不声明 absent：客户端错误即 E-rt-client（既有语义），报文带 E-timeout，超时后很快返回。
#[test]
fn 不声明absent_超时即e_rt_client() {
    let (o, 请求, 用时) = 跑(&源(""), 100, 3000, 超时, &mut Ledger::new());
    let e = o.expect_err("没有 absent 策略");
    assert!(e.contains("E-rt-client") && e.contains("E-timeout"), "{e}");
    assert_eq!(请求, 1, "传输层不自己重试超时");
    assert!(用时 < Duration::from_millis(2500), "{用时:?}");
}

/// (e) 画像没给超时（None）：不设超时，慢响应照常返回，行为同接线前。
#[test]
fn 不给超时时照常等响应() {
    let (o, 请求, 用时) = 跑(&源(""), 1, 300, None, &mut Ledger::new());
    assert_eq!(o.expect("跑完").value_json(), json!("act"));
    assert_eq!(请求, 1);
    assert!(用时 >= Duration::from_millis(300));
}
