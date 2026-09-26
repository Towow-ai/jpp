//! **账本头记 `CalibStore` 的哈希**（J-18 / `12` §2.10）。
//!
//! 洞的形状：**出口 = f(读数, 线)。读数进了账本，线没有。**
//! 账本头有 `profile_hash`（**档案里的缺省线**）和行为子集摘要，**但没有按键的那些线**，
//! 而 `cut` 用的正是按键的线。**同一份账本换一批校准记录重放，读数一样、出口可以不一样，
//! 而账本上看不出。**
//!
//! 运行期写入口那一包把它从「理论上可变」顶成「运行中就会变」：`absorb` 之后校准记录会长。
//! **J-18 那句「同程序重放逐字节相同」，对出口本来是一句空话。**

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports, calib_hash};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 步 15c：原 `impl Client for 定值`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
/// 改为一个 judge 闭包端口，恒答 0.7。
fn 定值端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", |_s: &State, qs: &[&Question]| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.7)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    }))
}

const 程序: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" },
           unsure: fn(u) { consume(u, "drop"); unsure_cause(u) }})
"#;

fn 跑(calib: &CalibStore, ledger: &mut Ledger) -> jpp::Outcome {
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    run(&program, 定值端口(), calib, &ActionRegistry::new(), ledger).expect("跑得完")
}

/// **这一包的红**：同一份账本、只改一条校准记录、重放——**读数一样、出口不一样，
/// 而账本头一声不吭**。
#[test]
fn 换线重放要报w_header() {
    let mut 松 = CalibStore::new();
    松.put("k", 0.60, 0.30, 50, "上岗", Some(0.05))
        .expect("写得进");
    let mut ledger = Ledger::new();
    let 首 = 跑(&松, &mut ledger);
    assert_eq!(
        首.value_json(),
        serde_json::json!("act"),
        "0.7 ≥ 0.60 → act"
    );
    assert_eq!(首.cost.calls, 1);

    // 换一条更严的线，**带着同一份账本**重放
    let mut 严 = CalibStore::new();
    严.put("k", 0.80, 0.20, 50, "上岗", Some(0.05))
        .expect("写得进");
    let 再 = 跑(&严, &mut ledger);

    // 读数确实是从账本里读回来的——**不是重跑了一遍**
    assert_eq!(再.cost.calls, 0, "没有新调用");
    assert!(再.cost.replayed > 0, "走的是重放路径");
    // 而出口变了
    assert_eq!(
        再.value_json(),
        serde_json::json!("band"),
        "0.7 落进 0.80/0.20 的带内"
    );

    // **修前这里一条告警都没有：静默。**
    // 断在 `trace` 而不是 `ledger.header_warning`，因为 `run()` 会把它 `take()` 进 trace——
    // **要断的是「跑的人看得见」，不是「内部字段设过」**。
    assert!(
        再.trace.warnings.iter().any(|w| w.starts_with("W-header")),
        "换线重放必须报 W-header，实得 {:?}",
        再.trace.warnings
    );
}

/// **不换线就不报**：同一批记录重放，头一致，不该造假警报。
/// 假警报和漏警报同样是错——**一个天天报 W-header 的账本等于没有 W-header**。
#[test]
fn 线没变就不报() {
    let mut calib = CalibStore::new();
    calib
        .put("k", 0.60, 0.30, 50, "上岗", Some(0.05))
        .expect("写得进");
    let mut ledger = Ledger::new();
    跑(&calib, &mut ledger);
    let 二 = 跑(&calib, &mut ledger);
    assert!(
        !二.trace.warnings.iter().any(|w| w.starts_with("W-header")),
        "同一批记录重放不该报：{:?}",
        二.trace.warnings
    );
}

/// **哈希用排除法，不用列举法。** 列举法会在有人加新字段时**说出一个假的「相同」**——
/// `behavior_hash` 就是列举法，它自己的注释承认了这个失效方式。
#[test]
fn 每个承载字段变了哈希都要动() {
    let 基 = |hi: f64, lo: f64, n: u64, st: &str| {
        let mut c = CalibStore::new();
        c.put("k", hi, lo, n, st, None).expect("写得进");
        calib_hash(&c)
    };
    let 原 = 基(0.6, 0.3, 50, "上岗");
    assert_ne!(原, 基(0.7, 0.3, 50, "上岗"), "hi 变了");
    assert_ne!(原, 基(0.6, 0.2, 50, "上岗"), "lo 变了");
    assert_ne!(原, 基(0.6, 0.3, 51, "上岗"), "n 变了");
    assert_ne!(原, 基(0.6, 0.3, 50, "停岗"), "status 变了");

    // 键名本身是身份的一部分：同样的线挂在别的键上是另一份库
    let mut 别的键 = CalibStore::new();
    别的键
        .put("k2", 0.6, 0.3, 50, "上岗", Some(0.05))
        .expect("写得进");
    assert_ne!(原, calib_hash(&别的键));

    // delta / unsure_rate / set_id 也承载：它们分别进 delta_for、J-10、J-16
    let mut c = CalibStore::new();
    c.put("k", 0.6, 0.3, 50, "上岗", Some(0.05))
        .expect("写得进");
    let a = calib_hash(&c);
    c.set_delta("k", 0.02).expect("写得进");
    let b = calib_hash(&c);
    assert_ne!(a, b, "delta 进 delta_for");
    c.set_unsure_rate("k", 0.1).expect("写得进");
    assert_ne!(b, calib_hash(&c), "unsure_rate 进 J-10");
}

/// **空库也有哈希，不是 `None`。**
/// `None` 在头里只该有一个意思：**这份账本早于这个字段**。让空库也占 `None`，
/// 两种情形就在账本上分不开——与「兜底档案的 `hash` 必须是 `None`」是同一条，方向相反。
#[test]
fn 空库有自己的哈希() {
    let 空 = calib_hash(&CalibStore::new());
    assert!(!空.is_empty());
    let mut 有 = CalibStore::new();
    有.put("k", 0.6, 0.3, 50, "上岗", Some(0.05))
        .expect("写得进");
    assert_ne!(
        空,
        calib_hash(&有),
        "「一条记录都没有」与「有一条记录」是两种状态"
    );
}

/// 哈希对记录的插入顺序不敏感——否则同一批线会因为写入顺序不同而报假 W-header。
#[test]
fn 哈希不随写入顺序变() {
    let mut a = CalibStore::new();
    a.put("甲", 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    a.put("乙", 0.7, 0.2, 60, "上岗", Some(0.05)).unwrap();
    let mut b = CalibStore::new();
    b.put("乙", 0.7, 0.2, 60, "上岗", Some(0.05)).unwrap();
    b.put("甲", 0.6, 0.3, 50, "上岗", Some(0.05)).unwrap();
    assert_eq!(calib_hash(&a), calib_hash(&b));
}
