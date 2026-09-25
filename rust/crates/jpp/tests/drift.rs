//! **漂移监控接线**（`12`:396「漂移监控（无标签：读数分布偏移 + 保形覆盖跌落**告警**）」）。
//!
//! `conformal::drift` 有实现、**零调用点**——前两轮顾问各点一次。
//!
//! **三问先答**：
//! 1. **谁来调**：内核在 `cut` 那一步算，**因为数据已经在记录里**——
//!    标注样本是参照分布，运行期积累的无标注观察是近期分布。**不需要新的数据源。**
//! 2. **`underpowered` 怎么用**：**照报，但不构成停岗的依据**。
//! 3. **停岗之后怎么复岗**：**这一问不成立，因为漂移不停岗**——
//!    `12`:396 写的是**告警**。停岗仍然是人下的判断（走 `put`）。
//!    **而这一问必须先答，正是因为我今晚在它上面栽过一次**（把「拦住」改成「停岗」造出永久锁）。

use jpp::effects::{CalibStore, LiteralMode, Sample};

fn 观察(c: &mut CalibStore, key: &str, ps: &[f64], label: Option<u8>) {
    for p in ps {
        c.absorb(
            key,
            Sample {
                p: Some(*p),
                label,
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
}

/// 参照分布 = 带标注的那些；近期分布 = 运行期积累的无标注观察。**数据已经在记录里。**
#[test]
fn 参照与近期分别取标注与无标注() {
    let mut c = CalibStore::new();
    let 标注: Vec<f64> = (0..40).map(|i| 0.10 + i as f64 * 0.02).collect();
    观察(&mut c, "k", &标注, Some(1));
    // 近期读数整体右移
    let 近期: Vec<f64> = (0..40).map(|i| 0.50 + i as f64 * 0.012).collect();
    观察(&mut c, "k", &近期, None);

    let d = c.drift_of("k").expect("两侧都有样本就算得出");
    println!(
        "ks={:.3} psi={:.3} n_ref={} n_recent={} underpowered={}",
        d.ks, d.psi, d.n_ref, d.n_recent, d.underpowered
    );
    assert_eq!((d.n_ref, d.n_recent), (40, 40));
    assert!(d.ks > 0.3, "整体右移，KS 该明显：{}", d.ks);
    assert!(!d.underpowered, "两边各 40 条，不算功效不足");
}

/// **一侧为空就算不出**——不是「漂移为 0」。与 `binomial_upper` 的 `n == 0 → 1.0` 同一条：
/// **算不出来不是一个值。**
#[test]
fn 一侧没有样本时算不出而不是零() {
    let mut c = CalibStore::new();
    观察(&mut c, "k", &[0.3, 0.4, 0.5], Some(1));
    assert!(c.drift_of("k").is_none(), "只有参照没有近期 → 算不出");
    let mut d = CalibStore::new();
    观察(&mut d, "k", &[0.3, 0.4, 0.5], None);
    assert!(d.drift_of("k").is_none(), "只有近期没有参照 → 算不出");
    assert!(CalibStore::new().drift_of("没这键").is_none());
}

/// **`underpowered` 照报，但不构成停岗的依据。**
/// 当初加这一位的理由是「一次 20 条的抽样不该把一个键停岗」；
/// **而更硬的理由是今晚长出来的：复岗今天不存在，所以一次假停岗是永久的。**
#[test]
fn 功效不足时照报但不算依据() {
    let mut c = CalibStore::new();
    观察(&mut c, "k", &[0.20, 0.25, 0.30], Some(1));
    观察(&mut c, "k", &[0.70, 0.75, 0.80], None);
    let d = c.drift_of("k").expect("算得出");
    assert!(
        d.underpowered,
        "各 3 条，KS 再大也分不出移没移：ks={}",
        d.ks
    );
    assert!(!d.可停岗(), "**功效不足时它不构成停岗的依据**");

    // 样本够了、且真的移了，才算得上依据
    let mut e = CalibStore::new();
    观察(
        &mut e,
        "k",
        &(0..60).map(|i| 0.05 + i as f64 * 0.008).collect::<Vec<_>>(),
        Some(1),
    );
    观察(
        &mut e,
        "k",
        &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(),
        None,
    );
    let f = e.drift_of("k").expect("算得出");
    assert!(!f.underpowered);
    assert!(f.可停岗(), "样本够、移得明显：ks={}", f.ks);
}

/// **漂移不停岗，只告警**（`12`:396）。**这条是「复岗不存在」那个坑的正面防线**：
/// 一个只报不动的机制，造不出永久锁。
#[test]
fn 漂移不改记录状态() {
    let mut c = CalibStore::new();
    // **这里必须走 commission 而不是 put**：证书门拦的正是「带标注的证据靠手写的线上岗」。
    // 第一版我写 put，被自己那道门当场拦住——**把这一包和上一包合起来看，第 4 问当场抓到一次。**
    观察(
        &mut c,
        "k",
        &(0..60)
            .map(|i| {
                if i % 7 == 0 {
                    0.9
                } else {
                    0.05 + i as f64 * 0.008
                }
            })
            .collect::<Vec<_>>(),
        Some(1),
    );
    c.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(
        &mut c,
        "k",
        &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(),
        None,
    );
    let d = c.drift_of("k").expect("算得出");
    assert!(d.可停岗(), "这批数据够得上依据");
    assert_eq!(
        c.get("k").status,
        "上岗",
        "**但它自己不动状态**——停岗仍是人下的判断（走 put）"
    );
}

/// **`cut` 是消费方**——否则 `drift_of` 就成了第二个「有实现没调用点」的东西，
/// **而那正是这一包要修的毛病**。
#[test]
fn cut那一步会因漂移告警() {
    use jpp::effects::{FnPort, JudgeResult, Ports};
    use jpp::interp::ActionRegistry;
    use jpp::ledger::Ledger;
    use jpp::run;
    use jpp::value::Answer;
    // 步 15c：原 `impl Client for 桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
    // 改为只注册一个 judge 闭包端口。
    let 跑 = |c: &CalibStore| {
        let program = jpp::lower(
            &jpp::syntax::parse(
                r#"
budget {calls: 4, cost: 1};
handle(cut(judge(state(mat("材料")), test("行吗", "k"))), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#,
            )
            .expect("解析"),
        )
        .expect("lower");
        let mut l = Ledger::new();
        let ports = Ports::new().with(FnPort::judge("m", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }));
        run(&program, ports, c, &ActionRegistry::new(), &mut l)
            .expect("跑得完")
            .trace
            .warnings
            .clone()
    };

    // 漂了
    let mut 漂 = CalibStore::new();
    观察(
        &mut 漂,
        "k",
        &(0..60)
            .map(|i| {
                if i % 7 == 0 {
                    0.9
                } else {
                    0.05 + i as f64 * 0.008
                }
            })
            .collect::<Vec<_>>(),
        Some(1),
    );
    漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(
        &mut 漂,
        "k",
        &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(),
        None,
    );
    let w = 跑(&漂);
    assert!(
        w.iter().any(|x| x.starts_with("W-drift")),
        "**漂了要告警**：{w:?}"
    );

    // 没漂：同一个分布 → 不告警（**一条天天响的告警等于没有告警**）
    let mut 没漂 = CalibStore::new();
    let ps: Vec<f64> = (0..60)
        .map(|i| {
            if i % 7 == 0 {
                0.9
            } else {
                0.05 + i as f64 * 0.008
            }
        })
        .collect();
    观察(&mut 没漂, "k", &ps, Some(1));
    没漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    观察(&mut 没漂, "k", &ps, None);
    let w2 = 跑(&没漂);
    assert!(
        !w2.iter().any(|x| x.starts_with("W-drift")),
        "没漂不该响：{w2:?}"
    );
}

/// **`cut` 不是唯一的消费方——而告警只在它那里发。**
///
/// `12`:649 写的是「漂移监控**必备**」，管的是**这条线还成不成立**，
/// 不是「谁在用它」。而实测全集（`interp.rs` 里读 `self.calib` 的位置）有三处在 `cut` 之外：
/// `allocate`（2401）、`unsure_bound`（2422）、`delta_for`（1020，`order`/`uncertainty` 用）。
///
/// **`unsure_bound` 最重**：它读的是 `rec.unsure_rate`，而且**只认「上岗」的记录**
/// （`strength.rs:155`）——它给出的是 **J-10 的联合上界**，一条语言自己承诺的保证。
/// 读数分布移开之后那个 `unsure_rate` 不再成立，**程序拿到一个静默失效的上界，
/// 且没有任何告警**。**失败开放**，且正落在「长处」那一侧的两个构件上。
///
/// 这一条的期望值来自依据文本（`12`:649 + J-10 的上界承诺），**不是从实现抄的**。
#[test]
fn 不经cut的消费方也要报漂移() {
    use jpp::effects::{FnPort, JudgeResult, Ports};
    use jpp::interp::ActionRegistry;
    use jpp::ledger::Ledger;
    use jpp::run;
    use jpp::value::Answer;
    // 步 15c：原 `impl Client for 桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
    // 改为只注册一个 judge 闭包端口。
    let 跑 = |src: &str, c: &CalibStore| {
        let program = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
        let mut l = Ledger::new();
        let ports = Ports::new().with(FnPort::judge("m", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }));
        run(&program, ports, c, &ActionRegistry::new(), &mut l)
            .expect("跑得完")
            .trace
            .warnings
            .clone()
    };

    // 与 `cut那一步会因漂移告警` 用**同一个**已漂的 store：唯一的变量是程序里用哪个消费方。
    let mut 漂 = CalibStore::new();
    观察(
        &mut 漂,
        "k",
        &(0..60)
            .map(|i| {
                if i % 7 == 0 {
                    0.9
                } else {
                    0.05 + i as f64 * 0.008
                }
            })
            .collect::<Vec<_>>(),
        Some(1),
    );
    漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    漂.set_unsure_rate("k", 0.2).expect("设得上");
    观察(
        &mut 漂,
        "k",
        &(0..60).map(|i| 0.60 + i as f64 * 0.006).collect::<Vec<_>>(),
        None,
    );
    assert!(
        漂.drift_of("k").expect("算得出").可停岗(),
        "前提：这批数据够得上依据"
    );

    let w = 跑(
        r#"
budget {calls: 4, cost: 1};
unsure_bound(judge(state(mat("材料")), test("行吗", "k")))
"#,
        &漂,
    );
    assert!(
        w.iter().any(|x| x.starts_with("W-drift")),
        "**只调 unsure_bound、不调 cut 的程序也在用这条线**（读 unsure_rate 给 J-10 的上界），\
         漂了必须报：{w:?}"
    );
    assert_eq!(
        w.iter().filter(|x| x.starts_with("W-drift")).count(),
        1,
        "每个键每次运行只报一次：{w:?}"
    );

    // **`allocate` 同样**：它读 `lines_for`（`strength.rs:76`），用的就是这条线。
    let w2 = 跑(
        r#"
budget {calls: 4, cost: 1};
allocate(judge(state(mat("材料")), test("行吗", "k")), 1)
"#,
        &漂,
    );
    assert!(
        w2.iter().any(|x| x.starts_with("W-drift")),
        "allocate 也在用线，漂了必须报：{w2:?}"
    );

    // **反面**：没漂就不许响。**一条天天响的告警等于没有告警**——
    // 这一条拦的是「把告警接成恒真」这种修法。
    let mut 没漂 = CalibStore::new();
    let ps: Vec<f64> = (0..60)
        .map(|i| {
            if i % 7 == 0 {
                0.9
            } else {
                0.05 + i as f64 * 0.008
            }
        })
        .collect();
    观察(&mut 没漂, "k", &ps, Some(1));
    没漂.commission("k", 0.60, 0.10, "条").expect("认得动");
    没漂.set_unsure_rate("k", 0.2).expect("设得上");
    观察(&mut 没漂, "k", &ps, None);
    let w3 = 跑(
        r#"
budget {calls: 4, cost: 1};
unsure_bound(judge(state(mat("材料")), test("行吗", "k")))
"#,
        &没漂,
    );
    assert!(
        !w3.iter().any(|x| x.starts_with("W-drift")),
        "没漂不该响：{w3:?}"
    );
}
