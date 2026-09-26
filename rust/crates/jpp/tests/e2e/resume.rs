//! 中断与续接（B55，步 18b 预注册第 4 条 (a)–(g)）。
//!
//! 真进程中断：测试把自己的二进制作为子进程再跑一次（环境变量 `JPP_E2E_CHILD` 指定要跑的一段），
//! 子进程里动作的执行函数调 `std::process::abort()`，没有任何收尾；父进程再读子进程留下的账本文件。

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use jpp::effects::{ActionProfile, CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::store::{Blob, DirBlob, IoErr, LedgerFile, MemBlob};
use jpp::value::{Answer, Value};
use jpp::{ActionRegistry, EntryArgs, Ledger, Outcome, Session, TaintOut, lower, syntax::parse};
use serde_json::Value as Json;

const 子进程: &str = "JPP_E2E_CHILD";
const 子目录: &str = "JPP_E2E_DIR";

/// 不可逆动作「发出去」无条件执行（没有守卫，J-08 不管），再对它的结果问一道题：失败值上的判断
/// 不发调用，出口是 `Unsure(fail)`（J-12）。
const 发出去: &str = r#"
budget {calls: 3, cost: 0};
let r = do("发出去", ["给甲"], 0);
let e = cut(judge(state(r), test("发出去了吗", "k")));
let how = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
{r: if is_fail(r) { text(r) } else { content(r) }, failed: is_fail(r), how: how}
"#;

/// 先判断（一层，层末落盘），再做可逆动作「记一下」。
const 先判后记: &str = r#"
budget {calls: 3, cost: 0};
let e = cut(judge(state(mat("材料")), test("行吗", "k")));
let how = handle(e, {act: fn() { "act" }, ignore: fn() { "ignore" }, unsure: fn(u) { consume(u, "drop"); "unsure" }});
{how: how, r: content(do("记一下", ["x"], 0))}
"#;

fn 目录(名: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-e2e-{名}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn 端口<'a>() -> Ports<'a> {
    Ports::new()
        .with(FnPort::judge("fixed-0", |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(0.9)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| Ok(None)))
}

/// 动作表：名字、是否可逆、执行时是否杀掉本进程；执行次数记在 `次数` 里。
fn 动作(名: &str, 可逆: bool, 杀: bool, 次数: Rc<Cell<u32>>) -> ActionRegistry {
    let mut a = ActionRegistry::new();
    a.register(名, 0.0, 可逆, TaintOut::Trusted, move |_| {
        次数.set(次数.get() + 1);
        if 杀 {
            std::process::abort();
        }
        Ok(Value::text("已做"))
    });
    a
}

fn 编译(src: &str) -> jpp::Program {
    lower(&parse(src).expect("解析")).expect("lower")
}

/// 跑一趟，账本逐行写到 `dir/key`；`上一趟` 为续接的账本。返回结果与写完后的账本。
fn 跑(
    src: &str,
    a: &ActionRegistry,
    calib: &CalibStore,
    dir: &Path,
    key: &str,
    上一趟: Ledger,
) -> (Result<Outcome, jpp::Error>, Ledger) {
    let mut f = LedgerFile::new(DirBlob::new(dir), key, 上一趟);
    let r = Session::new(端口(), calib, a).resume(&编译(src), &EntryArgs::default(), &mut f);
    let (l, _) = f.finish().expect("写得进");
    (r, l)
}

fn 读账本(p: &Path) -> (Ledger, bool) {
    let text = std::fs::read_to_string(p).unwrap();
    let (l, t) = Ledger::decode(&text).expect("能解码");
    (l, t.is_some())
}

/// 子进程：在不可逆动作（或可逆动作）执行中杀掉自己。
fn 子进程跑(段: &str) {
    let dir = PathBuf::from(std::env::var(子目录).unwrap());
    let calib = CalibStore::new();
    let n = Rc::new(Cell::new(0));
    match 段 {
        "不可逆" => {
            let a = 动作("发出去", false, true, n);
            let _ = 跑(发出去, &a, &calib, &dir, "l1.jsonl", Ledger::new());
        }
        "可逆" => {
            let a = 动作("记一下", true, true, n);
            let _ = 跑(先判后记, &a, &calib, &dir, "l1.jsonl", Ledger::new());
        }
        _ => unreachable!(),
    }
    // 走不到这里：动作里已经 abort
    std::process::exit(3);
}

/// 父进程：起子进程跑同一个测试，等它被杀。
fn 起子进程(测试名: &str, 段: &str, dir: &Path) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([测试名, "--exact", "--nocapture", "--test-threads=1"])
        .env(子进程, 段)
        .env(子目录, dir)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "子进程应当在动作里被杀：{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// (a) 意向之后、结果之前杀进程：账本最后一条是意向；续接不重执行，得到结果未知的失败值，出口
/// `Unsure(fail)`；只凭续接后的账本审计重放，同一个值、0 调用、0 次执行。
#[test]
fn a_不可逆动作执行中被杀_续接不重做() {
    if let Ok(段) = std::env::var(子进程) {
        return 子进程跑(&段);
    }
    let dir = 目录("a");
    起子进程("resume::a_不可逆动作执行中被杀_续接不重做", "不可逆", &dir);
    let (l1, 截断) = 读账本(&dir.join("l1.jsonl"));
    assert!(!截断, "中断处是一条完整的意向，不是半行");
    assert!(
        matches!(l1.entries.last(), Some(jpp::Entry::Intent { key, .. }) if key.starts_with("intent:")),
        "最后一条是意向：{:?}",
        l1.entries.last()
    );
    let 旧 = std::fs::read(dir.join("l1.jsonl")).unwrap();
    // 续接：写新文件
    let n = Rc::new(Cell::new(0));
    let a = 动作("发出去", false, false, n.clone());
    let calib = CalibStore::new();
    let (r, l2) = 跑(发出去, &a, &calib, &dir, "l2.jsonl", l1);
    let o = r.expect("续接照常返回");
    assert_eq!(n.get(), 0, "不重执行不可逆动作");
    let v = o.value_json();
    assert_eq!(v["failed"], true, "{v}");
    assert!(
        v["r"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown_outcome:"),
        "{v}"
    );
    assert_eq!(v["how"], "unsure", "{v}");
    assert!(
        o.exits.iter().any(|x| x["exit"]
            .as_str()
            .is_some_and(|s| s.starts_with("unsure(fail"))),
        "出口 Unsure(fail)：{:?}",
        o.exits
    );
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-unknown-outcome")),
        "{:?}",
        o.trace.warnings
    );
    assert_eq!(
        std::fs::read(dir.join("l1.jsonl")).unwrap(),
        旧,
        "续接写的是新文件，输入不动"
    );
    assert!(
        !l2.entries
            .iter()
            .any(|e| matches!(e, jpp::Entry::Effect { .. })),
        "不补写结果：有 Effect 即动作返回过"
    );
    // 审计重放续接后的账本
    let (mut l3, _) = 读账本(&dir.join("l2.jsonl"));
    let o3 = Session::new(jpp::NoCallPorts::ports(), &calib, &a)
        .replay(&编译(发出去), &EntryArgs::default(), &mut l3)
        .expect("重放照常返回");
    assert_eq!(o3.value_json(), v);
    assert_eq!(o3.cost.calls, 0);
    assert_eq!(n.get(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 一个 `append_durable` 总失败的存储。
struct 追加失败(MemBlob);

impl Blob for 追加失败 {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, IoErr> {
        self.0.get(key)
    }
    fn put_atomic(&mut self, key: &str, bytes: &[u8]) -> Result<(), IoErr> {
        self.0.put_atomic(key, bytes)
    }
    fn append_durable(&mut self, key: &str, _line: &[u8]) -> Result<(), IoErr> {
        Err(IoErr {
            key: key.into(),
            reason: "磁盘满（测试）".into(),
        })
    }
    fn list(&self, prefix: &str) -> Result<Vec<String>, IoErr> {
        self.0.list(prefix)
    }
}

/// (b) 意向写不进存储：`E-ledger-io`，动作不执行。
#[test]
fn b_意向写不进存储_动作不执行() {
    let n = Rc::new(Cell::new(0));
    let a = 动作("发出去", false, false, n.clone());
    let calib = CalibStore::new();
    let mut f = LedgerFile::new(追加失败(MemBlob::new()), "l.jsonl", Ledger::new());
    let e = Session::new(端口(), &calib, &a)
        .run(&编译(发出去), &EntryArgs::default(), &mut f)
        .expect_err("应当停下");
    let msg = e.render();
    assert!(msg.contains("E-ledger-io"), "{msg}");
    assert_eq!(n.get(), 0, "意向没落盘，动作不执行");
}

/// (c) 画像声明 `idempotent: true` 的不可逆动作：有意向无结果时续接重执行一次，意向只有一条。
#[test]
fn c_声明幂等的动作续接重执行() {
    let mut l1 = Ledger::new();
    let mut calib = CalibStore::new();
    // 先造「有意向无结果」的账本：跑一趟后去掉结果条目
    {
        let n = Rc::new(Cell::new(0));
        let a = 动作("发出去", false, false, n);
        let o = jpp::run(&编译(发出去), 端口(), &calib, &a, &mut l1).expect("首跑");
        assert!(o.value_json()["r"].is_string(), "{}", o.value_json());
    }
    l1.entries
        .retain(|e| !matches!(e, jpp::Entry::Effect { .. }));
    l1.rebuild_index();
    calib.profile.actions.insert(
        "发出去".into(),
        ActionProfile {
            idempotent: Some(true),
            ..Default::default()
        },
    );
    let n = Rc::new(Cell::new(0));
    let a = 动作("发出去", false, false, n.clone());
    let o = jpp::run(&编译(发出去), 端口(), &calib, &a, &mut l1).expect("续接");
    assert_eq!(n.get(), 1, "幂等动作重执行");
    assert_eq!(o.value_json()["r"], "已做");
    let 意向 = l1
        .entries
        .iter()
        .filter(|e| matches!(e, jpp::Entry::Intent { .. }))
        .count();
    assert_eq!(意向, 1);
}

/// (d)(g) 可逆动作不写意向；判断那一层在动作之前已落盘。动作执行中被杀：文件解码不截断，
/// 含判断条目；续接照常执行动作一次。
#[test]
fn d_g_可逆动作被杀_层末已落盘_续接照常执行() {
    if let Ok(段) = std::env::var(子进程) {
        return 子进程跑(&段);
    }
    let dir = 目录("d");
    起子进程(
        "resume::d_g_可逆动作被杀_层末已落盘_续接照常执行",
        "可逆",
        &dir,
    );
    let (l1, 截断) = 读账本(&dir.join("l1.jsonl"));
    assert!(!截断);
    assert!(
        l1.entries
            .iter()
            .any(|e| matches!(e, jpp::Entry::Judge { .. })),
        "判断那一层已落盘"
    );
    assert!(
        !l1.entries
            .iter()
            .any(|e| matches!(e, jpp::Entry::Intent { .. })),
        "可逆动作不写意向"
    );
    let n = Rc::new(Cell::new(0));
    let a = 动作("记一下", true, false, n.clone());
    let (r, l2) = 跑(先判后记, &a, &CalibStore::new(), &dir, "l2.jsonl", l1);
    let o = r.expect("续接");
    assert_eq!(n.get(), 1, "可逆动作续接照常执行");
    assert_eq!(o.cost.calls, 1, "判断命中账本不付费，只有动作一次");
    assert_eq!(
        std::fs::read_to_string(dir.join("l2.jsonl")).unwrap(),
        l2.encode(),
        "跑完的文件与整份编码逐字节相同"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (e)(f) 续接写新文件：新头、旧条目按新链重串在前；预算换成更大的数（等长字面量，站点不变）
/// 不报 `W-header`（B61）。
#[test]
fn e_f_续接写新文件_预算变大不报头() {
    let dir = 目录("e");
    let n = Rc::new(Cell::new(0));
    let a = 动作("记一下", true, false, n.clone());
    let calib = CalibStore::new();
    let (r, l1) = 跑(先判后记, &a, &calib, &dir, "l1.jsonl", Ledger::new());
    r.expect("首跑");
    let 旧 = std::fs::read(dir.join("l1.jsonl")).unwrap();
    let 大 = 先判后记.replace("calls: 3", "calls: 9");
    let (r, l2) = 跑(&大, &a, &calib, &dir, "l2.jsonl", l1.clone());
    let o = r.expect("续接");
    assert_eq!(n.get(), 1, "动作已有结果，续接命中不重做");
    assert!(
        !o.trace.warnings.iter().any(|w| w.starts_with("W-header")),
        "{:?}",
        o.trace.warnings
    );
    assert_eq!(std::fs::read(dir.join("l1.jsonl")).unwrap(), 旧);
    let (读回, 截断) = 读账本(&dir.join("l2.jsonl"));
    assert!(!截断);
    assert_eq!(读回.entries, l1.entries, "旧条目在前、一条不少");
    let h = 读回.header.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(h).unwrap()["budget"]["calls"],
        Json::from(9),
        "新头"
    );
    assert_eq!(读回.encode(), l2.encode());
    let _ = std::fs::remove_dir_all(&dir);
}
