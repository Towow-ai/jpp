//! B68：认证范围。线用在认证集材料范围之外时，出口照常路由，但不算放行不可逆 `do` 的可信合取项（J-08 拒）；
//! 范围内照常放行；记录没有指纹时按范围内处理。
//! 依据：B68（地基/附注/2026-09-24-探针首轮裁定.md）；`21` 步 20d-1。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, syntax::parse};
use jpp_value::stat::ScopeRanges;
use serde_json::{Value as Json, json};

/// 步 15c：原 `impl Client` 的桩改为三个闭包端口；`calls` 用 `RefCell` 记调用数（未被断言，保留计数是为了与原桩同纪律）
fn 桩端口(calls: &RefCell<u64>) -> Ports<'_> {
    Ports::new()
        .with(FnPort::judge("m", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.95)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

/// 认证集：中文日常短句（与第四轮 R 题式同风格）
const 认证集: [&str; 6] = [
    "妈妈削了一个苹果，分给我们两半，酸甜正好。",
    "早高峰的地铁挤得人喘不过气，他差点坐过站。",
    "她把积蓄的一部分投进了基金，心里还是没底。",
    "周末一家三口去公园放风筝，孩子笑得很开心。",
    "医生建议他少吃油炸食品，多吃新鲜蔬菜水果。",
    "他一个人吃完了那顿火锅，手机始终没有响过。",
];

fn 跑(材料: &str, 带指纹: bool) -> Result<(Json, Vec<String>), String> {
    let src = format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
handle(cut(judge(state(mat({材料:?})), test("该发吗", "k"))), {{
    act: fn() {{ content(do("发邮件", [], 0)) }},
    ignore: fn() {{ "没发" }},
    unsure: fn(u) {{ consume(u, "drop"); "没发" }}}})
"#
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    if !带指纹 {
        // B104-2（步 20h-1）：没有指纹的记录
        calib.records.get_mut("k").unwrap().scope = None;
    } else {
        let r = calib.records.get_mut("k").unwrap();
        r.scope = Some(jpp::effects::CalibScope {
            batches: vec!["t".into()],
            sources: Default::default(),
            note: String::new(),
            fingerprint: ScopeRanges::from_texts(认证集, (0.0, 1.0), None),
            n_text: None,
            extensions: vec![],
        });
    }
    let mut a = ActionRegistry::new();
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    let mut l = Ledger::new();
    let calls = RefCell::new(0u64);
    run(&program, 桩端口(&calls), &calib, &a, &mut l)
        .map(|o| (o.value_json(), o.trace.warnings.clone()))
        .map_err(|e| e.render())
}

const 范围外: &str = "ERROR: build failed at src/main.rs:42\nexpected `;`, found `}`\nerror: aborting due to 1 previous error";

/// 范围外：act 臂里的不可逆 do 被 J-08 拒，并报 W-calib-scope。
#[test]
fn 范围外的act不放行不可逆do() {
    let e = 跑(范围外, true).expect_err("范围外的线不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

/// 范围内：照常放行，不报 W-calib-scope。
#[test]
fn 范围内照常放行() {
    let (v, w) = 跑("他把旧自行车卖了，打算换一辆折叠车上下班。", true).expect("范围内该放行");
    assert_eq!(v, json!("已发"));
    assert!(!w.iter().any(|x| x.starts_with("W-calib-scope")), "{w:?}");
}

/// 记录没有指纹：范围未知（B104-2，步 20h-1 起），出口照常路由，但不放行不可逆 do（J-08 拒），报 W-scope-unknown。
#[test]
fn 没有指纹范围未知不放行() {
    let e = 跑("他把旧自行车卖了，打算换一辆折叠车上下班。", false)
        .expect_err("范围未知的线不该放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}
