//! 步 20j-1：作者声明线（B128）、`cut` 的 `alpha` 选证书（B129）、冷出口四条出路（B130）。
//!
//! (a)–(h) 按 `21` §四·13 步 20j-1 的测试清单；另加 `near_line` 按线 ±0.2（`20` v2 §4.4 第 9 条）、
//! `W-declared-line` 一趟一键一条、冷出口告警一趟一键一条。依据：`地基/附注/2026-09-25-作者主权与策略表达裁定.md`；
//! 过程记录 `地基/过程记录/工程-步20j-1.md`。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 判断器：按材料原文回读数（未列出的材料回 0.5）
fn 端口<'a>(表: Vec<(&'static str, Answer)>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs| {
        let 原文 =
            s.on.first()
                .or_else(|| s.over.first())
                .and_then(|m| m.content.as_str().map(String::from))
                .unwrap_or_default();
        let a = 表
            .iter()
            .find(|(k, _)| *k == 原文)
            .map(|(_, a)| a.clone())
            .unwrap_or(Answer::Noul(0.5));
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

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("发退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    a
}

struct 跑出 {
    exits: Vec<Json>,
    warnings: Vec<String>,
    value: Json,
}

fn 跑(src: &str, 表: Vec<(&'static str, Answer)>, calib: &CalibStore) -> Result<跑出, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    run(&program, 端口(表), calib, &动作表(), &mut l)
        .map(|o| 跑出 {
            exits: o.exits.clone(),
            warnings: o.trace.warnings.clone(),
            value: o.value_json(),
        })
        .map_err(|e| e.render())
}

/// 一段材料、一道是非题、一次 cut，出口种类作返回值
fn 单切(选项: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 0, depth: 16}};
let e = cut(judge(state(mat("甲")), test("行吗", "k")){选项});
let k = exit_kind(e);
consume(e, "drop");
k
"#
    )
}

fn 出口种类(o: &跑出) -> String {
    o.value.as_str().unwrap_or_default().to_string()
}

/// (a) 单线 `{hi: 0.7}`：0.7 → act、0.69 → ignore，没有 band
#[test]
fn a_单线没有未决带() {
    let c = CalibStore::new();
    let src = 单切(r#", {declare: {hi: 0.7}}"#);
    let o = 跑(&src, vec![("甲", Answer::Noul(0.7))], &c).unwrap();
    assert_eq!(出口种类(&o), "act");
    assert_eq!(o.exits[0]["grade"], "Declared");
    assert_eq!(o.exits[0]["releases"], false);
    assert_eq!(o.exits[0]["declared"]["lo"], 0.7);
    let o = 跑(&src, vec![("甲", Answer::Noul(0.69))], &c).unwrap();
    assert_eq!(出口种类(&o), "ignore");
    // 无记录：证据为 0 / null
    assert_eq!(o.exits[0]["evidence"]["labelled"], 0);
    assert_eq!(o.exits[0]["evidence"]["errors_at_line"], Json::Null);
    assert_eq!(o.exits[0]["evidence"]["certified"], Json::Null);
}

/// (b) 两侧线 `{hi: 0.7, lo: 0.3}`：0.5 → unsure(band)
#[test]
fn b_两侧线中间是未决带() {
    let c = CalibStore::new();
    let src = 单切(r#", {declare: {hi: 0.7, lo: 0.3}}"#);
    let o = 跑(&src, vec![("甲", Answer::Noul(0.5))], &c).unwrap();
    assert_eq!(出口种类(&o), "unsure(band)");
    let o = 跑(&src, vec![("甲", Answer::Noul(0.3))], &c).unwrap();
    assert_eq!(出口种类(&o), "ignore");
}

fn 样本(p: f64, 对: bool) -> jpp_effects::views::Sample {
    jpp_effects::views::Sample {
        p: Some(p),
        label: Some(u8::from(对)),
        perms: 0,
        mode_share: None,
        mode: Default::default(),
        phys: "noul".into(),
        cluster: None,
        stratum: None,
    }
}

/// (c) 同键有正式记录（线 0.9 / 0.1）时仍按声明线切；`evidence` 按记录标注算、列出认证线
#[test]
fn c_同键有正式线仍按声明线切() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.9, 0.1, 50);
    c.records.get_mut("k").unwrap().samples = vec![
        样本(0.75, false), // 按 0.7 切是 act，标注为假：错一条
        样本(0.95, true),
        样本(0.2, false),
        样本(0.5, true), // 带内，不计错
    ];
    // 不带 declare：按记录的线 0.9 / 0.1（δ 0.05）0.75 在带内
    let o = 跑(&单切(""), vec![("甲", Answer::Noul(0.75))], &c).unwrap();
    assert_eq!(出口种类(&o), "unsure(band)");
    assert_eq!(o.exits[0]["grade"], "Certified");
    // 带 declare：按作者写的 0.7 切成 act，不与记录取严、不被替换
    let o = 跑(
        &单切(r#", {declare: {hi: 0.7, lo: 0.3}}"#),
        vec![("甲", Answer::Noul(0.75))],
        &c,
    )
    .unwrap();
    assert_eq!(出口种类(&o), "act");
    let ev = &o.exits[0]["evidence"];
    assert_eq!(ev["labelled"], 4);
    assert_eq!(ev["errors_at_line"], 1);
    assert_eq!(ev["certified"]["hi"], 0.9);
    assert_eq!(ev["certified"]["grade"], "Certified");
    let w: Vec<&String> = o
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-declared-line"))
        .collect();
    assert_eq!(w.len(), 1, "{:?}", o.warnings);
    assert!(w[0].contains("按此线切错 1 条") && w[0].contains("--release-on-declared"));
}

/// (d) 声明线出口单独守不可逆 `do`：J-08 拒，报文说出作者声明线与开关名
#[test]
fn d_声明线单独守不可逆动作被拒() {
    let c = CalibStore::new();
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("要退款吗", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }})
"#;
    let e = 跑(src, vec![("甲", Answer::Noul(0.9))], &c)
        .err()
        .expect("声明线出口单独不放行不可逆动作");
    assert!(e.contains("J-08"), "{e}");
    assert!(
        e.contains("作者声明线") && e.contains("--release-on-declared"),
        "{e}"
    );
    assert!(e.contains("hi=0.7 lo=0.3"), "{e}");
}

/// (e) `E-cut-options`：`declare` 与 `cost` 或 `alpha` 同给、select 给 lo、lo > hi、越界；未知字段 `E-rt-arg`
#[test]
fn e_选项互斥与取值() {
    let c = CalibStore::new();
    let 表 = || vec![("甲", Answer::Noul(0.9))];
    for 选项 in [
        r#", {declare: {hi: 0.7}, cost: [1, 2]}"#,
        r#", {declare: {hi: 0.7}, alpha: 0.1}"#,
        r#", {declare: {hi: 0.3, lo: 0.7}}"#,
        r#", {declare: {hi: 1.5}}"#,
        r#", {declare: {lo: 0.3}}"#,
        r#", {alpha: 1.5}"#,
    ] {
        let e = 跑(&单切(选项), 表(), &c).err().expect(选项);
        assert!(e.contains("E-cut-options"), "{选项}：{e}");
    }
    let e = 跑(&单切(r#", {declare: {hi: 0.7}, cost: [1, 2]}"#), 表(), &c)
        .err()
        .unwrap();
    assert!(e.contains("声明线没有错误率保证"), "{e}");
    let e = 跑(&单切(r#", {decalre: {hi: 0.7}}"#), 表(), &c)
        .err()
        .unwrap();
    assert!(e.contains("E-rt-arg") && e.contains("decalre"), "{e}");
    let sel = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲"), {over: [mat("a"), mat("b")]}), select("哪个", "k")), {declare: {hi: 0.6, lo: 0.2}});
consume(e, "drop");
"#;
    let e = 跑(sel, 表(), &c).err().unwrap();
    assert!(e.contains("E-cut-options") && e.contains("只收 hi"), "{e}");
}

fn 证书(alpha: f64, hi: f64, n: usize, fp: &str) -> jpp::effects::Cert {
    jpp::effects::Cert {
        alpha,
        conf_delta: 0.10,
        hi,
        n_accepted: n,
        n_errors: 0,
        ucb: 0.05,
        cluster_unit: "测试合成证书".into(),
        resample: None,
        cost: None,
        bounded_side: "单侧".into(),
        label_fp: fp.into(),
        selection: None,
        grade: Default::default(),
        eff: None,
        label_source: jpp::effects::LabelSource::全体,
    }
}

/// (f) `alpha`：α ≤ a 里已决条数最多的一张，并列取 α 小者；hi 与 lo 取自同一次认证（配对下侧证书），
/// 配不上是单侧线；没有 α ≤ a 的证书即冷
#[test]
fn f_alpha选证书() {
    let mut c = CalibStore::new();
    // 甲：α 0.10、hi 0.8、已决 30、没有配对下侧（common::certified 的合成证书，指纹为空）
    common::certified(&mut c, "k", 0.8, 0.1, 30);
    let r = c.records.get_mut("k").unwrap();
    // 乙：α 0.25、hi 0.6、上侧已决 50，与下侧证书（lo 0.3、已决 20）同一次认证
    let 乙 = 证书(0.25, 0.6, 50, "fp-乙");
    r.certs.insert(乙.addr(), 乙);
    r.lower = Some(证书(0.25, 0.3, 20, "fp-乙"));
    let 选 = |a: f64, p: f64| {
        let o = 跑(
            &单切(&format!(", {{alpha: {a}}}")),
            vec![("甲", Answer::Noul(p))],
            &c,
        )
        .unwrap();
        (出口种类(&o), o.exits[0].clone(), o.warnings)
    };
    // alpha 0.2：只有甲合格，单侧线 0.8（0.7 在带内、0.85 act；Ignore 不可达）
    let (k, row, _) = 选(0.2, 0.7);
    assert_eq!(k, "unsure(band)", "{row}");
    assert_eq!(选(0.2, 0.85).0, "act");
    assert_eq!(选(0.2, 0.05).0, "unsure(band)");
    // alpha 0.3：乙已决 70 > 甲 30，取乙；两侧线 (0.6, 0.3) 来自同一次认证
    assert_eq!(选(0.3, 0.65).0, "act");
    assert_eq!(选(0.3, 0.25).0, "ignore");
    assert_eq!(选(0.3, 0.45).0, "unsure(band)");
    // alpha 0.05：没有合格证书 → 冷，修法说明「选不到证书」
    let (k, row, w) = 选(0.05, 0.99);
    assert_eq!(k, "unsure(cold|untested:alpha_line)", "{row}");
    assert_eq!(row["grade"], "Cold");
    assert!(
        w.iter()
            .any(|x| x.contains("alpha_line") && x.contains("α ≤ 0.05")),
        "{w:?}"
    );
    // 下侧证书指纹对不上：乙变单侧线，0.25 不再 ignore
    c.records.get_mut("k").unwrap().lower = Some(证书(0.25, 0.3, 20, "fp-别的"));
    let (k, _, _) = {
        let o = 跑(
            &单切(", {alpha: 0.3}"),
            vec![("甲", Answer::Noul(0.25))],
            &c,
        )
        .unwrap();
        (出口种类(&o), (), ())
    };
    assert_eq!(k, "unsure(band)");
}

/// (g) `select` 的 `declare: {hi}` 单侧：p_max ≥ hi 出 pick，否则 band
#[test]
fn g_select单侧() {
    let c = CalibStore::new();
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲"), {over: [mat("a"), mat("b")]}), select("哪个", "k")), {declare: {hi: 0.6}});
let k = exit_kind(e);
consume(e, "drop");
k
"#;
    let o = 跑(src, vec![("甲", Answer::Choice(vec![0.7, 0.3]))], &c).unwrap();
    assert_eq!(出口种类(&o), "pick(0)", "{:?}", o.exits);
    assert_eq!(o.exits[0]["grade"], "Declared");
    let o = 跑(src, vec![("甲", Answer::Choice(vec![0.55, 0.45]))], &c).unwrap();
    assert_eq!(出口种类(&o), "unsure(band)");
}

/// (h) 不带 `declare` 的 `cut` 判序与等级不变（同一程序同一库，`c_` 里已对照一次；这里对照冷键）
#[test]
fn h_不带declare照旧() {
    let c = CalibStore::new();
    let o = 跑(&单切(""), vec![("甲", Answer::Noul(0.99))], &c).unwrap();
    assert_eq!(出口种类(&o), "unsure(cold|untested:calib_line)");
    assert_eq!(o.exits[0]["grade"], "Cold");
    assert!(o.exits[0].get("declared").is_none());
    assert!(o.exits[0].get("evidence").is_none());
}

/// `near_line` 按线 ±0.2（`20` v2 §4.4 第 9 条）；`W-declared-line` 一趟一键一条
#[test]
fn 线附近读数与告警一趟一键一条() {
    let c = CalibStore::new();
    let src = r#"
budget {calls: 8, cost: 0, depth: 16};
fn 判(t) {
    let e = cut(judge(state(mat(t)), test("行吗", "k")), {declare: {hi: 0.7, lo: 0.3}});
    let k = exit_kind(e);
    consume(e, "drop");
    k
}
[判("一"), 判("二"), 判("三")]
"#;
    let o = 跑(
        src,
        vec![
            ("一", Answer::Noul(0.55)), // 离 0.7 是 0.15：计入
            ("二", Answer::Noul(0.95)), // 离 0.7 是 0.25：不计
            ("三", Answer::Noul(0.1)),  // 离 0.3 正好 0.2：计入
        ],
        &c,
    )
    .unwrap();
    let nl = &o.exits[0]["evidence"]["near_line"];
    assert_eq!(nl["count"], 2, "{nl}");
    assert_eq!(nl["share"], 0.6667);
    assert_eq!(nl["window"], 0.2);
    assert_eq!(
        o.warnings
            .iter()
            .filter(|w| w.starts_with("W-declared-line"))
            .count(),
        1
    );
}

/// 冷出口（B130）：四条出路；同键一趟只报一条
#[test]
fn 冷出口列四条出路且一趟一键一条() {
    let c = CalibStore::new();
    let src = r#"
budget {calls: 8, cost: 0, depth: 16};
fn 判(t) {
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    let k = exit_kind(e);
    consume(e, "drop");
    k
}
[判("一"), 判("二")]
"#;
    let o = 跑(src, vec![], &c).unwrap();
    let w: Vec<&String> = o
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-untested") && w.contains("calib_line"))
        .collect();
    assert_eq!(w.len(), 1, "{w:?}");
    for 出路 in ["bank/bank.json", "computed", "calib-import", "declare"] {
        assert!(w[0].contains(出路), "缺「{出路}」：{}", w[0]);
    }
    assert!(w[0].contains("【作者可改】"));
}

// ---------- 声明线入账（B142）：`CalibUsed` 键 `declared:<键>@<站点>` ----------

/// 同一站点三种写法：声明 0.7/0.3、声明 0.8/0.3、不声明（写校准键）。三份源码在 `cut` 之前逐字相同，
/// 站点（`cut` 表达式的字节起点）相同。
fn 入账程序(选项: &str) -> jpp::Program {
    lower(&parse(&单切(选项)).expect("解析")).expect("lower")
}

fn 首跑账本(c: &CalibStore) -> Ledger {
    let mut l = Ledger::new();
    run(
        &入账程序(r#", {declare: {hi: 0.7, lo: 0.3}}"#),
        端口(vec![("甲", Answer::Noul(0.75))]),
        c,
        &动作表(),
        &mut l,
    )
    .unwrap();
    l
}

fn 头告警(ws: &[String]) -> Vec<&String> {
    ws.iter().filter(|w| w.starts_with("W-header")).collect()
}

/// (k1) 首跑账本记下声明线；改 `declare` 的数后只凭账本重放报 `W-header`（旧新两组数与站点），数不变不报
#[test]
fn k1_改声明线后重放报告头不同() {
    let c = CalibStore::new();
    let l = 首跑账本(&c);
    let 声明键: Vec<&String> = l
        .calib_used
        .keys()
        .filter(|k| k.starts_with("declared:k@"))
        .collect();
    assert_eq!(
        声明键.len(),
        1,
        "{:?}",
        l.calib_used.keys().collect::<Vec<_>>()
    );
    assert_eq!(l.calib_used[声明键[0]]["record"]["line"], "declared");
    // 数不变：不报
    let mut 同 = l.clone();
    let o = jpp::run_replay(
        &入账程序(r#", {declare: {hi: 0.7, lo: 0.3}}"#),
        端口(vec![]),
        &c,
        &动作表(),
        &mut 同,
    )
    .unwrap();
    assert!(
        头告警(&o.trace.warnings).is_empty(),
        "{:?}",
        o.trace.warnings
    );
    // 改成 0.8：报
    let mut 改 = l.clone();
    let o = jpp::run_replay(
        &入账程序(r#", {declare: {hi: 0.8, lo: 0.3}}"#),
        端口(vec![]),
        &c,
        &动作表(),
        &mut 改,
    )
    .unwrap();
    let w = 头告警(&o.trace.warnings);
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(
        w[0].contains("declared 旧 hi=0.7 lo=0.3 新 hi=0.8 lo=0.3") && w[0].contains("B142"),
        "{}",
        w[0]
    );
    assert_eq!(o.value_json(), "unsure(band)", "重放按程序当前的线重算出口");
}

/// (k2) 续接（同一校准库里有同键的认证记录）不因 `declared:` 键报 `W-header`：它不与校准视图比
#[test]
fn k2_续接不拿声明键比校准视图() {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.9, 0.1, 50);
    let mut l = 首跑账本(&c);
    let o = run(
        &入账程序(r#", {declare: {hi: 0.7, lo: 0.3}}"#),
        端口(vec![("甲", Answer::Noul(0.75))]),
        &c,
        &动作表(),
        &mut l,
    )
    .unwrap();
    assert!(
        头告警(&o.trace.warnings).is_empty(),
        "{:?}",
        o.trace.warnings
    );
}

/// (k3) 账本里有该站点的声明线、程序里该站点已不声明：续接报 `W-header`（「新 无」）
#[test]
fn k3_站点已无声明线报告头不同() {
    let c = CalibStore::new();
    let mut l = 首跑账本(&c);
    let o = run(
        &入账程序(r#", "k""#),
        端口(vec![("甲", Answer::Noul(0.75))]),
        &c,
        &动作表(),
        &mut l,
    )
    .unwrap();
    let w = 头告警(&o.trace.warnings);
    assert_eq!(w.len(), 1, "{:?}", o.trace.warnings);
    assert!(w[0].contains("新 无"), "{}", w[0]);
}

/// (k4) 只凭账本重放补回校准库时跳过 `declared:` 键（它不是校准记录）
#[test]
fn k4_补回校准库跳过声明键() {
    let l = 首跑账本(&CalibStore::new());
    let mut c = CalibStore::new();
    let 补回 = jpp::Session::restore_calib(&mut c, &l).expect("声明键不该让补回失败");
    assert!(补回.iter().all(|k| !k.starts_with("declared:")), "{补回:?}");
    assert!(c.records.keys().all(|k| !k.starts_with("declared:")));
}

// ---------- 宿主接受作者声明线（B128，步 20j-2）：`--release-on-declared` / `EntryArgs.accept` ----------

fn 接受入口(接受: bool) -> jpp::EntryArgs {
    jpp::EntryArgs {
        accept: jpp::HostAccept {
            declared_lines: 接受,
        },
        ..Default::default()
    }
}

/// 按宿主入口编译（接受位随 `EntryArgs::decl()` 进 `Program.entry`，检查器读它）
fn 编译(src: &str, entry: &jpp::EntryArgs) -> jpp::Program {
    jpp::Session::compile(&parse(src).expect("解析"), &entry.decl()).expect("compile")
}

/// 经 `Session::run`（先检查、再执行）
fn 会话跑(
    src: &str,
    表: Vec<(&'static str, Answer)>,
    entry: &jpp::EntryArgs,
) -> Result<(跑出, Ledger), String> {
    let c = CalibStore::new();
    let a = 动作表();
    let mut l = Ledger::new();
    let o = jpp::Session::new(端口(表), &c, &a)
        .run(&编译(src, entry), entry, &mut l)
        .map_err(|e| e.render())?;
    Ok((
        跑出 {
            exits: o.exits.clone(),
            warnings: o.trace.warnings.clone(),
            value: o.value_json(),
        },
        l,
    ))
}

/// 跳过静态检查、只看运行期（`Interp` 直接跑）
fn 运行期跑(
    src: &str,
    表: Vec<(&'static str, Answer)>,
    entry: &jpp::EntryArgs,
) -> Result<Json, String> {
    let program = 编译(src, entry);
    let c = CalibStore::new();
    let a = 动作表();
    let mut l = Ledger::new();
    jpp::interp::Interp::new(端口(表), &mut l, &c, &a, program.budget.clone())
        .with_entry(entry.clone())
        .run(&program)
        .map(|o| o.value_json())
        .map_err(|e| format!("[{}] {}", e.rule.clone().unwrap_or_default(), e.message))
}

/// 声明线出口直接在 act 臂里守不可逆 `do`（与 (d) 同形）
const 声明守不可逆: &str = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("要退款吗", "k")), {declare: {hi: 0.7, lo: 0.3}});
handle(e, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }})
"#;

fn 入口哈希(l: &Ledger) -> Option<String> {
    l.header
        .as_ref()
        .and_then(|h| h.compared.entry_hash.clone())
}

/// (i) 同一程序：不带开关 J-08 拒（检查期与运行期都拒，运行期报文说开关名）；带开关放行执行，出口 `releases: true`，
/// 账本头 `entry_hash` 由空变有；CLI 报告带 `accept: {declared_lines: true}`
#[test]
fn i_宿主接受后放行不可逆动作() {
    let 表 = || vec![("甲", Answer::Noul(0.9))];
    let e = 会话跑(声明守不可逆, 表(), &接受入口(false))
        .err()
        .expect("不带开关拒");
    assert!(
        e.contains("J-08") && e.contains("--release-on-declared"),
        "{e}"
    );
    let e = 运行期跑(声明守不可逆, 表(), &接受入口(false)).expect_err("运行期也拒");
    assert!(
        e.contains("J-08") && e.contains("宿主未声明接受作者线放行"),
        "{e}"
    );
    let (不带, l0) = 会话跑(
        &声明守不可逆.replace(r#"do("发退款", [], 0)"#, r#""会退""#),
        表(),
        &接受入口(false),
    )
    .unwrap();
    assert_eq!(不带.exits[0]["releases"], false);
    let (带, l1) = 会话跑(声明守不可逆, 表(), &接受入口(true)).expect("带开关放行");
    assert_eq!(带.value["content"], "已退", "动作执行了：{}", 带.value);
    assert_eq!(带.exits[0]["grade"], "Declared");
    assert_eq!(带.exits[0]["releases"], true);
    assert_eq!(入口哈希(&l0), None);
    assert!(入口哈希(&l1).is_some());
    let w: Vec<&String> = 带
        .warnings
        .iter()
        .filter(|w| w.starts_with("W-declared-line"))
        .collect();
    assert!(w[0].contains("宿主已接受作者线放行"), "{}", w[0]);
    // CLI：报告的 `accept` 键只在带开关时出现
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = std::env::temp_dir().join(format!("jpp-b128-accept-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for (开关, 期望) in [(true, Some(true)), (false, None)] {
        let 路径 = |x: &str| root.join(x).display().to_string();
        let mut args: Vec<String> = vec![
            "run".into(),
            路径("examples/declare-refund.jpp"),
            "--fixtures".into(),
            路径("examples/fixtures/declare-refund.json"),
            "--calib".into(),
            路径("tests/golden/_calib/declare-refund"),
            "--ledger-out".into(),
            tmp.join("l.json").display().to_string(),
            "--output".into(),
            tmp.join("r.json").display().to_string(),
        ];
        if 开关 {
            args.push("--release-on-declared".into());
        }
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&tmp)
            .args(&args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let r: Json = serde_json::from_slice(&std::fs::read(tmp.join("r.json")).unwrap()).unwrap();
        assert_eq!(r["accept"]["declared_lines"].as_bool(), 期望, "{开关}");
    }
}

/// (j) 带开关但材料是不可信入口：仍 J-08 拒（`releases()` 不读 taint，`guard_trusted` 合取 taint），报文说材料、不说开关
#[test]
fn j_带开关而材料不可信仍拒() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let e = cut(judge(state(料), test("要退款吗", "k")), {declare: {hi: 0.7}});
handle(e, {
    act: fn() { do("发退款", [], 0) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); "转人工" }})
"#;
    let mut entry = 接受入口(true);
    entry
        .materials
        .push(jpp::EntryMat::untrusted("料", serde_json::json!("甲")));
    let 表 = || vec![("甲", Answer::Noul(0.9))];
    let e = 会话跑(src, 表(), &entry).err().expect("检查期拒");
    assert!(e.contains("J-08") && e.contains("宿主入口 料"), "{e}");
    assert!(!e.contains("--release-on-declared"), "{e}");
    let e = 运行期跑(src, 表(), &entry).expect_err("运行期拒");
    assert!(e.contains("J-08"), "{e}");
    assert!(!e.contains("宿主未声明接受作者线放行"), "{e}");
}

/// (k) 检查期（CLI 的动作表：`write_json` 不可逆）：不带开关，声明线直接守不可逆 `do` 报 J-08、报文含线与开关名；
/// 带开关不报；经函数包装不报；`stat` 线同样报；可逆动作不报
#[test]
fn k_检查期声明线守不可逆动作() {
    let 表 = jpp::actions::check_table();
    let 查 = |src: &str, 接受: bool| {
        jpp::Session::explain_with_actions(&编译(src, &接受入口(接受)), None, &表)
            .diagnostics
            .into_iter()
            .filter(|d| d.rule == "J-08")
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    let 直接 = r#"
budget {calls: 4, cost: 0, depth: 16};
handle(cut(judge(state(mat("甲")), test("要退款吗", "k")), {declare: {hi: 0.7}}), {
    act: fn() { do("write_json", ["o.json", 1], 0) },
    ignore: fn() { 0 },
    unsure: fn(u) { consume(u, "drop"); 0 }})
"#;
    let m = 查(直接, false);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("--release-on-declared") && m[0].contains("hi=0.7 lo=0.7"),
        "{}",
        m[0]
    );
    assert!(查(直接, true).is_empty());
    let 包装 = r#"
budget {calls: 4, cost: 0, depth: 16};
fn 写(x) { do("write_json", ["o.json", x], 0) }
handle(cut(judge(state(mat("甲")), test("要退款吗", "k")), {declare: {hi: 0.7}}), {
    act: fn() { 写(1) },
    ignore: fn() { 0 },
    unsure: fn(u) { consume(u, "drop"); 0 }})
"#;
    assert!(查(包装, false).is_empty());
    let 统计量 = 直接.replace(
        "{declare: {hi: 0.7}}",
        r#"{stat: "confidence", declare: {hi: 0.6}}"#,
    );
    let m = 查(&统计量, false);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("stat=confidence"), "{}", m[0]);
    // 可逆动作（CLI 的 `record_check`）不受 J-08
    let 可逆 = 直接.replace(
        r#"do("write_json", ["o.json", 1], 0)"#,
        r#"do("record_check", ["c", true], 0)"#,
    );
    assert!(查(&可逆, false).is_empty());
}

/// (l) 只凭账本重放换开关状态：报 `W-header`（`entry_hash`）；开关相同不报
#[test]
fn l_重放换开关报告头不同() {
    let src = &声明守不可逆.replace(r#"do("发退款", [], 0)"#, r#""会退""#);
    let (_, l) = 会话跑(src, vec![("甲", Answer::Noul(0.9))], &接受入口(false)).unwrap();
    let 重放 = |接受: bool| {
        let mut l = l.clone();
        let c = CalibStore::new();
        let a = 动作表();
        jpp::Session::new(端口(vec![]), &c, &a)
            .replay(&编译(src, &接受入口(接受)), &接受入口(接受), &mut l)
            .unwrap()
            .trace
            .warnings
    };
    let 同 = 重放(false);
    assert!(!同.iter().any(|w| w.contains("entry_hash")), "{同:?}");
    let 换 = 重放(true);
    assert!(
        换.iter()
            .any(|w| w.starts_with("W-header") && w.contains("entry_hash")),
        "{换:?}"
    );
}

/// (m) 检查期混合：外层守卫是不可信材料上的判断，内层是宿主未接受的声明线——报文两句都有；只带开关仍报
#[test]
fn m_检查期混合根() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 16};
let 外 = cut(judge(state(料), test("可疑吗", "k")));
handle(外, {
    act: fn() {
        handle(cut(judge(state(mat("甲")), test("要退款吗", "k")), {declare: {hi: 0.7}}), {
            act: fn() { do("write_json", ["o.json", 1], 0) },
            ignore: fn() { 0 },
            unsure: fn(u) { consume(u, "drop"); 0 }})
    },
    ignore: fn() { 0 },
    unsure: fn(u) { consume(u, "drop"); 0 }})
"#;
    let 表 = jpp::actions::check_table();
    let 查 = |接受: bool| {
        let mut entry = 接受入口(接受);
        entry
            .materials
            .push(jpp::EntryMat::untrusted("料", serde_json::json!("x")));
        jpp::Session::explain_with_actions(&编译(src, &entry), None, &表)
            .diagnostics
            .into_iter()
            .filter(|d| d.rule == "J-08")
            .map(|d| d.message)
            .collect::<Vec<_>>()
    };
    let m = 查(false);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("宿主入口 料") && m[0].contains("--release-on-declared"),
        "{}",
        m[0]
    );
    // 带开关：声明线那层成了可放行（静态按可放行计），外层仍不可信，但有一层不确定即不报（零假拒绝）
    assert!(查(true).is_empty(), "{:?}", 查(true));
}
