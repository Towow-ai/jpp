//! 真值通道（B19）：把一份标注文件折进校准记录，经现有 `commission` 认证上岗。
//!
//! **它不写线。** 线仍由 `commission` 的保形认证定；这里只做三件事：
//! 按键归组、决定每条材料用哪一份真值（人工或构造的优先于模型的）、
//! 以及算出「模型标注能不能单独撑起上岗」这道门。
//!
//! 门槛（抽检一致率下限、弃权率提示线）是导入参数，**不写死在规则里**。
//!
//! 复核批次（B36）：带 `spot_check` 批次号的行是复核行，**不论来源**（`human` 或
//! `model:<名>`）。复核行与同一条材料的标注行同源、或复核者即材料生成者时拒收。
//! 同一条材料的真值取序：`computed` > `human` > 复核行 > 标注行（分层取序，不是表决）。
//! 外延未定（B36 第 5(c) 条）：复核分歧够多且集中同向、或集中在同一填法档时，
//! 判题面外延未定，停在待真值、不追加复核（先改题面，B13/B23）。阈值是导入参数。

/// 导入参数里的边距类型（B68 修订），供命令行构造 [`ImportOptions`]。
pub use jpp_value::stat::ScopeMargins;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::calib::{
    CalibScope, CalibStore, CertGrade, LabelSource, LiteralMode, Refusal, Sample, Selection,
    SpotCheck, TruthSummary,
};
use jpp_value::value::{Form, Op};

mod cost;
mod review;
use cost::代价前置;
use review::{复核行回接, 序贯到达, 有效阿尔法};

/// 标注文件里的一行（JSONL）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelRow {
    /// 直接给校准键（与 `test(题面, calib)` 的 calib 同一个字符串）
    #[serde(default)]
    pub key: Option<String>,
    /// 或者给题式：键取题式键 `CalibStore::form_key(题式哈希)`
    #[serde(default)]
    pub form: Option<FormSpec>,
    /// 这一行对应哪条材料。**同一条材料的人工与模型标注靠它配对**（抽检一致率）
    pub item: String,
    /// 那次判断的读数（真值通道只收真值，读数来自一次实际运行）。
    /// `select` / `measure` 行填胜出候选（档位）的概率 p_max（B63）。
    /// 复核行（带 `spot_check`）**不得**带读数（B89：复核者不得见判断器的答案），由同一材料的标注行回接；
    /// 缺省值是 NaN 哨兵，只在反序列化时出现
    #[serde(default = "读数缺省")]
    pub p: f64,
    /// `test`：`true` / `false` / `"ambiguous"`；`select` / `measure`：真值的候选索引
    /// （按 `over` 顺序）或档位索引（按 `scale` 顺序），或 `"ambiguous"`
    pub label: Json,
    /// 这一行的题型：`test`（缺省）/ `select` / `measure`。给了 `form` 时以题式的 op 为准
    #[serde(default)]
    pub op: Option<String>,
    /// `select` / `measure` 行必填：那次读数的 argmax（判断器挑中的候选或档位索引）
    #[serde(default)]
    pub pick: Option<usize>,
    /// 这条材料的文本（被判断对象，即 `on` 槽）。一键的进线行全部带文本时，记录写认证范围的
    /// 材料指纹（B68）；部分带、部分不带时不写指纹并告警
    #[serde(default)]
    pub text: Option<String>,
    /// `human` / `computed` / `model:<名>`
    pub source: String,
    /// 复核批次号：带它的行是复核行（B36，不论来源），参与一致率计算
    #[serde(default)]
    pub spot_check: Option<String>,
    /// 这条材料的生成者（`model:<名>` 等）。复核者与它相同的复核行拒收（B36 第 3 条）
    #[serde(default)]
    pub generator: Option<String>,
    #[serde(default)]
    pub cluster: Option<String>,
    /// 校准类别标签（B34）：给了就把这一行归到类键 `CalibStore::class_key(标签)`，
    /// 行上的 `form` / `question` / `key` 改作样本来源的身份（见 [`来源身份`]）。
    /// B75：类记录须在混合样本上认证：不同来源数 ≥ `ImportOptions::class_min_sources`，
    /// 每来源进线条数 ≥ 该档 `n_needed`，分半按来源分层
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// 无题式的手写题的题面（类行用，B75 来源身份）：行上没有 `form` 时，来源取它的哈希
    /// （`q_text_hash` 口径）。行上有 `form` 时来源一律取题式键，本字段不参与：
    /// 同一题式的不同填法是同一来源（B75，否则类记录就是单一题式记录换名，即 B34 禁止的别名借用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// 这条材料所属的填法档（例如「具体物 / 类别 / 抽象主题」）。缺省时 B36 5(c)
    /// 的第二判据（分歧集中在同一填法档）不适用
    #[serde(default)]
    pub fill_tier: Option<String>,
    /// 判断器的出口或读数（任何行都不该带；复核行带了即拒收 `E-review-leak`，B89）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Json>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<Json>,
    /// 这一行对应的题（题哈希，B107，步 20h-2）：多题 `sieve` 里同一材料有多道题，材料标识 `item` 不唯一，
    /// 样本身份是 `(item, q)`。`--list-out` 的清单行带它；缺省为空（单题键照旧只凭 `item`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// 这道题的填法（B107：来自运行报告的 `questions` 表，给标注者看题面；只作展示，不参与认证）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Json>,
    /// 这道题的题类（B120 (a)：运行时算出的精化类，来自报告的 `questions` 表或作者搬运）；同键矛盾报 `E-kind-conflict`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<jpp_value::value::QuestionKind>,
    /// 或者给判断时 `on` 槽的形状（`one` / `pair`），与 `over_kind` 一起由 `question_kind` 算出题类（B120 (a)）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_kind: Option<jpp_value::value::OverKind>,
}

/// 样本身份（B107，步 20h-2）：带 `q` 的行以 `item` + `q` 为身份，同一材料上的不同题是不同样本。
/// 复核行按同一身份回接标注行，序贯的两端先标框也按同一身份（`calib-import` 用 [`样本身份`] 造框）。
pub fn 样本身份(item: &str, q: Option<&str>) -> String {
    match q {
        Some(q) => format!("{item}\u{1f}{q}"),
        None => item.to_string(),
    }
}

fn 读数缺省() -> f64 {
    f64::NAN
}

/// 题式的规格：与 `.jpp` 里 `form(op, 模板, {…})` 的参数一一对应，**哈希同算法**，
/// 所以导入时声明的题式与程序里写的题式落在同一个键上。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FormSpec {
    pub op: String,
    pub template: String,
    #[serde(default)]
    pub scale: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub presupposition: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
}

impl FormSpec {
    pub fn key(&self) -> Result<String, String> {
        let op = match self.op.as_str() {
            "test" => Op::Test,
            "select" => Op::Select,
            "measure" => Op::Measure,
            o => return Err(format!("题式 op 只能是 test/select/measure，收到 {o:?}")),
        };
        let f = Form::new(
            op,
            &self.template,
            "",
            self.scale.clone(),
            self.evidence.clone(),
            self.presupposition.clone(),
            self.request.clone(),
        )?;
        Ok(CalibStore::form_key(&f.hash))
    }
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub alpha: f64,
    pub conf_delta: f64,
    /// 模型标注单独上岗所需的人工抽检一致率下限（B19 写 0.9）。
    /// **按一致率的单侧置信下界判，不按点估计**（B19 修正：与 B24 同一纪律）。
    pub spot_check_min: f64,
    /// 上面那个下界的置信水平（B19 修正写 0.95）
    pub spot_check_conf: f64,
    /// 弃权率超过它就提示题面外延可能未定（B13）
    pub abstain_warn: f64,
    /// 批次名，进 `label_set_id` 与 `truth.batch`
    pub batch: String,
    /// 拆分样本认证（B24）的分半种子；写进证书
    pub seed: u64,
    /// B36 5(c)：复核分歧至少这么多条才判外延（默认 3）
    pub extent_min_disagree: usize,
    /// B36 5(c)：分歧同向占比达到它 → 外延未定（默认 0.8）
    pub extent_same_dir: f64,
    /// B36 5(c)：带填法档的分歧落在同一档的占比达到它 → 外延未定（默认 2/3）
    pub extent_same_tier: f64,
    /// B68：认证范围取指纹的分位区间（默认 [0.01, 0.99]）
    pub scope_quantiles: (f64, f64),
    /// B68 修订：分位区间外加的边距（默认 k = 2、m = 0.10）
    pub scope_margins: jpp_value::stat::ScopeMargins,
    /// B75：类记录至少要来自这么多个不同来源（题式或手写题）才算「混合样本」（命令行缺省 2）
    pub class_min_sources: usize,
    /// B72：试用 α（命令行缺省 0.25）。正式 α 认证不过（认证不过或样本不足）时按它再认证一次，
    /// 证书记 `Trial`；`None` 或不大于 `alpha` 时不试。已有正式线的键不试（试用线不覆盖正式线）
    pub alpha_trial: Option<f64>,
    /// 认证方式（B86、B85）：命令行缺省 `FixedSequence`；`Split` 为分层交替分半的拆分认证
    pub certify: CertifyMethod,
    /// 固定序的步长 s（`--step`）；`None` = 池的 5%（[`crate::calib::fixed_sequence_step`]）。证书写解析后的整数
    pub step: Option<usize>,
    /// 序贯导入的参数（`certify = Sequential` 时必须给）
    pub sequential: Option<SeqImport>,
}

/// `calib-import --certify` 的取值（B86、B85；序贯 `sequential` 落步 20h，B87），与 `--cost` 给出的代价线（B129，步 20a-2a）。
///
/// 代价线与另三种并列而不是 `ImportOptions` 上的一个字段：它就是一种认证方式（线由代价矩阵在标注集上定，
/// 证书只回答够不够 α），与固定序、拆分、序贯互斥，由类型给出。带 `f64`，所以不派生 `Eq`。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CertifyMethod {
    /// 固定序检验、不拆分（B86，缺省）
    FixedSequence,
    /// 拆分认证，分层交替分半（B85，退路与离线对照臂）
    Split,
    /// 序贯 e 过程（B87）：参数在 [`ImportOptions::sequential`]
    Sequential,
    /// 代价线 `(fp, fn)`（B129）：`commission_costed_graded` 按 `fp·#误放行 + fn·#漏放行` 最小定线，证书按 α 判上岗、
    /// 不按 δ 平移（`selection: None`）；只收 `test` 行；线下没有下侧证书（`lo = 0`）
    Cost(f64, f64),
}

/// 序贯导入的参数（B87、B88）。缺省值由命令行给（`--batch 10`、`--mix-weights 0.8,0.1,0.05,0.05`），
/// 不写在内核里。
#[derive(Clone, Debug)]
pub struct SeqImport {
    pub batch: usize,
    pub weights: [f64; 4],
    /// 覆盖目标 τ；`None` = settled
    pub coverage_target: Option<f64>,
    /// `true` = 两端先标（B88 待标清单的顺序）；`false` = 随机
    pub two_ends: bool,
    /// 抽样框（B88，来自账本）：`(材料标识, 读数)`，账本里首次出现的顺序。两端先标时必须给
    pub frame: Option<Vec<(String, f64)>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyReport {
    pub key: String,
    pub items: usize,
    pub truth: TruthSummary,
    pub status: String,
    pub hi: f64,
    pub lo: f64,
    /// 认证方式与各侧数字（B24）；未上岗时为空
    pub certification: Option<CertReport>,
    /// B68 修订：认证集自身被判范围外的条数（写了指纹时才有；应为 0）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_self_outside: Option<usize>,
    /// B72：试用档的结论（试了才有）：试用上岗、试用亦未过、或已有正式线不试
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial: Option<String>,
    pub warnings: Vec<String>,
}

/// 给人看的认证摘要：怎么选的线、在多少条上认证、两侧各自的错误与上界。
#[derive(Clone, Debug, Serialize)]
pub struct CertReport {
    pub selection: Option<Selection>,
    /// 认证等级（B72）：`formal` / `trial`
    pub grade: crate::calib::CertGrade,
    pub alpha: f64,
    pub conf_delta: f64,
    pub upper: (usize, usize, f64),
    pub lower: Option<(usize, usize, f64)>,
    /// 代价线的代价矩阵 `(fp, fn)`（B129，步 20a-2a）；不是代价线时为空、不序列化（旧报告逐字节不变）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<(f64, f64)>,
}

/// B36 5(c)：分歧够多且集中同向，或（有填法档时）集中在同一填法档 → 外延未定。
/// 返回写进门控的理由；不判时为 `None`。
fn extent_undetermined(
    dis: &[(Option<bool>, Option<String>)],
    opt: &ImportOptions,
) -> Option<String> {
    let n = dis.len();
    if n < opt.extent_min_disagree || n == 0 {
        return None;
    }
    // 同向判据只对有方向的分歧（test）；K 元划分只看填法档判据
    let dirs: Vec<bool> = dis.iter().filter_map(|(d, _)| *d).collect();
    if dirs.len() == n {
        let yes = dirs.iter().filter(|d| **d).count();
        let same = yes.max(n - yes);
        if same as f64 / n as f64 >= opt.extent_same_dir {
            return Some(format!("分歧同向 {same}/{n}，题面外延未定"));
        }
    }
    let tiered: Vec<&str> = dis.iter().filter_map(|(_, t)| t.as_deref()).collect();
    if !tiered.is_empty() {
        let mut count: BTreeMap<&str, usize> = BTreeMap::new();
        for t in &tiered {
            *count.entry(t).or_default() += 1;
        }
        if let Some((tier, c)) = count.into_iter().max_by_key(|(_, c)| *c) {
            if c as f64 / n as f64 >= opt.extent_same_tier {
                return Some(format!("分歧集中在填法档「{tier}」{c}/{n}，题面外延未定"));
            }
        }
    }
    None
}

/// 这一行的题型：题式的 op 优先，其次行上的 `op`，缺省 `test`。
fn row_op(r: &LabelRow) -> Result<&'static str, String> {
    let o = r
        .form
        .as_ref()
        .map(|f| f.op.as_str())
        .or(r.op.as_deref())
        .unwrap_or("test");
    match o {
        "test" => Ok("test"),
        "select" => Ok("select"),
        "measure" => Ok("measure"),
        other => Err(format!("op 只能是 test / select / measure，收到 {other:?}")),
    }
}

/// 一组标注行的共同基础题类（B76）：每行按 `question_kind(op, request, 未知槽形, 无声明)` 算，
/// 全部相同时返回它；空或不一致时 `None`。标注行不带状态，所以只有基础类；题式的 `over_kind`
/// 声明不在 `FormSpec` 里，按缺省（select → 归类）。
fn 共同题类<'a>(
    rows: impl Iterator<Item = &'a LabelRow>,
) -> Option<jpp_value::value::QuestionKind> {
    use jpp_value::value::{Request, SlotDecls, SlotShape, question_kind};
    let mut out = None;
    for r in rows {
        let op = match row_op(r).ok()? {
            "select" => Op::Select,
            "measure" => Op::Measure,
            _ => Op::Test,
        };
        let request = r
            .form
            .as_ref()
            .and_then(|f| f.request.as_deref())
            .and_then(Request::parse);
        let (k, _) = question_kind(op, request, &SlotShape::UNKNOWN, &SlotDecls::default());
        match out {
            None => out = Some(k),
            Some(prev) if prev != k => return None,
            _ => {}
        }
    }
    out
}

/// 记录的题类（B120 (a)，步 20h-2）：取序 行上 `kind`（或 `slot_shape` + `over_kind` 算出）> 基础类。
/// 行上给出的题类同键不一致、或声明与槽形矛盾，报 `E-kind-conflict`（整批拒收）。
fn 记录题类(
    key: &str,
    rows: &[&LabelRow],
) -> Result<Option<jpp_value::value::QuestionKind>, String> {
    use jpp_ir::question_kind::kind_conflict;
    use jpp_value::value::{OnShape, OverShape, Request, SlotDecls, SlotShape, question_kind};
    let mut 显式: Vec<jpp_value::value::QuestionKind> = vec![];
    for r in rows {
        let op = match row_op(r)? {
            "select" => Op::Select,
            "measure" => Op::Measure,
            _ => Op::Test,
        };
        let k = match (&r.kind, &r.slot_shape) {
            (Some(k), _) => Some(*k),
            (None, Some(s)) => {
                let on = match s.as_str() {
                    "one" => OnShape::One,
                    "pair" => OnShape::Pair,
                    o => {
                        // 依据：B120 (a)（槽形只收 one / pair）
                        return Err(format!(
                            "E-kind-conflict: 键 {key} 材料 {} 的 slot_shape 只能是 one / pair，收到 {o:?}",
                            r.item
                        ));
                    }
                };
                let shape = SlotShape {
                    on,
                    over: OverShape::Unknown,
                    question_material: false,
                };
                let decl = SlotDecls {
                    over_kind: r.over_kind,
                    accepts: None,
                };
                if let Some(c) = kind_conflict(op, &shape, &decl) {
                    // 依据：B76、B120 (a)（声明与结构矛盾即拒）
                    return Err(format!(
                        "E-kind-conflict: 键 {key} 材料 {}：{}",
                        r.item, c.reason
                    ));
                }
                let request = r
                    .form
                    .as_ref()
                    .and_then(|f| f.request.as_deref())
                    .and_then(Request::parse);
                Some(question_kind(op, request, &shape, &decl).0)
            }
            (None, None) => None,
        };
        if let Some(k) = k {
            if 显式.first().is_some_and(|p| *p != k) {
                // 依据：B120 (a)（同键各行的题类矛盾）
                return Err(format!(
                    "E-kind-conflict: 键 {key} 的标注行题类不一致（{} 与 {}）：同一条记录只能是一个题类",
                    显式[0].label(),
                    k.label()
                ));
            }
            显式.push(k);
        }
    }
    Ok(match 显式.first() {
        Some(k) => Some(*k),
        None => 共同题类(rows.iter().copied()),
    })
}

/// 有确定真值的标签：test 的布尔，或 K 元划分的索引（「模棱两可」不算）。
fn decided(l: &Json) -> bool {
    l.is_boolean() || l.as_u64().is_some()
}

/// 这条标注进线时的真值位：test 是标签本身；K 元划分是「argmax 等于真值索引」（B63）。
fn correct(r: &LabelRow, kary: bool) -> bool {
    if kary {
        r.pick.map(|k| k as u64) == r.label.as_u64()
    } else {
        r.label == Json::Bool(true)
    }
}

fn is_human(src: &str) -> bool {
    src == "human" || src == "computed"
}

/// 类行的来源身份（B75）：有题式取题式键；无题式有手写题面取题面哈希；都没有取 `key`。
/// 同一题式的不同填法是同一来源。
fn 来源身份(r: &LabelRow) -> Result<Option<String>, String> {
    if let Some(f) = &r.form {
        return f.key().map(Some);
    }
    if let Some(q) = &r.question {
        return Ok(Some(format!("q:{}", jpp_value::value::hash_of(&[q]))));
    }
    Ok(r.key.clone())
}

/// 把标注行折进 `store`。返回每个键的结论；**键内出错不中断其他键**，错误写进该键的 `gate`。
pub fn import_labels(
    store: &mut CalibStore,
    rows: &[LabelRow],
    opt: &ImportOptions,
) -> Result<Vec<KeyReport>, String> {
    // B107（步 20h-2）：带 `q` 的行以 (item, q) 为样本身份；先改身份再回接复核行
    let 身份化: Vec<LabelRow> = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.item = 样本身份(&r.item, r.q.as_deref());
            r
        })
        .collect();
    let 回接 = 复核行回接(&身份化)?;
    let rows: &[LabelRow] = &回接;
    // 1. 按键归组
    let mut by_key: BTreeMap<String, Vec<&LabelRow>> = BTreeMap::new();
    let mut key_ops: BTreeMap<String, &'static str> = BTreeMap::new();
    // 类键 → 来源身份集合（B75 混合样本）；类行 → 来源身份（分层分半用）
    let mut class_sources: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut row_source: BTreeMap<usize, String> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        let src_q = match (&r.key, &r.form) {
            (Some(k), None) => Some(k.clone()),
            (None, Some(f)) => Some(f.key().map_err(|e| format!("第 {} 行：{e}", i + 1))?),
            (None, None) => None,
            _ => return Err(format!("第 {} 行：key 与 form 最多给一个", i + 1)),
        };
        let key = match (&r.class, src_q) {
            (Some(label), _) => {
                let q = 来源身份(r)
                    .map_err(|e| format!("第 {} 行：{e}", i + 1))?
                    .ok_or_else(|| {
                        format!(
                            "第 {} 行：类行（class）要给来源题的身份：form、question 或 key",
                            i + 1
                        )
                    })?;
                let ck = CalibStore::class_key(label);
                class_sources
                    .entry(ck.clone())
                    .or_default()
                    .insert(q.clone());
                row_source.insert(r as *const LabelRow as usize, q);
                ck
            }
            (None, Some(k)) => k,
            (None, None) => return Err(format!("第 {} 行：key 与 form 必须恰好给一个", i + 1)),
        };
        if !(0.0..=1.0).contains(&r.p) {
            return Err(format!("第 {} 行：p 必须在 0..=1，收到 {}", i + 1, r.p));
        }
        if !(is_human(&r.source) || r.source.starts_with("model:")) {
            return Err(format!(
                "第 {} 行：source 只能是 human / computed / model:<名>，收到 {:?}",
                i + 1,
                r.source
            ));
        }
        let op = row_op(r).map_err(|e| format!("第 {} 行：{e}", i + 1))?;
        match (&r.label, op) {
            (Json::String(s), _) if s == "ambiguous" => {}
            (Json::Bool(_), "test") => {}
            (Json::Number(n), "select" | "measure") if n.as_u64().is_some() => {}
            (other, "test") => {
                return Err(format!(
                    "第 {} 行：test 行的 label 只能是 true / false / \"ambiguous\"，收到 {other}",
                    i + 1
                ));
            }
            (other, _) => {
                return Err(format!(
                    "第 {} 行：{op} 行的 label 只能是候选或档位索引（非负整数）或 \"ambiguous\"，收到 {other}",
                    i + 1
                ));
            }
        }
        // 依据：B63（K 元划分的样本 = (p_max, argmax 是否等于真值)）
        match (op, r.pick) {
            ("test", Some(_)) => return Err(format!("第 {} 行：test 行不收 pick", i + 1)),
            ("select" | "measure", None) => {
                return Err(format!(
                    "第 {} 行：{op} 行必须给 pick（那次读数的 argmax）",
                    i + 1
                ));
            }
            _ => {}
        }
        if let Some(prev) = key_ops.insert(key.clone(), op).filter(|p| *p != op) {
            return Err(format!(
                "第 {} 行：键 {key} 混了 {prev} 与 {op} 两种题型：不同尺不可比，一条记录只能认一个题型",
                i + 1
            ));
        }
        by_key.entry(key).or_default().push(r);
    }
    // 代价线（B129，步 20a-2a）的两道前置，在写任何记录之前（`truth/cost.rs`）
    if let CertifyMethod::Cost(..) = opt.certify {
        代价前置(rows, by_key.keys(), store)?;
    }

    let mut out = vec![];
    for (key, rs) in by_key {
        let op = key_ops[&key];
        let kary = op != "test";
        let 题类 = 记录题类(&key, &rs)?;
        let mut warnings = vec![];
        // 2. 每条材料定一份真值：人工或构造的优先；没有就用模型的
        let mut per_item: BTreeMap<&str, Vec<&LabelRow>> = BTreeMap::new();
        for r in &rs {
            per_item.entry(r.item.as_str()).or_default().push(r);
        }
        let mut sources: BTreeMap<String, u64> = BTreeMap::new();
        let ambiguous = rs
            .iter()
            .filter(|r| r.label == Json::String("ambiguous".into()))
            .count() as u64;
        let mut chosen: Vec<(&LabelRow, bool)> = vec![];
        let mut model_only_items = 0usize;
        let (mut sc_n, mut sc_agree) = (0u64, 0u64);
        let mut sc_batches: BTreeSet<String> = BTreeSet::new();
        let mut reviewers: BTreeSet<String> = BTreeSet::new();
        let mut rejected = 0usize;
        // B36 5(c)：复核分歧（标注行说什么、复核行说什么、填法档）
        // 方向只对 test 有意义（K 元划分的分歧没有「是 / 否」两个方向），记 None
        let mut disagreements: Vec<(Option<bool>, Option<String>)> = vec![];
        // B89：本批复核对（标注行读数，是否一致），a_lb(A) 只用读数落在已决区 A 的那些
        let mut 复核对: Vec<(f64, bool)> = vec![];
        for (item, group) in &per_item {
            let boolean = |r: &&&LabelRow| decided(&r.label);
            // 标注行：不带批次号的模型行
            let annot: Vec<&&LabelRow> = group
                .iter()
                .filter(|r| r.spot_check.is_none() && r.source.starts_with("model:"))
                .filter(boolean)
                .collect();
            // 复核行：带批次号的行，不论来源；同源（与本条任一标注行来源相同，或即生成者）拒收
            let mut review: Vec<&&LabelRow> = vec![];
            for r in group.iter().filter(|r| r.spot_check.is_some()) {
                let same_as_annot = group.iter().any(|a| {
                    a.spot_check.is_none() && a.source == r.source && !is_human(&a.source)
                });
                let same_as_gen = r.generator.as_deref() == Some(r.source.as_str());
                if same_as_annot || same_as_gen {
                    rejected += 1;
                    warnings.push(format!(
                        "W-review-same-source: 键 {key} 材料 {item} 的复核行来源 {} 与{}相同，拒收（B36 同源不计）",
                        r.source, if same_as_annot { "标注行" } else { "材料生成者" }
                    ));
                    continue;
                }
                if decided(&r.label) {
                    review.push(r);
                }
            }
            // 一致率：复核行对标注行（标注行取第一条）
            for h in &review {
                if let Some(m) = annot.first() {
                    sc_n += 1;
                    复核对.push((m.p, h.label == m.label));
                    if h.label == m.label {
                        sc_agree += 1;
                    } else {
                        // 方向：复核行判「是」而标注行判「否」记 true
                        let tier = h.fill_tier.clone().or_else(|| m.fill_tier.clone());
                        disagreements.push(((!kary).then(|| h.label == Json::Bool(true)), tier));
                    }
                    sc_batches.insert(h.spot_check.clone().unwrap_or_default());
                    if !is_human(&h.source) {
                        reviewers.insert(h.source.clone());
                    }
                }
            }
            // 取序：computed > human > 复核行 > 标注行
            let computed = group
                .iter()
                .filter(|r| r.source == "computed")
                .find(|r| decided(&r.label));
            let human = group
                .iter()
                .filter(|r| r.source == "human")
                .find(|r| decided(&r.label));
            let pick = computed
                .or(human)
                .or(review.iter().copied().find(|r| !is_human(&r.source)))
                .or(annot.first().copied());
            if let Some(r) = pick {
                if !is_human(&r.source) {
                    model_only_items += 1;
                }
                *sources.entry(r.source.clone()).or_default() += 1;
                chosen.push((r, correct(r, kary)));
            }
        }
        let _ = rejected;
        // **门要看整条记录，不只看这一批。** `--calib` 装进来的旧记录里可能已经躺着
        // 被上一次导入挡在「待核」的模型标注（样本在门之前就折进去了）；这一批只有人工行时
        // `model_only_items == 0`，门会被绕过，而认证用的是记录里的**全部**样本。
        // 所以把旧的真值账（来源计数、抽检）并进来一起判，并写回合并后的账，
        // 下一次导入也看得见。没有真值账的旧记录（手写 JSON）查不出来源，不在此列。
        if let Some(prev) = store.records.get(&key).and_then(|r| r.truth.clone()) {
            for (src, n) in &prev.sources {
                if !is_human(src) {
                    model_only_items += *n as usize;
                }
                *sources.entry(src.clone()).or_default() += n;
            }
            if let Some(ps) = &prev.spot_check {
                sc_n += ps.n;
                sc_agree += ps.agree;
                sc_batches.extend(ps.batches.iter().cloned());
                reviewers.extend(ps.model_reviewers.iter().cloned());
            }
        }
        let abstain_rate = if rs.is_empty() {
            0.0
        } else {
            ambiguous as f64 / rs.len() as f64
        };
        if abstain_rate > opt.abstain_warn {
            warnings.push(format!(
                "W-abstain: 键 {key} 标注者弃权率 {:.2} > {:.2}：题面外延可能未定（B13），先改题面再标注",
                abstain_rate, opt.abstain_warn
            ));
        }
        let spot = if sc_n > 0 {
            let lower = agree_lower(sc_agree, sc_n, opt.spot_check_conf);
            Some(SpotCheck {
                batches: sc_batches.into_iter().collect(),
                n: sc_n,
                agree: sc_agree,
                rate: sc_agree as f64 / sc_n as f64,
                lower: Some(lower),
                conf: Some(opt.spot_check_conf),
                model_reviewers: reviewers.iter().cloned().collect(),
            })
        } else {
            None
        };
        // 3. 上岗门：有模型单独撑起的真值时，要有抽检且一致率过门槛
        // 点估计不过门槛 → 待核；点估计过、下界不过 → **临时上岗**：线照常认证，
        // 但 `gate` 写明下界与转正所需的追加条数，`cut` 用到它时报 `W-provisional`。
        // 门槛文本写复核者（B36 第 2 条）：只有人工复核时沿用原文（人工批次的记录逐字节不变）
        let (抽检名, 上岗名) = if reviewers.is_empty() {
            ("人工抽检".to_string(), "上岗".to_string())
        } else {
            let who = reviewers.iter().cloned().collect::<Vec<_>>().join("、");
            (
                format!("复核（复核者 {who}）"),
                format!("上岗（复核者 {who}）"),
            )
        };
        let mut provisional: Option<String> = None;
        // 依据：B36 第 5(c) 条（分歧集中同向或同一填法档 → 题面外延未定，不追加复核）
        let extent = extent_undetermined(&disagreements, opt);
        if let Some(why) = &extent {
            warnings.push(format!(
                "W-extent: 键 {key} {why}：先改题面（B13/B23），不追加复核"
            ));
        }
        // B75：类记录每来源的进线条数（分层分半与每来源门槛用）
        let 来源计数: BTreeMap<String, u64> = if class_sources.contains_key(&key) {
            let mut m = BTreeMap::new();
            for (r, _) in &chosen {
                if let Some(src) = row_source.get(&(*r as *const LabelRow as usize)) {
                    *m.entry(src.clone()).or_default() += 1;
                }
            }
            m
        } else {
            BTreeMap::new()
        };
        // 依据：B34（类键只命中在混合样本上认证过的类记录）、B75（混合 = 不同来源数 ≥ 导入参数；
        // 来源 = 题式，填法不算不同来源）。只数这一批，不并旧记录（旧记录没有来源账，取拒绝侧）。
        // 只数有确定真值进线的来源（PR #31 P2）：全是模棱两可的来源不进线，不能凑来源数
        let 类来源不足 = class_sources
            .contains_key(&key)
            .then_some(来源计数.len())
            .filter(|n| *n < opt.class_min_sources);
        let gate_block: Option<String> = if let Some(n) = 类来源不足 {
            Some(format!(
                "待核：类记录来源不足（{n} 个来源，需 ≥ {}）",
                opt.class_min_sources
            ))
        } else if let Some(why) = extent {
            Some(format!("待核：{why}"))
        } else if model_only_items == 0 {
            None
        } else {
            match &spot {
                None => Some(format!(
                    "待核：{model_only_items} 条真值只有模型标注，同键没有人工抽检"
                )),
                Some(s) if s.rate < opt.spot_check_min => Some(format!(
                    "待核：{抽检名}一致率 {:.2}（{}/{}）< 门槛 {:.2}",
                    s.rate, s.agree, s.n, opt.spot_check_min
                )),
                Some(s) => {
                    let lo = s.lower.unwrap_or(0.0);
                    if lo < opt.spot_check_min {
                        let more =
                            extra_needed(s.agree, s.n, opt.spot_check_min, opt.spot_check_conf);
                        provisional = Some(format!(
                            "临时上岗：{}一致率 {}/{}，单侧 {:.0}% 置信下界 {:.3} < 门槛 {:.2}；{}",
                            抽检名,
                            s.agree,
                            s.n,
                            opt.spot_check_conf * 100.0,
                            lo,
                            opt.spot_check_min,
                            match more {
                                Some(m) => format!("再追加 {m} 条全一致即转正"),
                                None => "追加 200 条内全一致也到不了门槛，需重做抽检".into(),
                            }
                        ));
                    }
                    None
                }
            }
        };
        // B72：折样本之前看这条记录是否已有任何一张正式证书（试用线不覆盖它）。不看状态与夹具位：
        // 试用认证成功会把试用线的数写进记录并置「上岗」、清夹具位，而 `选中的证书`（α 最小）
        // 仍是那张正式证书，出口就会带着试用线的数被当成正式、放行不可逆 do。停岗候选（B25 漂移后
        // 重新标注）与带证书的夹具记录都走得到这条路，所以判据是「有正式证书」本身。
        let 已有正式线 = store
            .records
            .get(&key)
            .is_some_and(|r| r.certs.values().any(|c| c.grade.is_formal()));
        // B87：序贯要这次导入给出该键的全部标注（旧记录里另有带标注样本时，到达顺序无从定）
        let 旧标注 = store.records.get(&key).map(|r| r.labeled()).unwrap_or(0);
        // 4. 折样本
        for (r, lab) in &chosen {
            store
                .absorb(
                    &key,
                    Sample {
                        p: Some(r.p),
                        label: Some(u8::from(*lab)),
                        perms: 0,
                        mode_share: None,
                        mode: LiteralMode::default(),
                        phys: match op {
                            "select" => "choice",
                            "measure" => "score",
                            _ => "noul",
                        }
                        .into(),
                        cluster: r.cluster.clone(),
                        stratum: row_source.get(&(*r as *const LabelRow as usize)).cloned(),
                    },
                )
                .map_err(|e| format!("键 {key}：{e}"))?;
        }
        // B68；PR #31 P1：带文本的进线行的材料指纹随样本进记录，认证范围由认证用的同一批样本算
        if let Some(rec) = store.records.get_mut(&key) {
            rec.material_fps.extend(
                chosen
                    .iter()
                    .filter_map(|(r, _)| r.text.as_deref())
                    .map(jpp_value::stat::material_fingerprint),
            );
        }
        let batch = opt.batch.clone();
        let _ = store.set_label_set_id(&key, &format!("truth:{batch}"));
        // 导入的真值覆盖了这批导入的全部读数：由导入者声明「全体」
        let _ = store.set_label_source(&key, LabelSource::全体);
        if let Some(rec) = store.records.get_mut(&key).filter(|_| !来源计数.is_empty()) {
            rec.sources = 来源计数.clone();
        }
        // B76（步 20a-1）、B120 (a)（步 20h-2）：记录的题类 = 行上题类（报告搬运或作者给出）> 进线行基础类
        if let Some(rec) = store.records.get_mut(&key) {
            rec.kind = 题类;
        }
        // 步 15d-2：按 δ 平移的认证要记录带 δ。记录没有时取宿主装进来的画像的 δ 先验（CLI：`--profile`）；
        // 画像也没有就报错，不回退到任何默认值（B73「数字只住画像」）。依据：21 步 15d-2
        if store.records.get(&key).is_some_and(|r| r.delta.is_none()) {
            let 题型 = crate::calib::反查题型_pub(op).unwrap_or(jpp_value::value::Op::Test);
            match store.profile.delta_prior(题型) {
                Some(d) => {
                    let _ = store.set_delta(&key, d);
                }
                None => {
                    return Err(format!(
                        "键 {key}：认证要 δ，记录没有、画像也没有 δ 先验（delta.*）。修法：calib-import 带 --profile <画像>（δ 只从画像取，不兜底；步 15d-2）"
                    ));
                }
            }
        }
        // 一档认证：B75 每来源门槛（该档 n_needed）→ 按导入方式认证（B86 固定序缺省；B85 分层交替拆分），
        // K 元单侧线同形（B63），证书写等级（B72）。已有记录不动：只有这次导入的认证走这里
        let 序贯规格: Result<Option<crate::calib::SeqSpec>, String> =
            match (opt.certify, &opt.sequential) {
                (CertifyMethod::Sequential, None) => Err("认证不过：序贯导入缺序贯参数".into()),
                (CertifyMethod::Sequential, Some(sq)) if 旧标注 > 0 => {
                    let _ = sq;
                    Err(format!(
                        "待核：序贯导入要一次给出该键全部已标行（记录里已有 {旧标注} 条旧标注）"
                    ))
                }
                (CertifyMethod::Sequential, Some(sq)) => {
                    序贯到达(store, &key, &chosen, sq, opt).map(Some)
                }
                _ => Ok(None),
            };
        let 认证于 =
            |store: &mut CalibStore, alpha: f64, grade: CertGrade| -> Result<String, Refusal> {
                let need = jpp_value::stat::n_needed_zero_error(alpha, opt.conf_delta) as u64;
                if let Some((src, n)) = 来源计数.iter().find(|(_, n)| **n < need) {
                    return Err(Refusal::跑不成(format!(
                        "待核：类记录来源 {src} 不足 {need} 条（有 {n} 条）"
                    )));
                }
                let (c, s, seed) = (opt.conf_delta, opt.step, opt.seed);
                match (opt.certify, kary) {
                    // 依据：B129（代价线：代价定线、证书按 α 判上岗；等级按 B72 先正式后试用）。K 元行已在归组后拒收
                    (CertifyMethod::Cost(fp, fn_), _) => {
                        store.commission_costed_graded(&key, alpha, c, "条", (fp, fn_), grade)
                    }
                    (CertifyMethod::FixedSequence, true) => {
                        store.commission_upper_fixed_sequence_graded(&key, alpha, c, s, grade)
                    }
                    (CertifyMethod::FixedSequence, false) => {
                        store.commission_two_sided_fixed_sequence_graded(&key, alpha, c, s, grade)
                    }
                    (CertifyMethod::Split, true) => {
                        store.commission_upper_split_stratified_graded(&key, alpha, c, seed, grade)
                    }
                    (CertifyMethod::Split, false) => store
                        .commission_two_sided_split_stratified_graded(&key, alpha, c, seed, grade),
                    (CertifyMethod::Sequential, _) => match &序贯规格 {
                        Err(why) => Err(Refusal::跑不成(why.clone())),
                        Ok(None) => Err(Refusal::跑不成("认证不过：序贯规格缺失".into())),
                        Ok(Some(spec)) if kary => {
                            store.commission_upper_sequential_graded(&key, alpha, c, s, grade, spec)
                        }
                        Ok(Some(spec)) => store
                            .commission_two_sided_sequential_graded(&key, alpha, c, s, grade, spec),
                    },
                }
                .map(|c| c.addr())
            };
        let 原因 = |e: &Refusal| match e {
            Refusal::认证不过(c) => format!("认证不过：{c:?}"),
            Refusal::跑不成(w) if w.starts_with("待核") => w.clone(),
            Refusal::跑不成(w) => format!("认证不过：{w}"),
        };
        let mut trial: Option<String> = None;
        let mut 认证了 = false;
        let mut 新证书: Option<String> = None;
        let gate = match gate_block {
            Some(why) => why,
            None if chosen.is_empty() => "待核：没有带真值的条目（全部为模棱两可）".to_string(),
            None => match 认证于(store, opt.alpha, CertGrade::Formal) {
                Ok(addr) => {
                    认证了 = true;
                    新证书 = Some(addr);
                    provisional.clone().unwrap_or_else(|| 上岗名.clone())
                }
                Err(e) => {
                    let 正式原因 = 原因(&e);
                    // B72：只有「认证不过」或样本不足才试；停岗、参数错误不试
                    let 可试 = match &e {
                        Refusal::认证不过(_) => true,
                        Refusal::跑不成(w) => w.starts_with("待核"),
                    };
                    match opt.alpha_trial.filter(|at| 可试 && *at > opt.alpha) {
                        None => 正式原因,
                        Some(_) if 已有正式线 => {
                            trial = Some(format!(
                                "未试：已有正式线，试用线不覆盖（B72）；正式 α={} 未过：{正式原因}",
                                opt.alpha
                            ));
                            正式原因
                        }
                        Some(at) => match 认证于(store, at, CertGrade::Trial) {
                            Ok(addr) => {
                                认证了 = true;
                                新证书 = Some(addr);
                                let t = format!(
                                    "试用上岗（α={at}）：正式 α={} 未过：{正式原因}",
                                    opt.alpha
                                );
                                trial = Some(t.clone());
                                // 抽检下界不过时门控仍以「临时上岗」开头（W-provisional 按前缀触发）
                                match provisional.clone() {
                                    Some(p) => format!("{p}；按试用 α={at} 上岗（B72）"),
                                    None => t,
                                }
                            }
                            Err(e2) => {
                                trial = Some(format!("试用 α={at} 亦未过：{}", 原因(&e2)));
                                正式原因
                            }
                        },
                    }
                }
            },
        };
        // 依据：B89（有效 α：模型真值的记录按复核覆盖写 alpha_eff 与真值基准，超过 α 降为试用）
        let gate = match 新证书.as_ref() {
            Some(addr) => {
                let 模型真值 = sources.keys().any(|k| k.starts_with("model:"));
                match 有效阿尔法(
                    store,
                    &key,
                    addr,
                    &chosen,
                    &复核对,
                    spot.as_ref(),
                    &reviewers,
                    opt,
                )
                .filter(|_| 模型真值)
                {
                    Some((eff, 降级, 临时)) => {
                        let r = store.records.get_mut(&key).expect("刚认证过");
                        let a = r.certs.get(addr).map(|c| c.alpha).unwrap_or(opt.alpha);
                        let ae = eff.alpha_eff;
                        for c in r.certs.get_mut(addr).into_iter().chain(r.lower.as_mut()) {
                            c.eff = Some(eff.clone());
                            if 降级 {
                                c.grade = CertGrade::Trial;
                            }
                        }
                        if 降级 {
                            format!("{gate}；alpha_eff={ae:.3} > α={a}，按 B89 降为试用")
                        } else if 临时 {
                            // B89 解读 (b)（步 20c）：超过试用 α 为临时上岗，证书等级不改（运行时由 alpha_eff 派生）
                            let t = eff
                                .trial_alpha
                                .unwrap_or(jpp_value::stat::ALPHA_TRIAL_DEFAULT);
                            format!(
                                "{gate}；alpha_eff={ae:.3} > 试用 α={t}，按 B89 为临时上岗（Provisional）"
                            )
                        } else {
                            gate
                        }
                    }
                    None => gate,
                }
            }
            None => gate,
        };
        // 依据：B68（认证范围：认证集带文本时写指纹）；B104-2（缺指纹 = 范围未知，不放行）；PR #31 P1：
        // 只有这次真正认证上岗才写范围（被拦或认证不过时旧线仍在岗，旧范围不动），指纹取记录里全部
        // 带标注样本的材料指纹，即认证用的那一批；有样本没有文本（旧样本或本批部分行）时不写指纹
        // B104-2（步 20h-1）：部分带文本时用带文本的子集写指纹并记 n_text；一条文本都没有 = 范围未知
        let mut n_text: Option<usize> = None;
        let (fingerprint, scope_self_outside, 认证集条数) = if 认证了 {
            let rec = store.records.get(&key).expect("刚认证过");
            let (fps, n) = (&rec.material_fps, rec.labeled());
            if !fps.is_empty() && n > 0 {
                if fps.len() < n {
                    n_text = Some(fps.len());
                    // 依据：B104-2（认证集部分带文本：子集指纹）
                    warnings.push(format!(
                        "W-scope-partial: 键 {key} 的认证范围只由 {}/{n} 条带文本样本给出（其余样本没有文本；B104）",
                        fps.len()
                    ));
                }
                let f = jpp_value::stat::ScopeRanges::from_fingerprints(
                    fps,
                    opt.scope_quantiles,
                    Some(opt.scope_margins),
                );
                // 依据：B68 修订（认证集自判范围外必须为 0；非 0 说明边距不足，报 W-scope-self）
                let out = f
                    .as_ref()
                    .map(|f| fps.iter().filter(|x| f.outside(x).is_some()).count());
                (f, out, n)
            } else {
                (None, None, n)
            }
        } else {
            (None, None, 0)
        };
        if let Some(n) = scope_self_outside.filter(|n| *n > 0) {
            warnings.push(format!(
                "W-scope-self: 键 {key} 的认证集自身有 {n}/{认证集条数} 条落在认证范围外（B68 修订）；加大 --scope-margins 或检查认证集"
            ));
        }
        let scope = 认证了.then(|| {
            let mut batches = store
                .records
                .get(&key)
                .and_then(|r| r.scope.as_ref())
                .map(|sc| sc.batches.clone())
                .unwrap_or_default();
            if !batches.contains(&batch) {
                batches.push(batch.clone());
            }
            CalibScope {
                batches,
                sources: sources.clone(),
                note: if fingerprint.is_some() {
                    "认证集来源与材料指纹范围（B68）".into()
                } else {
                    "认证集来源；材料风格指纹与范围外告警（B24 补充）未做".into()
                },
                fingerprint,
                n_text,
                // B91：扩展验的是旧线对；重新认证换了线，旧扩展不再有凭据，清空（扩展要在新线上重做）
                extensions: vec![],
            }
        });
        let summary = TruthSummary {
            sources,
            ambiguous,
            abstain_rate,
            spot_check: spot,
            spot_check_min: opt.spot_check_min,
            gate: gate.clone(),
            batch,
        };
        if let Some(rec) = store.records.get_mut(&key) {
            rec.truth = Some(summary.clone());
            if let Some(sc) = scope {
                rec.scope = Some(sc);
            }
        }
        let certification = store.records.get(&key).and_then(|r| {
            if r.status != "上岗" {
                return None;
            }
            // 代价线（B129，步 20a-2a）：报告这次导入认证的那张（代价证书没有 selection，按下面的回退会落到
            // 按地址排第一的那张，同键再加一对代价时报错代价）；其余方式照旧
            let 本次代价证书 = 新证书
                .as_ref()
                .filter(|_| matches!(opt.certify, CertifyMethod::Cost(..)))
                .and_then(|a| r.certs.get(a));
            let up = 本次代价证书
                .or_else(|| r.选中的证书().filter(|c| c.selection.is_some()))
                .or_else(|| r.certs.values().find(|c| c.selection.is_some()))
                .or_else(|| r.certs.values().next())?;
            Some(CertReport {
                selection: up.selection.clone(),
                grade: up.grade,
                alpha: up.alpha,
                conf_delta: up.conf_delta,
                upper: (up.n_accepted, up.n_errors, up.ucb),
                lower: r.lower.as_ref().map(|c| (c.n_accepted, c.n_errors, c.ucb)),
                cost: up.cost,
            })
        });
        let rec = store.get(&key);
        out.push(KeyReport {
            key,
            items: chosen.len(),
            truth: summary,
            status: rec.status,
            hi: rec.hi,
            lo: rec.lo,
            certification,
            scope_self_outside,
            trial,
            warnings,
        });
    }
    Ok(out)
}

/// 一致率的单侧置信下界（Clopper–Pearson）：`1 − 不一致率的上界`。
/// 25/25、0.95 → 0.887（B19 修正引用的数）。
pub fn agree_lower(agree: u64, n: u64, conf: f64) -> f64 {
    jpp_value::stat::agreement_lower(agree, n, conf)
}

/// 再追加多少条**全一致**的抽检，下界才到门槛；200 条内到不了返回 `None`。
pub fn extra_needed(agree: u64, n: u64, min: f64, conf: f64) -> Option<u64> {
    (0..=200u64).find(|m| agree_lower(agree + m, n + m, conf) >= min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 步 15d-2：导入要 δ 先验（记录没有时取画像的）。测试取步 15d-2 前的代码兜底值，线与原来相同
    fn 库() -> CalibStore {
        let mut s = CalibStore::new();
        s.profile.delta =
            jpp_effects::Field::known((0.05, 0.15, 0.15), "测试：步 15d-2 前的代码兜底值");
        s
    }

    /// B19 修正：25/25 的单侧 95% 下界约 0.887（不过 0.9）；零分歧要 29 条（0.05^(1/29) ≈ 0.902），即再追加 4 条。
    #[test]
    fn spot_check_gate_uses_the_lower_bound() {
        let lo = agree_lower(25, 25, 0.95);
        assert!((lo - 0.887).abs() < 0.002, "{lo}");
        assert_eq!(extra_needed(25, 25, 0.9, 0.95), Some(4));
        assert!(agree_lower(30, 30, 0.95) >= 0.9);
    }

    fn 选项() -> ImportOptions {
        ImportOptions {
            alpha: 0.1,
            conf_delta: 0.1,
            spot_check_min: 0.9,
            spot_check_conf: 0.95,
            abstain_warn: 0.1,
            batch: "t".into(),
            seed: 1,
            extent_min_disagree: 3,
            extent_same_dir: 0.8,
            extent_same_tier: 2.0 / 3.0,
            scope_quantiles: (0.01, 0.99),
            scope_margins: Default::default(),
            class_min_sources: 2,
            alpha_trial: None,
            certify: CertifyMethod::Split,
            step: None,
            sequential: None,
        }
    }

    /// 造一批：30 条模型标注加模型复核；前 `dis` 条标注判「是」、复核判「否」（同向分歧），其余一致。
    fn 批(dis: usize) -> Vec<LabelRow> {
        let mut rows = vec![];
        for i in 0..30 {
            let item = format!("m{i}");
            let annot = i < dis || i % 2 == 0;
            let review = if i < dis { false } else { annot };
            let p = if annot { 0.9 } else { 0.1 };
            rows.push(serde_json::from_value(serde_json::json!({"key": "k", "item": item, "p": p, "label": annot, "source": "model:sonnet"})).unwrap());
            rows.push(serde_json::from_value(serde_json::json!({"key": "k", "item": item, "label": review, "source": "model:fable", "spot_check": "fable-r1"})).unwrap());
        }
        rows
    }

    /// B36 5(c) 命中：4 条分歧全部同向 → 外延未定，停在待真值，门控写理由。
    // 依据：B36 第 5(c) 条
    #[test]
    fn extent_undetermined_when_disagreements_share_a_direction() {
        let mut store = 库();
        let rep = import_labels(&mut store, &批(4), &选项()).unwrap();
        assert_eq!(rep[0].truth.gate, "待核：分歧同向 4/4，题面外延未定");
        assert_ne!(rep[0].status, "上岗");
        assert!(rep[0].warnings.iter().any(|w| w.starts_with("W-extent")));
    }

    /// B68：进线行全部带文本时记录写材料指纹；部分带时不写并告警 W-scope-partial。
    // 依据：B68 第 1 条
    #[test]
    fn scope_fingerprint_needs_text_on_every_row() {
        let mut rows = 两极批("k", 240, 0); // 可认证：P1 起范围只在认证时写
        for r in rows.iter_mut() {
            r.text = Some(format!("材料{}：今天下午的会议推迟到三点。", r.item));
        }
        let mut store = 库();
        import_labels(&mut store, &rows, &选项()).unwrap();
        let fp = store
            .get("k")
            .scope
            .and_then(|s| s.fingerprint)
            .expect("全部带文本 → 写指纹");
        assert_eq!(fp.ranges.len(), jpp_value::stat::FP_NAMES.len());

        rows[0].text = None;
        rows[1].text = None;
        let mut store = 库();
        let rep = import_labels(&mut store, &rows, &选项()).unwrap();
        // B104-2（步 20h-1）：部分带文本 → 用带文本的子集写指纹，记 n_text
        let sc = store.get("k").scope.unwrap();
        assert_eq!(sc.n_text, Some(238));
        assert_eq!(sc.fingerprint.unwrap().n, 238);
        assert!(
            rep[0]
                .warnings
                .iter()
                .any(|w| w.starts_with("W-scope-partial")),
            "{:?}",
            rep[0].warnings
        );
    }

    /// B68 修订：缺省边距下认证集自判范围外为 0；去掉边距、收窄分位时自判非 0 → W-scope-self。
    // 依据：B68 修订（认证集自判范围外必须为 0）
    #[test]
    fn scope_self_check_reports_w_scope_self() {
        let mut rows = 两极批("k", 240, 0); // 可认证：P1 起范围只在认证时写
        for (i, r) in rows.iter_mut().enumerate() {
            r.text = Some(format!(
                "材料{}：今天下午的会议推迟到三点{}。",
                r.item,
                "，大家准时到".repeat(i % 4)
            ));
        }
        let mut store = 库();
        let rep = import_labels(&mut store, &rows, &选项()).unwrap();
        assert_eq!(rep[0].scope_self_outside, Some(0));
        assert!(
            !rep[0]
                .warnings
                .iter()
                .any(|w| w.starts_with("W-scope-self"))
        );
        let fp = store.get("k").scope.and_then(|s| s.fingerprint).unwrap();
        assert_eq!(fp.margins, Some(ScopeMargins { k: 2.0, m: 0.10 }));

        let mut opt = 选项();
        opt.scope_quantiles = (0.3, 0.7);
        opt.scope_margins = ScopeMargins { k: 1.0, m: 0.0 };
        let mut store = 库();
        let rep = import_labels(&mut store, &rows, &opt).unwrap();
        assert!(rep[0].scope_self_outside.unwrap() > 0);
        assert!(
            rep[0]
                .warnings
                .iter()
                .any(|w| w.starts_with("W-scope-self")),
            "{:?}",
            rep[0].warnings
        );
    }

    /// B36 5(c) 不命中：分歧只有 2 条（少于 3），走原有的一致率门。
    // 依据：B36 第 5(c) 条
    #[test]
    fn extent_not_judged_below_the_disagreement_minimum() {
        let mut store = 库();
        let rep = import_labels(&mut store, &批(2), &选项()).unwrap();
        assert!(!rep[0].truth.gate.contains("外延"), "{}", rep[0].truth.gate);
        assert!(!rep[0].warnings.iter().any(|w| w.starts_with("W-extent")));
    }

    /// 类行：`computed` 真值，读数两极，`q` 道来源题轮流出样本（每侧每半够 22 条零错）。
    fn 类批(q: usize) -> Vec<LabelRow> {
        (0..240)
            .map(|i| {
                let yes = i % 2 == 0;
                serde_json::from_value(serde_json::json!({
                    "class": "c", "question": format!("题{}", i % q), "item": format!("m{i}"),
                    "p": if yes { 0.95 } else { 0.05 }, "label": yes, "source": "computed"
                }))
                .unwrap()
            })
            .collect()
    }

    /// B34、B75：类记录只在混合样本上认证：不同来源（这里是无题式的手写题面）≥ `class_min_sources`。
    // 依据：B34（类键只命中在混合样本上认证过的记录）、B75（混合样本的操作定义）
    #[test]
    fn class_record_needs_mixed_sources() {
        let ck = CalibStore::class_key("c");
        let mut store = 库();
        let rep = import_labels(&mut store, &类批(1), &选项()).unwrap();
        assert_eq!(rep[0].key, ck);
        assert_eq!(
            rep[0].truth.gate,
            "待核：类记录来源不足（1 个来源，需 ≥ 2）"
        );
        assert_ne!(store.get(&ck).status, "上岗");
        assert!(!store.records.contains_key("c"), "类行不写进作者键");

        let mut store = 库();
        let rep = import_labels(&mut store, &类批(3), &选项()).unwrap();
        assert_eq!(store.get(&ck).status, "上岗", "{:?}", rep[0].truth.gate);
        // 记录写来源计数；分半按来源分层，证书方法记 split-strata
        let r = store.records.get(&ck).unwrap();
        assert_eq!(r.sources.len(), 3);
        assert_eq!(r.sources.values().sum::<u64>(), 240);
        assert_eq!(
            r.选中的证书().unwrap().selection.as_ref().unwrap().method,
            "split-strata-stratified"
        );
    }

    /// 题键行：`n` 条 `computed` 真值，读数两极、正负各半（零错）。
    fn 两极批(key: &str, n: usize, 起: usize) -> Vec<LabelRow> {
        (起..起 + n)
            .map(|i| {
                let yes = i % 2 == 0;
                serde_json::from_value(serde_json::json!({
                    "key": key, "item": format!("m{i}"),
                    "p": if yes { 0.95 } else { 0.05 }, "label": yes, "source": "computed"
                }))
                .unwrap()
            })
            .collect()
    }

    fn 带试用() -> ImportOptions {
        ImportOptions {
            alpha_trial: Some(0.25),
            ..选项()
        }
    }

    /// B72：80 条正式 α 不够（每格要 22），试用 α 上岗；证书记 trial，门控写「试用上岗」，不以「临时上岗」开头。
    // 依据：B72（等级按 α 分档；先正式后试用）
    #[test]
    fn b72_trial_line_when_formal_falls_short() {
        let mut store = 库();
        let rep = import_labels(&mut store, &两极批("k", 80, 0), &带试用()).unwrap();
        assert_eq!(store.get("k").status, "上岗", "{}", rep[0].truth.gate);
        assert!(
            rep[0]
                .truth
                .gate
                .starts_with("试用上岗（α=0.25）：正式 α=0.1 未过：待核"),
            "{}",
            rep[0].truth.gate
        );
        assert!(rep[0].trial.as_deref().unwrap().starts_with("试用上岗"));
        let c = store.records["k"].选中的证书().unwrap();
        assert_eq!((c.grade, c.alpha), (CertGrade::Trial, 0.25));
        assert_eq!(
            rep[0].certification.as_ref().unwrap().grade,
            CertGrade::Trial
        );
        // 记录全文带等级（进账本头 calib_used）
        let j = serde_json::to_value(&store.records["k"]).unwrap();
        assert!(j.to_string().contains("\"grade\":\"trial\""));
        // 不给试用 α（或不大于正式 α）则不试
        let mut store = 库();
        let rep = import_labels(&mut store, &两极批("k", 80, 0), &选项()).unwrap();
        assert_ne!(store.get("k").status, "上岗");
        assert!(rep[0].trial.is_none());
        let mut store = 库();
        let opt = ImportOptions {
            alpha_trial: Some(0.1),
            ..选项()
        };
        import_labels(&mut store, &两极批("k", 80, 0), &opt).unwrap();
        assert_ne!(store.get("k").status, "上岗");
    }

    /// B72：正式 α 过了就不试；证书等级正式、不序列化 grade（旧记录逐字节不变的同一条）。
    #[test]
    fn b72_formal_first() {
        let mut store = 库();
        let rep = import_labels(&mut store, &两极批("k", 240, 0), &带试用()).unwrap();
        assert_eq!(rep[0].truth.gate, "上岗");
        assert!(rep[0].trial.is_none());
        assert_eq!(
            store.records["k"].选中的证书().unwrap().grade,
            CertGrade::Formal
        );
        let j = serde_json::to_value(&store.records["k"]).unwrap();
        assert!(!j.to_string().contains("\"grade\""));
    }

    /// B72：已有正式线的键，新一批正式不过时不试，线与证书原样（试用线不覆盖正式线）。
    #[test]
    fn b72_trial_never_overwrites_a_formal_line() {
        let mut store = 库();
        import_labels(&mut store, &两极批("k", 240, 0), &带试用()).unwrap();
        let before = store.records["k"].clone();
        // 新一批：高读数却判「否」的错例，使合并后的正式认证不过
        let mut bad = 两极批("k", 40, 1000);
        for r in bad.iter_mut() {
            r.label = serde_json::json!(r.p < 0.5);
        }
        let rep = import_labels(&mut store, &bad, &带试用()).unwrap();
        assert!(
            rep[0].truth.gate.starts_with("认证不过") || rep[0].truth.gate.starts_with("待核"),
            "{}",
            rep[0].truth.gate
        );
        assert!(
            rep[0]
                .trial
                .as_deref()
                .unwrap()
                .starts_with("未试：已有正式线"),
            "{:?}",
            rep[0].trial
        );
        let after = &store.records["k"];
        assert_eq!(
            (after.hi, after.lo, &after.status),
            (before.hi, before.lo, &before.status)
        );
        assert!(
            after.certs.values().all(|c| c.grade == CertGrade::Formal),
            "不许出现试用证书"
        );
    }

    /// B72：停岗候选（B25）上的正式线也不被试用线覆盖：新一批正式不过时不试，证书里不出现试用证书。
    // 依据：B72（试用线不得覆盖已存在的正式线）
    #[test]
    fn b72_trial_never_overwrites_a_suspend_candidate_formal_line() {
        let mut store = 库();
        import_labels(&mut store, &两极批("k", 240, 0), &带试用()).unwrap();
        store.records.get_mut("k").unwrap().status = "停岗候选".into();
        let before = store.records["k"].clone();
        let mut bad = 两极批("k", 40, 1000);
        for r in bad.iter_mut() {
            r.label = serde_json::json!(r.p < 0.5);
        }
        let rep = import_labels(&mut store, &bad, &带试用()).unwrap();
        assert!(
            rep[0]
                .trial
                .as_deref()
                .unwrap()
                .starts_with("未试：已有正式线"),
            "{:?}",
            rep[0].trial
        );
        let after = &store.records["k"];
        assert_eq!(
            (after.hi, after.lo, &after.status),
            (before.hi, before.lo, &before.status)
        );
        assert!(after.certs.values().all(|c| c.grade == CertGrade::Formal));
    }

    /// B72 + B63：K 元单侧线同样按 α 分档（30 条：正式每半要 22，试用要 9）。
    #[test]
    fn b72_kary_trial() {
        let rows: Vec<LabelRow> = (0..30)
            .map(|i| {
                serde_json::from_value(serde_json::json!({
                    "key": "s", "op": "select", "item": format!("m{i}"), "p": 0.9, "pick": 1, "label": 1, "source": "computed"
                }))
                .unwrap()
            })
            .collect();
        let mut store = 库();
        let rep = import_labels(&mut store, &rows, &带试用()).unwrap();
        assert_eq!(store.get("s").status, "上岗", "{}", rep[0].truth.gate);
        assert_eq!(
            store.records["s"].选中的证书().unwrap().grade,
            CertGrade::Trial
        );
    }

    /// 类行没有任何来源题身份（无 question、key、form）时拒收整批。
    #[test]
    fn class_row_without_source_question_is_rejected() {
        let rows: Vec<LabelRow> = vec![
            serde_json::from_value(serde_json::json!({
                "class": "c", "item": "m0", "p": 0.9, "label": true, "source": "computed"
            }))
            .unwrap(),
        ];
        let e = import_labels(&mut 库(), &rows, &选项()).unwrap_err();
        assert!(e.contains("来源题"), "{e}");
    }
}
