//! 长处构件**有没有用**：同一个程序、同一组固定观察，把复核名额按不确定性分配，比随机分配错得更少吗。
//!
//! 「写得出、测试绿」不算验收——那只证明代码在，不证明它有用。所以这里是一次实测：三条臂跑同一份
//! 数据，比错误率。全部固定观察，零调用、零花费。
//!
//! **预注册的预测（写在跑之前，结果是对它的检验，不是事后找补）**：
//!
//! 1. `错误率·按不确定性复核` == 0.0——十二段里恰好四段落在决定带内，`allocate` 应当把四个名额
//!    全给它们（下标 2、4、6、8），复核之后这四段不再算错。
//! 2. `错误率·随机复核` ≈ 0.222——随机抽 4/12，期望盖住 4×4/12 ≈ 1.33 段模棱两可的，
//!    剩下 ≈ 2.67 段仍是 unsure 记错，2.67/12 ≈ 0.222。
//! 3. `错误率·不复核` == 4/12 ≈ 0.333——四段 unsure 全部记错。
//! 4. 严格序：**按不确定性 < 随机 ≤ 不复核**。
//!
//! 一个随机种子只是一个样本，所以随机臂另外跑 200 个种子取平均；单种子的数也一并报出来。

use std::cell::RefCell;
use std::collections::HashMap;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

const CALIB: &str = "doc.含数字";

/// 十二段：四段明显含数字事实、四段明显不含、四段模棱两可。`p` 由固定规则给，`真值` 是标注。
/// 与 Python 侧 `foundation/jv/examples/strength.py` 的 `段落` 同一份数据。
const 段落: &[(&str, f64, i32)] = &[
    ("clear-yes 2025 年营收 12.4 亿元，同比增长 18%。", 0.93, 1),
    ("clear-no 我们相信产品会越来越好。", 0.05, 0),
    ("fuzzy-a 客户数量较上季度有所增加。", 0.52, 0),
    ("clear-yes 服务器 CPU 峰值 87%，持续 14 分钟。", 0.91, 1),
    ("fuzzy-b 交付周期缩短了大约一半。", 0.58, 1),
    ("clear-no 团队士气高涨。", 0.04, 0),
    ("fuzzy-c 多数用户在首周内完成了注册。", 0.47, 1),
    ("clear-yes 版本 3.2 修复了 27 个缺陷。", 0.95, 1),
    ("fuzzy-d 成本略有上升。", 0.43, 0),
    ("clear-no 这是一个令人兴奋的方向。", 0.06, 0),
    ("clear-yes 平均响应时间 230 毫秒。", 0.90, 1),
    ("clear-no 感谢所有参与者。", 0.03, 0),
];

/// 按状态里 `on` 槽的文本给固定的 p：不是模型，缺记录即错（与 Python 的 FakeClient 同纪律）。
/// 步 15c：原 `impl Client` 的桩改为三个闭包端口；`calls` 用 `RefCell` 借给判断端口计数。
fn 规则端口(table: HashMap<String, f64>, calls: &RefCell<u64>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |state, questions| {
            let on = state
                .on
                .first()
                .and_then(|m| m.content.as_str())
                .unwrap_or("")
                .to_string();
            let p = *table
                .get(&on)
                .ok_or_else(|| EffectError(format!("固定规则里没有这一段：{on}")))?;
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: questions.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |p, _c, _n, _r| {
            Err(EffectError(format!("这条程序不该 gen：{p}")))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("这条程序不该 ask".into()))
        }))
}

/// 源码就是算法本身：判断、算上界、分配复核名额、过线、记账，全在 J++ 里。
const 源码: &str = r#"
budget {calls: 20, cost: 1, depth: 8, escalate: 4};
fn 读一段(t) -> Record !{judge} { judge(state(mat(t)), test("这段文字包含可核对的数字事实吗？", "doc.含数字")) }
let 读数们 = map(段落, 读一段);
let 上界 = unsure_bound(读数们);
let 复核 = allocate(读数们, 4);
let 出口 = map(读数们, fn(r) -> Exit { cut(r) });
let 种类 = map(出口, fn(e) { exit_kind(e) });
consume(出口, "drop");
{复核: 复核, 上界: 上界, 出口: map(出口, fn(e) { exit_kind(e) })}
"#;

fn 跑一次() -> (Vec<usize>, Vec<String>, Vec<(String, f64)>) {
    let texts: Vec<String> = 段落.iter().map(|(t, _, _)| format!("\"{t}\"")).collect();
    let source = 源码.replace("段落", &format!("[{}]", texts.join(", ")));
    let parsed = parse(&source)
        .unwrap_or_else(|d| panic!("解析失败：{}", d.render("strength.jpp", &source)));
    let program = lower(&parsed)
        .unwrap_or_else(|d| panic!("lower 失败：{}", d[0].render("strength.jpp", &source)));

    let mut calib = CalibStore::new();
    calib
        .put(CALIB, 0.65, 0.35, 100, "上岗", Some(0.05))
        .expect("校准记录合法");
    calib
        .set_unsure_rate(CALIB, 0.2)
        .expect("标注集上实测的 unsure 率");
    let calls = RefCell::new(0u64);
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        规则端口(
            段落.iter().map(|(t, p, _)| (t.to_string(), *p)).collect(),
            &calls,
        ),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    let Some(Value::Record(fields)) = &outcome.value else {
        panic!("返回的该是记录，拿到 {:?}", outcome.value)
    };
    let get = |k: &str| {
        fields
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("返回里缺 {k}"))
    };
    // allocate 现在返回记录：`picked` 是榜，`算不出` 是排不了序的那些（对程序可见）
    let Value::Record(rep) = get("复核") else {
        panic!("复核该是记录")
    };
    let Value::List(picked) = rep
        .iter()
        .find(|(k, _)| k == "picked")
        .expect("有 picked")
        .1
        .clone()
    else {
        panic!("picked 该是下标表")
    };
    let picked: Vec<usize> = picked
        .iter()
        .map(|v| {
            if let Value::Int(i, _) = v {
                *i as usize
            } else {
                panic!("下标该是整数")
            }
        })
        .collect();
    let Value::List(kinds) = get("出口") else {
        panic!("出口该是列表")
    };
    let kinds: Vec<String> = kinds
        .iter()
        .map(|v| {
            if let Value::Text(t, _) = v {
                t.to_string()
            } else {
                panic!("出口种类该是文本")
            }
        })
        .collect();
    let Value::Record(b) = get("上界") else {
        panic!("上界该是记录")
    };
    let bound: Vec<(String, f64)> = b
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                match v {
                    Value::Float(f, _) => *f,
                    Value::Int(i, _) => *i as f64,
                    // **`independent_any` 故意不是数**：它只作参考值，J++ 那侧也不许拿它比大小。
                    // Rust 侧的 `仅供参考` 类型闸以前只拦得住 Rust 调用者，
                    // 而这门语言唯一的用户拿到的是裸浮点——现在两侧同一条纪律。
                    Value::Text(t, _) if k == "independent_any" => {
                        assert!(t.contains("仅供参考"), "取出来时那句话要跟着：{t}");
                        f64::NAN
                    }
                    other => panic!("上界里的 {k} 该是数，拿到 {}", other.type_name()),
                },
            )
        })
        .collect();
    (picked, kinds, bound)
}

/// 假真值下的错误率：被复核的按人答（不算错）；其余 act/ignore 与真值不符、或还是 unsure，都算错。
fn 错误率(kinds: &[String], reviewed: &[usize]) -> f64 {
    let mut n = 0;
    for (i, k) in kinds.iter().enumerate() {
        if reviewed.contains(&i) {
            continue;
        }
        let pred = match k.as_str() {
            "act" => Some(1),
            "ignore" => Some(0),
            _ => None,
        };
        if pred != Some(段落[i].2) {
            n += 1;
        }
    }
    (n as f64 / kinds.len() as f64 * 1000.0).round() / 1000.0
}

/// 确定性的随机抽样（不引第三方 crate）：线性同余，取前 k 个不重复下标
fn 随机抽(seed: u64, n: usize, k: usize) -> Vec<usize> {
    let mut s = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut pool: Vec<usize> = (0..n).collect();
    let mut out = vec![];
    for _ in 0..k.min(n) {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (s >> 33) as usize % pool.len();
        out.push(pool.remove(j));
    }
    out
}

#[test]
fn 按不确定性分配复核名额比随机分配错得少() {
    let (picked, kinds, bound) = 跑一次();

    // 预测 1：四个名额全给落在决定带内的那四段
    let mut sorted = picked.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![2, 4, 6, 8],
        "最不确定的四段正是四段 fuzzy，实际选了 {picked:?}；出口 {kinds:?}"
    );

    let e_alloc = 错误率(&kinds, &picked);
    let e_none = 错误率(&kinds, &[]);
    let e_rand0 = 错误率(&kinds, &随机抽(0, kinds.len(), picked.len()));
    let mut acc = 0.0;
    for seed in 0..200u64 {
        acc += 错误率(&kinds, &随机抽(seed, kinds.len(), picked.len()));
    }
    let e_rand_avg = (acc / 200.0 * 1000.0).round() / 1000.0;

    println!(
        "错误率·按不确定性={e_alloc} 随机(种子0)={e_rand0} 随机(200 种子平均)={e_rand_avg} 不复核={e_none}"
    );
    println!("J-10 上界：{bound:?}");

    // 预测 1、3、4
    assert_eq!(e_alloc, 0.0, "复核了全部四段 unsure，不该再有错");
    assert_eq!(e_none, 0.333, "不复核时四段 unsure 全记错 = 4/12");
    assert!(
        e_alloc < e_rand_avg,
        "按不确定性分配该比随机好：{e_alloc} vs {e_rand_avg}"
    );
    assert!(
        e_rand_avg <= e_none,
        "随机复核不该比不复核更差：{e_rand_avg} vs {e_none}"
    );
    // 预测 2：随机臂的期望 ≈ 2.67/12 ≈ 0.222
    assert!(
        (e_rand_avg - 0.222).abs() < 0.03,
        "随机臂 200 种子平均应当落在 0.222 附近，实际 {e_rand_avg}"
    );

    // J-10：十二题各 0.2，联合界 2.4；独立估计只作参考
    let of = |k: &str| bound.iter().find(|(n, _)| n == k).map(|(_, v)| *v).unwrap();
    assert_eq!(of("n"), 12.0);
    assert_eq!(of("union_bound"), 2.4);
    assert_eq!(of("n_unknown"), 0.0);
}

/// `allocate` / `unsure_bound` 不花钱：不发调用、不进账本、不动预算。
/// 十二段 = 十二次判断，一次都不多。
#[test]
fn 长处构件不花钱() {
    let texts: Vec<String> = 段落.iter().map(|(t, _, _)| format!("\"{t}\"")).collect();
    let source = 源码.replace("段落", &format!("[{}]", texts.join(", ")));
    let program = lower(&parse(&source).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib
        .put(CALIB, 0.65, 0.35, 100, "上岗", Some(0.05))
        .unwrap();
    calib.set_unsure_rate(CALIB, 0.2).unwrap();
    let calls = RefCell::new(0u64);
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        规则端口(
            段落.iter().map(|(t, p, _)| (t.to_string(), *p)).collect(),
            &calls,
        ),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("跑完");
    assert_eq!(
        outcome.cost.calls, 12,
        "十二段十二次判断；allocate 与 unsure_bound 一次都不该加"
    );
    assert_eq!(outcome.cost.usd, 0.0, "固定观察零花费");
    assert!(
        outcome.pending.is_empty(),
        "不该挂起：{:?}",
        outcome.pending
    );
}

/// J-01：这两个构件只接受读数。出口没有「离线多远」这个量，传进来就是错。
#[test]
fn 只接受读数() {
    let source = r#"
budget {calls: 2, cost: 1, depth: 8};
let r = judge(state(mat("a")), test("行吗", "k"));
let e = cut(r);
consume(e, "drop");
allocate([e], 1)
"#;
    let program = lower(&parse(source).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0u64);
    let mut ledger = Ledger::new();
    let e = run(
        &program,
        规则端口([("a".to_string(), 0.9)].into_iter().collect(), &calls),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect_err("出口不是读数，该报错");
    let text = e.render();
    assert!(text.contains("J-01"), "该是 J-01：{text}");
}

/// `12` §10 G2 那一行：「`judge` 一次调用 = **一状态多题**」。
///
/// 代码路径一直是成立的（判断端口收到 `state, &[&Question]`、一次 `charge`、一次 `calls += 1`），
/// 但**仓库里没有一个测试钉住它**——所有 `judge(` 调用点都只传单题。不钉住，这一行在新内核上
/// 就只是声称。P5「一次调用 = 一状态多题」是融合省钱的全部意义所在。
#[test]
fn 一状态多题是一次调用() {
    let src = r#"
budget {calls: 3, cost: 1, depth: 8};
let m = mat("一段文字");
let 读数们 = judge(state(m), [test("含数字吗", "doc.含数字"), test("含日期吗", "doc.含数字"), test("含人名吗", "doc.含数字")]);
let 出口 = map(读数们, fn(r) -> Exit { cut(r) });
let 种类 = map(出口, fn(e) { exit_kind(e) });
consume(出口, "drop");
{题数: len(种类)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib
        .put(CALIB, 0.65, 0.35, 100, "上岗", Some(0.05))
        .unwrap();
    let calls = RefCell::new(0u64);
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        规则端口(
            [("一段文字".to_string(), 0.9)].into_iter().collect(),
            &calls,
        ),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(
        outcome.value_json(),
        serde_json::json!({"题数": 3}),
        "三道题三条读数"
    );
    assert_eq!(
        outcome.cost.calls, 1,
        "三道题问的是同一个状态，应当**一次调用**问完（12 §10 G2 / P5）"
    );
    assert_eq!(*calls.borrow(), 1, "客户端那边也只该被叫一次");
    assert_eq!(
        outcome.trace.count("judge", false),
        3,
        "账本按题记三条，调用只有一次——记账粒度是题，调用粒度是状态"
    );
}
