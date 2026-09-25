//! 校准键的维度（`12`:136）：
//! `calib_key = (q_text_hash, slot_kinds, phys, render_version, literal_mode)`。
//!
//! **`literal_mode` 此前全树零命中**，`cut` 用的是扁平字符串 `r.calib`。
//!
//! **为什么现在比昨天急**：置换一致率刚在 Rust 路径上第一次可测，**那个数要记在某个校准键上**。
//! 键缺一维的后果与 `judge_key` 缺 `site` **完全同族**：不同 `literal_mode` 下的值
//! 合进同一格、第二个覆盖第一个，**而它长得像一次观察**。
//! **在那个数被真正记下来之前补上，比补上之后再迁移便宜。**

use jpp::effects::{CalibStore, LiteralMode};

#[test]
fn 不同literal_mode是不同的校准格() {
    let mut c = CalibStore::new();
    // 同一个键名、不同的字面模式：判代码字面 vs 判文档段落
    c.put_moded(
        "k",
        LiteralMode::CodeLiteral,
        0.70,
        0.30,
        100,
        "上岗",
        Some(0.05),
    )
    .expect("写得进");
    c.put_moded(
        "k",
        LiteralMode::DocSection,
        0.55,
        0.45,
        80,
        "上岗",
        Some(0.05),
    )
    .expect("写得进");

    let 代码 = c.get_moded("k", LiteralMode::CodeLiteral);
    let 文档 = c.get_moded("k", LiteralMode::DocSection);
    assert_eq!(
        (代码.hi, 代码.lo, 代码.n),
        (0.70, 0.30, 100),
        "两格各是各的，第二个不该覆盖第一个"
    );
    assert_eq!((文档.hi, 文档.lo, 文档.n), (0.55, 0.45, 80));

    // 没写过的那一档仍是冷的——**不继承别的模式的线**
    let 执行输出 = c.get_moded("k", LiteralMode::ExecOutput);
    assert_eq!(
        执行输出.status, "冷",
        "别的模式测过，不等于这一档测过：{:?}",
        执行输出
    );
}

/// 老接口（不带模式）仍然可用，取**默认档**——旧程序一个字不用改。
#[test]
fn 不带模式的老接口仍可用() {
    let mut c = CalibStore::new();
    c.put("k", 0.65, 0.35, 100, "上岗", Some(0.05))
        .expect("写得进");
    assert_eq!(c.get("k").hi, 0.65);
    assert_eq!(
        c.get_moded("k", LiteralMode::default()).hi,
        0.65,
        "老接口写的就是默认档"
    );
}

/// **键要能从值看出维度**：两个模式的键字符串不同，否则序列化后又合回一格。
#[test]
fn 键字符串带得上模式() {
    let a = CalibStore::keyed("k", LiteralMode::CodeLiteral);
    let b = CalibStore::keyed("k", LiteralMode::DocSection);
    let d = CalibStore::keyed("k", LiteralMode::default());
    assert_ne!(a, b, "不同模式要给出不同的键");
    assert_eq!(d, "k", "默认档就是裸键名——老账本读得回来");
    assert!(a.contains("k"), "键里要看得见原来的名字：{a}");
}
