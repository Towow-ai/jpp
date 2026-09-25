//! 步 24h：J-10 的 `Σ ≤ limit` 分支里，循环/函数体内站点即使没超预算也不是真正的上界
//! （B101 (b) 缺口，总账 K-185 备注 (b) 点出）。新诊断 `W-unsure-not-bound`。
//! 预注册见 `地基/过程记录/工程-步24h.md`。

use jpp::effects::{CalibStore, LiteralMode, Sample};
use jpp::{lower, syntax::parse};

fn 记录本(key: &str, unsure_rate: f64) -> CalibStore {
    let mut c = CalibStore::new();
    for i in 0..60 {
        let p = 0.02 + i as f64 * 0.016;
        c.absorb(
            key,
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
    c.commission(key, 0.10, 0.10, "条").expect("认得动");
    c.set_unsure_rate(key, unsure_rate).expect("设得上");
    c
}

fn 带unsure预算(src: &str, unsure: Option<f64>) -> jpp::Program {
    let mut p = lower(&parse(src).expect("解析")).expect("lower");
    p.budget.unsure = unsure;
    p
}

fn 找(src: &str, c: &CalibStore, unsure: Option<f64>) -> Vec<String> {
    let p = 带unsure预算(src, unsure);
    jpp::check_with_calib(&p, c)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "W-unsure-not-bound")
        .map(|d| d.message.clone())
        .collect()
}

/// 循环内站点、Σ ≤ limit：报 W-unsure-not-bound。
#[test]
fn 循环内站点未超预算_报不是上界() {
    let src = r#"
budget {calls: 8, cost: 1};
map([1, 2, 3], fn(i) {
    judge(state(mat("材料")), test("行吗", "ka"))
})
"#;
    let c = 记录本("ka", 0.1);
    let es = 找(src, &c, Some(0.9));
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("不是真正的上界"), "{}", es[0]);
}

/// 没有循环/函数体内站点、Σ ≤ limit：不报（既有 24d 之前的行为不变）。
#[test]
fn 没有循环站点未超预算_不报() {
    let src = r#"
budget {calls: 2, cost: 1};
judge(state(mat("材料")), test("行吗", "ka"))
"#;
    let c = 记录本("ka", 0.1);
    assert!(找(src, &c, Some(0.9)).is_empty());
}

/// Σ > limit（既有 J-10 该报的情形）：不报 W-unsure-not-bound（那条路走的是 J-10 本体，
/// 两条诊断互斥，不重复）。
#[test]
fn 超预算时不报本条只报j10() {
    let src = r#"
budget {calls: 8, cost: 1};
map([1, 2, 3], fn(i) {
    judge(state(mat("材料")), test("行吗", "ka"))
})
"#;
    let c = 记录本("ka", 0.9);
    let p = 带unsure预算(src, Some(0.5));
    let r = jpp::check_with_calib(&p, &c);
    assert!(
        r.diagnostics.iter().any(|d| d.rule == "J-10"),
        "{}",
        r.render()
    );
    assert!(
        !r.diagnostics.iter().any(|d| d.rule == "W-unsure-not-bound"),
        "{}",
        r.render()
    );
}

/// 没设 `budget.unsure`：不报（既有行为，`None` 不是 0）。
#[test]
fn 不设这一格不报() {
    let src = r#"
budget {calls: 8, cost: 1};
map([1, 2, 3], fn(i) {
    judge(state(mat("材料")), test("行吗", "ka"))
})
"#;
    let c = 记录本("ka", 0.9);
    assert!(找(src, &c, None).is_empty());
}
