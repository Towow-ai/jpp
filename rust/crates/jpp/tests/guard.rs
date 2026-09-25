//! J-08（T-taint）：**放行不可逆 `do` 的守卫表达式中，至少一个合取项来自 taint=trusted 的状态**；
//! untrusted 项的数量不改变这一要求；或经 `ask`（`12`:265）。
//!
//! `00-宪法.md:44` 写着 IFC 的纪律**只有一条**，就是这一条：
//! 「**不可信材料上的判断不得单独放行不可逆 `do`**」。
//!
//! **它拦住的是什么**：一份不可信材料上的判断，单独放行一个**改变世界且撤不回**的动作。
//! 前面几包让 taint 变准了（堵了五个洗白口子、`cut` 继承状态 taint）——**那些全是这条的输入**。
//! 没有这条，taint 准了也没人用它做判断。
//!
//! **它不拦什么**（`12`:649 Nature 的裁定）：**不拦「这个 trusted 是不是真的可信」**。
//! `taint_out="trusted"` 是作者的**显式标记**，语言只保证它可见可追，**不设审核方**。
//! 所以这条查的是「守卫里有没有一个 trusted 合取项」，不是「那个 trusted 配不配」。

mod common;
use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, TaintOut, run};
use jpp::{lower, syntax::parse};

/// 判断恒给 `p`、问人恒答 0.95、不该生成（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口<'a>(p: f64, calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Ok(Some(Answer::Noul(0.95)))
        }))
}

fn 跑(src: &str, p: f64) -> Result<jpp::Outcome, jpp::Error> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    common::certified(&mut calib, "k", 0.65, 0.35, 100);
    let mut actions = ActionRegistry::new();
    // 不可逆：发出去就收不回
    actions.register("发邮件", 0.0, false, TaintOut::Trusted, |_| {
        Ok(jpp::value::Value::text("已发"))
    });
    // 可逆：写本地草稿，J-08 不管它
    actions.register("存草稿", 0.0, true, TaintOut::Trusted, |_| {
        Ok(jpp::value::Value::text("已存"))
    });
    actions.register("取外部数据", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(jpp::value::Value::text("外面来的"))
    });
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    run(&program, 定值端口(p, &calls), &calib, &actions, &mut ledger)
}

/// **不可信材料上的判断单独放行不可逆 `do`** —— J-08 要拦的就是这一个。
#[test]
fn 不可信判断不能单独放行不可逆do() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 可以发吗 = handle(cut(judge(state(脏), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 可以发吗 { content(do("发邮件", [], 0)) } else { "没发" };
{r: 结果}
"#;
    let e = 跑(src, 0.9).expect_err("不可信判断单独放行不可逆 do 该被拦");
    let t = e.render();
    assert!(t.contains("J-08"), "该是 J-08：{t}");
    assert!(
        t.contains("trusted") || t.contains("可信"),
        "报文要说清缺什么：{t}"
    );
}

/// 守卫里**有一个 trusted 合取项**就放行（`12`:265「至少一个」）。
/// 注意：**untrusted 项的数量不改变这一要求**——旁边挂着多少个不可信判断都不影响。
#[test]
fn 有一个trusted合取项就放行() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 干净 = mat("源码里的字面量");
let 脏判断 = handle(cut(judge(state(脏), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 净判断 = handle(cut(judge(state(干净), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 净判断 && 脏判断 { content(do("发邮件", [], 0)) } else { "没发" };
{r: 结果}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("有 trusted 合取项该放行：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}

/// **可逆的 `do` 不受这条管**（`12`:265 只说「不可逆」）。别误伤。
#[test]
fn 可逆的do不受管() {
    let src = r#"
budget {calls: 3, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 可以吗 = handle(cut(judge(state(脏), test("该存吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 可以吗 { content(do("存草稿", [], 0)) } else { "没存" };
{r: 结果}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("可逆动作不该被 J-08 拦：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已存"));
}

/// **没有守卫的不可逆 `do` 不受这条管**：J-08 管的是「放行的守卫」，
/// 一个无条件执行的 `do` 没有守卫可查。作者直接写 `do` 是他自己的决定。
#[test]
fn 无条件的不可逆do不受管() {
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
{r: content(do("发邮件", [], 0))}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("无守卫的 do 不该被拦：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}

/// **经 `ask` 也放行**（`12`:265「或经 `ask`」）——人答是 trusted（§2.11）。
#[test]
fn 经ask放行() {
    let src = r#"
budget {calls: 3, cost: 1, depth: 8, escalate: 1};
let 脏 = do("取外部数据", [], 0);
let 人说 = handle(ask(state(脏), test("该发吗", "k")), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
let 结果 = if 人说 { content(do("发邮件", [], 0)) } else { "没发" };
{r: 结果}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("经 ask 该放行：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}

/// **来源要能穿过 helper 函数**。这条是实测撞出来的：J-08 第一版误伤了
/// `examples/lifecycle.jpp`——它的守卫是 `approval.resolved && approval.value`，
/// 而 `approval` 来自 `fn request_test(…) -> Record !{ask}`，**里面是走 ask 的**。
///
/// 出口按帧记，helper 自成一帧，求值结束帧就弹掉了——于是「经 ask」这个事实在返回时丢了。
/// **误伤会处理失败的程序，和放过无声失败一样是错**，所以这条留作回归。
#[test]
fn 来源要穿过helper函数() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8, escalate: 1};
fn 问人(m) -> Record !{ask} {
    handle(ask(state(m), test("该发吗", "k")), {
        act: fn() { {好了: true, 值: true} },
        ignore: fn() { {好了: true, 值: false} },
        unsure: fn(u) { consume(u, "drop"); {好了: false, 值: unit} }})
}
let 批了 = 问人(mat("甲"));
{r: if 批了.好了 && 批了.值 { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("helper 里走 ask 该放行：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));

    // 反面：helper 里不走 ask、状态是脏的，仍要拦——别为了穿透 helper 把检查弄没
    let 脏 = r#"
budget {calls: 2, cost: 1, depth: 8};
fn 看看(m) -> Record !{judge} {
    handle(cut(judge(state(m), test("该发吗", "k"))), {
        act: fn() { {好了: true, 值: true} },
        ignore: fn() { {好了: true, 值: false} },
        unsure: fn(u) { consume(u, "drop"); {好了: false, 值: unit} }})
}
let 脏料 = do("取外部数据", [], 0);
let 批了 = 看看(脏料);
{r: if 批了.好了 && 批了.值 { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let e = 跑(脏, 0.9).expect_err("helper 里是脏状态上的判断，仍该拦");
    assert!(e.render().contains("J-08"), "{}", e.render());
}

/// **三行就能击穿 J-08**（实测查出，`gap-sweep-1` 报的）：多套一层容器就绕过洗白检查。
///
/// ```text
/// let 脏 = do("取外部数据", [], 0);
/// let 拆了 = content(脏);
/// let 洗白了 = mat({outer: 拆了});   // ← 前两个补丁只比顶层那一个值，这里绕过去了
/// ```
///
/// **这是同一个洞的第三个包装**（前两个：`mat(content(脏))` 原样直传、`==` 吃掉不可比）。
/// 所以没有再打第三个窄补丁——**按值精确匹配这条路本身是错的，包装方式是无穷的**。
/// 改成记**内容及其所有子结构**，判定变成「这个新材料里含不含从 untrusted 材料拆出来的东西」。
///
/// 这条记录的方向也订正了：我先前在 `0dbcbc1` 里把它写成「已知的**假拒绝**方向」，
/// **实测是假放行**。记成假拒绝的东西没有人会急着修——**一个缺口记错方向，比没记还危险**。
#[test]
fn 套一层容器也不能洗白() {
    for (名, 包装) in [
        ("记录字段", "mat({outer: 拆了})"),
        ("列表", "mat([拆了])"),
        ("嵌两层", "mat({a: {b: 拆了}})"),
    ] {
        let src = format!(
            r#"
budget {{calls: 2, cost: 1, depth: 8}};
let 脏 = do("取外部数据", [], 0);
let 拆了 = content(脏);
let 洗白了 = {包装};
let 可以发吗 = handle(cut(judge(state(洗白了), test("该发吗", "k"))), {{
    act: fn() {{ true }}, ignore: fn() {{ false }},
    unsure: fn(u) {{ consume(u, "drop"); false }}}});
{{r: if 可以发吗 {{ content(do("发邮件", [], 0)) }} else {{ "没发" }}}}
"#
        );
        let e = 跑(&src, 0.9).expect_err(&format!("{名}：套一层容器也不该洗白，本该被 J-08 拦下"));
        assert!(
            e.render().contains("J-08"),
            "{名} 该是 J-08：{}",
            e.render()
        );
    }
}

/// 反面：**无辜的字面量套同样的容器不该被误伤**。
/// 假拒绝和假放行一样是错，而且这一条的判定收窄了（记子结构），更要防。
#[test]
fn 套容器的无辜字面量不被误伤() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let 干净 = mat({outer: "源码里的字面量"});
let 可以发吗 = handle(cut(judge(state(干净), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
{r: if 可以发吗 { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let out = 跑(src, 0.9).unwrap_or_else(|e| panic!("无辜字面量不该被拦：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}

/// **来源通道必须有作用域**（`12` §2.11 第五条，`boundary-scan-1` 实测查出）。
///
/// 我为了给 `Value::Bool`（没地方挂 taint）补来源，另开了一张**按变量名索引的表**，
/// 加一条跨帧的传送带。**那条传送带没有作用域**：一次「返回值不是 `Bool`/`Record`、
/// 内部走过 `ask`」的调用会把「经过 ask」留在带上，被之后**任意一条不相关的 `let`** 继承。
///
/// **而这同时解释了那次假拒绝**（`lifecycle.jpp` 的守卫经过 `ask` 却被拦）——
/// **漏和串是同一个病的两种表现，不是两个 bug**。我上次只修了漏的那一面，没动作用域。
///
/// 判别法：**这条来源通道，它的作用域是谁给的？** 现在是环境给的。
#[test]
fn 来源不能串到不相关的绑定上() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8, escalate: 1};
fn 走过ask(m) -> Mat !{ask} {
    let e = ask(state(m), test("问问", "k"));
    consume(e, "drop");
    mat("返回的是材料不是 Bool")
}
let 脏1 = do("取外部数据", [], 0);
let 材料 = 走过ask(脏1);
let 脏2 = do("取外部数据", [], 0);
let bad_guard = handle(cut(judge(state(脏2), test("该发吗", "k"))), {
    act: fn() { true }, ignore: fn() { false },
    unsure: fn(u) { consume(u, "drop"); false }});
{r: if bad_guard { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let e = 跑(src, 0.9).expect_err("不相关的脏判断不该继承别处的「经过 ask」");
    assert!(e.render().contains("J-08"), "该是 J-08：{}", e.render());
}

/// 纯字面量赋值更不该被污染。
#[test]
fn 纯字面量绑定不继承来源() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8, escalate: 1};
fn 走过ask(m) -> Mat !{ask} {
    let e = ask(state(m), test("问问", "k"));
    consume(e, "drop");
    mat("材料")
}
let 脏 = do("取外部数据", [], 0);
let 材料 = 走过ask(脏);
let ok = true;
{r: if ok { content(do("发邮件", [], 0)) } else { "没发" }}
"#;
    let e = 跑(src, 0.9).expect_err("`let ok = true` 没有任何来源，不该放行不可逆动作");
    assert!(e.render().contains("J-08"), "该是 J-08：{}", e.render());
}

/// **来源要跟到「值」，不是名字、不是整条记录**（真一号，独立校准顾问挖出的）。
///
/// ```text
/// let 包 = 混合(脏, 净);        // 一次求值，两种来源
/// if 包.脏字段 { do(不可逆) }   // 修之前：过
/// ```
///
/// **它自相矛盾**：`walk_conjuncts` 在**守卫那一层**专门拒绝追析取（注释写着
/// 「取反、析取：里面的东西不再是这个条件成立所保证的，不追」），
/// **而同一次修法在绑定那一层做了析取折叠**——守卫里不许写 `脏 || 净`，
/// 但包一层记录再取字段就过。**同一份谨慎，隔一层被自己拆掉。**
///
/// 根子：`12`:265 要的是「至少一个**合取项**来自 trusted 状态」，
/// **合取项是值级的概念，而实现做成了名字级。**
#[test]
fn 记录的每个字段各有各的来源() {
    let 头 = r#"
budget {calls: 6, cost: 1, depth: 8};
fn 混合(脏, 净) -> Record !{judge} {
    let 脏判 = handle(cut(judge(state(脏), test("脏问题", "k"))), {
        act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
    let 净判 = handle(cut(judge(state(净), test("净问题", "k"))), {
        act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, "drop"); false }});
    {脏字段: 脏判, 净字段: 净判}
}
let 脏 = do("取外部数据", [], 0);
let 净 = mat("源码里的字面量");
let 包 = 混合(脏, 净);
"#;
    // (c) 脏字段当守卫：该拦
    let 用脏 =
        format!("{头}{{r: if 包.脏字段 {{ content(do(\"发邮件\", [], 0)) }} else {{ \"没发\" }}}}");
    let e = 跑(&用脏, 0.9).expect_err("用不可信判断决定的那个字段当守卫，该被 J-08 拦");
    assert!(e.render().contains("J-08"), "该是 J-08：{}", e.render());

    // (c) 净字段当守卫：该过
    let 用净 =
        format!("{头}{{r: if 包.净字段 {{ content(do(\"发邮件\", [], 0)) }} else {{ \"没发\" }}}}");
    let out = 跑(&用净, 0.9).unwrap_or_else(|e| panic!("可信材料上的判断该放行：{}", e.render()));
    assert_eq!(out.value_json()["r"], serde_json::json!("已发"));
}
