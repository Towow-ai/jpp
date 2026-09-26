//! B155（步 15i）：候选随题走、按材料合批、候选标签与是非题标签；`RENDER_VERSION` 升 `r2`。
//!
//! 走真机端口 `JevPorts` 加本地替身传输（不发网络、$0），看线上请求体与调用数：
//! (a) 同材料两道候选集不同的 `select` 加一道 `test` → 1 次调用、3 条账本条目；
//! (b) 线上 `state` 无 `over`，候选是 `{label, text}` 记录时 `criteria` 键为 `label`、值为 `text`；
//! (c) `test` 带 `labels` → `criteria: {true, false}`，且 `q_hash` 与不带者不同；
//! (d) 只凭 `r1` 账本重放：报 `W-header … render_version`，按账本头的渲染版本算键，值相同、0 次新调用；
//!     另用一份改动前的 `r1` 金样账本走 CLI 重放（`tests/replay/r1/`）；
//! (e) `Pick(k)` 的下标在 `label` 键下仍按 `over` 下标；
//! (f) 不同 `on` 仍各一次调用（B62）；
//! (g) 标签键的 `select` 声明置换：只发一遍、`mode_share` 为空、出口不是 `Pick`（键序定死，逆序是假的）；
//! (h) 续接一份 `r1` 账本 → `E-render-version`；
//! (i) 同材料一道声明置换的 `select` 与一道未声明的合发：未声明那道的读数与它单发时相同（I2）；
//! (j) `form("test", …, {labels})` 的 `form_hash` 与不带者不同、`fill` 出的题带 `labels`；不带标签的哈希不变；
//! (k) `labels` 用在 `select` 上 → `E-rt-question`；
//! (l) `FnPort::judge` 收到合批调用时按状态分段调闭包、答案按题序合并；
//! (m) `calib-import --from-ledger` 读 `r1` 账本报 `W-render-version`；
//! (n) 无 `over`、无 `labels` 的题，`r2` 请求体与 `r1` 逐字节相同（题库 F1、F2、F5 重认的前提）；
//! (o) 直线段提升按材料分组：两个包装函数各问同材料异候选的 `select`，一次调用；
//! (p) 是非题声明 `evidence: ["over"]`：线上看不到候选，出口 `insufficient:over`（J-09 不因状态里有 `over` 放行）。
//! 变异检验：`flush.rs` 的分组键改回 `state.hash`，(a) 红（3 次调用）；`register.rs` 提升计数键改回
//! `st.hash`，(o) 红（2 次调用）。
//! 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）；预注册 `地基/过程记录/工程-步15i.md` §一。

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use jpp::effects::{CalibStore, EffectError, JevClient, JevPorts};
use jpp::ledger::{Entry, Header, Ledger};
use jpp::value::{Answer, Form, Mat, Op, Question, State, TestLabels};
use jpp::{ActionRegistry, run, run_replay};
use jpp::{lower, syntax::parse};
use jpp_effects::builtin_ports::FnPort;
use jpp_effects::port::{CallInput, EffectOut, JudgeResult};
use jpp_effects::{EffectId, ReplayPorts};
use serde_json::{Value as Json, json};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 替身判断器：是非题 0.9；选择题把 0.9 给内容是「乙」或「丙」的那个键（按发出的值找），其余 0.05。
/// 记下每一份请求体。
fn 替身(记: Rc<RefCell<Vec<Json>>>) -> impl FnMut(&Json) -> Result<Json, EffectError> {
    move |body: &Json| {
        记.borrow_mut().push(body.clone());
        let mut ans = serde_json::Map::new();
        for (qid, q) in body["questions"].as_object().expect("有题") {
            let a = match q["type"].as_str() {
                Some("noul") => json!({"noul": 0.9}),
                Some("choice") => {
                    let crit = q["criteria"].as_object().expect("选择题有候选");
                    let mut probs = serde_json::Map::new();
                    for (k, v) in crit {
                        let 中 = v == "乙" || v == "丙" || v == "丙的描述";
                        probs.insert(k.clone(), json!(if 中 { 0.9 } else { 0.05 }));
                    }
                    json!({"probabilities": probs})
                }
                other => panic!("没料到的题型 {other:?}"),
            };
            ans.insert(qid.clone(), a);
        }
        Ok(json!({"answers": ans, "usage": {"input_tokens": 10}}))
    }
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

fn 校准() -> CalibStore {
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    calib
}

struct 跑出 {
    值: Json,
    告警: Vec<String>,
    请求: Vec<Json>,
    账本: Ledger,
}

fn 跑(src: &str) -> 跑出 {
    跑_置换(src, false)
}

fn 跑_置换(src: &str, 宿主置换: bool) -> 跑出 {
    let program = 程序(src);
    let 记 = Rc::new(RefCell::new(vec![]));
    let mut c = JevClient::with_transport("jev-1.13.0", Box::new(替身(记.clone())));
    c.permute = 宿主置换;
    let mut jp = JevPorts::new(c);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        jp.ports(),
        &校准(),
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    let 请求 = 记.borrow().clone();
    跑出 {
        值: out.value_json(),
        告警: out.trace.warnings.clone(),
        请求,
        账本: ledger,
    }
}

fn 判断条目(l: &Ledger) -> Vec<&Entry> {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .collect()
}

const 同材料三题: &str = r#"
budget {calls: 6, cost: 1, depth: 16};
let m = mat("一段话");
let r1 = judge(state(m, {over: [mat("甲"), mat("乙")]}), select("哪个一", "k"));
let r2 = judge(state(m, {over: [mat("丙"), mat("丁"), mat("戊")]}), select("哪个二", "k"));
let r3 = judge(state(m), test("行吗", "k"));
let e1 = cut(r1);
let e2 = cut(r2);
let e3 = cut(r3);
let out = [exit_kind(e1), exit_kind(e2), exit_kind(e3)];
consume(e1, "drop");
consume(e2, "drop");
consume(e3, "drop");
out
"#;

/// (a) 同材料两道候选集不同的 `select` 加一道 `test`：1 次调用、3 条账本条目。
/// 变异检验：分组键改回 `StateHash`（`flush.rs`）时这里是 3 次调用。
#[test]
fn a_同材料三题一次调用() {
    let r = 跑(同材料三题);
    assert_eq!(r.请求.len(), 1, "同材料合成一次调用：{:#?}", r.请求);
    let js = 判断条目(&r.账本);
    assert_eq!(js.len(), 3, "三道题各一条账本条目");
    let calls: std::collections::BTreeSet<u64> = js
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { call, .. } => Some(*call),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 1, "三条条目同属一次调用");
    // 各道 select 的读数按自己的 `over` 长度（2 与 3）
    let lens: Vec<usize> = js
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                answer: Answer::Choice(v),
                ..
            } => Some(v.len()),
            _ => None,
        })
        .collect();
    assert_eq!(lens, vec![2, 3]);
    assert_eq!(r.请求[0]["questions"].as_object().unwrap().len(), 3);
}

/// (b) 线上 `state` 不含 `over`；候选是 `{label, text}` 记录时键取 `label`、值取 `text`。
#[test]
fn b_线上无over且键为标签() {
    let r = 跑(r#"
budget {calls: 2, cost: 1, depth: 16};
let s = state(mat("一段话"), {over: [mat({label: "A类", text: "甲的描述"}), mat({label: "B类", text: "丙的描述"})]});
let e = cut(judge(s, select("哪一类", "k")));
let k = exit_kind(e);
consume(e, "drop");
k
"#);
    assert_eq!(r.请求.len(), 1);
    let body = &r.请求[0];
    assert_eq!(
        body["state"],
        json!({"on": "一段话"}),
        "state 只发材料：{body}"
    );
    assert_eq!(
        body["questions"]["q0"]["criteria"],
        json!({"A类": "甲的描述", "B类": "丙的描述"})
    );
    // 纯文本候选仍是 c{k} 键
    let r2 = 跑(同材料三题);
    let b2 = &r2.请求[0];
    assert!(b2["state"].get("over").is_none(), "{b2}");
    assert_eq!(
        b2["questions"]["q0"]["criteria"],
        json!({"c0": "甲", "c1": "乙"})
    );
    assert_eq!(
        b2["questions"]["q1"]["criteria"],
        json!({"c0": "丙", "c1": "丁", "c2": "戊"})
    );
    assert!(
        b2["questions"]["q2"].get("criteria").is_none(),
        "是非题不带标签不发 criteria"
    );
}

/// (c) `test` 带 `labels` → `criteria: {true, false}`，且题哈希与不带者不同。
#[test]
fn c_是非题标签() {
    let r = 跑(r#"
budget {calls: 2, cost: 1, depth: 16};
let q1 = test("是垃圾邮件吗", "k", {labels: {yes: "垃圾邮件", no: "正常邮件"}});
let q2 = test("是垃圾邮件吗", "k");
let e = cut(judge(state(mat("一封信")), q1));
let k = exit_kind(e);
consume(e, "drop");
{k: k, h1: q1.hash, h2: q2.hash}
"#);
    let body = &r.请求[0];
    assert_eq!(
        body["questions"]["q0"]["criteria"],
        json!({"true": "垃圾邮件", "false": "正常邮件"})
    );
    assert_ne!(r.值["h1"], r.值["h2"], "labels 进题哈希");
    // 不带标签的题哈希与步 15i 前相同（钉 jpp-value 的构造：无标签时 with_labels 不动哈希）
    let q = Question::new(Op::Test, "是垃圾邮件吗", "k", vec![]);
    assert_eq!(json!(q.hash), r.值["h2"]);
    assert_eq!(q.clone().with_labels(None).hash, q.hash);
}

/// 把一本新渲染的账本改写成 `r1` 渲染下的同一本（头与每条判断键的渲染分量换成 `r1`，键重算）：
/// 这正是步 15i 之前的二进制会写出的那本账本。
fn 改成r1(l: &Ledger) -> Ledger {
    let mut h: Header = l.header.clone().expect("有头");
    h.compared.render_version = "r1".into();
    let mut out = Ledger::new();
    out.header = Some(h);
    for e in &l.entries {
        let e2 = match e.clone() {
            Entry::Judge {
                key: _,
                jkey: Some(mut jk),
                answer,
                tokens,
                cost,
                model_id,
                call,
                calib_ref,
                layer,
                merged_by,
                parents,
                hop,
                reused_from,
                perm,
                confidence,
            } => {
                jk.render = "r1".into();
                Entry::Judge {
                    key: jk.digest(),
                    jkey: Some(jk),
                    answer,
                    tokens,
                    cost,
                    model_id,
                    call,
                    calib_ref,
                    layer,
                    merged_by,
                    parents,
                    hop,
                    reused_from,
                    perm,
                    confidence,
                }
            }
            other => other,
        };
        out.put(e2);
    }
    out
}

/// (d) 只凭 `r1` 账本重放：按账本头的渲染版本算键，照样命中；报 `W-header … render_version`，0 次新调用。
#[test]
fn d_只凭r1账本重放() {
    let r = 跑(同材料三题);
    let mut r1 = 改成r1(&r.账本);
    let program = 程序(同材料三题);
    let rp = ReplayPorts::ports("jev-1.13.0");
    let out = run_replay(&program, rp, &校准(), &ActionRegistry::new(), &mut r1)
        .unwrap_or_else(|e| panic!("r1 账本应当照样重放：{}", e.render()));
    assert_eq!(out.value_json(), r.值, "值与首跑相同");
    assert_eq!(out.cost.calls, 0, "重放不发请求");
    let w = out
        .trace
        .warnings
        .iter()
        .find(|w| w.starts_with("W-header"))
        .unwrap_or_else(|| panic!("要报 W-header：{:?}", out.trace.warnings));
    assert!(w.contains("render_version 旧 r1 新 r2"), "{w}");
}

/// (d) 的 CLI 一半：改动前的 `r1` 金样账本（`tests/replay/r1/`，取自 `0e076e6f`）在新二进制上只凭账本重放，
/// 值与当时的重放报告相同。
#[test]
fn d_r1金样账本经cli重放() {
    let d = std::env::temp_dir().join(format!("jpp-b155-d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let out = d.join("report.json");
    let 写出 = d.join("replayed.ledger.json");
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "run",
            "examples/bank-which_named.jpp",
            "--replay",
            "tests/replay/r1/bank-which_named.ledger.json",
            "--ledger-out",
            写出.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // 重放写出的账本头与键一致（渲染版本仍是 r1），它自己也能再只凭账本重放
    let 头: Json = serde_json::from_str(
        std::fs::read_to_string(&写出)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(头["header"]["compared"]["render_version"], json!("r1"));
    let o2 = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args([
            "run",
            "examples/bank-which_named.jpp",
            "--replay",
            写出.to_str().unwrap(),
            "--output",
            d.join("report2.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        o2.status.success(),
        "{}",
        String::from_utf8_lossy(&o2.stderr)
    );
    let now: Json = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    let then: Json = serde_json::from_str(
        &std::fs::read_to_string(
            root().join("tests/replay/r1/bank-which_named.replay-report.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for f in ["value", "status", "returned_unsure", "pending", "exits"] {
        assert_eq!(now[f], then[f], "{f} 与 r1 时的重放报告相同");
    }
    assert_eq!(now["cost"]["calls"], json!(0));
    let ws = now["trace"]["warnings"].to_string();
    assert!(ws.contains("render_version 旧 r1 新 r2"), "{ws}");
}

/// (e) 标签键下读数仍按 `over` 下标：判断器把 0.9 给「B类」（`over[1]`），读数第 1 位是 0.9。
#[test]
fn e_标签键读数按下标() {
    let 记 = Rc::new(RefCell::new(vec![]));
    let mut c = JevClient::with_transport("jev-1.13.0", Box::new(替身(记.clone())));
    let s = State::new(
        vec![Mat::literal(json!("一段话"))],
        vec![],
        vec![],
        vec![
            Mat::literal(json!({"label": "A类", "text": "甲的描述"})),
            Mat::literal(json!({"label": "B类", "text": "丙的描述"})),
            Mat::literal(json!({"label": "C类", "text": "丁的描述"})),
        ],
        false,
    );
    let q = Question::new(Op::Select, "哪一类", "k", vec![]);
    let r = c.judge(&s, &[&q]).expect("跑得通");
    let Answer::Choice(v) = &r.answers[0] else {
        panic!("选择题读数")
    };
    assert_eq!(
        v,
        &vec![0.05, 0.9, 0.05],
        "argmax 在 over 下标 1（B类），Pick(1)"
    );
    // 标签重名时退回 c{k}：键不会撞
    let s2 = State::new(
        vec![Mat::literal(json!("一段话"))],
        vec![],
        vec![],
        vec![
            Mat::literal(json!({"label": "同名", "text": "甲的描述"})),
            Mat::literal(json!({"label": "同名", "text": "丙的描述"})),
        ],
        false,
    );
    let _ = c.judge(&s2, &[&q]).expect("跑得通");
    let crit = 记.borrow()[1]["questions"]["q0"]["criteria"].clone();
    assert_eq!(
        crit.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["c0", "c1"]
    );
}

/// (f) 不同 `on` 仍各一次调用（B62）。
#[test]
fn f_不同材料各一次() {
    let r = 跑(r#"
budget {calls: 6, cost: 1, depth: 16};
let over = [mat("甲"), mat("乙")];
let r1 = judge(state(mat("第一段"), {over: over}), select("哪个", "k"));
let r2 = judge(state(mat("第二段"), {over: over}), select("哪个", "k"));
let e1 = cut(r1);
let e2 = cut(r2);
let out = [exit_kind(e1), exit_kind(e2)];
consume(e1, "drop");
consume(e2, "drop");
out
"#);
    assert_eq!(r.请求.len(), 2, "两个不同的 on 是两份材料");
}

/// (g) 标签键的 `select` 声明置换：键按标签名排序发出，逆序发不出来——只发一遍、不记置换测量、出口不是 Pick。
#[test]
fn g_标签键不置换() {
    let r = 跑(r#"
budget {calls: 4, cost: 1, depth: 16};
let s = state(mat("一段话"), {over: [mat({label: "A类", text: "甲的描述"}), mat({label: "B类", text: "丙的描述"})]});
let e = cut(judge(s, select("哪一类", "k", {permute: true})));
let k = exit_kind(e);
consume(e, "drop");
k
"#);
    assert_eq!(r.请求.len(), 1, "标签键不发第二遍：{:#?}", r.请求);
    let perm = 判断条目(&r.账本)
        .iter()
        .any(|e| matches!(e, Entry::Judge { perm: Some(_), .. }));
    assert!(!perm, "没测置换，不记置换测量");
    let k = r.值.as_str().unwrap_or("");
    assert!(!k.starts_with("pick"), "不能给 Pick：{k}");
    assert!(
        r.告警
            .iter()
            .any(|w| w.starts_with("W-untested") && w.contains("label, text")),
        "说清楚置换为什么不生效：{:?}",
        r.告警
    );
    // 对照：同样的候选用纯文本，声明置换就发两遍
    let r2 = 跑(r#"
budget {calls: 4, cost: 1, depth: 16};
let s = state(mat("一段话"), {over: [mat("甲"), mat("丙")]});
let e = cut(judge(s, select("哪一类", "k", {permute: true})));
let k = exit_kind(e);
consume(e, "drop");
k
"#);
    assert_eq!(r2.请求.len(), 2);
    assert!(
        r2.值.as_str().unwrap_or("").starts_with("pick"),
        "{}",
        r2.值
    );
}

/// (h) 续接一份 `r1` 账本：续跑会发新请求，同一账本混两种渲染违反 B48，拒绝。
#[test]
fn h_续接r1账本拒绝() {
    let r = 跑(同材料三题);
    let mut r1 = 改成r1(&r.账本);
    let program = 程序(同材料三题);
    let 记 = Rc::new(RefCell::new(vec![]));
    let mut jp = JevPorts::new(JevClient::with_transport(
        "jev-1.13.0",
        Box::new(替身(记.clone())),
    ));
    let e = run(
        &program,
        jp.ports(),
        &校准(),
        &ActionRegistry::new(),
        &mut r1,
    )
    .expect_err("续接 r1 账本要报错");
    let m = e.render();
    assert!(m.contains("E-render-version"), "{m}");
    assert!(记.borrow().is_empty(), "拒绝在发请求之前");
}

/// (i) 同材料一道声明置换的 `select` 与一道未声明的合发：未声明那道只进第一遍，读数与单发相同，
/// 不记置换测量；声明的那道两遍、记 `perms = 2`。
#[test]
fn i_置换按题不改旁题读数() {
    let 合 = 跑(r#"
budget {calls: 6, cost: 1, depth: 16};
let m = mat("一段话");
let r1 = judge(state(m, {over: [mat("甲"), mat("乙")]}), select("哪个一", "k", {permute: true}));
let r2 = judge(state(m, {over: [mat("丙"), mat("丁"), mat("戊")]}), select("哪个二", "k"));
let e1 = cut(r1);
let e2 = cut(r2);
let out = [exit_kind(e1), exit_kind(e2)];
consume(e1, "drop");
consume(e2, "drop");
out
"#);
    assert_eq!(合.请求.len(), 2, "一次调用两遍");
    let 第二遍 = 合.请求[1]["questions"].as_object().unwrap();
    assert_eq!(
        第二遍.keys().collect::<Vec<_>>(),
        vec!["q0"],
        "第二遍只带声明置换的那道"
    );
    assert_eq!(
        合.请求[1]["questions"]["q0"]["criteria"],
        json!({"c0": "乙", "c1": "甲"}),
        "逆序"
    );
    let 单 = 跑(r#"
budget {calls: 6, cost: 1, depth: 16};
let m = mat("一段话");
let r2 = judge(state(m, {over: [mat("丙"), mat("丁"), mat("戊")]}), select("哪个二", "k"));
let e2 = cut(r2);
let out = exit_kind(e2);
consume(e2, "drop");
out
"#);
    let 读数 = |l: &Ledger, n: usize| -> Vec<(Answer, bool)> {
        判断条目(l)
            .iter()
            .filter_map(|e| match e {
                Entry::Judge {
                    answer: a @ Answer::Choice(v),
                    perm,
                    ..
                } if v.len() == n => Some((a.clone(), perm.is_some())),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        读数(&合.账本, 3),
        读数(&单.账本, 3),
        "旁题读数与单发相同（I2）"
    );
    assert!(!读数(&合.账本, 3)[0].1, "旁题不记置换测量");
    let 声明 = 判断条目(&合.账本)
        .iter()
        .find_map(|e| match e {
            Entry::Judge {
                answer: Answer::Choice(v),
                perm: Some(p),
                ..
            } if v.len() == 2 => Some(p.perms),
            _ => None,
        })
        .expect("声明置换的那道记了测量");
    assert_eq!(声明, 2);
}

/// (j) 是非题式带 `labels`：`form_hash` 与不带者不同，`fill` 出的题带 `labels`；不带标签的哈希不变。
#[test]
fn j_题式标签进哈希() {
    let f0 = Form::new(
        Op::Test,
        "{x} 是垃圾邮件吗",
        "k",
        vec![],
        vec![],
        None,
        None,
    )
    .unwrap();
    let h0 = f0.hash.clone();
    let l = Some(TestLabels {
        yes: "垃圾".into(),
        no: "正常".into(),
    });
    let f1 = f0.clone().with_labels(l.clone());
    assert_ne!(f1.hash, h0);
    assert_eq!(f0.clone().with_labels(None).hash, h0);
    let q = f1.fill(&[("x".into(), "这封信".into())]).unwrap();
    assert_eq!(q.labels, l);
    let q0 = f0.fill(&[("x".into(), "这封信".into())]).unwrap();
    assert_ne!(q.hash, q0.hash);
    // 运行时的题式构造同样带上
    let r = 跑(r#"
budget {calls: 2, cost: 1, depth: 16};
let f = form("test", "{x} 是垃圾邮件吗", {calib: "k", labels: {yes: "垃圾", no: "正常"}});
let g = form("test", "{x} 是垃圾邮件吗", {calib: "k"});
let e = cut(judge(state(mat("信")), fill(f, {x: "这封信"})));
let k = exit_kind(e);
consume(e, "drop");
{k: k, f: f.hash, g: g.hash}
"#);
    assert_ne!(r.值["f"], r.值["g"]);
    assert_eq!(r.值["g"], json!(h0), "不带标签的题式哈希与步 15i 前同算法");
    assert_eq!(
        r.请求[0]["questions"]["q0"]["criteria"],
        json!({"true": "垃圾", "false": "正常"})
    );
}

/// (k) `labels` 只用于是非题：用在 `select` 上报 `E-rt-question`。
#[test]
fn k_选择题不收labels() {
    let program = 程序(
        r#"
budget {calls: 2, cost: 1, depth: 16};
let q = select("哪个", "k", {labels: {yes: "是", no: "否"}});
q
"#,
    );
    let 记 = Rc::new(RefCell::new(vec![]));
    let mut jp = JevPorts::new(JevClient::with_transport("jev-1.13.0", Box::new(替身(记))));
    let e = run(
        &program,
        jp.ports(),
        &校准(),
        &ActionRegistry::new(),
        &mut Ledger::new(),
    )
    .expect_err("要报错");
    let m = e.render();
    assert!(m.contains("E-rt-question") && m.contains("labels"), "{m}");
}

/// (l) `FnPort::judge` 收到合批调用：按状态分段调单状态闭包，答案按题序拼回。
#[test]
fn l_fn端口按状态分段() {
    let 段 = Rc::new(RefCell::new(vec![]));
    let 段2 = 段.clone();
    let mut port = FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        段2.borrow_mut().push((s.over.len(), qs.len()));
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|_| Answer::Choice(vec![1.0 / s.over.len() as f64; s.over.len()]))
                .collect(),
            tokens: 1,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
            confidence: vec![],
        })
    });
    let 态 = |n: usize| {
        State::new(
            vec![Mat::literal(json!("材料"))],
            vec![],
            vec![],
            (0..n)
                .map(|i| Mat::literal(json!(format!("候选{i}"))))
                .collect(),
            false,
        )
    };
    let q = Question::new(Op::Select, "哪个", "k", vec![]);
    let mut ports = jpp_effects::Ports::new().with(&mut port);
    let EffectOut::Readings(r) = ports
        .call(
            EffectId::Judge,
            CallInput::MaterialQuestions {
                states: vec![态(2), 态(2), 态(3)],
                questions: vec![q.clone(), q.clone(), q],
            },
        )
        .expect("跑得通")
    else {
        panic!("要读数")
    };
    drop(ports);
    assert_eq!(*段.borrow(), vec![(2, 2), (3, 1)], "连续同状态一段");
    let lens: Vec<usize> = r
        .answers
        .iter()
        .map(|a| match a {
            Answer::Choice(v) => v.len(),
            _ => 0,
        })
        .collect();
    assert_eq!(lens, vec![2, 2, 3]);
    assert_eq!(r.tokens, 2);
}

/// (m) `calib-import --from-ledger` 读 `r1` 账本：stderr 报 `W-render-version`，不拒绝。
#[test]
fn m_导入r1账本报渲染版本() {
    let d = std::env::temp_dir().join(format!("jpp-b155-m-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .args([
            "calib-import",
            "--from-ledger",
            root()
                .join("tests/replay/r1/bank-which_named.ledger.json")
                .to_str()
                .unwrap(),
            "--key",
            "cls-which-mentioned",
            "--list-out",
            "list.jsonl",
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{err}");
    assert!(
        err.contains("W-render-version") && err.contains("r1"),
        "{err}"
    );
}

/// (n) 无 `over`、无 `labels` 的题：`r2` 的请求体与 `r1` 逐字节相同（`state` 就是 `to_json()`，是非题不发
/// `criteria`，打分题的 `criteria` 仍是档位）。题库 F1、F2、F5 重认按「请求体不变」预注册，依据是这一条。
#[test]
fn n_无候选无标签的请求体不变() {
    let s = State::new(
        vec![Mat::literal(json!("一段话"))],
        vec![Mat::literal(json!("语境"))],
        vec![],
        vec![],
        false,
    );
    let t = Question::new(Op::Test, "行吗", "k", vec![]);
    let m = Question::new(Op::Measure, "多少", "k", vec!["低".into(), "高".into()]);
    let body = JevClient::request_body("jev-1.13.0", &s, &[&t, &m]);
    let r1 = json!({
        "state": s.to_json(),
        "model": "jev-1.13.0",
        "questions": {
            "q0": {"type": "noul", "instructions": "行吗"},
            "q1": {"type": "score", "instructions": "多少", "criteria": ["低", "高"]}
        }
    });
    assert_eq!(body, r1);
}

/// (o) 直线段提升按材料分组（`register.rs::提升过调用`）：两个包装函数各问同一材料上候选集不同的
/// `select` 并当场检视，顺序调用（实参是名字，13b 的提升条件）——两处按材料算一组，提升到段首，一次调用。
/// 变异检验：提升计数键改回 `st.hash` 时两处各成一组、不提升，两次调用。
#[test]
fn o_提升按材料分组() {
    let r = 跑(r#"
budget {calls: 6, cost: 1, depth: 16};
let m = mat("一段话");
fn 问(s, 题面) !{judge} {
    let e = cut(judge(s, select(题面, "k")));
    handle(e, {pick: fn(k) { k }, unsure: fn(u) { consume(u, "drop"); -1 }})
}
let s1 = state(m, {over: [mat("甲"), mat("乙")]});
let s2 = state(m, {over: [mat("丙"), mat("丁"), mat("戊")]});
let a = 问(s1, "哪个一");
let b = 问(s2, "哪个二");
[a, b]
"#);
    assert_eq!(r.请求.len(), 1, "两处提升后同材料一次调用：{:#?}", r.请求);
}

/// (p) 是非题声明 `over` 为决定性证据：`r2` 下候选只随 `select` 发，是非题线上看不到它，
/// J-09 按缺证据处理（`insufficient:over`），不因状态里有 `over` 就信任 p。`select` 声明 `over` 照旧。
#[test]
fn p_是非题的over证据按缺() {
    let r = 跑(r#"
budget {calls: 4, cost: 1, depth: 16};
let s = state(mat("一段话"), {over: [mat("甲"), mat("乙")]});
let e1 = cut(judge(s, test("候选里有甲吗", "k", {evidence: ["over"]})));
let e2 = cut(judge(s, select("哪个", "k", {evidence: ["over"]})));
let out = [exit_kind(e1), exit_kind(e2)];
consume(e1, "drop");
consume(e2, "drop");
out
"#);
    assert_eq!(r.值[0], json!("unsure(insufficient:over)"), "{}", r.值);
    assert_ne!(r.值[1], json!("unsure(insufficient:over)"), "{}", r.值);
}
