//! 步 14b：宿主入口 `EntryArgs`（lib 级，不经 CLI）。B105（值条目 / 材料条目 / `purpose` / `entry_hash`）、
//! B106（`Program.entry` 由 `Session::compile` 写入）。预注册见 `地基/过程记录/工程-步14b.md` §四第 14 条；
//! 依据：地基/附注/2026-09-25-B105-B106裁定.md。
use jpp::effects::{CalibStore, FixedPorts};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ir::EntryDecl;
use jpp::ledger::Ledger;
use jpp::value::{Answer, Mat, Op, Question, State, Taint};
use jpp::{EntryArgs, EntryMat, EntryValue, Error, Program, Session};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, fs, path::PathBuf, process::Command, rc::Rc};

fn compile(src: &str, entry: &EntryArgs) -> Result<Program, Vec<String>> {
    let ast = jpp::syntax::parse(src).expect("语法");
    Session::compile(&ast, &entry.decl()).map_err(|ds| ds.into_iter().map(|d| d.message).collect())
}

struct Run {
    result: Result<Json, String>,
    entry_hash: Option<String>,
    wrote: bool,
}

/// 跑一次：固定观察给 `(内容, 题面, 校准键) → Noul(p)`；登记不可逆动作 `write`（记下是否执行）
fn run(src: &str, entry: &EntryArgs, calib: &CalibStore, obs: &[(Json, &str, &str, f64)]) -> Run {
    let prog = compile(src, entry).expect("compile");
    let mut fp = FixedPorts::new();
    for (on, text, key, p) in obs {
        let st = State::new(
            vec![Mat::literal(on.clone())],
            vec![],
            vec![],
            vec![],
            false,
        );
        fp.observe(
            &st,
            &Question::new(Op::Test, text, key, vec![]),
            Answer::Noul(*p),
        );
    }
    let wrote = Rc::new(RefCell::new(false));
    let w = wrote.clone();
    let mut actions = ActionRegistry::new();
    actions.register("write", 0.0, false, TaintOut::Inherit, move |args| {
        *w.borrow_mut() = true;
        Ok(args[0].clone())
    });
    let mut ledger = Ledger::new();
    let out = Session::new(fp.ports(), calib, &actions).run(&prog, entry, &mut ledger);
    let entry_hash = ledger
        .header
        .as_ref()
        .and_then(|h| h.compared.entry_hash.clone());
    let result = match out {
        Ok(o) => Ok(o.value.map(|v| v.to_json()).unwrap_or(Json::Null)),
        Err(Error::Check(r)) => Err(r.render()),
        Err(Error::Runtime(e)) => Err(e.render()),
    };
    let wrote = *wrote.borrow();
    Run {
        result,
        entry_hash,
        wrote,
    }
}

/// 正式线：可算真值 200 行经真值通道认证（与 `entry_input.rs` (g) 同一份），读数 0.95 落在 act 侧
fn certified_calib(tag: &str) -> CalibStore {
    let d: PathBuf =
        std::env::temp_dir().join(format!("jpp-entry-args-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let labels: String = (0..200)
        .map(|i| {
            let (p, l) = if i % 2 == 0 { (0.95, true) } else { (0.05, false) };
            json!({"key": "k", "item": format!("i{i}"), "p": p, "label": l, "source": "computed", "text": "hello"})
                .to_string()
                + "\n"
        })
        .collect();
    fs::write(d.join("labels.jsonl"), labels).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(&d)
        .env("HOME", &d)
        .args([
            "calib-import",
            "labels.jsonl",
            "--calib-out",
            "calib",
            "--profile",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json"),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    CalibStore::load(&d.join("calib")).unwrap()
}

const 守卫: &str = "budget {calls: 2, cost: 1};\nlet ok = handle(cut(judge(state(doc), test(\"行吗\", \"k\"))), {\n    act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { consume(u, \"drop\"); false }});\nlet w = if ok { content(do(\"write\", [\"x\"], 0)) } else { \"没写\" };\n{doc: doc, w: w}\n";

/// (h) 材料条目整份判：origin = ["input"]、taint untrusted；唯一守卫的不可逆 do 被 J-08 拦且报文含「宿主入口」；
/// 宿主声明 Trusted 且线经认证时放行；两次 `entry_hash` 不同。
#[test]
fn h_材料条目整份判() {
    let calib = certified_calib("h");
    let obs = [(json!("hello"), "行吗", "k", 0.95)];
    let untrusted = EntryArgs {
        materials: vec![EntryMat::untrusted("doc", json!("hello"))],
        ..Default::default()
    };
    let r = run(守卫, &untrusted, &calib, &obs);
    let err = r.result.expect_err("不可信入口材料不能单独放行不可逆 do");
    assert!(
        err.contains("J-08") && err.contains("宿主入口 doc"),
        "{err}"
    );
    assert!(!r.wrote);
    // 步 24-0（B108）：J-08 静态子面在执行前就拦下，账本头不写；不可信版本的 entry_hash 按同一算法直接算
    let h1 = untrusted.hash().expect("有入口时 entry_hash 非空");

    let mut m = Mat::literal(json!("hello"));
    m.taint = Taint::Trusted;
    let trusted = EntryArgs {
        materials: vec![EntryMat::new("doc", m)],
        ..Default::default()
    };
    let r = run(守卫, &trusted, &calib, &obs);
    let v = r.result.expect("宿主声明可信、线经认证：放行");
    assert!(r.wrote);
    assert_eq!(v["doc"]["origin"], json!(["input"]), "{v}");
    assert_eq!(v["doc"]["taint"], json!("Trusted"), "{v}");
    assert_ne!(Some(h1), r.entry_hash, "声明的 taint 进 entry_hash");

    // 不可信版本返回材料时 origin 也是 input、taint untrusted（不走 do 的同一程序）
    let src = "budget {calls: 0, cost: 0};\n{doc: doc}\n";
    let r = run(src, &untrusted, &CalibStore::new(), &[]);
    let v = r.result.unwrap();
    assert_eq!(v["doc"]["origin"], json!(["input"]), "{v}");
    assert_eq!(v["doc"]["taint"], json!("Untrusted"), "{v}");
}

/// (i) `purpose` 填进题面后出口为 Untrusted（B105-3 依 B58）。14b 时推迟（ignore），步 17b 随 B58 绑定后启用。
#[test]
fn i_purpose进题面出口不可信() {
    let with = EntryArgs {
        purpose: Some("找出要退款的对话".into()),
        ..EntryArgs::value("input", json!({"x": 1}))
    };
    let src = "budget {calls: 1, cost: 1};\nlet e = cut(judge(state(mat(\"hello\")), test(purpose, \"k\")));\n{p: purpose, t: taint(e)}\n";
    let calib = certified_calib("i");
    let r = run(
        src,
        &with,
        &calib,
        &[(json!("hello"), "找出要退款的对话", "k", 0.95)],
    );
    let v = r.result.expect("purpose 可读");
    assert_eq!(v["p"], json!("找出要退款的对话"));
    assert_eq!(v["t"], json!("untrusted"), "{v}");
}

/// (i′) `purpose` 进名字表（步 17b）：`Program.entry` 第一条是名为 `purpose` 的不可信值条目，程序可读，
/// 检查器不报 `E-name`；它照样进 `entry_hash`（带与不带不同）。14b 到 17b 之间本条钉的是「不进名字表」。
#[test]
fn i2_purpose进名字表_也进哈希() {
    let base = EntryArgs::value("input", json!({"x": 1}));
    let with = EntryArgs {
        purpose: Some("找出要退款的对话".into()),
        ..base.clone()
    };
    let p = compile("budget {calls: 0, cost: 0};\npurpose\n", &with)
        .expect("带 purpose 的入口 compile 通过");
    let first = &p.entry.params[0];
    assert_eq!(first.name, "purpose", "{:?}", p.entry);
    assert_eq!(first.kind, jpp::ir::EntryKind::Value, "{:?}", p.entry);
    assert_eq!(first.taint, jpp::ir::EntryTaint::Untrusted, "{:?}", p.entry);
    assert_eq!(p.entry, with.decl());
    let rep = jpp::check::check(&p);
    assert!(!rep.render().contains("E-name"), "{}", rep.render());
    let a = run(
        "budget {calls: 0, cost: 0};\n{p: purpose, x: input.x}\n",
        &with,
        &CalibStore::new(),
        &[],
    );
    let b = run(
        "budget {calls: 0, cost: 0};\ninput.x\n",
        &base,
        &CalibStore::new(),
        &[],
    );
    assert_eq!(a.result.unwrap(), json!({"p": "找出要退款的对话", "x": 1}));
    assert!(a.entry_hash.is_some() && b.entry_hash.is_some());
    assert_ne!(a.entry_hash, b.entry_hash, "purpose 进 entry_hash");
}

/// (j) 同名两条目 `E-entry-dup`；条目名撞内置 `E-entry-name`。
#[test]
fn j_名字重复与撞内置() {
    let dup = EntryArgs {
        values: vec![EntryValue::new("a", json!(1))],
        materials: vec![EntryMat::untrusted("a", json!("x"))],
        ..Default::default()
    };
    let e = compile("budget {calls: 0, cost: 0};\n1\n", &dup).unwrap_err();
    assert!(e.iter().any(|m| m.starts_with("E-entry-dup")), "{e:?}");
    let builtin = EntryArgs::value("len", json!(1));
    let e = compile("budget {calls: 0, cost: 0};\n1\n", &builtin).unwrap_err();
    assert!(e.iter().any(|m| m.starts_with("E-entry-name")), "{e:?}");
    // `purpose` 与同名值条目重名（步 17b 绑定 `purpose` 后加回，14b 时因不绑定而去掉）
    let purpose_dup = EntryArgs {
        purpose: Some("目的".into()),
        ..EntryArgs::value("purpose", json!("另一个"))
    };
    let e = compile("budget {calls: 0, cost: 0};\n1\n", &purpose_dup).unwrap_err();
    assert!(e.iter().any(|m| m.starts_with("E-entry-dup")), "{e:?}");
}

/// (k) 值条目宿主声明 Trusted 时 `mat(input.x)` origin 为 literal（B33 第 5 条），`entry_hash` 与缺省不同。
#[test]
fn k_值条目声明可信() {
    let src = "budget {calls: 0, cost: 0};\n{m: mat(input.x)}\n";
    let dflt = EntryArgs::value("input", json!({"x": "hello"}));
    let trusted = EntryArgs {
        values: vec![EntryValue::new("input", json!({"x": "hello"})).with_taint(Taint::Trusted)],
        ..Default::default()
    };
    let a = run(src, &dflt, &CalibStore::new(), &[]);
    let b = run(src, &trusted, &CalibStore::new(), &[]);
    let (va, vb) = (a.result.unwrap(), b.result.unwrap());
    assert_eq!(va["m"]["origin"], json!(["computed"]), "{va}");
    assert_eq!(vb["m"]["origin"], json!(["literal"]), "{vb}");
    assert_eq!(vb["m"]["taint"], json!("Trusted"), "{vb}");
    assert_ne!(a.entry_hash, b.entry_hash);
}

/// (l) `Program.entry` 为空时 IR 打印与序列化与无入口时逐字节相同；非空时多一行 `entry`、JSON 多一个键。
#[test]
fn l_空入口不改ir() {
    let src = "budget {calls: 0, cost: 0};\n1\n";
    let ast = jpp::syntax::parse(src).unwrap();
    let plain = jpp::lower(&ast).unwrap();
    let empty = Session::compile(&ast, &EntryDecl::default()).unwrap();
    assert_eq!(jpp::ir::print(&plain, None), jpp::ir::print(&empty, None));
    assert_eq!(
        serde_json::to_string(&plain).unwrap(),
        serde_json::to_string(&empty).unwrap()
    );
    assert!(!serde_json::to_string(&plain).unwrap().contains("\"entry\""));
    let with = compile(src, &EntryArgs::value("input", json!(1))).unwrap();
    let text = jpp::ir::print(&with, None);
    assert!(text.contains("entry input:value:untrusted"), "{text}");
    assert!(serde_json::to_string(&with).unwrap().contains("\"entry\""));
}
