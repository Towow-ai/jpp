//! 步 14b-0：宿主入口的 CLI 半 `--input`（规划建议 6，`地基/附注/2026-09-24-仪表读数1裁定.md` §六·1；
//! 预注册见 `地基/过程记录/工程-步14b-0.md` §一第 8 条）。
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-entry-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn jpp(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .env("HOME", dir)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

fn read(dir: &Path, f: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(f)).unwrap()).unwrap()
}

fn header(dir: &Path, f: &str) -> Value {
    let text = fs::read_to_string(dir.join(f)).unwrap();
    serde_json::from_str(text.lines().next().unwrap()).unwrap()
}

const 取值: &str = "budget {calls: 0, cost: 0};\n{a: input.a, n: input.n + 1}\n";

/// (a) 绑定：`input.x` 取得到值。叶子 untrusted 由 (g) 经 J-08 核（`taint` 内置只收未决责任与出口）。
#[test]
fn input绑定取值() {
    let d = scratch("bind");
    fs::write(d.join("p.jpp"), 取值).unwrap();
    fs::write(d.join("in.json"), r#"{"a": "hello", "n": 3}"#).unwrap();
    let (ok, _, err) = jpp(
        &d,
        &["run", "p.jpp", "--input", "in.json", "--output", "r.json"],
    );
    assert!(ok, "{err}");
    let r = read(&d, "r.json");
    assert_eq!(r["value"]["a"], json!("hello"));
    assert_eq!(r["value"]["n"], json!(4));
    let _ = fs::remove_dir_all(&d);
}

/// (c) 账本头 `entry_hash`：按规范化 JSON 取，键序与空白不同的同一 JSON 哈希相同；不带 `--input` 为 null。
#[test]
fn 账本头记entry_hash_只看规范化内容() {
    let d = scratch("hash");
    fs::write(d.join("p.jpp"), 取值).unwrap();
    fs::write(d.join("a.json"), r#"{"a": "hello", "n": 3}"#).unwrap();
    fs::write(d.join("b.json"), "{\n  \"n\" : 3,\n  \"a\":\"hello\"\n}\n").unwrap();
    for (f, led) in [("a.json", "la.jsonl"), ("b.json", "lb.jsonl")] {
        let (ok, _, err) = jpp(
            &d,
            &[
                "run",
                "p.jpp",
                "--input",
                f,
                "--ledger-out",
                led,
                "--output",
                "r.json",
            ],
        );
        assert!(ok, "{err}");
    }
    let (ha, hb) = (header(&d, "la.jsonl"), header(&d, "lb.jsonl"));
    let h = ha["header"]["compared"]["entry_hash"].clone();
    assert!(
        h.as_str().is_some_and(|s| s.len() == 24),
        "entry_hash 要有值：{ha}"
    );
    assert_eq!(
        h, hb["header"]["compared"]["entry_hash"],
        "键序与空白不改变 entry_hash"
    );
    // 不带 --input：头字段仍为 null（金样不变的依据）
    fs::write(d.join("q.jpp"), "budget {calls: 0, cost: 0};\n1\n").unwrap();
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "q.jpp",
            "--ledger-out",
            "lq.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(ok, "{err}");
    assert_eq!(
        header(&d, "lq.jsonl")["header"]["compared"]["entry_hash"],
        Value::Null
    );
    let _ = fs::remove_dir_all(&d);
}

const 判断: &str = "budget {calls: 2, cost: 1};\nlet e = cut(judge(state(mat(input.a)), test(\"行吗\", \"k\")));\nhandle(e, {act: fn() { \"是\" }, ignore: fn() { \"否\" }, unsure: fn(u) { consume(u, \"drop\"); \"未决\" }})\n";

fn 夹具(d: &Path) {
    fs::write(
        d.join("fx.json"),
        // 步 15d-2：上岗夹具线要显式给 δ，否则出口 Unsure(untested)；这里补 test 题式的代码兜底值 0.05
        json!({"calibrations": [{"key": "k", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗", "delta": 0.05}],
               "observations": [{"on": ["hello"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.9}},
                                {"on": ["bye"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.1}}]})
        .to_string(),
    )
    .unwrap();
}

/// (d) 重放：同一 `--input` 只凭账本重放 0 调用、报告值相同；换一份输入报 `W-header: entry_hash`。
#[test]
fn 重放要同一份输入() {
    let d = scratch("replay");
    fs::write(d.join("p.jpp"), 判断).unwrap();
    fs::write(d.join("a.json"), r#"{"a": "hello"}"#).unwrap();
    fs::write(d.join("b.json"), r#"{"a": "bye"}"#).unwrap();
    夹具(&d);
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--input",
            "a.json",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r1.json",
        ],
    );
    assert!(ok, "{err}");
    let (ok, _, err) = jpp(
        &d,
        &[
            "run", "p.jpp", "--input", "a.json", "--replay", "l.jsonl", "--output", "r2.json",
        ],
    );
    assert!(ok, "{err}");
    let (r1, r2) = (read(&d, "r1.json"), read(&d, "r2.json"));
    assert_eq!(r1["cost"]["calls"], json!(1));
    assert_eq!(r2["cost"]["calls"], json!(0), "重放不发调用");
    assert_eq!(r2["value"], r1["value"]);
    assert_eq!(r1["value"], json!("是"));
    // 换一份输入：头不同报 W-header；材料内容变了，账本里没有这道判断的记录
    let (_, _, err) = jpp(
        &d,
        &[
            "run", "p.jpp", "--input", "b.json", "--replay", "l.jsonl", "--output", "r3.json",
        ],
    );
    assert!(
        err.contains("E-replay") || d.join("r3.json").exists(),
        "{err}"
    );
    let (_, _, err2) = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--input",
            "b.json",
            "--resume",
            "l.jsonl",
            "--fixtures",
            "fx.json",
            "--output",
            "r4.json",
        ],
    );
    let r4 = read(&d, "r4.json");
    let ws = r4["trace"]["warnings"].to_string();
    assert!(
        ws.contains("W-header") && ws.contains("entry_hash"),
        "续接换了输入要报 W-header: entry_hash：{ws}\n{err2}"
    );
    assert_eq!(r4["value"], json!("否"));
    let _ = fs::remove_dir_all(&d);
}

/// (e) 不带 `--input` 时用到 `input` 报 `E-name`（行为不变）；`check --input` 通过。
#[test]
fn 不带input时报e_name_check也收input() {
    let d = scratch("name");
    fs::write(d.join("p.jpp"), 取值).unwrap();
    fs::write(d.join("in.json"), r#"{"a": "x", "n": 1}"#).unwrap();
    let (ok, _, err) = jpp(&d, &["run", "p.jpp"]);
    assert!(
        !ok && err.contains("E-name") && err.contains("input"),
        "{err}"
    );
    let (ok, _, err) = jpp(&d, &["check", "p.jpp"]);
    assert!(!ok && err.contains("E-name"), "{err}");
    let (ok, out, err) = jpp(&d, &["check", "p.jpp", "--input", "in.json"]);
    assert!(ok, "{out}{err}");
    // 程序自己定义 input：遮蔽宿主绑定，照常运行
    fs::write(
        d.join("own.jpp"),
        "budget {calls: 0, cost: 0};\nlet input = {a: \"own\"};\ninput.a\n",
    )
    .unwrap();
    let (ok, _, err) = jpp(
        &d,
        &["run", "own.jpp", "--input", "in.json", "--output", "r.json"],
    );
    assert!(ok, "{err}");
    assert_eq!(read(&d, "r.json")["value"], json!("own"));
    let _ = fs::remove_dir_all(&d);
}

/// (f) 输入文件不是合法 JSON：运行前报错并带路径；整数越界同 `read_json` 的校验。
#[test]
fn 输入不合法时报错带路径() {
    let d = scratch("bad");
    fs::write(d.join("p.jpp"), 取值).unwrap();
    fs::write(d.join("bad.json"), "{not json").unwrap();
    fs::write(
        d.join("big.json"),
        r#"{"a": "x", "n": 18446744073709551615}"#,
    )
    .unwrap();
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--input", "bad.json"]);
    assert!(!ok && err.contains("bad.json"), "{err}");
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--input", "big.json"]);
    assert!(
        !ok && err.contains("big.json") && err.contains("Int range"),
        "{err}"
    );
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--input", "missing.json"]);
    assert!(!ok && err.contains("missing.json"), "{err}");
    let _ = fs::remove_dir_all(&d);
}

/// (g) 以 `input` 派生的材料为唯一守卫的不可逆 `do` 被 J-08 拦下（与 `read_json` 同一结论）；同内容字面量不拦。
#[test]
fn input派生的守卫放行不了不可逆do() {
    let d = scratch("j08");
    let prog = |mat_expr: &str| {
        format!(
            "budget {{calls: 2, cost: 1}};\nlet ok = handle(cut(judge(state(mat({mat_expr})), test(\"行吗\", \"k\"))), {{\n    act: fn() {{ true }}, ignore: fn() {{ false }}, unsure: fn(u) {{ consume(u, \"drop\"); false }}}});\nif ok {{ content(do(\"write_json\", [\"out.json\", {{done: true}}], 0)) }} else {{ \"没写\" }}\n"
        )
    };
    fs::write(d.join("in.jpp"), prog("input.a")).unwrap();
    fs::write(d.join("lit.jpp"), prog("\"hello\"")).unwrap();
    fs::write(d.join("in.json"), r#"{"a": "hello"}"#).unwrap();
    夹具(&d);
    // 线走 --calib 目录（不是夹具线：夹具线本身不算可信合取项，B29），只剩 taint 一项决定放不放行
    // 读数取认证集里出现过的 0.95：固定序（B86，步 20g）平局取严，读数最高一组 0.95 以下、0.05 以上是没有标签的空白，线不放宽进去
    fs::write(d.join("nofx.json"), json!({"observations": [{"on": ["hello"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.95}}]}).to_string()).unwrap();
    // 正式线：经真值通道认证（没有证书的线是夹具线，B29）。可算真值 200 行，两侧零错
    let labels: String = (0..200)
        .map(|i| {
            let (p, l) = if i % 2 == 0 {
                (0.95, true)
            } else {
                (0.05, false)
            };
            // B104-2（步 20h-1）：带材料文本，记录才有范围指纹
            json!({"key": "k", "item": format!("i{i}"), "p": p, "label": l, "source": "computed", "text": "hello"})
                .to_string()
                + "\n"
        })
        .collect();
    fs::write(d.join("labels.jsonl"), labels).unwrap();
    let (ok, out, err) = jpp(
        &d,
        &[
            "calib-import",
            "labels.jsonl",
            "--calib-out",
            "calib",
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ],
    );
    assert!(ok, "认证失败：{out}{err}");
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "in.jpp",
            "--input",
            "in.json",
            "--fixtures",
            "nofx.json",
            "--calib",
            "calib",
            // 步 18b：有不可逆 do 的程序要给 --ledger-out（E-ledger-required）
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(
        !ok && err.contains("J-08"),
        "宿主输入派生的守卫不能放行不可逆 do：{err}"
    );
    assert!(!d.join("out.json").exists(), "不可逆 do 被执行了");
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "lit.jpp",
            "--input",
            "in.json",
            "--fixtures",
            "nofx.json",
            "--calib",
            "calib",
            // 步 18b：有不可逆 do 的程序要给 --ledger-out（E-ledger-required）
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(ok, "字面量守卫照常放行：{err}");
    assert!(d.join("out.json").exists());
    let _ = fs::remove_dir_all(&d);
}

/// (m) 步 14b-1（B108）：`--input-trusted` 让 (g) 里同一份被 J-08 拒的程序改为放行；
/// 报告 `entry` 段记 `taint: "trusted"`。复用 (g) 的守卫程序与同一份正式线（200 行可算真值）。
#[test]
fn m_input_trusted让静态j08也放行() {
    let d = scratch("j08-trusted");
    let prog = "budget {calls: 2, cost: 1};\nlet ok = handle(cut(judge(state(mat(input.a)), test(\"行吗\", \"k\"))), {\n    act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, \"drop\"); false }});\nif ok { content(do(\"write_json\", [\"out.json\", {done: true}], 0)) } else { \"没写\" }\n";
    fs::write(d.join("in.jpp"), prog).unwrap();
    fs::write(d.join("in.json"), r#"{"a": "hello"}"#).unwrap();
    fs::write(d.join("nofx.json"), json!({"observations": [{"on": ["hello"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.95}}]}).to_string()).unwrap();
    let labels: String = (0..200)
        .map(|i| {
            let (p, l) = if i % 2 == 0 {
                (0.95, true)
            } else {
                (0.05, false)
            };
            json!({"key": "k", "item": format!("i{i}"), "p": p, "label": l, "source": "computed", "text": "hello"})
                .to_string()
                + "\n"
        })
        .collect();
    fs::write(d.join("labels.jsonl"), labels).unwrap();
    let (ok, out, err) = jpp(
        &d,
        &[
            "calib-import",
            "labels.jsonl",
            "--calib-out",
            "calib",
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ],
    );
    assert!(ok, "认证失败：{out}{err}");
    // 未声明可信：与 (g) 同一结论，J-08 拦下，不可逆 do 不执行
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "in.jpp",
            "--input",
            "in.json",
            "--fixtures",
            "nofx.json",
            "--calib",
            "calib",
            // 步 18b：有不可逆 do 的程序要给 --ledger-out（E-ledger-required）
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(!ok && err.contains("J-08"), "{err}");
    assert!(!d.join("out.json").exists());
    // 声明可信：静态与运行期 J-08 都放行，不可逆 do 执行
    let (ok, _, err) = jpp(
        &d,
        &[
            "run",
            "in.jpp",
            "--input",
            "in.json",
            "--input-trusted",
            "--fixtures",
            "nofx.json",
            "--calib",
            "calib",
            // 步 18b：有不可逆 do 的程序要给 --ledger-out（E-ledger-required）
            "--ledger-out",
            "l.jsonl",
            "--output",
            "r.json",
        ],
    );
    assert!(ok, "声明可信、线经认证：应放行：{err}");
    assert!(d.join("out.json").exists(), "不可逆 do 应当执行");
    let r = read(&d, "r.json");
    assert_eq!(
        r["entry"],
        json!([{"name": "input", "kind": "value", "taint": "trusted"}]),
        "{r}"
    );
    let _ = fs::remove_dir_all(&d);
}

/// (n) `check`/`run` 的 `--input-trusted` 缺 `--input`：`options::parse` 报一行用法错误
/// （CLI 进程的 stderr 另外恒定追加 `HELP`，逐字节测在 `cli/options.rs` 的单元测试
/// `input_trusted_requires_input` 里按 `parse()` 的返回值直接测，这里只核对子进程确实失败、
/// 错误提到两个开关名字）。
#[test]
fn n_check的input_trusted缺input时报用法错误() {
    let d = scratch("j08-trusted-usage");
    fs::write(d.join("p.jpp"), 取值).unwrap();
    let (ok, _, err) = jpp(&d, &["check", "p.jpp", "--input-trusted"]);
    assert!(!ok, "应当失败");
    assert!(
        err.contains("--input-trusted") && err.contains("--input"),
        "{err}"
    );
    let (ok, _, err) = jpp(&d, &["run", "p.jpp", "--input-trusted"]);
    assert!(!ok, "应当失败");
    assert!(
        err.contains("--input-trusted") && err.contains("--input"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&d);
}
