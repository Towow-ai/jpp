//! **校准记录的运行期写入口**（`12`:347 亲口标为「未定」的两样之一）。
//!
//! > 「未定的部分照实标明：真值到达的通道、`CalibRecord` 的运行期写入口与并发/顺序语义，
//! > 两边（Rust 与 Python）都没有……**所以 `on_truth` 现在移植过去只会得到一个没有生产者
//! > 也没有消费者的注册表**，本版不做，待上面两样有了再议。」
//!
//! 这一包做的就是那句「待上面两样有了」里的第二样。**在此之前，这门语言能用线、不能产生证据。**
//!
//! **同时必须堵住的洗白口**：`samples` 是 `[[p, label]]`，而一次读数只有 `p`、**没有 label**。
//! 若把无标注的观察计进 `n`，那条线就会「看起来有 200 条标注撑着」而其实一条也没有——
//! `put` 自己的报错写着「上岗记录必须带 n > 0（**线只从标注记录来**）」，**标注**二字是承重的。
//! 所以：无标注的观察进 `samples`、进 `待真值`，**不进 `n`**。`待真值` 本来就是四个状态里
//! 为这件事留的那一格。

use jpp::ActionRegistry;
use jpp::effects::{
    CalibStore, EffectError, FnPort, JudgeResult, LiteralMode, Ports, Provenance, Sample,
};
use jpp::interp::Interp;
use jpp::ledger::Ledger;
use jpp::value::Answer;
use jpp::{lower, syntax::parse};

/// 判断恒给 0.9、不该生成、不该问人（步 15c：原 `impl Client` 的桩改为三个闭包端口）
fn 定值端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("fixed-0", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |_p, _c, _n, _r| {
            Err(EffectError("不该 gen".into()))
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }))
}

const 程序: &str = r#"
budget {calls: 5, cost: 1};
let s = state(mat("材料"));
handle(cut(judge(s, test("行吗", "线.键"))), {
    act: fn() { "act" }, ignore: fn() { "ig" },
    unsure: fn(u) { consume(u, "drop"); "un" }})
"#;

fn 跑一趟(ledger: &mut Ledger, calib: &CalibStore) -> jpp::Outcome {
    let program = lower(&parse(程序).expect("解析")).expect("lower");
    let actions = ActionRegistry::default();
    let it = Interp::new(
        定值端口(0.9),
        ledger,
        calib,
        &actions,
        program.budget.clone(),
    );
    it.run(&program).expect("跑得完")
}

/// **A 正面：程序跑完之后，那个键下面真的多了证据，而且能说出证据是哪来的。**
#[test]
fn 程序能产生证据而不只是消费线() {
    let mut calib = CalibStore::new();
    let mut ledger = Ledger::default();
    let out = 跑一趟(&mut ledger, &calib);
    assert!(out.value.is_some(), "程序要跑完");

    // 读数带着 perms / mode_share / 物理形式 / 字面模式落进缓冲区（总控立的规矩：
    // **`mode_share` 不许裸记，必须和 `perms` 一起进账本和校准记录**）
    assert_eq!(
        out.evidence.len(),
        1,
        "一道题应当产生一条证据：{:?}",
        out.evidence
    );
    let (键, 样本) = &out.evidence[0];
    assert_eq!(键, "线.键");
    assert_eq!(样本.p, Some(0.9));
    assert_eq!(样本.label, None, "读数本身没有真值");
    assert_eq!(样本.phys, "noul");

    // 折进库里：n **不动**（没有标注），状态进「待真值」
    calib.absorb(键, 样本.clone()).expect("折得进");
    let rec = calib.get("线.键");
    assert_eq!(rec.n, 0, "**无标注的观察不计进 n**——n 是线所倚仗的标注条数");
    assert_eq!(rec.observations(), 1, "但它确实被记下来了");
    assert_eq!(rec.status, "待真值", "四个状态里为这件事留的那一格");
    // **一条标注也没有 = 「只有观察」，不是「程序积累」。**
    // 以前它掉进「程序积累」，因为 `n == labeled()` 在两边都是 0 时偶然为真——
    // **而证书门正按这个判**，于是把「跑过几次的键」当成「积累了证据的键」锁住。
    assert_eq!(rec.provenance(), Provenance::只有观察);
}

/// **A 反面：宿主手填的线和程序积累的证据，在记录上分得开。**
/// 这是「兜底档案的 `hash` 必须是 `None` 而不是兜底值的哈希」同一条判据的第三次出现。
#[test]
fn 宿主手填与程序积累分得开() {
    let mut c = CalibStore::new();
    // 宿主手写一条 n=200 的线：一条样本也没有
    c.put("手填", 0.8, 0.2, 200, "上岗", Some(0.05))
        .expect("写得进");
    assert_eq!(c.get("手填").provenance(), Provenance::宿主手填);
    assert_eq!(
        c.get("手填").observations(),
        0,
        "**n=200 而一条证据也没有——这正是以前分不出来的那种记录**"
    );

    // 程序积累的：带标注的样本，n 跟着样本走
    let mut store = CalibStore::new();
    for i in 0..3 {
        store
            .absorb(
                "积累",
                Sample {
                    p: Some(0.5 + i as f64 * 0.1),
                    label: Some(1),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "noul".into(),
                    cluster: None,
                    stratum: None,
                },
            )
            .expect("折得进");
    }
    let rec = store.get("积累");
    assert_eq!((rec.n, rec.observations()), (3, 3), "**带标注的才进 n**");
    assert_eq!(rec.provenance(), Provenance::程序积累);

    // 混合：手填的 n 与实际标注条数对不上 → 记录自己说得出「这两者不一致」
    //
    // **这里不能写「上岗」**：证书门挡的正是「积累来的证据靠手写的线上岗」。
    // 要演示的是 `provenance` 分得清混合，**上岗与否是无关的那一半**。
    store
        .put("积累", 0.8, 0.2, 200, "待真值", None)
        .expect("写得进");
    assert_eq!(
        store.get("积累").observations(),
        3,
        "**写线不得抹掉已积累的证据**"
    );
    assert_eq!(
        store.get("积累").provenance(),
        Provenance::混合,
        "n=200 而只有 3 条标注"
    );

    // 空记录不谎称任何来源
    assert_eq!(CalibStore::new().get("没写过").provenance(), Provenance::空);
}

/// **重放不得重复计数。** 重放也产生读数；若积累挂在「有读数产生」上，第二趟 n 就翻倍，
/// **而账本这个审计物会跟自己对不上**。
#[test]
fn 重放不重复计数() {
    let calib = CalibStore::new();
    let mut ledger = Ledger::default();
    let 首趟 = 跑一趟(&mut ledger, &calib);
    assert_eq!(首趟.evidence.len(), 1);
    assert_eq!(首趟.cost.calls, 1);

    // 带着同一本账本再跑：零调用（重放），**零新证据**
    let 二趟 = 跑一趟(&mut ledger, &calib);
    assert_eq!(二趟.cost.calls, 0, "重放不该再花钱");
    assert!(二趟.cost.replayed > 0, "确实走的是重放路径");
    assert_eq!(二趟.evidence.len(), 0, "**重放读的是既有事实，不是新观察**");
}

/// `put` 该拦住非法状态（Python 在 `calib.py:62` 拦，Rust 一直没拦）。
#[test]
fn 状态只能是那四个() {
    let mut c = CalibStore::new();
    assert!(
        c.put("k", 0.8, 0.2, 10, "随便写的", None).is_err(),
        "四个状态之外的字面串不该进得去"
    );
    for s in ["冷", "上岗", "停岗", "待真值"] {
        assert!(c.put("k", 0.8, 0.2, 10, s, None).is_ok(), "{s} 是合法状态");
    }
}

/// **有线的键上跑程序 = 宿主手填 + 程序观察叠加，那是混合，不是「只有观察」。**
///
/// **这一格固定观察测不到**：既有测试要么只 `put`、要么只 `absorb`。
/// **而真机的工作流天然是两者叠加**——E-JPP-LIVE 第一次真机跑完折证据时撞出来的。
#[test]
fn 有线的键上跑出观察是混合() {
    let mut c = CalibStore::new();
    c.put("k", 0.66, 0.56, 73, "上岗", Some(0.05))
        .expect("宿主手填一条线");
    assert_eq!(c.get("k").provenance(), Provenance::宿主手填);
    c.absorb(
        "k",
        Sample {
            p: Some(0.56),
            label: None,
            perms: 0,
            mode_share: None,
            mode: LiteralMode::default(),
            phys: "noul".into(),
            cluster: None,
            stratum: None,
        },
    )
    .unwrap();
    let r = c.get("k");
    assert_eq!((r.n, r.labeled(), r.observations()), (73, 0, 1));
    assert_eq!(
        r.provenance(),
        Provenance::混合,
        "**宿主那 73 条不能被一条无标注观察说没了**"
    );
}
