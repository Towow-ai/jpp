//! 档案加载路径：线与 δ 从**档案**来，不是代码常数（`12` §1「凡是数字都是档案字段」）。
//!
//! 在这之前 `CalibStore::new()` 恒给 `Profile::default()`（代码兜底值），而 Python 从
//! `foundation/profile/profiles/*.json` 加载。两边因此算出**不同的线与 δ**——同一个程序
//! 在两个内核上会切出不同的出口，**而且不报错**。这不是「少一个功能」，是正确性分叉：
//! 我们全部对照移植判据都建在「与 Python 一致」上。
//!
//! Python 是 oracle，基准在 `tests/oracle/oracle.json` 的 `profile_oracle`
//! （由 `tests/oracle/generate.py` 生成，改了 Python 侧就重跑它）。

use std::path::{Path, PathBuf};

use jpp::effects::{CalibStore, Profile};
use serde_json::Value as Json;

fn oracle() -> Json {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/oracle.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("对照基准在")).expect("合法 JSON")
}

fn 档案路径() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json")
}

/// 同一份档案 JSON，Rust 与 Python 算出**同一组线与 δ**。
#[test]
fn 从档案读出的线与delta与python一致() {
    let path = 档案路径();
    if !path.exists() {
        println!("跳过：读不到 {}（发布目录不带档案）", path.display());
        return;
    }
    let want = &oracle()["profile_oracle"];
    let p = Profile::load(&path).unwrap_or_else(|e| panic!("档案该加载得了：{e}"));

    let s = want["safety_lines"].as_array().unwrap();
    assert_eq!(
        p.safety.get().copied(),
        Some((s[0].as_f64().unwrap(), s[1].as_f64().unwrap())),
        "冷键的保守线要从档案 lines.safety_default 来"
    );

    let d = &want["delta"];
    assert_eq!(
        p.delta.get().copied(),
        Some((
            d["noul"].as_f64().unwrap(),
            d["choice"].as_f64().unwrap(),
            d["score"].as_f64().unwrap()
        )),
        "δ 要从档案 delta.<题式>.immediate.p99 来（choice 走 choice_prob_chosen）"
    );

    // 步 15d-2：没有代码兜底值可比（`Profile::default` 已删）；未测画像两项都是未测
    let 未测 = Profile::untested();
    assert!(未测.safety.get().is_none() && 未测.delta.get().is_none());
}

/// 账本头要带 `profile_hash`（`12` §J-18：账本头不同即不承诺重放一致）。
/// 换了档案重放，现在**察觉不到**——`profile_hash` 是「换了模型档案」这件事的唯一痕迹。
#[test]
fn 账本头带得上profile_hash且与python同值() {
    let path = 档案路径();
    if !path.exists() {
        return;
    }
    let want = oracle();
    let p = Profile::load(&path).expect("加载得了");
    assert_eq!(
        p.hash.as_deref(),
        want["profile_oracle"]["profile_hash"].as_str(),
        "profile_hash 要与 Python 的 H(profile) 同值：sha256(canon([profile]))[:16]"
    );
}

/// **加载不到档案时不许悄悄用默认值继续**——那正是「替不确定说确定」。
/// 要么报错，要么明确标记「本次未加载档案」并让它进账本头，使重放能看见。
/// 这里选后者：`Profile::untested()`（步 15d-2 起唯一默认）的 `hash` 是 `None`，账本头记成 `"<未加载档案>"`。
#[test]
fn 没加载档案时要留下痕迹而不是装作有() {
    let 兜底 = Profile::untested();
    assert!(兜底.hash.is_none(), "未测画像没有哈希——它不是一份档案");

    let 读不到 = Profile::load(Path::new("/不存在的档案.json"));
    assert!(读不到.is_err(), "读不到档案要报错，不是悄悄回退到兜底值");

    // CalibStore::new() 仍给兜底（测试与不接档案的调用方要用），但它的痕迹留在 hash 上
    let c = CalibStore::new();
    assert!(
        c.profile.hash.is_none(),
        "没接档案的 CalibStore 要能看出来没接"
    );
}

/// 换了档案重放要**察觉得到**（`12` §J-18：账本头不同即不承诺重放一致）。
/// 以前察觉不到——头里根本没有档案这一项。
#[test]
fn 换档案重放会报w_header() {
    use jpp::ledger::{Header, Ledger};

    let 头 = |h: Option<&str>| {
        Header::new(10, 1.0, "m", "r", "h").with_profile_hash(h.map(|x| x.into()))
    };
    let mut l = Ledger::new();
    l.set_header(头(Some("档案甲")));
    assert!(l.header_warning.is_none(), "第一次写头不该告警");

    l.set_header(头(Some("档案乙")));
    let w = l.header_warning.take().expect("换了档案要报 W-header");
    assert!(w.contains("W-header"), "{w}");

    // 从「有档案」变成「没加载档案」同样要察觉得到——这正是兜底值留痕的意义
    let mut l = Ledger::new();
    l.set_header(头(Some("档案甲")));
    l.set_header(头(None));
    assert!(
        l.header_warning.is_some(),
        "从有档案变成无档案，线与 δ 会变，必须告警"
    );
}

/// `Mat` 没有 token 计数，于是 **J-14 的窗口检查做不了**——`12`:117「窗口检查（H6）：
/// 对象槽内单段材料按 `profile.window.text_slots.usable_lower`…槽间干扰按
/// `profile.window.json_slots`。静态已知时编译期查，否则调用前查」。
///
/// 后果不是算错，是**读数被语境接管而无人察觉**：档案里写着「≈1,000 token 带主张语境下
/// 翻转 60.7%，读数被语境接管」。超窗的状态照样发出去，答案偏了也没有痕迹。
///
/// token 估法与 Python 一致（`ir.py:84` `int(len(canon)/1.3) + 1`）——**同一个估法**，
/// 否则两边的窗口检查会在不同的地方触发。
#[test]
fn mat要能数token且与python同一个估法() {
    use jpp::value::Mat;

    // Python: int(len(canon(content))/1.3) + 1
    let 文本 = Mat::literal(serde_json::json!("一二三四五"));
    // canon 出的是带引号的 JSON 字符串："一二三四五" → 17 字节（5 个三字节汉字 + 两个引号）
    let 期望 = (jpp::value::canon(&文本.content).chars().count() as f64 / 1.3) as usize + 1;
    assert_eq!(
        文本.tokens(),
        期望,
        "token 估法要与 Python 的 int(len(canon)/1.3)+1 一致"
    );

    let 长文 = Mat::literal(serde_json::json!("x".repeat(2600)));
    assert!(
        长文.tokens() > 1900,
        "2600 字符该估出 1900+ token，实际 {}",
        长文.tokens()
    );
}

/// 窗口上限要**从档案读**，不是代码常数（与线、δ 同一条：`12` §1）。
#[test]
fn 窗口上限从档案读() {
    let path = 档案路径();
    if !path.exists() {
        return;
    }
    let p = Profile::load(&path).expect("加载得了");
    // Python `text_window()` 读 window.text_slots.claim_bearing_ctx.usable_lower
    assert!(
        p.window().expect("档案测过窗口").text > 0,
        "对象槽内单段的可用窗口该从档案来，实际 {}",
        p.window().expect("档案测过窗口").text
    );
    // Python `json_ctx_window()` 取 flip_frac_by_ctx_tokens 的最大档
    assert_eq!(
        p.window().expect("档案测过窗口").json_ctx,
        1800,
        "槽间窗口取 flip_frac_by_ctx_tokens 的最大档（~1800）"
    );
}

/// **行为承载子集摘要**：纯说明改动**不该**动它，线或 δ 改动**必须**动它。
///
/// 这条要解决的痛点是实测出来的：一晚上三次**纯文档更正**把对照基准打红，
/// **行为一个字节没变**。那会造出「**改对文档要付代价**」的反向激励——
/// 而同一个数被写错三次，正是在这个激励下发生的。
///
/// 它**不放行任何东西**：`profile_hash` 不同时该怎么判还怎么判（仍报 `W-header`）。
/// 它只回答「**这次不同属于哪一种**」。
#[test]
fn 行为摘要不随说明文字变() {
    use jpp::effects::{behavior_hash, profile_hash};
    use serde_json::json;

    let 原 = json!({
        "lines": {"safety_default": {"hi": 0.75, "lo": 0.25, "note": "原来的说明", "by": "E-CAL"}},
        "delta": {"noul": {"immediate": {"p99": 0.04}}},
        "notes": "整份档案的说明"
    });
    // 只改说明文字
    let 改了说明 = json!({
        "lines": {"safety_default": {"hi": 0.75, "lo": 0.25, "note": "改写过的说明，长得多", "by": "E-CAL（三次更正）"}},
        "delta": {"noul": {"immediate": {"p99": 0.04}}},
        "notes": "整份档案的说明，也改了"
    });
    // 改了线
    let 改了线 = json!({
        "lines": {"safety_default": {"hi": 0.80, "lo": 0.25, "note": "原来的说明", "by": "E-CAL"}},
        "delta": {"noul": {"immediate": {"p99": 0.04}}},
        "notes": "整份档案的说明"
    });

    assert_ne!(
        profile_hash(&原),
        profile_hash(&改了说明),
        "整份档案的哈希对说明敏感——这是它的定义，不动它"
    );
    assert_eq!(
        behavior_hash(&原),
        behavior_hash(&改了说明),
        "**行为摘要不该随说明文字变**：行为一个字节没变"
    );
    assert_ne!(
        behavior_hash(&原),
        behavior_hash(&改了线),
        "线变了行为摘要必须变"
    );
}

/// 真档案上两个摘要都算得出，且**不相等**（它们覆盖的范围本来就不同）。
#[test]
fn 真档案上两个摘要都在() {
    let path = 档案路径();
    if !path.exists() {
        return;
    }
    let p = Profile::load(&path).expect("加载得了");
    let b = p.behavior_hash.as_deref().expect("行为摘要在");
    assert_eq!(b.len(), 16);
    assert_ne!(Some(b), p.hash.as_deref(), "覆盖范围不同，值自然不同");
}

/// 档案**缺窗口字段 = 未测**（步 15d，`20` §3.9「缺字段 = `Untested`」；此前缺即报错）。
/// 不变的是**不悄悄用兜底值**：`window()` 是 `None`，使用处按 §3.9 不核长度并报 `W-window-untested`，
/// 不回退到 1000 / 1800；兜底值只在没加载档案时（`Profile::default`，15d-2 删）出现。
#[test]
fn 档案缺窗口字段是未测() {
    use jpp::effects::Profile;
    use serde_json::json;

    let 缺窗口 = json!({
        "lines": {"safety_default": {"hi": 0.75, "lo": 0.25}},
        "delta": {"noul": {"immediate": {"p99": 0.04}},
                  "choice_prob_chosen": {"immediate": {"p99": 0.0781}},
                  "score": {"immediate": {"p99": 0.1141}}}
    });
    let p = Profile::from_json(&缺窗口).expect("缺窗口字段不报错，是未测");
    assert_eq!(p.window(), None, "未测就是未测，不补兜底值");
    assert!(p.hash.is_some(), "这是一份档案");

    // 步 15d-2：没有代码兜底窗口了，未测画像的窗口也是未测
    assert_eq!(Profile::untested().window(), None);
    assert!(
        Profile::untested().hash.is_none(),
        "未测画像不是档案，没有哈希"
    );
}

/// 画像没测过窗口时，运行期不核对象长度，每站点报一次 `W-window-untested`（步 15d，`20` §3.9 窗口行）
#[test]
fn 窗口未测时报w_window_untested() {
    let mut j: Json =
        serde_json::from_str(&std::fs::read_to_string(档案路径()).expect("档案在")).unwrap();
    j.as_object_mut().unwrap().remove("window");
    let mut calib = CalibStore::new();
    calib.profile = Profile::from_json(&j).unwrap();
    calib.put("k", 0.65, 0.35, 50, "上岗", Some(0.05)).unwrap();
    let src = "budget {calls: 2, cost: 0, depth: 8};\nlet e = cut(judge(state(mat(\"材料\")), test(\"行吗\", \"k\")));\nconsume(e, \"drop\");\n1";
    let program = jpp::lower(&jpp::syntax::parse(src).unwrap()).unwrap();
    let mut fp = jpp::effects::FixedPorts::new();
    fp.observe(
        &jpp::value::State::new(
            vec![jpp::value::Mat::literal(serde_json::json!("材料"))],
            vec![],
            vec![],
            vec![],
            false,
        ),
        &jpp::value::Question::new(jpp::value::Op::Test, "行吗", "k", vec![]),
        jpp::value::Answer::Noul(0.9),
    );
    let o = jpp::run(
        &program,
        fp.ports(),
        &calib,
        &jpp::ActionRegistry::new(),
        &mut jpp::ledger::Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    let n = o
        .trace
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-window-untested"))
        .count();
    assert_eq!(n, 1, "{:?}", o.trace.warnings);
}

/// 档案里**未知字段报错并指名**（步 15d：`profile_schema` 是唯一来源；拼错的字段会被当成未测，所以要报）
#[test]
fn 档案未知字段要报错() {
    use jpp::effects::Profile;
    let mut j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(档案路径()).expect("档案在")).unwrap();
    j["window_bounded_"] = serde_json::json!(true);
    let e = Profile::from_json(&j).expect_err("未知字段要报错");
    assert!(e.contains("window_bounded_"), "报文要指名：{e}");
}
