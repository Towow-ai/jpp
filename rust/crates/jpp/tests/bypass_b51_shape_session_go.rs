//! 步 24h：`mat_shape`（B51-R2）接入 `session/mod.rs::go` 的真实路径（步 24g 只验证过手造
//! `ActionTable`）。`go()` 是私有方法，`W-diag-shape` 是 warning 级、不进 `Outcome.trace.warnings`
//! （`带出静态告警` 只转发 `J-10`），所以从外部看不到「跑通过、报告里有没有这条」——改用一个同一
//! 程序里**另外触发 error 级诊断**（未登记动作名，J-11）的写法，逼 `go()` 里的
//! `check_with_calib_actions` 报错，`Error::Check(report)` 把完整 `Report`（含 warning）带出来，
//! 借此检验 `go()` 真的把 `ActionRegistry` 里登记的 `mat_shape` 转成了 `ActionTable.shapes`
//! （不是只测消费者逻辑，是测 `go()` 这一步转换本身）。预注册见 `地基/过程记录/工程-步24h.md`。

use jpp::effects::{CalibStore, FixedPorts, MatShape, ShapeItems};
use jpp::ledger::Ledger;
use jpp::value::Value;
use jpp::{ActionRegistry, EntryArgs, Error, Program, Session, TaintOut};

fn compile(src: &str) -> Program {
    Session::compile(
        &jpp::syntax::parse(src).expect("解析"),
        &EntryArgs::default().decl(),
    )
    .expect("compile")
}

const 程序: &str = r#"budget {calls: 2, cost: 1};
let ok = do("没登记的动作", [], 0);
judge(state(mat(do("取材料", [], 0))), test("都符合吗", "k", {request: "all"}))
"#;

/// 登记 `取材料`（带 `mat_shape`，settled 不覆盖 Subset 要求的运算）与「没登记的动作」
/// 不登记（触发 J-11）。真实路径下应同时看到 J-11（error）与 W-diag-shape（warning）。
#[test]
fn go真实路径接入mat_shape_报w_diag_shape() {
    let p = compile(程序);
    let calib = CalibStore::new();
    let mut a = ActionRegistry::new();
    a.register("取材料", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::text("m"))
    });
    assert!(a.shape(
        "取材料",
        MatShape {
            items: ShapeItems::Unbounded,
            item_size: None,
            settled: vec![],
        }
    ));
    let mut fp = FixedPorts::new();
    let mut l = Ledger::new();
    let err = Session::new(fp.ports(), &calib, &a)
        .run(&p, &EntryArgs::default(), &mut l)
        .expect_err("未登记动作该在检查期就拦下");
    let Error::Check(report) = err else {
        panic!("该是 Check 错误：{err}");
    };
    assert!(
        report.diagnostics.iter().any(|d| d.rule == "J-11"),
        "{}",
        report.render()
    );
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.rule == "W-diag-shape")
        .unwrap_or_else(|| panic!("go() 真实路径没有把 mat_shape 接进去：{}", report.render()));
    assert!(d.message.contains("取材料"), "{}", d.message);
}

/// 同一动作**不声明** `mat_shape`：只报 J-11，不报 W-diag-shape（对照组，证明上一条的
/// W-diag-shape 确实来自 `mat_shape` 接线，不是巧合）。
#[test]
fn go真实路径未声明形状_不报w_diag_shape() {
    let p = compile(程序);
    let calib = CalibStore::new();
    let mut a = ActionRegistry::new();
    a.register("取材料", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::text("m"))
    });
    let mut fp = FixedPorts::new();
    let mut l = Ledger::new();
    let err = Session::new(fp.ports(), &calib, &a)
        .run(&p, &EntryArgs::default(), &mut l)
        .expect_err("未登记动作该在检查期就拦下");
    let Error::Check(report) = err else {
        panic!("该是 Check 错误：{err}");
    };
    assert!(
        !report.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        report.render()
    );
}
