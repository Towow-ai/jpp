//! 绕过测试 B72-4 谱系放行（步 17b；`21` 写的路径是 `tests/bypass/b72_lineage.rs`，按仓库惯例放在这里）。
//!
//! 出口作守卫证据时，被判断材料经来源可达的每个祖先出口都须已决且线放行（`releases()`），否则
//! `from_releasing_judgement` 为假，J-08 报文写「该材料由 {等级} 线的出口选出」。「可达」= 状态的
//! `parents`（槽材料的 sources ∪ 题的 sources）经账本 `parents` 的传递闭包（B84 补）；祖先按本趟出口表取。
//! 本趟出口表里查不到的祖先键算「无法证明」、不放行，报 `W-lineage-unknown`；未决祖先不提供谱系放行
//! （主会话 2026-09-25 两条保守读法）。依据：B72（地基/附注/2026-09-24-评估①裁定.md）、B84、B92、B121。

mod common;

use jpp::effects::{CalibStore, CertGrade, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::{Entry, Ledger};
use jpp::run;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 题面含「未决」的题回 0.5（切出 Unsure），其余回 0.95（Act）
fn 端口<'a>() -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        Ok(JudgeResult {
            answers: qs
                .iter()
                .map(|q| Answer::Noul(if q.text.contains("未决") { 0.5 } else { 0.95 }))
                .collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
        })
    }))
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

/// `k`、`k2`：正式线；`t`：试用线；`f`：夹具线（`put` 写的记录）
fn 库() -> CalibStore {
    let mut c = CalibStore::new();
    common::certified(&mut c, "k", 0.8, 0.2, 50);
    common::certified(&mut c, "k2", 0.8, 0.2, 50);
    common::certified(&mut c, "t", 0.8, 0.2, 50);
    for cert in c.records.get_mut("t").unwrap().certs.values_mut() {
        cert.grade = CertGrade::Trial;
    }
    c.put("f", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap(); // 步 15d-2：夹具线显式给 δ
    c
}

fn 跑在(src: &str, l: &mut Ledger) -> Result<Json, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    run(&program, 端口(), &库(), &动作表(), l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

fn 跑(src: &str) -> Result<Json, String> {
    跑在(src, &mut Ledger::new())
}

const 发: &str = r#"content(do("发邮件", [], 0))"#;

/// 第一跳用键 `第一` 从两份字面材料里筛，第二跳在被选出的材料上用正式线 `k2` 判断，act 臂里发。
fn 两跳(第一: &str, 第二跳材料: &str) -> String {
    format!(
        "budget {{calls: 8, cost: 1, depth: 8}};\nlet r = sieve([mat(\"甲\"), mat(\"乙\")], test(\"选哪个\", \"{第一}\"));\nlet m = {第二跳材料};\nhandle(cut(judge(state(m), test(\"该发吗\", \"k2\"))), {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})\n"
    )
}

#[test]
fn a_试用线选出的材料上正式线判断不放行() {
    let e = 跑(&两跳("t", "r.value[0].item")).expect_err("谱系里有试用线出口，不可逆 do 应被拒");
    assert!(e.contains("J-08"), "{e}");
    assert!(e.contains("该材料由 Trial 线的出口选出"), "{e}");
}

#[test]
fn b_两跳都是正式线放行() {
    assert_eq!(
        跑(&两跳("k", "r.value[0].item")).unwrap(),
        Json::from("已发")
    );
}

#[test]
fn c_夹具线选出的材料不放行() {
    let e = 跑(&两跳("f", "r.value[0].item")).expect_err("谱系里有夹具线出口");
    assert!(e.contains("J-08") && e.contains("Fixture"), "{e}");
}

#[test]
fn d_谱系只收紧判断证据_经ask照常放行() {
    // 同样由试用线选出的材料，第二跳改问人：`via_ask` 不受谱系影响
    let src = format!(
        "budget {{calls: 8, cost: 1, depth: 8, escalate: 1}};\nlet r = sieve([mat(\"甲\"), mat(\"乙\")], test(\"选哪个\", \"t\"));\nhandle(ask(state(r.value[0].item), test(\"该发吗\", \"human\")), {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})\n"
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let ports = 端口().with(FnPort::ask("h", |_s: &State, _q: &Question| {
        Ok(Some(Answer::Noul(1.0)))
    }));
    let v = run(&program, ports, &库(), &动作表(), &mut Ledger::new())
        .map(|o| o.value_json())
        .map_err(|e| e.render());
    assert_eq!(v.unwrap(), Json::from("已发"));
}

#[test]
fn e_值依赖边同样传递() {
    // content() 读出再拼接、mat() 造新材料：来源经值依赖边传到第二跳（B84、B92：谱系计全部边）
    let e = 跑(&两跳("t", "mat(content(r.value[0].item) + \"！\")"))
        .expect_err("值依赖边上的试用线祖先同样断谱系");
    assert!(e.contains("J-08") && e.contains("Trial"), "{e}");
    assert_eq!(
        跑(&两跳("k", "mat(content(r.value[0].item) + \"！\")")).unwrap(),
        Json::from("已发")
    );
}

#[test]
fn f_对照_第一跳试用线出口自身的证据本来就为假() {
    let src = format!(
        "budget {{calls: 8, cost: 1, depth: 8}};\nlet ok = handle(cut(judge(state(mat(\"甲\")), test(\"选哪个\", \"t\"))), {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}\n"
    );
    let e = 跑(&src).expect_err("试用线出口本身不放行（B72）");
    assert!(e.contains("J-08"), "{e}");
    assert!(!e.contains("谱系放行"), "这条不是谱系断：{e}");
}

#[test]
fn g_本趟出口表里没有的祖先键_无法证明即不放行() {
    // 单跳正式线程序先跑一遍（放行），再把账本里这条读数的 parents 改成一个本趟不会切出的键后重跑
    let src = format!(
        "budget {{calls: 4, cost: 1, depth: 8}};\nhandle(cut(judge(state(mat(\"甲\")), test(\"该发吗\", \"k\"))), {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})\n"
    );
    let mut l = Ledger::new();
    assert_eq!(跑在(&src, &mut l).unwrap(), Json::from("已发"));
    let mut 改了 = 0;
    for e in l.entries.iter_mut() {
        if let Entry::Judge { parents, .. } = e {
            parents.push("不存在的读数键".into());
            改了 += 1;
        }
    }
    assert_eq!(改了, 1);
    // 账本里已记的 do 会按记录重放、不再过放行核；去掉它，让第二趟真的走到 J-08
    l.entries.retain(|e| matches!(e, Entry::Judge { .. }));
    l.rebuild_index();
    let e = 跑在(&src, &mut l).expect_err("祖先键在本趟出口表里查不到，按不放行处理");
    assert!(e.contains("J-08"), "{e}");
    assert!(
        e.contains("来源读数 不存在的读数键 本趟没有切出出口"),
        "{e}"
    );
}

#[test]
fn h_未决祖先不提供谱系放行() {
    // 第一跳正式线切出 Unsure，unsure 臂里 literalize 重问（题的来源 = 未决出口键），第二跳 act 臂里发
    let src = format!(
        "budget {{calls: 8, cost: 1, depth: 8}};\nlet m = mat(\"甲\");\nlet s = state(m);\nhandle(cut(judge(s, test(\"未决吗\", \"k\"))), {{act: fn() {{ \"不发\" }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ handle(literalize(u, s, test(\"字面地说，该发吗\", \"k2\")), {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(v) {{ consume(v, \"drop\"); \"不发\" }}}}) }}}})\n"
    );
    let e = 跑(&src).expect_err("祖先是未决出口，不提供谱系放行");
    assert!(e.contains("J-08") && e.contains("线的未决出口选出"), "{e}");
}
