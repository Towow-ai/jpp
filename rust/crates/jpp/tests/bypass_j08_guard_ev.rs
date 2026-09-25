//! J-08 守卫证据 `GuardEv` 的绕过测试（`20` v2 §2.4 J-08 行「`bypass/j08_*`」、§3.1；`21` 步 16；B121）。
//!
//! 放行判定只读守卫栈上的 `GuardEv`。证据只在 `handle` 分派处产生（`GuardEv::from_exit`：未决恒空，
//! 否则「已决 ∧ 可信 ∧ 线放行」与 `via_ask`），压上所选臂的守卫栈，并进臂返回值的每个 `Bool` 叶子；
//! `&&` 取并，`||`、`!`、比较与其他运算为空；`if` 不把条件证据传给分支值；字段、下标、函数返回、
//! `let` 原样携带。每条一个程序，断言放行或 `J-08` 拒绝。
//!
//! **一条曾经红、现在绿的记录。** j08_2 第一句、j08_8 前两句、j08_9 第三句、j08_14 第一句在步 16 之前
//! 被拒：旧实现按名字与**求值窗口**归属来源（let 旁路表只收「这条 `let` 求值期间新产生的出口」），
//! 而这里的 `净` 在上一条 `let` 里就已切出，`净判` 取不到来源——那是假拒绝。步 16 改为值级证据
//! （B121-1、B121-5）后，它们都是可信、放行等级出口的读出或其 `&&` 合取与容器往返，放行。
//!
//! `20` 的原十条清单：字面 `true`（j08_1）、可信∧不可信（j08_2）、夹具线出口（j08_3）、读 `taint` 标签
//! （j08_4）、拼接（j08_5）、join（j08_13；析取另见 j08_6）、Fail（j08_7）、容器（j08_8）、handler 臂
//! （j08_9）、隐式流反例（j08_10）。另加：取反（j08_11）、未决出口（j08_12，B121-2）、直接给值的臂
//! （j08_14，B121-1 (f)）、嵌套 `handle` 取并（j08_15，B121-1 (g)）；GUIDE 的三种写法（j08_16–18，
//! B121-6）；运行期放行的形状静态面不报（j08_19，B121-4）。

mod common;

use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::run;
use jpp::value::{Answer, Question, State, Taint, Value};
use jpp::{lower, syntax::parse};
use serde_json::Value as Json;

/// 判断器对一切题回同一个是非读数 p
fn 端口<'a>(p: f64) -> Ports<'a> {
    Ports::new().with(FnPort::judge("m", move |_s: &State, qs: &[&Question]| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
        })
    }))
}

fn 动作表() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    // 取外部数据：可逆，产物不可信
    a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::Text("外面来的".into(), Taint::Trusted.into()))
    });
    // 发邮件：不可逆
    a.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已发".into(), Taint::Trusted.into()))
    });
    a
}

/// `k`：正式线（放行等级）；`f`：`put` 写的夹具线（不放行，B29）
fn 跑p(src: &str, p: f64) -> Result<Json, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.8, 0.2, 50);
    calib.put("f", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap(); // 步 15d-2：夹具线显式给 δ（步 15d-2 前的代码兜底值）
    let mut l = Ledger::new();
    run(&program, 端口(p), &calib, &动作表(), &mut l)
        .map(|o| o.value_json())
        .map_err(|e| e.render())
}

fn 跑(src: &str) -> Result<Json, String> {
    跑p(src, 0.95)
}

/// 共同前缀：`净判`、`脏判` 是 handle 分派出的 Bool；`净`、`脏` 是出口本身
const 前缀: &str = r#"
budget {calls: 4, cost: 1, depth: 8};
let 干净 = mat("程序自己写的材料");
let 外来 = do("取外部", [], 0);
let 净 = cut(judge(state(干净), test("该发吗", "k")));
let 脏 = cut(judge(state(外来), test("该发吗", "k")));
let 净判 = handle(净, {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
let 脏判 = handle(脏, {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
"#;

fn 程序(守卫与动作: &str) -> String {
    format!("{前缀}{守卫与动作}\n")
}

fn 放行(src: &str) {
    let v = 跑(src).unwrap_or_else(|e| panic!("应放行，却被拒：{e}\n{src}"));
    assert_eq!(v, Json::from("已发"), "{src}");
}

fn 拒(src: &str) {
    let e = 跑(src).expect_err(&format!("应被 J-08 拒：\n{src}"));
    assert!(e.contains("J-08"), "{e}");
}

const 发: &str = r#"content(do("发邮件", [], 0))"#;

#[test]
fn j08_1_字面true没有证据() {
    拒(&程序(&format!(
        "let ok = true;\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_2_可信合取不可信放行() {
    // taint 按 ∨ 是不可信，但有一个可信合取项（`12` J-08「untrusted 项的数量不改变这一要求」）
    放行(&程序(&format!(
        "if 脏判 && 净判 {{ {发} }} else {{ \"不发\" }}"
    )));
    // 只有不可信的一项：拒
    拒(&程序(&format!("if 脏判 {{ {发} }} else {{ \"不发\" }}")));
}

#[test]
fn j08_3_夹具线出口不算可信合取项() {
    let src = format!(
        "budget {{calls: 2, cost: 1, depth: 8}};\nlet 夹判 = handle(cut(judge(state(mat(\"程序自己写的材料\")), test(\"该发吗\", \"f\"))), {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif 夹判 {{ {发} }} else {{ \"不发\" }}\n"
    );
    拒(&src);
}

#[test]
fn j08_4_读taint标签不是判断() {
    // `12` 2026-09-21 13:45 裁定：字符串比较没有判断证据（条件为真、仍拒）
    拒(&程序(&format!(
        "if taint(净) == \"trusted\" {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_5_拼接与比较清空证据() {
    // 把可信判断的结果拼进文本再比较：运算产生的 Bool 没有证据
    拒(&程序(&format!(
        "if text(净判) + \"!\" == \"true!\" {{ {发} }} else {{ \"不发\" }}"
    )));
    拒(&程序(&format!(
        "if 净判 == true {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_6_析取不留证据() {
    // `a || b` 成立时不知道是哪一侧成立（`||` 清空，`20` v2 §3.1）
    拒(&程序(&format!(
        "if 净判 || 脏判 {{ {发} }} else {{ \"不发\" }}"
    )));
    拒(&程序(&format!(
        "if 净判 || false {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_7_fail判定没有证据() {
    拒(&程序(&format!(
        "if !is_fail(净判) {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_8_容器保留叶子各自的证据() {
    // 装进记录、列表再取出，证据跟着叶子走
    放行(&程序(&format!(
        "let r = {{v: 净判}};\nif r.v {{ {发} }} else {{ \"不发\" }}"
    )));
    放行(&程序(&format!(
        "let xs = [净判];\nif xs[0] {{ {发} }} else {{ \"不发\" }}"
    )));
    // 同一条记录里另一字段的证据不串到这个字段（值级，不按记录折）
    拒(&程序(&format!(
        "let r = {{脏字段: 脏判, 净字段: 净判}};\nif r.脏字段 {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_9_handler臂() {
    // do 写在臂里：分派出口就是守卫（守卫栈）
    放行(&程序(&format!(
        "handle(净, {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})"
    )));
    拒(&程序(&format!(
        "handle(脏, {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}})"
    )));
    // 臂返回的记录：每个 Bool 叶子带分派出口的证据（lib/observations.jpp 的写法）
    放行(&程序(&format!(
        "let o = handle(净, {{act: fn() {{ {{resolved: true, value: true}} }}, ignore: fn() {{ {{resolved: true, value: false}} }}, unsure: fn(u) {{ consume(u, \"drop\"); {{resolved: false, value: false}} }}}});\nif o.resolved && o.value {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_10_隐式流不传证据() {
    // if 分支里的字面量不带证据（B33 第 3 条：控制流不传播；证据只经 handle 分派产生）
    拒(&程序(&format!(
        "let ok = if 净判 {{ true }} else {{ false }};\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_11_取反清空证据() {
    拒(&程序(&format!(
        "if !(!净判) {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_12_未决出口不给证据() {
    // 可信材料、正式线，p = 0.5 切出 Unsure：臂里的 do 与臂返回的 true 都没有证据
    let 臂里 = format!(
        "budget {{calls: 2, cost: 1, depth: 8}};\nhandle(cut(judge(state(mat(\"程序自己写的材料\")), test(\"该发吗\", \"k\"))), {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); {发} }}}})\n"
    );
    let e = 跑p(&臂里, 0.5).expect_err("未决出口不是放行判定");
    assert!(e.contains("J-08"), "{e}");
    let 返回 = format!(
        "budget {{calls: 2, cost: 1, depth: 8}};\nlet ok = handle(cut(judge(state(mat(\"程序自己写的材料\")), test(\"该发吗\", \"k\"))), {{act: fn() {{ true }}, ignore: fn() {{ true }}, unsure: fn(u) {{ consume(u, \"drop\"); true }}}});\nif ok {{ {发} }} else {{ \"不发\" }}\n"
    );
    let e = 跑p(&返回, 0.5).expect_err("未决出口不是放行判定");
    assert!(e.contains("J-08"), "{e}");
}

#[test]
fn j08_13_join内置清空证据() {
    拒(&程序(&format!(
        "if join([text(净判)], \"\") == \"true\" {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_14_直接给值的臂同样盖值() {
    放行(&程序(&format!(
        "let ok = handle(净, {{act: true, ignore: false, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
    拒(&程序(&format!(
        "let ok = handle(脏, {{act: true, ignore: false, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
}

#[test]
fn j08_15_嵌套handle取并() {
    // 外层脏、内层净：内层臂返回值带净出口的证据，外层盖上脏出口的空证据，并仍放行
    放行(&程序(&format!(
        "let ok = handle(脏, {{act: fn() {{ handle(净, {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}}) }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
    // 外层净、内层脏：外层盖值带净出口的证据
    放行(&程序(&format!(
        "let ok = handle(净, {{act: fn() {{ handle(脏, {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}}) }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
    // 两层都脏：拒
    拒(&程序(&format!(
        "let ok = handle(脏, {{act: fn() {{ handle(脏, {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}}) }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
    )));
}

/// GUIDE 守卫一节的两种合法写法与一种不合法写法（B121-6），在同一个 `sieve` 结果上各跑一次。
const 筛: &str = r#"
budget {calls: 8, cost: 1, depth: 8};
let r = sieve([mat("甲"), mat("乙")], test("该发吗", "k"));
let xs = map(r.value, fn(e) { e.item });
"#;

#[test]
fn j08_16_guide写法_见证出口放行批量动作() {
    放行(&format!(
        "{筛}if len(xs) > 0 {{ handle(r.value[0].exit, {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}}) }} else {{ \"不发\" }}\n"
    ));
}

#[test]
fn j08_17_guide写法_do进臂逐项放行() {
    放行(&format!(
        "{筛}let 发了 = map(r.value, fn(e) {{ handle(e.exit, {{act: fn() {{ {发} }}, ignore: fn() {{ \"不发\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"不发\" }}}}) }});\n发了[0]\n"
    ));
}

/// 静态子面（24-0）与运行期一致：运行期放行的程序，静态面一处也不报（静态报出 ⊆ 运行期拒绝，B121-4）。
/// 取步 16 从拒改放的四种形状与嵌套取并，带动作表检查（`取外部` 登记为输出不可信，`发邮件` 不可逆）。
#[test]
fn j08_19_运行期放行的形状静态面不报() {
    use jpp::check::{ActionFacts, ActionTable, check_with_calib_actions};
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
        "取外部".into(),
        ActionFacts {
            reversible: true,
            output_untrusted: true,
            no_sandbox: false,
        },
    );
    let 形状 = [
        format!("if 脏判 && 净判 {{ {发} }} else {{ \"不发\" }}"),
        format!("let r = {{v: 净判}};\nif r.v {{ {发} }} else {{ \"不发\" }}"),
        format!(
            "let o = handle(净, {{act: fn() {{ {{resolved: true, value: true}} }}, ignore: fn() {{ {{resolved: true, value: false}} }}, unsure: fn(u) {{ consume(u, \"drop\"); {{resolved: false, value: false}} }}}});\nif o.resolved && o.value {{ {发} }} else {{ \"不发\" }}"
        ),
        format!(
            "let ok = handle(净, {{act: true, ignore: false, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
        ),
        format!(
            "let ok = handle(脏, {{act: fn() {{ handle(净, {{act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}}) }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
        ),
        // 脏出口分派、臂返回已带净证据的值：名字、直接给值的臂、容器三种写法
        format!(
            "let ok = handle(脏, {{act: fn() {{ 净判 }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
        ),
        format!(
            "let ok = handle(脏, {{act: 净判, ignore: false, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ {发} }} else {{ \"不发\" }}"
        ),
        format!(
            "let r = handle(脏, {{act: fn() {{ {{v: 净判}} }}, ignore: fn() {{ {{v: false}} }}, unsure: fn(u) {{ consume(u, \"drop\"); {{v: false}} }}}});\nif r.v {{ {发} }} else {{ \"不发\" }}"
        ),
    ];
    for g in 形状 {
        let src = 程序(&g);
        放行(&src);
        let p = lower(&parse(&src).expect("解析")).expect("lower");
        let r = check_with_calib_actions(&p, &CalibStore::new(), &t);
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| d.rule == "J-08" || d.rule == "W-guard-untrusted"),
            "运行期放行的程序静态面不该报：{:?}\n{src}",
            r.diagnostics
        );
    }
}

#[test]
fn j08_18_guide写法_比较式守卫不放行() {
    // triage-do 的形状：`len(xs) > 0` 是比较式，没有证据（B121-6 (i)）
    拒(&format!(
        "{筛}if len(xs) > 0 {{ {发} }} else {{ \"不发\" }}\n"
    ));
}
