//! B94 上半旁路测试（步 23c）：`cut` 惰性，出口在第一次被检视时才解析，刷新点从 `cut` 移到检视点。
//!
//! 依据：B94（`评估/2026-09-24-仪表读数4诊断与裁定.md` §三·2、§八）；`12` §2.2 刷新点；`21` 步 23c；
//! 预注册 `地基/过程记录/工程-步23c.md`。

use std::cell::RefCell;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::Passes;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, Outcome};
use jpp::{lower, syntax::parse};

/// 是非题恒 `p`；记下每次调用问了几道题
fn 端口<'a>(p: f64, 每次: &'a RefCell<Vec<usize>>) -> Ports<'a> {
    Ports::new().with(FnPort::judge("fixed-0", move |_s, qs| {
        每次.borrow_mut().push(qs.len());
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![],
            perms: vec![],
        })
    }))
}

fn 跑(
    src: &str,
    p: f64,
    passes: Passes,
) -> Result<(Outcome, Vec<usize>), (Option<String>, String)> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 每次 = RefCell::new(vec![]);
    let mut ledger = Ledger::new();
    let actions = ActionRegistry::new();
    let mut it = jpp::interp::Interp::new(
        端口(p, &每次),
        &mut ledger,
        &calib,
        &actions,
        program.budget.clone(),
    );
    it.passes = passes;
    let r = it
        .run(&program)
        .map_err(|e| (e.rule.clone(), e.message.clone()));
    r.map(|o| (o, 每次.borrow().clone()))
}

fn 关惰性() -> Passes {
    Passes {
        lazy_cut: false,
        ..Passes::default()
    }
}

/// winnow 的形状：报错题先 `cut`，再对各块 `sieve`，最后才 `handle` 报错题的出口
const 先切后筛: &str = r#"
budget {calls: 10, cost: 0, depth: 16};
let e = cut(judge(state(mat("整份输出")), test("有报错吗", "k")));
let r = sieve(["块一", "块二", "块三"], test("相关吗", "k"));
let k = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { {r: "unsure", exit: u} }});
{k: k, n: len(r.value)}
"#;

#[test]
fn 先切后筛_一层发出() {
    let (o, 每次) = 跑(先切后筛, 0.9, Passes::default()).expect("跑完");
    assert_eq!(
        o.layers.len(),
        1,
        "cut 不刷新，sieve 的刷新点把报错题一起发出"
    );
    assert_eq!(每次.iter().sum::<usize>(), 4);
    assert_eq!(o.value_json(), serde_json::json!({"k": "act", "n": 3}));
    let (o2, _) = 跑(先切后筛, 0.9, 关惰性()).expect("跑完");
    assert_eq!(o2.layers.len(), 2, "关掉惰性过桥即改前行为：cut 当场刷新");
    assert_eq!(o2.value_json(), o.value_json(), "出口不变");
}

#[test]
fn 同一份未解析出口复制两处_解析为同一个出口() {
    // 复制到两处后各自检视：同一份责任，一处 handle 转交、另一处读种类；返回值里只算一份未决
    let src = r#"
budget {calls: 10, cost: 0, depth: 16};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let pair = [e, e];
let k = exit_kind(pair[0]);
{k: k, again: exit_kind(pair[1]), e: pair[1]}
"#;
    let (o, 每次) = 跑(src, 0.5, Passes::default()).expect("跑完");
    assert_eq!(每次, vec![1]);
    assert_eq!(o.value_json()["k"], serde_json::json!("unsure(band)"));
    assert_eq!(o.value_json()["again"], o.value_json()["k"]);
    assert_eq!(o.returned_unsure.len(), 1, "{:?}", o.returned_unsure);
}

#[test]
fn 函数里切出没检视就返回_返回前解析_j05照旧核() {
    // 切出后既没检视、也没进返回值：返回前解析，按种类核——未决即 J-05
    let 丢了 = r#"
budget {calls: 10, cost: 0, depth: 16};
fn f(t) {
    let e = cut(judge(state(mat(t)), test("行吗", "k")));
    1
}
f("甲")
"#;
    let Err((rule, _)) = 跑(丢了, 0.5, Passes::default()) else {
        panic!("未决出口被丢，应当 J-05")
    };
    assert_eq!(rule.as_deref(), Some("J-05"));
    // 已决的出口不检视也不算丢（与改前相同：只有未决有责任）
    let (o, _) = 跑(丢了, 0.9, Passes::default()).expect("已决出口不检视也照常返回");
    assert_eq!(o.value_json(), serde_json::json!(1));
}
