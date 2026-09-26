//! 惰性生成值与生成缓存（步 15h-2，B149、B160）：`gen` 随所在层在刷新点交出，读值才等；账本按层、按登记序
//! 一次写；推测不等生成；`--gen-cache` 的运行时一半。默认构建下用假脚本生成端口（不发任何请求）。
//!
//! 依据：B149（`12` §2.4）、B160；B94/B145（23c 检视点）；B151；预注册 `地基/过程记录/工程-步15h-2.md` 一·订正 (a)–(h)。

use jpp::backends::claude_p::{ClaudePConfig, ClaudePPort};
use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, NoCallPorts, Ports, ReplayPorts};
use jpp::interp::{GenCache, GenCacheEntry};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Session, lower, run_replay, syntax::parse};
use serde_json::{Value as Json, json};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// 假生成器脚本（由 `/bin/sh` 执行）：记下被调用（同目录 `called`），读掉 stdin，睡 `sleep` 秒，回答 3 项
fn 脚本(sleep: f64, items: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jpp-gen-lazy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("claude");
    let outer = json!({"type": "result", "is_error": false, "result": items,
                       "usage": {"input_tokens": 10, "output_tokens": 5}});
    std::fs::write(
        &path,
        format!(
            "touch \"$(dirname \"$0\")/called\"\ncat > /dev/null\nsleep {sleep}\nprintf '%s' '{}'\n",
            outer.to_string().replace('\'', "'\\''")
        ),
    )
    .unwrap();
    path
}

fn 生成端口(script: PathBuf, model: &str) -> ClaudePPort {
    ClaudePPort::new(ClaudePConfig {
        program: "/bin/sh".into(),
        pre_args: vec![script.to_string_lossy().into_owned()],
        model: model.into(),
        concurrency: 4,
        timeout: Duration::from_secs(20),
        cost_per_call: 0.0,
        taint_out: Some(jpp::Taint::Untrusted),
    })
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 判断端口：是非题一律 0.9，睡 `sleep` 秒后返回，记下每次返回的时刻
fn 慢判断端口<'a>(times: &'a RefCell<Vec<Instant>>, sleep: f64) -> FnPort<'a> {
    FnPort::judge("fixed-0", move |_s, qs| {
        std::thread::sleep(Duration::from_secs_f64(sleep));
        times.borrow_mut().push(Instant::now());
        Ok::<_, EffectError>(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    })
}

fn 判断端口<'a>(times: &'a RefCell<Vec<Instant>>) -> FnPort<'a> {
    慢判断端口(times, 0.0)
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("parse")).expect("lower")
}

fn 跑(
    src: &str,
    ports: Ports<'_>,
    ledger: &mut Ledger,
    cache: Option<Rc<RefCell<GenCache>>>,
) -> Result<Json, String> {
    let calib = 库();
    let acts = ActionRegistry::new();
    let mut s = Session::new(ports, &calib, &acts);
    if let Some(c) = cache {
        s = s.with_gen_cache(c);
    }
    s.run(&程序(src), &EntryArgs::default(), ledger)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

/// 账本里各条目的种类（`gen` 或 `judge`），按账本顺序
fn 条目序(ledger: &Ledger) -> Vec<String> {
    ledger
        .encode()
        .lines()
        .filter_map(|l| {
            if l.contains(r#""kind":"gen""#) {
                Some("gen".to_string())
            } else if l.contains(r#""Judge""#) {
                Some("judge".to_string())
            } else {
                None
            }
        })
        .collect()
}

const 重叠: &str = r#"budget {calls: 4, cost: 0, depth: 64};
let g = gen("提 3 个候选名字", [mat("需求")], 3, 0);
let e = cut(judge(state(mat("别的材料")), test("这段材料相关吗？", "k")));
let k = exit_kind(e);
{k: k, n: len(g)}"#;

/// (a)(h) 同层重叠：生成（0.8 秒）与判断（0.5 秒）在同一个刷新点一起交出，判断先返回、程序继续，读生成时才等；
/// 总时长约为较慢的一个而不是两者之和；账本按登记序（生成先登记）；只凭账本重放零调用、值相同。
#[test]
fn a_h_同层重叠() {
    let times = RefCell::new(vec![]);
    let mut gp = 生成端口(脚本(0.8, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(慢判断端口(&times, 0.5)));
    ports.replace(Box::new(&mut gp));
    let mut ledger = Ledger::new();
    let t0 = Instant::now();
    let v = 跑(重叠, ports, &mut ledger, None).unwrap();
    let total = t0.elapsed();
    assert_eq!(v, json!({"k": "act", "n": 3}));
    let judged = times.borrow()[0].duration_since(t0);
    assert!(
        judged < Duration::from_millis(750),
        "判断应在生成（0.8 秒）完成前返回，实际 {judged:?}"
    );
    assert!(
        total >= Duration::from_millis(750) && total < Duration::from_millis(1150),
        "同层并行时总时长约 0.8 秒（不是 1.3 秒）：{total:?}"
    );
    assert_eq!(条目序(&ledger), ["gen", "judge"], "层收齐后按登记序入账");
    ledger.rebuild_index();
    let again = run_replay(
        &程序(重叠),
        ReplayPorts::ports("fixed-0"),
        &库(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(again.value_json(), v);
    assert_eq!(again.cost.calls, 0);
}

/// (b) map 里 4 次互不依赖的 gen 同时在飞：总耗时约为最慢的一个。
#[test]
fn b_map_里的生成同时在飞() {
    let src = r#"budget {calls: 8, cost: 0, depth: 64};
let gs = map([1, 2, 3, 4], fn(i) { gen("提候选", [mat(text(i))], 3, 0) });
len(concat(concat(gs[0], gs[1]), concat(gs[2], gs[3])))"#;
    let mut gp = 生成端口(脚本(0.5, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut gp));
    let t0 = Instant::now();
    let v = 跑(src, ports, &mut Ledger::new(), None).unwrap();
    let s = t0.elapsed();
    assert_eq!(v, json!(12));
    assert!(
        s < Duration::from_millis(1200),
        "4 个各 0.5 秒的生成应重叠：{s:?}"
    );
}

const 两次生成: &str = r#"budget {calls: 6, cost: 0, depth: 64};
let g1 = gen("甲", [mat("a")], 3, 0);
let e = cut(judge(state(mat("x")), test("这段材料相关吗？", "k")));
let g2 = gen("乙", [mat("b")], 3, 0);
let k = exit_kind(e);
{k: k, a: len(g2), b: len(g1)}"#;

/// (c) 账本顺序与生成快慢无关；生成之间按登记序（先读 g2，账本里 g1 仍在 g2 之前）。
#[test]
fn c_账本顺序与完成先后无关() {
    let mut encoded = vec![];
    for sleep in [0.0, 0.4] {
        let mut gp = 生成端口(脚本(sleep, r#"["x","y","z"]"#), "sonnet");
        let times = RefCell::new(vec![]);
        let mut ports = NoCallPorts::ports();
        ports.replace(Box::new(判断端口(&times)));
        ports.replace(Box::new(&mut gp));
        let mut ledger = Ledger::new();
        跑(两次生成, ports, &mut ledger, None).unwrap();
        let gens: Vec<&str> = ledger
            .encode()
            .lines()
            .filter(|l| l.contains(r#""kind":"gen""#))
            .map(|l| if l.contains(r#""甲""#) { "g1" } else { "g2" })
            .collect();
        assert_eq!(gens, ["g1", "g2"], "生成条目按登记序");
        encoded.push(ledger.encode());
    }
    assert_eq!(encoded[0], encoded[1]);
}

/// (d) 登记序：生成先登记时层内生成条目在判断之前；判断先登记（在生成之前已刷新）时判断在前。
#[test]
fn d_同一检视点按登记序() {
    let 生成在前 = r#"budget {calls: 4, cost: 0, depth: 64};
let g = gen("甲", [mat("a")], 3, 0);
let e = cut(judge(state(mat("x")), test("这段材料相关吗？", "k")));
let n = len([e, g]);
{n: n, k: exit_kind(e)}"#;
    let 判断在前 = r#"budget {calls: 4, cost: 0, depth: 64};
let e = cut(judge(state(mat("x")), test("这段材料相关吗？", "k")));
let g = gen("甲", [mat("a")], 3, 0);
let n = len([e, g]);
{n: n, k: exit_kind(e)}"#;
    for (src, want) in [(生成在前, ["gen", "judge"]), (判断在前, ["judge", "gen"])] {
        let mut gp = 生成端口(脚本(0.0, r#"["甲","乙","丙"]"#), "sonnet");
        let times = RefCell::new(vec![]);
        let mut ports = NoCallPorts::ports();
        ports.replace(Box::new(判断端口(&times)));
        ports.replace(Box::new(&mut gp));
        let mut ledger = Ledger::new();
        跑(src, ports, &mut ledger, None).unwrap();
        assert_eq!(条目序(&ledger), want, "{src}");
    }
}

/// (e) 推测不等生成：`if` 分支里读生成结果的判断站点不被推测，前面的判断在生成完成前发出。
#[test]
fn e_推测不等生成() {
    let src = r#"budget {calls: 6, cost: 0, depth: 64};
let g = gen("甲", [mat("a")], 3, 0);
let j = judge(state(mat("x")), test("这段材料相关吗？", "k"));
let k = exit_kind(cut(j));
let r = if k == "act" { exit_kind(cut(judge(state(g[0]), test("这个候选好吗？", "k")))) } else { "none" };
{k: k, r: r}"#;
    let times = RefCell::new(vec![]);
    let mut gp = 生成端口(脚本(0.8, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(判断端口(&times)));
    ports.replace(Box::new(&mut gp));
    let t0 = Instant::now();
    let v = 跑(src, ports, &mut Ledger::new(), None).unwrap();
    assert_eq!(v, json!({"k": "act", "r": "act"}));
    let first = times.borrow()[0].duration_since(t0);
    assert!(
        first < Duration::from_millis(500),
        "第一个判断应在生成完成前发出，实际 {first:?}"
    );
}

/// (f) 没被读取的生成：程序结束的刷新把它交出、收齐入账；在它之后、任何刷新点之前出错：不交出、不调用、不入账。
#[test]
fn f_结清() {
    let unused = r#"budget {calls: 2, cost: 0, depth: 64};
let g = gen("甲", [mat("a")], 3, 0);
{k: 1}"#;
    let mut gp = 生成端口(脚本(0.1, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut gp));
    let mut ledger = Ledger::new();
    跑(unused, ports, &mut ledger, None).unwrap();
    assert_eq!(条目序(&ledger), ["gen"]);

    let broken = r#"budget {calls: 2, cost: 0, depth: 64};
let g = gen("甲", [mat("a")], 3, 0);
let z = 0;
{k: 1 / z}"#;
    let script = 脚本(0.1, r#"["甲","乙","丙"]"#);
    let mut gp = 生成端口(script.clone(), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut gp));
    let mut ledger = Ledger::new();
    assert!(跑(broken, ports, &mut ledger, None).is_err());
    assert!(条目序(&ledger).is_empty(), "没走到刷新点的生成不入账");
    assert!(
        !script.parent().unwrap().join("called").exists(),
        "没走到刷新点的生成不交出、不花钱（B160）"
    );
}

/// (f2) 层开着时同键不重发：推测登记过的判断随生成所在层发出，真站点在层收齐前走到，从层里取回。
#[test]
fn f2_层开着时同键不重发() {
    let src = r#"budget {calls: 6, cost: 0, depth: 64};
let g = gen("甲", [mat("a")], 3, 0);
let e1 = cut(judge(state(mat("x")), test("这段材料相关吗？", "k")));
let k1 = exit_kind(e1);
let r = if k1 == "act" { exit_kind(cut(judge(state(mat("y")), test("这个候选好吗？", "k")))) } else { "none" };
{k1: k1, r: r, n: len(g)}"#;
    let times = RefCell::new(vec![]);
    let mut gp = 生成端口(脚本(0.3, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(判断端口(&times)));
    ports.replace(Box::new(&mut gp));
    let v = 跑(src, ports, &mut Ledger::new(), None).unwrap();
    assert_eq!(v, json!({"k1": "act", "r": "act", "n": 3}));
    assert_eq!(times.borrow().len(), 2, "x、y 两个状态各一次；y 不重发");
}

const 缓存程序: &str = r#"budget {calls: 2, cost: 0, depth: 64};
let g = gen("提 3 个候选名字", [mat("需求")], 3, 0);
if is_fail(g) { "fail" } else { map(g, fn(m) { content(m) }) }"#;

/// (g) 生成缓存：第二次运行不调用（假脚本换成会失败的也照样取到），换模型不命中，失败不进缓存。
#[test]
fn g_生成缓存() {
    let cache = Rc::new(RefCell::new(GenCache::default()));
    let mut gp = 生成端口(脚本(0.0, r#"["甲","乙","丙"]"#), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut gp));
    let v1 = 跑(缓存程序, ports, &mut Ledger::new(), Some(cache.clone())).unwrap();
    assert_eq!(v1, json!(["甲", "乙", "丙"]));
    // 宿主写回再读入（与 CLI 同一件事）
    let fresh: Vec<(String, GenCacheEntry)> = std::mem::take(&mut cache.borrow_mut().fresh);
    assert_eq!(fresh.len(), 1);
    let reload = Rc::new(RefCell::new(GenCache {
        entries: fresh.into_iter().collect(),
        ..Default::default()
    }));

    // 同模型：命中，不调用（脚本会失败，被调用就会得到 fail）
    let mut bad = 生成端口(脚本(0.0, "不是 JSON"), "sonnet");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut bad));
    let mut ledger = Ledger::new();
    let calib = 库();
    let acts = ActionRegistry::new();
    let o = Session::new(ports, &calib, &acts)
        .with_gen_cache(reload.clone())
        .run(&程序(缓存程序), &EntryArgs::default(), &mut ledger)
        .unwrap_or_else(|e| panic!("{}", e.render()));
    assert_eq!(o.value_json(), v1);
    assert_eq!(o.cost.calls, 0);
    assert_eq!(o.cost.replayed, 1);
    assert_eq!(reload.borrow().hits, 1);
    assert_eq!(条目序(&ledger), ["gen"], "命中也写账本条目");

    // 换模型：不命中，调用（这里的脚本失败 → fail），失败不进缓存
    let mut other = 生成端口(脚本(0.0, "不是 JSON"), "haiku");
    let mut ports = NoCallPorts::ports();
    ports.replace(Box::new(&mut other));
    let v3 = 跑(缓存程序, ports, &mut Ledger::new(), Some(reload.clone())).unwrap();
    assert_eq!(v3, json!("fail"));
    assert!(reload.borrow().fresh.is_empty(), "失败不进缓存");
}
