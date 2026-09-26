//! 步 25d：B161（合成出口累加 α 成联合界、读法内置 `cert`、`compose` 开放为 `.jpp` 名字）与 B162（J-05 的责任
//! 按账本键计一次、`W-duty-twice`、报告 `duties` 表）的旁路测试。
//!
//! 依据：`地基/附注/2026-09-26-批6裁定.md` §九、§十；预注册 `地基/过程记录/工程-步25d.md` §四。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::ActionRegistry;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Question, State};
use jpp::{lower, syntax::parse};

fn 端口<'a>() -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("m", |_s: &State, qs: &[&Question]| {
            Ok(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| {
                        Answer::Noul(if q.text.contains("未决") {
                            0.5
                        } else if q.text.contains("不") {
                            0.05
                        } else {
                            0.95
                        })
                    })
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![None; qs.len()],
                perms: vec![0; qs.len()],
                confidence: vec![],
            })
        }))
        .with(FnPort::ask("m", |_s, _q| Ok(Some(Answer::Noul(0.95)))))
}

struct 结果 {
    r: Result<serde_json::Value, String>,
    告警: Vec<String>,
    duties: Vec<serde_json::Value>,
}

fn 跑(src: &str) -> 结果 {
    let program = lower(&parse(src).unwrap_or_else(|e| panic!("解析：{e:?}"))).expect("lower");
    let mut calib = CalibStore::new();
    // 认证线（测试合成证书，α = 0.10）与夹具线（宿主 put、没有证书）
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    calib.put("fx", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
    // 试用线：证书按试用 α 认证（B72）
    common::certified(&mut calib, "tr", 0.8, 0.2, 50);
    for c in calib.records.get_mut("tr").unwrap().certs.values_mut() {
        c.grade = jpp::effects::CertGrade::Trial;
    }
    // 范围未知：正式证书、记录没有材料指纹（B104-2）
    common::certified(&mut calib, "su", 0.8, 0.2, 50);
    calib.records.get_mut("su").unwrap().scope = None;
    let actions = ActionRegistry::new();
    let mut ledger = Ledger::new();
    let it = jpp::interp::Interp::new(
        端口(),
        &mut ledger,
        &calib,
        &actions,
        program.budget.clone(),
    );
    match it.run(&program) {
        Ok(o) => 结果 {
            r: Ok(o.value_json()),
            告警: o.trace.warnings.clone(),
            duties: o.duties.clone(),
        },
        Err(e) => 结果 {
            r: Err(e.render()),
            告警: vec![],
            duties: vec![],
        },
    }
}

fn 值(x: &结果) -> &serde_json::Value {
    x.r.as_ref().unwrap_or_else(|e| panic!("程序应当跑完：{e}"))
}

fn 近(v: &serde_json::Value, x: f64) -> bool {
    v.as_f64().is_some_and(|a| (a - x).abs() < 1e-9)
}

#[test]
fn cert_三种出口() {
    // 认证线出口带证书 α；夹具线按 1 计、算一个未知；ask 出口人答即真值，计 0
    let x = 跑(r#"budget {calls: 8, cost: 1, depth: 16, escalate: 2};
let s = state(mat("甲"));
let a = cut(judge(s, test("行吗", "k")));
let b = cut(judge(s, test("行吗", "fx")));
let h = ask(s, test("行吗", "k"));
{a: cert(a), b: cert(b), h: cert(h)}
"#);
    let v = 值(&x);
    assert_eq!(v["a"]["grade"], "Certified");
    assert!(
        近(&v["a"]["alpha"], 0.10) && 近(&v["a"]["alpha_bound"], 0.10),
        "{}",
        v["a"]
    );
    assert_eq!(v["a"]["n_unknown"], 0);
    assert_eq!(v["b"]["grade"], "Fixture");
    assert!(v["b"]["alpha"].is_null(), "{}", v["b"]);
    assert!(近(&v["b"]["alpha_bound"], 1.0));
    assert_eq!(v["b"]["n_unknown"], 1);
    assert!(
        近(&v["h"]["alpha"], 0.0) && 近(&v["h"]["alpha_bound"], 0.0),
        "{}",
        v["h"]
    );
    assert_eq!(v["h"]["n_unknown"], 0);
}

#[test]
fn compose_规则与联合界() {
    // any / all / {first: k} 的种类；联合界 min(1, Σα)，未知分量按 1 计；first 只计读到的分量；合成套合成取内层的界
    let x = 跑(r#"budget {calls: 8, cost: 1, depth: 16};
let s = state(mat("甲"));
let x1 = cut(judge(s, test("行吗一", "k")));
let x2 = cut(judge(s, test("不行吗二", "k")));
let x3 = cut(judge(s, test("行吗三", "fx")));
{any: exit_kind(compose([x1, x2], "any")), all: exit_kind(compose([x1, x2], "all")),
 first: exit_kind(compose([x2, x1], {first: 1})),
 two: cert(compose([x1, x2], "all")),
 unknown: cert(compose([x1, x3], "all")),
 first_read: cert(compose([x1, x2, x3], {first: 1})),
 nested: cert(compose([compose([x1, x2], "any"), x1], "all"))}
"#);
    let v = 值(&x);
    assert_eq!(v["any"], "act");
    assert_eq!(v["all"], "ignore");
    assert_eq!(v["first"], "act");
    assert!(近(&v["two"]["alpha_bound"], 0.2), "{}", v["two"]);
    assert!(v["two"]["alpha"].is_null(), "合成出口没有自己的 α");
    assert!(近(&v["unknown"]["alpha_bound"], 1.0), "{}", v["unknown"]);
    assert_eq!(v["unknown"]["n_unknown"], 1);
    assert!(
        近(&v["first_read"]["alpha_bound"], 0.1),
        "只读到 x1：{}",
        v["first_read"]
    );
    assert_eq!(v["first_read"]["n_unknown"], 0);
    assert!(近(&v["nested"]["alpha_bound"], 0.3), "{}", v["nested"]);
}

#[test]
fn compose_规则封闭() {
    // min、sup 走同一入口（种类的真值表在 compose.rs 单元测试里）；规则集外的写法报 E-rt-arg
    let x = 跑(r#"budget {calls: 8, cost: 1, depth: 16};
let s = state(mat("甲"));
let x1 = cut(judge(s, test("行吗一", "k")));
compose([x1], "xor")
"#);
    let e = x.r.unwrap_err();
    assert!(e.contains("E-rt-arg") && e.contains("封闭规则集"), "{e}");
    let y = 跑(r#"budget {calls: 8, cost: 1, depth: 16};
compose([], "min")
"#);
    assert!(y.r.unwrap_err().contains("min 至少要一个分量"));
    let z = 跑(r#"budget {calls: 8, cost: 1, depth: 16};
compose([], {sup: [0, 1]})
"#);
    assert!(z.r.unwrap_err().contains("sup 至少要一个分量"));
}

const 同键: &str = r#"budget {calls: 8, cost: 1, depth: 16};
let r = judge(state(mat("甲")), test("未决吗", "k"));
let a = cut(r);
let b = cut(r);
"#;

#[test]
fn 同键两个持有者_只带一个也有去向() {
    // B162：a、b 是同一判断（同一账本键）的两份未决，只返回 a，b 是同一责任的另一个视图
    let x = 跑(&format!("{同键}{{a: a}}\n"));
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert!(
        x.告警
            .iter()
            .any(|w| w == "returned_unsure: unsure(band), unsure(band)"),
        "{:?}",
        x.告警
    );
    assert!(
        x.duties.is_empty(),
        "只有一个持有者在返回值里，不列 duties 表"
    );
}

#[test]
fn 同键两处都带_报告列持有者() {
    let x = 跑(&format!("{同键}{{x: a, y: [b]}}\n"));
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert_eq!(x.duties.len(), 1, "{:?}", x.duties);
    assert_eq!(x.duties[0]["holders"], serde_json::json!(["$.x", "$.y[0]"]));
}

#[test]
fn 同键消费两次_报_w_duty_twice() {
    let x = 跑(&format!(
        "{同键}consume(a, \"drop\");\nconsume(b, \"drop\");\n1\n"
    ));
    assert!(x.r.is_ok(), "{:?}", x.r);
    assert!(
        x.告警.iter().any(|w| w.starts_with("W-duty-twice:")),
        "{:?}",
        x.告警
    );
}

#[test]
fn 同键都不处理_报_j05() {
    let x = 跑(&format!("{同键}1\n"));
    assert!(x.r.unwrap_err().contains("[J-05]"));
}

#[test]
fn cert_保守_试用与范围未知按一计() {
    // 主会话 2026-09-26（B161 保守读法）：试用、范围未知的出口，证书上的 α 在这份材料上不成立，按 1 计入 n_unknown
    let x = 跑(r#"budget {calls: 8, cost: 1, depth: 16};
let s = state(mat("甲"));
let t = cut(judge(s, test("行吗", "tr")));
let u = cut(judge(s, test("行吗", "su")));
{t: cert(t), u: cert(u), both: cert(compose([t, u], "all"))}
"#);
    let v = 值(&x);
    assert_eq!(v["t"]["grade"], "Trial");
    assert!(v["t"]["alpha"].is_null(), "{}", v["t"]);
    assert_eq!(v["t"]["n_unknown"], 1);
    assert!(v["u"]["alpha"].is_null(), "{}", v["u"]);
    assert_eq!(v["u"]["n_unknown"], 1);
    assert!(近(&v["both"]["alpha_bound"], 1.0), "{}", v["both"]);
    assert_eq!(v["both"]["n_unknown"], 2);
}
