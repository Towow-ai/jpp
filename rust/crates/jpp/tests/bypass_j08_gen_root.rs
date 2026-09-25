//! 步 24c：J-08 静态面补「暂不认的根」之一——`gen` 的输出（24-0 遗留，B108）。`gen` 的
//! `taint_rule == Inherit`，输出 taint = ∨ `ctx` 槽；`ctx` 里有确定不可信的材料，`gen` 的
//! 输出就确定不可信，可以作为 J-08 静态子面认的根，不必等运行期才拦。判据不按效应名字面量
//! 分支（`taint_rule == Inherit && !produces_reading`，现行注册表里只有 `gen` 满足）。
//! 依据：24-0 完成记录「暂不认的根」一节；预注册见 `地基/过程记录/工程-步24c.md`。

use jpp::check::{ActionFacts, ActionTable, check_with_calib_actions};
use jpp::effects::CalibStore;
use jpp::value::Taint;
use jpp::{EntryArgs, EntryValue, Program, Session};
use serde_json::json;

fn 入口(taint: Taint) -> EntryArgs {
    EntryArgs {
        values: vec![EntryValue::new("input", json!("hello")).with_taint(taint)],
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
        },
    );
    t
}

const 程序: &str = "budget {calls: 1, cost: 1};\nlet materials = gen(\"总结一下\", [input], 1, 0);\nlet ok = handle(cut(judge(state(materials[0]), test(\"行吗\", \"k\"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, \"drop\"); false }});\nif ok { content(do(\"发邮件\", [], 0)) } else { \"没发\" }\n";

/// `gen` 的 `ctx` 里含确定不可信的入口值（未声明可信）：输出判定为确定不可信，
/// 唯一守卫来自它、动作不可逆 → 静态面报 `J-08`。
#[test]
fn gen的ctx含未声明可信入口_报j08() {
    let p = compile(程序, &入口(Taint::Untrusted));
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    assert!(
        r.diagnostics.iter().any(|d| d.rule == "J-08"),
        "{:?}",
        r.diagnostics
    );
}

/// `gen` 的 `ctx` 里的入口值声明可信：输出不判定为确定不可信 → 不报（零假拒绝面）。
#[test]
fn gen的ctx声明可信_不报() {
    let p = compile(程序, &入口(Taint::Trusted));
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    assert!(
        !r.diagnostics
            .iter()
            .any(|d| d.rule == "J-08" || d.rule == "W-guard-untrusted"),
        "{:?}",
        r.diagnostics
    );
}

/// `gen` 的 `ctx` 是空列表：没有元素可确定不可信 → 不报。
#[test]
fn gen的ctx为空_不报() {
    let src = "budget {calls: 1, cost: 1};\nlet materials = gen(\"总结一下\", [], 1, 0);\nlet ok = handle(cut(judge(state(materials[0]), test(\"行吗\", \"k\"))), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, \"drop\"); false }});\nif ok { content(do(\"发邮件\", [], 0)) } else { \"没发\" }\n";
    let p = compile(src, &EntryArgs::default());
    let r = check_with_calib_actions(&p, &CalibStore::new(), &动作表());
    assert!(
        !r.diagnostics
            .iter()
            .any(|d| d.rule == "J-08" || d.rule == "W-guard-untrusted"),
        "{:?}",
        r.diagnostics
    );
}
