//! **两道门把最常规的引导路径夹死了。**
//!
//! `put` 那道门拦「有 `samples`」，`commission` 那道门要「有带标注的样本」，
//! **而运行期写入口推的每一条 `Sample` 的 `label` 恒为 `None`**（真值通道还不存在）。
//! 于是：**跑程序收读数 → 人写线 → 上岗，这条路再也走不通**——
//! 而那正是 `foundation/experiments/e_cal_写校准记录.py` 做的事。
//!
//! **判据**：`put` 那道门**取错了量**。它取「有没有 `samples`」，该取的是
//! **「有没有带标注的样本」**——与「`n` 只随带标注的样本长」是同一条。
//! **一条只有无标注观察的记录，没有积累任何「关于线的证据」，它只是判过几次。**
//! 拿它去挡 `put`，是把**观察**当成了**证据**。

use jpp::conformal::Certificate;
use jpp::effects::{
    CalibStore, EffectError, FnPort, JudgeResult, LiteralMode, Ports, Provenance, Refusal, Sample,
};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 判断恒给 `p`、不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("m", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

const 程序: &str = r#"
budget {calls: 4, cost: 1};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { "act" }, ignore: fn() { "ig" },
           unsure: fn(u) { consume(u, "drop"); unsure_cause(u) }})
"#;

/// **一号的红，而且必须走真实那条路。**
/// 直接 `absorb(label: None)` 只证得了门的逻辑，**证不了「锁是那条路造出来的」**——
/// 锁是 `interp` 的 flush 每判一次就推一条无标注样本造出来的。
#[test]
fn 跑完程序之后人还能写线上岗() {
    let mut calib = CalibStore::new();
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("跑得完");

    // 这是 e_cal_写校准记录.py 的工作流：跑程序 → 收证据 → 人写线
    assert_eq!(out.evidence.len(), 1, "运行期写入口确实推了东西");
    for (k, s) in &out.evidence {
        assert_eq!(s.label, None, "**真值通道还不存在，label 恒为 None**");
        calib.absorb(k, s.clone()).expect("折得进");
    }

    // **修前这一行报错，路就断在这里**
    calib
        .put("k", 0.8, 0.2, 200, "上岗", Some(0.05))
        .expect("只有无标注观察的记录，不该挡住人写线——那是观察，不是关于线的证据");
    assert_eq!(calib.get("k").status, "上岗");
    assert_eq!(calib.get("k").observations(), 1, "观察照样留着，没被抹掉");
}

/// **`provenance()` 今天把「一条标注也没有」判成「程序积累」**，而门正按这个判。
/// 「算出来的答案填不错」的前提是**那个算法对**。
#[test]
fn 只有观察的记录不算程序积累() {
    let mut c = CalibStore::new();
    c.absorb(
        "k",
        Sample {
            p: Some(0.9),
            label: None,
            perms: 0,
            mode_share: None,
            mode: LiteralMode::default(),
            phys: "noul".into(),
            cluster: None,
            stratum: None,
        },
    )
    .unwrap();
    let rec = c.get("k");
    assert_eq!((rec.n, rec.observations(), rec.labeled()), (0, 1, 0));
    assert_eq!(
        rec.provenance(),
        Provenance::只有观察,
        "**一条标注也没有的记录不是「程序积累」**——它只是判过几次"
    );
}

/// **两种拒绝说同一句话，就是我们一直在消除的那个形状。**
/// `labeled()==0`（没有终点可报）与「有标注但认证不过」（有 `n_needed`）必须分得开。
#[test]
fn 没标注与认证不过说的不是同一句() {
    let mut 无标注 = CalibStore::new();
    无标注
        .absorb(
            "k",
            Sample {
                p: Some(0.9),
                label: None,
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    let a = 无标注
        .commission("k", 0.10, 0.10, "条")
        .expect_err("没标注就认不了");
    assert!(matches!(a, Refusal::跑不成(_)), "没有终点可报：{a:?}");
    assert_eq!(a.n_needed(), None);

    let mut 有标注 = CalibStore::new();
    for i in 0..10 {
        有标注
            .absorb(
                "k",
                Sample {
                    p: Some(0.5 + i as f64 * 0.04),
                    label: Some(i % 2),
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
    let b = 有标注
        .commission("k", 0.01, 0.10, "条")
        .expect_err("10 条撑不起 α=0.01");
    assert!(matches!(b, Refusal::认证不过(_)), "有终点可报：{b:?}");
    assert!(
        b.n_needed().is_some(),
        "**「还差多少条」只有这一种拒绝答得出**"
    );
}

/// **一号顺带：`put` 在已上岗的键上不许失败开放。**
/// 被拦住后记录保持原状 → **旧线继续放行，而你已经不能收紧它了**。
///
/// 判据是**方向**：`hi` 更高、`lo` 更低 = 带更宽 = `Unsure` 更多 = 往拒绝那边倒。
/// **这个方向不需要凭据，因为它不会让任何东西多放行。**
#[test]
fn 收紧不需要凭据放宽才需要() {
    let mut c = CalibStore::new();
    // 先用带标注的证据认证上岗
    for i in 0..30 {
        c.absorb(
            "k",
            Sample {
                p: Some(0.30 + i as f64 * 0.02),
                label: Some(if i > 6 { 1 } else { 0 }),
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
    c.commission("k", 0.45, 0.10, "条").expect("认得动");
    assert_eq!(c.get("k").status, "上岗");

    let 认证线 = c.get("k").hi;

    // **收紧**（hi 更高、lo 更低）：随时可写，不需要凭据
    c.put("k", 0.99, 0.0, 30, "上岗", Some(0.05))
        .expect("**收紧不该被拦**——拦住它就是失败开放：旧线继续放行而人改不动");
    assert_eq!(c.get("k").hi, 0.99);
    assert_eq!(c.get("k").certs.len(), 1, "收紧不抹掉证书");

    // **放宽**（hi 更低 = 多放行）：要凭据
    let e = c
        .put("k", 0.10, 0.0, 30, "上岗", Some(0.05))
        .expect_err("放宽必须要凭据");
    assert!(e.contains("commission"), "{e}");
    assert_eq!(c.get("k").hi, 0.99, "被拦住时线不动");

    // **而且不许把键弄死**：拦住之后这个键仍然认得回来
    assert_ne!(
        c.get("k").status,
        "停岗",
        "拦住换线不等于把键停掉——那会造出新的永久锁"
    );
    let _ = 认证线;
}

/// **二号：出口要说出这条线是哪来的。** `line_source` 今天只有 `题级`/`模式级`，
/// **没有 `手填`/`证书`**——而这两者的证据强度差着一整个保形认证。
#[test]
fn 出口分得出手填的线与证书的线() {
    let 跑 = |calib: &CalibStore| {
        let program = lower(
            &parse(
                r#"
budget {calls: 4, cost: 1};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
handle(e, {act: fn() { {r: "act", 源: line_source(e)} },
           ignore: fn() { {r: "ig", 源: line_source(e)} },
           unsure: fn(u) { consume(u, "drop"); {r: unsure_cause(u), 源: line_source(e)} }})
"#,
            )
            .expect("解析"),
        )
        .expect("lower");
        let mut l = Ledger::new();
        let o = run(
            &program,
            定值端口(0.9),
            calib,
            &ActionRegistry::new(),
            &mut l,
        )
        .expect("跑得完");
        (o.value_json(), o.trace.warnings.clone())
    };

    // 手填的线：n=1 也照样上岗，今天零告警
    let mut 手填 = CalibStore::new();
    手填.put("k", 0.8, 0.2, 1, "上岗", Some(0.05)).unwrap();
    let (v, w) = 跑(&手填);
    assert_eq!(v["r"], "act");
    assert_eq!(v["源"], "题级·手填", "**层级与凭据都要说**：{v}");
    assert!(
        w.iter().any(|x| x.starts_with("W-fixture-line")),
        "**强出口建在一条未经认证的手填线上，要出告警**：{w:?}"
    );

    // 证书定的线：同样是强出口，**不该告警**
    let mut 证书 = CalibStore::new();
    for i in 0..30 {
        证书
            .absorb(
                "k",
                Sample {
                    p: Some(0.30 + i as f64 * 0.02),
                    label: Some(if i > 6 { 1 } else { 0 }),
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
    证书.commission("k", 0.45, 0.10, "条").expect("认得动");
    let (v2, w2) = 跑(&证书);
    assert_eq!(v2["r"], "act");
    assert!(
        v2["源"].as_str().unwrap().starts_with("题级·证书:α="),
        "**要说出是哪张证书**：{v2}"
    );
    assert!(
        !w2.iter().any(|x| x.starts_with("W-fixture-line")),
        "认证过的不该告警：{w2:?}"
    );
}

/// **三号：两次认证不许覆盖。**
/// 实测过 α=0.60 线 0.195 → α=0.80 线 0.000（**从「过线才放行」变成「全放行」**），
/// 而被覆盖过的记录与只认证过一次的记录**逐字段相同**。
///
/// **哈希是封条，不是地址。** 它答「变了没有」，不答「这是哪一批材料、哪个 α 上的」。
/// **要防的不是篡改，是误用与合并**——没有东西被改，是**两个不同的测量占了同一个格子**。
#[test]
fn 两张不同风险目标的证书并存而不是覆盖() {
    let mut c = CalibStore::new();
    for i in 0..30 {
        c.absorb(
            "k",
            Sample {
                p: Some(0.30 + i as f64 * 0.02),
                label: Some(if i > 6 { 1 } else { 0 }),
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
    let 强 = c.commission("k", 0.45, 0.10, "条").expect("认得动");
    let 线1 = c.get("k").hi;

    // 再认一次，α 更松 → 线更低（更宽松）
    let 弱 = c
        .commission("k", 0.80, 0.10, "条")
        .expect("更松的 α 当然也认得动");
    assert!(弱.hi <= 强.hi, "α 越松线越低：{} vs {}", 弱.hi, 强.hi);

    // **两张都在，且取的是最强那张**（最小 α = 最严的风险目标 = 最保守）
    let rec = c.get("k");
    assert_eq!(rec.certs.len(), 2, "**两张证书并存**，不是后一张盖掉前一张");
    assert_eq!(rec.hi, 线1, "**更松的 α 不许把线放宽**——取最强那张");
    assert_eq!(rec.lo, 0.0);

    // 而且出口要说得出取的是哪一张
    assert!(c.选中的证书("k").map(|x| x.alpha) == Some(0.45), "取最小 α");
}

/// 同一个 α 重认一次是**更新那一格**，不是新增一格（键相同就该占同一格）。
#[test]
fn 同一个风险目标重认是更新那一格() {
    let mut c = CalibStore::new();
    for i in 0..30 {
        c.absorb(
            "k",
            Sample {
                p: Some(0.30 + i as f64 * 0.02),
                label: Some(if i > 6 { 1 } else { 0 }),
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
    c.commission("k", 0.45, 0.10, "条").unwrap();
    c.commission("k", 0.45, 0.10, "条").unwrap();
    assert_eq!(
        c.get("k").certs.len(),
        1,
        "同 (α, conf_delta, 簇单位) 是同一格"
    );
    // 但不同簇单位是不同的格：**按条取和按簇取不是同一个测量**
    let _ = c.commission("k", 0.45, 0.10, "对象段");
}

#[allow(dead_code)]
fn _用到(c: &Certificate) -> bool {
    c.is_refused()
}
