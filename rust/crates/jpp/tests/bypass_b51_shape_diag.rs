//! 步 24g：B51-R2 `mat_shape` 的诊断层静态消费者（`W-diag-shape`，B13「问的东西在面前材料里吗」
//! 的形状面）。「题类要求的运算」与「`settled` 名 → H4/H5」两张映射是本预注册的提案，不是依据
//! 文本，待 Fable 确认——见 `地基/过程记录/工程-步24g.md` §二·2、§二·3。
//!
//! 两组测试：`shape_check` 单元测试（直接构造 `DiagCx`，覆盖 `Enough`/`Subset` 与 H4/H5 两侧方向，
//! 不需要真的写出 `Enough` 形状的程序——`Enough` 的结构条件是状态 `ctx`/`ref` 含由 `Question` 值
//! 渲染的材料，字面程序不容易构造，直接测函数更直接）；`check_with_calib_actions` 端到端测试
//! （`Subset` 形状容易用字面写出：`test(..., {request: "all"})`），覆盖状态材料到 `do(...)` 的
//! 来源追溯与动作表查找。

use jpp::check::diag::b13::{DiagCx, shape_check};
use jpp::check::{ActionFacts, ActionTable, Report, check_with_calib_actions};
use jpp::effects::{CalibStore, MatShape, Profile, ShapeItems, Tri};
use jpp::{lower, syntax::parse};

// ---------------------------------------------------------------- shape_check 单元测试

fn 形状(settled: &[&str]) -> MatShape {
    MatShape {
        items: ShapeItems::Unbounded,
        item_size: None,
        settled: settled.iter().map(|s| s.to_string()).collect(),
    }
}

fn cx(settled: &[&str], one_hop: Tri, arithmetic_capable: Tri) -> DiagCx {
    DiagCx {
        mat_shape: Some(形状(settled)),
        shape_action: Some("取材料".into()),
        one_hop,
        arithmetic_capable,
    }
}

use jpp_ir::ir::Span;
use jpp_ir::question_kind::QuestionKind;

fn 空span() -> Span {
    Span::default()
}

/// `Enough`：`settled` 没有 `count`，H5 未测（保守默认）→ 报。
#[test]
fn enough_settled不含count_h5未测_报() {
    let cx = cx(&[], Tri::未测, Tri::未测);
    let d = shape_check(QuestionKind::Enough, 空span(), &cx);
    assert!(d.is_some());
    assert_eq!(d.unwrap().rule, "W-diag-shape");
}

/// `Enough`：`settled` 含 `count` → 已覆盖，不报。
#[test]
fn enough_settled含count_不报() {
    let cx = cx(&["count"], Tri::未测, Tri::未测);
    assert!(shape_check(QuestionKind::Enough, 空span(), &cx).is_none());
}

/// `Enough`：`settled` 没有 `count`，但 `arithmetic_capable == 真`（H5 不成立，模型明确会算/数）
/// → 抑制，不报。
#[test]
fn enough_arithmetic_capable为真_抑制() {
    let cx = cx(&[], Tri::未测, Tri::真);
    assert!(shape_check(QuestionKind::Enough, 空span(), &cx).is_none());
}

/// `Subset`：`settled` 只含 `count`（要求 `count`/`dedup` 任一），已覆盖，不报。
#[test]
fn subset_settled含count_不报() {
    let cx = cx(&["count"], Tri::未测, Tri::未测);
    assert!(shape_check(QuestionKind::Subset, 空span(), &cx).is_none());
}

/// `Subset`：`settled` 全不含，H4 与 H5 都未测（都照常报的默认方向）→ 报。
#[test]
fn subset_settled不覆盖_两个h都未测_报() {
    let cx = cx(&[], Tri::未测, Tri::未测);
    assert!(shape_check(QuestionKind::Subset, 空span(), &cx).is_some());
}

/// `Subset`：`settled` 全不含，但 `one_hop == 假`（H4 不成立）与 `arithmetic_capable == 真`
/// （H5 不成立）都满足 → 两个相关的 H 字段都不利方向被抑制，不报。
#[test]
fn subset_两个h都对模型有利_不报() {
    let cx = cx(&[], Tri::假, Tri::真);
    assert!(shape_check(QuestionKind::Subset, 空span(), &cx).is_none());
}

/// `Subset`：只有 `one_hop == 假`（H4 抑制），`arithmetic_capable` 仍未测（H5 不抑制）→
/// 仍然报（要求 `count`/`dedup` 任一，`dedup` 走 H4 被抑制，但 `count` 走 H5 没被抑制）。
#[test]
fn subset_只压住一半_仍报() {
    let cx = cx(&[], Tri::假, Tri::未测);
    assert!(shape_check(QuestionKind::Subset, 空span(), &cx).is_some());
}

/// 没有 `mat_shape`（判不出来源，或来源动作没声明）→ 不报，不管画像如何。
#[test]
fn 没有mat_shape_不报() {
    let cx = DiagCx {
        mat_shape: None,
        shape_action: None,
        one_hop: Tri::未测,
        arithmetic_capable: Tri::未测,
    };
    assert!(shape_check(QuestionKind::Subset, 空span(), &cx).is_none());
}

/// 其余七类（如 `Attr`）判不出运算需求，不判，不报——即使 `mat_shape`/H 字段都不利。
#[test]
fn 属性类不判_不报() {
    let cx = cx(&[], Tri::未测, Tri::未测);
    assert!(shape_check(QuestionKind::Attr, 空span(), &cx).is_none());
}

// ---------------------------------------------------------------- 端到端：Subset 形状 + do(...) 来源追溯

fn 动作表(settled: &[&str]) -> ActionTable {
    let mut t = ActionTable::default();
    t.actions.insert(
        "取材料".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: false,
        },
    );
    t.shapes.insert("取材料".into(), 形状(settled));
    t
}

fn 画像(one_hop: Tri, arithmetic_capable: Tri) -> Profile {
    let mut p = Profile::untested();
    if one_hop != Tri::未测 {
        p = p.with_one_hop(one_hop == Tri::真, "test");
    }
    if arithmetic_capable != Tri::未测 {
        p = p.with_arithmetic_capable(arithmetic_capable == Tri::真, "test");
    }
    p
}

fn 检查(src: &str, settled: &[&str], one_hop: Tri, arithmetic_capable: Tri) -> Report {
    let p = lower(&parse(src).expect("解析")).expect("降级");
    let mut calib = CalibStore::new();
    calib.profile = 画像(one_hop, arithmetic_capable);
    calib.profile.hash = Some("test".into());
    check_with_calib_actions(&p, &calib, &动作表(settled))
}

const 子集程序: &str = r#"budget {calls: 2, cost: 1};
judge(state(mat(do("取材料", [], 0))), test("都符合吗", "k", {request: "all"}))
"#;

/// 直接内联 `state(mat(do(...)))`：动作没声明 `settled` 覆盖「子集」要求的运算，H4/H5 都未测
/// （照常报的方向）→ 报 `W-diag-shape`。
#[test]
fn 内联do来源_子集_未覆盖_报() {
    let r = 检查(子集程序, &[], Tri::未测, Tri::未测);
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.rule == "W-diag-shape")
        .unwrap_or_else(|| panic!("{}", r.render()));
    assert!(d.message.contains("取材料"), "{}", d.message);
}

/// 同上，但动作声明 `settled` 含 `count`：已覆盖，不报。
#[test]
fn 内联do来源_子集_已覆盖_不报() {
    let r = 检查(子集程序, &["count"], Tri::未测, Tri::未测);
    assert!(
        !r.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        r.render()
    );
}

const 经绑定的子集程序: &str = r#"budget {calls: 2, cost: 1};
let m = do("取材料", [], 0);
judge(state(mat(m)), test("都符合吗", "k", {request: "all"}))
"#;

/// 状态材料经一次 `let` 绑定再引用：同样能追溯到来源，未覆盖时报。
#[test]
fn 经绑定的来源_子集_未覆盖_报() {
    let r = 检查(经绑定的子集程序, &[], Tri::未测, Tri::未测);
    assert!(
        r.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        r.render()
    );
}

/// 没有动作表（走 `jpp::check::check`，不传动作表）：判不出 `mat_shape`，不报。
#[test]
fn 没有动作表_不报() {
    let p = lower(&parse(子集程序).expect("解析")).expect("降级");
    let r = jpp::check::check(&p);
    assert!(
        !r.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        r.render()
    );
}

/// 动作名不在动作表里（没声明）：判不出 `mat_shape`，不报。
#[test]
fn 动作未登记形状_不报() {
    let mut calib = CalibStore::new();
    calib.profile.hash = Some("test".into());
    let mut t = ActionTable::default();
    t.actions.insert(
        "别的动作".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: false,
        },
    );
    let p = lower(&parse(子集程序).expect("解析")).expect("降级");
    let r = check_with_calib_actions(&p, &calib, &t);
    assert!(
        !r.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        r.render()
    );
}

/// 材料来源判不出（容器合并）：字面写 `state(mat("字面材料"))`（不经 `do`）——非本条管的形状，
/// 不报（本来就该走 J-08/其余规则，不是 `W-diag-shape` 的事）。
#[test]
fn 非do来源_不报() {
    let src = r#"budget {calls: 2, cost: 1};
judge(state(mat("字面")), test("都符合吗", "k", {request: "all"}))
"#;
    let r = 检查(src, &[], Tri::未测, Tri::未测);
    assert!(
        !r.diagnostics.iter().any(|d| d.rule == "W-diag-shape"),
        "{}",
        r.render()
    );
}
