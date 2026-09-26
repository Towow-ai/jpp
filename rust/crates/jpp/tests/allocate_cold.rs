//! **`allocate` 在冷键上静默退化成「按下标取前 k 个」。**
//!
//! `uncertainty` 带内一律返回 `0.0`，降序排 `0.0` 最前——**在有线的键上这是对的**。
//! 但冷键的线取档案保守线（缺省 `hi=1 / lo=0`），**于是每一个 p 都落在带内、
//! 每一条读数都得 `0.0`**，并列按下标升序 → 返回 `[0, 1, …]`，**而且零告警**。
//!
//! **这正是「按不确定性分配预算」最该起作用的那一类键——没校准过的键——
//! 而它在那里退化成了一个与不确定性无关的答案，看起来还像个正常答案。**
//!
//! 判据：**`0.0` 在这里同时承担了「带内，很不确定」和「算不出来」两件事**，
//! **而「算不出来」不是一个值，更不能是一个恰好排在最前面的值。**
//!
//! **Python 侧一样**（`runtime.py:1243` 同式）——所以这不是移植分叉，**是两边同一个洞**。

use std::rc::Rc;

use jpp::effects::CalibStore;
use jpp::strength::{allocate, allocate_report, uncertainty};
use jpp::value::{Answer, Op, Reading};

// 读数是句柄、答案在表里（步 11b-3）：测试自带一张答案表，经 `Answers` 交给 `strength`。
thread_local! {
    static 答: std::cell::RefCell<jpp::value::AnswerTable> = Default::default();
    static 号: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn 新句柄() -> u64 {
    号.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}
fn 记答(r: &Reading, a: Answer) {
    答.with(|t| t.borrow_mut().insert(r, a));
}
fn 答表() -> jpp::value::AnswerTable {
    答.with(|t| t.borrow().clone())
}

fn 读数(calib: &str, p: f64) -> Rc<Reading> {
    let r = Reading {
        q_hash: "q".into(),
        state_hash: "s".into(),
        op: Op::Test,
        calib: calib.into(),
        id: 新句柄(),
        fail: None,
        model_id: "m".into(),
        ledger_key: "lk".into(),
        over_len: 0,
        scale: vec![],
        perms: Default::default(),
        mode_share: Default::default(),
        missing_evidence: vec![],
        state_taint: Default::default(),
        form_hash: None,
        fp: None,
    };
    记答(&r, Answer::Noul(p));
    Rc::new(r)
}

const P: [f64; 6] = [0.98, 0.02, 0.55, 0.47, 0.90, 0.10];

/// **红**：冷键上 `allocate` 不许给出一个「与不确定性无关、却看起来像答案」的答案。
#[test]
fn 冷键上不许静默按下标排() {
    let calib = CalibStore::new(); // 全冷
    let rs: Vec<_> = P.iter().map(|p| 读数("k", *p)).collect();

    // **算不出来的不是一个数**
    for r in &rs {
        assert_eq!(
            uncertainty(&calib, &答表(), r),
            None,
            "冷键上 uncertainty 该是「算不出」，不是一个可排序的数"
        );
    }

    let rep = allocate_report(&calib, &答表(), &rs, 2);
    assert!(
        rep.picked.is_empty(),
        "**一条都排不了序，就不许给出前 k 个**：{:?}",
        rep.picked
    );
    assert_eq!(rep.算不出.len(), 6, "六条全算不出，而且要说出来");
    // 退化答案长这样，现在不许再出现
    assert_ne!(
        rep.picked,
        vec![0, 1],
        "**`[0,1]` 是下标顺序，不是不确定性顺序**"
    );
}

/// **正面**：有线的键上照常给正确答案——**别修成一个什么都不敢给的函数**。
/// `hi=0.8 / lo=0.2`、δ 取缺省：0.55 与 0.47 落在带内（最不确定），0.98/0.02 最远。
#[test]
fn 有线的键上照常给正确答案() {
    let mut calib = CalibStore::new();
    // 步 15d-2：δ 只从记录取（原写画像 δ = 0）
    calib.put("k", 0.8, 0.2, 100, "上岗", Some(0.0)).unwrap();
    let rs: Vec<_> = P.iter().map(|p| 读数("k", *p)).collect();

    for r in &rs {
        assert!(uncertainty(&calib, &答表(), r).is_some(), "有线就算得出");
    }
    let picked = allocate(&calib, &答表(), &rs, 2);
    assert_eq!(
        picked,
        vec![2, 3],
        "0.55 与 0.47 在带内 = 最不确定：{picked:?}"
    );
    assert!(allocate_report(&calib, &答表(), &rs, 2).算不出.is_empty());
}

/// **混着来**：有线的排得了、冷的排不了，**两者不许混在一张榜上**。
/// 排不了的不是「最不确定」也不是「最确定」——**它没有位置**。
#[test]
fn 冷的与有线的不混进一张榜() {
    let mut calib = CalibStore::new();
    // 步 15d-2：δ 只从记录取（原写画像 δ = 0）
    calib.put("热", 0.8, 0.2, 100, "上岗", Some(0.0)).unwrap();
    let rs = vec![
        读数("冷", 0.99),
        读数("热", 0.5),
        读数("冷", 0.01),
        读数("热", 0.99),
    ];

    let rep = allocate_report(&calib, &答表(), &rs, 3);
    assert_eq!(rep.picked, vec![1, 3], "只有两条排得了序，k=3 也只能给两条");
    assert_eq!(rep.算不出, vec![0, 2], "**排不了的要点名，不是悄悄掉队**");
}

/// **`.jpp` 那侧也要看得见「算不出」**——不能只进 trace。
#[test]
fn jpp那侧看得见算不出的那些() {
    use jpp::effects::{EffectError, FnPort, JudgeResult, Ports};
    use jpp::ledger::Ledger;
    use jpp::{ActionRegistry, run};
    use std::cell::RefCell;
    // 步 15c：原 `impl Client` 的桩改为三个闭包端口；`i` 用 `RefCell` 记轮到第几个答案
    let i = RefCell::new(0usize);
    let src = r#"
budget {calls: 8, cost: 1};
let rs = [judge(state(mat("a")), test("q","k")), judge(state(mat("b")), test("q","k")),
          judge(state(mat("c")), test("q","k"))];
allocate(rs, 2)
"#;
    let program = jpp::lower(&jpp::syntax::parse(src).expect("解析")).expect("lower");
    let mut l = Ledger::new();
    let out = run(
        &program,
        Ports::new()
            .with(FnPort::judge("m", |_s, qs| {
                let mut idx = i.borrow_mut();
                let answers = qs
                    .iter()
                    .map(|_| {
                        let a = Answer::Noul(P[*idx % 6]);
                        *idx += 1;
                        a
                    })
                    .collect();
                Ok(JudgeResult {
                    answers,
                    tokens: 0,
                    cost: 0.0,
                    mode_share: vec![],
                    perms: vec![],
                    confidence: vec![],
                })
            }))
            .with(FnPort::generate("m", |_p, _c, _n, _r| {
                Err(EffectError("x".into()))
            }))
            .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into())))),
        &CalibStore::new(),
        &ActionRegistry::new(),
        &mut l,
    )
    .expect("跑得完");
    let v = out.value_json();
    assert_eq!(
        v["picked"].as_array().unwrap().len(),
        0,
        "全冷 → 一条也排不了：{v}"
    );
    assert_eq!(
        v["算不出"].as_array().unwrap().len(),
        3,
        "**三条都要点名，对程序可见**：{v}"
    );
    assert!(
        out.trace
            .warnings
            .iter()
            .any(|w| w.starts_with("W-untested")),
        "并且留痕：{:?}",
        out.trace.warnings
    );
}
