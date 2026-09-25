//! 验收 3 第一级：换画像程序不改（`20` v2 §十、第 1078 行、第 1134 行、D20.3；`21` 步 15g）。
//!
//! 四节（`过程记录/工程-步15g.md` 预注册）：
//! - (A) 全部示例逐字段换值：金样用例在固定观察下，以发行画像为底，对 12 个类假设字段各换一次反值，
//!   与底画像的运行比，程序不改、出口与账本条目相同。**不计入通过数**：多数字段今天没有读者，这一节全绿也
//!   不证明降级，只证明「程序不改仍能跑」。
//! - (B) 八条类假设逐条降级（`12` §1.3、B39、B126）：每条一个或两个测试，断言该行规定的可观察差别。
//!   **计数规则**：一条假设的全部测试都不带 `#[ignore]` 且通过时计 1；读者未建的照写断言、标
//!   `#[ignore = "降级落在 …"]`，不计数。仪表只数这一节（`scripts/dashboard.py::item_profile_swap`）。
//! - (C) 数值字段各未测一次（`20` §3.9），不计入八条。
//! - (D) 第二判断器 `stub` 跑全部示例，以及校准不跨判断器（`12` B60、B127）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::check::{Report, Severity, check_with_profile};
use jpp::effects::Profile;
use jpp::{lower, syntax::parse};
use serde_json::{Value as Json, json};

// ---------- 公共 ----------

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 发行画像() -> Json {
    serde_json::from_slice(&std::fs::read(root().join("profiles/jev-1.13.0.json")).unwrap())
        .unwrap()
}

fn 画像(j: &Json) -> Profile {
    Profile::from_json(j).expect("画像读得动")
}

/// 发行画像改一个顶层字段（`None` = 删去，即未测）
fn 改(字段: &str, 值: Option<Json>) -> Profile {
    let mut j = 发行画像();
    match 值 {
        Some(v) => {
            j[字段] = v;
        }
        None => {
            j.as_object_mut().unwrap().remove(字段);
        }
    }
    画像(&j)
}

fn 查(src: &str, p: &Profile) -> Report {
    check_with_profile(&lower(&parse(src).expect("解析")).expect("lower"), p)
}

/// 报告里有没有点名 `字段` 的 `W-untested`
fn 点名未测(r: &Report, 字段: &str) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.rule == "W-untested" && d.message.contains(字段))
}

fn jpp(cwd: &Path, args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

fn 用例() -> Vec<Json> {
    let m: Json =
        serde_json::from_slice(&std::fs::read(root().join("tests/golden/manifest.json")).unwrap())
            .unwrap();
    m["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["expect"] != "error")
        .cloned()
        .collect()
}

fn 临时(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-swap-{}-{名}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 一次运行的结果：是否成功、报告、账本行（去掉 `prev` 与画像哈希）、stderr
struct 运行 {
    ok: bool,
    报告: Json,
    账本: Vec<Json>,
    错: String,
}

/// 跑一个金样用例。`后端` 为 `None` 走固定观察（`--fixtures`），`Some(名)` 走注册后端、不带夹具。
fn 跑用例(
    c: &Json, dir: &Path, 画像文件: &Path, 后端: Option<&str>, 带校准: bool
) -> 运行 {
    let name = c["name"].as_str().unwrap();
    let tmp = dir.join(name);
    std::fs::create_dir_all(&tmp).unwrap();
    if let Some(o) = c["files"].as_object() {
        for (k, v) in o {
            std::fs::write(tmp.join(k), v.as_str().unwrap()).unwrap();
        }
    }
    let mut a: Vec<String> = vec![
        "run".into(),
        root()
            .join(c["source"].as_str().unwrap())
            .display()
            .to_string(),
    ];
    match 后端 {
        None => {
            if let Some(f) = c["fixtures"].as_str() {
                a.push("--fixtures".into());
                a.push(root().join(f).display().to_string());
            }
        }
        Some(b) => {
            a.push("--backend".into());
            a.push(b.into());
        }
    }
    if 带校准 && let Some(f) = c["calib"].as_str() {
        a.push("--calib".into());
        a.push(root().join(f).display().to_string());
    }
    if let Some(from) = c["resume_from"].as_str() {
        a.push("--resume".into());
        a.push(dir.join(from).join("ledger.json").display().to_string());
    }
    a.extend([
        "--profile".into(),
        画像文件.display().to_string(),
        "--ledger-out".into(),
        "ledger.json".into(),
        "--output".into(),
        "report.json".into(),
    ]);
    let out = jpp(&tmp, &a);
    let 报告: Json = std::fs::read(tmp.join("report.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Json::Null);
    let 账本: Vec<Json> = std::fs::read_to_string(tmp.join("ledger.json"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut v: Json = serde_json::from_str(l).unwrap();
            去键(&mut v, &["prev", "profile_hash", "behavior_hash"]);
            v
        })
        .collect();
    运行 {
        ok: out.status.success(),
        报告,
        账本,
        错: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn 去键(v: &mut Json, ks: &[&str]) {
    match v {
        Json::Object(o) => {
            for k in ks {
                o.remove(*k);
            }
            o.values_mut().for_each(|x| 去键(x, ks));
        }
        Json::Array(a) => a.iter_mut().for_each(|x| 去键(x, ks)),
        _ => {}
    }
}

/// 报告里已决的判断出口（不是 `unsure…` 的）
fn 已决出口(rep: &Json) -> Vec<String> {
    rep["exits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["exit"].as_str())
        .filter(|x| !x.starts_with("unsure"))
        .map(String::from)
        .collect()
}

// ---------- (A) 全部示例逐字段换值：程序不改（不计数） ----------

/// (A) 节比对的一项：是否跑完、报告、账本行
type 比对项 = (bool, Json, Vec<Json>);

/// 12 个类假设字段各换一次反值
fn 反值() -> Vec<(&'static str, Option<Json>)> {
    let j = 发行画像();
    let 取反 = |k: &str| Some(json!(!j[k].as_bool().unwrap_or(true)));
    let mut 模态 = j["modalities_accepted"].clone();
    match 模态.as_array_mut() {
        Some(a) => a.push(json!("image")),
        None => 模态 = json!(["text", "image"]),
    }
    let mut k = j["k_limit"].clone();
    if k.is_object() {
        k["hard_max_options"] = json!(2);
    } else {
        k = json!({"hard_max_options": 2});
    }
    vec![
        ("text_only", 取反("text_only")),
        ("modalities_accepted", Some(模态)),
        ("fixed_output_types", 取反("fixed_output_types")),
        ("select_sums_to_one", 取反("select_sums_to_one")),
        ("k_limit", Some(k)),
        ("calibration", None),
        ("one_hop", 取反("one_hop")),
        ("arithmetic_capable", 取反("arithmetic_capable")),
        ("window_bounded", 取反("window_bounded")),
        ("questions_free", 取反("questions_free")),
        ("multi_object_crosstalk", 取反("multi_object_crosstalk")),
        ("assertion_susceptible", 取反("assertion_susceptible")),
    ]
}

#[test]
fn a_全部示例逐字段换值程序不改() {
    let base = 临时("a");
    let cases = 用例();
    let mut 变体 = vec![("底".to_string(), 发行画像())];
    for (k, v) in 反值() {
        let mut j = 发行画像();
        match v {
            Some(v) => j[k] = v,
            None => {
                j.as_object_mut().unwrap().remove(k);
            }
        }
        变体.push((k.to_string(), j));
    }
    let mut 结果: BTreeMap<String, BTreeMap<String, 比对项>> = BTreeMap::new();
    for (名, j) in &变体 {
        let d = base.join(名);
        std::fs::create_dir_all(&d).unwrap();
        let pf = d.join("profile.json");
        std::fs::write(&pf, serde_json::to_vec(j).unwrap()).unwrap();
        for c in &cases {
            let r = 跑用例(c, &d, &pf, None, true);
            结果
                .entry(名.clone())
                .or_default()
                .insert(c["name"].as_str().unwrap().into(), (r.ok, r.报告, r.账本));
        }
    }
    let 底 = &结果["底"];
    assert!(底.values().all(|r| r.0), "底画像下所有用例都要跑完");
    let mut 不同 = vec![];
    for (名, 每例) in &结果 {
        for (case, r) in 每例 {
            if r != &底[case] {
                不同.push(format!("{名} / {case}"));
            }
        }
    }
    assert!(不同.is_empty(), "换字段后与底画像不同：{不同:?}");
    assert_eq!(变体.len(), 13);
    let _ = std::fs::remove_dir_all(&base);
}

// ---------- (B) 八条类假设逐条降级（计数） ----------

const 单判: &str = r#"
budget {calls: 2, cost: 1};
let r = cut(judge(state(mat("材料")), test("行吗", "k")));
consume(r, "drop");
"#;

/// H1 守卫半（B126，步 24b `E-modality`）：`state` 收非文字材料时，`text_only` 未测或为 `true` 即 error，
/// 未测点名字段；`text_only: false` 且模态在 `modalities_accepted` 内放行。字面 `mat(…)` 恒为文字，不报。
#[test]
fn h1_守卫半() {
    use jpp::effects::{CalibStore, FixedPorts};
    use jpp::{ActionRegistry, EntryArgs, EntryMat, Mat, Session, ledger::Ledger};
    let src = r#"
budget {calls: 2, cost: 1};
let r = cut(judge(state(图), test("行吗", "k")));
consume(r, "drop");
"#;
    let mut 图 = Mat::literal(json!("（图像）"));
    图.modality = "image".into();
    let entry = EntryArgs {
        materials: vec![EntryMat::new("图", 图)],
        ..EntryArgs::default()
    };
    let 跑 = |p: Profile| {
        let program = Session::compile(&parse(src).unwrap(), &entry.decl()).unwrap();
        let mut calib = CalibStore::new();
        calib.profile = p;
        let mut fixed = FixedPorts::default();
        let acts = ActionRegistry::new();
        let mut l = Ledger::new();
        Session::new(fixed.ports(), &calib, &acts)
            .run(&program, &entry, &mut l)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    };
    let 未测 = 跑(改("text_only", None)).expect_err("未测按只收文字");
    assert!(
        未测.contains("E-modality") && 未测.contains("text_only"),
        "{未测}"
    );
    assert!(跑(改("text_only", Some(json!(true)))).is_err());
    let mut j = 发行画像();
    j["text_only"] = json!(false);
    j["modalities_accepted"] = json!(["text", "image"]);
    let 放行 = 跑(画像(&j));
    assert!(!format!("{放行:?}").contains("E-modality"), "{放行:?}");
    assert!(
        !查(单判, &改("text_only", None))
            .diagnostics
            .iter()
            .any(|d| d.rule == "E-modality")
    );
}

/// H1 渲染半（B126，步 25 S 库声明行）：`text_only: false` 时渲染类 `do`（此处 `ocr`）为空操作，返回
/// 输入材料本身、不发效应；未测按 B39 照常派发。
#[test]
#[ignore = "B126：渲染半落步 25（S 库渲染类 do 读画像）"]
fn h1_渲染半() {
    let src = r#"
budget {calls: 2, cost: 1};
ocr(mat("材料"))
"#;
    let 反值 = 查(src, &改("text_only", Some(json!(false))));
    assert!(反值.is_ok(), "{}", 反值.render());
    assert!(
        反值
            .diagnostics
            .iter()
            .any(|d| d.message.contains("空操作")),
        "{}",
        反值.render()
    );
}

/// H2：`select_sums_to_one` 未测按 `false`（「都不是」须显式给，诊断点名字段）；`true` 不报；
/// `fixed_output_types` 未测点名；`k_limit` 未测时候选超已知上限处报 `W-klimit-untested`。
#[test]
#[ignore = "select_sums_to_one/fixed_output_types 已落步 24d；k_limit 未测报 W-klimit-untested 待 23b（候选下沉 lower）"]
fn h2_三种输出结构() {
    let src = r#"
budget {calls: 2, cost: 1};
let e = cut(judge(state(mat("哪个好"), {over: [mat("甲"), mat("乙"), mat("丙")]}), select("选一个", "k")));
consume(e, "drop");
"#;
    let 未测 = 查(src, &改("select_sums_to_one", None));
    assert!(
        未测
            .diagnostics
            .iter()
            .any(|d| d.message.contains("select_sums_to_one")),
        "{}",
        未测.render()
    );
    let 和为一 = 查(src, &改("select_sums_to_one", Some(json!(true))));
    assert!(
        !和为一
            .diagnostics
            .iter()
            .any(|d| d.message.contains("select_sums_to_one"))
    );
    assert!(点名未测(
        &查(src, &改("fixed_output_types", None)),
        "fixed_output_types"
    ));
    let k未测 = 查(src, &改("k_limit", None));
    assert!(
        k未测.find("W-klimit-untested").is_some(),
        "{}",
        k未测.render()
    );
}

/// H3：`calibration` 未过检（未测）时代价比线不可用，`cut` 处报 `W-untested` 点名字段。
#[test]
fn h3_校准() {
    assert!(点名未测(
        &查(单判, &改("calibration", None)),
        "calibration"
    ));
}

/// H4：`one_hop` 未测按 `true`，J-09 `insufficient` 先查保留，判断处报 `W-untested` 点名字段。
/// `one_hop` 为假时的放宽形式（B39 未裁）本条不测，只测未测报文点名与已测（`true`）时不报名字。
#[test]
fn h4_一跳字面() {
    assert!(点名未测(&查(单判, &改("one_hop", None)), "one_hop"));
    assert!(!点名未测(
        &查(单判, &改("one_hop", Some(json!(true)))),
        "one_hop"
    ));
}

/// H5：`arithmetic_capable` 未测与 `false`：J-01 算术面 error；`true`：降 warn 并说出字段；未测点名字段。
#[test]
fn h5_不做算术() {
    let src = r#"
budget {calls: 2, cost: 1};
let r = judge(state(mat("材料")), test("行吗", "k"));
r + 1
"#;
    let 假 = 查(src, &改("arithmetic_capable", Some(json!(false))));
    assert_eq!(假.find("J-01").unwrap().severity, Severity::Error);
    assert!(!点名未测(&假, "arithmetic_capable"));
    let 真 = 查(src, &改("arithmetic_capable", Some(json!(true))));
    let d = 真.find("J-01").expect("降级不是消失");
    assert_eq!(d.severity, Severity::Warning, "{}", d.render());
    assert!(d.message.contains("arithmetic_capable"));
    let 未测 = 查(src, &改("arithmetic_capable", None));
    assert_eq!(未测.find("J-01").unwrap().severity, Severity::Error);
    assert!(
        未测
            .diagnostics
            .iter()
            .any(|d| d.message.contains("arithmetic_capable") && d.message.contains("未测")),
        "{}",
        未测.render()
    );
}

/// H6：`window_bounded` 未测按 `true`（裂变开），判断处报 `W-untested` 点名字段；`false` 裂变关、不报。
#[test]
#[ignore = "降级落在 23b（fission，先跑 V8）与步 24（J-14 窗口检查读画像）"]
fn h6_窗口() {
    assert!(点名未测(
        &查(单判, &改("window_bounded", None)),
        "window_bounded"
    ));
    assert!(!点名未测(
        &查(单判, &改("window_bounded", Some(json!(false)))),
        "window_bounded"
    ));
}

/// H7：`questions_free` 未测时融合开、成本估计上界按 `false` 计，并报 `W-untested` 点名字段。
#[test]
#[ignore = "降级落在步 22（Estimate、select_within）与 23a/23b（plan 估计）"]
fn h7_题近乎免费() {
    assert!(点名未测(
        &查(单判, &改("questions_free", None)),
        "questions_free"
    ));
}

/// H8 一对象（步 24d，J-14 读画像，B62）：`multi_object_crosstalk` 未测或 `true`：J-14 一对象 error，
/// 未测点名字段；`false`：降 warn。
#[test]
fn h8_一对象() {
    let src = r#"
budget {calls: 2, cost: 1};
let r = cut(judge(state([mat("甲"), mat("乙"), mat("丙")]), test("都在吗", "k")));
consume(r, "drop");
"#;
    let 未测 = 查(src, &改("multi_object_crosstalk", None));
    assert_eq!(未测.find("J-14").unwrap().severity, Severity::Error);
    assert!(点名未测(&未测, "multi_object_crosstalk"));
    let 假 = 查(src, &改("multi_object_crosstalk", Some(json!(false))));
    assert_eq!(假.find("J-14").unwrap().severity, Severity::Warning);
}

/// H8 去主张渲染（B126，步 25）：`assertion_susceptible: false` 时 `strip_assertions` 为空操作；
/// 未测按 B39 照常派发。
#[test]
#[ignore = "B126：渲染半落步 25（strip_assertions 读画像）"]
fn h8_去主张渲染() {
    let src = r#"
budget {calls: 2, cost: 1};
strip_assertions(mat("材料"))
"#;
    let 反值 = 查(src, &改("assertion_susceptible", Some(json!(false))));
    assert!(反值.is_ok(), "{}", 反值.render());
    assert!(
        反值
            .diagnostics
            .iter()
            .any(|d| d.message.contains("空操作")),
        "{}",
        反值.render()
    );
}

// ---------- (C) 数值字段各未测一次（不计入八条） ----------

/// δ：记录没有 δ 的上岗线，出口 `Unsure(untested)`（步 15d-2）
#[test]
fn c_delta未测出口未决() {
    use jpp::effects::{CalibStore, FnPort, JudgeResult, Ports};
    use jpp::ledger::Ledger;
    use jpp::value::Answer;
    let program = lower(&parse(单判).unwrap()).unwrap();
    let mut calib = CalibStore::new();
    calib.put("k", 0.6, 0.3, 50, "上岗", None).unwrap();
    let ports = Ports::new().with(FnPort::judge("m", |_s, qs| {
        Ok(JudgeResult {
            answers: qs.iter().map(|_| Answer::Noul(0.99)).collect(),
            tokens: 0,
            cost: 0.0,
            mode_share: vec![None; qs.len()],
            perms: vec![0; qs.len()],
        })
    }));
    let mut l = Ledger::new();
    let o = jpp::run(&program, ports, &calib, &jpp::ActionRegistry::new(), &mut l).unwrap();
    let ex = serde_json::to_string(&o.exits).unwrap();
    assert!(ex.contains("untested"), "{ex}");
}

fn 用例名(n: &str) -> Json {
    用例().into_iter().find(|c| c["name"] == n).unwrap()
}

/// 窗口：画像没有窗口时每个判断站点报 `W-window-untested`；有窗口不报（步 15d）
#[test]
fn c_窗口未测报告() {
    let d = 临时("c-window");
    let c = 用例名("sieve");
    let 有 = d.join("有.json");
    std::fs::write(&有, serde_json::to_vec(&发行画像()).unwrap()).unwrap();
    let mut j = 发行画像();
    j.as_object_mut().unwrap().remove("window");
    let 无 = d.join("无.json");
    std::fs::write(&无, serde_json::to_vec(&j).unwrap()).unwrap();
    let w = |r: &Json| {
        r["trace"]["warnings"]
            .to_string()
            .contains("W-window-untested")
    };
    let r有 = 跑用例(&c, &d.join("有"), &有, None, false);
    let r无 = 跑用例(&c, &d.join("无"), &无, None, false);
    assert!(r有.ok && r无.ok, "{}\n{}", r有.错, r无.错);
    assert!(!w(&r有.报告) && w(&r无.报告));
}

/// 并发：未测取 1（步 15e；窗口行为本身由 `bypass_15e_window.rs` 钉住）
#[test]
fn c_并发未测取一() {
    let mut j = 发行画像();
    j["concurrency"]
        .as_object_mut()
        .unwrap()
        .remove("lower_bound_ok");
    assert_eq!(画像(&j).concurrency(), None);
    assert_eq!(画像(&发行画像()).concurrency(), Some(32));
}

/// 时延：程序声明 `budget.latency_p95`、画像没有 p95 时检查器报 `W-untested`（B32）
#[test]
fn c_时延未测报告() {
    let src = r#"
budget {calls: 4, cost: 0, depth: 8, latency_p95: 5};
let r = cut(judge(state(mat("材料")), test("行吗", "k")));
consume(r, "drop");
"#;
    let mut j = 发行画像();
    j["concurrency"]
        .as_object_mut()
        .unwrap()
        .remove("latency_s");
    let 无 = 查(src, &画像(&j));
    assert!(
        无.diagnostics
            .iter()
            .any(|d| d.rule == "W-untested" && d.message.contains("p95")),
        "{}",
        无.render()
    );
    let 有 = 查(src, &画像(&发行画像()));
    assert!(
        !有.diagnostics
            .iter()
            .any(|d| d.rule == "W-untested" && d.message.contains("p95"))
    );
}

fn 替身画像() -> PathBuf {
    root().join("tests/profile_swap/stub-0.json")
}

/// 价格：注册后端的画像没有价格，报告报 `W-cost-unknown`（B73）；替身画像给价格 0 不报
#[test]
fn c_价格未测报告() {
    let d = 临时("c-price");
    let c = 用例名("sieve");
    let mut j: Json = serde_json::from_slice(&std::fs::read(替身画像()).unwrap()).unwrap();
    j.as_object_mut().unwrap().remove("cost");
    let 无价 = d.join("无价.json");
    std::fs::write(&无价, serde_json::to_vec(&j).unwrap()).unwrap();
    let w = |r: &Json| {
        r["trace"]["warnings"]
            .to_string()
            .contains("W-cost-unknown")
    };
    let r有 = 跑用例(&c, &d.join("有"), &替身画像(), Some("stub"), false);
    let r无 = 跑用例(&c, &d.join("无"), &无价, Some("stub"), false);
    assert!(r有.ok && r无.ok, "{}\n{}", r有.错, r无.错);
    assert!(!w(&r有.报告) && w(&r无.报告));
    // 替身没有网络传输：不报 transport.timeout_s 未测（15g-0 的 `transport` 字段）
    assert!(
        !r有.报告["trace"]["warnings"]
            .to_string()
            .contains("transport.timeout_s")
    );
}

/// `k_limit` 未测：候选超已知上限时报 `W-klimit-untested`、不下沉
#[test]
#[ignore = "落在 23b（lower pass）"]
fn c_k上限未测报告() {
    let src = r#"
budget {calls: 2, cost: 1};
let e = cut(judge(state(mat("哪个好"), {over: [mat("甲"), mat("乙"), mat("丙")]}), select("选一个", "k")));
consume(e, "drop");
"#;
    assert!(
        查(src, &改("k_limit", None))
            .find("W-klimit-untested")
            .is_some()
    );
}

// ---------- (D) 第二判断器 ----------

/// (D1) 程序不改在替身判断器上跑：不带夹具、不带校准，全部出口未决（重认前全部冷出口是预期）
#[test]
fn d1_替身判断器跑全部示例() {
    let d = 临时("d1");
    let mut 跑完 = vec![];
    let mut 停: BTreeMap<String, String> = BTreeMap::new();
    for c in 用例() {
        if c["resume_from"].is_string() {
            continue;
        }
        let name = c["name"].as_str().unwrap().to_string();
        let r = 跑用例(&c, &d, &替身画像(), Some("stub"), false);
        if r.ok {
            assert_eq!(r.报告["backend"], "stub-0", "{name}");
            assert!(
                已决出口(&r.报告).is_empty(),
                "{name}：没有校准，出口应全部未决：{:?}",
                已决出口(&r.报告)
            );
            跑完.push(name);
        } else {
            停.insert(name, r.错);
        }
    }
    let 停名: Vec<&str> = 停.keys().map(|s| s.as_str()).collect();
    // 预注册预测停 4 个（三个调 gen、pair-team 下标越界）；实测多一个 `partial`：全部冷出口时它有一条
    // 未消费的 unsure，运行期 J-05（程序自身对冷出口的处理，不是替身的问题；过程记录 §三）
    assert_eq!(
        停名,
        vec![
            "lifecycle",
            "pair-team",
            "partial",
            "sieve-review",
            "unsure-causes"
        ],
        "跑完 {}：{跑完:?}；停：{停:#?}",
        跑完.len()
    );
    for g in ["lifecycle", "sieve-review", "unsure-causes"] {
        assert!(停[g].contains("stub 判断器不生成"), "{g}：{}", 停[g]);
    }
    assert!(
        停["pair-team"].contains("E-rt-index"),
        "{}",
        停["pair-team"]
    );
    assert!(停["partial"].contains("J-05"), "{}", 停["partial"]);
    // 步 23c 新增两个金样用例（seq-wrapped-old/new），都跑得完：23 → 25
    assert_eq!(跑完.len(), 25);
}

/// (D2) 校准不跨判断器（`12` B60：校准键含 `model`）：带 jev 校准记录的用例在替身判断器上跑，
/// 出口不能用 jev 的线判成已决。20a-2 删 B127 过渡守卫后取消 ignore，作其门禁。
#[test]
#[ignore = "待 20a-2：校准键含 model（B60）"]
fn d2_校准不跨判断器() {
    let d = 临时("d2");
    let mut 借线 = BTreeMap::new();
    let mut 带校准 = 0;
    for c in 用例() {
        if !c["calib"].is_string() {
            continue;
        }
        带校准 += 1;
        let name = c["name"].as_str().unwrap().to_string();
        let r = 跑用例(&c, &d, &替身画像(), Some("stub"), true);
        assert!(r.ok, "{name}：{}", r.错);
        let 已决 = 已决出口(&r.报告);
        if !已决.is_empty() {
            借线.insert(name, 已决);
        }
    }
    // 预注册写 7 个，数错了：`question-forms@truth`、`sieve@truth`、`topic-relevance`、`truth-pending` 与五个 `bank-*`，共 9 个
    assert_eq!(带校准, 9);
    assert!(借线.is_empty(), "替身判断器借用了 jev 的校准线：{借线:?}");
}

/// (D2') B127 过渡守卫（20a-2 合入前）：`stub` 带 `--calib` 或 `--calib-out` 报用法错误 `E-calib-model`；
/// `live` 带 `--calib` 不因此被拒（`fixed` 带 `--calib` 由 (A) 节覆盖）；不带校准照跑。`--calib-out`
/// 一并守住是主会话 2026-09-25 的决定（待 Fable 确认）。20a-2 删守卫时删本测试、取消上一条的 ignore。
#[test]
fn d2_过渡守卫拒绝借线() {
    let d = 临时("d2-guard");
    let src = root()
        .join("examples/topic-relevance.jpp")
        .display()
        .to_string();
    let calib = root().join("tests/golden/_calib/b24").display().to_string();
    let 替身 = 替身画像().display().to_string();
    let 跑 = |extra: &[&str]| {
        let mut a: Vec<String> = vec!["run".into(), src.clone()];
        a.extend(extra.iter().map(|s| s.to_string()));
        let o = jpp(&d, &a);
        (
            o.status.success(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    };
    let (ok, e) = 跑(&["--backend", "stub", "--profile", &替身, "--calib", &calib]);
    assert!(!ok && e.contains("E-calib-model"), "{e}");
    let out = d.join("out").display().to_string();
    let (ok, e) = 跑(&["--backend", "stub", "--profile", &替身, "--calib-out", &out]);
    assert!(!ok && e.contains("E-calib-model"), "{e}");
    let (_, e) = 跑(&["--backend", "live", "--calib", &calib]);
    assert!(!e.contains("E-calib-model"), "{e}");
    let (ok, e) = 跑(&["--backend", "stub", "--profile", &替身]);
    assert!(ok, "不带校准照跑：{e}");
}
