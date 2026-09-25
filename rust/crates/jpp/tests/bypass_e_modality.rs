//! 步 24b（B126 守卫半）：`state` 收到 `Mat.modality ≠ "text"` 的材料时，画像 `text_only`
//! 未测或为 `true` 即 `E-modality`；`text_only: false` 且模态在 `modalities_accepted` 内放行，
//! 不在内仍 `E-modality`。源码层今天造不出非文字材料（S 库渲染类 `do` 在步 25 才落地），
//! 测试直接把 `Mat.modality` 改成非文字来触达这条判据（唯一能测到它的办法，见预注册
//! `地基/过程记录/工程-步24b.md` §二·2「静态半本批不做」）。依据：B126
//! （`地基/附注/2026-09-25-待补批量裁定-2.md` §六）。

use jpp::effects::{CalibStore, FixedPorts, Profile};
use jpp::ledger::Ledger;
use jpp::syntax::parse;
use jpp::value::{Mat, Taint};
use jpp::{ActionRegistry, Session};

const 程序: &str = "budget {calls: 0, cost: 0};\nstate(input)\n";

fn 跑(profile: Profile) -> Result<(), String> {
    let entry = jpp::EntryArgs {
        materials: vec![jpp::EntryMat::new("input", 非文字材料())],
        ..Default::default()
    };
    let program = Session::compile(&parse(程序).expect("解析"), &entry.decl()).expect("compile");
    let mut calib = CalibStore::new();
    calib.profile = profile;
    let a = ActionRegistry::new();
    let mut fp = FixedPorts::new();
    let mut l = Ledger::new();
    // `run` 的公共入口不带 EntryArgs（见 crate 根 `pub fn run`，恒 `EntryArgs::default()`），
    // 要带材料入口得走 `Session` 手工装配，与 `entry_args.rs` 同一条路径。
    Session::new(fp.ports(), &calib, &a)
        .run(&program, &entry, &mut l)
        .map(|_| ())
        .map_err(|e| match e {
            jpp::Error::Runtime(rt) => rt.render(),
            jpp::Error::Check(r) => r.render(),
        })
}

fn 非文字材料() -> Mat {
    let mut m = Mat::literal(serde_json::json!({"data": "..."}));
    m.modality = "image".into();
    m.taint = Taint::Trusted;
    m
}

/// text_only 未测（默认画像）：报 E-modality。
#[test]
fn text_only未测时报() {
    let err = 跑(Profile::untested()).expect_err("应当拒绝");
    assert!(err.contains("E-modality"), "{err}");
}

/// text_only: true：报 E-modality。
#[test]
fn text_only为真时报() {
    let profile = Profile::untested().with_text_only(true, "test");
    let err = 跑(profile).expect_err("应当拒绝");
    assert!(err.contains("E-modality"), "{err}");
}

/// text_only: false 且模态在 modalities_accepted 内：放行。
#[test]
fn text_only为假且模态在列表内_放行() {
    let profile = Profile::untested()
        .with_text_only(false, "test")
        .with_modalities_accepted(vec!["image".into()], "test");
    跑(profile).expect("应当放行");
}

/// text_only: false 但模态不在 modalities_accepted 内：仍报 E-modality。
#[test]
fn text_only为假但模态不在列表内_仍报() {
    let profile = Profile::untested()
        .with_text_only(false, "test")
        .with_modalities_accepted(vec!["audio".into()], "test");
    let err = 跑(profile).expect_err("应当拒绝");
    assert!(err.contains("E-modality"), "{err}");
}
