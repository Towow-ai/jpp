//! 步 23c 独立审查的复现（r1–r8）改成回归测试：每条两臂，惰性过桥（`lazy_cut`）或直线段提升（`lift`）
//! 开 / 关，关 = 改前行为。审查修复之后两臂在值、动作执行次数、判断与缺席条目上相同；唯一登记的已知
//! 差异是 r2 的 `spent` 归属（修复 6）。
//!
//! 依据：B94、B72-4、B93；预注册 `地基/过程记录/工程-步23c.md` §四。审查者原稿在会话暂存区
//! `r23c/…/zz_review_23c.rs`，这里只把打印改成断言，并补 r9（修复 5）。

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jpp::effects::{CalibStore, CertGrade, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, Passes, TaintOut};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};

fn 端口<'a>(每次: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        每次.borrow_mut().push(qs.len());
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
}

fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    common::certified(&mut c, "k2", 0.8, 0.2, 50);
    common::certified(&mut c, "t", 0.8, 0.2, 50);
    for cert in c.records.get_mut("t").unwrap().certs.values_mut() {
        cert.grade = CertGrade::Trial;
    }
    // 严线：0.95 落在带内 → Unsure(band)
    common::certified(&mut c, "hi", 0.99, 0.2, 50);
    c
}

struct 结果 {
    r: Result<serde_json::Value, String>,
    每次: Vec<usize>,
    执行: u32,
    judge条目: usize,
    absent条目: usize,
    告警: Vec<String>,
}

fn 跑(src: &str, lazy: bool, lift: bool) -> 结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = 库();
    let 每次 = RefCell::new(vec![]);
    let 计数 = Rc::new(Cell::new(0u32));
    let mut actions = ActionRegistry::new();
    let c1 = 计数.clone();
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        c1.set(c1.get() + 1);
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let c2 = 计数.clone();
    actions.register("记一笔", 0.0, true, TaintOut::Trusted, move |_| {
        c2.set(c2.get() + 1);
        Ok(Value::Text("记了".into(), Taint::Trusted.into()))
    });
    let mut ledger = Ledger::new();
    let (r, 告警) = {
        let mut it = jpp::interp::Interp::new(
            端口(&每次),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        );
        it.passes = Passes {
            lazy_cut: lazy,
            lift,
            ..Passes::default()
        };
        match it.run(&program) {
            Ok(o) => {
                let w = o.trace.warnings.clone();
                (Ok(o.value_json()), w)
            }
            Err(e) => (Err(e.render()), vec![]),
        }
    };
    let judge条目 = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    let absent条目 = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Absent { .. }))
        .count();
    let 每次 = 每次.borrow().clone();
    结果 {
        r,
        每次,
        执行: 计数.get(),
        judge条目,
        absent条目,
        告警,
    }
}

fn 有推测未用(x: &结果) -> bool {
    x.告警.iter().any(|w| w.starts_with("W-spec-unused"))
}

fn 报(名: &str, x: &结果) {
    eprintln!(
        "[{名}] r={:?}\n    每次={:?} 执行={} judge条目={} absent条目={}\n    告警={:?}",
        x.r, x.每次, x.执行, x.judge条目, x.absent条目, x.告警
    );
}

#[test]
fn r1_transform_j11() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let m = transform(fn(x) { cut(judge(state(x), test("行吗", "k"))) }, mat("甲"));
content(m)
"#;
    let (关, 开) = (跑(src, false, true), 跑(src, true, true));
    报("r1 关", &关);
    报("r1 开", &开);
    // 修复 2：闭包返回的惰性出口先检视，照改前报 J-11
    assert!(关.r.as_ref().unwrap_err().contains("[J-11]"));
    assert_eq!(开.r, 关.r);
}

#[test]
fn r2_sieve_evidence_spent() {
    let src = r#"
budget {calls: 10, cost: 1, depth: 16};
let e = cut(judge(state(mat("整份输出")), test("有报错吗", "k")));
let r = sieve(["块一", "块二", "块三"], test("相关吗", "k"));
let k = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {r: "unsure", exit: u} }});
{k: k, ev: len(r.evidence), sp: r.spent}
"#;
    let (关, 开) = (跑(src, false, true), 跑(src, true, true));
    报("r2 关", &关);
    报("r2 开", &开);
    // 修复 6（已知变化，登记不改）：`sieve` 入口前登记、未检视的判断随 `sieve` 的刷新发出，算进它的
    // `spent`；其余字段与调用序列两臂相同
    let (a, b) = (关.r.clone().unwrap(), 开.r.clone().unwrap());
    assert_eq!(a["sp"]["calls"], 3);
    assert_eq!(b["sp"]["calls"], 4);
    assert_eq!((&a["k"], &a["ev"]), (&b["k"], &b["ev"]));
    assert_eq!(开.每次, 关.每次);
}

#[test]
fn r3_budget_do_order() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let a = do("记一笔", [], 0);
let b = do("记一笔", [], 0);
{k: exit_kind(e), e: e}
"#;
    let (关, 开) = (跑(src, false, true), 跑(src, true, true));
    报("r3 关", &关);
    报("r3 开", &开);
    // 修复 3a：`do` 前先解析已登记的出口，判断先于效应计费，出口仍是 act、第二次 do 发不起
    assert_eq!(关.r.as_ref().unwrap()["k"], "act");
    assert_eq!(开.r, 关.r);
    assert_eq!((开.执行, 关.执行), (1, 1));
    assert_eq!(开.每次, 关.每次);
}

const 发: &str = r#"content(do("发邮件", [], 0))"#;

#[test]
fn r4_lineage_trial() {
    let src = format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
let r = judge(state(mat("甲")), test("选吗", "k"));
let strict = cut(r, "t");
let loose = cut(r);
let m = mat(loose);
let out = handle(cut(judge(state(m), test("该发吗", "k2"))), {{act: fn() {{ {发} }}, ignore: fn() {{ "不发" }}, unsure: fn(u) {{ consume(u, "drop"); "不发" }}}});
{{out: out, strict: exit_kind(strict)}}
"#
    );
    let (关, 开) = (跑(&src, false, true), 跑(&src, true, true));
    报("r4 关", &关);
    报("r4 开", &开);
    // 修复 1：谱系先解析同键的全部出口再查放行表；不放行的那条线让 J-08 照改前拦下不可逆动作
    assert!(关.r.as_ref().unwrap_err().contains("[J-08]"));
    assert_eq!(开.r, 关.r);
    assert_eq!((开.执行, 关.执行), (0, 0));
}

#[test]
fn r4b_lineage_unsure_then_j05() {
    let src = format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
let r = judge(state(mat("甲")), test("选吗", "k"));
let strict = cut(r, "hi");
let loose = cut(r);
let m = mat(loose);
handle(cut(judge(state(m), test("该发吗", "k2"))), {{act: fn() {{ {发} }}, ignore: fn() {{ "不发" }}, unsure: fn(u) {{ consume(u, "drop"); "不发" }}}})
"#
    );
    let (关, 开) = (跑(&src, false, true), 跑(&src, true, true));
    报("r4b 关", &关);
    报("r4b 开", &开);
    // 修复 1：谱系先解析同键的全部出口再查放行表；不放行的那条线让 J-08 照改前拦下不可逆动作
    assert!(关.r.as_ref().unwrap_err().contains("[J-08]"));
    assert_eq!(开.r, 关.r);
    assert_eq!((开.执行, 关.执行), (0, 0));
}

#[test]
fn r5_andand_rhs_lift() {
    let src = r#"
budget {calls: 8, cost: 1, depth: 8};
fn g(s, q) {
    let e = cut(judge(s, q));
    {ok: exit_kind(e) == "act", e: e}
}
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let q2 = test("行吗", "k");
let a = g(s, q1);
let ok = a.ok && g(s, q2).ok;
{ok: ok, a: a}
"#;
    let (关, 开) = (跑(src, true, false), 跑(src, true, true));
    报("r5 lift关", &关);
    报("r5 lift开", &开);
    // 修复 4：`&&` 右侧不一定走到，直线段提升遇短路即停
    assert_eq!(开.每次, vec![1]);
    assert_eq!(开.每次, 关.每次);
    assert_eq!(开.r, 关.r);
    assert!(!有推测未用(&开));
}

#[test]
fn r5b_andand_rhs_lift_budget() {
    // 预算 0：a 的组（真站点 q1 + 推测 q2）整组停发；q2 未走到也记 Absent
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
fn g(s, q) {
    let e = cut(judge(s, q));
    {ok: exit_kind(e) == "act", e: e}
}
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let q2 = test("行吗", "k");
let z = cut(judge(state(mat("乙")), test("行吗0", "k")));
let a = g(s, q1);
let ok = a.ok && g(s, q2).ok;
{ok: ok, a: a, z: z}
"#;
    let (关, 开) = (跑(src, true, false), 跑(src, true, true));
    报("r5b lift关", &关);
    报("r5b lift开", &开);
    assert_eq!(开.r, 关.r);
    assert_eq!((开.judge条目, 开.absent条目), (关.judge条目, 关.absent条目));
    assert!(!有推测未用(&开));
}

type 两跑 = (
    Result<serde_json::Value, String>,
    Result<serde_json::Value, String>,
    Vec<usize>,
);

fn 首跑再重放(src: &str) -> 两跑 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = 库();
    let 计数 = Rc::new(Cell::new(0u32));
    let mut actions = ActionRegistry::new();
    let c1 = 计数.clone();
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        c1.set(c1.get() + 1);
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let c2 = 计数.clone();
    actions.register("记一笔", 0.0, true, TaintOut::Trusted, move |_| {
        c2.set(c2.get() + 1);
        Ok(Value::Text("记了".into(), Taint::Trusted.into()))
    });
    let mut ledger = Ledger::new();
    let 每次 = RefCell::new(vec![]);
    let first = {
        let mut it = jpp::interp::Interp::new(
            端口(&每次),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        );
        it.passes = Passes::default();
        it.run(&program)
            .map(|o| o.value_json())
            .map_err(|e| e.render())
    };
    let 每次2 = RefCell::new(vec![]);
    let again = {
        let mut it = jpp::interp::Interp::new(
            端口(&每次2),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        )
        .audit_replay();
        it.passes = Passes::default();
        it.run(&program)
            .map(|o| o.value_json())
            .map_err(|e| e.render())
    };
    eprintln!(
        "[重放] first={first:?}\n        again={again:?}\n        重放调用={:?} 动作执行={}",
        每次2.borrow(),
        计数.get()
    );
    let 重放调用 = 每次2.borrow().clone();
    (first, again, 重放调用)
}

#[test]
fn r6_replay_consistency() {
    // 修复 3a：首跑与审计重放在同一处先解析出口，值相同、重放不发调用
    let 两次 = [
        首跑再重放(
            r#"
budget {calls: 2, cost: 1, depth: 8};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let a = do("记一笔", [], 0);
let b = do("记一笔", [], 0);
{k: exit_kind(e), e: e}
"#,
        ),
        首跑再重放(
            r#"
budget {calls: 1, cost: 1, depth: 8};
fn g(s, q) {
    let e = cut(judge(s, q));
    {ok: exit_kind(e) == "act", e: e}
}
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let q2 = test("行吗", "k");
let z = cut(judge(state(mat("乙")), test("行吗0", "k")));
let a = g(s, q1);
let ok = a.ok && g(s, q2).ok;
{ok: ok, a: a, z: z}
"#,
        ),
    ];
    for (first, again, 重放调用) in 两次 {
        assert!(first.is_ok());
        assert_eq!(again, first);
        assert!(重放调用.is_empty());
    }
}

#[test]
fn r7_straight_line_spec_group_ahead() {
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
fn g(s, q) { let e = cut(judge(s, q)); {ok: exit_kind(e) == "act", e: e} }
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let q2 = test("行吗", "k");
let z = cut(judge(state(mat("乙")), test("行吗0", "k")));
let a = g(s, q1);
let b = g(s, q2);
{a: a.e, b: b.e, z: z}
"#;
    let (关, 开) = (跑(src, true, false), 跑(src, true, true));
    报("r7 lift关", &关);
    报("r7 lift开", &开);
    // 修复 3b：提升登记的组排在程序序更靠前的真站点（z）之后，预算只够一次时发的是 z
    assert_eq!(开.r.as_ref().unwrap()["z"]["exit"], "act");
    assert_eq!(开.r, 关.r);
    assert_eq!((开.judge条目, 开.absent条目), (关.judge条目, 关.absent条目));
}

#[test]
fn r8_unreached_spec_absent() {
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
fn g(s, q) { let e = cut(judge(s, q)); {ok: exit_kind(e) == "act", e: e} }
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let z = cut(judge(state(mat("乙")), test("行吗0", "k")));
let q2 = test("行吗", "k");
let a = g(s, q1);
let ok = a.ok && g(s, q2).ok;
{ok: ok, a: a, z: z}
"#;
    let (关, 开) = (跑(src, true, false), 跑(src, true, true));
    报("r8 lift关", &关);
    报("r8 lift开", &开);
    // 修复 3b + 4：没走到的推测项不记缺席账
    assert_eq!(开.r, 关.r);
    assert_eq!(开.absent条目, 关.absent条目);
    assert!(!有推测未用(&开));
}

#[test]
fn r9_block_keeps_born() {
    // 修复 5：块表达式沿用外层 `born`。`s` 在段内重新绑定，块里的 `g(s, q2)` 此刻算不出状态，不收；
    // 改前块内另起空集，按旧的 `s`（甲）提升，与 q1 凑成两处，多问一道没被用上的题
    let src = r#"
budget {calls: 8, cost: 1, depth: 8};
fn g(s, q) { let e = cut(judge(s, q)); {ok: exit_kind(e) == "act", e: e} }
let s = state(mat("甲"));
let q1 = test("不行吗", "k");
let q2 = test("行吗", "k");
let a = g(s, q1);
let s = state(mat("乙"));
let b = { let t = 1; g(s, q2) };
{a: a.e, b: b.e}
"#;
    let (关, 开) = (跑(src, true, false), 跑(src, true, true));
    报("r9 lift关", &关);
    报("r9 lift开", &开);
    assert_eq!(开.每次, 关.每次);
    assert_eq!(开.r, 关.r);
    assert!(!有推测未用(&开));
}

// ── 复核（主会话 2026-09-25）：v1a–v1e、v4a、v5 取自复核者的 `r23c2/…/zz_recheck_23c.rs`，v5w 为本轨补。
// 预注册 `工程-步23c.md` §六。

/// 复核用端口：状态里含「坏」的判断，客户端报错（没声明 absent 策略 = 运行期错误）
fn 复核端口<'a>(每次: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |s: &State, qs: &[&Question]| {
        if s.on_text().contains("坏") {
            return Err(jpp::effects::EffectError("判断器挂了".into()));
        }
        每次.borrow_mut().push(qs.len());
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| Answer::Noul(if q.text.contains("不") { 0.05 } else { 0.95 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
            confidence: vec![],
        })
    }))
}

struct 复核结果 {
    /// 值（JSON）或错误全文
    r: String,
    每次: Vec<usize>,
    动作: u32,
    judge条目: usize,
    absent条目: usize,
    告警: Vec<String>,
}

impl 复核结果 {
    /// 两臂比较的部分（告警单独核）
    fn 可比(&self) -> (&str, &[usize], u32, usize, usize) {
        (
            &self.r,
            &self.每次,
            self.动作,
            self.judge条目,
            self.absent条目,
        )
    }
    fn 推测未用(&self) -> bool {
        self.告警.iter().any(|w| w.starts_with("W-spec-unused"))
    }
}

fn 复核跑(src: &str, passes: Passes) -> 复核结果 {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = 库();
    let 每次 = RefCell::new(vec![]);
    let 计数 = Rc::new(Cell::new(0u32));
    let mut actions = ActionRegistry::new();
    let c1 = 计数.clone();
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, move |_| {
        c1.set(c1.get() + 1);
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let mut ledger = Ledger::new();
    let (r, 告警) = {
        let mut it = jpp::interp::Interp::new(
            复核端口(&每次),
            &mut ledger,
            &calib,
            &actions,
            program.budget.clone(),
        );
        it.passes = passes;
        match it.run(&program) {
            Ok(o) => (o.value_json().to_string(), o.trace.warnings.clone()),
            Err(e) => (format!("Err {}", e.render()), vec![]),
        }
    };
    let 数 = |f: fn(&Entry) -> bool| ledger.entries.iter().filter(|e| f(e)).count();
    let 结果 = 复核结果 {
        r,
        每次: 每次.borrow().clone(),
        动作: 计数.get(),
        judge条目: 数(|e| matches!(e, Entry::Judge { .. })),
        absent条目: 数(|e| matches!(e, Entry::Absent { .. })),
        告警,
    };
    eprintln!(
        "    r={} 每次={:?} 动作={} judge条目={} absent条目={} 告警={:?}",
        结果.r.chars().take(160).collect::<String>(),
        结果.每次,
        结果.动作,
        结果.judge条目,
        结果.absent条目,
        结果.告警
    );
    结果
}

/// 惰性过桥关 / 开（提升开）
fn 惰性两臂(src: &str) -> (复核结果, 复核结果) {
    let 臂 = |lazy_cut| Passes {
        lazy_cut,
        lift: true,
        ..Passes::default()
    };
    (复核跑(src, 臂(false)), 复核跑(src, 臂(true)))
}

/// 直线段提升关 / 开（惰性开）；`fuse` 给融合开关
fn 提升两臂(src: &str, fuse: bool) -> (复核结果, 复核结果) {
    let 臂 = |lift| Passes {
        lazy_cut: true,
        lift,
        fuse,
        ..Passes::default()
    };
    (复核跑(src, 臂(false)), 复核跑(src, 臂(true)))
}

const 发臂: &str = r#"{act: fn() { content(do("发邮件", [], 0)) }, ignore: fn() { "不发" }, unsure: fn(u) { consume(u, "drop"); "不发" }}"#;

/// 修复 1 的变体：两臂同一条 J-08、不可逆动作不执行
fn 谱系两臂一致(src: &str) {
    let (关, 开) = 惰性两臂(src);
    assert!(关.r.contains("[J-08]"), "{}", 关.r);
    assert_eq!(开.可比(), 关.可比());
    assert_eq!(开.动作, 0);
}

#[test]
fn v1a_同键三条线() {
    谱系两臂一致(&format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
let r = judge(state(mat("甲")), test("选吗", "k"));
let s1 = cut(r, "t");
let s2 = cut(r, "hi");
let loose = cut(r);
let m = mat(loose);
let out = handle(cut(judge(state(m), test("该发吗", "k2"))), {发臂});
{{out: out, s1: exit_kind(s1), s2: s2}}
"#
    ));
}

#[test]
fn v1b_嵌套函数里判断_外层同键未检视() {
    谱系两臂一致(&format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
fn go(x) {{ handle(cut(judge(state(mat(x)), test("该发吗", "k2"))), {发臂}) }}
let r = judge(state(mat("甲")), test("选吗", "k"));
let strict = cut(r, "t");
let loose = cut(r);
let out = go(loose);
{{out: out, strict: exit_kind(strict)}}
"#
    ));
}

#[test]
fn v1c_外层函数帧里未检视_内层函数判断() {
    谱系两臂一致(&format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
fn inner(l) {{ handle(cut(judge(state(mat(l)), test("该发吗", "k2"))), {发臂}) }}
fn outer(r) {{
    let strict = cut(r, "t");
    let loose = cut(r);
    let o = inner(loose);
    {{o: o, s: exit_kind(strict)}}
}}
outer(judge(state(mat("甲")), test("选吗", "k")))
"#
    ));
}

#[test]
fn v1d_两跳祖先未检视() {
    谱系两臂一致(&format!(
        r#"
budget {{calls: 8, cost: 1, depth: 8}};
let r = judge(state(mat("甲")), test("选吗", "k"));
let strict = cut(r, "t");
let loose = cut(r);
let r2 = judge(state(mat(loose)), test("再选吗", "k"));
let loose2 = cut(r2);
let out = handle(cut(judge(state(mat(loose2)), test("该发吗", "k2"))), {发臂});
{{out: out, strict: exit_kind(strict)}}
"#
    ));
}

#[test]
fn v1e_谱系里解析出错往上传() {
    // 复核修复 7：handle(d) 的谱系要解析同键的 strict，那次刷新把 z 发出去、判断器报错。
    // 修前错误被吞成「不放行」、z 的读数没有答案，之后切 z 时 panic；修后与改前一样报 E-rt-client
    let src = r#"
budget {calls: 8, cost: 1, depth: 8};
let r = judge(state(mat("甲")), test("选吗", "k"));
let strict = cut(r, "t");
let loose = cut(r);
let m = mat(loose);
let d = cut(judge(state(m), test("该发吗", "k2")));
let k0 = exit_kind(d);
let z = cut(judge(state(mat("坏")), test("炸吗", "k")));
let out = handle(d, {act: fn() { "ok" }, ignore: fn() { "no" }, unsure: fn(u) { consume(u, "drop"); "no" }});
{out: out, z: exit_kind(z), s: exit_kind(strict)}
"#;
    let (关, 开) = 惰性两臂(src);
    assert!(
        关.r.contains("[E-rt-client]") && 关.r.contains("判断器挂了"),
        "{}",
        关.r
    );
    assert_eq!(开.可比(), 关.可比());
}

#[test]
fn v4a_被调函数体里的短路右侧() {
    // 复核修复 9：穿进 g 时不收 `&&` 右侧的 judge(s, q2)、judge(s, q4)；q1、q3 同状态提升，一次发出
    let src = r#"
budget {calls: 8, cost: 1, depth: 8};
fn g(s, q1, q2) {
    let e = cut(judge(s, q1));
    exit_kind(e) == "act" && exit_kind(cut(judge(s, q2))) == "act"
}
let s = state(mat("甲"));
let q1 = test("不行1", "k");
let q2 = test("行2", "k");
let q3 = test("不行3", "k");
let q4 = test("行4", "k");
let a = g(s, q1, q2);
let b = g(s, q3, q4);
{a: a, b: b}
"#;
    let (关, 开) = 提升两臂(src, true);
    assert_eq!(开.r, 关.r);
    assert_eq!(关.每次, vec![1, 1]);
    assert_eq!(开.每次, vec![2]);
    assert!(!开.推测未用());
}

/// v5 与 v5w 的程序：融合关时一次 judge 登记两道题
fn 融合关多题(calls: u32) -> String {
    format!(
        r#"
budget {{calls: {calls}, cost: 1, depth: 8}};
fn g(s, qs) {{ let e = cut(judge(s, qs)); {{e: e}} }}
let s = state(mat("甲"));
let qa = [test("行a1", "k"), test("行a2", "k")];
let qb = [test("行b1", "k"), test("行b2", "k")];
let a = g(s, qa);
let b = g(s, qb);
{{a: a, b: b}}
"#
    )
}

#[test]
fn v5_融合关_提升与推测的多题登记() {
    // 复核修复 8：拆出的题继承提升标记与位置，提升组不再抢在 a 的第二题前面花掉预算
    let (关, 开) = 提升两臂(&融合关多题(2), false);
    assert_eq!(关.每次, vec![1, 1]);
    assert_eq!(关.absent条目, 2);
    assert_eq!(开.可比(), 关.可比());
}

#[test]
fn v5w_融合关_同键只问一次() {
    // 复核修复 8 加的一处：融合关时按账本键分组，提升登记与真站点的同一道题只问一次（修前 6 次）
    let (关, 开) = 提升两臂(&融合关多题(8), false);
    assert_eq!(关.每次, vec![1, 1, 1, 1]);
    assert_eq!(开.可比(), 关.可比());
}
