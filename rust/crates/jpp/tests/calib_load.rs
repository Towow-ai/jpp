//! **把 E-CAL 那三条真校准记录读进 Rust 内核**——在这之前，内核从没读过它们。
//!
//! `CalibStore` **没有任何落盘/装载**（`grep "fn load"` 在 `effects.rs` 下只命中
//! `Profile::load`），CLI 也从不构造它。**所以「把 `label_source` 记上去」此前没有目标可记。**
//!
//! 而那三条记录里 `label_source` **已经写着了**——**埋在 `source` 那个自由文本字段里**：
//!
//! > `label_source=模型双标+人抽检（Fable+Opus 标签一致且至少一方置信高；两方一致但都非
//! > 高置信的条目未并入，另列核对臂）`
//!
//! **而 `source` 正是我用「全仓零引用」论证过不该做成自由字符串的那个字段。**
//! 这是「写在六处、那个数照样丢掉一半」的第六处，一字不差。

use jpp::effects::{CalibStore, LabelSource, LiteralMode, Sample};

fn 记录夹具() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calib_legacy")
}

/// 旧记录格式的可携带回归：保留字段形状，不依赖未发布的研究运行目录。
#[test]
fn 读得进e_cal旧格式记录夹具() {
    let store = CalibStore::load(&记录夹具()).expect("仓库内三条旧格式夹具该读得进来");
    for (键, n) in [
        ("e_cal.noul", 73u64),
        ("e_cal.choice", 74),
        ("e_cal.score", 55),
    ] {
        let r = store.get(键);
        assert_eq!(r.status, "上岗", "{键}");
        assert_eq!(r.n, n, "{键}");
        assert_eq!(r.set_id, "e-cal-v2-模型双标-2026-09-20", "{键}");
        // **`samples` 是 0**：这三条记录没留标注集，所以在 Rust 这边**重认证不了**
        assert_eq!(r.samples.len(), 0, "{键} 没留标注集（记录里 samples 为空）");
        // **`label_source` 读出来是「未声明」**——文件里那句话在 `source` 散文里，不是字段
        assert_eq!(r.label_source, LabelSource::未声明, "{键}");
        assert!(r.label_source.可疑());
    }
}

/// **装载不许静默丢字段。** 与 `Profile::load` 缺字段就报错同一条纪律：
/// **档案里有而内核没地方放的字段，要么报错、要么具名地「知道但不映射」，不许无声吞掉。**
#[test]
fn 不认得的字段要报错而不是无声吞掉() {
    let dir = std::env::temp_dir().join(format!("jpp-calib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("x.json"),
        r#"{"key":"x","hi":0.8,"lo":0.2,"n":5,"status":"上岗","新字段":1}"#,
    )
    .unwrap();
    let e = CalibStore::load(&dir).expect_err("不认得的字段要报错");
    assert!(e.contains("新字段"), "要指名道姓：{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **装载认得 `label_source` 这个真字段**（不是散文里那句）。
#[test]
fn 装载认得结构化的label_source() {
    let dir = std::env::temp_dir().join(format!("jpp-calib-ls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("y.json"),
        r#"{"key":"y","hi":0.8,"lo":0.2,"n":5,"status":"上岗",
        "label_source":{"选择子集":{"判据":"两模型一致且至少一方高置信","与对错相关":0.527}}}"#,
    )
    .unwrap();
    let store = CalibStore::load(&dir).expect("读得进");
    match &store.get("y").label_source {
        LabelSource::选择子集 {
            判据, 与对错相关
        } => {
            assert_eq!(判据, "两模型一致且至少一方高置信");
            assert_eq!(*与对错相关, Some(0.527));
        }
        other => panic!("该是选择子集：{other:?}"),
    }
    assert!(
        !store.get("y").label_source.可疑(),
        "声明了判据也测了相关性，不可疑"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **标量相关性成立的那个前提，此前没有东西保证。**
///
/// `与对错相关` 是一个标量，而相关性按题型分叉（noul +0.527 / choice +0.418 /
/// **score −0.062，反向**）。**一条记录一个值是对的——因为一条记录就是一个题型**，
/// 键里有 `phys`。**但那是约定，不是保证**：`absorb` 收任何 `phys` 字符串，
/// 而 `commission` 的**非代价路径从来不查题型是否同质**（代价路径查）。
///
/// **不同尺不可比**——这是本项目自己的判据，而认证路径上此前有一处没用上它。
#[test]
fn 混题型的样本不许认证() {
    let mut c = CalibStore::new();
    for i in 0..20 {
        c.absorb(
            "k",
            Sample {
                p: Some(0.3 + i as f64 * 0.03),
                label: Some(if i > 5 { 1 } else { 0 }),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                // **一半 noul、一半 choice**
                phys: if i % 2 == 0 {
                    "noul".into()
                } else {
                    "choice".into()
                },
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
    let e = c
        .commission("k", 0.45, 0.10, "条")
        .expect_err("混题型的样本不该认证得出一条线");
    let s = format!("{e:?}");
    assert!(
        s.contains("noul") && s.contains("choice"),
        "要说清混了哪几种：{s}"
    );

    // 单一题型照常认得动
    let mut d = CalibStore::new();
    for i in 0..20 {
        d.absorb(
            "k",
            Sample {
                p: Some(0.3 + i as f64 * 0.03),
                label: Some(if i > 5 { 1 } else { 0 }),
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
    assert!(d.commission("k", 0.45, 0.10, "条").is_ok());
}

/// **往返：`save` 写出来的，`load` 必须原样读得回去。**
///
/// **中间不许有任何一行去改那个文件**——总控点出的正是这条：
/// 我上一包的验收在「写」与「读」之间插了一行手写 JSON，
/// **于是「写」有覆盖、「读」有覆盖，而接缝零覆盖。**
///
/// **根因**：装载器的「认得的字段」表是**手写的列举**
/// ——我给 `calib_hash` 选了排除法、给这里留了列举法，**而它在几小时内就漂了**：
/// `label_locator` / `label_fp` 是结构体字段，**却不在那张表里**。
/// 实测症状：`--calib-out` 写出来的目录喂给 `--calib` 报
/// 「有内核不认得的字段 `label_fp`」。
#[test]
fn save写出来的load读得回去() {
    use jpp::effects::Cert;
    let dir = std::env::temp_dir().join(format!("jpp-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // **把每一个字段都填上**——只填一半的记录，往返测不出漏掉的那一半
    let mut c = CalibStore::new();
    c.put("k", 0.8, 0.2, 30, "上岗", Some(0.05)).unwrap();
    c.set_unsure_rate("k", 0.1).unwrap();
    c.set_delta("k", 0.02).unwrap();
    c.set_set_id("k", "conf-B").unwrap();
    c.declare_label_set("k", "label-A", "somewhere/labels.csv", "deadbeefdeadbeef")
        .unwrap();
    c.set_label_source(
        "k",
        LabelSource::选择子集 {
            判据: "两模型一致".into(),
            与对错相关: Some(0.527),
        },
    )
    .unwrap();
    c.absorb(
        "k",
        Sample {
            p: Some(0.9),
            label: Some(1),
            perms: 2,
            mode_share: Some(1.0),
            mode: LiteralMode::CodeLiteral,
            phys: "noul".into(),
            cluster: Some("段甲".into()),
            stratum: None,
        },
    )
    .unwrap();
    let cert = Cert {
        alpha: 0.45,
        conf_delta: 0.10,
        hi: 0.8,
        n_accepted: 20,
        n_errors: 1,
        ucb: 0.4,
        cluster_unit: "条".into(),
        resample: Some((200, "全过才算过".into())),
        cost: Some((10.0, 1.0)),
        bounded_side: "单侧".into(),
        label_fp: "abcdef0123456789".into(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: LabelSource::全体,
    };
    c.records
        .get_mut("k")
        .unwrap()
        .certs
        .insert(cert.addr(), cert);

    c.save(&dir).expect("写得出");
    // **原样读回来，中间一个字节都不改**
    let back = CalibStore::load(&dir).expect("**写出来的必须读得回去**");
    assert_eq!(
        back.get("k"),
        c.get("k"),
        "**逐字段相同**——往返不许丢任何一维"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
