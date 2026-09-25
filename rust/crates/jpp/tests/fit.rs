//! 第二包：`fit` 桥与「长处」那侧缺席的入口。
//!
//! 第一步：**`cut` 判序的第一步 `insufficient`** 与它依赖的 `Q.evidence`。
//!
//! `12`:148 原文：「顺序：先 `insufficient`（**该题声明的决定性证据槽不在状态里 → 不信任 p**，
//! J-09），再 `taint`…，再过线，再 `band`」。:244 J-09：「`insufficient` 在信任 p 之前检查：
//! 题声明『决定性证据槽』，状态缺该槽 → `Unsure(insufficient)`」。
//!
//! **它拦住的是什么**：模型对一道它没有证据可依的题照样会给出一个 p——而且常常是个自信的 p。
//! 没有这一步，那个 p 会照常过线、照常变成 `Act`。`insufficient` 是唯一一处**在看 p 之前**
//! 就把它挡住的检查。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 判断恒给 `p`，不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口；调用数此文件
/// 里没有测试用到，故不再计数）
fn 定值端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

/// 按状态顺序给不同的 p（判断向量的测试要每个对象各一个值）；不该生成、不该问人
/// （步 15c：原 `impl Client` 的桩改为三个闭包端口，游标搬到调用处的 `RefCell`）
fn 多态端口<'a>(ps: &'a [f64], 下一个: &'a RefCell<usize>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            let mut i = 下一个.borrow_mut();
            let answers = qs
                .iter()
                .map(|_| {
                    let p = *ps.get(*i).unwrap_or(&0.5);
                    *i += 1;
                    Answer::Noul(p)
                })
                .collect();
            Ok(JudgeResult {
                answers,
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

/// 给 choice 答案 + 可选的置换众数占比（`None` = 这条路上没测过置换）；不该生成、不该问人
/// （步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn choice端口(probs: Vec<f64>, mode_share: Option<f64>) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Choice(probs.clone())).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: qs.iter().map(|_| mode_share).collect(),
                perms: qs.iter().map(|_| 2).collect(),
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

fn 跑_choice(src: &str, probs: Vec<f64>, mode_share: Option<f64>) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let mut ledger = Ledger::new();
    run(
        &program,
        choice端口(probs, mode_share),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()))
}

fn 跑_多态(src: &str, ps: &[f64]) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    // B28：重复合并的结果用含 n 的独立键
    calib
        .put("k\u{1f}repeat(n=3)", 0.65, 0.35, 100, "上岗", Some(0.05))
        .unwrap();
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    run(
        &program,
        多态端口(ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()))
}

fn 跑(src: &str, p: f64) -> jpp::Outcome {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let mut ledger = Ledger::new();
    run(
        &program,
        定值端口(p),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()))
}

/// 题声明了决定性证据槽，状态里没有那个槽 → `Unsure(insufficient)`，**不看 p**。
#[test]
fn 缺决定性证据槽时不信任p() {
    // p = 0.95 远过上线：没有 insufficient 这一步，它会变成 Act
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let q = test("这份合同有没有违约条款？", "k", {evidence: ["ctx"]});
let e = cut(judge(state(mat("合同摘要")), q));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let out = 跑(src, 0.95);
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("unsure(insufficient:ctx)"),
        "题声明 ctx 是决定性证据，状态里没有 ctx：不该信任 p（12:148、J-09）。实际 {:?}",
        out.value_json()
    );
}

/// 证据槽在状态里 → 照常判，p 该过线就过线。
#[test]
fn 证据槽齐了就照常判() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let q = test("这份合同有没有违约条款？", "k", {evidence: ["ctx"]});
let e = cut(judge(state(mat("合同摘要"), {ctx: [mat("第七条：违约金为合同额 20%")]}), q));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    assert_eq!(
        跑(src, 0.95).value_json()["出口"],
        serde_json::json!("act"),
        "证据齐了就该照常过线"
    );
}

/// 没声明 evidence 的题不受影响——**别让新检查误伤旧程序**。
#[test]
fn 没声明证据槽的题照旧() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let e = cut(judge(state(mat("随便一段")), test("行吗", "k")));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    assert_eq!(跑(src, 0.95).value_json()["出口"], serde_json::json!("act"));
}

/// **判序**：`insufficient` 在 `taint` 与过线**之前**（`12`:148）。
/// 状态既 untrusted 又缺证据槽时，报的该是 `insufficient` 而不是别的。
#[test]
fn insufficient排在taint与过线之前() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let q = test("行吗", "k", {evidence: ["ctx"]});
let e = cut(judge(state(脏), q));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let mut actions = ActionRegistry::new();
    actions.register(
        "取外部数据",
        0.0,
        true,
        jpp::TaintOut::Untrusted,
        |_| Ok(jpp::value::Value::text("外面来的")),
    );
    let mut ledger = Ledger::new();
    let out = run(&program, 定值端口(0.95), &calib, &actions, &mut ledger).expect("跑完");
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("unsure(insufficient:ctx)"),
        "insufficient 先判：{:?}",
        out.value_json()
    );
}

// ---------------------------------------------------------------- 第二步：判断向量

/// **`order` 拦住的是什么**：把读数排序这件事，作者用 `map`+`sort` 也能做——但那样做出来的
/// 是**按 p 排的全序**，而 p 在 δ 之内的差别**不是真差别**（档案：δ 是同一读数重测的抖动）。
/// 全序会让「0.71 排在 0.70 前面」看起来像个结论，其实两者不可分。
///
/// `.order()` 给的是**偏序分档**：相邻差 ≤ δ 的并列成一档（`12`:134「同题同锚单独渲染的
/// 跨对象偏序（**相邻档并列**）」）。它拦住的正是「把抖动当成排名」。
#[test]
fn order给偏序分档而不是全序() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 状态们 = map(["甲", "乙", "丙", "丁"], fn(t) { state(mat(t)) });
let 读数们 = judge(状态们, test("行吗", "k"));
let 档位 = order(读数们);
{档位: 档位}
"#;
    // 四个对象的 p：0.90 / 0.88 / 0.50 / 0.48。δ(noul) 默认 0.05：
    // 0.90 与 0.88 差 0.02 ≤ δ → 同档；0.50 与 0.48 同理；两组之间差 0.38 → 分档
    let out = 跑_多态(src, &[0.90, 0.88, 0.50, 0.48]);
    assert_eq!(
        out.value_json()["档位"],
        serde_json::json!([[0, 1], [2, 3]]),
        "相邻差 ≤ δ 的该并列成一档，不是排成全序。实际 {:?}",
        out.value_json()
    );
}

/// 失败/停止的读数（J-12）单独一档排最后，不混进偏序里。
#[test]
fn order把失败的读数排最后() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 状态们 = [state(mat("甲")), state(fail("取不到")), state(mat("丙"))];
let 档位 = order(judge(状态们, test("行吗", "k")));
{档位: 档位}
"#;
    let out = 跑_多态(src, &[0.90, 0.0, 0.40]);
    let 档 = &out.value_json()["档位"];
    assert_eq!(
        档.as_array().map(|a| a.len()),
        Some(3),
        "两个正常读数各一档 + 失败的一档：{档:?}"
    );
    assert_eq!(
        档[2],
        serde_json::json!([1]),
        "失败的读数排最后一档：{档:?}"
    );
}

/// **`repeat`（原 `agg`，B28）拦住的是什么**：同一道题跨多次运行的读数，作者拿 `fold` 求平均也能算出个数——
/// 但那个数**不是读数**，进不了 `cut`、也不带校准键。`repeat` 合并之后**仍然是读数**，所以还能 `cut`；
/// B28 起只许均值 / 中位数、禁众数，合并结果过桥用含 n 的独立校准键，未通过重跑分歧检验时告警。
#[test]
fn repeat合并跨运行的读数且结果仍是读数() {
    let src = |f: &str| {
        format!(
            r#"
budget {{calls: 6, cost: 1, depth: 8}};
let m = mat("同一段");
let q = test("行吗", "k");
let 三次 = [judge(state(m), q), judge(state(m), q), judge(state(m), q)];
let 合并 = {f};
let e = cut(合并);
consume(e, "drop");
{{出口: exit_kind(e)}}
"#
        )
    };
    // 三次读数 0.80 / 0.90 / 0.70，均值 0.80 > hi 0.65 → act（键 k·repeat(n=3) 有记录）
    let out = 跑_多态(&src("repeat(三次)"), &[0.80, 0.90, 0.70]);
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("act"),
        "{:?}",
        out.value_json()
    );
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-repeat-persistent")),
        "{:?}",
        out.trace.warnings
    );
    // 中位数
    let out = 跑_多态(&src(r#"repeat(三次, "median")"#), &[0.80, 0.90, 0.70]);
    assert_eq!(out.value_json()["出口"], serde_json::json!("act"));
    // 旧名照常可用，给弃用提示
    let out = 跑_多态(&src("agg(三次)"), &[0.80, 0.90, 0.70]);
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-deprecated")),
        "{:?}",
        out.trace.warnings
    );
    // 众数禁止
    let program = lower(&parse(&src(r#"repeat(三次, "mode")"#)).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let ps = [0.8, 0.9, 0.7];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let e = run(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect_err("众数禁止");
    assert!(e.render().contains("众数"), "{}", e.render());
}

/// 合并结果**不借原键的线**：没有 `键·repeat(n)` 的记录就是冷（B28）。
#[test]
fn repeat结果不借原键的线() {
    let src = r#"
budget {calls: 6, cost: 1, depth: 8};
let m = mat("同一段");
let q = test("行吗", "k");
let e = cut(repeat([judge(state(m), q), judge(state(m), q)]));
let c = exit_kind(e);
consume(e, "drop");
c
"#;
    let out = 跑_多态(src, &[0.80, 0.90]);
    assert!(
        out.value_json().as_str().unwrap_or("").contains("cold"),
        "n=2 没有记录：{:?}",
        out.value_json()
    );
}

/// 判断向量上**只有这两种操作**（`12`:134「其余运算不存在（J-01）」）。
/// 读数仍然不可比、不可算——这条上一包已经钉过，这里确认新入口没有把它打开。
#[test]
fn 判断向量不开新口子() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 读数们 = judge([state(mat("甲")), state(mat("乙"))], test("行吗", "k"));
{和: sum(读数们)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let mut ledger = Ledger::new();
    let e = run(
        &program,
        定值端口(0.9),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect_err("读数不能求和");
    assert!(e.render().contains("J-01"), "该是 J-01：{}", e.render());
}

// ---------------------------------------------------------------- 第三步之一：J-12 的静态面

/// `12`:263 J-12 最后一句：「**程序边界不含 `⊎ Fail` 时必须 `on_fail` 处理**」，
/// 那一行的栏位写着**静态**。
///
/// 此前只做了运行期那半（`Reading.fail` → `Unsure(fail:…)`）。静态这半完全没有：
/// `do` 的 `Fail` 一路带到程序返回值、返回类型没提 `Fail`，检查器一条诊断都不报。
///
/// **它拦住的是什么**：一个 `Fail` **无声地成为程序的结果**。调用者拿到的是
/// `{"fail": "…"}` 这样一个记录，而**没有任何地方说过这是失败**——它长得像数据。
/// 与 J-05「未决必须被消费」是同一条纪律的两面：**未决会被拦，失败不会**。
#[test]
fn fail带到程序边界而没处理是错() {
    let src = r#"
budget {calls: 1, cost: 0, depth: 8};
let r = do("可能失败", [], 0);
{结果: r}
"#;
    let report = jpp::check(&lower(&parse(src).expect("解析")).expect("lower"));
    let d = report
        .find("J-12")
        .unwrap_or_else(|| panic!("Fail 带到程序边界而没处理该报 J-12：\n{}", report.render()));
    assert!(
        d.message.contains("is_fail") || d.message.contains("处理"),
        "报文要给修法：{}",
        d.message
    );
}

/// 查过就不报——`is_fail` 是处理它的入口。**别让新检查误伤会处理失败的程序。**
#[test]
fn 查过is_fail就不报() {
    for src in [
        r#"
budget {calls: 1, cost: 0, depth: 8};
let r = do("可能失败", [], 0);
{出错了: is_fail(r), 结果: if is_fail(r) { unit } else { content(r) }}
"#,
        r#"
budget {calls: 1, cost: 0, depth: 8};
let r = do("可能失败", [], 0);
{好了: 1}
"#,
    ] {
        let report = jpp::check(&lower(&parse(src).expect("解析")).expect("lower"));
        assert!(
            report.find("J-12").is_none(),
            "处理过或没带出去的不该报：\n{src}\n{}",
            report.render()
        );
    }
}

// ---------------------------------------------------------------- fit 桥本体（第七种形式）

/// `12` §6.0:315 `jv.fit(jv.fitref("名"), r[a], r[b])`，**输出仍要 `cut`**。
///
/// **它让什么活下来**（按「拦住什么**或**让什么活下来」这条判据）：**跨题的联合判断，
/// 在类型上仍然是读数**——因而仍要过线、仍可能是 unsure。
///
/// 作者不用 `fit` 也能合并两道题：`cut` 出两个出口再自己写 `if`。但那样一来，
/// **合并这一步的不确定性就消失了**——两个 `act` 合出来的结论看着和一个 `act` 一样确定，
/// 而它其实经过了一个没有校准过的函数。`fit` 的结果仍是读数，所以它**必须再过一次线**，
/// 而那条线是为这个 fit 单独校准的（J-16：训练集 ≠ 保形集）。
///
/// J-16 四条约束（`12`:274 原文）：指纹逐项相同、`n ≥ max(50, 20×特征数)`、
/// 训练集 ≠ 保形集、**输出必经 `cut`**。
#[test]
fn fit的结果仍是读数还要过线() {
    use jpp::effects::FitRegistry;

    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = judge(state(mat("代码")), test("测试全过吗", "t.全过"));
let 乙 = judge(state(mat("代码")), test("改动局部吗", "d.局部"));
let 合 = fit("绿且局部", [甲, 乙]);
let e = cut(合, "fit.绿且局部");
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib
        .put("t.全过", 0.6, 0.3, 60, "上岗", Some(0.05))
        .unwrap();
    calib
        .put("d.局部", 0.6, 0.3, 60, "上岗", Some(0.05))
        .unwrap();
    calib
        .put("fit.绿且局部", 0.6, 0.3, 80, "上岗", Some(0.05))
        .unwrap();
    calib.set_set_id("fit.绿且局部", "conf-B").unwrap();
    let mut fits = FitRegistry::new();
    // 两个特征：(校准键, 指纹种类)；训练集 train-A ≠ 保形集 conf-B
    fits.register(
        "绿且局部",
        &[("t.全过", "noul"), ("d.局部", "noul")],
        60,
        "train-A",
        |ps| ps[0] * ps[1],
    );
    let ps = [0.9, 0.9];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = jpp::run_with_fits(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &fits,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    // 0.9 × 0.9 = 0.81 > hi 0.6 → act。关键是它**过了线**，而不是直接给个分数
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("act"),
        "fit 的结果要经 cut 判出出口：{:?}",
        out.value_json()
    );
}

/// J-16：未注册的 `fit` 不认。
#[test]
fn 未注册的fit不认() {
    use jpp::effects::FitRegistry;
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = judge(state(mat("代码")), test("测试全过吗", "t.全过"));
let e = cut(fit("没注册过", [甲]), "fit.绿且局部");
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib
        .put("t.全过", 0.6, 0.3, 60, "上岗", Some(0.05))
        .unwrap();
    calib
        .put("fit.绿且局部", 0.6, 0.3, 80, "上岗", Some(0.05))
        .unwrap();
    let ps = [0.9];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let e = jpp::run_with_fits(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &FitRegistry::new(),
        &mut ledger,
    )
    .expect_err("未注册的 fit 该被拒");
    assert!(e.render().contains("J-16"), "该是 J-16：{}", e.render());
}

/// J-04：输入指纹必须与注册特征**逐项相同**。喂错题就是错。
#[test]
fn fit的输入指纹要与注册特征逐项相同() {
    use jpp::effects::FitRegistry;
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 别的题 = judge(state(mat("代码")), test("随便问问", "别的键"));
let 乙 = judge(state(mat("代码")), test("改动局部吗", "d.局部"));
let e = cut(fit("绿且局部", [别的题, 乙]), "fit.绿且局部");
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    for k in ["别的键", "d.局部", "fit.绿且局部"] {
        calib.put(k, 0.6, 0.3, 60, "上岗", Some(0.05)).unwrap();
    }
    let mut fits = FitRegistry::new();
    fits.register(
        "绿且局部",
        &[("t.全过", "noul"), ("d.局部", "noul")],
        60,
        "train-A",
        |ps| ps[0] * ps[1],
    );
    let ps = [0.9, 0.9];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let e = jpp::run_with_fits(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &fits,
        &mut ledger,
    )
    .expect_err("指纹不符该被拒");
    assert!(e.render().contains("J-04"), "该是 J-04：{}", e.render());
}

/// J-16：训练集 ≠ 保形集。同源就是「拿训练数据给自己打分」。
#[test]
fn 训练集不能与保形集同源() {
    use jpp::effects::FitRegistry;
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = judge(state(mat("代码")), test("测试全过吗", "t.全过"));
let 乙 = judge(state(mat("代码")), test("改动局部吗", "d.局部"));
let e = cut(fit("绿且局部", [甲, 乙]), "fit.绿且局部");
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    for k in ["t.全过", "d.局部", "fit.绿且局部"] {
        calib.put(k, 0.6, 0.3, 60, "上岗", Some(0.05)).unwrap();
    }
    // 保形集与训练集同一个 id
    calib.set_set_id("fit.绿且局部", "train-A").unwrap();
    let mut fits = FitRegistry::new();
    fits.register(
        "绿且局部",
        &[("t.全过", "noul"), ("d.局部", "noul")],
        60,
        "train-A",
        |ps| ps[0] * ps[1],
    );
    let ps = [0.9, 0.9];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let e = jpp::run_with_fits(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &fits,
        &mut ledger,
    )
    .expect_err("训练集与保形集同源该被拒");
    assert!(e.render().contains("J-16"), "该是 J-16：{}", e.render());
}

/// J-16：`n ≥ max(50, 20×特征数)`。样本不够就不该用它下结论。
#[test]
fn 注册样本数不够不认() {
    use jpp::effects::FitRegistry;
    let mut fits = FitRegistry::new();
    // 两个特征 → 至少 max(50, 40) = 50；给 40 不够
    let e = fits.register_checked(
        "绿且局部",
        &[("a", "noul"), ("b", "noul")],
        40,
        "train-A",
        |ps| ps[0] * ps[1],
    );
    assert!(e.is_err(), "40 < max(50, 20×2) 该被拒");
    assert!(
        fits.register_checked(
            "绿且局部",
            &[("a", "noul"), ("b", "noul")],
            50,
            "train-A",
            |ps| ps[0] * ps[1]
        )
        .is_ok()
    );
    // 三个特征 → 至少 60
    assert!(
        fits.register_checked(
            "三特征",
            &[("a", "noul"), ("b", "noul"), ("c", "noul")],
            50,
            "train-A",
            |ps| ps[0]
        )
        .is_err()
    );
}

/// J-04（`12`:255）：「**跨题**、跨候选集（`over` 指纹不同）、跨刻度或异锚（`ref` 指纹不同）
/// 的读数**不可比**」。
///
/// `order` 是排序，排序就是比——所以它也受这条管。`agg` 已经核了同题（上一步做的），
/// **`order` 当时没核**：实测两道不同的题排出了 `[[0], [1]]`，看着像一个结论。
///
/// 为什么跨题排序没有意义：两道题各有各的校准线，p 不在同一把尺子上。
/// 「问题一 0.9」和「问题二 0.5」谁更高，这个比较本身不成立。
#[test]
fn order不能跨题排序() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = judge(state(mat("x")), test("问题一", "k"));
let 乙 = judge(state(mat("x")), test("问题二", "k2"));
{r: order([甲, 乙])}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    for k in ["k", "k2"] {
        calib.put(k, 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    }
    let ps = [0.9, 0.5];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let e = run(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect_err("跨题排序该被拒");
    assert!(e.render().contains("J-04"), "该是 J-04：{}", e.render());
}

/// 同题跨对象照常排——那正是 `order` 的用途（`12`:134「同题**同锚**单独渲染的跨对象偏序」）。
#[test]
fn order同题跨对象照常排() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
{r: order(judge([state(mat("甲")), state(mat("乙"))], test("同一道题", "k")))}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let ps = [0.9, 0.5];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("同题该跑通");
    assert_eq!(out.value_json()["r"], serde_json::json!([[0], [1]]));
}

// ---------------------------------------------------------------- select 的 Pick：置换众数一致

/// `12`:151：「`select` 的 `Pick` **要求置换众数一致**（`profile.position_bias`）**且先验策略
/// 通过；不一致 → `Unsure(tie)`**。」
///
/// **它拦住的是什么**：置换不一致，意思是**换一下候选的排列顺序，模型就选了别的**。
/// 那时给 `Pick(k)` 是把一个**不稳定的选择**当成确定结论——`k` 看着像答案，其实是个位置效应。
///
/// **这条检查在真数据上几乎测不到**（三次更正后的准确说法）：E-CAL 正式版 97 条 `select` 里
/// 89 条因候选 176–372 token 落在档案 `k_limit` 未测档而被下沉成 K-noul，**K-noul 分支的
/// `mode_share` 是写死的 1.0**（`runtime.py:1079`），那 89 条是恒真项不是测量。
///
/// **余下 8 条走原生 choice，是真测量**：各跑正序与逆序两个置换（`runtime.py:949`），
/// 两次都选同一项，`mode_share = 2/2 = 1.0`。我先前报过「真测量是 0」，**那是错的**——
/// 我只看了「97 条的值全是 1.0」，没去看那 8 条的 1.0 是**怎么来的**。
/// 恒真的 1.0 与测出来的 1.0 数值相同，**来源不同**。
///
/// n=8 功效极低：若真实一致率是判据里的 0.75，8/8 出现的概率是 0.10。
#[test]
fn 置换不一致时给unsure_tie而不是pick() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let s = state(mat("对象"), {over: [mat("甲"), mat("乙"), mat("丙")]});
let e = cut(judge(s, select("挑一个", "k")));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    // 置换一致（mode_share = 1.0）：正常给 Pick
    let out = 跑_choice(src, vec![0.8, 0.1, 0.1], Some(1.0));
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("pick(0)"),
        "置换一致该给 Pick：{:?}",
        out.value_json()
    );

    // 置换不一致（mode_share = 0.67，正是档案 position_bias 里 code_K16 的实测值）：
    // 换个排列模型就选别的，这时的 k 是位置效应不是答案
    let out = 跑_choice(src, vec![0.8, 0.1, 0.1], Some(0.67));
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("unsure(tie)"),
        "置换众数不一致该给 Unsure(tie)（12:151），而不是把不稳定的选择当结论：{:?}",
        out.value_json()
    );
}

/// **K-noul 路径上不该给 `Pick`**。
///
/// `12` 对这一格没写，所以这是 core 的判断，写明理由：K-noul 是把一道 select 拆成 K 道
/// 独立的 noul 再取 argmax——**那条路上根本没有「置换」这回事**（每道 noul 各问各的，
/// 候选顺序不参与）。既然置换一致性**测不到**，就不能声称它通过；
/// 而 `12`:151 要求 `Pick` 必须置换一致，所以这条路上只能给 `Unsure(tie)`。
///
/// 反面说：给 `Pick` 就是**拿一个从未做过的检查当成通过了**——正是「恒真的检查」那一类。
#[test]
fn k_noul路径上不给pick() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let s = state(mat("对象"), {over: [mat("甲"), mat("乙"), mat("丙")]});
let e = cut(judge(s, select("挑一个", "k")));
consume(e, "drop");
{出口: exit_kind(e)}
"#;
    // mode_share 缺失 = 这条路上没测过置换（K-noul 就是这个情形）
    let out = 跑_choice(src, vec![0.8, 0.1, 0.1], None);
    // **不是 `tie`**（总控裁定二号）：`tie` 的语义是「测了，不一致」，这里是「没测」。
    // 两者路由不同，而且在**账本**里不能是同一个值——账本是审计物。
    assert_eq!(
        out.value_json()["出口"],
        serde_json::json!("unsure(untested:permutation)"),
        "置换一致性没测过就不能声称通过，而且不能说成 tie：{:?}",
        out.value_json()
    );
}
/// **`fit` 的结果不得借走模式级先验。**
///
/// `cut(fit结果)` 不带第二参时 `key = "fit:{名}"`，而那条记录几乎从不上岗
/// （fit 的校准住在 `error_rate` 里，不是一条线）。模式级回退接上之后，它会掉到
/// `mode_key("noul", …)` 上——**fit 的可靠性与「裸 noul 判断这一类的可靠性」毫无关系**，
/// 这是跨种借线，正是模式键按 `phys` 分格要避免的那件事从另一道门进来。
///
/// 实测过：修之前这段程序返回 `{"出口": "act", "线源": "模式级"}`。
/// 它同时满足插队门槛的两条——**会算错**，且**不需要特殊条件就走得到**。
#[test]
fn fit结果不借模式级先验() {
    use jpp::effects::{CalibStore, FitRegistry, LiteralMode};
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = judge(state(mat("代码")), test("测试全过吗", "t.全过"));
let 乙 = judge(state(mat("代码")), test("改动局部吗", "d.局部"));
let 合 = fit("绿且局部", [甲, 乙]);
let e = cut(合);
consume(e, "drop");
{出口: exit_kind(e), 线源: line_source(e)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib
        .put("t.全过", 0.6, 0.3, 60, "上岗", Some(0.05))
        .unwrap();
    calib
        .put("d.局部", 0.6, 0.3, 60, "上岗", Some(0.05))
        .unwrap();
    // **没有** fit:绿且局部 的记录；只有一条通用 noul 模式级先验
    calib
        .put(
            &CalibStore::mode_key("noul", LiteralMode::default()),
            0.80,
            0.20,
            200,
            "上岗",
            Some(0.05),
        )
        .unwrap();
    let mut fits = FitRegistry::new();
    fits.register(
        "绿且局部",
        &[("t.全过", "noul"), ("d.局部", "noul")],
        60,
        "train-A",
        |ps| ps[0] * ps[1],
    );
    let ps = [0.99, 0.99];
    let 下一个 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = jpp::run_with_fits(
        &program,
        多态端口(&ps, &下一个),
        &calib,
        &ActionRegistry::new(),
        &fits,
        &mut ledger,
    )
    .unwrap();
    let v = out.value_json();
    assert_eq!(
        v["出口"],
        serde_json::json!("unsure(cold|untested:calib_line)"),
        "**fit 的结果不该借裸 noul 那一类的线**：{v}"
    );
    assert_eq!(v["线源"], serde_json::json!(""), "没借到线就不该留来源");
}
