//! `order` 的两处订正（步 15k）：B166 一条 `select` 读数按候选概率分档（不产生出口，未置换报
//! `W-order-unpermuted`）；B167 `order(rs, {stat?, tie?})` 与 `cut` 共用 `stat_of`，`measure` 读数缺省按档位，
//! `expect` 按 `tie` 并档，选项错报 `E-order-options`，`confidence` 取不到报 `E-stat-unavailable`。
//! 闭包端口，不发请求。
//!
//! 依据：B166、B167（地基/附注/2026-09-26-批6裁定.md §十四、§十五）；B63；B64；B154 (3)；
//! 预注册 `地基/过程记录/工程-步15k.md` 一·2·8 (a)–(h)。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Outcome, Session, lower, syntax::parse};
use serde_json::json;

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    c.put("k", 0.75, 0.25, 50, "上岗", Some(0.05)).unwrap();
    c
}

/// 判断端口：按材料文字查表给答案；`置换` 为真时每题报一次置换测量（perms 2、众数占比 1）
fn 端口<'a>(model: &str, 置换: bool) -> Ports<'a> {
    Ports::new().with(FnPort::judge(model, move |s, qs| {
        let text = s.on_text();
        let answers: Vec<Answer> = qs
            .iter()
            .map(|q| match (q.scale.is_empty(), text.as_str()) {
                (true, _) if !q.text.contains("哪") => {
                    Answer::Noul(if text.contains("好") { 0.9 } else { 0.1 })
                }
                (true, "四候选甲") => Answer::Choice(vec![0.5, 0.3, 0.15, 0.05]),
                (true, "四候选乙") => Answer::Choice(vec![0.4, 0.38, 0.2, 0.02]),
                (true, _) => Answer::Choice(vec![0.7, 0.2, 0.1, 0.0]),
                (false, "r0") => Answer::Score(vec![0.1, 0.2, 0.7, 0.0]),
                (false, "r1") => Answer::Score(vec![0.6, 0.4, 0.0, 0.0]),
                (false, "r2") => Answer::Score(vec![0.5, 0.05, 0.0, 0.45]),
                (false, _) => Answer::Score(vec![0.1, 0.1, 0.1, 0.7]),
            })
            .collect();
        let n = answers.len();
        Ok::<_, EffectError>(JudgeResult {
            answers,
            tokens: 0,
            cost: 0.0,
            mode_share: if 置换 { vec![Some(1.0); n] } else { vec![] },
            perms: if 置换 { vec![2; n] } else { vec![] },
            confidence: vec![],
        })
    }))
}

fn 跑_用(src: &str, ports: Ports<'_>) -> Result<Outcome, String> {
    let program = lower(&parse(src).expect("parse")).expect("lower");
    let calib = 库();
    let acts = ActionRegistry::new();
    Session::new(ports, &calib, &acts)
        .run(&program, &EntryArgs::default(), &mut Ledger::new())
        .map_err(|e| e.render())
}

fn 跑(src: &str) -> Result<Outcome, String> {
    跑_用(src, 端口("fixed-0", false))
}

const 选择: &str = r#"budget {calls: 8, cost: 0, depth: 64};
let q = select("哪一个候选最合适？", "k");
let cands = {over: [mat("A"), mat("B"), mat("C"), mat("D")]};
"#;

const 打分: &str = r#"budget {calls: 8, cost: 0, depth: 64};
let q = measure("这个方案的完成度", ["差", "一般", "可用", "好"], "k");
let rs = judge([state(mat("r0")), state(mat("r1")), state(mat("r2"))], q);
"#;

fn 告警(o: &Outcome, code: &str) -> usize {
    o.trace
        .warnings
        .iter()
        .filter(|w| w.starts_with(code))
        .count()
}

/// (a) 一条 select 读数：候选下标按概率分档，相邻差 ≤ δ（0.05）并档
#[test]
fn a_一条_select_读数按候选分档() {
    for (材料, 预期) in [
        ("四候选甲", json!([[0], [1], [2], [3]])),
        ("四候选乙", json!([[0, 1], [2], [3]])),
    ] {
        let src = format!("{选择}let r = judge(state(mat(\"{材料}\"), cands), q);\norder(r)");
        let o = 跑(&src).unwrap();
        assert_eq!(o.value_json(), 预期, "{材料}");
    }
}

/// (b) 没有置换测量时报 W-order-unpermuted，同一站点一趟一条；报了置换测量时不报
#[test]
fn b_未置换告警() {
    let src = format!(
        "{选择}let rs = map([\"四候选甲\", \"四候选乙\"], fn(t) {{ judge(state(mat(t), cands), q) }});
map(rs, fn(r) {{ order(r) }})"
    );
    let o = 跑(&src).unwrap();
    assert_eq!(告警(&o, "W-order-unpermuted"), 1, "{:?}", o.trace.warnings);
    let o = 跑_用(&src, 端口("fixed-0", true)).unwrap();
    assert_eq!(告警(&o, "W-order-unpermuted"), 0, "{:?}", o.trace.warnings);
    assert_eq!(
        o.value_json(),
        json!([[[0], [1], [2], [3]], [[0, 1], [2], [3]]])
    );
}

/// (c) 不产生出口：order 与 cut 并存时报告 exits 只有 cut 那一行，出口种类与不调 order 时相同
#[test]
fn c_不产生出口() {
    let 基 = format!("{选择}let r = judge(state(mat(\"四候选甲\"), cands), q);\nlet e = cut(r);\n");
    let 有 = 跑(&(基.clone() + "{kind: exit_kind(e), e: e, tiers: order(r)}")).unwrap();
    let 无 = 跑(&(基 + "{kind: exit_kind(e), e: e}")).unwrap();
    assert_eq!(有.exits.len(), 1, "{:?}", 有.exits);
    assert_eq!(有.value_json()["kind"], 无.value_json()["kind"]);
    assert_eq!(有.value_json()["tiers"], json!([[0], [1], [2], [3]]));
}

/// (d) 三条打分读数：缺省按档位（2、0、0），expect 按期望档位（1.6、0.4、1.4），max 为旧键（0.7、0.6、0.5）
#[test]
fn d_打分读数缺省按档位() {
    for (选项, 预期) in [
        ("", json!([[0], [1, 2]])),
        (", {stat: \"argmax\"}", json!([[0], [1, 2]])),
        (", {stat: \"expect\"}", json!([[0], [2], [1]])),
        (", {stat: \"max\"}", json!([[0], [1], [2]])),
    ] {
        let o = 跑(&format!("{打分}order(rs{选项})")).unwrap();
        assert_eq!(o.value_json(), 预期, "选项「{选项}」");
    }
}

/// (e) expect 按 tie 并档：1.6 与 1.4 相差 0.2 ≤ 0.25
#[test]
fn e_expect_按_tie_并档() {
    let o = 跑(&format!("{打分}order(rs, {{stat: \"expect\", tie: 0.25}})")).unwrap();
    assert_eq!(o.value_json(), json!([[0, 2], [1]]));
    let o = 跑(&format!("{打分}order(rs, {{stat: \"expect\", tie: 0}})")).unwrap();
    assert_eq!(o.value_json(), json!([[0], [2], [1]]));
}

/// (f) 选项错报 E-order-options；confidence 取不到报 E-stat-unavailable（非固定端口、判断器没报）
#[test]
fn f_选项错() {
    let 是非 = r#"budget {calls: 8, cost: 0, depth: 64};
let t = test("这个好吗？", "k");
let ts = judge([state(mat("好")), state(mat("坏"))], t);
"#;
    let 单选 = format!("{选择}let r = judge(state(mat(\"四候选甲\"), cands), q);\n");
    for src in [
        format!("{是非}order(ts, {{stat: \"expect\"}})"),
        format!("{单选}order(r, {{stat: \"max\"}})"),
        format!("{打分}order(rs, {{stat: \"max\", tie: 0.1}})"),
        format!("{打分}order(rs, {{by: \"expect\"}})"),
        format!("{打分}order(rs, {{stat: \"expect\", tie: -1}})"),
        format!("{打分}order(rs, \"expect\")"),
    ] {
        let e = 跑(&src).err().unwrap_or_else(|| panic!("该报错：{src}"));
        assert!(e.contains("E-order-options"), "{src}\n{e}");
    }
    let e = 跑_用(
        &format!("{打分}order(rs, {{stat: \"confidence\"}})"),
        端口("fn-0", false),
    )
    .expect_err("判断器没报 confidence");
    assert!(e.contains("E-stat-unavailable"), "{e}");
}

/// (g) 两层嵌套：map 里每组各排一次；sieve 的接受流读数排序后取第一档
#[test]
fn g_两层嵌套() {
    let src = r#"budget {calls: 12, cost: 0, depth: 64};
let q = measure("这个方案的完成度", ["差", "一般", "可用", "好"], "k");
let groups = [[mat("r0"), mat("r1")], [mat("r1"), mat("r2")]];
let per = map(groups, fn(g) { order(judge(map(g, fn(m) { state(m) }), q), {stat: "expect"}) });
let t = test("这个好吗？", "k");
let ok = sieve([mat("好一"), mat("坏"), mat("好二")], t).value;
let tiers = order(judge(map(ok, fn(e) { state(e.item) }), t));
{per: per, top: map(tiers[0], fn(i) { content(ok[i].item) })}"#;
    let o = 跑(src).unwrap();
    let v = o.value_json();
    assert_eq!(v["per"], json!([[[0], [1]], [[1], [0]]]));
    assert_eq!(v["top"], json!(["好一", "好二"]), "两条 0.9 并档");
}

/// (h) 静态检查放过：一条 select 读数的 order 与带 stat 的 order 都没有诊断
#[test]
fn h_检查放过() {
    for src in [
        format!("{选择}let r = judge(state(mat(\"四候选甲\"), cands), q);\norder(r)"),
        format!("{打分}order(rs, {{stat: \"expect\", tie: 0.1}})"),
    ] {
        let p = lower(&parse(&src).expect("parse")).expect("lower");
        let r = jpp::check(&p);
        assert!(r.diagnostics.is_empty(), "{src}\n{:?}", r.diagnostics);
    }
}
