//! 账本 v2 的格式契约（步 7，格式步）。依据：`20` §2.3 `jpp-ledger`、§九 账本行；`21` §三·4 步 7、E5；B61。

use jpp::ledger::{EffectKey, Entry, Header, JudgeKey, Ledger, V1_ARCHIVE_TAG, judge_key};
use jpp::value::Answer;

fn 样本() -> Ledger {
    let mut l = Ledger::new();
    l.set_header(Header::new(10, 1.0, "m", "r1", "h").with_calib_hash(Some("c".into())));
    let jk = JudgeKey::new("m", "s", "q", "noul", 0, 0, 7);
    assert_eq!(
        jk.digest(),
        judge_key("m", "s", "q", "noul", 0, 0, 7),
        "结构化键的哈希与旧键函数逐字节相同"
    );
    l.put(Entry::judge(jk.digest(), Answer::Noul(0.9), 3, 0.5, "m", 1));
    l.put(Entry::effect(
        EffectKey::new("gen", &["7", "p"]),
        "gen",
        serde_json::json!(["x"]),
        0.1,
    ));
    l.put(Entry::Ask {
        key: "ask1".into(),
        ekey: None,
        answer: None,
    });
    l
}

/// 记录—重放逐字节（§二·2 第 2 条的格式面）：编码、解码、再编码，字节相同。
#[test]
fn 往返逐字节() {
    let l = 样本();
    let text = l.encode();
    let (back, t) = Ledger::decode(&text).expect("解码");
    assert!(t.is_none());
    assert_eq!(back.encode(), text);
    assert_eq!(back.entries.len(), 3);
    assert!(
        back.get(&JudgeKey::new("m", "s", "q", "noul", 0, 0, 7).digest())
            .is_some(),
        "索引随解码重建"
    );
}

/// 末行半写（进程在写最后一条时退出）：截断到最后一条完整条目并报告，不拒绝整本。
#[test]
fn 尾行半写截断() {
    let text = 样本().encode();
    let cut = &text[..text.len() - 10]; // 去掉末行尾部（含换行）
    let (back, t) = Ledger::decode(cut).expect("截断而不是拒绝");
    let t = t.expect("要报告截断");
    assert_eq!(back.entries.len(), 2);
    assert_eq!(t.kept, 2);
    assert!(t.render().starts_with("W-ledger-truncated"));
}

/// 中间一行被改：链断，拒绝并指出行号。
#[test]
fn 链断拒绝() {
    let text = 样本().encode();
    let lines: Vec<&str> = text.lines().collect();
    let tampered = lines[1].replace("0.9", "0.1");
    let bad = format!("{}\n{}\n{}\n{}\n", lines[0], tampered, lines[2], lines[3]);
    let e = Ledger::decode(&bad).expect_err("改过的中间行要被拒");
    assert!(e.starts_with("E-ledger-corrupt"), "{e}");
    assert!(e.contains("第 3 行"), "指出第一处接不上的行：{e}");
}

/// 未知字段不无声吞掉（`12` §2.11 通则）。
#[test]
fn 未知字段拒绝() {
    let text = 样本().encode();
    // 账本 v3（步 18a）：版本号随格式改为 3
    let bad = text.replacen("\"version\":3,", "\"version\":3,\"extra\":1,", 1);
    let e = Ledger::decode(&bad).expect_err("头行多一个字段要被拒");
    assert!(e.starts_with("E-ledger-corrupt"), "{e}");
}

/// v2 账本（步 7 至 18c）不直接读：报 E-ledger-v2 并给出迁移方法与归档标签（步 18a，B124 Q3）。
#[test]
fn v2_报迁移() {
    let v2 = "{\"version\":2,\"header\":null,\"calib_used\":{}}\n";
    let e = Ledger::decode(v2).expect_err("v2 不直接读");
    assert!(e.starts_with("E-ledger-v2"), "{e}");
    assert!(
        e.contains("ledger-migrate") && e.contains(jpp::ledger::V2_ARCHIVE_TAG),
        "{e}"
    );
}

/// v1 账本（整份 JSON）不迁移：报 E-ledger-archived 并指出归档标签。
#[test]
fn v1_报归档() {
    let v1 = r#"{
  "header": null,
  "entries": [],
  "header_warning": null
}
"#;
    let e = Ledger::decode(v1).expect_err("v1 不读");
    assert!(e.starts_with("E-ledger-archived"), "{e}");
    assert!(e.contains(V1_ARCHIVE_TAG), "{e}");
}

/// B61：预算记录但不比对。续接时预算变大不报 W-header；比对字段变了照报。
#[test]
fn 预算不比对() {
    let mut l = Ledger::new();
    l.set_header(Header::new(10, 1.0, "m", "r1", "h"));
    l.set_header(Header::new(99, 9.0, "m", "r1", "h"));
    assert!(l.header_warning.is_none(), "只改预算不报");
    l.set_header(Header::new(99, 9.0, "m2", "r1", "h"));
    let w = l.header_warning.take().expect("换模型要报");
    assert!(w.contains("model_id 旧 m 新 m2"), "{w}");
}

/// 已问未答入账；续跑得到答案另起一条（只增），索引指向已答那条；其余同键写入仍不覆盖。
#[test]
fn 已问未答续答只增() {
    let mut l = 样本();
    l.put_answer(Entry::Ask {
        key: "ask1".into(),
        ekey: None,
        answer: Some(Answer::Noul(1.0)),
    });
    assert_eq!(l.entries.len(), 4, "另起一条，不改旧条目");
    assert!(matches!(
        l.get("ask1"),
        Some(Entry::Ask {
            answer: Some(_),
            ..
        })
    ));
    l.put_answer(Entry::Ask {
        key: "ask1".into(),
        ekey: None,
        answer: Some(Answer::Noul(0.0)),
    });
    assert_eq!(l.entries.len(), 4, "已答之后同键不再写");
    let (back, _) = Ledger::decode(&l.encode()).unwrap();
    assert!(
        matches!(back.get("ask1"), Some(Entry::Ask { answer: Some(Answer::Noul(p)), .. }) if *p == 1.0),
        "解码后索引仍指向已答条目"
    );
}
