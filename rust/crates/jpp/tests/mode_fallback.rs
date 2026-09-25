//! **模式键不在查找链上（B44，步 11b-2）**。
//!
//! 原先（`12`:136 旧文）题级与题式级都没有上岗记录时，借模式级「上岗」记录直接切出口
//! （`W-mode-prior`）。B44 裁定：模式级记录只作 `commission` 的先验输入，不是认证线；
//! 查找链是 题键 → 题式键 → 冷（类键一级见 B34，步 20）。本文件原来钉的是回退行为，
//! 步 11b-2 起改钉「不回退」：有模式级记录也仍是冷，且不留来源。

use jpp::effects::{CalibStore, LiteralMode};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

mod 桩 {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::value::Answer;
    /// 判断恒给 `p`，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
    pub fn 定值端口(p: f64) -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("fixed-0", move |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
                Err(EffectError("不该 gen".into()))
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| {
                Err(EffectError("不该 ask".into()))
            }))
    }
}

/// handler 两臂都把「线是哪来的」读出来——**它对 handler 可见，不只进 trace**。
const 程序: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let s = state(mat("对象"));
let e = cut(judge(s, test("行吗", "题级.没测过")));
handle(e, {
  act: fn() { {结果: "act", 线源: line_source(e)} },
  ignore: fn() { {结果: "ignore", 线源: line_source(e)} },
  unsure: fn(u) { consume(u, "drop"); {结果: unsure_cause(u), 线源: line_source(e)} }
})
"#;

fn 跑(calib: &CalibStore, p: f64) -> serde_json::Value {
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let mut ledger = Ledger::new();
    run(
        &program,
        桩::定值端口(p),
        calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("跑得完")
    .value_json()
}

/// **B 的红**：题级冷、模式级也没有 → 还是 `Unsure(cold)`。这一条是对照臂，
/// **它证明回退没有把所有冷键都放行**。
#[test]
fn 题级冷且模式级也没有时仍然冷() {
    let v = 跑(&CalibStore::new(), 0.95);
    assert_eq!(v["结果"], "cold", "两级都没线，就该还是冷：{v}");
    assert_eq!(v["线源"], "", "没有线可用，就不该谎称有来源");
}

/// **B44：题级冷时不借模式级。** 模式级有上岗记录，出口仍是冷，且不留来源。
/// （步 11b-2 前这里断言 `act` 与 `模式级·手填`，是回退行为。）
#[test]
fn 题级冷时不借模式级() {
    let mut calib = CalibStore::new();
    // 这一类题（noul + 默认字面模式）的记录：只作先验，不供线
    calib
        .put(
            &CalibStore::mode_key("noul", LiteralMode::default()),
            0.80,
            0.20,
            200,
            "上岗",
            Some(0.05),
        )
        .expect("写得进");

    let v = 跑(&calib, 0.95);
    assert_eq!(v["结果"], "cold", "模式级记录不供线（B44）：{v}");
    assert_eq!(v["线源"], "", "没有用上线就不留来源");

    let v = 跑(&calib, 0.5);
    assert_eq!(v["结果"], "cold");
    assert_eq!(v["线源"], "");
}

/// **题级有线时不回退**：自己的线优先，来源是题级。
#[test]
fn 题级有线时用自己的() {
    let mut calib = CalibStore::new();
    calib
        .put("题级.没测过", 0.90, 0.10, 50, "上岗", Some(0.05))
        .expect("写得进");
    calib
        .put(
            &CalibStore::mode_key("noul", LiteralMode::default()),
            0.30,
            0.20,
            200,
            "上岗",
            Some(0.05),
        )
        .expect("写得进");
    // 0.5 在题级线（0.90/0.10）的带内；若错用了模式级线（0.30）就会变成 act
    let v = 跑(&calib, 0.5);
    assert_eq!(v["结果"], "band", "该用题级的线：{v}");
    assert_eq!(
        v["线源"], "题级·手填",
        "**层级与凭据是两件正交的事，都要说**"
    );
}

/// **硬边界：`停岗` 不得被模式级的线救回来。**
/// 停岗是人下的判断（这条线不能再用了），**回退到一个类级先验把它放行，是彻头彻尾的假放行**。
#[test]
fn 停岗不被模式级救回() {
    let mut calib = CalibStore::new();
    calib
        .put("题级.没测过", 0.90, 0.10, 50, "停岗", None)
        .expect("写得进");
    calib
        .put(
            &CalibStore::mode_key("noul", LiteralMode::default()),
            0.80,
            0.20,
            200,
            "上岗",
            Some(0.05),
        )
        .expect("写得进");
    let v = 跑(&calib, 0.95);
    assert_ne!(v["结果"], "act", "停岗的题不该因为同类有线就放行：{v}");
    assert_eq!(v["线源"], "", "停岗时不回退，也就没有来源可留");
}

/// **模式级的冷记录不得注入假线**：`get` 命不中时合成的记录带着 `0.65/0.35`，
/// 直接读 `rec.hi`/`rec.lo` 就会把它当成一条线。必须走 `lines_for`（只认上岗）。
#[test]
fn 模式级的冷记录不算线() {
    let mut calib = CalibStore::new();
    // 模式级那一格存在但不是上岗
    calib
        .put(
            &CalibStore::mode_key("noul", LiteralMode::default()),
            0.80,
            0.20,
            5,
            "待真值",
            None,
        )
        .expect("写得进");
    let v = 跑(&calib, 0.95);
    assert_eq!(v["结果"], "cold", "模式级没上岗 = 没有线可借：{v}");
    assert_eq!(v["线源"], "");
}

/// 模式键按**物理形式**分格：`12` 自己的 `delta_for` 就按 `op.phys()` 分，
/// 一把尺子上的线不能给另一把尺子用（本项目「不同尺不可比」的判据）。
#[test]
fn 模式键按物理形式与字面模式分格() {
    let a = CalibStore::mode_key("noul", LiteralMode::default());
    let b = CalibStore::mode_key("choice", LiteralMode::default());
    let c = CalibStore::mode_key("noul", LiteralMode::CodeLiteral);
    assert_ne!(a, b, "noul 与 choice 不同尺");
    assert_ne!(a, c, "字面模式是键的一维（12:136 第五维）");
    // 模式键落在保留命名空间里，不会与任何题级键名相撞
    assert!(a.starts_with('\u{1f}'), "{a}");
}

/// 为 `Answer` 留一个编译期用到的引用，免得 import 被判无用。
#[allow(dead_code)]
fn _用到answer(a: &Answer) -> bool {
    matches!(a, Answer::Noul(_))
}

/// **回归：`待真值` 的记录不得供线。**
///
/// A 部分的 `absorb` 会把冷记录推到 `待真值`。而 `get` 命不中时合成的记录带着
/// `0.65/0.35` 这个**缺省值**——若 `待真值` 照样走过线比较，那么「程序积累了几条
/// 无标注观察」就会**静默地**把一个原本 `Unsure(cold)` 的键变成 0.65 就放行。
/// **积累证据不该改变判定**，这是 A 与 B 交界处的坑，两包合起来才看得见。
#[test]
fn 待真值不供线() {
    use jpp::effects::Sample;
    let mut calib = CalibStore::new();
    calib
        .absorb(
            "题级.没测过",
            Sample {
                p: Some(0.9),
                label: None,
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .expect("折得进");
    assert_eq!(calib.get("题级.没测过").status, "待真值");

    let v = 跑(&calib, 0.95);
    assert_eq!(
        v["结果"], "cold",
        "**积累无标注证据不得把冷键变成会放行的键**：{v}"
    );
    assert_eq!(v["线源"], "");
}
