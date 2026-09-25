//! **`responses` 未命中是静默的**——第二个零上下文作者的一对一对照：
//! 同一个探针、同一份夹具，**只把题面的全角「？」改成半角「?」**——
//! 改 `observations` **大声报错并吐两份 JSON**；改 `responses`
//! **stderr 空、warnings 空、status 仍是 `pending`，与「人确实还没答」逐字段一模一样**。
//! **它 5 个探针本来一条报错就能省掉。**
//!
//! 根在 `effects.rs:137`：`self.asks.get(&key).cloned().unwrap_or(None)`
//! ——**又一个 `unwrap_or`**：「键不在表里」（你的 responses 没命中）与
//! 「键在表里、值是 `None`」（人还没答）**压成了同一个 `None`**。
//!
//! **而「在等人答」是合法状态，不能把它也报成错**——所以判据是：
//! **夹具压根没给 `responses` 时静默挂起（那是第一趟，本来就没人答）；
//! 给了却没命中时出声。**

use jpp::effects::{CallInput, EffectError, EffectId, EffectOut, FixedPorts, JudgeResult};
use jpp::value::{Answer, Mat, Op, Question, State};
use serde_json::json;

fn 状态(s: &str) -> State {
    State::new(vec![Mat::literal(json!(s))], vec![], vec![], vec![], false)
}

/// 经端口问一次人（步 15c：`FixedClient::ask` 已删，改经 `Ports::call` 走 `FixedAsk` 端口）。
fn ask(fp: &mut FixedPorts, s: &State, q: &Question) -> Result<Option<Answer>, EffectError> {
    let mut ports = fp.ports();
    match ports.call(
        EffectId::Ask,
        CallInput::StateQuestion {
            state: s.clone(),
            question: q.clone(),
        },
    )? {
        EffectOut::Answer(a) => Ok(a),
        _ => unreachable!("ask 该给 Answer"),
    }
}

/// 经端口判断一次（步 15c：`FixedClient::judge` 已删，改经 `Ports::call` 走 `FixedJudge` 端口）。
fn judge(fp: &mut FixedPorts, s: &State, qs: &[&Question]) -> Result<JudgeResult, EffectError> {
    let mut ports = fp.ports();
    match ports.call(
        EffectId::Judge,
        CallInput::StateQuestions {
            state: s.clone(),
            questions: qs.iter().map(|q| (*q).clone()).collect(),
        },
    )? {
        EffectOut::Readings(r) => Ok(r),
        _ => unreachable!("judge 该给 Readings"),
    }
}

/// **第一趟：一条 `responses` 都没给 → 静默挂起。** 这是正常的等待，不该出声。
#[test]
fn 没给responses时静默挂起() {
    let mut c = FixedPorts::new();
    let q = Question::new(Op::Test, "批不批？", "human", vec![]);
    assert_eq!(
        ask(&mut c, &状态("甲"), &q).expect("不该报错"),
        None,
        "**没人答就是没人答**"
    );
}

/// **给了 `responses` 却没命中 → 要出声，并且吐出可对照的两份 JSON。**
#[test]
fn 给了responses却没命中要出声() {
    let mut c = FixedPorts::new();
    // 登记的是全角「？」
    c.fix_ask(
        &状态("甲"),
        &Question::new(Op::Test, "批不批？", "human", vec![]),
        Some(Answer::Noul(1.0)),
    );
    // 程序问的是半角「?」
    let q = Question::new(Op::Test, "批不批?", "human", vec![]);
    let Err(e) = ask(&mut c, &状态("甲"), &q) else {
        panic!("**给了却没命中，不该静默挂起**")
    };
    assert!(e.0.contains("批不批?"), "要说出程序问的是哪一道：{}", e.0);
    assert!(
        e.0.contains("responses"),
        "要说清是 responses 这一栏：{}",
        e.0
    );
    println!("{}", e.0);
}

/// **命中的照常答。**
#[test]
fn 命中的照常答() {
    let mut c = FixedPorts::new();
    let q = Question::new(Op::Test, "批不批？", "human", vec![]);
    c.fix_ask(&状态("甲"), &q, Some(Answer::Noul(1.0)));
    assert_eq!(
        ask(&mut c, &状态("甲"), &q).expect("命中"),
        Some(Answer::Noul(1.0))
    );
}

/// **登记了但明说「还没答」的，静默挂起**——那是真的在等人。
#[test]
fn 登记了但还没答的静默挂起() {
    let mut c = FixedPorts::new();
    let q = Question::new(Op::Test, "批不批？", "human", vec![]);
    c.fix_ask(&状态("甲"), &q, None);
    assert_eq!(ask(&mut c, &状态("甲"), &q).expect("不该报错"), None);
}

/// **未命中报文打出来的那两份 JSON 要能直接粘进夹具。**
/// 作者实测照抄跑不起来：`on` 打成对象**而夹具要列表**（`invalid type: map, expected a sequence`）；
/// `op` 打成内核名 `noul` **而夹具只认 `test`**。
/// **报文说「照抄这两份 JSON」，那句话原来是假的。**
#[test]
fn 打出来的json要能直接粘贴() {
    let mut c = FixedPorts::new();
    let s = State::new(
        vec![Mat::literal(json!({"item": "flight"}))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let q = Question::new(Op::Measure, "多大把握", "k", vec!["低".into(), "高".into()]);
    let Err(e) = judge(&mut c, &s, &[&q]) else {
        panic!("该未命中")
    };

    assert!(
        e.0.contains(r#""on":[{"item":"flight"}]"#),
        "**`on` 要是列表**，夹具只收列表：{}",
        e.0
    );
    assert!(
        e.0.contains(r#""op":"measure""#),
        "**`op` 要用夹具认的名字**（test/select/measure），不是内核的 noul/choice/score：{}",
        e.0
    );
    assert!(!e.0.contains(r#""op":"score""#), "{}", e.0);
    println!("{}", e.0);
}

/// **`iter_seq` 认的是「这一轮会不会变」，不是「写没写成字面量」。**
///
/// 作者实测：`let s = 0` 放循环外再传进去，**四种写法都撞不出 `W-seq-const`**
/// ——而那个键每轮一模一样，**后果与写字面量 `0` 完全相同**。
/// **「常量」是一个语义性质，不是一个语法形状。**
#[test]
fn 循环外绑的常量也算常量() {
    let 查 = |src: &str| {
        let p = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
        jpp::check(&p)
    };
    // 字面量：本来就拦
    let a = 查(r#"
budget {calls: 0, cost: 0, depth: 8};
loop(3, 0, fn(acc, i) { let _ = do("read_json", ["x.json"], 0); acc })
"#);
    assert!(
        a.find("J-13").is_some() || a.find("W-seq-const").is_some(),
        "{}",
        a.render()
    );

    // **循环外绑的名字：以前完全静默**
    let b = 查(r#"
budget {calls: 0, cost: 0, depth: 8};
let 序号 = 0;
loop(3, 0, fn(acc, i) { let _ = do("read_json", ["x.json"], 序号); acc })
"#);
    assert!(
        b.find("J-13").is_some() || b.find("W-seq-const").is_some(),
        "**循环外绑的常量与字面量后果相同，不能静默**：{}",
        b.render()
    );

    // **反面：由轮次算出来的名字不算常量**——不许把它误报成常量（那是假拒绝）
    let c = 查(r#"
budget {calls: 0, cost: 0, depth: 8};
loop(3, 0, fn(acc, i) { let 序号 = i + 1; let _ = do("read_json", ["x.json"], 序号); acc })
"#);
    assert!(
        c.find("J-13").is_none() && c.find("W-seq-const").is_none(),
        "**`let 序号 = i + 1` 每轮都变，不该报**：{}",
        c.render()
    );
}
