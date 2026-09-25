//! `jpp calib-import`：真值通道的命令行入口（B19）。逻辑在 `jpp::truth`。
//! 步 20h：`--from-ledger` 以首跑账本作抽样框（B88）——导出待标清单（`--list-out`）或回填导入。
use crate::options::ImportArgs;
use jpp::{
    effects::{CalibStore, Profile},
    truth::{CertifyMethod, ImportOptions, LabelRow, ScopeMargins, SeqImport, import_labels},
};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::fs;

/// 抽样框的一条（B107，步 20h-2）：材料标识（状态哈希）、题哈希、读数；K 元读数另带 argmax
#[derive(Clone, Debug)]
struct 框行 {
    item: String,
    q: String,
    p: f64,
    pick: Option<usize>,
}

/// 键 → 抽样框
type 框表 = BTreeMap<String, Vec<框行>>;

/// 账本里的抽样框（B88、B107）：键 → 该键下全部 `(状态哈希, 题哈希, 读数)`，按首次出现的顺序；
/// 同一 `(状态, 题)` 只取第一条（`repeat` 的多次读数走含 n 的独立键，B28）。
/// 是非题收 `Noul`；K 元（`select`/`measure`）收 `p_max` 与 argmax。
fn 账本框(path: &std::path::Path) -> Result<(框表, String), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // PR35 评审修复（缺陷一）：此前直接逐行 `serde_json::from_str`，只跳过头行，从不检查版本、
    // `seq` 序号与 `prev` 哈希链——改过某条 `Judge.answer` 但链没有重算的账本，仍会被当合法抽样框。
    // 改成先用 `read_any` 解码整份文本（v3 校验链；v2 在内存迁移；链断、未知字段、完整行读不成报
    // `E-ledger-corrupt`），出错直接返回错误、不往下走；再用解码出的 `Ledger::encode()` 得到的规范
    // v3 文本，复用下面既有的逐行取值逻辑（末行半写的截断已经在 `read_any`/`decode` 里处理过）。
    // 指纹仍对**原始文件文本**取哈希，不对重编码后的文本取——这个指纹是「这份账本文件」的身份。
    // 参照 `crates/jpp/src/cli/run_io.rs:224` 的 `--replay`/`--resume` 路径同一处理。
    let (ledger, truncated, note) = jpp::store::migrations::ledger_v2::read_any(&raw)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(n) = &note {
        eprintln!("{}: {n}", path.display());
    }
    if let Some(t) = &truncated {
        eprintln!("{}: {}", path.display(), t.render());
    }
    let text = ledger.encode();
    let mut out: 框表 = BTreeMap::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let v: Json = serde_json::from_str(line)
            .map_err(|e| format!("{} 第 {} 行：{e}", path.display(), i + 1))?;
        let Some(j) = v.get("entry").and_then(|e| e.get("Judge")) else {
            continue;
        };
        let (Some(k), Some(st), Some(q)) = (
            j["calib_ref"]["declared"].as_str(),
            j["jkey"]["state"].as_str(),
            j["jkey"]["q"].as_str(),
        ) else {
            continue;
        };
        let (p, pick) = if let Some(p) = j["answer"]["Noul"].as_f64() {
            (p, None)
        } else if let Some(v) = j["answer"]["Choice"]
            .as_array()
            .or(j["answer"]["Score"].as_array())
        {
            // 依据：B107（K 元读数的框同形：p_max 与 argmax 由回接给出，清单不带）
            let ps: Vec<f64> = v.iter().filter_map(Json::as_f64).collect();
            let Some((k, m)) =
                ps.iter()
                    .enumerate()
                    .fold(None, |b: Option<(usize, f64)>, (i, x)| match b {
                        Some((_, bm)) if bm >= *x => b,
                        _ => Some((i, *x)),
                    })
            else {
                continue;
            };
            (m, Some(k))
        } else {
            continue;
        };
        let xs = out.entry(k.to_string()).or_default();
        if !xs.iter().any(|x| x.item == st && x.q == q) {
            xs.push(框行 {
                item: st.to_string(),
                q: q.to_string(),
                p,
                pick,
            });
        }
    }
    Ok((out, jpp::value::hash_of(&[&raw])))
}

/// 运行报告的 `questions` 表（B107、B120 (a)）：题哈希 → 该行（`template`、`fill`、`kind`）
fn 报告题表(path: &std::path::Path) -> Result<BTreeMap<String, Json>, String> {
    let v: Json = serde_json::from_str(
        &fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(v["questions"]
        .as_array()
        .map(|xs| {
            xs.iter()
                .filter_map(|x| x["q"].as_str().map(|q| (q.to_string(), x.clone())))
                .collect()
        })
        .unwrap_or_default())
}

/// 标注行在报告 `questions` 表里的题类（B120 (a)）：行带 `q` 按 `q` 取；否则行带 `form` 时按题式哈希取，
/// 该题式在报告里只有一种题类才取（多种就不猜，落回基础类）。
fn 报告题类(表: &BTreeMap<String, Json>, v: &Json) -> Option<Json> {
    if let Some(q) = v["q"].as_str() {
        return 表.get(q).map(|r| r["kind"].clone());
    }
    let spec: jpp::truth::FormSpec = serde_json::from_value(v.get("form")?.clone()).ok()?;
    let fk = spec.key().ok()?;
    let h = fk.rsplit('\u{1f}').next()?.to_string();
    let kinds: std::collections::BTreeSet<String> = 表
        .values()
        .filter(|r| r["form_hash"].as_str() == Some(h.as_str()))
        .map(|r| r["kind"].to_string())
        .collect();
    match kinds.len() {
        1 => serde_json::from_str(kinds.iter().next()?).ok(),
        _ => None,
    }
}

/// 材料文本 → 状态哈希（与 `state(mat(文本))` 同算法），给清单行附上材料本身。
fn 材料表(path: &std::path::Path) -> Result<BTreeMap<String, String>, String> {
    let texts: Vec<String> = serde_json::from_str(
        &fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
    )
    .map_err(|e| format!("{}：须是 JSON 字符串数组（{e}）", path.display()))?;
    use jpp::value::{Mat, State};
    Ok(texts
        .into_iter()
        .map(|t| {
            (
                State::new(
                    vec![Mat::literal(Json::String(t.clone()))],
                    vec![],
                    vec![],
                    vec![],
                    false,
                )
                .hash,
                t,
            )
        })
        .collect())
}

fn 库(a: &ImportArgs) -> Result<CalibStore, String> {
    // 步 20c：装载即按证书重跑认证（B117，与画像无关）；降级与改写报到 stderr
    let mut store = match &a.calib {
        Some(d) => {
            let s = CalibStore::load(d).map_err(|e| format!("{}: {e}", d.display()))?;
            for line in &s.load_report {
                eprintln!("{line}");
            }
            s
        }
        None => CalibStore::new(),
    };
    if let Some(p) = &a.profile {
        store.profile = Profile::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(store)
}

/// B88：导出待标清单。行只带 `item`（状态哈希）、`group`，给了材料表时另带 `material`；不带读数与出口。
fn 导出清单(a: &ImportArgs) -> Result<(), String> {
    let (ledger, key, out) = (
        a.from_ledger.as_ref().unwrap(),
        a.key.as_ref().unwrap(),
        a.list_out.as_ref().unwrap(),
    );
    let (框, 账本哈希) = 账本框(ledger)?;
    let frame = 框
        .get(key)
        .ok_or_else(|| format!("账本里没有键 {key} 的读数"))?;
    let store = 库(a)?;
    let op = if frame.iter().any(|x| x.pick.is_some()) {
        jpp::value::Op::Select
    } else {
        jpp::value::Op::Test
    };
    // 步 15d-2：两端先标的 δ 取记录的，记录没有取 --profile 画像的 δ 先验，都没有报错
    let delta = store
        .delta_for(&store.get(key), op)
        .or_else(|| store.profile.delta_prior(op))
        .ok_or(
            "两端先标要 δ：记录没有、画像也没有 δ 先验。修法：带 --profile <画像>（步 15d-2）",
        )?;
    let ps: Vec<f64> = frame.iter().map(|x| x.p).collect();
    let 题表 = match &a.report {
        Some(r) => Some(报告题表(r)?),
        None => None,
    };
    let 题数 = frame
        .iter()
        .map(|x| x.q.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if 题数 > 1 && 题表.is_none() {
        // 依据：B107（多题键不带报告，清单行看不见题面）
        eprintln!(
            "W-list-no-question: 键 {key} 下有 {题数} 道题，清单行只有题哈希 q；用 --report <首跑的 report.json> 给每行附上题面与填法（B107）"
        );
    }
    let order = jpp::effects::two_ends_order(
        &ps,
        a.alpha,
        a.conf_delta,
        delta,
        a.step,
        &a.weights,
        a.seed,
    );
    let 材料 = match &a.materials {
        Some(m) => Some(材料表(m)?),
        None => None,
    };
    let mut lines = vec![json!({"list": {"key": key, "seed": a.seed, "ledger": 账本哈希, "n": frame.len(), "rule": "two-ends/v1"}}).to_string()];
    let mut 对上 = 0;
    for (i, g) in &order {
        let x = &frame[*i];
        let item = &x.item;
        // B107：清单行带题哈希 q（同一材料可被问多道题）；带报告时附题面与填法（作者写的题，不是判断器的输出）
        let mut row = json!({"item": item, "q": x.q, "group": g});
        if let Some(t) = 材料.as_ref().and_then(|m| m.get(item)) {
            row["material"] = json!(t);
            对上 += 1;
        }
        if let Some(qr) = 题表.as_ref().and_then(|t| t.get(&x.q)) {
            for f in ["template", "fill"] {
                if let Some(v) = qr.get(f) {
                    row[f] = v.clone();
                }
            }
        }
        lines.push(row.to_string());
    }
    fs::write(out, lines.join("\n") + "\n").map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!(
        "待标清单：键 {key}，框 {} 条，写到 {}（不带读数与出口，B88）{}",
        frame.len(),
        out.display(),
        if 材料.is_some() {
            format!("；材料对上 {对上}/{}", frame.len())
        } else {
            String::new()
        }
    );
    Ok(())
}

/// B91（步 20d-2）：范围扩展认证。标注行读 `p`、`label`（布尔；K 元为 argmax 是否正确）、`text`，每行都要带文本。
fn 扩展范围(a: &ImportArgs, key: &str) -> Result<(), String> {
    let labels = a.labels.as_ref().expect("已校验");
    let calib_out = a.calib_out.as_ref().expect("已校验");
    if a.calib.is_none() {
        return Err("--extend-scope 要用 --calib 给出已有记录的目录".into());
    }
    let text = fs::read_to_string(labels).map_err(|e| format!("{}: {e}", labels.display()))?;
    let mut rows = vec![];
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let at = |e: &str| format!("{} 第 {} 行：{e}", labels.display(), i + 1);
        let v: Json = serde_json::from_str(line).map_err(|e| at(&e.to_string()))?;
        let p = v["p"].as_f64().ok_or_else(|| at("缺 p"))?;
        let label = v["label"]
            .as_bool()
            .ok_or_else(|| at("label 须是 true / false"))?;
        let t = v["text"]
            .as_str()
            .ok_or_else(|| at("扩展行须带 text（扩展的就是这批材料的风格指纹，B91）"))?;
        rows.push(jpp::effects::ExtendRow {
            p,
            label,
            text: t.to_string(),
        });
    }
    let mut store = 库(a)?;
    let batch = labels
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = store.extend_scope(
        key,
        &rows,
        &jpp::effects::ExtendOptions {
            alpha_trial: Some(a.alpha_trial),
            quantiles: a.scope_quantiles,
            margins: ScopeMargins {
                k: a.scope_margins.0,
                m: a.scope_margins.1,
            },
            batch,
        },
    )?;
    store
        .save(calib_out)
        .map_err(|e| format!("{}: {e}", calib_out.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"key": key, "extension": {"n_up": ext.n_up, "n_down": ext.n_down, "alpha": ext.alpha, "batch": ext.batch}}))
            .map_err(|e| e.to_string())?
    );
    eprintln!(
        "范围扩展（B91）：键 {key} 并入一条扩展，α={}（上侧已决 {}、下侧已决 {}），写回 {}",
        ext.alpha,
        ext.n_up,
        ext.n_down,
        calib_out.display()
    );
    Ok(())
}

pub fn run(a: &ImportArgs) -> Result<(), String> {
    if a.list_out.is_some() {
        return 导出清单(a);
    }
    if let Some(k) = a.extend_scope.clone() {
        return 扩展范围(a, &k);
    }
    let labels = a.labels.as_ref().expect("已校验");
    let calib_out = a.calib_out.as_ref().expect("已校验");
    let text = fs::read_to_string(labels).map_err(|e| format!("{}: {e}", labels.display()))?;
    // 回填导入（B88）：按 item 从账本回接读数；行里给了读数且与账本不同则拒收
    let 框 = match &a.from_ledger {
        Some(l) => Some(账本框(l)?.0),
        None => None,
    };
    let 题表 = match &a.report {
        Some(r) => Some(报告题表(r)?),
        None => None,
    };
    let mut rows: Vec<LabelRow> = vec![];
    let mut 键们: std::collections::BTreeSet<String> = Default::default();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let at = |e: String| format!("{} 第 {} 行：{e}", labels.display(), i + 1);
        let mut v: Json = serde_json::from_str(line).map_err(|e| at(e.to_string()))?;
        if v.get("list").is_some() {
            continue; // 待标清单的头行
        }
        if let Some(f) = &框 {
            let k = v["key"]
                .as_str()
                .ok_or_else(|| at("--from-ledger 回填的行须给 key".into()))?
                .to_string();
            let item = v["item"]
                .as_str()
                .ok_or_else(|| at("缺 item".into()))?
                .to_string();
            // B107：按 (key, item, q) 回接；行里没有 q 而该材料在该键下有多个读数时不猜
            let 候选: Vec<&框行> = f
                .get(&k)
                .map(|xs| {
                    xs.iter()
                        .filter(|x| x.item == item && v["q"].as_str().is_none_or(|q| q == x.q))
                        .collect()
                })
                .unwrap_or_default();
            let x = match 候选.as_slice() {
                [x] => *x,
                [] => return Err(at(format!("账本里键 {k} 没有材料 {item} 的读数"))),
                _ => {
                    // 依据：B107（多题下同一材料多个读数，标签行须带 q）
                    return Err(at(format!(
                        "E-list-ambiguous: 键 {k} 下材料 {item} 有 {} 个读数（{} 道题），标签行须带 q（待标清单的每行都带）",
                        候选.len(),
                        候选.len()
                    )));
                }
            };
            match v.get("p").and_then(Json::as_f64) {
                Some(q) if q != x.p => {
                    return Err(at(format!(
                        "读数 {q} 与账本 {} 不同（B88：读数只从账本回接）",
                        x.p
                    )));
                }
                _ => v["p"] = json!(x.p),
            }
            v["q"] = json!(x.q);
            if let Some(k) = x.pick {
                v["pick"] = json!(k);
            }
            键们.insert(k);
        }
        // B120 (a)：题类取序 行上 > 报告 > 基础类（回填与直接导入都可从报告取）
        if v.get("kind").is_none()
            && let Some(kind) = 题表.as_ref().and_then(|t| 报告题类(t, &v))
        {
            v["kind"] = kind;
        }
        rows.push(serde_json::from_value(v).map_err(|e| at(e.to_string()))?);
    }
    let mut store = 库(a)?;
    let batch = labels
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    // 回填导入缺省序贯、两端先标（B88）；显式给了 --certify 时按显式的
    let certify = if 框.is_some() && !a.certify_explicit {
        CertifyMethod::Sequential
    } else {
        a.certify
    };
    // 代价线（B129，步 20a-2a）不用到达顺序，回填时也不按两端框（其余方式照旧）
    let two_ends = match a.order.as_deref() {
        Some(o) => o == "two-ends",
        None => 框.is_some() && !matches!(certify, CertifyMethod::Cost(..)),
    };
    let frame = match (&框, two_ends) {
        (Some(f), true) => {
            if 键们.len() != 1 {
                return Err(format!(
                    "两端先标的回填一次只收一个键，收到 {} 个",
                    键们.len()
                ));
            }
            // 两端先标的框以样本身份 (item, q) 为标识，与导入里的样本身份同算法（B107）
            f.get(键们.iter().next().unwrap()).map(|xs| {
                xs.iter()
                    .map(|x| (jpp::truth::样本身份(&x.item, Some(&x.q)), x.p))
                    .collect()
            })
        }
        _ => None,
    };
    let sequential = Some(SeqImport {
        batch: a.batch,
        weights: a.weights,
        coverage_target: a.coverage_target,
        two_ends,
        frame,
    });
    let opt = ImportOptions {
        alpha: a.alpha,
        conf_delta: a.conf_delta,
        spot_check_min: a.spot_check_min,
        spot_check_conf: a.spot_check_conf,
        abstain_warn: a.abstain_warn,
        batch,
        seed: a.seed,
        extent_min_disagree: a.extent_min_disagree,
        extent_same_dir: a.extent_same_dir,
        extent_same_tier: a.extent_same_tier,
        scope_quantiles: a.scope_quantiles,
        scope_margins: ScopeMargins {
            k: a.scope_margins.0,
            m: a.scope_margins.1,
        },
        class_min_sources: a.class_min_sources,
        alpha_trial: Some(a.alpha_trial),
        certify,
        step: a.step,
        sequential,
    };
    let reports = import_labels(&mut store, &rows, &opt)?;
    for r in &reports {
        for w in &r.warnings {
            eprintln!("{w}");
        }
    }
    store
        .save(calib_out)
        .map_err(|e| format!("{}: {e}", calib_out.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&reports).map_err(|e| e.to_string())?
    );
    eprintln!(
        "导入 {} 行，{} 个键，写回 {}",
        rows.len(),
        reports.len(),
        calib_out.display()
    );
    Ok(())
}
