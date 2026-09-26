//! 生成器端口 `claude -p`（步 15h-1，B149）：非阻塞 `submit`/`poll`、并发、失败四类、输出 taint、
//! 失败走 `Fail`、重放。默认构建下用假脚本代替 `claude`（不发任何请求）；真机一条 `#[ignore]`。
//!
//! 依据：B149（`12` §2.4）；`20` v2 §五 S12；`21` 步 15h；预注册 `地基/过程记录/工程-步15h-1.md` (a)–(g)。

use jpp::backends::claude_p::{ClaudePConfig, ClaudePPort};
use jpp::effects::{
    CalibStore, CallInput, EffectCall, EffectId, EffectInstance, EffectOut, EffectPort, FixedPorts,
    NoCallPorts, Ports, ReplayPorts, Ticket,
};
use jpp::ledger::Ledger;
use jpp::{ActionRegistry, lower, run, run_replay, syntax::parse};
use serde_json::{Value as Json, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Poll;
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// 写一个假 `claude` 脚本（由 `/bin/sh` 执行）：读掉 stdin，睡 `sleep` 秒，按 `body` 输出（shell 片段）。
fn fake(sleep: f64, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jpp-gen-port-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude");
    std::fs::write(&path, format!("cat > /dev/null\nsleep {sleep}\n{body}\n")).unwrap();
    path
}

/// 假脚本的一次正常回答：`result` 是 JSON 数组的文本
fn ok_body(items: &str) -> String {
    let outer = json!({"type": "result", "is_error": false, "result": items,
                       "usage": {"input_tokens": 10, "output_tokens": 5}});
    format!("printf '%s' '{}'", outer.to_string().replace('\'', "'\\''"))
}

fn port(program: PathBuf, concurrency: usize, timeout_s: f64) -> ClaudePPort {
    // 经 `/bin/sh <脚本>` 跑：新建的可执行文件第一次执行时 macOS 先扫描数秒，会把计时测试拖垮
    ClaudePPort::new(ClaudePConfig {
        program: "/bin/sh".into(),
        pre_args: vec![program.to_string_lossy().into_owned()],
        model: "sonnet".into(),
        concurrency,
        timeout: Duration::from_secs_f64(timeout_s),
        cost_per_call: 0.0,
        taint_out: Some(jpp::Taint::Untrusted),
    })
}

fn call(n: usize) -> EffectCall {
    EffectCall {
        instance: EffectInstance {
            effect: EffectId::Gen,
            model: "claude-p/sonnet".into(),
        },
        input: CallInput::Prompt {
            prompt: "提 3 个候选".into(),
            ctx: vec![json!("需求")],
            n,
            retry_seq: 0,
        },
    }
}

/// 等到完成，返回（输出、失败）
fn wait(p: &mut ClaudePPort, t: &Ticket) -> (Vec<Json>, Option<String>) {
    loop {
        if let Poll::Ready(r) = p.poll(t) {
            let Ok(EffectOut::Mats(g)) = r else {
                panic!("端口应当给材料")
            };
            return (g.outputs, g.failure);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// (a) `submit` 立即返回；`poll` 先 `Pending` 后完成。
#[test]
fn a_submit_立即返回() {
    let mut p = port(fake(0.5, &ok_body(r#"["甲","乙","丙"]"#)), 4, 10.0);
    let t0 = Instant::now();
    let ts = p.submit(vec![call(3)]).unwrap();
    assert!(
        t0.elapsed() < Duration::from_millis(100),
        "submit 等了 {:?}",
        t0.elapsed()
    );
    assert!(p.poll(&ts[0]).is_pending(), "子进程还在睡，不该完成");
    let (out, fail) = wait(&mut p, &ts[0]);
    assert_eq!(fail, None);
    assert_eq!(out, vec![json!("甲"), json!("乙"), json!("丙")]);
    assert!(t0.elapsed() >= Duration::from_millis(450));
}

/// (b) 并发：4 个调用各睡 0.5 秒，并发 4 时重叠，并发 1 时串行。
#[test]
fn b_并发上限来自配置() {
    let script = fake(0.5, &ok_body(r#"["x","y","z"]"#));
    for (conc, lo, hi) in [(4usize, 0.45, 1.2), (1, 2.0, 10.0)] {
        let mut p = port(script.clone(), conc, 10.0);
        let t0 = Instant::now();
        let ts = p.submit((0..4).map(|_| call(3)).collect()).unwrap();
        for t in &ts {
            assert_eq!(wait(&mut p, t).1, None);
        }
        let s = t0.elapsed().as_secs_f64();
        assert!(
            s >= lo && s < hi,
            "并发 {conc}：总耗时 {s:.2} 秒，应在 [{lo}, {hi})"
        );
    }
}

/// (c) 失败四类：非 JSON 与项数不对 → malformed；空 → empty；超时 → timeout；退出码非零与 is_error → failed。
#[test]
fn c_失败四类() {
    let err_body = {
        let outer = json!({"is_error": true, "result": "rate limited"});
        format!("printf '%s' '{outer}'")
    };
    let cases: Vec<(PathBuf, f64, &str)> = vec![
        (fake(0.0, &ok_body("你好")), 10.0, "malformed"),
        (fake(0.0, &ok_body(r#"["只有一项"]"#)), 10.0, "malformed"),
        (fake(0.0, "echo not-json"), 10.0, "malformed"),
        (fake(0.0, &ok_body("[]")), 10.0, "empty"),
        (fake(0.0, &ok_body(r#"["", "  ", ""]"#)), 10.0, "empty"),
        (fake(3.0, &ok_body(r#"["a","b","c"]"#)), 0.3, "timeout"),
        (fake(0.0, "exit 3"), 10.0, "failed"),
        (fake(0.0, &err_body), 10.0, "failed"),
    ];
    for (script, timeout, kind) in cases {
        let mut p = port(script.clone(), 1, timeout);
        let ts = p.submit(vec![call(3)]).unwrap();
        let (out, fail) = wait(&mut p, &ts[0]);
        assert!(out.is_empty(), "{kind}：失败时不给输出");
        let f = fail.unwrap_or_else(|| panic!("{} 应当失败为 {kind}", script.display()));
        assert!(f.starts_with(&format!("{kind}:")), "应为 {kind}，实为 {f}");
    }
}

const 程序: &str = r#"budget {calls: 2, cost: 0, depth: 64};
let brief = mat("为社区咖啡馆起名");
let xs = gen("提 3 个候选名字", [brief], 3, 0);
if is_fail(xs) { {fail: true, taints: []} } else { {fail: false, taints: map(xs, fn(m) { m.taint })} }"#;

fn 跑(ports: Ports<'_>, ledger: &mut Ledger) -> Json {
    let program = lower(&parse(程序).expect("parse")).expect("lower");
    run(
        &program,
        ports,
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
    .value_json()
}

fn 重放(ledger: &mut Ledger) -> Json {
    let program = lower(&parse(程序).expect("parse")).expect("lower");
    ledger.rebuild_index();
    run_replay(
        &program,
        ReplayPorts::ports("fixed-0"),
        &CalibStore::new(),
        &ActionRegistry::new(),
        ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()))
    .value_json()
}

/// (d)(f) 生成的材料按端口声明为 untrusted；夹具生成端口不声明，照旧 ∨ ctx（trusted）；重放取回同一 taint。
#[test]
fn d_f_输出_taint_与重放() {
    let mut p = port(fake(0.0, &ok_body(r#"["甲","乙","丙"]"#)), 2, 10.0);
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut p));
    let mut ledger = Ledger::new();
    let v = 跑(ports, &mut ledger);
    assert_eq!(
        v,
        json!({"fail": false, "taints": ["untrusted", "untrusted", "untrusted"]})
    );
    assert_eq!(重放(&mut ledger), v, "只凭账本重放：零调用、taint 相同");

    let mut fp = FixedPorts::new();
    fp.fix_gen(
        "提 3 个候选名字",
        0,
        vec![json!("甲"), json!("乙"), json!("丙")],
    );
    let mut l2 = Ledger::new();
    let v2 = 跑(fp.ports(), &mut l2);
    assert_eq!(
        v2,
        json!({"fail": false, "taints": ["trusted", "trusted", "trusted"]})
    );
}

/// (e)(f) 生成器失败 → `Fail` 值，程序照常返回，账本有一条 gen 条目；重放取回同一个 `Fail`。
#[test]
fn e_f_失败走_fail() {
    let mut p = port(fake(0.0, "echo not-json"), 1, 10.0);
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut p));
    let mut ledger = Ledger::new();
    let v = 跑(ports, &mut ledger);
    assert_eq!(v, json!({"fail": true, "taints": []}));
    let gens = ledger
        .encode()
        .lines()
        .filter(|l| l.contains(r#""kind":"gen""#))
        .count();
    assert_eq!(gens, 1, "失败也记一条 gen 条目");
    assert_eq!(重放(&mut ledger), v);
}

/// (g) 画像换并发（1 与 4）：同程序两次运行的账本逐字节相同。
#[test]
fn g_换并发账本不变() {
    let script = fake(0.1, &ok_body(r#"["甲","乙","丙"]"#));
    let mut encoded = vec![];
    for conc in [1usize, 4] {
        let mut p = port(script.clone(), conc, 10.0);
        let mut ports = NoCallPorts::ports();
        ports.replace(Box::new(&mut p));
        let mut ledger = Ledger::new();
        跑(ports, &mut ledger);
        encoded.push(ledger.encode());
    }
    assert_eq!(encoded[0], encoded[1]);
}

/// 真机一条（`--features live`，手动跑；花费上限见预注册）：CLI 跑最小示例，`claude -p` 一次、JEV 一次。
#[test]
#[ignore]
#[cfg(feature = "live")]
fn 真机_gen_choose() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&root)
        .args([
            "run",
            "examples/gen-choose.jpp",
            "--backend",
            "live",
            "--gen-model",
            "sonnet",
            "--profiles-dir",
            "profiles",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(r["gen_backend"]["name"], "claude-p");
}
