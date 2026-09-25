//! **赌 2 的另一半：两内核的账本键对不对得上。**
//!
//! 结论（**$0 拿到的**）：**对不上，而且不可能对得上**——
//! 两边的**分量完全一致**，**哈希构造不同**。
//!
//! | | Python | Rust |
//! |---|---|---|
//! | 分量 | `judge, model_id, state_hash, q_hash, phys, render_version, perm_seed, run_seq, site` | **逐项相同** |
//! | 构造 | `sha256(canon([分量…]))` | `sha256(分量 \u{1f} 连接)` |
//! | 长度 | **16 个十六进制字符** | **24 个** |
//!
//! **所以「观察身份」这件事上两边没有语义差——分量一模一样；差的是编码。**
//! 预注册写的是「对不上就是内核语义差」，**而这次对不上不是语义差**，
//! 修法也因此不同：不是回到对照移植逐层查，是**统一哈希构造**（带迁移代价）。
//!
//! **而同一个仓库里 `profile_hash` 两边是逐字节相同的**（`ea01589429412ed0`）——
//! **因为那一条被明确要求过必须同值，而账本键没人看过。**
//! **「被要求过的对上了，没被要求的没对上」——这不是运气，是覆盖面。**

use jpp::effects::profile_hash;
use jpp::ledger::judge_key;

/// **`profile_hash` 两边必须同值**（它进账本头，`12` §J-18 的重放判定建在它上面）。
#[test]
fn profile_hash与python逐字节相同() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p).expect("真档案在")).expect("合法 JSON");
    assert_eq!(
        profile_hash(&j),
        "ea01589429412ed0",
        "**它与 Python 的 H(profile) 必须同值**——两边算不出同一个数，跨内核的重放判定就废了"
    );
}

/// **账本键今天两边对不上，把它钉住。**
///
/// **钉的不是「它应该不一样」**——钉的是**这个已知的不一致不许被静默改掉或静默留着**。
/// 哪天有人统一了哈希构造，这条会红，**而那正是该有人看一眼的时刻**：
/// 统一会让**所有既有账本失配**，那是一次迁移，不是一次重构。
#[test]
fn 账本键与python的不一致是已知的() {
    let k = judge_key("jev-1.13.0", "sh", "qh", "noul", 0, 0, 42);
    assert_eq!(
        k.len(),
        24,
        "Rust 侧取 sha256 前 12 字节 = 24 个十六进制字符"
    );
    assert_eq!(k, "66b055d30697ec3173510075");
    const PYTHON: &str = "718d5bfb5eac67dd";
    assert_eq!(PYTHON.len(), 16, "Python 侧取前 16 个十六进制字符");
    assert_ne!(
        k, PYTHON,
        "**今天对不上**；哪天对上了，这条会红，而那时要先想清楚迁移"
    );
}
