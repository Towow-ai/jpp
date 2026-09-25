use std::{fs, process::Command};

#[test]
fn source_cli_parses_methods_and_renders_user_errors() {
    let root = std::env::temp_dir().join(format!("jpp-cli-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let good = root.join("methods.jpp");
    fs::write(&good, "fn twice(x: Int) -> Int { x * 2 }\ntwice(21)").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .args(["parse", good.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bad = root.join("bad.jpp");
    fs::write(&bad, "let x = 1;\nlet y = ;").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .args(["parse", bad.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bad.jpp:2:9: expected an expression"),
        "{stderr}"
    );
    assert!(stderr.contains("let y = ;"));
    fs::remove_dir_all(root).unwrap();
}
