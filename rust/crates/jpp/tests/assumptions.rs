//! **类假设 + 降级**（`12` §1）——这门语言的身份主张。
//!
//! §1 自己的「为什么」写着：把 jev-1.13 的性质**降为绑字段的假设**并写死降级规则，
//! **「换版本程序不改」才从口号变成机制**。
//!
//! 缺口在三层，**最要命的是第三层**：`check(program: &Program) -> Report` 只收一个程序,
//! **没有 `Profile`**——就算降级逻辑写好了，**档案字段也没有路径能到达检查器**。
//! 没填是缺料，**没有管道是缺结构**。
//!
//! 这一包只做能证死的那一段：**把管道修通，再拿一条假设走完全程**。

use jpp::check::{Report, Severity, check, check_with_profile};
use jpp::effects::{Profile, Tri};
use jpp::{lower, syntax::parse};

/// 读数做算术：J-01 的「算术在宿主」那一面，`12`:328 明写依赖 H5。
const 算术: &str = r#"
budget {calls: 2, cost: 1};
let r = judge(state(mat("材料")), test("行吗", "k"));
r + 1
"#;

/// 读数做比较：**这一面不归 H5 管**，它是 I3（跨题读数不成恒等式）。
const 比较: &str = r#"
budget {calls: 2, cost: 1};
let r = judge(state(mat("材料")), test("行吗", "k"));
r < 1
"#;

fn 档(field: Tri) -> Profile {
    // **不另存一份档案 JSON**：一份写着 `arithmetic_capable: true` 的文件摆在真实测量旁边,
    // 迟早被人读成一次测量。改成读真档案、在内存里注入这一个字段。
    let 真档 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    let text = std::fs::read_to_string(&真档).expect("真档案要在");
    let mut j: serde_json::Value = serde_json::from_str(&text).expect("合法 JSON");
    match field {
        Tri::真 => {
            j["arithmetic_capable"] = serde_json::json!(true);
        }
        Tri::假 => {
            j["arithmetic_capable"] = serde_json::json!(false);
        }
        Tri::未测 => {
            j.as_object_mut().unwrap().remove("arithmetic_capable");
        }
    }
    Profile::from_json(&j).expect("真档案加一个字段仍该读得动")
}

fn 判(src: &str, p: &Profile) -> jpp::check::Report {
    check_with_profile(&lower(&parse(src).expect("解析")).expect("lower"), p)
}

/// **这一包的红**：档案说 `arithmetic_capable: true`（H5 **不**成立），
/// 同一个程序**不改一个字**，J-01 的算术面从 error 降成 warn。
#[test]
fn 档案说模型会算术时j01的算术面降warn() {
    let 说不会 = 判(算术, &档(Tri::假));
    let d = 说不会.find("J-01").expect("H5 成立时算术是错");
    assert_eq!(d.severity, Severity::Error, "{}", d.render());

    // **同一段源码，一个字没动**
    let 说会 = 判(算术, &档(Tri::真));
    let d = 说会.find("J-01").expect("降级不是消失——它还要留痕");
    assert_eq!(
        d.severity,
        Severity::Warning,
        "档案说模型会算术 → 降 warn（12 §1.3）：{}",
        d.render()
    );
    assert!(说会.is_ok(), "warn 不阻塞");
    assert!(
        d.message.contains("arithmetic_capable"),
        "要说出是哪条档案字段让它降的：{}",
        d.message
    );
}

/// **降级只降算术那一面。** 比较是 I3 的事，与 H5 无关——
/// 「共用机制的前提是要保证的东西相同，不是听起来像同一类」。
#[test]
fn 比较那一面不跟着降() {
    for t in [Tri::假, Tri::真, Tri::未测] {
        let r = 判(比较, &档(t));
        let d = r.find("J-01").expect("比较一直是错");
        assert_eq!(
            d.severity,
            Severity::Error,
            "{t:?} 下比较仍是错：{}",
            d.render()
        );
    }
}

/// **反面一：没加载档案，必须和「档案说 H5 成立」区分得开。**
/// 这和「兜底档案的 `hash` 必须是 `None`」是同一条。
#[test]
fn 没档案与档案说成立区分得开() {
    // 没档案：`check` 老接口走的就是这条路
    let 无档 = check(&lower(&parse(算术).expect("解析")).expect("lower"));
    let 有档 = 判(算术, &档(Tri::假));

    // 两边 J-01 都是 error——**行为相同**（保守项一致）
    assert_eq!(无档.find("J-01").unwrap().severity, Severity::Error);
    assert_eq!(有档.find("J-01").unwrap().severity, Severity::Error);

    // 但**痕迹必须不同**：没档案时按 J-15 报 W-untested，有档案说了就不报——
    // 只看 `arithmetic_capable` 这一条的痕迹（步 24d 起 `judge` 站点另有 `one_hop`（H4）
    // 未测的 `W-untested`，与本测试要证的 H5 降级行为无关，用字段名把两者分开）
    let 关于算术字段 = |r: &Report| {
        r.diagnostics
            .iter()
            .any(|d| d.rule == "W-untested" && d.message.contains("arithmetic_capable"))
    };
    assert!(
        关于算术字段(&无档),
        "没加载档案 = 这条假设没被任何测量支持，必须留痕：{}",
        无档.render()
    );
    assert!(
        !关于算术字段(&有档),
        "档案明说了就不该报未测：{}",
        有档.render()
    );
}

/// **反面二：档案里这个字段「未测」，按 J-15 取该假设为真并报 W-untested**（`12` §1.3 末行）。
/// 而且它与「没加载档案」也要分得开——**两种不确定不是同一种**。
#[test]
fn 字段未测与没档案也分得开() {
    let 未测 = 判(算术, &档(Tri::未测));
    assert_eq!(
        未测.find("J-01").unwrap().severity,
        Severity::Error,
        "未测取保守项"
    );
    // 只看 `arithmetic_capable` 这一条（步 24d 起同一程序另有 `one_hop`（H4）未测的
    // `W-untested`，`.find()` 拿到的可能是它，不是本测试要证的那条）
    let w = 未测
        .diagnostics
        .iter()
        .find(|d| d.rule == "W-untested" && d.message.contains("arithmetic_capable"))
        .expect("未测要报 W-untested（J-15）");
    assert!(
        w.message.contains("档案"),
        "要说清是「档案说未测」：{}",
        w.message
    );

    let 无档 = check(&lower(&parse(算术).expect("解析")).expect("lower"));
    let w2 = 无档
        .diagnostics
        .iter()
        .find(|d| d.rule == "W-untested" && d.message.contains("arithmetic_capable"))
        .expect("没档案也要报");
    assert_ne!(
        w.message, w2.message,
        "**「档案说未测」与「根本没档案」是两种不确定**"
    );
}

/// 管道本身：`Profile` 真的带上了这一维，且真档案里**今天没有这个字段**。
#[test]
fn 真档案里今天没有这个字段() {
    let 真档 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&真档).unwrap()).unwrap();
    assert!(
        j.get("arithmetic_capable").is_none(),
        "**H5 的档案字段今天不存在**——这是 §1.2 八条里缺的那几条之一"
    );
    // 所以真档案读出来就是「未测」，而不是内核替它编一个值
    assert_eq!(
        Profile::from_json(&j).unwrap().arithmetic_capable(),
        Tri::未测
    );
}

/// **管道要在真实运行路径上通，不只在测试里通。**
/// 一个只在测试里通的管道是**构造，不是功能**——把消费方（`run` 传档案那一行）删掉
/// 它照样绿，那就证不了任何事。这一条删掉那一行就红。
#[test]
fn run这条路上档案也到得了检查器() {
    use jpp::ActionRegistry;
    use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
    use jpp::ledger::Ledger;
    use jpp::value::{Answer, Question, State};
    // 步 15c：原 `impl Client for 桩`（只用得到 judge，generate/ask 是占位 Err 且程序不会调）
    // 改为一个 judge 闭包端口。
    let program = lower(&parse(算术).expect("解析")).expect("lower");
    let 跑 = |p: Profile| {
        let mut store = CalibStore::new();
        store.profile = p;
        let mut l = Ledger::new();
        let ports = Ports::new().with(FnPort::judge("m", |_s: &State, qs: &[&Question]| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }));
        jpp::run(&program, ports, &store, &ActionRegistry::new(), &mut l)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    };
    // 档案说 H5 成立 → 静态就被 J-01 挡住，根本不开跑
    assert!(跑(档(Tri::假)).is_err(), "H5 成立时静态挡住");
    // 档案说模型会算术 → **静态不再挡**（降 warn），程序得以进入运行期
    let r = 跑(档(Tri::真));
    let e = format!("{r:?}");
    assert!(
        !e.contains("Check("),
        "**降级之后不该再被静态挡住**，实得 {e}"
    );
}
