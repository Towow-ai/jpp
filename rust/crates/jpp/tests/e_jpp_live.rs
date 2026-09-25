//! **E-JPP-LIVE**：一个 `.jpp` 程序对真机 Jev 跑通（预注册见 `EXPERIMENTS.md`:1261 与修订 :1375）。
//!
//! **默认 `ignore`**：真机那条要花钱，只在显式点名时跑。
//! 基准那条不花钱，照常跑。
//!
//! **测不到 CLI 那一层**——CLI 今天没有真机路径（`--help` 明写 "no model API requests are
//! made"），接一条是第四件越界。**按预注册的退路走 core 的 API，而这个限制必须进结论。**

use jpp::effects::{CalibStore, FnPort, GenResult, JudgeResult, Ports, Profile};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;
use std::cell::RefCell;

/// 覆盖四种效应：`judge`（test + select）、`gen`、`do`、`ask`（预置答案，不真打断人）。
/// **`gen` 不在这份程序里**，而那本身是这次实验的第一条发现（$0 拿到的）：
/// **`JevClient::generate` 返回 `Err("JevClient 不生成")`——真机路径上根本没有 `gen`。**
/// 而替身实现了它，于是所有含 `gen` 的测试在替身上绿、在真机上**跑都跑不起来**。
/// 预注册的「四种效应」这一趟只能覆盖三种，**这要进结论，不是绕过去**。
const 程序: &str = r#"
budget {calls: 12, cost: 1, depth: 64, escalate: 2};
let 料 = do("取内部", [], 0);
let 稿 = mat("门店客流上升，周末为主。");
let 合规 = handle(cut(judge(state(料), test("这段材料适合公开吗？", "live.noul"))), {
    act: fn() { "可公开" },
    ignore: fn() { "不可公开" },
    unsure: fn(u) { consume(u, "drop"); "待定" }});
let 挑 = handle(cut(judge(state(mat("选一个最合适的说法"), {over: [稿, 料]}),
                          select("哪个更适合对外", "live.choice"))), {
    pick: fn(k) { k },
    unsure: fn(u) { consume(u, "drop"); -1 }});
{合规: 合规, 挑: 挑}
"#;

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("取内部", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::Text(
            "本季度门店客流同比上升 12%，其中周末占比 58%。".into(),
            jpp::value::Taint::Trusted.into(),
        ))
    });
    a
}

fn 校准() -> CalibStore {
    let mut c = CalibStore::new();
    // **线继承 E-CAL 那批数据的选择偏倚**（校准集是「两模型都同意」的子集）——
    // 预注册修订里那条赌要用它，跑完要按线的两个读法各算一遍。
    c.put("live.noul", 0.66, 0.56, 73, "上岗", Some(0.05))
        .unwrap();
    c.put("live.choice", 0.35, 0.35, 74, "上岗", Some(0.05))
        .unwrap();
    let 真档 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    c.profile = Profile::load(&真档).expect("真档案读得动");
    c
}

/// 固定观察的桩（步 15c 由 `impl Client` 改为闭包端口）：判断、生成、问人三个实例。
// **与真机同一个 model_id**：账本键含 `model_id`，用 "fixed-baseline" 会让
// 赌 2（账本键逐条对得上）变成**我自己造出来的不一致**——
// 一个由我引起的键不匹配，读起来会像语义差。
fn 桩(calls: &RefCell<usize>) -> Ports<'_> {
    const M: &str = "jev-1.13.0";
    Ports::new()
        .with(FnPort::judge(M, move |s: &State, qs: &[&Question]| {
            *calls.borrow_mut() += 1;
            let answers = qs
                .iter()
                .map(|q| match q.op {
                    jpp::value::Op::Select => {
                        let n = s.over.len().max(1);
                        let mut v = vec![0.1; n];
                        v[0] = 1.0 - 0.1 * (n - 1) as f64;
                        Answer::Choice(v)
                    }
                    jpp::value::Op::Measure => Answer::Score(q.scale.iter().map(|_| 0.5).collect()),
                    jpp::value::Op::Test => Answer::Noul(0.9),
                })
                .collect();
            let (ms, pm) = qs
                .iter()
                .map(|q| match q.op {
                    jpp::value::Op::Select => (Some(1.0), 2),
                    _ => (None, 0),
                })
                .collect::<(Vec<_>, Vec<_>)>();
            Ok(JudgeResult {
                answers,
                tokens: 0,
                cost: 0.0,
                mode_share: ms,
                perms: pm,
            })
        }))
        .with(FnPort::generate(M, |_p, _c, n, _r| {
            Ok(GenResult {
                outputs: (0..n)
                    .map(|_| Json::String("门店客流上升，周末为主。".into()))
                    .collect(),
                tokens: 0,
                cost: 0.0,
            })
        }))
        .with(FnPort::ask(M, |_s, _q| Ok(None)))
}

fn 报(名: &str, o: &jpp::Outcome, l: &Ledger) {
    println!(
        "【{名}】value={} calls={} replayed={} tokens={} usd={:.6} layers={} 证据={}",
        o.value_json(),
        o.cost.calls,
        o.cost.replayed,
        o.cost.tokens,
        o.cost.usd,
        o.layers.len(),
        o.evidence.len()
    );
    for w in &o.trace.warnings {
        println!("  warn: {w}");
    }
    // 账本 v3：`CalibUsed` 条目不进键索引（键为空），不列
    let mut keys: Vec<&str> = l
        .entries
        .iter()
        .map(|e| e.key())
        .filter(|k| !k.is_empty())
        .collect();
    keys.sort();
    println!(
        "  账本键（{}）: {}",
        keys.len(),
        keys.iter().map(|k| &k[..8]).collect::<Vec<_>>().join(" ")
    );
}

/// **基准（不花钱）**：固定观察那一趟，记下出口、层数、调用数、账本键。
/// **没有基准的真机数没法归因。**
#[test]
fn 基准_固定观察() {
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut l = Ledger::new();
    let o = jpp::run(&program, 桩(&calls), &校准(), &动作表(), &mut l).expect("跑得完");
    报("基准·固定观察", &o, &l);
    assert!(o.value_json()["合规"].is_string());
}

/// **真机重跑 3 次**：把「两内核的差」与「模型自身的抖动」分开。
/// **没有这一步，任何差异都归不了因。**
#[test]
#[ignore]
fn 真机_重跑三次() {
    #[cfg(feature = "live")]
    {
        let program = lower(&parse(程序).expect("解析")).expect("lower");
        let (mut 总usd, mut 总tok, mut 总请求) = (0.0, 0u64, 0u64);
        let mut 出口 = vec![];
        for i in 1..=3 {
            let mut c = jpp::effects::JevClient::live("jev-1.13.0", 发行画像价格(), 发行画像超时())
                .expect("密钥只从 ~/.typesafe-key 读");
            c.permute = true;
            let mut c = jpp::effects::JevPorts::new(c);
            let mut l = Ledger::new();
            let o = jpp::run(&program, c.ports(), &校准(), &动作表(), &mut l).expect("跑得完");
            let mut keys: Vec<String> = l.entries.iter().map(|e| e.key().to_string()).collect();
            keys.sort();
            println!(
                "第 {i} 次：value={} usd={:.6} tokens={} 请求={} 键={}",
                o.value_json(),
                o.cost.usd,
                o.cost.tokens,
                c.judge.calls(),
                keys.iter()
                    .map(|k| k[..8].to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            出口.push((o.value_json().to_string(), keys));
            总usd += o.cost.usd;
            总tok += o.cost.tokens;
            总请求 += c.judge.calls();
        }
        println!(
            "*** 三次合计 usd={:.6} tokens={} 请求={}",
            总usd, 总tok, 总请求
        );
        // **赌 2（最硬的一条）：账本键与模型抖动无关，三次必须逐条对得上**
        assert_eq!(
            出口[0].1, 出口[1].1,
            "**账本键三次要一样**——它与模型抖动无关，对不上就是语义差"
        );
        assert_eq!(出口[1].1, 出口[2].1);
        println!("赌 2：账本键三次逐条相同 ✓（与模型抖动无关，这一条成立）");
        let 一致 = 出口.iter().filter(|x| x.0 == 出口[0].0).count();
        println!(
            "赌 1：出口三次里 {一致}/3 相同 → {}",
            if 一致 == 3 {
                "无抖动"
            } else {
                "有抖动，差异可由抖动解释"
            }
        );
    }
}

/// **真机一次最小调用**——先看实际单价，再决定要不要跑满。
/// `cargo test -p jpp-core --features live --test e_jpp_live 真机 -- --ignored --nocapture`
#[test]
#[ignore]
fn 真机_单次() {
    #[cfg(not(feature = "live"))]
    panic!("要真机就开 --features live");
    #[cfg(feature = "live")]
    {
        let program = lower(&parse(程序).expect("解析")).expect("lower");
        let mut c = jpp::effects::JevClient::live("jev-1.13.0", 发行画像价格(), 发行画像超时())
            .expect("密钥只从 ~/.typesafe-key 读");
        // **这一跑要显式开置换**：默认关是我自己定的（「这笔钱不能默认替作者花掉」），
        // 而赌 5b 点名的正是置换路径——**不开就花了钱也验不到它**，
        // 而且 `mode_share` 空会让 `cut` 给 `Unsure(untested:permutation)`，
        // 与基准的 `pick(0)` 不一致**却与内核语义无关**。select 的调用数因此 ×2。
        c.permute = true;
        let mut c = jpp::effects::JevPorts::new(c);
        let mut l = Ledger::new();
        let o = jpp::run(&program, c.ports(), &校准(), &动作表(), &mut l).expect("跑得完");
        报("真机·第 1 次", &o, &l);
        println!(
            "  客户端实际发出的请求数 = {}（`cost.calls` 是解释器层的刷新次数；\
                  置换让 select 在**一次 judge 里**发两次，所以两个数不一样——\
                  报成本要用这个）",
            c.judge.calls()
        );
        // **第 3 问**：这个数从 `Outcome.cost.usd` 取——它在 `flush` 里由后端返回值写入
        // （`13` §5「返回即记事实」），**不是估的**。`tokens` 一起报，
        // **一个没有 token 数的 usd，别人没法重算**。
        // **赌 5c：`absorb` 在真机路径上积累无标注样本那条路没人验过。**
        // 证书门、`provenance()`、`不可达出口()` 全部只在固定观察下跑过。
        let mut store = 校准();
        for (k, sm) in &o.evidence {
            println!(
                "  证据：键={k} p={:?} label={:?} perms={} mode_share={:?} phys={}",
                sm.p, sm.label, sm.perms, sm.mode_share, sm.phys
            );
            store.absorb(k, sm.clone()).expect("折得进");
        }
        for k in ["live.noul", "live.choice"] {
            let r = store.get(k);
            println!(
                "  折进之后 {k}：status={} n={} 观察={} 来源={:?} 不可达出口={:?}",
                r.status,
                r.n,
                r.observations(),
                r.provenance(),
                r.不可达出口()
            );
        }
        println!(
            "*** 实际花费 usd={:.6} tokens={} calls={}（从 Outcome.cost 取，flush 时由后端返回值写入）",
            o.cost.usd, o.cost.tokens, o.cost.calls
        );
    }
}

/// 价格只住画像（B73）：真机测试从发行画像 `profiles/jev-1.13.0.json` 读价格。
#[cfg(feature = "live")]
fn 发行画像价格() -> Option<f64> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    jpp::effects::Profile::load(&p)
        .expect("发行画像读得到")
        .price_per_input_token()
}

/// 单次请求超时只从发行画像来（`transport.timeout_s`；地基/过程记录/工程-传输超时.md）。
fn 发行画像超时() -> Option<std::time::Duration> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
    jpp::effects::Profile::load(&p)
        .expect("发行画像读得到")
        .transport_timeout_s()
        .map(std::time::Duration::from_secs_f64)
}
