//! **越界接线的验收**（CLI 侧三行；随时可 `git revert`）。
//!
//! 这三件的内核侧早就就位、也有测试，**而 CLI 这一侧没有入口，于是三件在真实路径上
//! 全都够不着**——按我们自己的判别法，**把消费方删掉还绿，因为本来就没有消费者**。

use std::{fs, path::PathBuf, process::Command};

fn 根() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn 临时(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-wiring-{}-{名}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    d
}
fn 跑(args: &[&str]) -> (String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .args(args)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

/// **G3「换版本程序不改」的第一条可执行证据。**
///
/// 同一份 `.jpp`、两份**只差 `arithmetic_capable` 一个字段**的档案，
/// **CLI 跑出不同的 J-01 严重度，而程序一个字不改**（`12` §1.3 的降级表）。
/// **这条断言在接线之前写不出来**，因为 CLI 根本没有 `--profile`。
#[test]
fn 同一个程序两份档案两种严重度() {
    let d = 临时("g3");
    fs::write(d.join("p.jpp"), "budget {calls: 2, cost: 0};\nlet r = judge(state(mat(\"材料\")), test(\"行吗\", \"k\"));\nr + 1\n").unwrap();
    let 真档 = fs::read_to_string(根().join("../src/foundation/profile/profiles/jev-1.13.0.json"))
        .unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&真档).unwrap();
    for (名, 值) in [("成立", false), ("不成立", true)] {
        j["arithmetic_capable"] = serde_json::json!(值);
        fs::write(
            d.join(format!("{名}.json")),
            serde_json::to_string(&j).unwrap(),
        )
        .unwrap();
    }
    let p = d.join("p.jpp");
    let (_, e1) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--profile",
        d.join("成立.json").to_str().unwrap(),
    ]);
    let (_, e2) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--profile",
        d.join("不成立.json").to_str().unwrap(),
    ]);

    assert!(
        e1.contains("J-01") && !e1.contains("降 warn"),
        "H5 成立 → 挡住：{e1}"
    );
    assert!(
        e2.contains("J-01") && e2.contains("降 warn"),
        "**同一个程序，档案说模型会算术就降 warn**：{e2}"
    );
    assert!(
        e2.contains("arithmetic_capable"),
        "要说出是哪个字段让它降的：{e2}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// **`--calib` 喂一条带 `unsure_rate` 的记录，`union_bound` 不再恒等于 `n`。**
/// 第二个作者实测「在 `.jpp` + CLI 这条路上 `unsure_bound` 不是判据，是常量」——**必红**。
#[test]
fn calib目录让unsure_bound不再恒等于n() {
    let d = 临时("calib");
    let cd = d.join("calib");
    fs::create_dir_all(&cd).unwrap();
    fs::write(
        cd.join("k.json"),
        r#"{"key":"k","hi":0.75,"lo":0.25,"n":60,"status":"上岗","unsure_rate":0.1,"set_id":"s1"}"#,
    )
    .unwrap();
    fs::write(d.join("u.jpp"), "budget {calls: 4, cost: 0};\nlet rs = [judge(state(mat(\"甲\")), test(\"行吗\",\"k\")), judge(state(mat(\"乙\")), test(\"行吗\",\"k\"))];\nunsure_bound(rs)\n").unwrap();
    fs::write(d.join("u.json"), r#"{"observations":[{"on":["甲"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.9}},{"on":["乙"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.5}}]}"#).unwrap();
    let (u, f) = (d.join("u.jpp"), d.join("u.json"));
    let (无, _) = 跑(&[
        "run",
        u.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
    ]);
    let (有, _) = 跑(&[
        "run",
        u.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
    ]);
    let 读 = |s: &str| -> (f64, i64) {
        let v: serde_json::Value = serde_json::from_str(s).unwrap();
        (
            v["value"]["union_bound"].as_f64().unwrap(),
            v["value"]["n_unknown"].as_i64().unwrap(),
        )
    };
    assert_eq!(
        读(&无),
        (2.0, 2),
        "没有 --calib 时它恒等于 n——第二个作者报的就是这个"
    );
    assert_eq!(读(&有), (0.2, 0), "**接上之后它是一个真的界**：{有}");
    let _ = fs::remove_dir_all(&d);
}

/// 一个参数都不给时 stdout 与接线前逐字节相同；stderr 只多一行「未加载画像」提示。
/// 步 15d-0（B73）起这行提示无条件打印：固定观察无画像时线与 δ 用的是代码兜底，每次都要看得见。
/// 步 15d-2：`Profile` 没有代码兜底默认了，提示文案改为「画像字段全部未测（线与 δ 只从校准记录取）」。
#[test]
fn 不传新参数时输出不变() {
    let d = 临时("same");
    fs::write(d.join("h.jpp"), "budget {calls: 0, cost: 0};\n1 + 1\n").unwrap();
    let (out, err) = 跑(&["run", d.join("h.jpp").to_str().unwrap()]);
    assert_eq!(
        err,
        "档案：未加载，画像字段全部未测（线与 δ 只从校准记录取）；校准记录：0 条（仅来自 --fixtures）\n",
        "stderr 只该有画像提示这一行"
    );
    assert!(out.contains("\"value\": 2"), "{out}");
    let _ = fs::remove_dir_all(&d);
}

/// **第 4 问：`--calib` 与 `--fixtures` 的 `calibrations` 谁优先——而且要说得出来。**
/// 两条都能给线，**别让它们互相盖掉而没人知道**。
#[test]
fn 夹具覆盖目录时要报出来() {
    let d = 临时("prio");
    let cd = d.join("calib");
    fs::create_dir_all(&cd).unwrap();
    // 目录里 hi=0.90；夹具里 hi=0.50 —— 夹具赢（这一次跑的显式布置优先于常备资产）
    fs::write(
        cd.join("k.json"),
        r#"{"key":"k","hi":0.90,"lo":0.10,"n":60,"status":"上岗"}"#,
    )
    .unwrap();
    fs::write(d.join("p.jpp"), "budget {calls: 2, cost: 0};\nhandle(cut(judge(state(mat(\"x\")), test(\"行吗\",\"k\"))), {act: fn(){\"act\"}, ignore: fn(){\"ig\"}, unsure: fn(u){consume(u,\"drop\");\"un\"}})\n").unwrap();
    // 步 15d-2：上岗夹具线要显式给 δ，否则出口 Unsure(untested)；补 test 题式的代码兜底值 0.05
    fs::write(d.join("f.json"), r#"{"calibrations":[{"key":"k","hi":0.50,"lo":0.10,"n":60,"status":"上岗","delta":0.05}],"observations":[{"on":["x"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.7}}]}"#).unwrap();
    let (out, err) = 跑(&[
        "run",
        d.join("p.jpp").to_str().unwrap(),
        "--fixtures",
        d.join("f.json").to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
    ]);
    assert!(
        out.contains("\"value\": \"act\""),
        "0.7 过了夹具的 0.50 → 夹具赢：{out}"
    );
    assert!(
        err.contains("覆盖") && err.contains("k"),
        "**谁盖了谁要说出来**：{err}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// **那条环第一次闭合**：跑程序 → 积累证据 → 认证 → 用上。
///
/// **J-03 决定了程序永远写不了线**，所以这条环**只能靠宿主/CLI 闭合**；
/// `--calib` 是入料，`--calib-out` 是出料，**在它之前这条环在命令行上永远闭不上**。
#[test]
fn 校准环闭合() {
    let d = 临时("loop");
    let cd = d.join("calib");
    fs::write(d.join("p.jpp"), "budget {calls: 4, cost: 0};\nhandle(cut(judge(state(mat(\"材料\")), test(\"行吗\",\"k\"))), {act: fn(){{r:\"act\", 源: line_source(e0())}}, ignore: fn(){{r:\"ignore\", 源: \"\"}}, unsure: fn(u){consume(u,\"drop\");{r: unsure_cause(u), 源: \"\"}}})\n").unwrap();
    // 上面那行用了不存在的 e0()，改写成不带 line_source 的最小形
    fs::write(d.join("p.jpp"), "budget {calls: 4, cost: 0};\nlet e = cut(judge(state(mat(\"材料\")), test(\"行吗\",\"k\")));\nhandle(e, {act: fn(){{r:\"act\", 源: line_source(e)}}, ignore: fn(){{r:\"ignore\", 源: line_source(e)}}, unsure: fn(u){consume(u,\"drop\");{r: unsure_cause(u), 源: line_source(e)}}})\n").unwrap();
    fs::write(d.join("f.json"), r#"{"observations":[{"on":["材料"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.9}}]}"#).unwrap();
    let (p, f) = (d.join("p.jpp"), d.join("f.json"));

    // 第一趟：没有线 → 冷；同时把证据写出去
    let (一, _) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib-out",
        cd.to_str().unwrap(),
    ]);
    let v1: serde_json::Value = serde_json::from_str(&一).unwrap();
    assert_eq!(v1["value"]["r"], "cold", "没有线就是冷：{一}");
    assert!(cd.join("k.json").exists(), "**证据要落盘**");
    let rec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(cd.join("k.json")).unwrap()).unwrap();
    assert_eq!(
        rec["status"], "待真值",
        "无标注观察把冷记录推到待真值，**n 不动**"
    );
    assert_eq!(rec["n"], 0);
    assert_eq!(
        rec["samples"].as_array().unwrap().len(),
        1,
        "这一趟判了一次，折进一条观察"
    );

    // 宿主侧认证（`commission` 留在宿主侧——认证是校准过程，不是程序行为）：
    // 这里直接把一条上岗线写进那个目录，模拟「校准过程给了线」
    // 步 15d-2：δ 只从校准记录取，手写线要显式给，否则出口 Unsure(untested)
    fs::write(
        cd.join("k.json"),
        r#"{"key":"k","hi":0.8,"lo":0.2,"n":50,"status":"上岗","samples":[],"delta":0.05}"#,
    )
    .unwrap();

    // 第二趟：把线读回来 → 出口变了，而且说得出线是哪来的
    let (二, _) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
    ]);
    let v2: serde_json::Value = serde_json::from_str(&二).unwrap();
    assert_eq!(
        v2["value"]["r"], "act",
        "**线上岗了，0.9 过线 → act**：{二}"
    );
    assert_eq!(
        v2["value"]["源"], "题级·手填",
        "**`line_source` 报得出线是哪来的**"
    );
    let _ = fs::remove_dir_all(&d);
}

/// 不传 `--calib-out` 时不多出校准写回的输出（`--calib-out` 这一轮加的，同一条保障）；
/// stderr 只有步 15d-0 起无条件打印的画像提示。
#[test]
fn 不传calib_out时也不多出任何输出() {
    let d = 临时("same2");
    fs::write(d.join("h.jpp"), "budget {calls: 0, cost: 0};\n1 + 1\n").unwrap();
    let (out, err) = 跑(&["run", d.join("h.jpp").to_str().unwrap()]);
    assert_eq!(
        err.lines().count(),
        1,
        "stderr 只该有画像提示这一行：{err:?}"
    );
    assert!(
        err.contains("未加载") && !err.contains("校准记录已写回"),
        "{err:?}"
    );
    assert!(out.contains("\"value\": 2"));
    let _ = fs::remove_dir_all(&d);
}

/// **真正的往返：`--calib-out` 写出来的目录，原样喂给 `--calib`，中间不改任何字节。**
///
/// **上一条 `校准环闭合` 照不出这个**：它在两趟之间插了一行手写 JSON 模拟宿主认证，
/// **于是把「出料的产物能不能被入料读回去」这个断言一起抹掉了**
/// ——**而那正是那一包唯一新增的东西**。
/// **一个「写出来 → 读回去」的验收，中间不许有任何一行去改那个文件。**
#[test]
fn calib_out的产物能原样喂回calib() {
    let d = 临时("rt");
    let cd = d.join("calib");
    fs::write(d.join("p.jpp"), "budget {calls: 4, cost: 0};\nlet e = cut(judge(state(mat(\"材料\")), test(\"行吗\",\"k\")));\nhandle(e, {act: fn(){\"act\"}, ignore: fn(){\"ignore\"}, unsure: fn(u){consume(u,\"drop\");unsure_cause(u)}})\n").unwrap();
    fs::write(d.join("f.json"), r#"{"observations":[{"on":["材料"],"op":"test","text":"行吗","calib":"k","answer":{"Noul":0.9}}]}"#).unwrap();
    let (p, f) = (d.join("p.jpp"), d.join("f.json"));

    let (_, e1) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib-out",
        cd.to_str().unwrap(),
    ]);
    assert!(cd.join("k.json").exists(), "第一趟要落盘：{e1}");
    let 原样 = fs::read_to_string(cd.join("k.json")).unwrap();

    // **中间一个字节都不改**，直接喂回去
    let (out2, err2) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
    ]);
    assert!(
        !err2.contains("不认得的字段"),
        "**写出来的必须读得回去**：{err2}"
    );
    assert!(out2.contains("\"value\""), "第二趟要跑完：{out2} / {err2}");
    assert_eq!(
        fs::read_to_string(cd.join("k.json")).unwrap(),
        原样,
        "读那一趟不该改动它"
    );

    // 再跑一趟 out → 仍读得回去（**累积之后也要往返**）
    let (_, _) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
        "--calib-out",
        cd.to_str().unwrap(),
    ]);
    let (_, err3) = 跑(&[
        "run",
        p.to_str().unwrap(),
        "--fixtures",
        f.to_str().unwrap(),
        "--calib",
        cd.to_str().unwrap(),
    ]);
    assert!(
        !err3.contains("不认得的字段"),
        "累积一轮之后仍要读得回去：{err3}"
    );
    let _ = fs::remove_dir_all(&d);
}
