//! 步 24b（B62/I-10）：J-14 运行期面 `W-multi-object`。`on` 槽材料的 JSON 顶层含长度大于 1
//! 的列表、且这次问的题面像是按编号或键引用其中的元素时报（warn，不阻塞）；单对象 JSON 不报。
//! 静态面只查 `state(on)` 的实参个数，看不见 `mat({task, blocks})` 这种把列表塞进一个材料的
//! 写法。依据：B62/I-10（`地基/附注/2026-09-24-探针首轮裁定.md` §一·4）；预注册见
//! `地基/过程记录/工程-步24b.md`。

use jpp::effects::{CalibStore, FixedPorts};
use jpp::ledger::Ledger;
use jpp::value::{Answer, Mat, Op, Question, State};
use jpp::{ActionRegistry, run};
use jpp::{lower, syntax::parse};
use serde_json::json;

/// 跑一个「state(mat(content))」+ 一道 `test` 题的最小程序，给一份固定观察让它能跑完；
/// 返回运行期的 trace.warnings。
fn warnings_for(content: serde_json::Value, text: &str) -> Vec<String> {
    let src = format!(
        "budget {{calls: 1, cost: 1}};\nlet e = cut(judge(state(mat({})), test(\"{}\", \"k\")));\nhandle(e, {{act: fn() {{ \"是\" }}, ignore: fn() {{ \"否\" }}, unsure: fn(u) {{ consume(u, \"drop\"); \"未决\" }}}})\n",
        j_literal(&content),
        text
    );
    let program = lower(&parse(&src).expect("解析")).expect("lower");
    let mut fp = FixedPorts::new();
    let st = State::new(vec![Mat::literal(content)], vec![], vec![], vec![], false);
    let q = Question::new(Op::Test, text, "k", vec![]);
    fp.observe(&st, &q, Answer::Noul(0.5));
    let a = ActionRegistry::new();
    let mut l = Ledger::new();
    let o = run(&program, fp.ports(), &CalibStore::new(), &a, &mut l)
        .unwrap_or_else(|e| panic!("跑得完：{}", e.render()));
    o.trace.warnings.clone()
}

/// 把 `serde_json::Value` 写成 J++ 源码里的字面量（本测试只用得到字符串、数字、列表、记录）。
fn j_literal(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => format!("{s:?}"),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(a) => {
            format!(
                "[{}]",
                a.iter().map(j_literal).collect::<Vec<_>>().join(", ")
            )
        }
        serde_json::Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{k}: {}", j_literal(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "unit".into(),
    }
}

/// 命中：`blocks` 是三元素列表，题面按编号「第 2 段」引用。
#[test]
fn 编号引用_命中() {
    let content = json!({"blocks": [{"name": "甲"}, {"name": "乙"}, {"name": "丙"}]});
    let ws = warnings_for(content, "第 2 段说的是谁？");
    let hits: Vec<_> = ws
        .iter()
        .filter(|w| w.starts_with("W-multi-object"))
        .collect();
    assert_eq!(hits.len(), 1, "{ws:?}");
    assert!(hits[0].contains("3 个元素"), "{}", hits[0]);
}

/// 命中：题面按元素对象的**字段名**引用（键，不是值、不是数字）——收集列表元素全部顶层
/// 字段名，题面逐字包含其中之一即算。
#[test]
fn 字段名引用_命中() {
    let content = json!({"blocks": [{"apple": "苹果"}, {"banana": "香蕉"}, {"cherry": "樱桃"}]});
    let ws = warnings_for(content, "apple 字段说的是什么？");
    let hits: Vec<_> = ws
        .iter()
        .filter(|w| w.starts_with("W-multi-object"))
        .collect();
    assert_eq!(hits.len(), 1, "{ws:?}");
}

/// 不命中：单对象 JSON，没有顶层列表。
#[test]
fn 单对象json_不命中() {
    let content = json!({"name": "甲", "role": "buyer"});
    let ws = warnings_for(content, "这是谁？");
    assert!(
        ws.iter().all(|w| !w.starts_with("W-multi-object")),
        "{ws:?}"
    );
}

/// 不命中：有多元素列表，但题面既不含数字也不含元素字段名（不像是在引用某一个）。
#[test]
fn 列表但题面不像引用_不命中() {
    let content = json!({"blocks": [{"topic": "天气"}, {"topic": "交通"}, {"topic": "教育"}]});
    let ws = warnings_for(content, "这份材料整体讨论的是什么主题？");
    assert!(
        ws.iter().all(|w| !w.starts_with("W-multi-object")),
        "{ws:?}"
    );
}

/// 命中：复现 `probes/winnow/winnow-batched.jpp` 的真实写法——
/// `mat({task, blocks})` 打包多块，题面模板「状态里编号为 {id} 的输出块…」按块 id（含数字）
/// 逐块填出。全仓扫描（§完成小结）在这份探针文件上确认了这个命中，本测试是最小复现。
#[test]
fn 复现winnow_batched写法_命中() {
    let content = json!({
        "task": "找出报错块",
        "blocks": [{"id": "b0", "text": "正常"}, {"id": "b1", "text": "报错"}]
    });
    let ws = warnings_for(
        content,
        "状态里编号为 b0 的输出块，对完成状态里的 task 是否有用？",
    );
    let hits: Vec<_> = ws
        .iter()
        .filter(|w| w.starts_with("W-multi-object"))
        .collect();
    assert_eq!(hits.len(), 1, "{ws:?}");
}
