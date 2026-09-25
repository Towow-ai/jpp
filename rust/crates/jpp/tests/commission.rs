//! **上岗要一张证书。**
//!
//! 缺口的形状（`rust-core-4` 在 `conformal-proto/tests/gate.rs` 实测出来的）：
//! `absorb` 收了 73 条带标注的样本，记录停在 `待真值`——**这是设计，不是缺陷**，
//! 记录不该自己让自己上岗。但全库唯一能上岗的入口 `put` **只核 `n > 0`**，
//! 于是 `put(0.78, 0.22, 73, "上岗")` 直接通过，**而同一批数据的证书是拒绝**
//! （`best_ucb 0.319`，要 α=0.10 得 `n_needed 22` 条零错放行）。
//!
//! **把生产端补对了，门就空在下一格。**

use jpp::conformal::{Certificate, certify};
use jpp::effects::{CalibStore, LiteralMode, Refusal, Sample};
use serde_json::Value as Json;

fn 取(t: &str) -> Vec<(f64, bool, String)> {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    j[t].as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r[0].as_f64().unwrap(),
                r[1].as_i64().unwrap() == 1,
                r[2].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn 装样本(store: &mut CalibStore, key: &str, t: &str) {
    for (p, l, seg) in 取(t) {
        store
            .absorb(
                key,
                Sample {
                    p: Some(p),
                    label: Some(if l { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: t.into(),
                    cluster: Some(seg),
                    stratum: None,
                },
            )
            .expect("折得进");
    }
}

/// **红的那一半**：手写一条线让积累来的证据上岗，必须被拒，**并说出还差多少条**。
#[test]
fn 积累的证据要上岗必须过证书门() {
    let mut store = CalibStore::new();
    装样本(&mut store, "e_cal.noul", "noul");
    let rec = store.get("e_cal.noul");
    assert_eq!(
        (rec.status.as_str(), rec.n, rec.samples.len()),
        ("待真值", 73, 73)
    );

    // **修前这一行 ok，修后必须错**
    let e = store
        .put("e_cal.noul", 0.78, 0.22, 73, "上岗", Some(0.05))
        .expect_err("手写的线不该让积累的证据上岗");
    assert!(e.contains("commission"), "错误要指出正门在哪：{e}");

    // 正门：跑证书。这批数据上 α=0.10 认证不过
    let 拒 = store
        .commission("e_cal.noul", 0.10, 0.10, "条")
        .expect_err("α=0.10 在这批数据上过不了");
    match &拒 {
        Refusal::认证不过(Certificate::Refused {
            best_ucb, n_needed, ..
        }) => {
            println!("证书拒绝：best_ucb={best_ucb:.3} n_needed={n_needed}");
            assert_eq!(
                *n_needed, 22,
                "**要能说出还差多少条**——它是唯一告诉作者「这条路有终点」的东西"
            );
            assert!((*best_ucb - 0.319).abs() < 0.01, "与原型同值：{best_ucb}");
        }
        other => panic!("该拒绝：{other:?}"),
    }
    assert_eq!(拒.n_needed(), Some(22));
    // **「跑不成」不编一个 n_needed 出来**：α 越界与「样本不够」是两回事，
    // 报一个假的终点，作者会照着它去凑样本，而问题根本不在样本数上。
    assert_eq!(
        store
            .commission("e_cal.noul", 0.0, 0.10, "条")
            .expect_err("α=0 越界")
            .n_needed(),
        None
    );
    // 拒绝不改记录：留在待真值
    assert_eq!(
        store.get("e_cal.noul").status,
        "待真值",
        "认证不过就留在待真值——**不阻塞，但也不放行**"
    );
}

/// **绿的那一半**：α 放松到这批数据认得动的程度，证书通过，记录上岗，**证书存进记录**。
#[test]
fn 过了证书就上岗且证书存进记录() {
    let mut store = CalibStore::new();
    装样本(&mut store, "k", "noul");
    let cert = store
        .commission("k", 0.45, 0.10, "条")
        .expect("α=0.45 认得动");
    let rec = store.get("k");
    assert_eq!(rec.status, "上岗");
    assert_eq!(rec.hi, cert.hi, "线由证书定，不由调用方写");

    assert_eq!(
        rec.lo, 0.0,
        "**证书只管放行那一侧**：弃权那一侧没有凭据，就不放行任何 Ignore"
    );
    let c = rec.选中的证书().expect("**证书要存进记录**");
    assert_eq!((c.alpha, c.conf_delta), (0.45, 0.10));
    assert_eq!(c.cluster_unit, "条");
    assert!(c.ucb <= c.alpha, "上界要真的 ≤ α：{} vs {}", c.ucb, c.alpha);
    assert!(c.n_accepted > 0, "空放行区不算解");
    println!(
        "认证线 hi={:.3} 放行 {}/73 错 {} 上界 {:.3}",
        rec.hi, c.n_accepted, c.n_errors, c.ucb
    );
}

/// **证书要跟着进 `calib_hash`。**
/// 两条记录可以有**一模一样的 `hi`/`lo`**，背后却是两张不同的证书——
/// 一张 α=0.10 一张 α=0.40。**只哈希 `hi/lo/n/status`，就覆盖了线、没覆盖线的凭据。**
/// 与「兜底档案的 `hash` 必须是 `None`」同形，深了一层：那条说「用了兜底」要留痕，
/// 这条说「**凭什么**」要留痕。
#[test]
fn 线相同而证书不同时哈希要不同() {
    use jpp::effects::calib_hash;
    let mut 宽 = CalibStore::new();
    装样本(&mut 宽, "k", "noul");
    宽.commission("k", 0.45, 0.10, "条").expect("认得动");
    let 线 = (宽.get("k").hi, 宽.get("k").lo);

    // 另一份库：同样的线，但**是宿主手填的，没有证书**
    let mut 手填 = CalibStore::new();
    手填
        .put("k", 线.0, 线.1, 73, "上岗", Some(0.05))
        .expect("宿主手填这条路仍然通");
    assert_eq!((手填.get("k").hi, 手填.get("k").lo), 线, "线一模一样");
    assert_ne!(
        calib_hash(&宽),
        calib_hash(&手填),
        "**线相同、凭据不同，哈希必须不同**"
    );
}

/// **`cluster_unit` 是声明出来的，不是推断出来的。**
/// 样本没有簇 id 却声明按对象段分簇 → **是错，不是降级**。
/// 降级会造出一张写着「按条核过」的证书，而真相是**没人说过簇是什么**。
#[test]
fn 簇的单位必须声明且不许推断() {
    let mut 有簇 = CalibStore::new();
    装样本(&mut 有簇, "k", "noul");
    // 有簇 id 时按对象段是允许的；这批数据上簇级更严，认不过也是诚实结果
    let r = 有簇.commission("k", 0.45, 0.10, "对象段");
    assert!(
        !matches!(r, Err(Refusal::跑不成(_))),
        "有簇 id 时不该报「跑不成」：{r:?}"
    );

    // 没有簇 id 的样本，声明按对象段 → 报错
    let mut 无簇 = CalibStore::new();
    for (p, l, _) in 取("noul") {
        无簇
            .absorb(
                "k",
                Sample {
                    p: Some(p),
                    label: Some(if l { 1 } else { 0 }),
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
    match 无簇.commission("k", 0.45, 0.10, "对象段") {
        Err(Refusal::跑不成(why)) => assert!(why.contains("簇"), "{why}"),
        other => panic!("没有簇 id 却声明按对象段，该是「跑不成」而不是 {other:?}"),
    }
}

/// **簇级比条级更差**（原型 `ecal.rs` 的实测）：可交换性**不是被时间打破的，是被材料复用打破的**。
/// 同一段落的多条读数不是多次独立观察。**不记按什么分的簇，`n` 这个数本身就没有意义。**
#[test]
fn 簇级比条级严() {
    // **用 score 那一列**：原型实测 noul 条级本来就无解（false / 0），
    // 拿它证明不了「簇级更差」——**一个两边都是 false 的对照臂证不了任何事**。
    let 条: Vec<(f64, bool)> = 取("score").into_iter().map(|(p, l, _)| (p, l)).collect();
    let 条级有解 = !certify(&条, 0.30, 0.10).is_refused();
    let mut 簇级有解 = 0;
    for seed in 0..200u64 {
        if !certify(
            &jpp::conformal::cluster_subsample(&取("score"), seed),
            0.30,
            0.10,
        )
        .is_refused()
        {
            簇级有解 += 1;
        }
    }
    println!("score α=0.30：条级(n=55) 有解={条级有解}；簇级 200 次重采样有解 {簇级有解}/200");
    assert!(条级有解, "条级有解");
    assert_eq!(
        簇级有解, 0,
        "**簇级一次也不成立**——55 条读数不是 55 次独立观察"
    );
}

/// **反面：宿主手填那条路仍然通**，且 `provenance()` 报「宿主手填」。
/// I4：线来自程序之外，**宿主为它负责**；责任归属可查，就不必再加一道门。
#[test]
fn 宿主手填不需要证书() {
    let mut c = CalibStore::new();
    c.put("手填", 0.8, 0.2, 200, "上岗", Some(0.05))
        .expect("**这条路不该被证书门挡住**");
    assert_eq!(c.get("手填").status, "上岗");
    assert_eq!(
        c.get("手填").provenance(),
        jpp::effects::Provenance::宿主手填
    );
    assert!(
        c.get("手填").certs.is_empty(),
        "手填的线没有证书——而这一点本身是可查的"
    );
}
