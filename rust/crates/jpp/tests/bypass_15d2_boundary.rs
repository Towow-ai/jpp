//! 步 15d-2：已决区边界统一。认证判「已决」用 `p ≥ h`，记录存 `hi = h − δ`，`cut` 判 `p ≥ hi + δ`：
//! 浮点下 `(h − δ) + δ` 可能比 h 大 1ulp，读数恰好等于 h 时运行期判成 band（27a F3 实测 5 条）。
//! 统一后认证、`unsure_rate`、复核已决区与 `cut` 共用 `jpp_value::stat::decided_up/down`（带往返容差）。
//! 这里用性质测试钉住：在大量随机标注集上认证，**认证检验过的已决集合与 `cut` 的已决集合逐条相同**，
//! 记录的 `unsure_rate` 等于按 `cut` 逐条算出的未决占比。依据：主会话 2026-09-25（15d-2 追加）。

use jpp::effects::{CalibStore, CertGrade, LiteralMode, Sample};
use jpp_value::bridge::{CutInput, decide};
use jpp_value::value::{Answer, ExitKind};

fn 样本(p: f64, label: bool, phys: &str) -> Sample {
    Sample {
        p: Some(p),
        label: Some(u8::from(label)),
        perms: 0,
        mode_share: None,
        mode: LiteralMode::default(),
        phys: phys.into(),
        cluster: None,
        stratum: None,
    }
}

/// splitmix64：可复现的伪随机
fn 下一个(s: &mut u64) -> u64 {
    *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D1_049B_133A_11EB);
    z ^ (z >> 31)
}

/// 三位小数的读数（与真机读数精度相同，读数正好落在候选线上的情形常见）
fn 读数(s: &mut u64, lo: u64, hi: u64) -> f64 {
    (lo + 下一个(s) % (hi - lo + 1)) as f64 / 1000.0
}

fn cut的出口(hi: f64, lo: f64, delta: f64, p: f64) -> ExitKind {
    decide(&CutInput {
        fail: None,
        absent: None,
        suspended: false,
        line: Some((hi, lo)),
        cost_requested: false,
        answer: Some(Answer::Noul(p)),
        delta: Some(delta),
        mode_share: None,
    })
    .0
}

/// 两侧固定序：认证已决集合（p ≥ h 或 p ≤ l，h、l 取样本里等于线的那个读数）= cut 的已决集合；
/// `unsure_rate` = cut 逐条未决占比。δ 取会产生往返误差的 0.0781（27a 画像的 choice δ）与 0.05。
#[test]
fn 两侧认证与cut的已决集合逐条相同() {
    let mut 种子 = 20260925u64;
    let mut 认证过 = 0;
    let mut 边界读数 = 0;
    for 轮 in 0..400 {
        let delta = if 轮 % 2 == 0 { 0.0781 } else { 0.05 };
        let mut c = CalibStore::new();
        let n = 40 + (下一个(&mut 种子) % 40) as usize;
        for _ in 0..n {
            // 正例多在高端、负例多在低端，少量越界
            let 正 = 下一个(&mut 种子).is_multiple_of(2);
            let p = if 正 {
                读数(&mut 种子, 550, 999)
            } else {
                读数(&mut 种子, 1, 450)
            };
            c.absorb("k", 样本(p, 正, "noul")).unwrap();
        }
        c.set_delta("k", delta).unwrap();
        if c.commission_two_sided_fixed_sequence_graded("k", 0.1, 0.1, None, CertGrade::Formal)
            .is_err()
        {
            continue;
        }
        认证过 += 1;
        let r = c.get("k");
        let 带标注: Vec<f64> = r
            .samples
            .iter()
            .filter(|s| s.label.is_some())
            .filter_map(|s| s.p)
            .collect();
        // 认证的 h、l：认证在读数（或相邻读数中点）上选线，hi = h − δ
        let (h, l) = (r.hi + delta, r.lo - delta);
        let mut 未决 = 0usize;
        for p in &带标注 {
            let 认证已决 = *p >= h - 1e-12 || *p <= l + 1e-12;
            if (p - h).abs() < 1e-9 || (p - l).abs() < 1e-9 {
                边界读数 += 1;
            }
            let e = cut的出口(r.hi, r.lo, delta, *p);
            let cut已决 = matches!(e, ExitKind::Act | ExitKind::Ignore);
            assert_eq!(
                认证已决, cut已决,
                "轮 {轮}：p={p} h={h} l={l} δ={delta} 认证已决={认证已决} cut 出口 {e:?}"
            );
            未决 += usize::from(!cut已决);
        }
        let u = r.unsure_rate.expect("认证会测 unsure 率");
        // 记录的率按四位小数存
        let 算 = (未决 as f64 / 带标注.len() as f64 * 10000.0).round() / 10000.0;
        assert!(
            (u - 算).abs() < 1e-12,
            "轮 {轮}：记录的 unsure_rate {u} 与 cut 逐条算的 {} 不同",
            未决 as f64 / 带标注.len() as f64
        );
    }
    assert!(认证过 > 100, "性质测试要覆盖足够多的认证：{认证过}");
    assert!(边界读数 > 50, "要有足够多正好落在线上的读数：{边界读数}");
}

/// 往返误差的具体例子：h = 0.58、δ = 0.0781 时 (h − δ) + δ = 0.5800000000000001 > h。
/// 读数正好 0.58 属于认证的已决区，cut 也要判已决（修前判 band）。
#[test]
fn 读数恰在线上判已决() {
    let (h, delta) = (0.58f64, 0.0781f64);
    let hi = h - delta;
    assert!(hi + delta > h, "前提：这组数有往返误差");
    assert_eq!(cut的出口(hi, 0.0, delta, 0.58), ExitKind::Act);
    assert!(matches!(
        cut的出口(hi, 0.0, delta, 0.579),
        ExitKind::Unsure(_)
    ));
}
