//! B24：两侧联合认证的多重比较要求。同批选线的二项上界对「最大化后选出的线」不成立，
//! 真值通道改用拆分样本：选线半选线，认证半对选出的那一对只检验一次。
use jpp::effects::{CalibStore, LiteralMode, Refusal, Sample};

fn 折入(n: usize) -> CalibStore {
    let mut c = CalibStore::new();
    for i in 0..n {
        let pos = i % 2 == 0;
        let p = if pos {
            0.95 + (i % 5) as f64 * 0.01
        } else {
            0.01 + (i % 5) as f64 * 0.01
        };
        c.absorb(
            "k",
            Sample {
                p: Some(p),
                label: Some(u8::from(pos)),
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
    // 步 15d-2：按 δ 平移的认证要求记录已有 δ，调用方先 set_delta；noul 题式用 0.05。
    c.set_delta("k", 0.05).unwrap();
    c
}

/// 60 条：固定序（不拆分，B86）能上岗（每侧 30 条零错误），拆分后每半每侧约 15 条，
/// 不到零错误所需条数（22），按规则停在待核而不是放宽门槛。
#[test]
fn 同批选线能过的样本拆分后停在待核() {
    let mut 同批 = 折入(60);
    assert!(
        同批
            .commission_two_sided_fixed_sequence_graded(
                "k",
                0.10,
                0.10,
                None,
                jpp::effects::CertGrade::Formal
            )
            .is_ok()
    );
    let mut 拆分 = 折入(60);
    match 拆分.commission_legacy_seed_split_test_only(
        "k",
        0.10,
        0.10,
        20260923,
        true,
        jpp::effects::CertGrade::Formal,
    ) {
        Err(Refusal::跑不成(why)) => {
            assert!(why.starts_with("待核") && why.contains("样本不足"), "{why}")
        }
        other => panic!("应停在待核：{other:?}"),
    }
    assert_ne!(拆分.get("k").status, "上岗");
}

/// 200 条：拆分认证上岗，证书写明方式、种子、两半条数，认证条数只来自认证半。
#[test]
fn 拆分认证写明方式与两半条数() {
    let mut c = 折入(200);
    let cert = c
        .commission_legacy_seed_split_test_only(
            "k",
            0.10,
            0.10,
            7,
            true,
            jpp::effects::CertGrade::Formal,
        )
        .expect("认证过");
    let sel = cert.selection.clone().expect("拆分证书带 selection");
    assert_eq!(sel.method, "split");
    assert_eq!(sel.seed, 7);
    assert_eq!(sel.n_select + sel.n_certify, 200);
    let rec = c.get("k");
    assert_eq!(rec.status, "上岗");
    let lower = rec.lower.expect("下侧证书");
    assert!(
        cert.n_accepted + lower.n_accepted <= sel.n_certify,
        "认证条数只来自认证半"
    );
    assert!(rec.lo > 0.0 && rec.lo < rec.hi);
}
