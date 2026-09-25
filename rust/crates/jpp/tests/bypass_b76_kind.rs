//! 绕行测试（`21` §四·6 步 12e-1，文件名按 `bypass_b55.rs` 的放法；`21` 写作 `tests/bypass/b76_kind.rs`）：
//! 题类由 `question_kind(op, request, 状态槽形, 题式槽声明)` 推出，作者不可写（B46、B76）。
//! 九类各一例（函数层全部九类；检查器层对今天能写出程序的类各一例），声明与结构矛盾报 `E-kind-conflict`。
//! 依据：`附注/2026-09-24-评估①裁定.md` §六「B76」映射表与「不唯一与推不出时怎么处理」。

use jpp::check::diag::{KindSite, question_kinds};
use jpp::{check, lower, syntax::parse};
use jpp_ir::key::Op;
use jpp_ir::question_kind::{
    Accepts, KindSource, OnShape, OverKind, OverShape, QuestionKind as K, Request, SlotDecls,
    SlotShape, kind_conflict, question_kind,
};

const B: &str = "budget {calls: 4, cost: 0};\n";

fn shape(on: OnShape, over: OverShape, question_material: bool) -> SlotShape {
    SlotShape {
        on,
        over,
        question_material,
    }
}

fn kind(op: Op, request: Option<Request>, s: SlotShape, d: SlotDecls) -> (K, KindSource) {
    question_kind(op, request, &s, &d)
}

fn decl(over_kind: Option<OverKind>, accepts: Option<Accepts>) -> SlotDecls {
    SlotDecls { over_kind, accepts }
}

// ---------------------------------------------------------------- 函数层：九类各一例

#[test]
fn 函数层九类各一例() {
    use KindSource::{Declared, Inferred};
    let one = shape(OnShape::One, OverShape::Empty, false);
    let none = SlotDecls::default();
    let cases = [
        (kind(Op::Test, None, one, none), (K::Attr, Inferred)),
        (
            kind(Op::Test, None, one, decl(None, Some(Accepts::Concrete))),
            (K::Mention, Declared),
        ),
        (
            kind(
                Op::Test,
                None,
                shape(OnShape::Pair, OverShape::Empty, false),
                none,
            ),
            (K::Rel, Inferred),
        ),
        (
            kind(
                Op::Test,
                None,
                shape(OnShape::One, OverShape::Empty, true),
                none,
            ),
            (K::Enough, Inferred),
        ),
        (
            kind(Op::Test, Some(Request::All), SlotShape::UNKNOWN, none),
            (K::Subset, Inferred),
        ),
        (
            kind(
                Op::Select,
                None,
                shape(OnShape::Unknown, OverShape::Labels, false),
                none,
            ),
            (K::Class, Inferred),
        ),
        (
            kind(
                Op::Select,
                None,
                shape(OnShape::One, OverShape::Materials, false),
                none,
            ),
            (K::Cmp, Inferred),
        ),
        (
            kind(
                Op::Select,
                None,
                shape(OnShape::Unknown, OverShape::Questions, false),
                none,
            ),
            (K::Decide, Inferred),
        ),
        (
            kind(
                Op::Select,
                None,
                shape(OnShape::Unknown, OverShape::Labels, false),
                decl(Some(OverKind::Actions), None),
            ),
            (K::Decide, Declared),
        ),
        (kind(Op::Measure, None, one, none), (K::Degree, Inferred)),
    ];
    for (i, (got, want)) in cases.iter().enumerate() {
        assert_eq!(got, want, "第 {i} 例");
    }
    let seen: std::collections::HashSet<K> = cases.iter().map(|(g, _)| g.0).collect();
    assert_eq!(seen.len(), K::ALL.len(), "九类都要出现");
}

#[test]
fn 函数层矛盾() {
    // 声明 actions 而 over 是材料（附注举例）
    assert!(
        kind_conflict(
            Op::Select,
            &shape(OnShape::One, OverShape::Materials, false),
            &decl(Some(OverKind::Actions), None)
        )
        .is_some()
    );
    // over_kind 声明在非 select 题式上
    assert!(
        kind_conflict(
            Op::Test,
            &SlotShape::UNKNOWN,
            &decl(Some(OverKind::Labels), None)
        )
        .is_some()
    );
    // 提及类（单对象）而 on 是一对
    assert!(
        kind_conflict(
            Op::Test,
            &shape(OnShape::Pair, OverShape::Empty, false),
            &decl(None, Some(Accepts::Category))
        )
        .is_some()
    );
    // 结构看不见不算矛盾；动作描述与标签同为字面文本，不矛盾
    assert!(
        kind_conflict(
            Op::Select,
            &SlotShape::UNKNOWN,
            &decl(Some(OverKind::Actions), None)
        )
        .is_none()
    );
    assert!(
        kind_conflict(
            Op::Select,
            &shape(OnShape::Unknown, OverShape::Labels, false),
            &decl(Some(OverKind::Actions), None)
        )
        .is_none()
    );
}

// ---------------------------------------------------------------- 检查器层

fn program_kinds(body: &str) -> Vec<KindSite> {
    let src = format!("{B}{body}");
    let p = lower(&parse(&src).expect("解析")).expect("降级");
    question_kinds(&p)
}

fn conflicts(body: &str) -> Vec<String> {
    let src = format!("{B}{body}");
    let p = lower(&parse(&src).expect("解析")).expect("降级");
    check(&p)
        .diagnostics
        .iter()
        .filter(|d| d.rule == "E-kind-conflict")
        .map(|d| src[d.span.start..d.span.end].to_string())
        .collect()
}

/// 判断站点上的精化类（第一个带精化类的站点）
fn refined(body: &str) -> K {
    program_kinds(body)
        .iter()
        .find_map(|s| s.refined)
        .expect("应有判断站点")
        .0
}

#[test]
fn 检查器层_基础类() {
    let ks = program_kinds(
        "let a = test(\"是否下雨？\", \"k\");\nlet b = select(\"哪个？\", \"k\");\nlet c = measure(\"多高？\", [\"低\", \"高\"], \"k\");\n[a, b, c]",
    );
    let base: Vec<K> = ks.iter().map(|s| s.base.0).collect();
    assert_eq!(base, [K::Attr, K::Class, K::Degree]);
    assert!(ks.iter().all(|s| s.refined.is_none()));
}

#[test]
fn 检查器层_属性() {
    assert_eq!(
        refined("judge(state(mat(\"甲\")), test(\"是否下雨？\", \"k\"))"),
        K::Attr
    );
}

#[test]
fn 检查器层_关系_状态一对与配对筛() {
    assert_eq!(
        refined("judge(state([mat(\"甲\"), mat(\"乙\")]), test(\"a 能完成 b 吗？\", \"k\"))"),
        K::Rel
    );
    // pair-team 的写法：基础属性、精化关系，不算矛盾
    let body = "let q = test(\"b 能完成 a 吗？\", \"k\");\nsieve(pair([\"需求\"], [\"成员\"]), q)";
    assert_eq!(refined(body), K::Rel);
    assert!(conflicts(body).is_empty());
}

#[test]
fn 检查器层_充分性() {
    assert_eq!(
        refined(
            "let q = test(\"城市在长江边吗？\", \"k\");\njudge(state(mat(\"甲\"), {ctx: [mat(q)]}), test(\"材料够回答这道题吗？\", \"k2\"))"
        ),
        K::Enough
    );
}

#[test]
fn 检查器层_子集() {
    assert_eq!(
        refined(
            "judge(state(mat(\"甲\")), test(\"哪些段落提到价格？\", \"k\", {request: \"all\"}))"
        ),
        K::Subset
    );
}

#[test]
fn 检查器层_归类() {
    assert_eq!(
        refined(
            "judge(state(mat(\"甲\"), {over: [\"租赁\", \"买卖\"]}), select(\"是哪类合同？\", \"k\"))"
        ),
        K::Class
    );
}

#[test]
fn 检查器层_比较() {
    assert_eq!(
        refined(
            "let xs = [\"甲\", \"乙\"];\njudge(state(mat(\"题\"), {over: [mat(xs[0]), mat(xs[1])]}), select(\"哪份更好？\", \"k\"))"
        ),
        K::Cmp
    );
}

#[test]
fn 检查器层_决定() {
    // over 是题值
    assert_eq!(
        refined(
            "judge(state(mat(\"甲\"), {over: [test(\"下雨吗？\", \"k1\"), test(\"刮风吗？\", \"k2\")]}), select(\"下一步问哪道？\", \"k\"))"
        ),
        K::Decide
    );
    // 动作描述须声明 over_kind = actions，经 fill 沿用
    let body = "let f = form(\"select\", \"对{x}下一步做什么？\", {calib: \"k\", over_kind: \"actions\"});\njudge(state(mat(\"甲\"), {over: [\"退款\", \"补发\"]}), fill(f, {x: \"订单\"}))";
    let ks = program_kinds(body);
    assert!(
        ks.iter()
            .any(|s| s.refined == Some((K::Decide, KindSource::Declared)))
    );
    assert!(conflicts(body).is_empty());
}

#[test]
fn 检查器层_程度() {
    assert_eq!(
        refined("judge(state(mat(\"甲\")), measure(\"多紧急？\", [\"低\", \"高\"], \"k\"))"),
        K::Degree
    );
}

#[test]
fn 检查器层_提及_accepts落地前按属性() {
    // accepts（B23）在步 8 的 Form.slots.accepts 落地前不读：写了也按属性
    let body = "let f = form(\"test\", \"是否提到{c}？\", {calib: \"k\", accepts: \"concrete\"});\njudge(state(mat(\"甲\")), fill(f, {c: \"钢琴\"}))";
    assert_eq!(refined(body), K::Attr);
}

#[test]
fn 矛盾声明报冲突码() {
    // 声明 actions 而 over 是计算出的材料：报在判断站点
    let body = "let xs = [\"甲\", \"乙\"];\nlet f = form(\"select\", \"哪个{x}？\", {calib: \"k\", over_kind: \"actions\"});\njudge(state(mat(\"题\"), {over: [mat(xs[0]), mat(xs[1])]}), fill(f, {x: \"好\"}))";
    let hits = conflicts(body);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].starts_with("judge("));
    // over_kind 声明在是非题式上：报在题式处，不在判断站点重复
    let body = "let f = form(\"test\", \"是否{x}？\", {calib: \"k\", over_kind: \"labels\"});\njudge(state(mat(\"甲\")), fill(f, {x: \"好\"}))";
    let hits = conflicts(body);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].starts_with("form("));
    // 形状看不见（名字引用的 over）不报
    let body = "let os = [\"甲\"];\nlet f = form(\"select\", \"哪个{x}？\", {calib: \"k\", over_kind: \"candidates\"});\njudge(state(mat(\"题\"), {over: os}), fill(f, {x: \"好\"}))";
    assert!(conflicts(body).is_empty());
}
