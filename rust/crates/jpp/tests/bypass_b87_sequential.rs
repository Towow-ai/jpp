//! B87：序贯 e 过程认证（`--certify sequential`，步 20h；主会话 2026-09-24 按方案 A：候选首个 `seq_first` 条、
//! 只按混合 E 判定，settled 线对只会与固定序一样宽或更严）。依据：`地基/附注/2026-09-24-标注门槛裁定.md` §二。

use jpp::effects::{
    CalibStore, CertGrade, LiteralMode, Sample, SeqSpec, seq_first, two_ends_order,
};
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, SeqImport, import_labels};
use serde_json::json;

const W: [f64; 4] = [0.8, 0.1, 0.05, 0.05];

fn 序贯(
    two_ends: bool,
    frame: Option<Vec<(String, f64)>>,
    tau: Option<f64>,
    batch: usize,
) -> SeqImport {
    SeqImport {
        batch,
        weights: W,
        coverage_target: tau,
        two_ends,
        frame,
    }
}

fn 选项(sq: SeqImport, alpha_trial: Option<f64>) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "b87".into(),
        seed: 20260924,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial,
        certify: CertifyMethod::Sequential,
        step: None,
        sequential: Some(sq),
    }
}

/// `(材料, 读数, 真值)` → 标注行（键 k）。
fn 行(xs: &[(String, f64, bool)]) -> Vec<LabelRow> {
    xs.iter()
        .map(|(i, p, l)| {
            serde_json::from_value(
                json!({"key": "k", "item": i, "p": p, "label": l, "source": "computed"}),
            )
            .unwrap()
        })
        .collect()
}

/// 两极：`pos` 条读数 0.95 为真、`neg` 条 0.05 为假。
fn 两极(pos: usize, neg: usize) -> Vec<(String, f64, bool)> {
    let mut v: Vec<(String, f64, bool)> = (0..pos).map(|i| (format!("p{i}"), 0.95, true)).collect();
    v.extend((0..neg).map(|i| (format!("n{i}"), 0.05, false)));
    v
}

fn 导入(
    xs: &[(String, f64, bool)],
    sq: SeqImport,
    trial: Option<f64>,
) -> (CalibStore, jpp::truth::KeyReport) {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &行(xs), &选项(sq, trial)).unwrap();
    (store, rep.into_iter().next().unwrap())
}

/// 缺省权重下零错门槛：正式 24、试用 9。裁定 §二·1 写的「10（试用）」与研究者 `compare.py::seq_first`
/// 实算（24, 9）不符，按实算；已报主会话。
#[test]
fn 零错门槛() {
    assert_eq!(seq_first(0.1, 0.1, &W), 24);
    assert_eq!(seq_first(0.25, 0.1, &W), 9);
}

/// (a) 24 + 24 零错随机顺序 → 正式上岗，stopped_at ≤ 48；10 + 10 → 正式样本不足、试用上岗。
#[test]
fn 零错24加24正式上岗() {
    let (store, rep) = 导入(&两极(24, 24), 序贯(false, None, None, 10), None);
    assert_eq!(rep.status, "上岗", "{}", rep.truth.gate);
    let sel = store.records["k"]
        .选中的证书()
        .unwrap()
        .selection
        .clone()
        .unwrap();
    assert_eq!(sel.method, "sequential");
    let sq = sel.sequential.unwrap();
    assert!(sq.stopped_at <= 48 && sq.order == "random", "{sq:?}");
    let (store, rep) = 导入(&两极(10, 10), 序贯(false, None, None, 10), Some(0.25));
    assert!(
        rep.truth
            .gate
            .starts_with("试用上岗（α=0.25）：正式 α=0.1 未过：待核：样本不足"),
        "{}",
        rep.truth.gate
    );
    assert_eq!(
        store.records["k"].选中的证书().unwrap().grade,
        CertGrade::Trial
    );
}

/// (b) 23 + 23：正式样本不足（无抽样框时按已标正负例预检，判定等价）。
#[test]
fn 二十三条样本不足() {
    let (_, rep) = 导入(&两极(23, 23), 序贯(false, None, None, 10), None);
    assert_eq!(
        rep.truth.gate,
        "待核：样本不足（正例 23、负例 23，序贯零错误也需每侧已决 ≥ 24）"
    );
}

/// (c) 上侧有 1 条错：40 对 1 错时 E 到不了 1/δ，未停；99 对 1 错时过线。
#[test]
fn 一错要更多正确样本() {
    let mut xs = 两极(40, 30);
    xs.push(("bad".into(), 0.95, false));
    let (_, rep) = 导入(&xs, 序贯(false, None, None, 10), None);
    assert!(
        rep.truth.gate.starts_with("待核：序贯未停"),
        "{}",
        rep.truth.gate
    );
    let mut xs = 两极(99, 30);
    xs.push(("bad".into(), 0.95, false));
    let (_, rep) = 导入(&xs, 序贯(false, None, None, 10), None);
    assert_eq!(rep.status, "上岗", "{}", rep.truth.gate);
}

/// (d) 批粒度 5 与 10 的停时之差不超过 10。
#[test]
fn 批粒度只差一批() {
    let stop = |b: usize| {
        let (store, _) = 导入(&两极(40, 40), 序贯(false, None, None, b), None);
        store.records["k"]
            .选中的证书()
            .unwrap()
            .selection
            .clone()
            .unwrap()
            .sequential
            .unwrap()
            .stopped_at
    };
    let (a, b) = (stop(5), stop(10));
    assert!(a.abs_diff(b) <= 10, "{a} {b}");
}

/// 两端先标用的抽样框：上侧 24 条 0.99、24 条 0.9，下侧 24 条 0.01、24 条 0.1（共 96 条）。
fn 框() -> Vec<(String, f64)> {
    let mut f = vec![];
    for (tag, p) in [("A", 0.99), ("B", 0.9), ("C", 0.01), ("D", 0.1)] {
        f.extend((0..24).map(|i| (format!("{tag}{i}"), p)));
    }
    f
}

/// 按清单顺序标注前 `m` 条：A、C 组照真值（A 真、C 假）；B、D 组的真值由 `bd` 给。
fn 清单前缀(m: usize, bd: (bool, bool)) -> Vec<(String, f64, bool)> {
    let f = 框();
    let ps: Vec<f64> = f.iter().map(|x| x.1).collect();
    two_ends_order(&ps, 0.1, 0.1, 0.05, None, &W, 20260924)
        .into_iter()
        .take(m)
        .map(|(i, _)| {
            let (id, p) = f[i].clone();
            let t = match &id[..1] {
                "A" => true,
                "C" => false,
                "B" => bd.0,
                _ => bd.1,
            };
            (id, p, t)
        })
        .collect()
}

/// (e) **核心**（B87 §二·2）：两端先标，首组 48 条全对、次组只标了 12 条（全对）。次组候选（p ≥ 0.9）
/// 按部分标注的 E 已过线，但它的已决集没标完，按规则不判 → 覆盖目标 0.6（只有次组候选够得着）到不了，未停。
/// 对照：同一批到达、去掉顺序条件（按随机顺序读）时次组候选被认证——那就是「用首组的零错认证次组」。
#[test]
fn 首组零错不使次组通过() {
    let xs = 清单前缀(60, (true, false));
    let (_, rep) = 导入(&xs, 序贯(true, Some(框()), Some(0.6), 10), None);
    assert!(
        rep.truth.gate.starts_with("待核：序贯未停"),
        "{}",
        rep.truth.gate
    );
    // 对照：直接调序贯认证，同样的框与到达顺序，只把 two_ends 关掉
    let mut c = CalibStore::new();
    let mut 规范: Vec<(f64, bool)> = xs.iter().map(|x| (x.1, x.2)).collect();
    规范.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (p, l) in &规范 {
        c.absorb(
            "k",
            Sample {
                p: Some(*p),
                label: Some(u8::from(*l)),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: "noul".into(),
                cluster: None,
                stratum: None,
            },
        )
        .unwrap();
    }
    // 步 15d-2：按 δ 平移的认证要求记录已有 δ，调用方先 set_delta；noul 题式用 0.05。
    c.set_delta("k", 0.05).unwrap();
    let mut 用 = vec![false; 规范.len()];
    let arrival: Vec<usize> = xs
        .iter()
        .map(|x| {
            let i = (0..规范.len())
                .find(|i| !用[*i] && 规范[*i] == (x.1, x.2))
                .unwrap();
            用[i] = true;
            i
        })
        .collect();
    let spec = SeqSpec {
        pool: Some(框().iter().map(|x| x.1).collect()),
        arrival,
        two_ends: false,
        order: "random".into(),
        seed: 0,
        batch: 10,
        weights: W,
        coverage_target: Some(0.6),
    };
    c.commission_two_sided_sequential_graded("k", 0.1, 0.1, None, CertGrade::Formal, &spec)
        .expect("去掉顺序条件时次组候选被（无效地）认证");
    let r = c.get("k");
    assert!(
        r.hi + 0.05 <= 0.9 + 1e-9 || r.lo - 0.05 >= 0.1 - 1e-9,
        "至少一侧的线进了次组（次组没标完）：hi {} lo {}",
        r.hi,
        r.lo
    );
}

/// (e') 次组全错且标完：次组候选已定、不过 → settled 停在首组线对。
#[test]
fn 次组全错停在首组() {
    let xs = 清单前缀(96, (false, true));
    let (store, rep) = 导入(&xs, 序贯(true, Some(框()), None, 10), None);
    assert_eq!(rep.status, "上岗", "{}", rep.truth.gate);
    let r = &store.records["k"];
    assert!(r.hi + 0.05 > 0.9 + 1e-9, "hi {}", r.hi);
    assert!(r.lo - 0.05 < 0.1 - 1e-9, "lo {}", r.lo);
    assert_eq!(
        r.选中的证书()
            .unwrap()
            .selection
            .as_ref()
            .unwrap()
            .sequential
            .as_ref()
            .unwrap()
            .order,
        "two-ends"
    );
}

/// (f) B87 修订（步 20h-1）：零错、全标、每侧候选 ≥ seq_first 条时，序贯 settled 与同池固定序线对相同——
/// 读数分散（40 个互异读数一侧），步长 s 取 1–5 都成立（候选取 B86 序列中已决数 ≥ seq_first 者，不平移格）。
#[test]
fn 零错全标与固定序同线对() {
    let xs: Vec<(String, f64, bool)> = (0..40)
        .map(|i| (format!("u{i}"), 0.6 + i as f64 * 0.01, true))
        .chain((0..40).map(|i| (format!("d{i}"), 0.01 + i as f64 * 0.01, false)))
        .collect();
    for s in 1..=5 {
        let mut o = 选项(序贯(false, None, None, 10), None);
        o.step = Some(s);
        let mut s1 = CalibStore::new();
        // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
        s1.profile.delta =
            jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        import_labels(&mut s1, &行(&xs), &o).unwrap();
        o.certify = CertifyMethod::FixedSequence;
        let mut s2 = CalibStore::new();
        s2.profile.delta =
            jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        import_labels(&mut s2, &行(&xs), &o).unwrap();
        assert_eq!(s1.get("k").status, "上岗", "s={s}");
        assert_eq!(
            (s1.get("k").hi, s1.get("k").lo),
            (s2.get("k").hi, s2.get("k").lo),
            "s={s}"
        );
    }
}

/// (g) 同一批数据导入两次，证书逐字段相同；证书带序贯各字段。
#[test]
fn 确定性与证书字段() {
    let (a, _) = 导入(&两极(40, 40), 序贯(false, None, None, 10), None);
    let (b, _) = 导入(&两极(40, 40), 序贯(false, None, None, 10), None);
    assert_eq!(a.records["k"].certs, b.records["k"].certs);
    let sq = a.records["k"]
        .选中的证书()
        .unwrap()
        .selection
        .clone()
        .unwrap()
        .sequential
        .unwrap();
    assert_eq!((sq.batch, sq.weights, sq.seed), (10, W, 20260924));
    assert_eq!(sq.pool.len(), 80);
    assert_eq!(sq.arrival.len(), 80);
    assert!(!sq.e_upper.is_empty() && sq.e_upper.len() == sq.n_upper.len());
}

/// (h) 覆盖目标 0：第一个合法对就停，覆盖不高于 settled。
#[test]
fn 覆盖目标零首对就停() {
    let xs = 清单前缀(96, (true, false));
    let (s0, _) = 导入(&xs, 序贯(true, Some(框()), Some(0.0), 10), None);
    let (s1, _) = 导入(&xs, 序贯(true, Some(框()), None, 10), None);
    let stop = |s: &CalibStore| {
        s.records["k"]
            .选中的证书()
            .unwrap()
            .selection
            .clone()
            .unwrap()
            .sequential
            .unwrap()
            .stopped_at
    };
    assert!(stop(&s0) <= stop(&s1));
    assert!(s0.get("k").hi >= s1.get("k").hi, "τ=0 的线不宽于 settled");
}

/// (i) K 元同形：select 30 条全对 → 正式上岗（≥ 24），无下侧证书。
#[test]
fn k元序贯() {
    let rows: Vec<LabelRow> = (0..30)
        .map(|i| {
            serde_json::from_value(json!({"key": "k", "op": "select", "item": format!("m{i}"), "p": 0.9, "pick": 1, "label": 1, "source": "computed"}))
                .unwrap()
        })
        .collect();
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let rep = import_labels(&mut store, &rows, &选项(序贯(false, None, None, 10), None)).unwrap();
    assert_eq!(rep[0].status, "上岗", "{}", rep[0].truth.gate);
    assert!(store.get("k").lower.is_none());
}

/// 序贯要这次导入给出全部标注：记录里已有旧标注时拒收。
#[test]
fn 旧标注拒收() {
    let mut store = CalibStore::new();
    // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
    store.profile.delta =
        jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
    let mut o = 选项(序贯(false, None, None, 10), None);
    o.certify = CertifyMethod::FixedSequence;
    import_labels(&mut store, &行(&两极(30, 30)), &o).unwrap();
    let rep = import_labels(
        &mut store,
        &行(&两极(30, 30)),
        &选项(序贯(false, None, None, 10), None),
    )
    .unwrap();
    assert!(
        rep[0]
            .truth
            .gate
            .starts_with("待核：序贯导入要一次给出该键全部已标行"),
        "{}",
        rep[0].truth.gate
    );
}

/// (j) B87 修订 (2)（步 20h-1）：`random` 到达顺序必须与标签无关。36 条 K 元行读数全为 0.99（一个并列块、一个候选），
/// 每条以 α = 0.1 的概率错（原假设边界），300 次。修法（经 `import_labels`，按 (p, item) 排序后置换）假认证率 ≤ 0.12；
/// 旧法（按 (p, 真值) 规范序置换，直接调 API）在种子 95 上 > 0.4（裁定复算 `seq_order_check.py` 的种子扫描同形，
/// 本仓库的批 10 下扫得 95 号约 0.53）。
#[test]
fn 并列块到达顺序与标签无关() {
    const SEED: u64 = 95;
    let mut 状态: u64 = 20260925;
    let mut 随机 = move || {
        状态 = 状态.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = 状态;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    let (mut 新, mut 旧) = (0usize, 0usize);
    const 次: usize = 300;
    for _ in 0..次 {
        let 对: Vec<bool> = (0..36).map(|_| 随机() >= 0.1).collect();
        // 修法：经真值通道
        let rows: Vec<LabelRow> = 对
            .iter()
            .enumerate()
            .map(|(i, c)| {
                serde_json::from_value(
                    json!({"key": "k", "op": "select", "item": format!("m{i:02}"), "p": 0.99,
                    "pick": 1, "label": if *c { 1 } else { 0 }, "source": "computed"}),
                )
                .unwrap()
            })
            .collect();
        let mut o = 选项(序贯(false, None, None, 10), None);
        o.seed = SEED;
        let mut st = CalibStore::new();
        // 步 15d-2：import_labels 记录没有 δ 时取画像先验；这里给回步 15d-2 前的代码兜底值。
        st.profile.delta =
            jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        let rep = import_labels(&mut st, &rows, &o).unwrap();
        新 += (rep[0].status == "上岗") as usize;
        // 旧法：规范序（按 (p, 真值)，错例在前）上按种子置换
        let mut c = CalibStore::new();
        let mut 规范 = 对.clone();
        规范.sort();
        for x in &规范 {
            c.absorb(
                "k",
                Sample {
                    p: Some(0.99),
                    label: Some(u8::from(*x)),
                    perms: 0,
                    mode_share: None,
                    mode: LiteralMode::default(),
                    phys: "choice".into(),
                    cluster: None,
                    stratum: None,
                },
            )
            .unwrap();
        }
        // 步 15d-2：按 δ 平移的认证要求记录已有 δ，调用方先 set_delta；choice 题式用 0.15。
        c.set_delta("k", 0.15).unwrap();
        let spec = SeqSpec {
            pool: None,
            arrival: jpp::effects::random_arrival(36, SEED),
            two_ends: false,
            order: "random".into(),
            seed: SEED,
            batch: 10,
            weights: W,
            coverage_target: None,
        };
        旧 += c
            .commission_upper_sequential_graded("k", 0.1, 0.1, None, CertGrade::Formal, &spec)
            .is_ok() as usize;
    }
    let (新率, 旧率) = (新 as f64 / 次 as f64, 旧 as f64 / 次 as f64);
    assert!(新率 <= 0.12, "修法假认证率 {新率}");
    assert!(旧率 > 0.4, "旧法在种子 {SEED} 上的假认证率 {旧率}");
}
