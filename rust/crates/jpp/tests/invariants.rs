//! 内核不变量：每条都是「说得出什么情况下什么东西会变红」的规则，不是 taste。
//!
//! 来源是第二轮盘点点名的几条：J-01 的容器逃逸、账本只增、未决跨恢复、`filter` 的谓词契约。

use std::cell::RefCell;

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::Answer;
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};

/// 判断给定值 p、生成造 n 个占位材料、问人不该被调（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口<'a>(p: f64, calls: &'a RefCell<u64>) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            *calls.borrow_mut() += 1;
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                perms: vec![],
                mode_share: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", move |_p, _c, n, _r| {
            *calls.borrow_mut() += 1;
            Ok(GenResult {
                outputs: (0..n).map(|i| serde_json::json!({"第": i})).collect(),
                tokens: 0,
                cost: 0.0,
                ..Default::default()
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

fn 跑(src: &str, p: f64, ledger: &mut Ledger) -> Result<jpp::Outcome, jpp::Error> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    run(
        &program,
        定值端口(p, &calls),
        &calib,
        &ActionRegistry::new(),
        ledger,
    )
}

// ---------------------------------------------------------------- J-01 不能靠容器绕过

/// J-01：读数不可比。但**包进列表或记录再比，就绕过了检查**——`Value::equals` 的容器分支
/// 逐元素比时没有把「不可比」这个 `None` 传上来，于是 `[r1] == [r2]` 得到一个 bool。
///
/// 读数没有可读的值，装进容器也还是没有。这是「会算错」级别，不是漏报。
#[test]
fn 读数装进容器也不可比() {
    let 列表 = r#"
budget {calls: 2, cost: 1, depth: 8};
let r1 = judge(state(mat("甲")), test("行吗", "k"));
let r2 = judge(state(mat("乙")), test("行吗", "k"));
let 同不同 = [r1] == [r2];
let e1 = cut(r1); let e2 = cut(r2);
consume([e1, e2], "drop");
{同不同: 同不同}
"#;
    let e = 跑(列表, 0.9, &mut Ledger::new()).expect_err("读数装进列表也不该可比");
    assert!(e.render().contains("J-01"), "该是 J-01：{}", e.render());

    let 记录 = r#"
budget {calls: 2, cost: 1, depth: 8};
let r1 = judge(state(mat("甲")), test("行吗", "k"));
let r2 = judge(state(mat("乙")), test("行吗", "k"));
let 同不同 = {读数: r1} == {读数: r2};
let e1 = cut(r1); let e2 = cut(r2);
consume([e1, e2], "drop");
{同不同: 同不同}
"#;
    let e = 跑(记录, 0.9, &mut Ledger::new()).expect_err("读数装进记录字段也不该可比");
    assert!(e.render().contains("J-01"), "该是 J-01：{}", e.render());
}

// ---------------------------------------------------------------- 账本是审计物不是缓存

/// `Ledger::put` 遇同键直接 return，不覆盖——「**账本是审计物不是缓存**」这个不变量全靠它，
/// 却一直没有一条断言对着它。同键二次写入必须**不覆盖**，否则重放会读到被改写过的历史。
#[test]
fn 账本只增不覆盖() {
    let mut l = Ledger::new();
    l.put(Entry::judge("K", Answer::Noul(0.9), 1, 0.5, "m1", 0));
    l.put(Entry::judge("K", Answer::Noul(0.1), 99, 9.9, "m2", 0));

    assert_eq!(l.entries.len(), 1, "同键只该有一条");
    let Some(Entry::Judge {
        answer,
        tokens,
        cost,
        model_id,
        ..
    }) = l.get("K")
    else {
        panic!("取得到这一条")
    };
    assert_eq!(
        *answer,
        Answer::Noul(0.9),
        "留下的必须是**先写的那条**，不是后写的"
    );
    assert_eq!(
        (*tokens, *cost, model_id.as_str()),
        (1, 0.5, "m1"),
        "费用与模型也不许被改写"
    );
}

// ---------------------------------------------------------------- 未决跨恢复

/// `13` §3 验收第三句：「**暂停恢复仍能找到它**」。这条一直没验过——我在 `COORDINATION.md`
/// 里写过「§3 四条验收里三条绿」，实际 §3 只有三句验收，我测了两句，**把没测的这句算成了绿**。
/// 一个假绿比一条已知的红更坏，所以这条测试连同那处订正一起留下。
#[test]
fn 未决责任跨恢复仍找得到() {
    // 预算只够一次调用：第二次 judge 停发（步 22-0 起不挂起，出口 Unsure(budget) 随返回值转交），
    // 此时第一条未决已经产生
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
let r1 = judge(state(mat("甲")), test("行吗", "k"));
let e1 = cut(r1);
let 第一份 = handle(e1, {act: fn() { "act" }, ignore: fn() { "ignore" },
                       unsure: fn(u) { consume(u, "drop"); "unsure" }});
let r2 = judge(state(mat("乙")), test("行吗", "k"));
let e2 = cut(r2);
{甲: 第一份, 乙: exit_kind(e2), 待: e2}
"#;
    let mut ledger = Ledger::new();
    // p = 0.5 落在带内 → unsure
    let 第一次 = 跑(src, 0.5, &mut ledger).expect("超预算是停发不是错");
    assert_eq!(
        第一次.value_json()["乙"],
        serde_json::json!("unsure(budget)"),
        "第二次 judge 预算停发"
    );
    assert!(!ledger.entries.is_empty(), "挂起前已完成的调用要留在账本里");

    // 带着账本恢复：已完成的那次不重复付费，程序能继续跑完
    ledger.rebuild_index();
    let mut 更大预算 = src.replace("budget {calls: 1,", "budget {calls: 4,");
    更大预算 = 更大预算.to_string();
    let 第二次 = 跑(&更大预算, 0.5, &mut ledger).expect("恢复后应当跑完");
    assert_eq!(第二次.value_json()["甲"], serde_json::json!("unsure"));
    assert_eq!(第二次.value_json()["乙"], serde_json::json!("unsure(band)"));
    assert_eq!(第二次.cost.replayed, 1, "第一次那条从账本命中，不重复付费");
}

// ---------------------------------------------------------------- filter 的谓词契约

/// `filter` 的谓词返回非 `Bool` 时现在被**静默当假**。出口、未决责任传进去会无声消失——
/// 正是 `13` §3 要堵的那类。定契约：谓词必须返回 `Bool`，否则报错。
#[test]
fn filter的谓词必须返回bool() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let r = judge(state(mat("甲")), test("行吗", "k"));
let e = cut(r);
let 剩下 = filter([1, 2, 3], fn(x) { e });
consume(e, "drop");
{剩下: len(剩下)}
"#;
    let err = 跑(src, 0.9, &mut Ledger::new()).expect_err("谓词返回出口不该被静默当假");
    let t = err.render();
    assert!(t.contains("filter"), "报文要说清是 filter 的谓词：{t}");
    assert!(
        t.contains("Bool") || t.contains("真假"),
        "报文要说清要的是什么：{t}"
    );
}

/// `13` §3 明列的合法去向之一：「随返回值/**继续方法**交给调用者」。
///
/// 以前表达不出来：类型侧已用 `captures_responsibility` 认了方法能捕获责任，
/// 而 `collect_exit_ids` 不进函数捕获环境，于是把未决装进续接方法返回给调用者
/// **会被判成「责任丢了」**。这不是缺口，是内核两半打架导致的假拒绝。
#[test]
fn 未决可以装进续接方法交给调用者() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let r = judge(state(mat("甲")), test("行吗", "k"));
let 包 = handle(cut(r), {
    act: fn() { {状态: "act", 续: unit} },
    ignore: fn() { {状态: "ignore", 续: unit} },
    unsure: fn(u) { {状态: "unsure", 续: fn() { u }} }});
if 包.状态 == "unsure" { consume(包.续(), "drop") } else { unit };
{状态: 包.状态}
"#;
    // p = 0.5 落在带内 → unsure；责任被装进续接方法返回，调用者调用它再消费
    let out = 跑(src, 0.5, &mut Ledger::new()).unwrap_or_else(|e| {
        panic!(
            "把未决装进续接方法是合法去向（13 §3），不该被判成责任丢了：{}",
            e.render()
        )
    });
    assert_eq!(out.value_json(), serde_json::json!({"状态": "unsure"}));
}

// ---------------------------------------------------------------- 同一形状的缝：静默兜底

/// J-01 那条的真因是 `equals` 的 `None`（「不可比」）被 `unwrap_or(false)` 压成「不相等」——
/// **三值被静默打成两值**，正是 `00-宪法.md` 第 30 行那条借用登记（SQL 三值 WHERE，反面）
/// 要防的事。总控提示「同一个形状的缝可能不止一条」，全仓 grep 之后确认**还有一条，而且更糟**：
///
/// `json_to_effect_value` 反序列化材料时，`taint` 字段解析失败一律兜底成 `Trusted`
/// （`serde_json::from_value(...).unwrap_or(Taint::Trusted)`）。于是一份 **untrusted 材料
/// 经账本往返回来会变成 trusted**——这不是漏报，是**洗白**，比 J-01 那条更重：
/// J-01 丢的是一个比较结果，这条丢的是来源可信度，而 `12` §2.11 的 taint 代数整个建在它上面。
///
/// 兜底要往**保守**那边倒：解析不出来就当 `Untrusted`，不是当 `Trusted`。
///
/// 账本 v3（步 18a）起材料的 taint 在类型化的 `output_mat.taint` 里，必填：坏值或缺失时整份账本
/// 解码拒绝（`E-ledger-corrupt`），不再兜底——比「兜底为 Untrusted」更严。失败值的 taint 仍在 `output`
/// 的 JSON 里，保留兜底为 Untrusted。
#[test]
fn taint解析不出来要往保守那边兜底() {
    use jpp::interp::{effect_value_to_entry, entry_to_effect_value};
    use jpp::ledger::{Entry, Ledger};
    use jpp::value::{Mat, Taint, Value};
    use serde_json::json;

    // 正常往返：untrusted 还是 untrusted
    let m = Value::Mat(std::rc::Rc::new(Mat::new(
        json!({"x": 1}),
        "a",
        vec!["lit".into()],
        Taint::Untrusted,
        Default::default(),
    )));
    let (out, meta) = effect_value_to_entry(&m);
    let Value::Mat(m2) = entry_to_effect_value(&out, meta.as_ref()) else {
        panic!("该是材料")
    };
    assert_eq!(m2.taint, Taint::Untrusted, "正常往返不该变");

    // 账本里 output_mat.taint 坏了或缺了：解码拒绝，不洗白
    let mut l = Ledger::new();
    l.put(Entry::Effect {
        key: "k".into(),
        ekey: None,
        kind: "do".into(),
        output: out.clone(),
        output_mat: meta.clone().map(Box::new),
        cost: 0.0,
    });
    let text = l.encode();
    assert!(text.contains("\"taint\":\"Untrusted\""), "{text}");
    for 坏 in [
        "\"taint\":\"不是合法的 taint\"",
        "\"taint\":42",
        "\"taint\":null",
    ] {
        let bad = text.replace("\"taint\":\"Untrusted\"", 坏);
        let e = Ledger::decode(&bad).expect_err("坏 taint 要拒绝");
        assert!(e.contains("E-ledger-corrupt"), "{e}");
    }
    let 缺 = text.replace(",\"taint\":\"Untrusted\"", "");
    assert_ne!(缺, text, "要真的去掉了 taint");
    assert!(Ledger::decode(&缺).is_err(), "taint 缺失也要拒绝");

    // 失败值：taint 坏了或缺了仍兜底为 Untrusted
    for 坏 in [json!("不是合法的 taint"), json!(42), json!(null)] {
        let v = entry_to_effect_value(&json!({"__fail": "x", "taint": 坏}), None);
        let Value::Fail(_, p) = v else {
            panic!("该是失败值")
        };
        assert_eq!(p.taint, Taint::Untrusted, "失败值 taint 坏值 {坏}");
    }
}

/// **`content(m)` 拆包 + `mat(...)` 重包 = 洗白来源链**，与 `taint` 兜底那条是同一形状。
///
/// `content` 把材料拆成裸值（taint 与 origin 都留在壳上），`mat` 再包回去时给的是
/// `Mat::literal(...)` —— **`Taint::Trusted`、`origin = ["literal"]`**。于是三行源码就能把
/// 一份 untrusted 材料变成 trusted：
///
/// ```text
/// let 脏 = do("取外部数据", [], 0);     // untrusted
/// let 干净 = mat(content(脏));          // ← 这里洗白了
/// ```
///
/// 这不是漏报，是**语言里现成的一条洗白路径**。而 `00-宪法.md` 的 IFC 那一行唯一那条纪律
/// 「不可信材料上的判断不得单独放行不可逆 `do`」整个建在 taint 上——被洗白之后它不会报错，
/// 它只是失效。
///
/// 按已立的判据处置：`mat()` 收到一个**从材料拆出来的值**时，不能默认它可信。
#[test]
fn 拆包重包不能洗白来源链() {
    use jpp::value::Taint;

    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 干净 = mat(content(脏));
{脏: 脏.taint, 干净: 干净.taint}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let mut actions = ActionRegistry::new();
    // 登记一个产出 untrusted 的动作
    actions.register(
        "取外部数据",
        0.0,
        true,
        jpp::TaintOut::Untrusted,
        |_| Ok(jpp::value::Value::text("外面来的")),
    );
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &actions,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    let v = out.value_json();
    assert_eq!(
        v["脏"],
        serde_json::json!("untrusted"),
        "do 的输出登记成 Untrusted"
    );
    assert_eq!(
        v["干净"],
        serde_json::json!("untrusted"),
        "拆包重包不该洗白：untrusted 的内容包回材料还是 untrusted。实际 {v:?}"
    );
    let _ = Taint::Trusted;
}

/// 上一条的反面：**不能因为堵洗白就把无辜的字面量也打成 untrusted**。
/// 假拒绝同样是错——判断标准是「这份内容是不是从 untrusted 材料里拆出来的」，
/// 不是「有没有经过 mat()」。
#[test]
fn 堵洗白不能误伤无辜的字面量() {
    let src = r#"
budget {calls: 1, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 拆了一下 = content(脏);
let 无辜 = mat("自己写的字面量");
let 也无辜 = mat({来自: "源码"});
{脏: 脏.taint, 无辜: 无辜.taint, 也无辜: 也无辜.taint}
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
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &actions,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    let v = out.value_json();
    assert_eq!(v["脏"], serde_json::json!("untrusted"));
    assert_eq!(
        v["无辜"],
        serde_json::json!("trusted"),
        "源码里的字面量还是可信的：{v:?}"
    );
    assert_eq!(
        v["也无辜"],
        serde_json::json!("trusted"),
        "记录字面量同样：{v:?}"
    );
}

// ---------------------------------------------------------------- 账本键要带调用点

/// `judge_key` 缺 `site`：**同状态同题的两个不同站点会撞键**，于是第二个站点
/// 从账本里命中第一个站点的答案——不是漏记，是**命中一条本不该命中的记录**。
///
/// Python 侧的键带 site（`foundation/jv/store.py:26`：
/// `H("judge", model_id, state_hash, q_hash, phys, render_version, perm_seed, run_seq, site)`，
/// `site` 是「第一个不在 jv 包内的栈帧，文件名:行号，同程序重放时稳定」）。Rust 有 `Span`，
/// 干的是同一件事。
///
/// **为什么两个站点该分开**：同一个程序里问同一道题两次，是两次判断——它们可能夹着
/// 不同的上下文、各自的出口要分别消费。合成一次，第二次就不再是一次观察，而是复制第一次。
#[test]
fn 同状态同题的不同站点不撞键() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let m = mat("同一段文字");
let q = test("行吗", "k");
let r1 = judge(state(m), q);
let e1 = cut(r1);
let r2 = judge(state(m), q);
let e2 = cut(r2);
consume([e1, e2], "drop");
{甲: exit_kind(e1), 乙: exit_kind(e2)}
"#;
    let mut ledger = Ledger::new();
    let out = 跑(src, 0.9, &mut ledger).unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));
    assert_eq!(
        out.value_json(),
        serde_json::json!({"甲": "act", "乙": "act"})
    );

    // 两个站点 = 两条账本记录。撞键的话只会有一条，而第二次会记成「重放命中」
    let judges = ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .count();
    assert_eq!(
        judges, 2,
        "两个不同站点问同一道题，该记两条账；撞键会让第二条并进第一条。实际 {judges} 条"
    );
    assert_eq!(
        out.cost.replayed, 0,
        "第二个站点不该从账本里命中第一个站点的答案；实际重放了 {} 次",
        out.cost.replayed
    );
}

/// `gen` 的实际费用**不进预算也不进账本**：`Client::generate` 的返回值里根本没有费用这一项，
/// `interp` 把账本条目的 `cost` 写死成 `0.0`。于是 `budget.cost` 对 gen 整条路**失效**——
/// 一个只 gen 不 judge 的程序，花多少钱都不会被预算拦住。
///
/// 与 `judge` 对照：那条路上 `JudgeResult` 带 `cost`，进 `cost.usd`、进账本、参与 `charge`。
#[test]
fn gen的费用要进预算和账本() {
    // 步 15c：原 `impl Client` 的收费生成桩改为三个闭包端口（judge/ask 不该被调，直接报错）
    let 每次 = 0.004;
    let 次数 = RefCell::new(0u64);

    let src = r#"
budget {calls: 5, cost: 0.01, depth: 8};
let 出品 = gen("造两个", [], 2, 0);
{个数: len(出品)}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        Ports::new()
            .with(FnPort::judge("fixed-0", |_s, _qs| {
                Err(EffectError("这条程序不该 judge".into()))
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, n, _r| {
                *次数.borrow_mut() += 1;
                Ok(GenResult {
                    outputs: (0..n).map(|i| serde_json::json!({"第": i})).collect(),
                    tokens: 30,
                    cost: 每次,
                    ..Default::default()
                })
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| {
                Err(EffectError("不该 ask".into()))
            })),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(out.value_json(), serde_json::json!({"个数": 2}));
    assert!(
        (out.cost.usd - 0.004).abs() < 1e-9,
        "gen 的实际费用要进 cost.usd，实际 {}",
        out.cost.usd
    );
    assert_eq!(out.cost.tokens, 30, "token 也要记");
    let Some(Entry::Effect { cost, .. }) = ledger
        .entries
        .iter()
        .find(|e| matches!(e, Entry::Effect { kind, .. } if kind == "gen"))
    else {
        panic!("账本里该有 gen 这一条")
    };
    assert!(
        (cost - 0.004).abs() < 1e-9,
        "账本条目也要带实际费用，实际 {cost}"
    );
}

/// 改键之后，**旧账本必须是 miss 而不是撞错**。
///
/// 加 `site` 进键会让已有账本全部失效，这是预期的（键变了就是新键）。要确认的是失效的**方式**：
/// 用旧键写的记录，新键查过去应当**查不到**（于是重新发一次调用），
/// 而不是碰巧命中另一条记录的答案。**如果旧账本被新键「命中」了，说明键还不够分。**
#[test]
fn 改键之后旧账本是miss不是撞错() {
    use jpp::ledger::judge_key;

    // 同一状态同一题，两个不同站点
    let 甲 = judge_key("m", "状态哈希", "题哈希", "noul", 0, 0, 100);
    let 乙 = judge_key("m", "状态哈希", "题哈希", "noul", 0, 0, 200);
    assert_ne!(甲, 乙, "两个站点必须给出不同的键——这就是撞键那条修的东西");

    // 用旧口径（无 site）写的账本：新键查不到它，而不是命中它
    let 旧 = jpp::value::hash_of(&[
        "judge",
        "m",
        "状态哈希",
        "题哈希",
        "noul",
        jpp::ledger::RENDER_VERSION,
        "0",
        "0",
    ]);
    let mut l = Ledger::new();
    l.put(Entry::judge(旧.clone(), Answer::Noul(0.9), 0, 0.0, "m", 0));
    assert!(
        l.get(&甲).is_none(),
        "新键不该命中旧口径写下的记录——失效方式必须是 miss"
    );
    assert!(l.get(&乙).is_none());
    assert_ne!(旧, 甲, "新旧键本来就该不同");

    // perm_seed / run_seq 也要真的进键（它们与 site 是同一处的三个症状）
    assert_ne!(
        甲,
        judge_key("m", "状态哈希", "题哈希", "noul", 1, 0, 100),
        "perm_seed 要进键"
    );
    assert_ne!(
        甲,
        judge_key("m", "状态哈希", "题哈希", "noul", 0, 1, 100),
        "run_seq 要进键"
    );
}

/// `do` 的入参 taint。**直接放在参数表里的脏材料本来就认得**（`"do"` 那一臂先把列表拆开，
/// 元素以顶层 `Value::Mat` 的身份进 `do_`），这一半是绿的。
///
/// **但嵌一层就不认了**：`do("动作", [{料: 脏}], 0)` 的元素是**记录**，落进
/// `_ => Taint::Trusted`。于是 `TaintOut::Inherit` 的动作拿到「入参全可信」的结论——
/// 与前三条同一形状：在说不准的地方替不可信说了「可信」。
#[test]
fn do的入参taint要递归看嵌套容器() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 直接 = do("原样回传", [脏], 0);
let 嵌一层 = do("原样回传", [{料: 脏}], 0);
let 藏在列表里 = do("原样回传", [[脏]], 0);
{脏: 脏.taint, 直接: 直接.taint, 嵌一层: 嵌一层.taint, 藏在列表里: 藏在列表里.taint}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let mut actions = ActionRegistry::new();
    actions.register(
        "取外部数据",
        0.0,
        true,
        jpp::TaintOut::Untrusted,
        |_| Ok(jpp::value::Value::text("外面来的")),
    );
    // Inherit：输出的可信度跟着入参走
    actions.register("原样回传", 0.0, true, jpp::TaintOut::Inherit, |args| {
        Ok(args[0].clone())
    });
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &actions,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    let v = out.value_json();
    assert_eq!(v["脏"], serde_json::json!("untrusted"));
    assert_eq!(
        v["直接"],
        serde_json::json!("untrusted"),
        "直接放参数表里的脏材料本来就认得：{v:?}"
    );
    assert_eq!(
        v["嵌一层"],
        serde_json::json!("untrusted"),
        "记录字段里的脏材料也要认：{v:?}"
    );
    assert_eq!(
        v["藏在列表里"],
        serde_json::json!("untrusted"),
        "嵌套列表里的脏材料也要认：{v:?}"
    );
}

/// **第五个口子，自己按判据走出来的**（不是别人点名的）：`cut` 把状态 taint 扔了。
///
/// `12`:150 原文「出口是 `Mat` 的子类型…**出口 taint 继承状态 taint**」，§2.11 的 taint 代数
/// 同样写「cut 继承」。而 `interp.rs::cut` 里是 `let taint = Taint::Trusted;`——
/// **一份不可信材料上的判断，切出来的出口是可信的**。
///
/// `State::new` 明明已经把 `on/ctx/ref/over` 的 taint 折算好存在 `State.taint` 里，
/// 是 `cut` 没有去读它。所以这不是「没实现」，是**算好了又丢掉**。
///
/// 后果与前四条同族但更直接：宪法 IFC 那行唯一那条纪律「不可信材料上的判断不得单独放行
/// 不可逆 `do`」，判断的结论（出口）根本不带不可信这个标记，那条纪律就没有抓手。
#[test]
fn cut的出口要继承状态的taint() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let 脏 = do("取外部数据", [], 0);
let 出口 = cut(judge(state(脏), test("行吗", "k")));
let 材料化 = mat(出口);
consume(出口, "drop");
{出口taint: 材料化.taint}
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
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &actions,
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(
        out.value_json()["出口taint"],
        serde_json::json!("untrusted"),
        "不可信材料上的判断，切出来的出口也不可信（12:150「出口 taint 继承状态 taint」）：{:?}",
        out.value_json()
    );
}

/// 反面：可信材料上的判断，出口仍是可信的——别为了堵洗白把一切都打成 untrusted。
#[test]
fn 可信材料的出口仍然可信() {
    let src = r#"
budget {calls: 2, cost: 1, depth: 8};
let 出口 = cut(judge(state(mat("源码里的字面量")), test("行吗", "k")));
let 材料化 = mat(出口);
consume(出口, "drop");
{出口taint: 材料化.taint}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("跑完");
    assert_eq!(
        out.value_json()["出口taint"],
        serde_json::json!("trusted"),
        "{:?}",
        out.value_json()
    );
}

/// 四个效应键的成分要与 `12` 逐项对上。**同一处不对称最容易只修一半**——
/// `judge_key` 补了 `site` 之后，`gen`（`12`:158）与 `transform`（`12`:189）还缺着。
///
/// 依据原文（自核）：
/// - `gen`       `12`:158「键：`(site, prompt_hash, ctx_hash, n, retry_seq)`」
/// - `do`        `12`:169「键：`(site, action, args_hash, iter_seq)`」
/// - `transform` `12`:189「键 `(site, f_hash, args_hash)`」
/// - `ask`       `12`:177「键 `(state_hash, q_hash)`」——**故意没有 site**，
///   同一节写着「有预算与**去重（同键只问一次）**」：问人很贵，两个站点问同一个人同一道题
///   就该复用那一个答案。所以这里不是漏，是**依据要求的**。
#[test]
fn 四个效应键的成分与依据对得上() {
    let 跑 = |src: &str| {
        let program = lower(&parse(src).expect("解析")).expect("lower");
        let calib = CalibStore::new();
        let mut actions = ActionRegistry::new();
        actions.register("记一笔", 0.0, true, jpp::TaintOut::Trusted, |_| {
            Ok(jpp::value::Value::Int(1, jpp::value::Taint::Trusted.into()))
        });
        let calls = RefCell::new(0);
        let mut ledger = Ledger::new();
        let out = run(
            &program,
            定值端口(0.9, &calls),
            &calib,
            &actions,
            &mut ledger,
        )
        .unwrap_or_else(|e| panic!("跑完：{}", e.render()));
        let n = ledger.entries.len();
        (out, n)
    };

    // gen：同 prompt / ctx / n / retry_seq，两个站点 → 两条账
    let (_, n) = 跑(r#"
budget {calls: 4, cost: 1, depth: 8};
let 甲 = gen("同一句话", [], 1, 0);
let 乙 = gen("同一句话", [], 1, 0);
{甲: len(甲), 乙: len(乙)}
"#);
    assert_eq!(
        n, 2,
        "gen 的键要带 site（12:158）：两个站点同一段 prompt 该记两条，实际 {n} 条"
    );

    // transform：同一个方法、同一份材料，两个站点 → 两条账
    let (_, n) = 跑(r#"
budget {calls: 2, cost: 1, depth: 8};
let m = mat({x: 1});
let 甲 = transform(fn(old) { with(content(old), "y", 2) }, m);
let 乙 = transform(fn(old) { with(content(old), "y", 2) }, m);
{甲: content(甲).y, 乙: content(乙).y}
"#);
    assert_eq!(n, 2, "transform 的键要带 site（12:189）：实际 {n} 条");

    // do：本来就对，留作回归
    let (_, n) = 跑(r#"
budget {calls: 2, cost: 1, depth: 8};
let 甲 = do("记一笔", [], 0);
let 乙 = do("记一笔", [], 0);
{甲: content(甲), 乙: content(乙)}
"#);
    assert_eq!(n, 2, "do 的键本来就带 site（12:169）：实际 {n} 条");
}

/// `budget.escalate` 是**问人的总次数上限**，不是「每次运行 k 次」。
///
/// `12`:177「`ask` 有预算（`budget.escalate`）」，:180「**恢复 = 从头重跑**，账本重放使此前所有
/// `judge`/`do`/`gen` 不付费…`ask` 的答案到达后作为账本条目参与重放」。所以一次挂起-恢复里，
/// 账本里那些**已经问过的** ask 必须计入上限——否则 `cost.asks` 每次运行都从 0 起，
/// 于是「上限 2」在三轮恢复里能问到 6 次人。**问人是最贵的效应，这个失效方向是多花钱。**
#[test]
fn escalate是总次数上限不是每次运行的上限() {
    // 步 15c：原 `impl Client` 改为三个闭包端口；judge 恒给 0.5（落带内 → unsure），
    // generate 不该被调，ask 有问必答并计数
    fn 有问必答(次数: &RefCell<u64>) -> Ports<'_> {
        Ports::new()
            .with(FnPort::judge("fixed-0", |_s, qs| {
                Ok(JudgeResult {
                    answers: qs.iter().map(|_| Answer::Noul(0.5)).collect(),
                    tokens: 0,
                    cost: 0.0,
                    perms: vec![],
                    mode_share: vec![],
                    confidence: vec![],
                })
            }))
            .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
                Err(EffectError("不该 gen".into()))
            }))
            .with(FnPort::ask("fixed-0", |_s, _q| {
                *次数.borrow_mut() += 1;
                Ok(Some(Answer::Noul(0.95)))
            }))
    }

    // 上限 1：第一次运行问 1 次人
    let src = r#"
budget {calls: 4, cost: 1, depth: 8, escalate: 1};
let e = cut(judge(state(mat("甲")), test("行吗", "k")));
let 结论 = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" },
                     unsure: fn(u) { exit_kind(escalate(u, state(mat("甲")), test("行吗", "k"))) }});
{结论: 结论}
"#;
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let 次数 = RefCell::new(0);
    let mut ledger = Ledger::new();
    let _ = run(
        &program,
        有问必答(&次数),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("第一次跑完");
    assert_eq!(*次数.borrow(), 1, "第一次运行问了 1 次人");

    // 把账本里那条 ask 记录去掉 answer 之外的东西不现实；改为直接再跑一次同一本账本：
    // 账本里已有那条 ask，重放该命中、不再问人
    ledger.rebuild_index();
    let 次数2 = RefCell::new(0);
    let _ = run(
        &program,
        有问必答(&次数2),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("恢复后跑完");
    assert_eq!(
        *次数2.borrow(),
        0,
        "账本里已有的 ask 该重放命中，不重复问人"
    );

    // 换一个**新**站点（账本里没有）：上限已经用掉 1 次，这次必须被拦住
    let src2 = src.replace(r#"test("行吗", "k")"#, r#"test("换一道题", "k")"#);
    let program2 = lower(&parse(&src2).expect("解析")).expect("lower");
    let 次数3 = RefCell::new(0);
    let out = run(
        &program2,
        有问必答(&次数3),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("挂起不是错");
    assert_eq!(
        *次数3.borrow(),
        0,
        "上限 1 已经在上一轮用掉：恢复后不该再问第二个人。escalate 是总次数上限，不是每次运行 k 次"
    );
    assert!(
        out.pending.iter().any(|p| p.cause == "budget.escalate"),
        "该因 escalate 上限挂起：{:?}",
        out.pending
    );
}

/// 窗口检查（J-14 / `12`:117）：超窗要**留下痕迹**。
///
/// 超窗不是算错，是**读数被语境接管而无人察觉**——档案原话「≈1,000 token 带主张语境下
/// 翻转 60.7%，读数被语境接管」。所以它是 warning 不是 error（Python 侧 `_check_window`
/// 也是 `warn`），但**必须有**：没有它，答案偏了也没有任何痕迹。
#[test]
fn 状态超窗要报w_window() {
    let 长 = "语".repeat(3000); // 远超 1000 token
    let src = format!(
        r#"
budget {{calls: 2, cost: 1, depth: 8}};
let e = cut(judge(state(mat("短对象"), {{ctx: [mat("{长}")]}}), test("行吗", "k")));
consume(e, "drop");
{{出口: exit_kind(e)}}
"#
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut calib = CalibStore::new();
    calib.put("k", 0.65, 0.35, 100, "上岗", Some(0.05)).unwrap();
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("超窗是提示不是错：{}", e.render()));

    assert!(
        out.trace.warnings.iter().any(|w| w.starts_with("W-window")),
        "语境槽超窗要报 W-window，实际告警：{:?}",
        out.trace.warnings
    );

    // 反面：不超窗就不该报
    let 短 = src.replace(&长, "短语境");
    let program = lower(&parse(&短).expect("解析")).expect("lower");
    let calls = RefCell::new(0);
    let mut ledger = Ledger::new();
    let out = run(
        &program,
        定值端口(0.9, &calls),
        &calib,
        &ActionRegistry::new(),
        &mut ledger,
    )
    .expect("跑完");
    // 步 15d-2：没带画像时每个判断站点都会多一条 W-window-untested（§3.9），与这里要判的
    // 「超窗」告警（W-window:）无关；排除掉它再看真正的超窗告警有没有误报。
    assert!(
        !out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-window") && !w.starts_with("W-window-untested")),
        "不超窗不该报：{:?}",
        out.trace.warnings
    );
}

/// **`derived_from` 在三处重造边界被恒清零，J-02 实测失守**（`boundary-scan-1` 查出）。
///
/// `do` / `gen` / `transform` 三处输出都写 `BTreeSet::new()`——**taint 用
/// `fold(Trusted, join)` 折算了，`derived_from` 从不折算**。同一行模式的三份拷贝。
/// 于是 `transform(fn(x){content(x)}, m)` **一次恒等变换**就洗掉派生关系，J-02 不再拦。
/// 而 `transform` 是切分、抽字段、拼摘要这类常规操作。
///
/// 按 `12` J-02 范围裁定修：**保一跳**（并入各输入材料的 `derived_from`），
/// **不加跳**（`as_mat(exit)` 仍只放本次的 `q_hash`，闭包在那里截断——
/// 做成传递闭包会重演「逐字传播让几乎所有输出不可用」，材料越传越「派生自所有题」）。
#[test]
fn transform不能洗掉derived_from() {
    let src = r#"
budget {calls: 4, cost: 1, depth: 8};
let m = mat("材料");
let q = test("行吗", "k");
let e = cut(judge(state(m), q));
consume(e, "drop");
let 出口料 = mat(e);
let 洗过的 = transform(fn(x) { content(x) }, 出口料);
let 再问一次 = judge(state(洗过的), q);
let e2 = cut(再问一次);
consume(e2, "drop");
{r: exit_kind(e2)}
"#;
    let e = 跑(src, 0.9, &mut Ledger::new()).expect_err("一次恒等变换不该洗掉禁自指");
    assert!(
        e.render().contains("J-02"),
        "该是 J-02 禁自指：{}",
        e.render()
    );
}

/// `derived_from` 要**写得出也读得回**：账本序列化此前独立丢一次（写时没这个字段、
/// 读回来硬编码空集）。后果是**重放出来的程序与原程序在 J-02 上不是同一个程序**，
/// 而 J-18 的整套重放判定建立在它们是同一个上。
///
/// 账本 v3（步 18a，B84、B92）起账本写的是来源边（带种类），`derived_from` 是值依赖边的投影，
/// 读回时重算：往返后来源边与种类不变，`derived_from` 等于值依赖边的题哈希。
#[test]
fn derived_from要能往返账本() {
    use jpp::interp::{effect_value_to_entry, entry_to_effect_value};
    use jpp::value::{Mat, Sources, Taint, Value};
    use std::collections::BTreeSet;

    let s = Sources::value("出口甲", "题哈希甲")
        .union(&Sources::value("出口乙", "题哈希乙"))
        .union(&Sources::select("出口丙", "题哈希丙"));
    let m = Value::Mat(std::rc::Rc::new(
        Mat::new(
            serde_json::json!({"x": 1}),
            "a",
            vec!["do:k".into()],
            Taint::Untrusted,
            BTreeSet::new(),
        )
        .with_sources(&s),
    ));

    let (out, meta) = effect_value_to_entry(&m);
    let 回来 = entry_to_effect_value(&out, meta.as_ref());
    let Value::Mat(m2) = 回来 else {
        panic!("该是材料")
    };
    let d: BTreeSet<String> = ["题哈希甲", "题哈希乙"]
        .iter()
        .map(|x| x.to_string())
        .collect();
    assert_eq!(
        m2.derived_from, d,
        "derived_from 要往返得回来（值依赖边的投影），否则重放出来的不是同一个程序"
    );
    // 选择边只记键与种类（题哈希只对值依赖边有用，材料上不存），往返后比键、种类与值边的题哈希
    let 期望 = Sources::value("出口甲", "题哈希甲")
        .union(&Sources::value("出口乙", "题哈希乙"))
        .union(&Sources::from_key("出口丙"));
    assert_eq!(m2.prov().sources, 期望, "来源边与种类要往返");
    assert_eq!(m2.taint, Taint::Untrusted, "taint 也要（这条上一包修过）");
}
