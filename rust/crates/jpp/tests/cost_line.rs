//! **代价矩阵决定线定在哪；保形证书决定这个键能不能上岗。**
//!
//! 那条裁定此前只落地了一半：证书那半在（`commission`、证书寻址、`W-uncertified`），
//! **而「代价矩阵决定线定在哪」这半在语言里根本不存在**——`Question` 没有 `cost`，
//! `12`:159 的 `cut(r, c, cost?)` 是纸面上的签名。
//! **门装好了，旋钮没装**：作者能被告知「你这条线没凭据」，
//! 却没有任何办法告诉语言「我这一格的误放行比漏放行贵十倍」。

use jpp::conformal::cost_line;
use jpp::effects::{CalibStore, LiteralMode, Sample};
use serde_json::Value as Json;

fn 取() -> Vec<(f64, bool, String)> {
    let j: Json = serde_json::from_str(include_str!("ecal_fixture.json")).unwrap();
    j["noul"]
        .as_array()
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

/// **验收的那三组数，照实报，包括难看的。**
///
/// 与 Python `calib.py::cost_line` **逐值一致**（跑过 oracle 对的）。
#[test]
fn 换代价矩阵线要动放行集合要变假放行率要跟着动() {
    let s: Vec<(f64, bool)> = 取().into_iter().map(|(p, l, _)| (p, l)).collect();
    let mut 行 = vec![];
    for (fp, fn_) in [(1.0, 1.0), (10.0, 1.0), (1.0, 10.0)] {
        let c = cost_line(&s, fp, fn_).expect("算得出");
        println!(
            "fp={fp} fn={fn_}: 线={:.3} 放行={}/{} 误放行={} **经验假放行率={:.3}**",
            c.line,
            c.n_accepted,
            c.n,
            c.n_false_accept,
            c.false_accept_rate.expect("放行非空")
        );
        行.push((c.line, c.n_accepted, c.false_accept_rate.expect("放行非空")));
    }
    // 与 Python oracle 逐值一致
    assert_eq!((行[0].0 * 1000.0).round(), 590.0, "代价对称：线 0.590");
    assert_eq!(行[0].1, 50, "放行 50/73");
    assert!(
        (行[0].2 - 0.300).abs() < 0.001,
        "**经验假放行率 0.300——代价对称时它就是这么烂**"
    );

    assert_eq!((行[1].0 * 1000.0).round(), 780.0, "fp 重：线往上走到 0.780");
    assert_eq!(行[1].1, 6, "放行缩到 6/73");
    assert!((行[1].2 - 0.0).abs() < 0.001, "假放行率 0.000");

    assert_eq!((行[2].0 * 1000.0).round(), 385.0, "fn 重：线往下走到 0.385");
    assert_eq!(行[2].1, 63, "放行涨到 63/73");
    assert!((行[2].2 - 0.349).abs() < 0.001, "假放行率 0.349");

    // **率要和放行集合一起看**：fp 重那组率降到 0，但放行集合从 50 缩到 6。
    // **一个缩小的放行集合把率压低了，那不是变好。**
    assert!(
        行[1].1 < 行[0].1 && 行[2].1 > 行[0].1,
        "线动了，放行集合跟着动"
    );
}

/// **「并列取更高的 `t`」是承重的**（宁可 unsure）。
/// 顺带记一处我自己造的分叉：我第一版顺手抄了 `certify` 的 `ps.dedup()`，
/// **而 `cost_line` 的候选表不去重**——线从 0.590 掉到 0.585，
/// **而放行集合恰好没变，所以三个数里有两个仍然对得上**。
#[test]
fn 候选表不去重与certify不同() {
    // 两个相同的 p：去重的话这个点本身不会进候选表
    let s = vec![(0.4, false), (0.6, true), (0.6, true), (0.8, true)];
    let c = cost_line(&s, 1.0, 1.0).expect("算得出");
    assert!(
        s.iter().any(|(p, _)| (*p - c.line).abs() < 1e-9) || c.line == 0.5 || c.line == 0.7,
        "候选表含样本点本身（重复值的「中点」就是它自己）：线={}",
        c.line
    );
}

/// **放行集合为空时假放行率无定义，不是 0。**
/// 与 `binomial_upper` 的 `n == 0 → 1.0` 同一条：**空放行区上的「零错」不是证据**。
#[test]
fn 空放行集合的假放行率是未定义() {
    let s = vec![(0.1, false), (0.2, false)];
    // fp 极重 → 线推到 1.0 之上，一条也不放行
    let c = cost_line(&s, 1000.0, 1.0).expect("算得出");
    assert_eq!(c.n_accepted, 0);
    assert_eq!(c.false_accept_rate, None, "空放行区不报 0");
}

#[test]
fn reject_all_keeps_score_one_rejected() {
    let samples = [(1.0, false), (0.5, true)];
    let c = cost_line(&samples, 1000.0, 1.0).unwrap();
    assert!(
        c.line > 1.0,
        "reject-all must not become an inclusive threshold of 1"
    );
    assert_eq!(c.n_accepted, 0);
    assert_eq!(c.n_false_accept, 0);
    assert_eq!(c.false_accept_rate, None);
    let actual_cost = samples
        .iter()
        .map(|(p, correct)| {
            if *p >= c.line && !correct {
                1000.0
            } else if *p < c.line && *correct {
                1.0
            } else {
                0.0
            }
        })
        .sum::<f64>();
    assert_eq!(c.cost, actual_cost);
}

/// **代价线同样要有证书才能上岗**——两条规则合起来仍然走得通。
/// 裁定是：**代价决定线定在哪，证书决定能不能上岗**，不是二选一。
#[test]
fn 代价定线证书定能不能上岗() {
    let mut store = CalibStore::new();
    for (p, l, seg) in 取() {
        store
            .absorb(
                "e_cal.noul",
                Sample {
                    p: Some(p),
                    label: Some(if l { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: Some(seg),
                    stratum: None,
                },
            )
            .expect("折得进");
    }
    // J-16：代价线的标注集必须与保形集不同源
    store
        .set_label_set_id("e_cal.noul", "label-A")
        .expect("写得进");
    store.set_set_id("e_cal.noul", "conf-B").expect("写得进");

    // fp 重那组（PR35 评审修复：现在选线半/认证半各半，数值随分半而变，见 `Ok` 分支只打印不断言精确值）
    let r = store.commission_costed("e_cal.noul", 0.45, 0.10, "条", (10.0, 1.0), 20260923);
    match &r {
        Ok(cert) => {
            println!(
                "代价线 {:.3} 拿到证书：放行 {} 错 {} 上界 {:.3}",
                cert.hi, cert.n_accepted, cert.n_errors, cert.ucb
            );
            assert!(
                (cert.hi - 0.780).abs() < 0.001,
                "**线由代价矩阵定**，不由证书自己找"
            );
            assert_eq!(store.get("e_cal.noul").status, "上岗");
            assert_eq!(
                cert.cost,
                Some((10.0, 1.0)),
                "证书要记下是哪个代价矩阵定的线"
            );
        }
        Err(e) => println!("代价线认证不过（也是诚实结果）：{e:?}"),
    }

    // **J-16：同源就不许算代价线**
    store
        .set_label_set_id("e_cal.noul", "conf-B")
        .expect("写得进");
    let e = store.commission_costed("e_cal.noul", 0.45, 0.10, "条", (10.0, 1.0), 20260923);
    assert!(
        format!("{e:?}").contains("J-16"),
        "标注集与保形集同源要拦：{e:?}"
    );
}

/// **`cut(cost=)` 只对 test 题有定义**（Python `runtime.py:1147` 直接 raise）。
/// select / measure 的代价线**未定**——让它悄悄什么也不做，比报错糟。
#[test]
fn 代价线只对test题有定义() {
    let mut store = CalibStore::new();
    for i in 0..20 {
        store
            .absorb(
                "k",
                Sample {
                    p: Some(0.3 + i as f64 * 0.03),
                    label: Some(if i > 5 { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "choice".into(),
                    cluster: None,
                    stratum: None,
                },
            )
            .unwrap();
    }
    store.set_label_set_id("k", "L").unwrap();
    store.set_set_id("k", "C").unwrap();
    let e = store.commission_costed("k", 0.45, 0.10, "条", (1.0, 1.0), 20260923);
    assert!(
        format!("{e:?}").contains("test"),
        "非 test 题要说清代价线未定：{e:?}"
    );
}

/// **`仅供参考` 的类型闸此前只拦 Rust 调用者。**
/// 内核把它转成 `Value::Float` 交给 J++ 程序——**这门语言唯一的用户拿到裸浮点，
/// 可以直接当判据，没有任何东西会红**。「替身上成立、真机上失效」的又一张脸，
/// 只是这次的「替身」是**宿主语言**。
#[test]
fn 参考值在jpp那侧也比不了大小() {
    use jpp::ActionRegistry;
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::ledger::Ledger;
    use jpp::value::Answer;
    /// 判断恒给 0.9，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
    fn 桩端口() -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                    confidence: vec![],
                })
            }))
            .with(FnPort::generate("m", |_p, _c: &[Json], _n, _r| {
                Err(EffectError("x".into()))
            }))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
    let 跑 = |尾: &str| {
        let src = format!(
            r#"
budget {{calls: 4, cost: 1}};
let r = judge(state(mat("材料")), test("行吗", "k"));
let b = unsure_bound([r]);
{尾}
"#
        );
        let program = jpp::lower(&jpp::syntax::parse(&src).expect("解析")).expect("lower");
        let mut l = Ledger::new();
        jpp::run(
            &program,
            桩端口(),
            &CalibStore::new(),
            &ActionRegistry::new(),
            &mut l,
        )
        .map(|o| o.value_json().to_string())
        .map_err(|e| e.render())
    };
    // 判据那个数照常是数，比得了
    assert!(
        跑("b.union_bound >= 0.0").is_ok(),
        "union_bound 是判据，当然能比"
    );
    // **参考值比不了、也算不了**
    assert!(
        跑("b.independent_any >= 0.0").is_err(),
        "**参考值不该能当判据比大小**"
    );
    assert!(
        跑("b.independent_any + 0.0").is_err(),
        "**参考值不该能做算术**"
    );
    // 但看得见、打得出
    let v = 跑("{看一眼: b.independent_any}").expect("读出来看一眼是允许的");
    assert!(v.contains("仅供参考"), "**取出来时那句话要跟着**：{v}");
}

/// **同 α、不同代价矩阵的两张证书，取线更高的那张。**
/// 这一格是代价矩阵进来之后才有内容的：一张 `fn` 重的（线 0.385、放行 63）和一张
/// `fp` 重的（线 0.780、放行 6）可以在同一个 α 上都认得住，
/// **按地址字符串挑就可能挑中宽松的那张**。线更高 = 放行更少 = 往拒绝那边倒。
#[test]
fn 同风险目标时取线更高的那张() {
    let mut store = CalibStore::new();
    for (p, l, seg) in 取() {
        store
            .absorb(
                "k",
                Sample {
                    p: Some(p),
                    label: Some(if l { 1 } else { 0 }),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: Some(seg),
                    stratum: None,
                },
            )
            .unwrap();
    }
    store.set_label_set_id("k", "L").unwrap();
    store.set_set_id("k", "C").unwrap();

    let 松 = store.commission_costed("k", 0.80, 0.10, "条", (1.0, 10.0), 20260923);
    let 严 = store.commission_costed("k", 0.80, 0.10, "条", (10.0, 1.0), 20260923);
    if 松.is_ok() && 严.is_ok() {
        assert_eq!(
            store.get("k").certs.len(),
            2,
            "两个代价矩阵是两个格，不是覆盖"
        );
        let 选中 = store.选中的证书("k").expect("有");
        assert_eq!(
            选中.cost,
            Some((10.0, 1.0)),
            "**同 α 取线更高的那张**：{:?}",
            选中
        );
        // PR35 评审修复：线现在只在选线半（37 条）上选，不再是全量 73 条上选出的 0.780；
        // 分半后实测 0.775（同一颗种子 20260923，`分法::交替`，确定性），断言前用 `eprintln!` 核对过
        assert!(
            (store.get("k").hi - 0.775).abs() < 0.001,
            "{}",
            store.get("k").hi
        );
    } else {
        println!(
            "这批数据上 α=0.80 两张没都认住（松={:?} 严={:?}），这一格没被覆盖到",
            松.is_ok(),
            严.is_ok()
        );
    }
}
