//! 步 20j-3：`cut` 的 `stat`（B153 `mass`、`expect`、`cuts`；B154 `confidence`）、声明线两端开闭 `closed`（B165）、
//! 统计量函数 `stat_of` 一处（B167 (1)）。
//!
//! (a)–(k) 按 `21` §四·15 步 20j-3 与其〔B154 追加 (5)〕〔B165 追加 (6)〕的测试清单（(g) 由金样测试钉住）；
//! 另加 (l) 惰性出口开关两态出口相同、(m) 上端开与 `cuts` 开、(n) 声明记录只在非缺省时带 `stat`/`closed`。
//! 依据：`地基/附注/2026-09-26-批6裁定.md` §一、§二、§十三、§十五；过程记录 `地基/过程记录/工程-步20j-3.md`。

mod common;

use jpp::effects::{CalibStore, FixedPorts, FnPort, JudgeResult, Ports, Profile};
use jpp::interp::{ActionRegistry, Passes};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{lower, run, syntax::parse};
use serde_json::{Value as Json, json};

/// 判断器（非固定观察，模型 `m`）：每道题回同一个答案；置换测过且一致；不报自报置信度
fn 端口<'a>(a: Answer) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| a.clone()).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: qs.iter().map(|_| Some(1.0)).collect(),
            perms: qs.iter().map(|_| 2).collect(),
            confidence: vec![],
        })
    }))
}

fn 程序(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

struct 跑出 {
    kind: String,
    exits: Vec<Json>,
    warnings: Vec<String>,
    ledger: Ledger,
}

fn 跑(src: &str, a: Answer, calib: &CalibStore) -> Result<跑出, String> {
    let mut l = Ledger::new();
    let o =
        run(&程序(src), 端口(a), calib, &ActionRegistry::new(), &mut l).map_err(|e| e.render())?;
    Ok(跑出 {
        kind: o.value_json().as_str().unwrap_or_default().to_string(),
        exits: o.exits.clone(),
        warnings: o.trace.warnings.clone(),
        ledger: l,
    })
}

/// 一次 cut，出口种类作返回值。`读数` 是 judge 表达式，`选项` 是 cut 的第二位起
fn 单切(读数: &str, 选项: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
let e = cut({读数}{选项});
let k = exit_kind(e);
consume(e, "drop");
k
"#
    )
}

const 四选一: &str = r#"judge(state(mat("甲"), {over: [mat("a"), mat("b"), mat("c"), mat("d")]}), select("哪个", "k"))"#;
const 四档: &str = r#"judge(state(mat("甲")), measure("多重", ["无", "轻", "中", "重"], "k"))"#;
const 是非: &str = r#"judge(state(mat("甲")), test("行吗", "k"))"#;

/// (a) `mass [0, 1]`：读数 [0.3, 0.25, 0.4, 0.05] → 0.55 ≥ 0.5 → act（取大是 0.3，会落进带里）
#[test]
fn a_mass按单元并集求和() {
    let src = 单切(
        四选一,
        r#", {stat: {mass: [0, 1]}, declare: {hi: 0.5, lo: 0.2}}"#,
    );
    let o = 跑(
        &src,
        Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]),
        &CalibStore::new(),
    )
    .unwrap();
    assert_eq!(o.kind, "act");
    let row = &o.exits[0];
    assert_eq!(row["grade"], "Declared");
    assert_eq!(row["releases"], false);
    assert_eq!(row["stat"], json!({"mass": [0, 1]}));
    assert_eq!(row["declared"]["hi"], 0.5);
    assert_eq!(row["declared"]["lo"], 0.2);
    // 标注样本在 p_max 上，对 mass 无从核
    assert_eq!(row["evidence"]["errors_at_line"], Json::Null);
    // [2, 3] 的质量 0.45：不到 0.5，也不在 0.2 以下，落在带里
    let src = 单切(
        四选一,
        r#", {stat: {mass: [3, 2]}, declare: {hi: 0.5, lo: 0.2}}"#,
    );
    let o = 跑(
        &src,
        Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]),
        &CalibStore::new(),
    )
    .unwrap();
    assert_eq!(o.kind, "unsure(band)");
    assert_eq!(o.exits[0]["stat"], json!({"mass": [2, 3]}), "单元去重升序");
}

/// (b) `expect` 配 `{hi: 2.0}`：E = 2.0 → act，E = 1.99 → ignore（单线，没有带）
#[test]
fn b_expect单线() {
    let src = 单切(四档, r#", {stat: "expect", declare: {hi: 2.0}}"#);
    let c = CalibStore::new();
    let o = 跑(&src, Answer::Score(vec![0.0, 0.0, 1.0, 0.0]), &c).unwrap();
    assert_eq!(o.kind, "act");
    let o = 跑(&src, Answer::Score(vec![0.0, 0.01, 0.99, 0.0]), &c).unwrap();
    assert_eq!(o.kind, "ignore");
    assert_eq!(o.exits[0]["stat"], "expect");
}

/// (c) `cuts [0.5, 1.5, 2.5]`：E = 1.7 → `At(2)`
#[test]
fn c_cuts分桶() {
    let src = 单切(
        四档,
        r#", {stat: "expect", declare: {cuts: [0.5, 1.5, 2.5]}}"#,
    );
    let o = 跑(
        &src,
        Answer::Score(vec![0.1, 0.2, 0.6, 0.1]),
        &CalibStore::new(),
    )
    .unwrap();
    assert_eq!(o.kind, "at(2)");
    assert_eq!(o.exits[0]["declared"]["cuts"], json!([0.5, 1.5, 2.5]));
    assert!(o.exits[0]["declared"].get("hi").is_none());
}

/// (d) 同键有 p_max 上的认证记录：不带 `stat` 的 `cut` 用上它，`stat: "expect"` 不带 `declare` 仍冷
#[test]
fn d_别的统计量不借认证线() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.9, 0.1, 50);
    let a = Answer::Score(vec![0.0, 0.0, 0.97, 0.03]);
    let 照常 = 跑(&单切(四档, ""), a.clone(), &c).unwrap();
    assert_eq!(照常.kind, "at(2)", "记录能用：{:?}", 照常.exits);
    assert_ne!(照常.exits[0]["grade"], "Cold");
    let o = 跑(&单切(四档, r#", {stat: "expect"}"#), a, &c).unwrap();
    assert_eq!(o.kind, "unsure(cold|untested:calib_line)");
    assert_eq!(o.exits[0]["grade"], "Cold");
    assert_eq!(o.exits[0]["stat"], "expect");
    let w: Vec<&String> = o
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-untested"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.warnings);
    assert!(
        w[0].contains("p_max") && w[0].contains("declare"),
        "{}",
        w[0]
    );
    // 不查记录：账本不因它记校准命中
    assert!(o.ledger.calib_used.is_empty(), "{:?}", o.ledger.calib_used);
}

/// (e) 选项与读数不配 → `E-cut-options`
#[test]
fn e_选项错() {
    let c = CalibStore::new();
    let 错 = |读数: &str, 选项: &str, a: Answer| {
        let e = 跑(&单切(读数, 选项), a, &c)
            .err()
            .unwrap_or_else(|| panic!("{选项} 该报错"));
        assert!(e.contains("E-cut-options"), "{选项}：{e}");
        e
    };
    let p = || Answer::Noul(0.8);
    let 选 = || Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]);
    let 档 = || Answer::Score(vec![0.1, 0.2, 0.6, 0.1]);
    // test 读数给单元上的统计量
    错(是非, r#", {stat: {mass: [0]}, declare: {hi: 0.5}}"#, p());
    错(是非, r#", {stat: "expect", declare: {hi: 0.5}}"#, p());
    错(是非, r#", {stat: "expect"}"#, p());
    // argmax 不在 cut 上
    错(四选一, r#", {stat: "argmax", declare: {hi: 0.5}}"#, 选());
    // expect 只用于 measure
    错(四选一, r#", {stat: "expect", declare: {hi: 1.0}}"#, 选());
    // mass 下标越界
    let e = 错(
        四选一,
        r#", {stat: {mass: [0, 4]}, declare: {hi: 0.5}}"#,
        选(),
    );
    assert!(e.contains("越界"), "{e}");
    // stat 与 cost / alpha
    错(四档, r#", {stat: "expect", cost: [10, 1]}"#, 档());
    错(四档, r#", {stat: "expect", alpha: 0.2}"#, 档());
    // cuts 非递增、cuts 配 mass、hi 与 cuts 同给
    错(
        四档,
        r#", {stat: "expect", declare: {cuts: [1.5, 0.5]}}"#,
        档(),
    );
    错(
        四选一,
        r#", {stat: {mass: [0]}, declare: {cuts: [0.5]}}"#,
        选(),
    );
    错(
        四档,
        r#", {stat: "expect", declare: {hi: 1.0, cuts: [0.5]}}"#,
        档(),
    );
    // 单线给 closed.lo；cuts 线给 closed.hi
    错(是非, r#", {declare: {hi: 0.7, closed: {lo: false}}}"#, p());
    错(
        四档,
        r#", {stat: "expect", declare: {cuts: [0.5], closed: {hi: false}}}"#,
        档(),
    );
    // expect 的线越过最高档 K − 1 = 3；mass 的线越过 1
    错(四档, r#", {stat: "expect", declare: {hi: 3.5}}"#, 档());
    错(四选一, r#", {stat: {mass: [0]}, declare: {hi: 1.5}}"#, 选());
    // 未知统计量
    错(四档, r#", {stat: "mean", declare: {hi: 1.0}}"#, 档());
}

/// (f) `expect` + `cuts` 出 at 型出口：`handle` 写 `act` 臂。直接嵌套检查期报 J-05；经 `let` 绑定运行期报 J-05
#[test]
fn f_按选项取臂() {
    let 直接 = format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
handle(cut({四档}, {{stat: "expect", declare: {{cuts: [0.5, 1.5, 2.5]}}}}), {{
    act: fn() {{ 1 }},
    unsure: fn(u) {{ consume(u, "drop"); 0 }}}})
"#
    );
    let r = jpp::check(&程序(&直接));
    let d = r.find("J-05").expect("直接嵌套：检查期按选项取臂");
    assert!(
        d.message.contains("走不到") && d.message.contains("缺臂 at"),
        "{}",
        d.message
    );
    // 没写 stat 的同形程序检查期不报（现有程序不受影响）
    let 照旧 = 直接.replace(
        r#"{stat: "expect", declare: {cuts: [0.5, 1.5, 2.5]}}"#,
        r#""k""#,
    );
    assert!(jpp::check(&程序(&照旧)).find("J-05").is_none());
    let 绑定 = format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
let e = cut({四档}, {{stat: "expect", declare: {{cuts: [0.5, 1.5, 2.5]}}}});
handle(e, {{
    act: fn() {{ 1 }},
    unsure: fn(u) {{ consume(u, "drop"); 0 }}}})
"#
    );
    let e = 跑(
        &绑定,
        Answer::Score(vec![0.1, 0.2, 0.6, 0.1]),
        &CalibStore::new(),
    )
    .err()
    .expect("运行期按出口种类取臂");
    assert!(
        e.contains("J-05") && e.contains("at 型") && e.contains("缺分支 at"),
        "{e}"
    );
    // test 型的 stat 出口在 select 读数上要 act / ignore 臂，不要 pick
    let 选 = format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
let e = cut({四选一}, {{stat: {{mass: [0, 1]}}, declare: {{hi: 0.5}}}});
handle(e, {{
    pick: fn(k) {{ k }},
    unsure: fn(u) {{ consume(u, "drop"); 0 }}}})
"#
    );
    let e = 跑(
        &选,
        Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]),
        &CalibStore::new(),
    )
    .err()
    .expect("test 型出口缺 act / ignore");
    assert!(e.contains("J-05") && e.contains("test 型"), "{e}");
}

/// 固定观察端口：材料「甲」上的是非题读数 0.8，可选夹具置信度
fn 固定(confidence: Option<f64>) -> FixedPorts {
    let mut fp = FixedPorts::new();
    let s = State::new(
        vec![Mat::literal(json!("甲"))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let k = fp.observe(
        &s,
        &Question::new(Op::Test, "行吗", "k", vec![]),
        Answer::Noul(0.8),
    );
    if let Some(c) = confidence {
        fp.fix_confidence(&k, c);
    }
    fp
}

fn 固定跑(src: &str, confidence: Option<f64>) -> (String, Vec<Json>, Ledger) {
    let mut fp = 固定(confidence);
    let mut l = Ledger::new();
    let o = run(
        &程序(src),
        fp.ports(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    (
        o.value_json().as_str().unwrap_or_default().into(),
        o.exits.clone(),
        l,
    )
}

fn 重放(src: &str, l: &Ledger) -> String {
    let mut l = l.clone();
    let o = jpp::run_replay(
        &程序(src),
        jpp::effects::ReplayPorts::ports(jpp::effects::FIXED_MODEL),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .unwrap_or_else(|e| panic!("{}", e.render()));
    o.value_json().as_str().unwrap_or_default().into()
}

fn 判断条目(l: &Ledger) -> Json {
    let e = l
        .entries
        .iter()
        .find(|e| matches!(e, Entry::Judge { .. }))
        .expect("一条判断条目");
    serde_json::to_value(e).unwrap()
}

/// (h) 夹具 `confidence` 0.55、p 0.8：`stat: "confidence"` 配 `{hi: 0.6}` → ignore，`stat: "max"` → act；
/// 账本条目带 `confidence`，只凭账本重放出口相同。夹具不给时按 p（夹具缺省 p_max）→ act，账本条目不带该字段
#[test]
fn h_夹具置信度() {
    let 置信 = 单切(是非, r#", {stat: "confidence", declare: {hi: 0.6}}"#);
    let 最大 = 单切(是非, r#", {stat: "max", declare: {hi: 0.6}}"#);
    let (k, exits, l) = 固定跑(&置信, Some(0.55));
    assert_eq!(k, "ignore");
    assert_eq!(exits[0]["stat"], "confidence");
    assert_eq!(
        exits[0]["confidence"], 0.55,
        "报告 exits 行可见（B154 (1)）"
    );
    assert_eq!(判断条目(&l)["Judge"]["confidence"], 0.55);
    assert_eq!(重放(&置信, &l), "ignore", "重放从账本取回 0.55");
    let (k, exits, _) = 固定跑(&最大, Some(0.55));
    assert_eq!(k, "act");
    assert!(exits[0].get("stat").is_none(), "max 不写");
    // 夹具不给：缺省 p_max（是非题即 p = 0.8）
    let (k, exits, l) = 固定跑(&置信, None);
    assert_eq!(k, "act");
    assert!(exits[0].get("confidence").is_none(), "夹具缺省不写进报告");
    assert!(判断条目(&l)["Judge"].get("confidence").is_none());
    assert_eq!(重放(&置信, &l), "act");
}

fn 真画像(reports: Option<bool>) -> Profile {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    let mut j: Json =
        serde_json::from_str(&std::fs::read_to_string(p).expect("真画像在")).expect("合法 JSON");
    match reports {
        Some(b) => j["reports_confidence"] = json!(b),
        None => {
            j.as_object_mut().unwrap().remove("reports_confidence");
        }
    }
    Profile::from_json(&j).expect("画像读得动")
}

/// (i) H9：画像未测或为假 → 检查期 `E-stat-unavailable`；填 true 不报；不加载画像不报。非固定端口、判断器没报
/// 置信度 → 运行期 `E-stat-unavailable`（不退回 p_max）
#[test]
fn i_画像未测报取不到() {
    let src = 单切(是非, r#", {stat: "confidence", declare: {hi: 0.6}}"#);
    let p = 程序(&src);
    for (画像, 报) in [(None, true), (Some(false), true), (Some(true), false)] {
        let r = jpp::check_with_profile(&p, &真画像(画像));
        let d = r.find("E-stat-unavailable");
        assert_eq!(d.is_some(), 报, "reports_confidence = {画像:?}");
        if let Some(d) = d {
            assert!(d.message.contains("reports_confidence"), "{}", d.message);
        }
    }
    assert!(jpp::check(&p).find("E-stat-unavailable").is_none());
    let e = 跑(&src, Answer::Noul(0.8), &CalibStore::new())
        .err()
        .expect("非固定端口没有 confidence");
    assert!(e.contains("E-stat-unavailable"), "{e}");
}

/// (j) `{hi: 0.7, lo: 0.3, closed: {lo: false}}`：0.3 → band、0.29 → ignore
#[test]
fn j_下端开() {
    let src = 单切(
        是非,
        r#", {declare: {hi: 0.7, lo: 0.3, closed: {lo: false}}}"#,
    );
    let c = CalibStore::new();
    assert_eq!(
        跑(&src, Answer::Noul(0.3), &c).unwrap().kind,
        "unsure(band)"
    );
    let o = 跑(&src, Answer::Noul(0.29), &c).unwrap();
    assert_eq!(o.kind, "ignore");
    assert_eq!(
        o.exits[0]["declared"]["closed"],
        json!({"hi": true, "lo": false})
    );
}

/// (k) 缺省两端闭：0.3 → ignore
#[test]
fn k_缺省闭() {
    let src = 单切(是非, r#", {declare: {hi: 0.7, lo: 0.3}}"#);
    let o = 跑(&src, Answer::Noul(0.3), &CalibStore::new()).unwrap();
    assert_eq!(o.kind, "ignore");
    assert!(o.exits[0]["declared"].get("closed").is_none(), "全闭不写");
}

/// (l) 惰性出口开关两态：(a) 的出口相同
#[test]
fn l_惰性出口两态相同() {
    let src = 单切(
        四选一,
        r#", {stat: {mass: [0, 1]}, declare: {hi: 0.5, lo: 0.2}}"#,
    );
    for lazy in [true, false] {
        let program = 程序(&src);
        let calib = CalibStore::new();
        let actions = ActionRegistry::new();
        let mut l = Ledger::new();
        let mut it = jpp::interp::Interp::new(
            端口(Answer::Choice(vec![0.3, 0.25, 0.4, 0.05])),
            &mut l,
            &calib,
            &actions,
            program.budget.clone(),
        );
        it.passes = Passes {
            lazy_cut: lazy,
            ..Passes::default()
        };
        let o = it.run(&program).expect("跑完");
        assert_eq!(o.value_json(), "act", "lazy_cut = {lazy}");
    }
}

/// (m) 上端开：0.7 → band（闭时 act）；`cuts` 开：E = 1.5 → `At(1)`（闭时 `At(2)`）；K 元 max 线上端开同理
#[test]
fn m_上端开与cuts开() {
    let c = CalibStore::new();
    let src = 单切(
        是非,
        r#", {declare: {hi: 0.7, lo: 0.3, closed: {hi: false}}}"#,
    );
    assert_eq!(
        跑(&src, Answer::Noul(0.7), &c).unwrap().kind,
        "unsure(band)"
    );
    assert_eq!(跑(&src, Answer::Noul(0.71), &c).unwrap().kind, "act");
    let e15 = Answer::Score(vec![0.0, 0.5, 0.5, 0.0]);
    let 闭 = 单切(
        四档,
        r#", {stat: "expect", declare: {cuts: [0.5, 1.5, 2.5]}}"#,
    );
    let 开 = 单切(
        四档,
        r#", {stat: "expect", declare: {cuts: [0.5, 1.5, 2.5], closed: {cuts: false}}}"#,
    );
    assert_eq!(跑(&闭, e15.clone(), &c).unwrap().kind, "at(2)");
    let o = 跑(&开, e15, &c).unwrap();
    assert_eq!(o.kind, "at(1)");
    assert_eq!(o.exits[0]["declared"]["closed"], json!({"cuts": false}));
    let k元 = 单切(四选一, r#", {declare: {hi: 0.4, closed: {hi: false}}}"#);
    let a = Answer::Choice(vec![0.3, 0.25, 0.4, 0.05]);
    assert_eq!(跑(&k元, a, &c).unwrap().kind, "unsure(band)");
}

/// (n) 声明记录（B142 格式）只在非缺省时带 `stat`、`closed`；普通声明线的记录与 20j-1 相同
#[test]
fn n_声明记录的缺省值不写() {
    let c = CalibStore::new();
    let 记录 = |o: &跑出| -> Json {
        let (k, v) = o
            .ledger
            .calib_used
            .iter()
            .find(|(k, _)| k.starts_with("declared:k@"))
            .expect("一条声明记录");
        assert!(k.starts_with("declared:k@"));
        v["record"].clone()
    };
    let o = 跑(
        &单切(是非, r#", {declare: {hi: 0.7, lo: 0.3}}"#),
        Answer::Noul(0.5),
        &c,
    )
    .unwrap();
    let r = 记录(&o);
    let mut 键: Vec<&String> = r.as_object().unwrap().keys().collect();
    键.sort();
    assert_eq!(键, ["hi", "line", "lo", "site"]);
    let o = 跑(
        &单切(四档, r#", {stat: "expect", declare: {cuts: [0.5, 1.5]}}"#),
        Answer::Score(vec![0.1, 0.2, 0.6, 0.1]),
        &c,
    )
    .unwrap();
    let r = 记录(&o);
    assert_eq!(r["stat"], "expect");
    assert_eq!(r["cuts"], json!([0.5, 1.5]));
    assert!(r.get("hi").is_none() && r.get("closed").is_none());
    let o = 跑(
        &单切(
            是非,
            r#", {declare: {hi: 0.7, lo: 0.3, closed: {lo: false}}}"#,
        ),
        Answer::Noul(0.5),
        &c,
    )
    .unwrap();
    assert_eq!(记录(&o)["closed"], json!({"hi": true, "lo": false}));
}
