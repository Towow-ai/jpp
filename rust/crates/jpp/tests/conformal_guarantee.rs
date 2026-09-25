//! **保形那一族的独立来源不是第二个实现，是那条数学保证本身。**
//!
//! 背景（上一包那张表的最后一行）：`binomial_upper` / `certify` / `drift` /
//! `n_needed_zero_error` / `cluster_subsample` **Python 一个都没有**
//! ——**它们的断言至今全是照着实现写的，而差分法救不了它们。**
//! **而这一族正是「长处」那一侧。**
//!
//! **所以改用定义与保证当来源**：
//! - `binomial_upper`：**按定义反查**——它返回的 `p*` 要满足 `P(X ≤ k | n, p*) = δ`。
//!   这里用**另一种写法**算那个 CDF（阶乘直算，不是生产代码的递推），**算的是定义不是实现**。
//! - `certify`：一个合成分布下的蒙特卡洛回归，不能证明一般覆盖率。
//!   当前扫描复用选线样本，未实现选择校正或独立验证集。

use jpp::conformal::{
    Certificate, binomial_upper, certify, cluster_subsample, drift, n_needed_zero_error,
};

#[test]
fn binomial_upper_large_samples_do_not_underflow() {
    // For k=n-1, P(X<=k)=1-p^n, so the upper endpoint has a closed form.
    // Starting a recurrence at P(X=0) underflows before reaching these terms.
    for n in [1000usize, 10000] {
        let expected = (1.0_f64 - 0.05).powf(1.0 / n as f64);
        assert!((binomial_upper(n - 1, n, 0.05) - expected).abs() < 1e-10);
    }
    let upper = binomial_upper(900, 1000, 0.05);
    assert!(
        upper > 0.91 && upper < 0.92,
        "unexpected upper bound: {upper}"
    );
}

/// 可复现的线性同余，**不引第三方**
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// `P(X ≤ k | n, p)`，**用阶乘直算**——与 `binomial_upper` 内部的递推是两种写法。
fn cdf(k: usize, n: usize, p: f64) -> f64 {
    let ln_fact = |m: usize| (1..=m).map(|i| (i as f64).ln()).sum::<f64>();
    (0..=k)
        .map(|i| {
            let ln_c = ln_fact(n) - ln_fact(i) - ln_fact(n - i);
            (ln_c + i as f64 * p.ln() + (n - i) as f64 * (1.0 - p).ln()).exp()
        })
        .sum()
}

/// **`binomial_upper` 按定义反查。**
///
/// 覆盖理由：`k = 0`（零错，最常用的那一格）、`k` 接近 `n`（界该逼近 1）、
/// 小 `n`（保形在我们这里的真实规模：6–73）、两个 δ。
/// **挑的是「定义的边界情形」，不是多跑几组。**
#[test]
fn binomial_upper满足它自己的定义() {
    let mut 坏 = vec![];
    for n in [5usize, 7, 20, 73] {
        for k in [0usize, 1, n / 2] {
            if k >= n {
                continue;
            }
            for delta in [0.10f64, 0.05] {
                let p = binomial_upper(k, n, delta);
                let c = cdf(k, n, p);
                // 定义：`p*` 是使 `P(X ≤ k | n, p*) = δ` 的那个 p
                if (c - delta).abs() > 2e-3 {
                    坏.push(format!(
                        "n={n} k={k} δ={delta}: 返回 p={p:.6}，而 P(X≤k|n,p)={c:.6}（该 ≈ δ）"
                    ));
                }
            }
        }
    }
    assert!(
        坏.is_empty(),
        "**{} 组不满足定义**：\n  {}",
        坏.len(),
        坏.join("\n  ")
    );
    // `k >= n` 与 `n == 0` 是承重的边界：空放行区的风险**没有定义**，返回 1.0 是「往拒绝倒」
    assert_eq!(binomial_upper(0, 0, 0.1), 1.0, "n=0 必须给 1.0，不是 0.0");
    assert_eq!(binomial_upper(5, 5, 0.1), 1.0, "全错时上界是 1");
    println!("binomial_upper：按定义反查 20 组，全部满足 P(X≤k|n,p*) ≈ δ ✓");
}

/// **`n_needed_zero_error` 有闭式，手算几个点。** `ln δ / ln(1−α)` 向上取整。
#[test]
fn n_needed有闭式手算对得上() {
    // α=0.10 δ=0.10：ln0.1/ln0.9 = 21.85 → 22（设计文里那个 22 就是它）
    assert_eq!(n_needed_zero_error(0.10, 0.10), 22);
    // α=0.05 δ=0.05：ln0.05/ln0.95 = 58.4 → 59
    assert_eq!(n_needed_zero_error(0.05, 0.05), 59);
    // α=0.5 δ=0.5：ln0.5/ln0.5 = 1
    assert_eq!(n_needed_zero_error(0.5, 0.5), 1);
}

/// `certify` 在单一合成分布上的回归检查，不是一般风险保证。
///
/// **造数**：读数 `p ~ U(0,1)`，标签 `Bernoulli(p)`（**完美校准**，
/// 于是放行区 `[t,1]` 上的**真实**错误率有闭式 `(1−t)/2`）。
/// 记录选线后「真实错误率 > 逐阈值 ucb」的比例。条件于成功选线的比例
/// 也不同于固定阈值的无条件覆盖率；本测试不能证明 δ 级保证。
///
/// **重复次数 300、δ=0.10、n=60、α=0.45**，判据放到 **2δ**——
/// **因为 300 次重复本身有 ±1.7% 的噪声，而我要区分的是「略超」与「结构性超」。**
/// **这个判据是我定的，分岔时先怀疑它。**
#[test]
fn certify在指定合成分布上的回归() {
    let (重复, delta, n, alpha) = (300usize, 0.10f64, 60usize, 0.45f64);
    let (mut 认证次数, mut 违反次数) = (0usize, 0usize);
    let mut rng = Rng(12345);
    for _ in 0..重复 {
        let s: Vec<(f64, bool)> = (0..n)
            .map(|_| {
                let p = rng.next();
                (p, rng.next() < p)
            })
            .collect();
        if let Certificate::Line { hi, ucb, .. } = certify(&s, alpha, delta) {
            认证次数 += 1;
            let 真实错误率 = (1.0 - hi) / 2.0; // E[1−p | p ≥ hi]
            if 真实错误率 > ucb {
                违反次数 += 1;
            }
        }
    }
    let 违反率 = 违反次数 as f64 / 认证次数.max(1) as f64;
    println!(
        "certify 覆盖率：{重复} 次重复，认证成功 {认证次数} 次，真实错误率超过 ucb 的 {违反次数} 次 = {:.3}（δ={delta}）",
        违反率
    );
    assert!(
        认证次数 > 重复 / 2,
        "α=0.45 在这个造数下该常常认得动，否则测的不是覆盖率：{认证次数}/{重复}"
    );
    assert!(
        违反率 <= 2.0 * delta,
        "**违反率 {违反率:.3} 超过 2δ={:.2}**——要么实现的上界不成立，要么我这个判据设计错了。\
             **先别自裁：扫阈值本身是一次多重比较，而 Clopper–Pearson 是按单次算的。**",
        2.0 * delta
    );
}

/// **`drift` 的性质检验**（没有第二实现，检性质）。
#[test]
fn drift的三条性质() {
    let a: Vec<f64> = (0..200).map(|i| i as f64 / 200.0).collect();
    // 一、同分布 → KS ≈ 0
    let d = drift(&a, &a, 10);
    assert!(d.ks < 1e-9, "同分布 KS 该是 0：{}", d.ks);
    // **200v200 同分布：功效充足、没有漂移**——这两件事不许压成一位
    assert!(!d.significant, "没漂就不该显著");
    assert!(
        !d.underpowered,
        "**200 条对 200 条不是功效不足**（原来的 `underpowered` 在这里报 true）"
    );
    assert!(!d.可停岗());
    // 二、已知移位 → KS 要明显
    let b: Vec<f64> = a.iter().map(|x| (x + 0.5).min(1.0)).collect();
    let d2 = drift(&a, &b, 10);
    assert!(d2.ks > 0.4, "整体右移 0.5，KS 该大：{}", d2.ks);
    assert!(!d2.underpowered, "各 200 条，功效够");
    // 三、**我第一版这条写错了**：4v4 完全分离的 KS=1 > crit=0.962，**那是统计上正确的显著**
    //（精确检验 p = 2/C(8,4) = 0.029）。**实现没错，是我的检验设计错了。**
    let 四 = drift(&a[..4], &b[..4], 10);
    assert!(
        四.significant && !四.underpowered,
        "4v4 完全分离确实显著：ks={}",
        四.ks
    );
    // 真正的功效不足：**连完全分离都不显著**，那要 n ≤ 3
    let 三 = drift(&a[..3], &b[..3], 10);
    assert!(
        三.underpowered,
        "3v3 时 crit>1，完全分离都不显著——那才叫功效不足"
    );
    assert!(!三.可停岗(), "功效不足不构成停岗依据");
}

/// **`cluster_subsample` 的性质检验。**
#[test]
fn cluster_subsample的三条性质() {
    let s: Vec<(f64, bool, String)> = (0..30)
        .map(|i| (i as f64 / 30.0, i % 3 == 0, format!("seg{}", i % 5)))
        .collect();
    // 一、每簇恰一个
    let a = cluster_subsample(&s, 7);
    assert_eq!(a.len(), 5, "5 个簇就该取 5 条");
    // 二、同种子可复现
    assert_eq!(a, cluster_subsample(&s, 7));
    // 三、跨种子会取到不同的代表（否则那个「重采样 200 次」什么也没采）
    let 不同 = (0..50u64)
        .map(|sd| {
            cluster_subsample(&s, sd)
                .iter()
                .map(|(p, _)| format!("{p:.6}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        不同.len() > 1,
        "50 个种子该采出不止一种组合，否则重采样是空转"
    );
    println!(
        "cluster_subsample：5 簇各取 1，同种子可复现，50 个种子采出 {} 种组合 ✓",
        不同.len()
    );
}
