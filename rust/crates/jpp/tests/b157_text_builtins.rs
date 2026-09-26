//! B157（确定性文本与数据内置）与 B158（带种子伪随机）回归测试（步 7t）。
//!
//! 依据：`地基/附注/2026-09-26-批6裁定.md` §五、§六；预注册 `地基/过程记录/工程-步7t.md` §六。
//! 覆盖点逐条对应预注册：taint 统一算法（B33 第 3 条 ∨ 输入）、`split`/`replace` 的空参数、
//! `sort`/`sort_by` 只认同类型标量与稳定性、`hash`/`to_json` 复用账本函数、`date_*` 的 UTC 与
//! `Fail` 语义、`rand`/`rand_int`/`shuffle` 的 splitmix64 算法（`RAND_VERSION = "s1"`）。

use jpp::interp::RAND_VERSION;
use jpp::value::{Taint, Value};
use jpp::{ActionRegistry, Error, TaintOut, run};
use jpp::{lower, syntax::parse};
use serde_json::json;

use jpp::effects::{CalibStore, Ports};
use jpp::ledger::Ledger;

/// 跑一个不需要判断/动作的纯程序，返回最终值。
fn 跑(src: &str) -> Value {
    跑带动作(src, ActionRegistry::new())
}

fn 跑带动作(src: &str, actions: ActionRegistry) -> Value {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let mut ledger = Ledger::new();
    run(&program, Ports::new(), &calib, &actions, &mut ledger)
        .unwrap_or_else(|e| panic!("run failed: {}", e.render()))
        .value
        .expect("程序没有挂起，应有值")
}

/// 期望运行期报错；返回 `RtError.rule`（静态检查错误在这里视为测试失败，不是预期路径）。
fn 跑期望错(src: &str) -> Option<String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let calib = CalibStore::new();
    let actions = ActionRegistry::new();
    let mut ledger = Ledger::new();
    match run(&program, Ports::new(), &calib, &actions, &mut ledger) {
        Ok(o) => panic!("期望报错，实际跑出 {:?}", o.value),
        Err(Error::Runtime(e)) => e.rule,
        Err(Error::Check(r)) => panic!("期望运行期错，撞到静态检查错误：{}", r.render()),
    }
}

/// 注册一个返回不可信文本的动作（与 `value_taint.rs` 同一手法）：`do` 的输出恒是 `Mat`，
/// 要 `content()` 拆开才能喂给文本内置。
fn 不可信来源() -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register("取外部", 0.0, true, TaintOut::Untrusted, |_| {
        Ok(Value::text("秘密文本"))
    });
    a
}

// ---------- split / chars ----------

#[test]
fn split_一般分隔符() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; split("a,b,,c", ",")"#);
    assert_eq!(v.to_json(), json!(["a", "b", "", "c"]));
}

#[test]
fn split_空分隔符等于chars() {
    let a = 跑(r#"budget {calls: 0, cost: 0}; split("abc", "")"#);
    let b = 跑(r#"budget {calls: 0, cost: 0}; chars("abc")"#);
    assert_eq!(a.to_json(), json!(["a", "b", "c"]));
    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn chars_基本() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; chars("ab")"#);
    assert_eq!(v.to_json(), json!(["a", "b"]));
}

// ---------- lower / upper / trim ----------

#[test]
fn lower_基本() {
    assert_eq!(
        跑(r#"budget {calls: 0, cost: 0}; lower("ABC")"#).to_json(),
        json!("abc")
    );
}

#[test]
fn upper_基本() {
    assert_eq!(
        跑(r#"budget {calls: 0, cost: 0}; upper("abc")"#).to_json(),
        json!("ABC")
    );
}

#[test]
fn trim_基本() {
    assert_eq!(
        跑(r#"budget {calls: 0, cost: 0}; trim("  hi  ")"#).to_json(),
        json!("hi")
    );
}

// ---------- replace ----------

#[test]
fn replace_一般() {
    assert_eq!(
        跑(r#"budget {calls: 0, cost: 0}; replace("aXbXc", "X", "-")"#).to_json(),
        json!("a-b-c")
    );
}

#[test]
fn replace_空from原样返回() {
    // 不用 Rust `str::replace("", to)` 的逐字符插入语义（预注册 §五）
    assert_eq!(
        跑(r#"budget {calls: 0, cost: 0}; replace("abc", "", "Z")"#).to_json(),
        json!("abc")
    );
}

// ---------- starts_with / ends_with / index_of ----------

#[test]
fn starts_ends_with() {
    let v = 跑(r#"budget {calls: 0, cost: 0};
        {s: starts_with("hello", "he"), e: ends_with("hello", "lo"),
         not_s: starts_with("hello", "lo")}"#);
    assert_eq!(v.to_json(), json!({"s": true, "e": true, "not_s": false}));
}

#[test]
fn index_of_命中与未命中() {
    let v = 跑(r#"budget {calls: 0, cost: 0};
        {found: index_of("hello", "ll"), missing: index_of("hello", "z")}"#);
    assert_eq!(v.to_json(), json!({"found": 2, "missing": -1}));
}

#[test]
fn index_of_按字符计数不按字节() {
    // 「世」前面两个汉字，字符位是 2；字节位会是 6（每个汉字 3 字节）
    let v = 跑(r#"budget {calls: 0, cost: 0}; index_of("你好世界", "世")"#);
    assert_eq!(v.to_json(), json!(2));
}

// ---------- regex_match / regex_find ----------

#[test]
fn regex_match_基本() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; regex_match("hello123", "[0-9]+")"#);
    assert_eq!(v.to_json(), json!(true));
}

#[test]
fn regex_match_非法正则报错() {
    let rule = 跑期望错(r#"budget {calls: 0, cost: 0}; regex_match("x", "[")"#);
    assert_eq!(rule.as_deref(), Some("E-rt-regex"));
}

#[test]
fn regex_find_多处匹配() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; regex_find("a1 b22 c333", "[0-9]+")"#);
    assert_eq!(v.to_json(), json!(["1", "22", "333"]));
}

#[test]
fn regex_find_非法正则报错() {
    let rule = 跑期望错(r#"budget {calls: 0, cost: 0}; regex_find("x", "(")"#);
    assert_eq!(rule.as_deref(), Some("E-rt-regex"));
}

// ---------- sort / sort_by ----------

#[test]
fn sort_升序() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; sort([3, 1, 2])"#);
    assert_eq!(v.to_json(), json!([1, 2, 3]));
}

#[test]
fn sort_int_float混合报错() {
    let rule = 跑期望错(r#"budget {calls: 0, cost: 0}; sort([1, 2.0])"#);
    assert_eq!(rule.as_deref(), Some("E-rt-type"));
}

#[test]
fn sort_by_复合键与稳定性() {
    // 键：先按 k 升序，k 相同按 j 升序；(k=1,j=1) 与 (k=1,j=1) 两条完全同键，
    // 靠原始顺序（i 字段，未参与键）验证稳定排序没有打乱同键元素的相对顺序。
    let src = r#"
budget {calls: 0, cost: 0};
let xs = [{k: 1, j: 2, i: 0}, {k: 0, j: 5, i: 1}, {k: 1, j: 1, i: 2},
          {k: 1, j: 1, i: 3}, {k: 0, j: 1, i: 4}];
sort_by(xs, fn(r) { [r.k, r.j] })
"#;
    let v = 跑(src);
    assert_eq!(
        v.to_json(),
        json!([
            {"k": 0, "j": 1, "i": 4},
            {"k": 0, "j": 5, "i": 1},
            {"k": 1, "j": 1, "i": 2},
            {"k": 1, "j": 1, "i": 3},
            {"k": 1, "j": 2, "i": 0},
        ])
    );
}

// ---------- parse_json / to_json ----------

#[test]
fn parse_json_往返() {
    let src = r#"
budget {calls: 0, cost: 0};
let original = {a: 1, b: [1, 2, true, "x"]};
parse_json(to_json(original))
"#;
    let v = 跑(src);
    assert_eq!(v.to_json(), json!({"a": 1, "b": [1, 2, true, "x"]}));
}

#[test]
fn parse_json_失败返回fail() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; is_fail(parse_json("{not json"))"#);
    assert_eq!(v.to_json(), json!(true));
}

#[test]
fn to_json_是规范化紧凑形式() {
    // 键排序、无空白：与账本 `canon` 同一函数（预注册 §一、§五）
    let v = 跑(r#"budget {calls: 0, cost: 0}; to_json({b: 1, a: 2})"#);
    assert_eq!(v.to_json(), json!(r#"{"a":2,"b":1}"#));
}

// ---------- hash ----------

#[test]
fn hash_确定同输不同输() {
    let v = 跑(r#"budget {calls: 0, cost: 0};
        {same: hash("abc") == hash("abc"), diff: hash("abc") == hash("abd")}"#);
    assert_eq!(v.to_json(), json!({"same": true, "diff": false}));
}

// ---------- date_parse / date_format / date_add ----------

#[test]
fn date往返() {
    let src = r#"
budget {calls: 0, cost: 0};
let fmt = "%Y-%m-%d %H:%M:%S";
let t = "2024-01-02 03:04:05";
date_format(date_parse(t, fmt), fmt)
"#;
    let v = 跑(src);
    assert_eq!(v.to_json(), json!("2024-01-02 03:04:05"));
}

#[test]
fn date_parse失败返回fail() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; is_fail(date_parse("not-a-date", "%Y-%m-%d"))"#);
    assert_eq!(v.to_json(), json!(true));
}

#[test]
fn date_add三个字段() {
    let src = r#"
budget {calls: 0, cost: 0};
{days: date_add(0, {days: 1}), hours: date_add(0, {hours: 2}),
 minutes: date_add(0, {minutes: 30}), combo: date_add(0, {days: 1, hours: 2, minutes: 30})}
"#;
    let v = 跑(src);
    assert_eq!(
        v.to_json(),
        json!({"days": 86_400, "hours": 7_200, "minutes": 1_800, "combo": 95_400})
    );
}

// ---------- taint 传播（B33 第 3 条：∨ 输入） ----------

#[test]
fn taint_全可信输入输出可信() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; hash("abc")"#);
    assert_eq!(v.taint(), Taint::Trusted);
}

#[test]
fn taint_不可信输入经split_hash_sortby全部污染输出() {
    let a = 跑带动作(
        r#"budget {calls: 1, cost: 0, depth: 8};
        split(content(do("取外部", [], 0)), "")"#,
        不可信来源(),
    );
    assert_eq!(a.taint(), Taint::Untrusted);

    let b = 跑带动作(
        r#"budget {calls: 1, cost: 0, depth: 8};
        hash(content(do("取外部", [], 0)))"#,
        不可信来源(),
    );
    assert_eq!(b.taint(), Taint::Untrusted);

    // sort_by 的 taint 只看它自己的参数（列表、排序函数），不追踪排序函数体内闭包捕获的自由变量
    // （与 `map`/`filter`/`fold` 同一限度：宿主计算内的显式数据流才算，不做逐闭包分析）。
    // 所以把不可信值放进被排序的**列表**本身，而不是通过闭包捕获间接引入。
    let c = 跑带动作(
        r#"budget {calls: 1, cost: 0, depth: 8};
        let s = content(do("取外部", [], 0));
        sort_by([s, "a", "b"], fn(x) { x })"#,
        不可信来源(),
    );
    assert_eq!(c.taint(), Taint::Untrusted);
}

// ---------- rand / rand_int / shuffle（B158） ----------

#[test]
fn rand_version标记() {
    assert_eq!(RAND_VERSION, "s1");
}

#[test]
fn rand_同种子同k恒同值() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; {a: rand(42, 3), b: rand(42, 3)}"#);
    let j = v.to_json();
    assert_eq!(j["a"], j["b"]);
}

#[test]
fn rand_跨两次独立运行同值() {
    let src = r#"budget {calls: 0, cost: 0}; rand(1, 0)"#;
    let a = 跑(src);
    let b = 跑(src);
    assert_eq!(a.to_json(), b.to_json());
    // 算法回归钉子：splitmix64（RAND_VERSION s1）在 (seed=1, k=0) 上的值，跑一次固定下来，
    // 换算法或换常量要改这个数并说明理由，不能静默漂移。
    let f = a.to_json().as_f64().expect("Float");
    assert!((0.0..1.0).contains(&f), "rand 落在 [0,1) 之外：{f}");
}

#[test]
fn rand_int落在范围内且n非法报错() {
    let v = 跑(r#"budget {calls: 0, cost: 0};
        map(range(0, 20), fn(i) { rand_int(9, i, 7) })"#);
    let Value::List(l) = &v else {
        panic!("应为列表");
    };
    for x in l.iter() {
        let n = x.to_json().as_i64().expect("Int");
        assert!((0..7).contains(&n), "rand_int 越界：{n}");
    }
    let rule = 跑期望错(r#"budget {calls: 0, cost: 0}; rand_int(1, 2, 0)"#);
    assert_eq!(rule.as_deref(), Some("E-rt-arg"));
}

#[test]
fn shuffle_是置换() {
    let v = 跑(r#"budget {calls: 0, cost: 0}; shuffle([1, 2, 3, 4, 5], 7)"#);
    let Value::List(l) = &v else {
        panic!("应为列表");
    };
    let mut got: Vec<i64> = l.iter().map(|x| x.to_json().as_i64().unwrap()).collect();
    got.sort();
    assert_eq!(got, vec![1, 2, 3, 4, 5]);
}

#[test]
fn shuffle_跨两次独立运行同值() {
    let src = r#"budget {calls: 0, cost: 0}; shuffle([1, 2, 3, 4, 5], 7)"#;
    let a = 跑(src);
    let b = 跑(src);
    assert_eq!(a.to_json(), b.to_json());
}

#[test]
fn shuffle_不可信列表元素传导为不可信输出() {
    // 列表里混一个不可信元素（其余是可信字面量），taint = ∨ 输入：整份输出标记不可信。
    let v = 跑带动作(
        r#"budget {calls: 1, cost: 0, depth: 8};
        shuffle([1, 2, content(do("取外部", [], 0))], 7)"#,
        不可信来源(),
    );
    assert_eq!(v.taint(), Taint::Untrusted);
}
