//! 按缓存键复用与两段式（B40、B151，步 19 预注册第 6 条 (a)(b)(c)(e)(f)；(d) 在 `jpp-ir::key` 单测）。

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::Command;

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, NoCallPorts, Ports};
use jpp::ledger::{Entry, Ledger, observations};
use jpp::store::CacheIndex;
use jpp::value::Answer;
use jpp::{ActionRegistry, EntryArgs, Session, lower, syntax::parse};
use serde_json::{Value as Json, json};

/// 搭建程序：生成三个候选名字，再判一道题。
const 搭建: &str = r#"
budget {calls: 4, cost: 0};
let g = gen("写三个候选名字", [mat("需求")], 3, 0);
let e = cut(judge(state(mat("候选甲")), test("合适吗", "k")));
let how = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
{names: if is_fail(g) { [] } else { map(g, fn(m) { content(m) }) }, how: how}
"#;

/// 日常程序：同一生成与同一判断换了调用位置（前面多一行），另加一道新题。
const 日常: &str = r#"
budget {calls: 4, cost: 0};
let pad = 1;
let g = gen("写三个候选名字", [mat("需求")], 3, 0);
let e = cut(judge(state(mat("候选甲")), test("合适吗", "k")));
let how = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
let e2 = cut(judge(state(mat("候选甲")), test("好记吗", "k")));
let how2 = handle(e2, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
{names: if is_fail(g) { [] } else { map(g, fn(m) { content(m) }) }, how: how, how2: how2}
"#;

fn 编译(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

/// 判断恒 0.9、生成给固定三项；各自数调用次数。
fn 端口<'a>(判: &'a Cell<u32>, 生: &'a Cell<u32>, 模型: &str) -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            判.set(判.get() + 1);
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate(模型, move |_p, _c, _n, _r| {
            生.set(生.get() + 1);
            Ok(GenResult {
                outputs: vec![json!("甲"), json!("乙"), json!("丙")],
                tokens: 0,
                cost: 0.0,
                failure: None,
                taint_out: None,
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该问人".into()))
        }))
}

struct 一趟 {
    value: Json,
    ledger: Ledger,
    cache: Option<jpp::interp::CacheStats>,
    warnings: Vec<String>,
}

fn 跑(
    src: &str,
    ports: Ports<'_>,
    ledger: Ledger,
    cache: Option<&CacheIndex>,
    gen_model: Option<&str>,
    replay: bool,
) -> 一趟 {
    let calib = CalibStore::new();
    let acts = ActionRegistry::new();
    let mut l = ledger;
    let mut s = Session::new(ports, &calib, &acts).with_gen(gen_model.map(String::from), None);
    if let Some(c) = cache {
        s = s.with_cache(c);
    }
    let o = if replay {
        s.replay(&编译(src), &EntryArgs::default(), &mut l)
    } else {
        s.resume(&编译(src), &EntryArgs::default(), &mut l)
    }
    .unwrap_or_else(|e| panic!("{}", e.render()));
    一趟 {
        value: o.value_json(),
        ledger: l,
        cache: o.cache,
        warnings: o.trace.warnings,
    }
}

fn 复用来源(l: &Ledger) -> Vec<String> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { reused_from, .. } | Entry::Effect { reused_from, .. } => {
                reused_from.clone()
            }
            _ => None,
        })
        .collect()
}

/// (a) 两段式与 (b) 账本自足。
#[test]
fn a_b_搭建一次_日常命中_只凭日常账本重放() {
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 搭 = 跑(
        搭建,
        端口(&判, &生, "gen-a"),
        Ledger::new(),
        None,
        Some("gen-a"),
        false,
    );
    assert_eq!((判.get(), 生.get()), (1, 1));
    let ix = CacheIndex::build(&[("build.jsonl".to_string(), 搭.ledger)]);

    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 日 = 跑(
        日常,
        端口(&判, &生, "gen-a"),
        Ledger::new(),
        Some(&ix),
        Some("gen-a"),
        false,
    );
    assert_eq!(生.get(), 0, "生成命中，不调用生成端口");
    assert_eq!(判.get(), 1, "同键判断命中；只有新题发了一次");
    assert_eq!(日.value["names"], json!(["甲", "乙", "丙"]));
    assert_eq!(日.value["how"], 搭.value["how"]);
    let 来源 = 复用来源(&日.ledger);
    assert_eq!(来源.len(), 2, "{来源:?}");
    assert!(
        来源.iter().all(|s| s.starts_with("ext:build.jsonl#")),
        "{来源:?}"
    );
    for e in &日.ledger.entries {
        if let Entry::Judge {
            reused_from: Some(_),
            cost,
            call,
            ..
        } = e
        {
            assert_eq!((*cost, *call), (0.0, 0));
        }
    }
    let c = 日.cache.expect("给了缓存就有 cache 一节");
    assert_eq!(
        (c.hits["judge"], c.hits["gen"], c.cross_run, c.same_run),
        (1, 1, 2, 0)
    );
    assert_eq!(c.saved_calls, 2);
    assert_eq!((c.requests["judge"], c.requests["gen"]), (1, 0));

    // (b) 只凭日常账本审计重放：不给缓存、不许调用，值相同
    let 重放 = 跑(
        日常,
        NoCallPorts::ports(),
        日.ledger.clone(),
        None,
        Some("gen-a"),
        true,
    );
    assert_eq!(重放.value, 日.value);
    assert!(重放.cache.is_none(), "重放命中的是账本，不是复用");
}

/// (c) 换生成器模型：生成不命中、照常调用；续接时头比对报 `W-header`。
#[test]
fn c_换生成器模型不命中_续接报头() {
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 搭 = 跑(
        搭建,
        端口(&判, &生, "gen-a"),
        Ledger::new(),
        None,
        Some("gen-a"),
        false,
    );
    let ix = CacheIndex::build(&[("build.jsonl".to_string(), 搭.ledger.clone())]);
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 日 = 跑(
        日常,
        端口(&判, &生, "gen-b"),
        Ledger::new(),
        Some(&ix),
        Some("gen-b"),
        false,
    );
    assert_eq!(生.get(), 1, "换了模型，生成照常调用");
    assert_eq!(日.cache.unwrap().hits["gen"], 0);
    // 续接搭建账本、换生成器模型：头比对报 W-header（与判断器画像同一套）
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 续 = 跑(
        搭建,
        端口(&判, &生, "gen-b"),
        搭.ledger,
        None,
        Some("gen-b"),
        false,
    );
    assert!(
        续.warnings
            .iter()
            .any(|w| w.starts_with("W-header") && w.contains("gen_model")),
        "{:?}",
        续.warnings
    );
}

/// (e) 同一运行里两个调用位置问同一状态同一题（第二次在第一次 `cut` 之后）：只发一次，第二条是复用条目，
/// 校准观察不含它。
#[test]
fn e_同运行复用() {
    let src = r#"
budget {calls: 4, cost: 0};
let s = state(mat("材料"));
let e1 = cut(judge(s, test("行吗", "k")));
let a = handle(e1, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
let e2 = cut(judge(s, test("行吗", "k")));
let b = handle(e2, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
{a: a, b: b}
"#;
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let r = 跑(
        src,
        端口(&判, &生, "fixed-0"),
        Ledger::new(),
        None,
        None,
        false,
    );
    assert_eq!(判.get(), 1, "同缓存键只发一次");
    // 键 k 没有校准记录，两处都按判断器的回答走（意图汇编 11a）；要核的是两处走同一条路
    assert_eq!(r.value, json!({"a": "act", "b": "act"}));
    let 判断条目: Vec<&Entry> = r
        .ledger
        .entries
        .iter()
        .filter(|e| matches!(e, Entry::Judge { .. }))
        .collect();
    assert_eq!(判断条目.len(), 2);
    assert!(
        matches!(判断条目[1], Entry::Judge { reused_from: Some(k), .. } if k == 判断条目[0].key())
    );
    assert_eq!(observations(&r.ledger).len(), 1, "复用条目不进校准观察");
    let c = r.cache.expect("有命中就有 cache 一节");
    assert_eq!((c.hits["judge"], c.same_run, c.cross_run), (1, 1, 0));
}

// ---------- (f) CLI ----------

fn 目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-e2e-cache-{名}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("cache")).unwrap();
    d
}

fn jpp(d: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(d)
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

const 预算: &str = "budget {calls: 2, cost: 0};\n";
const 一题: &str = "let e = cut(judge(state(mat(\"hello\")), test(\"行吗\", \"k\")));\nhandle(e, {act: fn() { \"act\" }, ignore: fn() { \"ignore\" }, unsure: fn(u) { consume(u, \"drop\"); \"unsure\" }})\n";

/// (f) `--cache` 跳过不是账本的文件并报数；命中时不需要观察；`--replay` 带 `--cache` 不查缓存。
#[test]
fn f_cli_cache() {
    let d = 目录("cli");
    std::fs::write(d.join("p1.jpp"), format!("{预算}{一题}")).unwrap();
    // 同一道题换个调用位置
    std::fs::write(d.join("p2.jpp"), format!("{预算}let pad = 1;\n{一题}")).unwrap();
    std::fs::write(
        d.join("fx.json"),
        json!({"observations": [{"on": ["hello"], "op": "test", "text": "行吗", "calib": "k", "answer": {"Noul": 0.95}}]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        d.join("empty.json"),
        json!({"observations": []}).to_string(),
    )
    .unwrap();
    std::fs::write(d.join("cache/readme.txt"), "不是账本").unwrap();
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p1.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "cache/l1.jsonl",
            "--output",
            "r1.json",
        ],
    );
    assert!(ok, "{err}");
    // 日常：夹具里没有观察，命中缓存才能跑通
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p2.jpp",
            "--fixtures",
            "empty.json",
            "--cache",
            "cache",
            "--output",
            "r.json",
        ],
    );
    assert!(ok, "{err}");
    assert!(err.contains("跳过 1 个文件"), "{err}");
    let r: Json =
        serde_json::from_str(&std::fs::read_to_string(d.join("r.json")).unwrap()).unwrap();
    let r1: Json =
        serde_json::from_str(&std::fs::read_to_string(d.join("r1.json")).unwrap()).unwrap();
    assert_eq!(r["value"], r1["value"], "与搭建时的结果相同");
    assert_eq!(r["cache"]["hits"]["judge"], 1, "{r}");
    assert_eq!(r["cost"]["calls"], 0);
    // 重放只凭账本：p2 的账本键不在 l1 里，带 --cache 也报缺记录
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p2.jpp",
            "--fixtures",
            "empty.json",
            "--replay",
            "cache/l1.jsonl",
            "--cache",
            "cache",
        ],
    );
    assert!(!ok && err.contains("E-replay"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// (g) 公开 PR #37 评审 P2：缓存读不成要报错，不许悄悄当空缓存、重新付费。`--cache` 指向文件（不是目录）、
/// 目录里有读不了的文件，都报 `E-cache` 并带路径，程序不运行（夹具里没有观察，一发请求就会是别的错）；
/// 目录不存在按空缓存。
#[test]
fn g_cli_cache_读错误报错() {
    let d = 目录("err");
    std::fs::write(d.join("p1.jpp"), format!("{预算}{一题}")).unwrap();
    std::fs::write(
        d.join("empty.json"),
        json!({"observations": []}).to_string(),
    )
    .unwrap();
    std::fs::write(d.join("not-a-dir"), "x").unwrap();
    let (ok, err) = jpp(
        &d,
        &[
            "run",
            "p1.jpp",
            "--fixtures",
            "empty.json",
            "--cache",
            "not-a-dir",
        ],
    );
    assert!(
        !ok && err.contains("E-cache") && err.contains("not-a-dir"),
        "{err}"
    );
    assert!(!err.contains("固定观察未命中"), "报错在运行之前：{err}");
    // 读不了的文件（去掉读权限）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = d.join("cache/locked.jsonl");
        std::fs::write(&f, "{}").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&f).is_err() {
            let (ok, err) = jpp(
                &d,
                &[
                    "run",
                    "p1.jpp",
                    "--fixtures",
                    "empty.json",
                    "--cache",
                    "cache",
                ],
            );
            assert!(
                !ok && err.contains("E-cache") && err.contains("locked.jsonl"),
                "{err}"
            );
            assert!(!err.contains("固定观察未命中"), "报错在运行之前：{err}");
        }
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    // 目录不存在：按空缓存，程序照常运行（这里因为夹具没有观察而在运行里失败，说明走到了运行）
    let (_, err) = jpp(
        &d,
        &[
            "run",
            "p1.jpp",
            "--fixtures",
            "empty.json",
            "--cache",
            "no-such-dir",
        ],
    );
    assert!(err.contains("不存在，按空缓存"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// (h) 推测发出的判断也查缓存（说话 v2 实测：全部命中缓存却照样发出请求、照样计费，2026-09-27）。
/// 循环里下一轮的判断会被推测提前发出；推测若不查缓存，缓存命中省不下调用。
const 循环: &str = r#"
budget {calls: 40, cost: 0, depth: 64};
fn step(acc, i) !{judge} {
    let ms = map(["甲", "乙", "丙"], fn(w) { mat({已说: acc.s, 块: w}) });
    let rs = map(ms, fn(m) { judge(state(m), test("接得上吗", "k")) });
    let ok = map(rs, fn(r) { handle(cut(r), {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(u) { consume(u, "drop"); 0 }}) });
    {s: acc.s + text(sum(ok)), left: acc.left - 1}
}
iterate(3, {s: "", left: 3}, step, fn(acc) { acc.left }).value.s
"#;

#[test]
fn h_推测也查缓存() {
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 首 = 跑(
        循环,
        端口(&判, &生, "gen-a"),
        Ledger::new(),
        None,
        None,
        false,
    );
    assert!(判.get() >= 1);
    let ix = CacheIndex::build(&[("first.jsonl".to_string(), 首.ledger)]);
    let (判, 生) = (Cell::new(0), Cell::new(0));
    let 再 = 跑(
        循环,
        端口(&判, &生, "gen-a"),
        Ledger::new(),
        Some(&ix),
        None,
        false,
    );
    assert_eq!(再.value, 首.value);
    let c = 再.cache.expect("cache 一节");
    assert_eq!(
        判.get(),
        0,
        "全部命中缓存，不该有任何请求（含推测）；命中 {:?}",
        c.hits
    );
    assert_eq!(c.requests["judge"], 0);
    assert_eq!(c.hits["judge"], 9);
}
