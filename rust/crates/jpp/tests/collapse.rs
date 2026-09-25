//! **一个改动若使某个出口种类变得不可达，它就不是「收紧」，不论方向。**
//!
//! 活口子：`put` 的收紧判据写的是 `hi >= old.hi && lo <= old.lo`，
//! **于是 `lo` 降到 `0.0` 算「收紧」、免凭据——而 `p <= 0` 意味着 `Ignore` 在那个键上不可达**。
//!
//! 通则原文的理由是「带更宽 = `Unsure` 更多 = 往拒绝那边倒」，
//! **而那句把「拒绝」默认等同于「不给 `Act`」**。
//! **`Act` 与 `Ignore` 是对称的两个判定**——写 `if 不安全(x) { 拦下 }` 时，
//! **`Ignore` 才是放行的那个答案**。**语言不知道哪一侧对这个程序才是安全的那一侧。**

mod common;
use jpp::effects::{CalibStore, LiteralMode, Sample};

fn 装(c: &mut CalibStore, key: &str) {
    for i in 0..30 {
        c.absorb(
            key,
            Sample {
                p: Some(0.30 + i as f64 * 0.02),
                label: Some(if i > 6 { 1 } else { 0 }),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
}

/// **红一：把 `lo` 推到 0 不是收紧，要凭据。**
#[test]
fn 把lo推到零不算收紧() {
    let mut c = CalibStore::new();
    装(&mut c, "k");
    c.commission("k", 0.45, 0.10, "条").expect("认得动");
    // 先给它一条两侧都可达的线（收紧 hi、抬起 lo 不算收紧，所以这一步也要走证书之外的路）
    c.put("k", 0.99, 0.0, 30, "停岗", None)
        .expect("停岗不受收紧门管");
    c.put("k", 0.90, 0.10, 30, "上岗", Some(0.05))
        .expect_err("从停岗改上岗要凭据");

    // 造一条两侧都可达的上岗线（没有带标注证据的键不受门管）
    let mut d = CalibStore::new();
    d.put("k", 0.80, 0.20, 5, "上岗", Some(0.05)).unwrap();
    装(&mut d, "k"); // 现在它有带标注的证据了

    // 抬 hi（少放行）+ 保持 lo → 是收紧，免凭据
    d.put("k", 0.90, 0.20, 35, "上岗", Some(0.05))
        .expect("两侧都不扩大放行，免凭据");
    // **把 lo 推到 0 → `Ignore` 不可达 → 不是收紧**
    let e = d
        .put("k", 0.90, 0.0, 35, "上岗", Some(0.05))
        .expect_err("**使 Ignore 不可达不是收紧**");
    assert!(
        e.contains("不可达") || e.contains("Ignore"),
        "要说清是哪一种出口没了：{e}"
    );
}

/// **红二：把 `hi` 推到 1 同样不算收紧**——对称的那一侧不能只防一边。
#[test]
fn 把hi推到一也不算收紧() {
    let mut d = CalibStore::new();
    d.put("k", 0.80, 0.20, 5, "上岗", Some(0.05)).unwrap();
    装(&mut d, "k");
    let e = d
        .put("k", 1.0, 0.20, 35, "上岗", Some(0.05))
        .expect_err("**使 Act 不可达也不是收紧**");
    assert!(e.contains("不可达") || e.contains("Act"), "{e}");
}

/// **`commission` 自己是这条规则以前最大的违反者**：它置 `r.lo = 0.0`。
/// 它**有凭据**（那张证书），所以照办；但**必须留痕**——而且留的痕要能被算出来核对，
/// 不是一句注释。
#[test]
fn 认证之后不可达的那一侧要说得出来() {
    let mut c = CalibStore::new();
    装(&mut c, "k");
    let cert = c.commission("k", 0.45, 0.10, "条").expect("认得动");
    let rec = c.get("k");
    assert_eq!(rec.lo, 0.0);
    // **算出来的，不是填的**
    assert_eq!(
        rec.不可达出口(),
        vec!["ignore"],
        "认证之后 Ignore 不可达，记录要说得出来"
    );
    assert!(
        cert.bounded_side.contains("lo"),
        "证书上那句也还在：{}",
        cert.bounded_side
    );

    // 两侧都可达的线上，这一栏是空的
    let mut d = CalibStore::new();
    d.put("k", 0.8, 0.2, 5, "上岗", Some(0.05)).unwrap();
    assert!(d.get("k").不可达出口().is_empty());
}

/// **`.jpp` 作者要能读到出口的 taint。**
///
/// 宪法第 44 行唯一那条 IFC 纪律建在 taint 上，**而 `taint` 从 `cause` 删掉之后，
/// `.jpp` 作者再没有任何东西能说出「这个判断站在不可信材料上」**——
/// J-08 只会在 `do` 那里**拒绝**，作者拿不到任何**在被拒绝之前**读得到的东西。
///
/// 对照：「测没测过」被判为必须是一位正交的、对 handler 可见的东西。
/// **重的那条待遇更弱。**
#[test]
fn 出口的taint对jpp可见() {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::interp::{ActionRegistry, TaintOut};
    use jpp::ledger::Ledger;
    use jpp::run;
    use jpp::value::{Answer, Value};
    /// 判断恒给 0.99，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
    fn 桩端口() -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate(
                "m",
                |_p, _c: &[serde_json::Value], _n, _r| Err(EffectError("x".into())),
            ))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
    let 跑 = |动作: &str| {
        let mut a = ActionRegistry::new();
        a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
            Ok(Value::Text("脏".into(), jpp::value::Taint::Trusted.into()))
        });
        a.register("取内部", 0.0, true, TaintOut::Trusted, |_| {
            Ok(Value::Text("净".into(), jpp::value::Taint::Trusted.into()))
        });
        let src = format!(
            r#"
budget {{calls: 4, cost: 1, depth: 8}};
let m = do("{动作}", [], 0);
let e = cut(judge(state(m), test("行吗", "k")));
handle(e, {{act: fn() {{ {{来源: taint(e)}} }}, ignore: fn() {{ {{来源: taint(e)}} }},
           unsure: fn(u) {{ consume(u, "drop"); {{来源: taint(e)}} }}}})
"#
        );
        let program = jpp::lower(&jpp::syntax::parse(&src).expect("解析")).expect("lower");
        let mut calib = CalibStore::new();
        calib.put("k", 0.8, 0.2, 50, "上岗", Some(0.05)).unwrap();
        let mut l = Ledger::new();
        run(&program, 桩端口(), &calib, &a, &mut l)
            .expect("跑得完")
            .value_json()
    };
    assert_eq!(跑("取内部")["来源"], "trusted");
    assert_eq!(
        跑("取外部")["来源"],
        "untrusted",
        "**作者要能在被 J-08 拒绝之前就读到这件事**"
    );
}

/// **读得到 taint 不等于据它分支就合规**——总控裁定：
/// **`if taint(e) == "trusted" { do(不可逆) }` 不满足 J-08。**
///
/// 理由：J-08 要的是**一个在可信状态上做出的判断**（那是一次 `judge`，
/// 可信度来自被判断的那份材料）；而 `taint(e) == "trusted"` 是**一次字符串比较**，
/// 可信度来自「我读了一个字段」。**两者在守卫里都是 `Bool`，
/// 但一个说「可信材料支持这件事」，另一个只说「我看过标签」。**
/// 允许它，**J-08 就变成可以自证的**——而那个 `trusted` 恰恰是 J-08 要去查的东西本身。
///
/// **实测结论：今天已经拦得住**，三种写法都拦。原因是 `walk_conjuncts` 只认 `&&`，
/// **任何别的 `Binary` 都不贡献可信项**。这一条把它**钉住**，免得有人「顺手」
/// 让比较表达式也传递来源——**那一改就是开这个口子**。
#[test]
fn 读taint不能替代可信判断() {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::interp::{ActionRegistry, TaintOut};
    use jpp::ledger::Ledger;
    use jpp::run;
    use jpp::value::{Answer, Value};
    /// 判断恒给 0.99，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
    fn 桩端口() -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate(
                "m",
                |_p, _c: &[serde_json::Value], _n, _r| Err(EffectError("x".into())),
            ))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
    let 跑 = |src: &str| {
        let mut a = ActionRegistry::new();
        a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
            Ok(Value::Text("脏".into(), jpp::value::Taint::Trusted.into()))
        });
        a.register("取内部", 0.0, true, TaintOut::Trusted, |_| {
            Ok(Value::Text("净".into(), jpp::value::Taint::Trusted.into()))
        });
        a.register("发出去", 0.0, false, TaintOut::Trusted, |_| {
            Ok(Value::Text(
                "发了".into(),
                jpp::value::Taint::Trusted.into(),
            ))
        });
        let program = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
        let mut calib = CalibStore::new();
        common::certified(&mut calib, "k", 0.7, 0.3, 50);
        let mut l = Ledger::new();
        run(&program, 桩端口(), &calib, &a, &mut l)
            .map(|o| o.value_json().to_string())
            .map_err(|e| e.render())
    };

    // 形一：直接把比较写进守卫。
    //
    // **比较写成 `== "untrusted"`，不是总控举的 `== "trusted"`**——脏材料上
    // `taint(e)` 就是 `"untrusted"`，写 `== "trusted"` 守卫恒假、`do` 根本不执行，
    // **那样测出来的「没触发」是「分支没走到」，不是「J-08 拦住了」**。
    // 我第一版就是这么写的，拿到 `"没做"`——**一个因为错误的理由而通过的测试**。
    let e1 = 跑(r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(do("取外部", [], 0)), test("行吗","k")));
if taint(e) == "untrusted" { content(do("发出去", [], 0)) } else { "没做" }
"#)
    .expect_err("**读标签不是可信判断**");
    assert!(e1.contains("J-08"), "{e1}");

    // 形二：先绑成布尔再用——**来源不许跟着比较结果走**
    let e2 = 跑(r#"
budget {calls: 2, cost: 0, depth: 8};
let e = cut(judge(state(do("取外部", [], 0)), test("行吗","k")));
let t = taint(e) == "untrusted";
if t { content(do("发出去", [], 0)) } else { "没做" }
"#)
    .expect_err("绕一层布尔绑定也不行");
    assert!(e2.contains("J-08"), "{e2}");

    // 形三（正面）：真在可信材料上做一次判断，就该放行
    let ok = 跑(r#"
budget {calls: 5, cost: 0, depth: 8};
let 脏判 = handle(cut(judge(state(do("取外部", [], 0)), test("行吗","k"))), {
    act: fn(){true}, ignore: fn(){false}, unsure: fn(u){consume(u,"drop");false}});
let 净判 = handle(cut(judge(state(do("取内部", [], 0)), test("行吗","k"))), {
    act: fn(){true}, ignore: fn(){false}, unsure: fn(u){consume(u,"drop");false}});
if 脏判 && 净判 { content(do("发出去", [], 0)) } else { "没做" }
"#);
    assert_eq!(
        ok.as_deref(),
        Ok("\"发了\""),
        "**合取一个真的可信判断就该放行**：{ok:?}"
    );
}

/// **`12`:182「`gen`/`do`/`transform` 的输出默认入库」对 `transform` 的
/// `W-no-cache` 那条路是假的**：它在 `ledger.put` 之前就返回，那份材料**根本不进账本**
/// ——而账本是今天唯一存着效应输出内容的地方。
///
/// 本版不修（修法要么让它进账本、要么承认它是例外，两条都牵动重放语义），
/// **但把缺口钉住**：哪天有人以为「输出都在账本里」而据此建料库，这条会红。
#[test]
fn 指纹不出来的transform输出不进账本() {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::interp::ActionRegistry;
    use jpp::ledger::{Entry, Ledger};
    use jpp::run;
    use jpp::value::Answer;
    /// 判断恒给 0.9，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；无状态，故 'static）
    fn 桩端口() -> Ports<'static> {
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                })
            }))
            .with(FnPort::generate(
                "m",
                |_p, _c: &[serde_json::Value], _n, _r| Err(EffectError("x".into())),
            ))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
    }
    // **捕获一个读数**——注释写着指纹取不到的正是「捕获里有读数/出口/嵌套太深」
    let src = r#"
budget {calls: 2, cost: 0, depth: 8};
let r = judge(state(mat("被判的")), test("行吗", "k"));
let g = fn(m) { let _ = r; content(m) };
{a: content(transform(g, mat("材料")))}
"#;
    let program = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let out = run(
        &program,
        桩端口(),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("跑得完");
    let _ = &out;

    let 进了账本 = l
        .entries
        .iter()
        .any(|e| matches!(e, Entry::Effect { kind, .. } if kind == "transform"));
    if out
        .trace
        .warnings
        .iter()
        .any(|w| w.starts_with("W-no-cache"))
    {
        assert!(!进了账本, "W-no-cache 那条路确实不进账本——缺口钉住");
        let w = out
            .trace
            .warnings
            .iter()
            .find(|w| w.starts_with("W-no-cache"))
            .unwrap();
        assert!(
            w.contains("不进账本"),
            "**告警要说全**：不只是「不进跨运行缓存」：{w}"
        );
    } else {
        // **不许留成一条「没走到也算过」的测试**——那正是今晚拆了一整夜的形状。
        panic!(
            "没有触发 W-no-cache，这个程序证不了任何事：{:?}",
            out.trace.warnings
        );
    }
}
