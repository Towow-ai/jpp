//! **标注集：算需要数据，审计只需要身份。**
//!
//! 问题：那 202 条标注该整份进校准记录，还是只进摘要？
//! **两边都不对**，而分岔在于把两件不同的需要混成了一件：
//!
//! - **算一条线**，需要真的 `(p, label)` 数据——摘要算不出线。
//! - **事后审计 / 重放比对**，只需要知道**是哪一份**——不需要材料本身。
//!
//! 所以第三条路：**证书里记标注集的指纹，记录里记它的身份与去处，材料本身不进记录。**
//!
//! **指纹是算出来的，不是填的。** 这与 `label_source` **必须声明**恰好相反，
//! 而两者的理由是同一条：**算得出来的就不许填**（填得错），
//! **算不出来的就必须有人明确说**（`label_source` 是系统之外的事实）。

use jpp::effects::{CalibStore, LiteralMode, Sample};

fn 装(c: &mut CalibStore, key: &str, 偏移: f64) {
    for i in 0..30 {
        c.absorb(
            key,
            Sample {
                p: Some(0.30 + i as f64 * 0.02 + 偏移),
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
}

/// **证书永远带着它用的那份标注集的指纹**——不靠谁记得填。
#[test]
fn 证书总是带着标注集指纹() {
    let mut c = CalibStore::new();
    装(&mut c, "k", 0.0);
    let cert = c.commission("k", 0.45, 0.10, "条").expect("认得动");
    assert!(!cert.label_fp.is_empty(), "**指纹是算出来的，不是填的**");
    assert_eq!(
        cert.label_fp.len(),
        16,
        "取 16 位十六进制，与 profile_hash 同口径"
    );
}

/// **两份不同的标注集 = 两个测量**，哪怕 α、簇单位、`label_source` 全同。
/// 与 `label_source` 的分工：**那个说「怎么选的」，这个说「是哪一份」，互相替代不了。**
#[test]
fn 不同标注集是不同的证书格() {
    let mut a = CalibStore::new();
    装(&mut a, "k", 0.0);
    let ca = a.commission("k", 0.45, 0.10, "条").expect("认得动");

    let mut b = CalibStore::new();
    装(&mut b, "k", 0.01); // 同样 30 条、同样标签分布，**数值不同 = 另一份标注集**
    let cb = b.commission("k", 0.45, 0.10, "条").expect("认得动");

    assert_ne!(ca.label_fp, cb.label_fp);
    assert_ne!(
        ca.addr(),
        cb.addr(),
        "**指纹进地址**：两份标注集上的证书不许占同一格"
    );
}

/// **指纹算的是「用到的那些 `(p, label)` 对的规范形」，不是源文件字节。**
/// 源文件会被重新导出、重新排序——**按字节算会在数据没变时乱跳，
/// 而乱跳的检查会教会人绕过它。**
#[test]
fn 指纹对顺序不敏感() {
    let mut 正 = CalibStore::new();
    for i in 0..10 {
        正.absorb(
            "k",
            Sample {
                p: Some(0.3 + i as f64 * 0.05),
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
    let mut 逆 = CalibStore::new();
    for i in (0..10).rev() {
        逆.absorb(
            "k",
            Sample {
                p: Some(0.3 + i as f64 * 0.05),
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
    assert_eq!(
        正.commission("k", 0.6, 0.10, "条").map(|c| c.label_fp),
        逆.commission("k", 0.6, 0.10, "条").map(|c| c.label_fp),
        "同一批对、不同写入顺序，指纹必须相同"
    );
}

/// **声明了是哪一份，就要对得上。** 声明是给人看的（id / 去处），
/// **而检查只信指纹**——去处解析不了是「这台机器上没有那份数据」，不是「检查失败」。
#[test]
fn 声明与实际对不上要拦() {
    let mut c = CalibStore::new();
    装(&mut c, "k", 0.0);
    let 真指纹 = c
        .commission("k", 0.45, 0.10, "条")
        .expect("认得动")
        .label_fp;

    // 声明一个对得上的
    c.declare_label_set(
        "k",
        "e-cal-v2",
        "foundation/runs/jv/e-cal/labels.csv",
        &真指纹,
    )
    .unwrap();
    assert!(
        c.commission("k", 0.40, 0.10, "条").is_ok(),
        "声明对得上，照常认证"
    );

    // 声明一个对不上的 → 拦
    c.declare_label_set("k", "别的集", "somewhere/else.csv", "0000000000000000")
        .unwrap();
    let e = c
        .commission("k", 0.35, 0.10, "条")
        .expect_err("声明与实际对不上要拦");
    let s = format!("{e:?}");
    assert!(
        s.contains("指纹") && s.contains("别的集"),
        "要说清是哪一份对不上：{s}"
    );
}

/// **跨机器：验得了，重认不了——而那是诚实的边界，不是缺口。**
/// 没有数据的那台机器上，线还读得到、证书的主张还核得了、指纹还比得了；
/// **只是重新认证需要数据，这本来就该需要数据。**
#[test]
fn 没有数据的机器上仍然读得到线和凭据() {
    let mut 有数据 = CalibStore::new();
    装(&mut 有数据, "k", 0.0);
    有数据.commission("k", 0.45, 0.10, "条").expect("认得动");
    let 线 = 有数据.get("k").hi;
    let 指纹 = 有数据.选中的证书("k").unwrap().label_fp;

    // 模拟另一台机器：记录在（证书、线、指纹都在），**材料不在**
    let mut 无数据 = CalibStore::new();
    let mut rec = 有数据.get("k");
    rec.samples.clear();
    无数据.records.insert("k".into(), rec);

    assert_eq!(无数据.get("k").hi, 线, "线照常读得到");
    assert_eq!(
        无数据.选中的证书("k").unwrap().label_fp,
        指纹,
        "凭据照常比得了"
    );
    // 但重认证需要数据——**这本来就该需要数据**
    let e = 无数据
        .commission("k", 0.45, 0.10, "条")
        .expect_err("没有数据就重认不了");
    assert!(format!("{e:?}").contains("标注"), "{e:?}");
}
