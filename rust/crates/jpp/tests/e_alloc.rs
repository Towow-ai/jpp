//! E-ALLOC：不确定度能不能排出「哪些判断更可能错」——在**真模型的读数**上。
//!
//! 预注册见 `foundation/experiments/EXPERIMENTS.md` 的 E-ALLOC 一节（写于跑之前）。
//! 数据是 E-CAL 已经花过钱的 297 条真机读数，这里再用一次，**边际成本为零**。
//! 用的是第五包交付的 `jpp::strength::allocate` 本体，不另写一份 Python——
//! 要验的就是那个实现在真读数上还成不成立。
//!
//! 两处必要适配（理由写在预注册里）：297 条出口全是 `unsure(cold)`，所以「错误」改成
//! **读数所蕴含的判断**与真值不符，解码沿用 `e_cal_写校准记录.py:256-268` 的同一套映射；
//! 只有 202 条有真值（noul 73 / choice 74 / score 55）。
//!
//! 数据不在仓库里时这条测试**跳过**（公开发布的 rust 目录不带实验数据）。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use jpp::effects::CalibStore;
use jpp::strength::allocate;
use jpp::value::{Answer, Op, Reading};
use serde_json::Value as Json;

fn 数据目录() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../foundation/experiments")
}

/// 档案字段从 `tests/oracle/oracle.json` 读——那份是 Python 内核实跑记下来的，不在这里重写常数
fn 档案() -> (f64, f64, (f64, f64, f64)) {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/oracle.json");
    let j: Json =
        serde_json::from_str(&std::fs::read_to_string(p).expect("对照基准在")).expect("合法 JSON");
    let s = j["profile"]["safety_lines"].as_array().unwrap();
    let d = &j["profile"]["delta"];
    (
        s[0].as_f64().unwrap(),
        s[1].as_f64().unwrap(),
        (
            d["noul"].as_f64().unwrap(),
            d["choice"].as_f64().unwrap(),
            d["score"].as_f64().unwrap(),
        ),
    )
}

struct 条 {
    op: Op,
    p: f64,
    /// 这条读数**所蕴含的判断**对不对
    对: bool,
}

/// 真值解码：与 `e_cal_写校准记录.py:256-268` 同一套映射，不自创
fn 判对(题型: &str, truth: &str, value: Option<i64>, p: f64) -> Option<bool> {
    if truth.is_empty() {
        return None;
    }
    match 题型 {
        "noul" => Some((p >= 0.5) == (truth == "act")),
        "choice" => {
            let letter = match truth {
                "A" => 0,
                "B" => 1,
                "C" => 2,
                "D" => 3,
                "都不是" => 4,
                _ => return None,
            };
            Some(value? == letter)
        }
        "score" => Some(value? + 1 == truth.parse::<i64>().ok()?),
        _ => None,
    }
}

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

fn 读数(c: &条, 键: &str) -> Rc<Reading> {
    let r = Rc::new(Reading {
        q_hash: String::new(),
        state_hash: String::new(),
        op: c.op,
        calib: 键.into(),
        id: 新句柄(),
        fail: None,
        model_id: "jev".into(),
        ledger_key: String::new(),
        over_len: 0,
        scale: vec![],
        perms: Default::default(),
        mode_share: Default::default(),
        missing_evidence: vec![],
        state_taint: Default::default(),
        form_hash: None,
        fp: None,
    });
    记答(&r, Answer::Noul(c.p));
    r
}

/// 错误率：被复核的那些按真值算（不算错），其余按读数所蕴含的判断算
fn 错误率(条们: &[条], 复核: &[usize]) -> f64 {
    let n = 条们.len();
    let 错 = 条们
        .iter()
        .enumerate()
        .filter(|(i, c)| !复核.contains(i) && !c.对)
        .count();
    错 as f64 / n as f64
}

/// 确定性随机抽样（与 tests/strength.rs 同一套，不引第三方 crate）
fn 随机抽(seed: u64, n: usize, k: usize) -> Vec<usize> {
    let mut s = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut pool: Vec<usize> = (0..n).collect();
    let mut out = vec![];
    for _ in 0..k.min(n) {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (s >> 33) as usize % pool.len();
        out.push(pool.remove(j));
    }
    out
}

#[test]
fn e_alloc_真读数上不确定度能不能排出错误() {
    let path = 数据目录().join("raw/e_cal/readings.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!(
            "跳过 E-ALLOC：读不到 {}（公开发布的目录不带实验数据）",
            path.display()
        );
        return;
    };
    let (hi, lo, (dn, dc, ds)) = 档案();
    // 两种配置各跑一轮：
    // 冷 —— 那次真跑时的实际状态（三个键都没有记录，线取档案保守线）。实测带内 85–89%，
    //       不确定度一律并列为 0，`allocate` 退化成按文件顺序取前 k 条——量的不是不确定度。
    // 上岗 —— `e_cal_写校准记录.py` 扫出来的线（`e_cal_曲线.md` §4 的表）。这一轮才回答得了
    //       「不确定度能不能排出错误」。两轮的数字都照录，不拿后一轮替换前一轮。
    let 冷 = {
        let mut c = CalibStore::new();
        c.profile.safety = jpp::effects::Field::known((hi, lo), "档案");
        c.profile.delta = jpp::effects::Field::known((dn, dc, ds), "档案");
        c
    };
    let 上岗 = {
        let mut c = CalibStore::new();
        c.profile.delta = jpp::effects::Field::known((dn, dc, ds), "档案");
        c.put("noul", 0.66, 0.56, 73, "上岗", Some(0.05)).unwrap();
        c.set_delta("noul", 0.04).unwrap();
        // choice / score 只有上线，没有下线：按 lo = 0 处理
        c.put("choice", 0.35, 0.0, 74, "上岗", Some(0.05)).unwrap();
        c.set_delta("choice", 0.04).unwrap();
        c.put("score", 0.59, 0.0, 55, "上岗", Some(0.05)).unwrap();
        c.set_delta("score", 0.1141).unwrap();
        c
    };

    let mut 按题型: BTreeMap<String, Vec<条>> = BTreeMap::new();
    let mut 无真值 = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let d: Json = serde_json::from_str(line).expect("每行都是合法 JSON");
        let 题型 = d["type"].as_str().unwrap().to_string();
        let ans = &d["ans"];
        let p = ans["p"].as_f64().unwrap_or(0.0);
        let value = ans["value"].as_i64();
        let truth = d["truth"].as_str().unwrap_or("");
        // 两栏分报，不合并：真值强度不同。主栏 = 模型双标定下来的真值；
        // 核对臂 = 没定真值但两个模型判一致的，拿一致标签当近似真值。
        let (栏, 用) = if !truth.is_empty() {
            ("主栏", truth.to_string())
        } else if d["fable"].as_str().is_some() && d["fable"] == d["opus"] {
            ("核对臂", d["fable"].as_str().unwrap().to_string())
        } else {
            无真值 += 1;
            continue;
        };
        let Some(对) = 判对(&题型, &用, value, p) else {
            无真值 += 1;
            continue;
        };
        // δ 按 ans.phys 取（choice 多数被 K-noul 下沉成 noul，那次跑的就是这个物理形式）
        let op = match ans["phys"].as_str().unwrap_or("noul") {
            "choice" => Op::Select,
            "score" => Op::Measure,
            _ => Op::Test,
        };
        按题型
            .entry(format!("{栏}/{题型}"))
            .or_default()
            .push(条 { op, p, 对 });
    }

    println!("\n=== E-ALLOC：真读数上，按不确定度分配复核 vs 随机分配 ===");
    println!("无真值（模型双标未覆盖）跳过 {无真值} 条");
    // k 取到样本的三成上下就够；原来取到 50 时 score（n=55）已经复核掉九成，那一头的差值没有意义
    let ks = [0usize, 3, 5, 8, 12, 20];
    for (轮, calib) in [
        ("冷（那次真跑的实际状态）", &冷),
        ("上岗（扫出来的线）", &上岗),
    ] {
        println!("\n######## {轮}");
        for (题型, 条们) in &按题型 {
            let n = 条们.len();
            // 校准键只取题型那一半：格名是「栏/题型」，前缀不是键
            let 键 = 题型.split('/').next_back().unwrap();
            let rs: Vec<Rc<Reading>> = 条们.iter().map(|c| 读数(c, 键)).collect();
            // 并列为 0 的比例：它直接决定 allocate 有没有区分力
            let 并列 = rs
                .iter()
                .filter(|r| jpp::strength::uncertainty(calib, &答表(), r) == Some(0.0))
                .count();
            println!(
                "--- {题型}（n={n}，不复核错误率={:.3}，不确定度并列为 0 的 {并列} 条 = {:.0}%）",
                错误率(条们, &[]),
                并列 as f64 / n as f64 * 100.0
            );
            println!(
                "  {:>4} {:>12} {:>14} {:>10}",
                "k", "按不确定度", "随机(200种子)", "差"
            );
            for k in ks {
                if k > n {
                    continue;
                }
                let a = 错误率(条们, &allocate(calib, &答表(), &rs, k));
                let mut acc = 0.0;
                for seed in 0..200u64 {
                    acc += 错误率(条们, &随机抽(seed, n, k));
                }
                let r = acc / 200.0;
                println!("  {k:>4} {a:>12.3} {r:>14.3} {:>10.3}", r - a);
            }
        }
    }
    // 这条测试的作用是**产出证据**，不是把赌注钉成断言：赌中了固然好，赌错了也要留下数字。
    // 唯一的硬断言是数据读进来了、三种题型都有样本——否则打印出来的是空表。
    assert_eq!(
        按题型.len(),
        6,
        "两栏 × 三种题型都该有样本，实际 {:?}",
        按题型.keys().collect::<Vec<_>>()
    );
    for (格, 条们) in &按题型 {
        assert!(条们.len() >= 20, "{格} 只有 {} 条，太少", 条们.len());
    }
}
