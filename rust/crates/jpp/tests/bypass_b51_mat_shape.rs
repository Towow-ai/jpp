//! B51-R2（步 15d）：动作声明输出形状 `mat_shape`，`do` 返回后运行期核基数与单项尺寸，违反出
//! `Fail(ShapeMismatch)`；未声明不核。依据：B51-R2（`20` 附录 A）；静态形状面在步 24。

use jpp::effects::{CalibStore, FixedPorts, MatShape, ShapeItems};
use jpp::ledger::Ledger;
use jpp::value::Value;
use jpp::{ActionRegistry, TaintOut, run};
use jpp::{lower, syntax::parse};

const 程序: &str = "budget {calls: 1, cost: 0, depth: 8};\nlet r = do(\"取三条\", [], 0);\nif is_fail(r) { text(r) } else { \"ok\" }";

fn 跑(shape: Option<MatShape>) -> String {
    let mut a = ActionRegistry::new();
    a.register("取三条", 0.0, true, TaintOut::Trusted, |_| {
        Ok(Value::list(vec![
            Value::text("一"),
            Value::text("二二二二二"),
            Value::text("三"),
        ]))
    });
    if let Some(s) = shape {
        assert!(a.shape("取三条", s));
    }
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let o = run(
        &program,
        FixedPorts::new().ports(),
        &CalibStore::new(),
        &a,
        &mut Ledger::new(),
    )
    .unwrap_or_else(|e| panic!("跑得完：{}", e.render()));
    o.value_json().as_str().unwrap_or("").to_string()
}

#[test]
fn 未声明不核() {
    assert_eq!(跑(None), "ok");
}

#[test]
fn 超项数出shape_mismatch() {
    let v = 跑(Some(MatShape {
        items: ShapeItems::AtMost(2),
        item_size: None,
        settled: vec![],
    }));
    assert!(
        v.contains("ShapeMismatch") && v.contains("至多 2 项"),
        "{v}"
    );
}

#[test]
fn 超单项尺寸出shape_mismatch() {
    let v = 跑(Some(MatShape {
        items: ShapeItems::Unbounded,
        item_size: Some(3),
        settled: vec!["count".into()],
    }));
    assert!(v.contains("ShapeMismatch") && v.contains("第 1 项"), "{v}");
}

#[test]
fn 形状合规照常() {
    let v = 跑(Some(MatShape {
        items: ShapeItems::AtMost(3),
        item_size: Some(5),
        settled: vec![],
    }));
    assert_eq!(v, "ok");
}
