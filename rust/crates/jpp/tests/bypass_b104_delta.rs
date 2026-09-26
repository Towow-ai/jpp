//! 候选 B104（暂行修法，主会话 2026-09-24，交 Fable 确认）：证书记认证时的 δ，`cut` 的判区在证书 δ 与
//! 运行期 δ 之间取更严者（加载了画像时），未加载画像时按证书的 δ；线只收紧不放松，两者不同报
//! `W-delta-mismatch`。起因：`过程记录/工程-步20g.md` V7「新发现」（只凭账本重放不带画像，δ 回退）。

use jpp::effects::{
    CalibStore, CertGrade, EffectError, FnPort, JudgeResult, LiteralMode, Ports, Sample,
};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, run, syntax::parse};
use serde_json::Value as Json;

/// 判断恒给 `p`、不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 桩端口(p: f64) -> Ports<'static> {
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

const 路由: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
handle(cut(judge(state(mat("甲")), test("行吗", "k"))), {
    act: fn() { "act" },
    ignore: fn() { "ignore" },
    unsure: fn(u) { consume(u, "drop"); "unsure" }})
"#;

fn 样本(p: f64, l: bool) -> Sample {
    Sample {
        p: Some(p),
        label: Some(u8::from(l)),
        perms: 0,
        mode_share: None,
        mode: LiteralMode::default(),
        phys: "noul".into(),
        cluster: None,
        stratum: None,
    }
}

/// 22 + 22 零错两极（0.95 / 0.05），记录 δ = `导入` 下固定序认证：h = 0.95、l = 0.05。
/// 步 15d-2：认证的 δ 由调用方显式给（`set_delta`），不从画像取
fn 认证(导入: f64) -> CalibStore {
    let mut c = CalibStore::new();
    c.profile.hash = Some("导入画像".into());
    for _ in 0..22 {
        c.absorb("k", 样本(0.95, true)).unwrap();
        c.absorb("k", 样本(0.05, false)).unwrap();
    }
    c.set_delta("k", 导入).unwrap();
    c.commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal)
        .expect("22 + 22 上岗");
    c
}

/// 换成运行期画像：`Some(δ)` = 加载了 δ 先验的画像；`None` = 未加载画像。步 15d-2 起运行期画像的 δ 不进 `cut`
/// （只作认证先验），两种情形出口都按证书的 δ，也不再报 `W-delta-mismatch`（B104 max 分支退役）。
fn 运行(mut c: CalibStore, 画像: Option<f64>, 读数: f64) -> (String, Vec<String>) {
    match 画像 {
        Some(d) => {
            c.profile.delta = jpp::effects::Field::known((d, 0.15, 0.15), "运行画像");
            c.profile.hash = Some("运行画像".into());
        }
        None => c.profile = jpp::effects::Profile::untested(),
    }
    let program = lower(&parse(路由).unwrap()).unwrap();
    let o = run(
        &program,
        桩端口(读数),
        &c,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .map_err(|e| e.render())
    .unwrap();
    let w = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-delta-mismatch"))
        .cloned()
        .collect();
    (o.value_json().as_str().unwrap().to_string(), w)
}

/// (a) 证书 δ 0.05、运行期画像 0.04：按证书 0.05 → p ≥ 0.95，0.945 unsure；步 15d-2 起不再报错配。
#[test]
fn 证书带宽大于运行期不放宽() {
    let c = 认证(0.05);
    assert_eq!(
        c.records["k"]
            .选中的证书()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .delta,
        Some(0.05)
    );
    let (e, w) = 运行(c, Some(0.04), 0.945);
    assert_eq!(e, "unsure");
    assert!(w.is_empty(), "W-delta-mismatch 已退役（步 15d-2）：{w:?}");
}

/// (b) 证书 δ 0.04、运行期画像 0.05：步 15d-2 起按证书 0.04（判区正好是认证检验过的 p ≥ 0.95），0.955 act；
/// 修前按 max 取 0.05 判 unsure（按预注册改）。
#[test]
fn 证书带宽小于运行期取运行期() {
    let (e, w) = 运行(认证(0.04), Some(0.05), 0.955);
    assert_eq!(e, "act");
    assert!(w.is_empty(), "{w:?}");
    let (e, _) = 运行(认证(0.04), Some(0.05), 0.97);
    assert_eq!(e, "act");
}

/// (c) 未加载画像：按证书的 δ 0.04 取线（判区正好是认证检验过的 p ≥ 0.95），0.955 act；步 15d-2 起不报错配。
#[test]
fn 未加载画像按证书带宽() {
    let (e, w) = 运行(认证(0.04), None, 0.955);
    assert_eq!(e, "act");
    assert!(w.is_empty(), "{w:?}");
}

/// (d) 两者相等：不报。
#[test]
fn 相等不报() {
    let (e, w) = 运行(认证(0.05), Some(0.05), 0.97);
    assert_eq!(e, "act");
    assert!(w.is_empty(), "{w:?}");
}

/// (e) 旧证书（种子拆分，不记 δ）：步 15d-2 起用记录的 δ（认证时 `set_delta` 给的 0.04），不看运行期画像、不报。
#[test]
fn 旧证书行为不变() {
    let mut c = CalibStore::new();
    for _ in 0..100 {
        c.absorb("k", 样本(0.95, true)).unwrap();
        c.absorb("k", 样本(0.05, false)).unwrap();
    }
    c.set_delta("k", 0.04).unwrap();
    c.commission_legacy_seed_split_test_only(
        "k",
        0.1,
        0.1,
        7,
        true,
        jpp::effects::CertGrade::Formal,
    )
    .unwrap();
    assert!(
        c.records["k"]
            .选中的证书()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .delta
            .is_none()
    );
    let hi = c.records["k"].hi;
    // 记录 δ 0.04：判区 p ≥ hi + 0.04（运行期画像给 0.05 也不影响）
    let (e, w) = 运行(c, Some(0.05), hi + 0.045);
    assert_eq!(e, "act");
    assert!(w.is_empty(), "{w:?}");
}

/// (f) 下侧对称：证书 δ 0.05、lo = 0.10，运行期画像 0.04：按证书 0.05 → p ≤ 0.05，0.055 unsure。
#[test]
fn 下侧同样不放宽() {
    let (e, w) = 运行(认证(0.05), Some(0.04), 0.055);
    assert_eq!(e, "unsure");
    assert!(w.is_empty(), "{w:?}");
    let (e, _) = 运行(认证(0.05), Some(0.04), 0.03);
    assert_eq!(e, "ignore");
}

/// (g) B104-1（步 20h-1）：线按 δ 平移过而证书没记 δ（旧种子拆分证书）→ 出口照常，`W-delta-unknown`
/// 每键每趟一次，报告 `releases: false`、`delta_unknown: true`。
#[test]
fn 旧证书不放行() {
    let mut c = CalibStore::new();
    for _ in 0..100 {
        c.absorb("k", 样本(0.95, true)).unwrap();
        c.absorb("k", 样本(0.05, false)).unwrap();
    }
    c.set_delta("k", 0.05).unwrap();
    c.commission_legacy_seed_split_test_only(
        "k",
        0.1,
        0.1,
        7,
        true,
        jpp::effects::CertGrade::Formal,
    )
    .unwrap();
    let program = lower(&parse(路由).unwrap()).unwrap();
    let o = run(
        &program,
        桩端口(0.97),
        &c,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap();
    assert_eq!(o.value_json(), Json::String("act".into()));
    let n = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-delta-unknown"))
        .count();
    assert_eq!(n, 1, "{:?}", o.trace.warnings);
    assert_eq!(o.exits[0]["releases"], Json::Bool(false));
    assert_eq!(o.exits[0]["delta_unknown"], Json::Bool(true));
}

/// (h) `selection.delta` 与写 hi / lo 的 δ 同源（B104-1 封闭性的代码不变量）：用证书记的 δ 与同一批样本重跑固定序，
/// 逐位复现 hi / lo。
#[test]
fn 证书带宽与线同源() {
    let c = 认证(0.04);
    let r = &c.records["k"];
    let d = r
        .选中的证书()
        .unwrap()
        .selection
        .as_ref()
        .unwrap()
        .delta
        .unwrap();
    let mut 重跑 = CalibStore::new();
    for s in &r.samples {
        重跑.absorb("k", s.clone()).unwrap();
    }
    重跑.set_delta("k", d).unwrap();
    重跑
        .commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal)
        .unwrap();
    assert_eq!((重跑.records["k"].hi, 重跑.records["k"].lo), (r.hi, r.lo));
}

/// (i) 步 15d-2：线的记录没有 δ（非 certify、非代价线、没有 δ 的夹具线）→ 出口 `Unsure(untested)`，
/// 载体 `Delta`，报 `W-untested`（`20` §3.9「记录的 delta 未测」行）。
#[test]
fn 记录没有带宽出未测() {
    let mut c = CalibStore::new();
    c.put("k", 0.8, 0.2, 50, "上岗", None).unwrap();
    let program = lower(&parse(路由).unwrap()).unwrap();
    let o = run(
        &program,
        桩端口(0.99),
        &c,
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .unwrap();
    assert_eq!(o.value_json(), Json::String("unsure".into()));
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested") && w.contains("Delta")),
        "{:?}",
        o.trace.warnings
    );
}
