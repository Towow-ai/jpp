//! 步 20a-2a：`calib-import --cost fp,fn`（B129）。代价线是导入的一种认证方式：`commission_costed_graded` 在
//! 选线半上按 `fp·#误放行 + fn·#漏放行` 最小定线，认证半上判 α 够不够，证书按结果上岗（先正式、不过再试用，
//! B72），证书带 `cost`、`selection` 记分半方法与种子（B85 分层交替分半，PR35 评审修复缺陷二：此前选线与
//! 认证用同一批样本，二项上界只对事先固定的线成立，对「在这批数据上挑出来的线」不成立）；
//! `cut(r, {cost: [fp, fn]})` 取这张证书，别的代价查不到（冷）。代价线只定一条放行线、记录 `lo = 0`：线上 `act`，
//! 线下 `unsure(band)`（`Ignore` 除 p = 0 外不可达，现行 `commission_costed` 的口径，本步照实钉住，见过程记录 Q11）。
//!
//! 另核：装载（步 20c 的重跑）复现正式与试用两种代价证书；J-16（标注集 ≠ 保形集）；只收 test 行；
//! 已有两侧线的键拒做代价认证（代价线把 `lo` 置 0，会让已有的 Ignore 出口不可达）；CLI 的参数与互斥。
//!
//! 依据：B129（`地基/附注/2026-09-25-作者主权与策略表达裁定.md` §二）；B72；B117；`21` 步 20a-2 的〔B129〕施工注；
//! `地基/过程记录/工程-步20a-2a.md`；`地基/过程记录/工程-PR35评审修复.md`（分半改动与样本量调整的算式）。

use jpp::effects::{CalibStore, EffectError, FnPort, JudgeResult, Ports};
use jpp::interp::{ActionRegistry, TaintOut};
use jpp::ledger::Ledger;
use jpp::truth::{CertifyMethod, ImportOptions, LabelRow, import_labels};
use jpp::value::{Answer, Taint, Value};
use jpp::{lower, run, syntax::parse};
use serde_json::{Value as Json, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const 模板: &str = "顾客是否要{x}？";
const 材料: &str = "顾客说要退款";
const 代价: (f64, f64) = (1.0, 10.0);

/// 判断恒给 `p`、不生成、不问人
fn 端口(p: f64) -> Ports<'static> {
    Ports::new()
        .with(FnPort::judge("m", move |_s, qs| {
            Ok(JudgeResult {
                answers: qs.iter().map(|_| Answer::Noul(p)).collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
            })
        }))
        .with(FnPort::generate("m", |_p, _c, _n, _r| {
            Err(EffectError("x".into()))
        }))
        .with(FnPort::ask("m", |_s, _q| Err(EffectError("x".into()))))
}

fn 选项(certify: CertifyMethod) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "s20a2a".into(),
        seed: 20260925,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: Some(0.25),
        certify,
        step: None,
        sequential: None,
    }
}

/// `n` 个正例（p = 0.9）与 `n` 个反例（p = 0.1）的构造真值，题式行，带与运行材料同风格的文本（有范围指纹）。
fn 行(n: usize, op: &str) -> Vec<Json> {
    (0..2 * n)
        .map(|i| {
            let yes = i % 2 == 0;
            let mut v = json!({
                "form": {"op": op, "template": 模板},
                "item": format!("m{i}"), "p": if yes { 0.9 } else { 0.1 },
                "label": yes, "source": "computed", "text": 材料
            });
            if op != "test" {
                v["label"] = json!(u8::from(!yes));
                v["pick"] = json!(0);
            }
            v
        })
        .collect()
}

fn 标注(rows: &[Json]) -> Vec<LabelRow> {
    rows.iter()
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .collect()
}

fn 空库() -> CalibStore {
    let mut store = CalibStore::new();
    // 导入要 δ（记录没有时取画像先验）；代价线本身不按 δ 平移
    store.profile.delta = jpp::effects::Field::known((0.05, 0.15, 0.15), "测试：δ 先验");
    store
}

/// 用 `--cost` 导入 `n` 对行后的库与题式键
fn 代价库(n: usize) -> (CalibStore, String) {
    let mut store = 空库();
    let rep = import_labels(
        &mut store,
        &标注(&行(n, "test")),
        &选项(CertifyMethod::Cost(代价.0, 代价.1)),
    )
    .unwrap();
    assert_eq!(rep.len(), 1);
    (store, rep[0].key.clone())
}

fn 程序(cut参: &str, act: &str) -> String {
    format!(
        r#"
budget {{calls: 4, cost: 1, depth: 8}};
let f = form("test", "{模板}", {{calib: "refund"}});
handle(cut(judge(state(mat("{材料}")), fill(f, {{x: "退款"}})){cut参}), {{
    act: fn() {{ {act} }},
    ignore: fn() {{ "ignore" }},
    unsure: fn(u) {{ consume(u, "drop"); "unsure" }}}})
"#
    )
}

fn 跑(src: &str, calib: &CalibStore, p: f64) -> Result<jpp::Outcome, String> {
    let program = lower(&parse(src).expect("解析")).expect("lower");
    let mut a = ActionRegistry::new();
    a.register("退款", 0.0, false, TaintOut::Trusted, |_| {
        Ok(Value::Text("已退".into(), Taint::Trusted.into()))
    });
    run(&program, 端口(p), calib, &a, &mut Ledger::new()).map_err(|e| e.render())
}

const 带代价: &str = ", {cost: [1, 10]}";

/// P3：正式代价证书的形状与出口。线 = 0.9（两类完全可分，选线半/认证半的分界都落在 0.1/0.9 之间，
/// 并列取更高），`lo = 0`，证书带代价、`selection` 记分半方法与种子、正式等级。
///
/// PR35 评审修复（缺陷二）：N 从 40 对提到 **50 对**——认证半只有约一半的已决正例，40 对时认证半 20 个
/// 零错正例，`ucb ≈ 0.109 > α=0.1`，只能拿到试用线；50 对时认证半 25 个，`ucb ≈ 0.088 ≤ 0.1`，正式线仍然
/// 过（算式见 `地基/过程记录/工程-PR35评审修复.md` §二）。
#[test]
fn 代价线证书与出口() {
    let (store, key) = 代价库(50);
    let rec = &store.records[&key];
    assert_eq!(rec.status, "上岗");
    assert_eq!((rec.hi, rec.lo), (0.9, 0.0), "代价线只有放行线，lo 置 0");
    assert!(rec.lower.is_none(), "代价线没有下侧证书");
    assert_eq!(rec.certs.len(), 1);
    let c = rec.certs.values().next().unwrap();
    assert_eq!(c.cost, Some(代价));
    let sel = c.selection.as_ref().expect("代价分半（B85）要写 selection");
    assert_eq!(sel.delta, Some(0.0), "代价线不按 δ 平移：显式声明 δ=0");
    assert!(
        sel.method.starts_with("cost-"),
        "方法名前缀 cost-：{}",
        sel.method
    );
    assert_eq!(c.grade, jpp::effects::CertGrade::Formal);
    // P7：J-16 前置——标注集 id 与保形集 id 不同源
    assert_eq!(rec.label_set_id, "truth:s20a2a");
    assert_ne!(rec.label_set_id, rec.set_id);

    let 出口 = |p: f64, 参: &str| {
        let o = 跑(&程序(参, "\"act\""), &store, p).unwrap();
        (o.value_json(), o.exits[0]["grade"].clone())
    };
    assert_eq!(出口(0.95, 带代价), (json!("act"), json!("Form")));
    assert_eq!(出口(0.9, 带代价).0, json!("act"), "p ≥ hi 即放行一侧");
    assert_eq!(
        出口(0.5, 带代价).0,
        json!("unsure"),
        "线下是 band，不是 ignore"
    );
    assert_eq!(
        出口(0.05, 带代价).0,
        json!("unsure"),
        "lo = 0：Ignore 不可达"
    );
    // 不带代价的 cut 按记录的线切，结果同
    assert_eq!(出口(0.95, "").0, json!("act"));
    assert_eq!(出口(0.5, "").0, json!("unsure"));
    // 换一对代价：没有这张证书，冷
    let o = 跑(&程序(", {cost: [2, 10]}", "\"act\""), &store, 0.95).unwrap();
    assert_eq!(o.value_json(), json!("unsure"));
    assert!(
        o.trace
            .warnings
            .iter()
            .any(|w| w.contains("W-untested") && w.contains("cost_line")),
        "{:?}",
        o.trace.warnings
    );
}

/// P3：正式代价线放行不可逆 do；试用代价线（正式 α 不过、试用 α 过）路由但不放行，J-08 拒。
///
/// PR35 评审修复（缺陷二）：正式那半 `代价库(40)` → `代价库(50)`（理由同 `代价线证书与出口`）；
/// 试用那半 `代价库(12)` → `代价库(20)`——12 对时认证半只有 6 个零错正例，`ucb ≈ 0.319`，正式（0.1）
/// 与试用（0.25）都过不了，会导致后面取 `certs.values().next().unwrap()` 直接 panic；
/// 20 对时认证半 10 个，`ucb ≈ 0.206`，正式过不了但试用过，符合这个测试本来要测的「试用线」场景。
#[test]
fn 正式代价线放行_试用代价线不放行() {
    let (正式, _) = 代价库(50);
    let o = 跑(&程序(带代价, "content(do(\"退款\", [], 0))"), &正式, 0.95).unwrap();
    assert_eq!(o.value_json(), json!("已退"));
    assert_eq!(o.exits[0]["releases"], json!(true));

    // 20 对零错：正式 α 0.1 的上界约 0.206 不过，试用 α 0.25 过（B72）
    let (试用, key) = 代价库(20);
    let c = 试用.records[&key].certs.values().next().unwrap();
    assert_eq!(c.grade, jpp::effects::CertGrade::Trial);
    assert_eq!(c.cost, Some(代价));
    let o = 跑(&程序(带代价, "\"act\""), &试用, 0.95).unwrap();
    assert_eq!(o.value_json(), json!("act"));
    assert_eq!(o.exits[0]["grade"], json!("Trial"));
    assert_eq!(o.exits[0]["releases"], json!(false));
    let e = 跑(&程序(带代价, "content(do(\"退款\", [], 0))"), &试用, 0.95)
        .expect_err("试用代价线不放行不可逆 do");
    assert!(e.contains("J-08"), "{e}");
}

fn 临时(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("jpp-s20a2a-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    d
}

/// P4：存盘再装载（步 20c 按证书重跑），正式与试用两种代价证书都复现，不降夹具。
///
/// PR35 评审修复（缺陷二）：N 值同 `正式代价线放行_试用代价线不放行` 的理由，`(40, "formal")` →
/// `(50, "formal")`、`(12, "trial")` → `(20, "trial")`；重跑走证书自带的 `selection.seed`
/// （见 `rerun.rs` 新增的 `cost-split-stratified` 分支），同一批样本、同一颗种子确定性复现。
#[test]
fn 装载重跑复现代价证书() {
    for (n, tag) in [(50, "formal"), (20, "trial")] {
        let (store, key) = 代价库(n);
        let d = 临时(tag);
        store.save(&d).unwrap();
        let s = CalibStore::load(&d).unwrap();
        assert!(s.load_report.is_empty(), "{tag}：{:?}", s.load_report);
        let r = &s.records[&key];
        assert!(!r.fixture, "{tag}：不降夹具");
        assert_eq!(r.certs, store.records[&key].certs, "{tag}：证书逐字段复现");
        let _ = fs::remove_dir_all(&d);
    }
}

/// P6（API 层）：只收 test 行；已有两侧线的键拒做代价认证；只有代价证书的键可以再加一对代价。
#[test]
fn 代价导入的前置() {
    let mut store = 空库();
    let e = import_labels(
        &mut store,
        &标注(&行(40, "select")),
        &选项(CertifyMethod::Cost(1.0, 10.0)),
    )
    .expect_err("select 行不收");
    assert!(e.contains("--cost 只收 test 行"), "{e}");
    assert!(store.records.is_empty(), "拒收时不写任何记录");

    let mut 两侧 = 空库();
    import_labels(
        &mut 两侧,
        &标注(&行(40, "test")),
        &选项(CertifyMethod::FixedSequence),
    )
    .unwrap();
    let 前 = 两侧.records.clone();
    let e = import_labels(
        &mut 两侧,
        &标注(&行(40, "test")),
        &选项(CertifyMethod::Cost(1.0, 10.0)),
    )
    .expect_err("已有两侧线");
    assert!(e.contains("Ignore 出口不可达"), "{e}");
    assert_eq!(两侧.records, 前, "拒收时记录不动");

    let (mut 仅代价, key) = 代价库(40);
    let rep = import_labels(
        &mut 仅代价,
        &标注(&行(40, "test")),
        &选项(CertifyMethod::Cost(10.0, 1.0)),
    )
    .unwrap();
    // 报告说的是这次认证的那张（代价证书没有 selection，不能按地址排序取第一张）
    assert_eq!(
        rep[0].certification.as_ref().unwrap().cost,
        Some((10.0, 1.0))
    );
    let costs: Vec<_> = 仅代价.records[&key]
        .certs
        .values()
        .map(|c| c.cost)
        .collect();
    assert!(
        costs.contains(&Some((1.0, 10.0))) && costs.contains(&Some((10.0, 1.0))),
        "{costs:?}"
    );
}

// ---------------------------------------------------------------- CLI

const 画像: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/delta_prior_legacy.json");

fn jpp(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn 写行(dir: &Path, name: &str, rows: &[Json]) {
    let text: String = rows.iter().map(|r| format!("{r}\n")).collect();
    fs::write(dir.join(name), text).unwrap();
}

/// P6（CLI）：`--cost 1,10` 导入成功，报告带代价、selection 为空；坏值与互斥参数是用法错误；select 行与已有两侧线拒收且不写目录。
#[test]
fn cli_cost参数() {
    let d = 临时("cli");
    fs::create_dir_all(&d).unwrap();
    写行(&d, "t.jsonl", &行(40, "test"));
    写行(&d, "s.jsonl", &行(40, "select"));
    let o = jpp(
        &d,
        &[
            "calib-import",
            "t.jsonl",
            "--calib-out",
            "out",
            "--profile",
            画像,
            "--cost",
            "1,10",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rep: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rep[0]["certification"]["cost"], json!([1.0, 10.0]));
    // PR35 评审修复（缺陷二）：代价线现在也分半，selection 不再是 null，记方法与种子
    assert!(
        rep[0]["certification"]["selection"]["method"]
            .as_str()
            .is_some_and(|m| m.starts_with("cost-")),
        "{}",
        rep[0]
    );
    assert_eq!(rep[0]["certification"]["selection"]["delta"], json!(0.0));
    assert_eq!(rep[0]["status"], json!("上岗"));

    for bad in ["1", "0,1", "a,b", "-1,2"] {
        let o = jpp(
            &d,
            &[
                "calib-import",
                "t.jsonl",
                "--calib-out",
                "o2",
                "--profile",
                画像,
                "--cost",
                bad,
            ],
        );
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(
            !o.status.success() && err.contains("--cost expects"),
            "{bad}：{err}"
        );
    }
    for (flag, v) in [
        ("--certify", "split"),
        ("--order", "random"),
        ("--step", "3"),
        ("--batch", "5"),
        ("--coverage-target", "0.5"),
        ("--mix-weights", "0.8,0.1,0.05,0.05"),
        ("--extend-scope", "k"),
    ] {
        let o = jpp(
            &d,
            &[
                "calib-import",
                "t.jsonl",
                "--calib-out",
                "o3",
                "--profile",
                画像,
                flag,
                v,
                "--cost",
                "1,10",
            ],
        );
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(
            !o.status.success() && err.contains("--cost cannot be combined with"),
            "{flag}：{err}"
        );
    }
    let o = jpp(
        &d,
        &[
            "calib-import",
            "s.jsonl",
            "--calib-out",
            "o4",
            "--profile",
            画像,
            "--cost",
            "1,10",
        ],
    );
    assert!(
        !o.status.success() && String::from_utf8_lossy(&o.stderr).contains("--cost 只收 test 行")
    );
    assert!(!d.join("o4").exists(), "拒收时不写目录");

    // 先普通导入（两侧线），再带 --calib 做代价认证：拒收
    let o = jpp(
        &d,
        &[
            "calib-import",
            "t.jsonl",
            "--calib-out",
            "two",
            "--profile",
            画像,
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = jpp(
        &d,
        &[
            "calib-import",
            "t.jsonl",
            "--calib",
            "two",
            "--calib-out",
            "o5",
            "--profile",
            画像,
            "--cost",
            "1,10",
        ],
    );
    assert!(
        !o.status.success() && String::from_utf8_lossy(&o.stderr).contains("Ignore 出口不可达")
    );
    assert!(!d.join("o5").exists());
    let _ = fs::remove_dir_all(&d);
}

/// P6（CLI）：`--from-ledger` 与 `--cost` 可同给：代价线取代回填缺省的序贯，也不按两端框，所以一次回填两个键可以；
/// 不带 `--cost` 的回填照旧按两端框、一次只收一个键（本步不改）。
#[test]
fn cli_回填加代价() {
    use jpp::value::{Mat, State};
    let d = 临时("ledger");
    fs::create_dir_all(&d).unwrap();
    let mats: Vec<String> = (0..40)
        .map(|i| format!("第{i}段对话：顾客说了些话。"))
        .collect();
    let 真 = |i: usize| i.is_multiple_of(2);
    let mut obs = vec![];
    for (i, m) in mats.iter().enumerate() {
        for (text, key) in [("甲？", "k1"), ("乙？", "k2")] {
            obs.push(json!({"on": [m], "op": "test", "text": text, "calib": key,
                            "answer": {"Noul": if 真(i) { 0.9 } else { 0.1 }}}));
        }
    }
    fs::write(
        d.join("fx.json"),
        json!({ "observations": obs }).to_string(),
    )
    .unwrap();
    let list = mats
        .iter()
        .map(|m| format!("{m:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        d.join("p.jpp"),
        format!(
            "budget {{calls: 200, cost: 1, depth: 256}};\nsieve([{list}], [test(\"甲？\", \"k1\"), test(\"乙？\", \"k2\")])\n"
        ),
    )
    .unwrap();
    let o = jpp(
        &d,
        &[
            "run",
            "p.jpp",
            "--fixtures",
            "fx.json",
            "--ledger-out",
            "led.jsonl",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rows: Vec<Json> = mats
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            let item = State::new(
                vec![Mat::literal(Json::String(m.clone()))],
                vec![],
                vec![],
                vec![],
                false,
            )
            .hash;
            ["k1", "k2"]
                .map(|k| json!({"key": k, "item": item, "label": 真(i), "source": "computed"}))
        })
        .collect();
    写行(&d, "labels.jsonl", &rows);
    let o = jpp(
        &d,
        &[
            "calib-import",
            "labels.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--calib-out",
            "out",
            "--profile",
            画像,
            "--cost",
            "1,10",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rep: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rep.as_array().unwrap().len(), 2);
    for r in rep.as_array().unwrap() {
        assert_eq!(r["certification"]["cost"], json!([1.0, 10.0]), "{r}");
    }
    // 不带 --cost：回填缺省序贯、两端框，一次只收一个键（照旧）
    let o = jpp(
        &d,
        &[
            "calib-import",
            "labels.jsonl",
            "--from-ledger",
            "led.jsonl",
            "--calib-out",
            "out2",
            "--profile",
            画像,
        ],
    );
    assert!(
        !o.status.success() && String::from_utf8_lossy(&o.stderr).contains("一次只收一个键"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let _ = fs::remove_dir_all(&d);
}
