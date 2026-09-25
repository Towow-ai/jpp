//! 证书的生产者不变量（步 20a-2a，批量裁定解读 (a)）：`selection: None` 的证书只许由 `certify`（`commission`）
//! 写出；按 δ 平移线的现行生产者必须写 `selection` 且带 `selection.delta`。生产者名单由源码扫出、封闭列举：
//! 新加一个 `pub fn commission*` 而不进这里的名单即红。
//!
//! 种子分半是 B85 之前的旧法：写 `selection` 不写 `delta`，这样 `load` 重跑旧证书时能逐字节复现。它的证书是 δ 未知
//! （B104：路由、不放行不可逆 `do`）。步 20a-2b 起（主会话对问题 Q12 的决定）两个种子分半入口降为 crate 内部，
//! 公开面只剩一个名字标明「旧法重现、只供测试」的 `commission_legacy_seed_split_test_only`；它是名单里唯一的旧法例外，
//! 另有守卫核 `crates/*/src` 里没有任何调用。
//!
//! 代价线（`commission_costed` / `commission_costed_graded`）PR35 评审修复（缺陷二）之前也算「不平移」，
//! 写 `selection: None`；改成 B85 分层交替分半后不再属于这一类——线不按 δ 平移的事实没变，但现在显式记
//! `selection.delta = Some(0.0)`（「已知 δ=0」，不是「δ 未知」），方法名前缀 `cost-`。批量裁定 §五 (a) 原文
//! 只限制「谁能写 `selection: None`」（`certify` 与代价线两个生产者），没有强制代价线必须写 `None`；
//! 缩小许可范围不违反那条裁定，见 `地基/过程记录/工程-PR35评审修复.md` §二的核对。
//!
//! 另核装载一侧：`load` 之后，凡不是夹具的记录，带 `selection` 的证书都带 `delta`（旧证书由样本与线解出 δ
//! 写回，解不出或不唯一即降夹具，B117 (c)、B122）。
//!
//! 依据：`地基/附注/2026-09-25-批量裁定.md` §五 (a)；B104；B117；`21` 步 20a-2 的〔B116〕施工注；
//! `地基/过程记录/工程-PR35评审修复.md`（代价线分半的重分类）。

use jpp_calib::{CalibRecord, CalibStore, Cert, CertGrade, LiteralMode, Refusal, Sample, SeqSpec};
use std::collections::BTreeSet;

const 键: &str = "k";

/// 240 条两极样本（正例 p ∈ [0.90, 0.96]、反例 p ∈ [0.04, 0.10]，交替；与 `bypass_20c_load_rerun.rs` 同形），
/// 足够每种认证方式每侧零错过线，旧法证书装载时也能由样本与线唯一解出 δ。
fn 库(phys: &str) -> CalibStore {
    let mut c = CalibStore::new();
    for i in 0..240 {
        let pos = i % 2 == 0;
        let j = (i % 7) as f64 * 0.01;
        c.absorb(
            键,
            Sample {
                p: Some(if pos { 0.9 + j } else { 0.1 - j }),
                label: Some(u8::from(pos)),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: phys.into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
    c.set_delta(键, 0.05).unwrap();
    // 代价线的 J-16 前置：标注集 id 必须给出且不同于保形集 id
    c.set_label_set_id(键, "L").unwrap();
    c.set_set_id(键, "C").unwrap();
    c
}

fn 序贯规格(n: usize) -> SeqSpec {
    SeqSpec {
        pool: None,
        arrival: jpp_calib::random_arrival(n, 7),
        two_ends: false,
        order: "random".into(),
        seed: 7,
        batch: 10,
        weights: [0.8, 0.1, 0.05, 0.05],
        coverage_target: None,
    }
}

/// 生产者的四类
#[derive(Clone, Copy, Debug, PartialEq)]
enum 类 {
    /// 不按 δ 平移：`selection` 为空
    不平移,
    /// 现行平移生产者：`selection` 与 `selection.delta` 都在
    平移,
    /// 旧法重现（种子分半）：`selection` 在、`delta` 缺
    旧法,
    /// 代价线分半（B85，PR35 评审修复缺陷二）：`selection` 在、`delta == Some(0.0)`（显式声明「已知
    /// δ=0」，不是「δ 未知」）、方法名前缀 `cost-`——不按 δ 平移这件事没变，只是不再用 `selection: None`
    /// 表达它
    代价分半,
}

type 调用 = fn(&mut CalibStore) -> Result<Cert, Refusal>;

/// 名单的一项（函数实参是强转位置：闭包在这里转成函数指针）
fn 项(
    n: &'static str,
    p: &'static str,
    k: 类,
    f: 调用,
) -> (&'static str, &'static str, 类, 调用) {
    (n, p, k, f)
}

/// 名单：名字、题型、类、调用。与源码扫出的名单逐个对上。
fn 名单() -> Vec<(&'static str, &'static str, 类, 调用)> {
    use CertGrade::{Formal, Trial};
    vec![
        项("commission", "noul", 类::不平移, |c| {
            c.commission(键, 0.1, 0.1, "条")
        }),
        项("commission_costed", "noul", 类::代价分半, |c| {
            c.commission_costed(键, 0.1, 0.1, "条", (1.0, 10.0), 20260923)
        }),
        项("commission_costed_graded", "noul", 类::代价分半, |c| {
            c.commission_costed_graded(键, 0.25, 0.1, "条", (1.0, 10.0), 20260923, Trial)
        }),
        项(
            "commission_legacy_seed_split_test_only",
            "noul",
            类::旧法,
            |c| c.commission_legacy_seed_split_test_only(键, 0.1, 0.1, 20260923, true, Formal),
        ),
        项(
            "commission_two_sided_split_stratified_graded",
            "noul",
            类::平移,
            |c| c.commission_two_sided_split_stratified_graded(键, 0.1, 0.1, 20260923, Formal),
        ),
        项(
            "commission_legacy_seed_split_test_only",
            "choice",
            类::旧法,
            |c| c.commission_legacy_seed_split_test_only(键, 0.1, 0.1, 20260923, false, Formal),
        ),
        项(
            "commission_upper_split_stratified_graded",
            "choice",
            类::平移,
            |c| c.commission_upper_split_stratified_graded(键, 0.1, 0.1, 20260923, Formal),
        ),
        项(
            "commission_two_sided_fixed_sequence_graded",
            "noul",
            类::平移,
            |c| c.commission_two_sided_fixed_sequence_graded(键, 0.1, 0.1, None, Formal),
        ),
        项(
            "commission_upper_fixed_sequence_graded",
            "choice",
            类::平移,
            |c| c.commission_upper_fixed_sequence_graded(键, 0.1, 0.1, None, Formal),
        ),
        项(
            "commission_two_sided_sequential_graded",
            "noul",
            类::平移,
            |c| {
                c.commission_two_sided_sequential_graded(键, 0.1, 0.1, None, Formal, &序贯规格(240))
            },
        ),
        项(
            "commission_upper_sequential_graded",
            "choice",
            类::平移,
            |c| c.commission_upper_sequential_graded(键, 0.1, 0.1, None, Formal, &序贯规格(240)),
        ),
    ]
}

/// 源码里全部 `pub fn commission*` 的名字
fn 源码名单() -> BTreeSet<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/calib");
    let mut out = BTreeSet::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        for line in text.lines() {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("pub fn commission") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                out.insert(format!("commission{name}"));
            }
        }
    }
    out
}

/// 一条记录上的全部证书（上侧证书与下侧证书）
fn 证书们(r: &CalibRecord) -> Vec<&Cert> {
    r.certs.values().chain(r.lower.as_ref()).collect()
}

/// 名单封闭：源码里的生产者与测试名单逐个相同。新加生产者须先在这里归类。
#[test]
fn 生产者名单由源码扫出且封闭() {
    let 测: BTreeSet<String> = 名单().iter().map(|x| x.0.to_string()).collect();
    assert_eq!(
        源码名单(),
        测,
        "源码里的 pub fn commission* 与本测试的名单不同：新生产者要先归类"
    );
    assert_eq!(测.len(), 10);
}

/// 每个生产者写出的证书：不平移的 `selection` 为空；现行平移的 `selection` 与 `delta` 都在；旧法的 `delta` 缺；
/// 代价分半的 `delta` 恒为 `Some(0.0)`、方法名前缀 `cost-`。
#[test]
fn 每个生产者的证书按类写selection() {
    for (name, phys, k, 调) in 名单() {
        let mut c = 库(phys);
        let cert = 调(&mut c).unwrap_or_else(|e| panic!("{name} 认证不过：{e:?}"));
        let rec = &c.records[键];
        let mut 全部 = 证书们(rec);
        全部.push(&cert);
        for x in 全部 {
            match k {
                类::不平移 => {
                    assert!(x.selection.is_none(), "{name}：不平移的证书不写 selection")
                }
                类::平移 => {
                    let s = x
                        .selection
                        .as_ref()
                        .unwrap_or_else(|| panic!("{name}：平移生产者必须写 selection"));
                    assert!(
                        s.delta.is_some(),
                        "{name}：平移生产者必须写 selection.delta"
                    );
                }
                类::旧法 => {
                    let s = x
                        .selection
                        .as_ref()
                        .unwrap_or_else(|| panic!("{name}：旧法也写 selection"));
                    assert!(s.delta.is_none(), "{name}：旧法重现不写 δ（已知例外，Q12）");
                }
                类::代价分半 => {
                    let s = x
                        .selection
                        .as_ref()
                        .unwrap_or_else(|| panic!("{name}：代价分半也写 selection"));
                    assert_eq!(
                        s.delta,
                        Some(0.0),
                        "{name}：代价线不按 δ 平移，显式声明 δ=0"
                    );
                    assert!(
                        s.method.starts_with("cost-"),
                        "{name}：代价分半的方法名前缀 cost-，实际 {}",
                        s.method
                    );
                }
            }
        }
    }
}

/// 反过来：`selection` 为空的证书只出自 `certify`（批量裁定解读 (a) 的第一句）。代价线改成 B85 分半后
/// （PR35 评审修复缺陷二）不再属于这一类，见文件头注释。
#[test]
fn selection为空的证书只出自certify() {
    let 不平移: BTreeSet<&str> = 名单()
        .iter()
        .filter(|x| x.2 == 类::不平移)
        .map(|x| x.0)
        .collect();
    assert_eq!(不平移, ["commission"].into_iter().collect());
    let 代价分半: BTreeSet<&str> = 名单()
        .iter()
        .filter(|x| x.2 == 类::代价分半)
        .map(|x| x.0)
        .collect();
    assert_eq!(
        代价分半,
        ["commission_costed", "commission_costed_graded"]
            .into_iter()
            .collect()
    );
    let 旧法: BTreeSet<&str> = 名单()
        .iter()
        .filter(|x| x.2 == 类::旧法)
        .map(|x| x.0)
        .collect();
    assert_eq!(
        旧法,
        ["commission_legacy_seed_split_test_only"]
            .into_iter()
            .collect(),
        "旧法例外的名单封闭，不许再加"
    );
}

/// 旧法入口只供测试（步 20a-2b，Q12）：`crates/*/src` 下没有任何文件调用它（定义处是 `fn` 不是 `.`调用）。
#[test]
fn 旧法入口只供测试() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut 调用处: Vec<String> = vec![];
    let mut 栈: Vec<std::path::PathBuf> = std::fs::read_dir(&crates)
        .unwrap()
        .map(|e| e.unwrap().path().join("src"))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(d) = 栈.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                栈.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let t = std::fs::read_to_string(&p).unwrap();
                if t.contains(".commission_legacy_seed_split_test_only(") {
                    调用处.push(p.display().to_string());
                }
            }
        }
    }
    assert!(
        调用处.is_empty(),
        "生产代码调用了只供测试的旧法入口：{调用处:?}"
    );
}

/// 装载一侧：旧法证书存盘再 `load`，重跑由样本与线解出 δ 写回；装载后凡不是夹具的记录，带 `selection` 的证书都带 `delta`。
#[test]
fn 装载后非夹具记录的平移证书都带delta() {
    let mut c = 库("noul");
    c.commission_legacy_seed_split_test_only(键, 0.1, 0.1, 20260923, true, CertGrade::Formal)
        .unwrap();
    assert!(
        证书们(&c.records[键])
            .iter()
            .all(|x| x.selection.as_ref().is_some_and(|s| s.delta.is_none())),
        "前提：旧法证书不带 δ"
    );
    let d = std::env::temp_dir().join(format!("jpp-s20a2a-sel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    c.save(&d).unwrap();
    let s = CalibStore::load(&d).unwrap();
    // 不空转：这批样本上旧证书的 δ 唯一解出（0.05），记录写回而不是降夹具
    assert!(!s.records[键].fixture, "{:?}", s.load_report);
    assert!(
        证书们(&s.records[键])
            .iter()
            .all(|x| x.selection.as_ref().and_then(|v| v.delta) == Some(0.05)),
        "{:?}",
        s.load_report
    );
    for r in s.records.values().filter(|r| !r.fixture) {
        for x in 证书们(r) {
            if let Some(sel) = &x.selection {
                assert!(
                    sel.delta.is_some(),
                    "{}：装载后仍缺 δ 的平移证书只能在夹具记录上",
                    r.key
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}
