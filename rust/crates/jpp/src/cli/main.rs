mod calib_confirm;
mod calib_import;
mod diag_json;
mod fixture;
mod options;
mod profile_resolve;
mod questions_out;
mod run_io;
mod runner;

use options::{Command, help};
use std::{env, process::ExitCode};

fn execute(command: Command, questions_out: Option<std::path::PathBuf>) -> Result<(), String> {
    let path = match &command {
        Command::Help => {
            println!("{}", help());
            return Ok(());
        }
        Command::CalibImport(a) => return calib_import::run(a),
        Command::CalibConfirm { dir, key, suspend } => {
            return calib_confirm::run(dir, key, *suspend);
        }
        Command::LedgerMigrate { from, to } => return run_io::ledger_migrate(from, to),
        Command::Parse { source, .. } | Command::Check { source, .. } => source,
        Command::Run(options) => &options.source,
    };
    let filename = path.to_string_lossy();
    let loaded = jpp_syntax::loader::load(path)?;
    let parsed = &loaded.program;
    if let Command::Parse { ast, .. } = &command {
        if *ast {
            println!("{parsed:#?}");
        } else {
            println!(
                "Parsed {filename}: {} statements and {} result expression",
                parsed.body.statements.len(),
                usize::from(parsed.body.result.is_some())
            );
        }
        return Ok(());
    }
    // 宿主入口声明（B106）：给了 `--input` 就声明一条名为 `input` 的值条目，交给 `compile`
    // 写进 `Program.entry`。只看有没有给、不读文件，保持「降级诊断先于读输入」的报错顺序。
    // taint 按 `--input-trusted` 给（步 14b-1，B108）：静态 J-08（步 24-0，`Session::go` 执行前
    // 复检）只读 `Program.entry.params[].taint`，不声明可信这里就放行不了不可逆 `do`——
    // 必须与下面读文件时建的运行期 `输入` 用同一个 taint，否则 CLI 会先撞见静态报文。
    let 给了输入 = match &command {
        Command::Run(r) => r.input.is_some(),
        Command::Check { input, .. } => input.is_some(),
        _ => false,
    };
    let 输入可信 = match &command {
        Command::Run(r) => r.input_trusted,
        Command::Check { input_trusted, .. } => *input_trusted,
        _ => false,
    };
    let 入口声明 = if 给了输入 {
        let taint = if 输入可信 {
            jpp::Taint::Trusted
        } else {
            jpp::Taint::Untrusted
        };
        jpp::EntryArgs {
            values: vec![jpp::EntryValue::new("input", serde_json::Value::Null).with_taint(taint)],
            ..Default::default()
        }
        .decl()
    } else {
        jpp::ir::EntryDecl::default()
    };
    // 降级诊断与检查诊断走同一渲染层（步 9a）：同码同址折叠，`--json` 时出机读格式
    let program = match jpp::Session::compile(parsed, &入口声明) {
        Ok(p) => p,
        Err(ds) => {
            let items: Vec<_> = ds.iter().map(diag_json::from_lower).collect();
            if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
                print_check_doc(&filename, &loaded, items);
                return Err(String::new());
            }
            return Err(diag_json::render_all(&loaded, items).join("\n"));
        }
    };
    // 步 20a-2b（B116 (5)）：`check --questions-out` 在降级成功之后、静态检查之前写字面题导出——
    // 程序有检查错误也照写（导出只读语法与字面量），降级失败则走不到这里、不写
    if let Some(out) = &questions_out {
        let (标签, 题, 跳过) = questions_out::write(&program, &loaded, path, out)?;
        eprintln!(
            "题面导出（B116）：{标签} 个标签、{题} 道题、跳过 {跳过} 处 → {}",
            out.display()
        );
    }
    // 画像只解析一次（B73）：静态检查、账本头 `profile_hash`、「档案：…」提示都用这一个结果。
    // 真机运行解析不到画像即报 `E-profile-missing`，在检查与初始化真机客户端之前。
    let 画像 = match &command {
        Command::Run(r) => profile_resolve::resolve(r)?,
        _ => None,
    };
    // 宿主入口（步 14b-0 `--input`；B105 起为一条值条目）：检查前读文件，读不成在检查前报错；
    // taint 与上面的 `入口声明` 同一个 `输入可信`（步 14b-1）
    let 输入 = match &command {
        Command::Run(r) => r.input.as_deref(),
        Command::Check { input, .. } => input.as_deref(),
        _ => None,
    }
    .map(|p| run_io::read_host_input(p, 输入可信))
    .transpose()?
    .unwrap_or_default();
    // 步 24c（B108 已知限制收口）：`check` 与 `run` 共用这一次预检查，都带上 CLI 唯一注册的
    // 三个内置动作（与用户输入无关，随时能给）——J-08 静态子面从此能对 `record_check` 这类可逆
    // 动作不报、对 `write_json` 这类不可逆动作在检查期就报 error，不必等 `Session::go` 内部
    // 真正带表的那次检查。
    let report = jpp::Session::explain_with_actions(
        &program,
        画像.as_ref().map(|p| &p.profile),
        &jpp::actions::check_table(),
    );
    let items: Vec<_> = report
        .diagnostics
        .iter()
        .map(diag_json::from_check)
        .collect();
    if diag_json::json_mode() && matches!(command, Command::Check { .. }) {
        print_check_doc(&filename, &loaded, items);
        return if report.is_ok() {
            Ok(())
        } else {
            Err(String::new())
        };
    }
    for line in diag_json::render_all(&loaded, items) {
        eprintln!("{line}");
    }
    if !report.is_ok() {
        return Err(format!(
            "{filename}: check failed ({} errors)",
            report.errors().len()
        ));
    }
    match command {
        Command::Check { .. } => println!(
            "Checked {filename}: no static errors ({} warnings)",
            report.warnings().len()
        ),
        Command::Run(options) => run_io::run_checked(&program, &options, &loaded, 画像, 输入)?,
        _ => unreachable!(),
    }
    Ok(())
}

/// `check --json`：stdout 一个文档，stderr 不出东西（步 9a）。
fn print_check_doc(
    filename: &str,
    loaded: &jpp_syntax::loader::LoadedProgram,
    items: Vec<diag_json::Item>,
) {
    let folded = diag_json::fold(items);
    let count = |l: diag_json::Level| {
        folded
            .iter()
            .filter(|(it, _)| it.level == l)
            .map(|(_, n)| n)
            .sum::<usize>()
    };
    let doc = serde_json::json!({
        "file": filename,
        "ok": count(diag_json::Level::Error) == 0,
        "errors": count(diag_json::Level::Error),
        "warnings": count(diag_json::Level::Warning),
        "diagnostics": folded.iter().map(|(it, n)| diag_json::to_json(loaded, it, *n)).collect::<Vec<_>>(),
    });
    println!("{doc}");
}

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    match options::take_json(&mut args) {
        Ok(on) => diag_json::set_json(on),
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    }
    let questions_out = match options::take_questions_out(&mut args) {
        Ok(q) => q,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    let command = match options::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n\n{}", help());
            return ExitCode::from(2);
        }
    };
    match execute(command, questions_out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // 空报文：诊断已按 `--json` 输出过
            if !error.is_empty() {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
