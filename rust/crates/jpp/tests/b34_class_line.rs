//! B34 类键借线（步 20b）：查找链 题键 → 题式键 → 类键 → 冷（B44）。
//! 类线出口报 `W-class-line`、线源 `类级·…`；题式有线时题式先命中；题式停岗时不借类线；
//! B68 范围按类记录核；B75：类线出口不放行不可逆 `do`；只凭账本重放 0 调用。
//! 依据：B34、B44、B68（`地基/12-IR与类契约-v0.1.md` §2.2、§2.3）；`21` 步 20b；过程记录 `工程-步20b.md`。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, syntax::parse};
use jpp::{run, run_replay};
use jpp_value::stat::ScopeRanges;
use serde_json::{Value as Json, json};

/// 判断恒给 0.95，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口，计数搬到调用处）
fn 桩端口<'a>(calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("m", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.95)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

const 短句: &str = "他把旧自行车卖了，打算换一辆折叠车上下班。";
const 认证集: [&str; 4] = [
    "妈妈削了一个苹果，分给我们两半，酸甜正好。",
    "早高峰的地铁挤得人喘不过气，他差点坐过站。",
    "周末一家三口去公园放风筝，孩子笑得很开心。",
    "医生建议他少吃油炸食品，多吃新鲜蔬菜水果。",
];
const 范围外: &str = "ERROR: build failed at src/main.rs:42\nexpected `;`, found `}`\nerror: aborting due to 1 previous error";

/// 只路由、不做不可逆动作：act → "act"
fn 路由(材料: &str, 题: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
let q = {题};
handle(cut(judge(state(mat({材料:?})), q)), {{
    act: fn() {{ "act" }},
    ignore: fn() {{ "ignore" }},
    unsure: fn(u) {{ let c = unsure_cause(u); consume(u, "drop"); c }}}})
"#
    )
}

/// act 臂做不可逆 do
fn 放行(材料: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
handle(cut(judge(state(mat({材料:?})), test("该发吗", "k"))), {{
    act: fn() {{ content(do("发邮件", [], 0)) }},
    ignore: fn() {{ "没发" }},
    unsure: fn(u) {{ consume(u, "drop"); "没发" }}}})
"#
    )
}

fn 类库(指纹: bool) -> CalibStore {
    let mut calib = CalibStore::new();
    let ck = CalibStore::class_key("k");
    common::certified(&mut calib, &ck, 0.8, 0.2, 50);
    if 指纹 {
        calib.records.get_mut(&ck).unwrap().scope = Some(jpp::effects::CalibScope {
            batches: vec!["t".into()],
            sources: Default::default(),
            note: String::new(),
            fingerprint: ScopeRanges::from_texts(认证集, (0.0, 1.0), None),
            n_text: None,
            extensions: vec![],
        });
    }
    calib
}

fn 跑(src: &str, calib: &CalibStore) -> Result<(Json, Vec<String>, Ledger), String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let mut l = Ledger::new();
    let calls = RefCell::new(0);
    run(&program, 桩端口(&calls), calib, &a, &mut l)
        .map(|o| (o.value_json(), o.trace.warnings.clone(), l.clone()))
        .map_err(|e| e.render())
}

fn 有(w: &[String], p: &str) -> usize {
    w.iter().filter(|x| x.starts_with(p)).count()
}

/// 题键无记录、类键有认证记录 → 借类线出 act，报 W-class-line；账本记下类记录。
#[test]
fn 题键冷时借类线() {
    let (v, w, l) = 跑(&路由(短句, r#"test("该发吗", "k")"#), &类库(false)).unwrap();
    assert_eq!(v, json!("act"), "{w:?}");
    assert_eq!(有(&w, "W-class-line"), 1, "{w:?}");
    assert!(
        l.calib_used.contains_key(&CalibStore::class_key("k")),
        "账本要记下所借的类记录"
    );
}

/// 没有类记录 → 冷（现状不变）。
#[test]
fn 没有类记录仍然冷() {
    let (v, w, _) = 跑(&路由(短句, r#"test("该发吗", "k")"#), &CalibStore::new()).unwrap();
    assert_eq!(v, json!("cold"));
    assert_eq!(有(&w, "W-class-line"), 0);
}

/// 题键自己有线时用自己的，不借类线。
#[test]
fn 题键有线时不借类线() {
    let mut calib = 类库(false);
    common::certified(&mut calib, "k", 0.99, 0.9, 50);
    let (v, w, _) = 跑(&路由(短句, r#"test("该发吗", "k")"#), &calib).unwrap();
    assert_eq!(
        v,
        json!("band"),
        "题键自己的线（0.99/0.9）判 0.95 在带内，不是类线的 act：{w:?}"
    );
    assert_eq!(有(&w, "W-class-line"), 0, "{w:?}");
}

const 题式: &str = r#"fill(form("test", "这段话是否与「{c}」有关？", {calib: "k"}), {c: "交通"})"#;

fn 题式键() -> String {
    let f = jpp_value::value::Form::new(
        jpp_value::value::Op::Test,
        "这段话是否与「{c}」有关？",
        "",
        vec![],
        vec![],
        None,
        None,
    )
    .unwrap();
    CalibStore::form_key(&f.hash)
}

/// 题式有认证线 → 题式先命中，类层不用。
#[test]
fn 题式有线时题式先命中() {
    let mut calib = 类库(false);
    common::certified(&mut calib, &题式键(), 0.8, 0.2, 50);
    let (v, w, _) = 跑(&路由(短句, 题式), &calib).unwrap();
    assert_eq!(v, json!("act"));
    assert_eq!(有(&w, "W-form-line"), 1, "{w:?}");
    assert_eq!(有(&w, "W-class-line"), 0, "{w:?}");
}

/// 题式无记录 → 由题式填出的题也能借类线。
#[test]
fn 题式无线时借类线() {
    let (v, w, _) = 跑(&路由(短句, 题式), &类库(false)).unwrap();
    assert_eq!(v, json!("act"));
    assert_eq!(有(&w, "W-class-line"), 1, "{w:?}");
}

/// 题式记录停岗 → 不绕过停岗去借类线，出口冷。
#[test]
fn 题式停岗时不借类线() {
    let mut calib = 类库(false);
    common::certified(&mut calib, &题式键(), 0.8, 0.2, 50);
    calib.records.get_mut(&题式键()).unwrap().status = "停岗".into();
    let (v, w, _) = 跑(&路由(短句, 题式), &calib).unwrap();
    assert_eq!(v, json!("cold"), "{w:?}");
    assert_eq!(有(&w, "W-class-line"), 0, "{w:?}");
}

/// B68：类线用在范围外材料 → 报 W-calib-scope，路由照常。
#[test]
fn 类线也核认证范围() {
    let (v, w, _) = 跑(&路由(范围外, r#"test("该发吗", "k")"#), &类库(true)).unwrap();
    assert_eq!(v, json!("act"));
    assert_eq!(有(&w, "W-calib-scope"), 1, "{w:?}");
    let (_, w, _) = 跑(&路由(短句, r#"test("该发吗", "k")"#), &类库(true)).unwrap();
    assert_eq!(有(&w, "W-calib-scope"), 0, "{w:?}");
}

/// 范围外的类线 act 守卫不可逆 do → J-08 拒。
#[test]
fn 范围外类线不放行() {
    let e = 跑(&放行(范围外), &类库(true)).expect_err("范围外的类线不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

/// B75：范围内的类线 act 也不放行不可逆 do（`Class` 等级不放行，`20` §3.4 为准；步 20b 的临时拒绝侧即永久做法）。
#[test]
fn b75_范围内类线不放行() {
    let e = 跑(&放行(短句), &类库(true)).expect_err("B75：类线不放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
    // 对照：同一条线挂在题键上（题级认证线）照常放行
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    let (v, _, _) = 跑(&放行(短句), &calib).expect("题级认证线放行");
    assert_eq!(v, json!("已发"));
}

/// 只凭账本重放：类线出口 0 调用、结果相同。
#[test]
fn 类线出口重放零调用() {
    let src = 路由(短句, r#"test("该发吗", "k")"#);
    let calib = 类库(false);
    let (v, _, mut l) = 跑(&src, &calib).unwrap();
    let program = lower(&parse(&src).unwrap()).unwrap();
    let calls = RefCell::new(0);
    let o = run_replay(
        &program,
        桩端口(&calls),
        &calib,
        &ActionRegistry::new(),
        &mut l,
    )
    .map_err(|e| e.render())
    .unwrap();
    assert_eq!(o.value_json(), v);
    assert_eq!(*calls.borrow(), 0, "重放不该再调用");
}
