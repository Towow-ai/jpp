//! **`unsure_rate` 曾经有消费方、没有生产者——而它的消费方交出去的是一条保证。已补上生产者。**
//!
//! `12`:591 J-10 原话：「unsure 预算：**有标注集时用经验联合 unsure 率**；
//! 无标注集时用联合界 Σuᵢ 作上界」；`12`:814 那一栏写的是「✓ 估计 | **实测**」。
//! 字段自己的文档也写「**标注集上实测的** unsure 率」（`effects.rs:699`）。
//!
//! **而内核里曾经没有任何东西算它**：写入点全集是 `set_unsure_rate`（Rust API）
//! 与 `--calib` 装载器（读 JSON 里已经填好的数）。**`commission` 不算。**
//! 于是走真实路径（`absorb` 标注样本 → `commission`）拿到的记录 `unsure_rate` 是 `None`，
//! `unsure_bound` 把它按 1 计（`strength.rs:155`），**界退化成 `union_bound == n`，
//! 也就是「这 n 道题可能全是 unsure」——一条真的、但什么也没说的界。**
//!
//! **修前实测（这两条测试第一版跑出来的）**：认证成功、有证书、`status == "上岗"`，
//! 而 `unsure_rate == None`、`n_unknown == 1`、`union_bound == 1.0`。
//! **修后**：`unsure_rate == Some(0.6167)`、`n_unknown == 0`、`union_bound == 0.6167`。
//! 0.6167 这个数大，是因为**证书只界定放行那一侧**，`commission` 把 `lo` 定为 `0.0`，
//! **`Ignore` 出口实际不可达**——线以下的全是 unsure。**那是真的，不是错的。**
//!
//! **这一条的意义在于它更正了一个已经写进状态文件的诊断**：
//! `unsure_bound` 产不出东西**不是因为没有线**，所以 `--calib-out`（出料）**没有修好它**。
//! 已实测的那次「`unsure_bound` 从 `n` 变 `0.2`」用的是**手写 `--calib` 夹具**里
//! 填好的 `unsure_rate: 0.1`（`jpp-cli/tests/wiring.rs:53`），**不是认证算出来的**。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, LiteralMode, Ports, Sample};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::Answer;

/// 判断恒给 0.9、不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
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
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

/// 让 judge 恒返回某个 p 的端口表（给「与 cut 的出口比」那条用）
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

/// **按真实路径造一条上岗记录**：`absorb` 带标注的观察 → `commission` 认证。
/// **中间不手填任何数**——手填就测不出生产者缺没缺，这与「往返验收中间不许改文件」同一条。
fn 真实路径上岗() -> CalibStore {
    let mut c = CalibStore::new();
    // 可分的一批：低 p 多为负、高 p 多为正，认证过得去
    for i in 0..60 {
        let p = 0.02 + i as f64 * 0.016;
        c.absorb(
            "k",
            Sample {
                p: Some(p),
                label: Some(if p > 0.55 { 1 } else { 0 }),
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
    c.commission("k", 0.10, 0.10, "条").expect("认得动");
    c
}

#[test]
fn 认证会算出unsure率() {
    let c = 真实路径上岗();
    let rec = c.get("k");
    assert_eq!(rec.status, "上岗", "前提：这条记录真的上岗了");
    assert!(!rec.certs.is_empty(), "前提：真的拿到了证书");
    let u = rec
        .unsure_rate
        .expect("**认证要算 unsure_rate**：字段文档写的是「标注集上实测」");
    assert!((0.0..=1.0).contains(&u), "是个率：{u}");
    // **手算一遍当独立来源**：证书只界定放行那一侧，`lo = 0`，所以 `Ignore` 不可达，
    // unsure = 「没过 hi + δ 的那些」。certify 线的 δ 是 0（批量裁定解读 (a)，步 15d-2）。
    let (hi, lo) = (rec.hi, rec.lo);
    assert_eq!(lo, 0.0, "前提：证书把 lo 定成 0");
    let d = 0.0; // certify 线 δ = 0（步 15d-2）
    let 手算 = rec
        .samples
        .iter()
        .filter(|s| s.label.is_some())
        .filter_map(|s| s.p)
        .filter(|p| !(*p >= hi + d) && !(*p <= lo - d))
        .count() as f64
        / rec
            .samples
            .iter()
            .filter(|s| s.label.is_some() && s.p.is_some())
            .count() as f64;
    assert!(
        (u - 手算).abs() < 1e-4,
        "算出来的 {u} 与手算 {手算} 对不上（hi={hi} δ={d}）"
    );
}

/// **测的必须是消费方真的会碰上的那个事件，不是一个长得像它的量。**
/// 所以不拿公式跟公式比，**拿它跟 `cut` 真的走出来的出口比**——
/// `cut` 的出口路由是另一处实现，我写这条断言时没有照着它抄。
#[test]
fn 那个判据与cut真走出来的出口一致() {
    let c = 真实路径上岗();
    let rec = c.get("k");
    let (hi, lo) = (rec.hi, rec.lo);
    let d = 0.0; // certify 线 δ = 0（步 15d-2）
    // 三个点：明显过线、带内、以及「若 lo 有意义的话会被 Ignore」的低分
    for p in [(hi + d + 0.05).min(0.999), (hi + d) / 2.0, 0.001] {
        let 按判据算的是unsure = !(p >= hi + d) && !(p <= lo - d);
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
        let out = run(&program, 定值端口(p), &c, &ActionRegistry::new(), &mut l).expect("跑得完");
        let 走出来的 = match &out.value {
            Some(jpp::value::Value::Text(t, _)) => t.to_string(),
            v => panic!("{v:?}"),
        };
        assert_eq!(
            走出来的 == "un",
            按判据算的是unsure,
            "p={p} hi={hi} lo={lo} δ={d}：cut 走的是 {走出来的}，而 unsure_rate 的判据说 unsure={按判据算的是unsure}"
        );
    }
}

/// **后果**：J-10 的界在真实路径上不再平凡。
/// 修前是 `union_bound == n`，意思是「这 n 道题可能全是 unsure」。
#[test]
fn 真实路径上的界不再平凡() {
    let c = 真实路径上岗();
    let program = jpp::lower(
        &jpp::syntax::parse(
            r#"
budget {calls: 4, cost: 1};
unsure_bound(judge(state(mat("材料")), test("行吗", "k")))
"#,
        )
        .expect("解析"),
    )
    .expect("lower");
    let mut l = Ledger::new();
    let out = run(&program, 桩端口(), &c, &ActionRegistry::new(), &mut l).expect("跑得完");
    let Some(jpp::value::Value::Record(r)) = &out.value else {
        panic!("unsure_bound 返回记录：{:?}", out.value)
    };
    let 取 = |名: &str| {
        r.iter()
            .find(|(k, _)| k == 名)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("没有这一栏：{名}"))
    };
    let 取整 = |名: &str| match 取(名) {
        jpp::value::Value::Int(i, _) => i,
        v => panic!("{名} 不是整数：{v:?}"),
    };
    let 取浮 = |名: &str| match 取(名) {
        jpp::value::Value::Float(f, _) => f,
        v => panic!("{名} 不是浮点：{v:?}"),
    };
    println!(
        "n={} union_bound={} n_unknown={}",
        取整("n"),
        取浮("union_bound"),
        取整("n_unknown")
    );
    assert_eq!(取整("n"), 1);
    assert_eq!(
        取整("n_unknown"),
        0,
        "**上岗且有证书的题，现在有自己的 unsure_rate**"
    );
    let ub = 取浮("union_bound");
    assert!(ub < 1.0, "**界不再退化成 n**（修前是 1.0）：{ub}");
    assert!(
        (ub - c.get("k").unsure_rate.expect("有值")).abs() < 1e-9,
        "单题的界就是它自己那个率"
    );
}

/// **往返**：`commission` 算出来的 `unsure_rate` 要能写出去、再读回来。
/// **中间不许手写任何 JSON**——这一条是 `--calib-out` 那次验收栽过的坑（`5c908fd`：
/// 验收把第一趟落的盘用一行手写 JSON 覆盖掉，于是接缝零覆盖）。
#[test]
fn 算出来的unsure率写得出也读得回() {
    let c = 真实路径上岗();
    let 原值 = c.get("k").unsure_rate.expect("认证算出来了");
    let dir = std::env::temp_dir().join(format!("jpp_unsure_rt_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    c.save(&dir).expect("写得出");
    let 读回 = CalibStore::load(&dir)
        .expect("**装载器要认得 unsure_rate**（它是结构体字段，认得的字段是算出来的）");
    let r2 = 读回.get("k");
    assert_eq!(r2.status, "上岗", "状态要原样回来");
    assert_eq!(
        r2.unsure_rate,
        Some(原值),
        "**这个数要原样回来**，否则出料等于没出"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **边界（2026-09-23 更新，施工件 a）**：`.jpp` 作者现在有一条认证入口——`jpp calib-import`
/// （真值通道）经 `jpp::truth::import_labels` 调 `commission_two_sided`，于是走 CLI 的人
/// 也能得到由认证算出的 `unsure_rate`。**原来的单侧 `commission` 仍然没有 CLI 入口**，
/// 它只在宿主（Rust）调用方那条路上。
///
/// 这条测试仍然不断言行为，只把边界钉在代码里：CLI 源码里**调用**认证的地方只能是真值通道。
#[test]
fn 边界_commission的CLI入口只有真值通道() {
    let cli = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli");
    let mut 调用 = vec![];
    for e in walk(&cli) {
        let t = std::fs::read_to_string(&e).unwrap_or_default();
        for (i, 行) in t.lines().enumerate() {
            let 行 = 行.trim_start();
            if 行.starts_with("//") {
                continue;
            }
            if 行.contains(".commission") || 行.contains("import_labels(") {
                调用.push(format!("{}:{}: {}", e.display(), i + 1, 行));
            }
        }
    }
    assert!(
        调用
            .iter()
            .all(|x| x.contains("calib_import.rs") && x.contains("import_labels(")),
        "**CLI 多了一条认证入口就该来改这条测试和它上面那段话**：{调用:?}"
    );
    assert!(!调用.is_empty(), "真值通道的入口不见了：{调用:?}");
}

fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out
}
