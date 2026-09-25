use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jpp-library-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
    fn call(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn run(&self, args: &[&str]) -> Value {
        let output = self.call(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn library_methods_return_store_and_call_typed_methods() {
    let temp = Temp::new();
    let source = root().join("examples/library-methods.jpp");
    let result = temp.run(&["run", source.to_str().unwrap()]);
    assert_eq!(result["value"], json!({"results":[4,6,42],"direct":42}));
    assert_eq!(result["cost"]["calls"], 0);
}

#[test]
fn imports_are_relative_and_shared_dependencies_load_once() {
    let temp = Temp::new();
    temp.write("base.jpp", "fn inc(x) { x + 1 }");
    temp.write("left.jpp", "import \"base.jpp\"; fn left(x) { inc(x) }");
    temp.write(
        "right.jpp",
        "import \"./base.jpp\"; fn right(x) { inc(inc(x)) }",
    );
    temp.write(
        "main.jpp",
        "import \"left.jpp\"; import \"right.jpp\"; budget {calls:0,cost:0}; left(right(10))",
    );
    let parsed = temp.call(&["parse", "main.jpp"]);
    assert!(String::from_utf8_lossy(&parsed.stdout).contains("3 statements"));
    assert_eq!(temp.run(&["run", "main.jpp"])["value"], 13);
}

#[test]
fn imported_static_and_runtime_errors_keep_their_file_and_line() {
    let temp = Temp::new();
    temp.write(
        "main.jpp",
        "import \"helper.jpp\"; budget {calls:0,cost:0}; problem(0)",
    );
    temp.write(
        "helper.jpp",
        "// 中文注释\nfn problem(x) { missing_name(x) }",
    );
    let checked = temp.call(&["check", "main.jpp"]);
    assert!(!checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stderr).contains("helper.jpp:2:"));
    temp.write("helper.jpp", "// 中文注释\nfn problem(x) { 10 / x }");
    let ran = temp.call(&["run", "main.jpp"]);
    assert!(!ran.status.success());
    assert!(String::from_utf8_lossy(&ran.stderr).contains("helper.jpp:2:"));
}

#[test]
fn import_cycles_conflicts_and_library_entrypoints_are_reported() {
    let temp = Temp::new();
    temp.write("main.jpp", "import \"a.jpp\"; budget {calls:0,cost:0}; 1");
    temp.write("a.jpp", "import \"main.jpp\";");
    assert!(
        String::from_utf8_lossy(&temp.call(&["check", "main.jpp"]).stderr).contains("import cycle")
    );
    temp.write("a.jpp", "budget {calls:0,cost:0}; 1");
    assert!(
        String::from_utf8_lossy(&temp.call(&["check", "main.jpp"]).stderr)
            .contains("a library contains declarations")
    );
    temp.write("a.jpp", "fn duplicate(x) { x }");
    temp.write(
        "main.jpp",
        "import \"a.jpp\"; budget {calls:0,cost:0}; fn duplicate(x) { x } duplicate(1)",
    );
    assert!(
        String::from_utf8_lossy(&temp.call(&["check", "main.jpp"]).stderr)
            .contains("already declared")
    );
}

#[test]
fn generation_judgment_pending_response_and_file_actions_share_one_ledger() {
    let temp = Temp::new();
    temp.write("input.json", r#"{"goal":"a reusable plan"}"#);
    let source = root().join("examples/lifecycle.jpp");
    let fixture = root().join("examples/fixtures/lifecycle.json");
    let response = root().join("examples/fixtures/lifecycle-response.json");
    let first = temp.run(&[
        "run",
        source.to_str().unwrap(),
        "--fixtures",
        fixture.to_str().unwrap(),
        "--ledger-out",
        "pending.json",
    ]);
    assert_eq!(first["status"], "pending");
    assert_eq!(
        first["cost"]["calls"], 4,
        "判断、生成、do、ask 各一次（B38）"
    );
    assert_eq!(first["pending"][0]["cause"], "ask");
    assert!(!temp.0.join("result.json").exists());
    // Resume must retain the input snapshot even when the original file is gone.
    fs::remove_file(temp.0.join("input.json")).unwrap();
    let resumed = temp.run(&[
        "run",
        source.to_str().unwrap(),
        "--fixtures",
        response.to_str().unwrap(),
        "--resume",
        "pending.json",
        "--ledger-out",
        "complete.json",
    ]);
    assert_eq!(resumed["status"], "returned");
    assert_eq!(
        resumed["cost"]["calls"], 2,
        "续跑新执行 do、ask 各一次（B38）"
    );
    assert_eq!(resumed["cost"]["asks"], 1);
    assert_eq!(resumed["value"]["plan"]["cost"], 2);
    assert_eq!(resumed["value"]["request"]["goal"], "a reusable plan");
    let saved: Value =
        serde_json::from_slice(&fs::read(temp.0.join("result.json")).unwrap()).unwrap();
    assert_eq!(saved, resumed["value"]);
    temp.write("result.json", "preserve this marker");
    let replay = temp.run(&[
        "run",
        source.to_str().unwrap(),
        "--fixtures",
        response.to_str().unwrap(),
        "--replay",
        "complete.json",
    ]);
    assert_eq!(replay["value"], resumed["value"]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["cost"]["asks"], 0);
    assert_eq!(
        fs::read_to_string(temp.0.join("result.json")).unwrap(),
        "preserve this marker"
    );
}

#[test]
fn replay_rejects_unrecorded_file_writes_and_input_overflow_is_a_fail_value() {
    let temp = Temp::new();
    temp.write("empty.jpp", "budget {calls:0,cost:0}; 0");
    temp.run(&["run", "empty.jpp", "--ledger-out", "empty.json"]);
    temp.write(
        "write.jpp",
        "budget {calls:1,cost:0}; is_fail(do(\"write_json\", [\"out.json\", 42], 0))",
    );
    // 步 3（B35）：只凭账本重放遇到未记录的写文件，是重放缺记录，报 E-replay（致命），不执行动作
    let replayed = temp.call(&["run", "write.jpp", "--replay", "empty.json"]);
    assert!(!replayed.status.success());
    assert!(
        String::from_utf8_lossy(&replayed.stderr).contains("E-replay"),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert!(!temp.0.join("out.json").exists());
    temp.write("input.json", "{\"x\":18446744073709551615}");
    temp.write(
        "read.jpp",
        "budget {calls:1,cost:0}; is_fail(do(\"read_json\", [\"input.json\"], 0))",
    );
    assert_eq!(temp.run(&["run", "read.jpp"])["value"], true);
}

#[test]
fn mismatched_response_is_a_fixture_error_not_a_runtime_panic() {
    let temp = Temp::new();
    temp.write("ask.jpp", "budget {calls:0,cost:0,escalate:1}; handle(ask(state(mat(\"x\")), test(\"ok?\", \"human\")), {act:fn(){true}, ignore:fn(){false}, unsure:fn(u){{pending:u}}})");
    temp.write("bad.json", r#"{"responses":[{"on":["x"],"op":"test","text":"ok?","calib":"human","answer":{"Choice":[1.0]}}]}"#);
    let output = temp.call(&["run", "ask.jpp", "--fixtures", "bad.json"]);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("bad.json") && error.contains("requires a Noul answer"),
        "{error}"
    );
    assert!(!error.contains("panicked"), "{error}");
}
