//! **标签是怎么选的，要进那个数的地址，不是它的注释。**
//!
//! 今晚最重的测量发现：E-CAL 的校准集**在定义上就是「两个模型都同意」的那个子集**
//! ——202 条真值里两模型一致 **202/202 = 100%**，而未标注的 95 条里只有 **77/95 = 81%**。
//! 「是否同意」与「对不对」**已实测相关**（noul +0.527、choice +0.418），
//! **所以这个集合上算出来的每个数都同向乐观有偏**——包括已公开的 ECE 0.057、AUC 0.748，
//! **以及今后每一张证书**。
//!
//! 而这件事今天横跨数据/代码/流程**写在六处**，那个数照样丢掉一半
//! （限定留存率 ECE 47%、AUC 46%）。**加第七处会得到第七个 50%。**
//!
//! 判据与 `cluster_unit` 同源：
//! **不记「簇是怎么分的」，`n` 这个数就没有意义；不记「标签是怎么选的」，那张证书也没有意义。**

use jpp::effects::{CalibStore, LabelSource, LiteralMode, Sample, calib_hash};

fn 装(store: &mut CalibStore, key: &str) {
    for i in 0..30 {
        store
            .absorb(
                key,
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
            .expect("折得进");
    }
}

/// **总控给的验收。**
///
/// 构造上要留神：`label_source` 进了证书地址（和 `cost` 一样），所以**同一个库里认两次
/// 会得到两格而不是一格**——那样比的就不是「同一张证书换了标签来源」了。
/// 所以用**两个各只有一张证书的库**，逐字段相同、只差 `label_source`。
/// **它必须进地址**：不进就是后一张盖掉前一张，那正是刚修好的覆盖那个坑换了一根轴重演。
#[test]
fn 只差标签来源的两条记录哈希必须不同() {
    let 建 = |src: LabelSource| {
        let mut c = CalibStore::new();
        装(&mut c, "k");
        c.set_label_source("k", src).expect("写得进");
        c.commission("k", 0.45, 0.10, "条").expect("认得动");
        c
    };
    let 全体 = 建(LabelSource::全体);
    let 子集 = 建(LabelSource::选择子集 {
        判据: "两个模型都同意".into(),
        与对错相关: Some(0.527),
    });

    // 除了标签来源，线、n、状态、簇单位逐字段相同
    let (a, b) = (全体.get("k"), 子集.get("k"));
    assert_eq!(
        (a.hi, a.lo, a.n, a.status.clone()),
        (b.hi, b.lo, b.n, b.status.clone())
    );
    assert_eq!(
        a.选中的证书().unwrap().cluster_unit,
        b.选中的证书().unwrap().cluster_unit
    );

    // **而哈希必须不同**
    assert_ne!(
        calib_hash(&全体),
        calib_hash(&子集),
        "**一张在「两模型都同意」的子集上认证出来的证书，和一张在全体上认证出来的，不是同一个测量**"
    );
}

/// **`全体` 是一个要有人明确声明的断言，不是默认。**
/// 缺失值不许默认成「全体」——**那是替不确定说了确定，而且方向是乐观的那一侧**。
#[test]
fn 缺省是未声明不是全体() {
    let mut c = CalibStore::new();
    装(&mut c, "k");
    assert_eq!(
        c.get("k").label_source,
        LabelSource::未声明,
        "**没人说过就是没人说过**"
    );
    let cert = c.commission("k", 0.45, 0.10, "条").expect("认得动");
    assert_eq!(
        cert.label_source,
        LabelSource::未声明,
        "证书冻结它认证时的那个声明"
    );
}

/// **`选择子集` 里的 `与对错相关: None` 是「没测」，不是「不相关」。**
/// 与 `Tri::未测` 同一条：**一个没测过相关性的选择子集，比一个声明了 0.0 的更可疑，不是更不可疑。**
#[test]
fn 相关性未测比声明零更可疑() {
    let 未测 = LabelSource::选择子集 {
        判据: "两模型同意".into(),
        与对错相关: None,
    };
    let 测了 = LabelSource::选择子集 {
        判据: "两模型同意".into(),
        与对错相关: Some(0.0),
    };
    assert_ne!(未测, 测了, "**「没测」与「测了是 0」是两种状态**");
    assert!(未测.可疑(), "没测相关性的选择子集是可疑的");
    assert!(!测了.可疑(), "声明了 0.0 相关的不可疑");
    assert!(!LabelSource::全体.可疑());
    assert!(LabelSource::未声明.可疑());
}

/// **`未声明` 的证书给出强出口时要留痕，而且这条告警的正确稳态是「消失」。**
///
/// 与 `bounded_side` **不告警**那条的分界：**`bounded_side` 今天只有一个取值，
/// 一条在每个已认证键上都响的告警承载零信息**；而 `label_source` 区分得开记录，
/// **所以它响的时候是在说一件别的记录不成立的事**。
#[test]
fn 未声明标签来源的证书给强出口时留痕() {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::ledger::Ledger;
    use jpp::value::Answer;
    use jpp::{ActionRegistry, run};
    use std::cell::RefCell;
    /// 判断恒给 0.99，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
    fn 桩端口<'a>(calls: &'a RefCell<u64>) -> Ports<'a> {
        Ports::new()
            .with(FnPort::judge("m", move |_s, qs| {
                *calls.borrow_mut() += 1;
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate("m", |_p, _c, _n, _r| {
                Err(EffectError("x".into()))
            }))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
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
        let calls = RefCell::new(0);
        run(&program, 桩端口(&calls), c, &ActionRegistry::new(), &mut l)
            .expect("跑得完")
            .trace
            .warnings
            .clone()
    };

    let mut 未声明 = CalibStore::new();
    装(&mut 未声明, "k");
    未声明.commission("k", 0.45, 0.10, "条").expect("认得动");
    let w = 跑(&未声明);
    assert!(
        w.iter().any(|x| x.starts_with("W-label-source")),
        "**没人说过标签怎么选的，强出口要留痕**：{w:?}"
    );

    let mut 声明了 = CalibStore::new();
    装(&mut 声明了, "k");
    声明了.set_label_source("k", LabelSource::全体).unwrap();
    声明了.commission("k", 0.45, 0.10, "条").expect("认得动");
    let w2 = 跑(&声明了);
    assert!(
        !w2.iter().any(|x| x.starts_with("W-label-source")),
        "**声明过就不该再响**——这条告警的正确稳态是消失：{w2:?}"
    );
}

/// **证书地址不许被格式化精度或自由文本撞掉。**
///
/// 两处（总控点的）：`{:.4}` 让第五位小数不同的两张证书同址——而 `certs` 是
/// `BTreeMap<addr, Cert>`，**后写的静默覆盖先写的**；以及地址里嵌的自由文本
/// （`cluster_unit`、`判据`）**含 `\u{1f}` 就撞地址**。
///
/// **今天都不可达**（α 全是字面量、判据是中文散文），**但两者都是公开 API 的入参**，
/// 「调用方传一个算出来的 α」「判据里出现一个控制字符」都是完全正常的事。
/// **一行换掉两颗按取值决定生死的雷。**
#[test]
fn 证书地址不被精度或分隔符撞掉() {
    use jpp::effects::Cert;
    let 造 = |alpha: f64, cu: &str, 判据: &str| Cert {
        alpha,
        conf_delta: 0.10,
        hi: 0.8,
        n_accepted: 10,
        n_errors: 0,
        ucb: 0.05,
        cluster_unit: cu.into(),
        resample: None,
        cost: None,
        bounded_side: String::new(),
        label_fp: "abcd".into(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: LabelSource::选择子集 {
            判据: 判据.into(),
            与对错相关: Some(0.5),
        },
    };
    // 第五位小数不同 → **地址必须不同**
    assert_ne!(
        造(0.10000, "条", "甲").addr(),
        造(0.10001, "条", "甲").addr(),
        "**`{{:.4}}` 会让这两张同址，后写的静默覆盖先写的**"
    );
    // 自由文本里塞分隔符 → **不许撞进别人的地址**
    let a = 造(0.1, "条", "甲");
    let b = 造(0.1, "条", &format!("甲\u{1f}fp=abcd"));
    assert_ne!(a.addr(), b.addr(), "判据里的 \\u{{1f}} 不许改变地址的分段");
    let c = 造(0.1, &format!("条\u{1f}x"), "甲");
    assert_ne!(a.addr(), c.addr());
    // 同样的取值仍然同址（否则并存那套就废了）
    assert_eq!(造(0.1, "条", "甲").addr(), 造(0.1, "条", "甲").addr());
}
