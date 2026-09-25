//! 步 24e：J-11 静态面——`do` 的动作名是字面量、检查器拿到了宿主动作表、但这个名字没登记时，
//! 检查期就报，不必等运行期真的触发它（同 `do_()` 的运行期报文口径，`effects_exec.rs`）。
//! 没有动作表时不报（同 J-08 静态子面口径，避免库调用方被误报）；动作名不是字面量时静态判不了，
//! 交运行期。预注册见 `地基/过程记录/工程-步24e.md`。

use jpp::check::{ActionFacts, ActionTable, Report, check, check_with_calib_actions};
use jpp::effects::CalibStore;
use jpp::{lower, syntax::parse};

fn compile(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("降级")
}

fn 动作表() -> ActionTable {
    let mut t = ActionTable::default();
    t.actions.insert(
        "record_check".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: false,
        },
    );
    t
}

fn 带表检查(src: &str) -> Report {
    let p = compile(src);
    let calib = CalibStore::new();
    check_with_calib_actions(&p, &calib, &动作表())
}

fn 报了j11(r: &Report) -> bool {
    r.diagnostics.iter().any(|d| d.rule == "J-11")
}

/// 字面动作名未登记：带动作表的检查报 J-11。
#[test]
fn 未登记动作名_带动作表报j11() {
    let src = "budget {calls: 2, cost: 1};\ndo(\"没登记的动作\", [], 0)\n";
    let r = 带表检查(src);
    let d = r.find("J-11").unwrap_or_else(|| panic!("{}", r.render()));
    assert!(d.message.contains("没登记的动作"), "{}", d.message);
}

/// 字面动作名已登记：不报。
#[test]
fn 已登记动作名_不报() {
    let src = "budget {calls: 2, cost: 1};\ndo(\"record_check\", [{a: 1}], 0)\n";
    let r = 带表检查(src);
    assert!(!报了j11(&r), "{}", r.render());
}

/// 没有动作表（走 `check`，不传动作表）：不报——检查期不知道注册了什么，避免库调用方被误报。
#[test]
fn 没有动作表_不报() {
    let src = "budget {calls: 2, cost: 1};\ndo(\"没登记的动作\", [], 0)\n";
    let p = compile(src);
    let r = check(&p);
    assert!(!报了j11(&r), "{}", r.render());
}
