//! 步 24-0：J-08 静态子面「守卫全不可信」（B108）。两面各有用例：
//! - 报的一面：唯一守卫是未声明可信的入口上的判断，且动作不可逆。`Session` 执行前那次检查（带动作表）
//!   报 `J-08`，一次判断都不花；不带动作表的检查（`Session::explain`，也就是 CLI `jpp check`）报
//!   `W-guard-untrusted`。
//! - 零假拒绝的一面：宿主声明可信、另合取一个字面材料上的判断、守卫经函数包装、守卫链上有 `ask`、
//!   动作可逆，这些情形都不报。
//!
//! 预注册见 `地基/过程记录/工程-步24-0.md` §五。`21` 写的路径是 `tests/bypass/j08_static_untrusted.rs`，
//! 按仓库惯例放在这里。依据：B108（地基/附注/2026-09-25-批量裁定.md §二）。
use jpp::check::{ActionFacts, ActionTable, Report, check_with_calib_actions};
use jpp::effects::{CalibStore, FixedPorts};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::Taint;
use jpp::{EntryArgs, EntryValue, Error, Program, Session};
use serde_json::json;

fn 入口(taint: Taint) -> EntryArgs {
    EntryArgs {
        values: vec![EntryValue::new("input", json!({"x": "hello"})).with_taint(taint)],
        ..Default::default()
    }
}

fn compile(src: &str, entry: &EntryArgs) -> Program {
    let ast = jpp::syntax::parse(src).expect("语法");
    Session::compile(&ast, &entry.decl()).expect("compile")
}

fn 动作表() -> ActionTable {
    let mut t = ActionTable::default();
    t.actions.insert(
        "发邮件".into(),
        ActionFacts {
            reversible: false,
            output_untrusted: false,
            no_sandbox: false,
        },
    );
    t.actions.insert(
        "存草稿".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: false,
            no_sandbox: false,
        },
    );
    t
}

/// (带动作表的检查, 不带动作表的检查)
fn 两种检查(src: &str, entry: &EntryArgs) -> (Report, Report) {
    let p = compile(src, entry);
    let calib = CalibStore::new();
    (
        check_with_calib_actions(&p, &calib, &动作表()),
        Session::explain(&p, None),
    )
}

fn 报了(r: &Report, code: &str) -> bool {
    r.diagnostics.iter().any(|d| d.rule == code)
}

fn 都不报(src: &str, entry: &EntryArgs) {
    let (有表, 无表) = 两种检查(src, entry);
    assert!(!报了(&有表, "J-08"), "{}", 有表.render());
    assert!(!报了(&无表, "W-guard-untrusted"), "{}", 无表.render());
}

const 判断: &str = "handle(cut(judge(state(mat(input.x)), test(\"该发吗\", \"k\"))), {\n    act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, \"drop\"); false }})";

fn 唯一守卫(action: &str) -> String {
    format!(
        "budget {{calls: 2, cost: 1}};\nlet ok = {判断};\nif ok {{ content(do(\"{action}\", [], 0)) }} else {{ \"没发\" }}\n"
    )
}

/// 1. 唯一守卫是未声明可信的入口上的判断，动作不可逆：`Session` 在执行前报 `J-08`，账本里一条记录都没有；
///    不带动作表的检查报 `W-guard-untrusted`；两者的报文都说出来源是宿主入口经计算值造的材料。
#[test]
fn 一_唯一守卫不可信_执行前就停() {
    let entry = 入口(Taint::Untrusted);
    let src = 唯一守卫("发邮件");
    let (有表, 无表) = 两种检查(&src, &entry);
    let d = 有表
        .find("J-08")
        .unwrap_or_else(|| panic!("{}", 有表.render()));
    assert!(d.message.contains("静态子面"), "{}", d.message);
    assert!(
        d.message.contains("计算值") && d.message.contains("宿主入口 input"),
        "{}",
        d.message
    );
    assert!(报了(&无表, "W-guard-untrusted"), "{}", 无表.render());
    assert!(无表.is_ok(), "不带动作表只是 warn：{}", 无表.render());

    let p = compile(&src, &entry);
    let mut fp = FixedPorts::new();
    let calib = CalibStore::new();
    let mut actions = ActionRegistry::new();
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        panic!("不可逆动作不该执行")
    });
    let mut ledger = Ledger::new();
    let out = Session::new(fp.ports(), &calib, &actions).run(&p, &entry, &mut ledger);
    match out {
        Err(Error::Check(r)) => assert!(r.find("J-08").is_some(), "{}", r.render()),
        Err(Error::Runtime(e)) => panic!("该在检查期停下，却到了运行期：{}", e.render()),
        Ok(_) => panic!("该被拦"),
    }
    assert_eq!(fp.calls(), 0, "一次判断都不该花");
    assert!(ledger.entries.is_empty(), "账本该是空的");
}

/// 2. 同一程序，宿主声明入口可信（`EntryValue::with_taint(Trusted)`；CLI 开关随 14b-1）：不报。
#[test]
fn 二_宿主声明可信_不报() {
    都不报(&唯一守卫("发邮件"), &入口(Taint::Trusted));
}

/// 3. 同一程序在守卫里再合取一个字面材料上的判断：那个合取项可能可信，不报（运行期看线的等级再定）。
#[test]
fn 三_另合取一个字面材料上的判断_不报() {
    let src = format!(
        "budget {{calls: 4, cost: 1}};\nlet ok = {判断};\nlet 净 = handle(cut(judge(state(mat(\"源码里的字面量\")), test(\"该发吗\", \"k\"))), {{\n    act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok && 净 {{ content(do(\"发邮件\", [], 0)) }} else {{ \"没发\" }}\n"
    );
    都不报(&src, &入口(Taint::Untrusted));
}

/// 4. 守卫经函数包装：追不到来源，按可信计，不报（零假拒绝面；运行期面兜底）。
#[test]
fn 四_守卫经函数包装_不报() {
    let src = format!(
        "budget {{calls: 2, cost: 1}};\nfn 可以吗() !{{judge}} {{ {判断} }}\nif 可以吗() {{ content(do(\"发邮件\", [], 0)) }} else {{ \"没发\" }}\n"
    );
    都不报(&src, &入口(Taint::Untrusted));
}

/// 5. `do` 写在 `handle` 的 `act` 臂里，出口来自不可信入口：报 `J-08`；换成可逆动作则不报
///    （不带动作表时两者都只报 warn）。
#[test]
fn 五_handle臂里的do() {
    let prog = |action: &str| {
        format!(
            "budget {{calls: 2, cost: 1}};\nhandle(cut(judge(state(mat(input.x)), test(\"该发吗\", \"k\"))), {{\n    act: fn() {{ content(do(\"{action}\", [], 0)) }}, ignore: fn() {{ \"不发\" }},\n    unsure: fn(u) {{ consume(u, \"drop\"); \"未定\" }}}})\n"
        )
    };
    let entry = 入口(Taint::Untrusted);
    let (有表, 无表) = 两种检查(&prog("发邮件"), &entry);
    assert!(报了(&有表, "J-08"), "{}", 有表.render());
    assert!(报了(&无表, "W-guard-untrusted"), "{}", 无表.render());
    let (有表, _) = 两种检查(&prog("存草稿"), &entry);
    assert!(!报了(&有表, "J-08"), "可逆动作不受 J-08：{}", 有表.render());
}

/// 6. 守卫链上有 `ask`（外层由人拍板）：运行期放行，静态面不报。
#[test]
fn 六_守卫链上有ask_不报() {
    let src = format!(
        "budget {{calls: 2, cost: 1, escalate: 1}};\nlet ok = {判断};\nlet 人 = handle(ask(state(mat(\"草稿\")), test(\"发吗\", \"human\")), {{\n    act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif 人 {{ if ok {{ content(do(\"发邮件\", [], 0)) }} else {{ \"没发\" }} }} else {{ \"没发\" }}\n"
    );
    都不报(&src, &入口(Taint::Untrusted));
}
