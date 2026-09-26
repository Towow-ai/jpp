//! 能力画像（档案）：按效应实例分表（步 15d，`20` §3.9、B37、B60）与档案哈希。
//!
//! 画像文件描述一个判断器（模型取 `model_version`）的读数效应实例；`gen`、`ask`、`actions` 三节可选，
//! 缺则该实例未测。字段一律是 [`Field`]：已测给值与来源，未测为 `None`，内核不替它补数（`20` A7）。
//! 画像 JSON Schema 的唯一来源是 [`profile_schema`]；[`Profile::from_json`] 按它核顶层键（未知键报错），
//! 非 δ 字段缺失即未测。
//!
//! 步 15d-2：`Profile` 没有 `Default`，唯一默认是 [`Profile::untested`]；δ 先验（`delta.*`）与冷键保守线
//! （`lines.safety_default`）也是 [`Field`]，缺即未测。画像的 δ 只作认证的先验输入（宿主经 `calib-import`
//! 取用），桥用的 δ 只从校准记录取（`20` §3.9）。依据：`21` §四·8「步 15d 拆分」。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::{EffectId, EffectInstance};
use jpp_value::value::canon;

/// 一个**类假设档案字段**的三态（`12` §1.2 绑字段、§1.3 末行「任一字段『未测』」）。
///
/// **不是 `Option<bool>`**：`未测` 是 §1.3 亲口规定过的一个**合法状态**，有自己的降级
/// 规则（按 J-15 取该假设为真、报 W-untested）。写成 `Option` 会招来 `unwrap_or(false)`
/// ——**那正是替不确定做了确定的默认**。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tri {
    真,
    假,
    /// **档案里有这份档案，但这个字段没测过。** 与「根本没加载档案」不是一回事——
    /// 前者是一份测量报告说「这项没测」，后者是连报告都没有。
    #[default]
    未测,
}

impl Tri {
    pub fn from_json(j: &Json, field: &str) -> Tri {
        match j.get(field).and_then(|v| v.as_bool()) {
            Some(true) => Tri::真,
            Some(false) => Tri::假,
            None => Tri::未测,
        }
    }
}

/// 画像字段（`20` §3.9）：已测给值与来源（画像里的路径或实验），未测为 `None`。
#[derive(Clone, Debug, PartialEq)]
pub struct Field<T> {
    pub value: Option<T>,
    pub tested_by: Option<String>,
}

impl<T> Default for Field<T> {
    fn default() -> Self {
        Field::untested()
    }
}

impl<T> Field<T> {
    pub fn untested() -> Field<T> {
        Field {
            value: None,
            tested_by: None,
        }
    }
    pub fn known(v: T, tested_by: &str) -> Field<T> {
        Field {
            value: Some(v),
            tested_by: Some(tested_by.to_string()),
        }
    }
    fn from_opt(v: Option<T>, tested_by: &str) -> Field<T> {
        match v {
            Some(v) => Field::known(v, tested_by),
            None => Field::untested(),
        }
    }
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }
    pub fn is_tested(&self) -> bool {
        self.value.is_some()
    }
}

impl Field<bool> {
    /// 布尔类假设投影为三态（J-15 的载体）
    pub fn tri(&self) -> Tri {
        match self.value {
            Some(true) => Tri::真,
            Some(false) => Tri::假,
            None => Tri::未测,
        }
    }
}

/// 窗口（H6，`window.text_slots.*`、`window.json_slots.*`）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// 对象槽内**单段**材料的可用窗口（`window.text_slots.claim_bearing_ctx.usable_lower`）
    pub text: usize,
    /// 槽间干扰的窗口（`window.json_slots…flip_frac_by_ctx_tokens` 的最大档）
    pub json_ctx: usize,
}

/// 产出读数的效应才有的类假设 H1–H8（`12` §1.2、B37、B39）与 B12 候选字段。
/// 结构较复杂的测量节（`k_limit`、`calibration`、`position_bias`）以原始 JSON 记为已测，由读它的 pass 解析。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassAssumptions {
    pub text_only: Field<bool>,
    pub modalities_accepted: Field<Vec<String>>,
    pub fixed_output_types: Field<bool>,
    pub select_sums_to_one: Field<bool>,
    pub k_limit: Field<u32>,
    pub calibration: Field<Json>,
    pub one_hop: Field<bool>,
    pub arithmetic_capable: Field<bool>,
    pub window_bounded: Field<bool>,
    pub window: Field<Window>,
    pub questions_free: Field<bool>,
    pub multi_object_crosstalk: Field<bool>,
    pub assertion_susceptible: Field<bool>,
    /// H9（B154，步 20j-3）：判断器随答案给出与最大概率不同的自报置信度。未测按假（B39 守卫侧）：
    /// `cut` 的 `stat: "confidence"` 在检查期报 `E-stat-unavailable`。
    pub reports_confidence: Field<bool>,
    pub position_bias: Field<Json>,
    /// B12 候选：错误相关性（重跑分歧率）
    pub error_correlation: Field<f64>,
    /// B12 候选：按候选类的噪声率
    pub noise_by_candidate_class: Field<Json>,
    /// B12 候选：高频单字存在性判断偏肯定
    pub single_char_existence_bias: Field<Json>,
}

/// 一个效应实例的画像分表（`20` §3.9 `EffectProfile`）：调度与预算输入；读数效应另带类假设。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectProfile {
    /// 每个 input token 的价格（美元；`cost.price_usd_per_input_token`，B73）
    pub cost: Field<f64>,
    /// 每次调用的 p95 时延（秒；`concurrency.latency_s.p95`，B32）
    pub latency_p95: Field<f64>,
    /// 端口内并发上限（`concurrency.lower_bound_ok`；未测取 1，`20` §3.9，步 15e）
    pub concurrency: Field<u32>,
    /// 单次请求超时（秒；`transport.timeout_s`，宿主传输策略）
    pub timeout_s: Field<f64>,
    pub empty_rate: Field<f64>,
    pub answer_rate: Field<f64>,
    pub fail_types: Vec<String>,
    pub taint_default: Option<String>,
    /// 只当 `EffectSpec.produces_reading`（B37）
    pub assumptions: Option<ClassAssumptions>,
}

/// 动作的声明输出形状（B51-R2 候选字段）：基数上界、单项尺寸上界、已由确定性代码定案的子判断。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatShape {
    pub items: ShapeItems,
    /// 单项尺寸上界（字符数，按规范 JSON 文本计）
    pub item_size: Option<usize>,
    /// 已定案的子判断（`count`、`order`、`boundary`、`dedup`、`arithmetic`）；本版只记录（静态面在步 24）
    pub settled: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeItems {
    One,
    AtMost(usize),
    Unbounded,
}

/// 动作画像（`20` §3.9 `ActionProfile`）
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionProfile {
    pub cost: Field<f64>,
    pub latency_p95: Field<f64>,
    pub reversible: Option<bool>,
    pub idempotent: Option<bool>,
    pub fail_types: Vec<String>,
    pub taint_out: Option<String>,
    pub mat_shape: Option<MatShape>,
}

/// 能力画像。
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// 画像描述的判断器（`model_version`）；未加载画像时为空
    pub model_id: Option<String>,
    /// 这份档案的哈希，进账本头（`12` §J-18：头不同即不承诺重放一致）。
    /// **`None` = 本次运行没有加载档案**，线与 δ 是代码兜底值——这个痕迹必须留下，
    /// 否则「用了兜底值」和「档案恰好等于兜底值」在账本上分不开。
    pub hash: Option<String>,
    /// 行为承载子集的摘要：`profile_hash` 不同时，它相同就说明只改了说明文字。它不放行任何东西。
    pub behavior_hash: Option<String>,
    /// 按效应实例的分表（B60）；没有条目的实例一律未测
    pub effects: BTreeMap<EffectInstance, EffectProfile>,
    /// 动作画像（`do`），按动作名
    pub actions: BTreeMap<String, ActionProfile>,
    /// 冷键（非上岗）用的保守线 `(hi, lo)`（`lines.safety_default`）；只供 `allocate`/`unsure_bound` 的强度排序
    pub safety: Field<(f64, f64)>,
    /// δ 先验：noul / choice / score（`delta.<题式>.immediate.p99`）。只作认证输入（`20` §3.9 `delta_prior`），
    /// `cut` 不读它
    pub delta: Field<(f64, f64, f64)>,
}

/// 画像 JSON Schema 的唯一来源（`20` §2.3、§九；步 15d）。顶层键封闭（未知键报错），嵌套测量节开放。
/// 顶层键 = 现行两份画像文件出现过的键 + 类假设字段 + B12 候选字段 + `gen`/`ask`/`actions` 三节。
pub fn profile_schema() -> Json {
    const 测量节: &[&str] = &[
        "model_version",
        "date",
        "sources",
        "window",
        "delta",
        "flip_rate",
        "batch_invariance",
        "cost",
        "concurrency",
        "k_limit",
        "position_bias",
        "anchors",
        "calibration",
        "language",
        "state_representation_default",
        "modalities_accepted",
        "question_types",
        "notes",
        "field_stats",
        "lines",
        "transport",
        "provenance",
        "run_status",
        "run_budget_usd",
    ];
    const 布尔类假设: &[&str] = &[
        "text_only",
        "fixed_output_types",
        "select_sums_to_one",
        "one_hop",
        "arithmetic_capable",
        "window_bounded",
        "questions_free",
        "multi_object_crosstalk",
        "assertion_susceptible",
        "reports_confidence",
    ];
    const 候选: &[&str] = &[
        "error_correlation",
        "noise_by_candidate_class",
        "single_char_existence_bias",
    ];
    let mut props = serde_json::Map::new();
    for k in 测量节 {
        props.insert((*k).into(), json!({}));
    }
    for k in 布尔类假设 {
        props.insert((*k).into(), json!({"type": "boolean"}));
    }
    for k in 候选 {
        props.insert(
            (*k).into(),
            json!({"description": "B12 候选字段，缺即未测"}),
        );
    }
    let 调度表 = json!({
        "type": "object",
        "properties": {
            "model": {"type": "string"},
            "price_usd_per_input_token": {"type": "number"},
            "latency_p95_s": {"type": "number"},
            "concurrency": {"type": "integer"},
            "timeout_s": {"type": "number"},
            "empty_rate": {"type": "number"},
            "answer_rate": {"type": "number"},
            "fail_types": {"type": "array", "items": {"type": "string"}}
        },
        "additionalProperties": false
    });
    props.insert("gen".into(), 调度表.clone());
    props.insert("ask".into(), 调度表);
    props.insert(
        "actions".into(),
        json!({"type": "object", "additionalProperties": {"type": "object"}}),
    );
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "J++ 能力画像（profiles/<model>.json）",
        "type": "object",
        "properties": Json::Object(props),
        "required": [],
        "additionalProperties": false
    })
}

impl Profile {
    /// 唯一的「全部未测」画像（`20` §3.9：画像字段的默认值只有一处）。`hash` 为空。
    pub fn untested() -> Profile {
        Profile {
            model_id: None,
            hash: None,
            behavior_hash: None,
            effects: BTreeMap::new(),
            actions: BTreeMap::new(),
            safety: Field::untested(),
            delta: Field::untested(),
        }
    }

    /// 某题型的 δ 先验（`Op` 顺序 noul / choice / score）；未测为 `None`
    pub fn delta_prior(&self, op: jpp_ir::key::Op) -> Option<f64> {
        self.delta.get().map(|d| match op {
            jpp_ir::key::Op::Test => d.0,
            jpp_ir::key::Op::Select => d.1,
            jpp_ir::key::Op::Measure => d.2,
        })
    }

    /// 读数效应实例的分表（本版一个读数实例，B38）。没有则为 `None`。
    pub fn judge(&self) -> Option<&EffectProfile> {
        self.effects
            .iter()
            .find(|(k, _)| crate::spec(k.effect).produces_reading)
            .map(|(_, v)| v)
    }
    fn judge_mut(&mut self) -> &mut EffectProfile {
        let effect = crate::find(|s| s.produces_reading).expect("注册表里有判断");
        if !self.effects.keys().any(|k| k.effect == effect) {
            let model = self.model_id.clone().unwrap_or_default();
            self.effects.insert(
                EffectInstance { effect, model },
                EffectProfile {
                    assumptions: Some(ClassAssumptions::default()),
                    ..EffectProfile::default()
                },
            );
        }
        self.effects
            .iter_mut()
            .find(|(k, _)| k.effect == effect)
            .map(|(_, v)| v)
            .expect("刚插入")
    }
    /// 某个效应的分表（按效应；本版每种效应至多一个实例）
    pub fn effect(&self, effect: EffectId) -> Option<&EffectProfile> {
        self.effects
            .iter()
            .find(|(k, _)| k.effect == effect)
            .map(|(_, v)| v)
    }
    fn assumptions(&self) -> Option<&ClassAssumptions> {
        self.judge().and_then(|e| e.assumptions.as_ref())
    }

    /// **H5 的档案字段**（`12` §1.2）：模型会不会算术。**`true` 表示 H5 _不_ 成立**。
    pub fn arithmetic_capable(&self) -> Tri {
        self.assumptions()
            .map(|a| a.arithmetic_capable.tri())
            .unwrap_or_default()
    }
    /// **H1 的档案字段**（`12` §1.2、B126）：判断器只收文字。未测按 `true`（B39），
    /// 与 `state` 的 `E-modality` 守卫半同用（步 24）。
    pub fn text_only(&self) -> Tri {
        self.assumptions()
            .map(|a| a.text_only.tri())
            .unwrap_or_default()
    }
    /// `text_only: false` 时判断器接受的模态集合（B126）；未测为 `None`。
    pub fn modalities_accepted(&self) -> Option<&[String]> {
        self.assumptions()
            .and_then(|a| a.modalities_accepted.get())
            .map(|v| v.as_slice())
    }
    /// **H2 的档案字段之一**（`12` §1.2，步 24d）：`test`/`select`/`measure` 三种输出结构的
    /// 形状是否固定（无「都不是」除非显式给）。
    pub fn fixed_output_types(&self) -> Tri {
        self.assumptions()
            .map(|a| a.fixed_output_types.tri())
            .unwrap_or_default()
    }
    /// **H2 的档案字段之一**（`12` §1.2，步 24d）：`select` 的候选概率和是否恒为 1。
    pub fn select_sums_to_one(&self) -> Tri {
        self.assumptions()
            .map(|a| a.select_sums_to_one.tri())
            .unwrap_or_default()
    }
    /// **H3 的档案字段**（`12` §1.2，步 24d）：判断器的校准测量是否过检。结构较复杂，记为原始
    /// JSON（`Field<Json>`），这里只问「测没测」，不解析内容。
    pub fn calibration_tested(&self) -> bool {
        self.assumptions()
            .is_some_and(|a| a.calibration.is_tested())
    }
    /// **H4 的档案字段**（`12` §1.2，步 24d）：模型是不是只能一跳字面。未测按 `true`（B39），
    /// `insufficient` 先查保留。
    pub fn one_hop(&self) -> Tri {
        self.assumptions()
            .map(|a| a.one_hop.tri())
            .unwrap_or_default()
    }
    /// **H8 的档案字段**（`12` §1.2，步 24d）：同状态多对象逐对象判是否存在串扰。未测按 `true`
    /// （B39，B62 已有真机证据），J-14 的静态面据此决定 error 还是 warn。
    pub fn multi_object_crosstalk(&self) -> Tri {
        self.assumptions()
            .map(|a| a.multi_object_crosstalk.tri())
            .unwrap_or_default()
    }
    /// **H9 的档案字段**（`12` §1.2，B154，步 20j-3）：判断器是否随答案给出自报置信度。未测按假
    /// （B39 守卫侧）：用 `stat: "confidence"` 的 `cut` 在检查期报 `E-stat-unavailable`，不静默退回 p_max。
    pub fn reports_confidence(&self) -> Tri {
        self.assumptions()
            .map(|a| a.reports_confidence.tri())
            .unwrap_or_default()
    }
    /// 判断调用的 p95 时延（秒），B32 静态时延估计用；未测为 `None`
    pub fn latency_p95(&self) -> Option<f64> {
        self.judge().and_then(|e| e.latency_p95.get().copied())
    }
    /// 每个 input token 的价格（B73）；未测为 `None`
    pub fn price_per_input_token(&self) -> Option<f64> {
        self.judge().and_then(|e| e.cost.get().copied())
    }
    /// 单次真机请求的超时（秒）；未测为 `None`
    pub fn transport_timeout_s(&self) -> Option<f64> {
        self.judge().and_then(|e| e.timeout_s.get().copied())
    }
    /// 端口内并发上限；未测为 `None`（使用处取 1，`20` §3.9）
    pub fn concurrency(&self) -> Option<u32> {
        self.judge().and_then(|e| e.concurrency.get().copied())
    }
    /// 窗口（H6）；未测为 `None`（使用处不核对象长度、报 `W-window-untested`）
    pub fn window(&self) -> Option<Window> {
        self.assumptions().and_then(|a| a.window.get().copied())
    }

    /// 测试与宿主设置判断实例的 p95 时延（B32）
    pub fn with_latency_p95(mut self, p95: f64, tested_by: &str) -> Profile {
        self.judge_mut().latency_p95 = Field::known(p95, tested_by);
        self
    }
    /// 端口内并发上限（步 15e；宿主与测试用，文件画像经 `concurrency.lower_bound_ok` 读）
    pub fn with_concurrency(mut self, n: u32, tested_by: &str) -> Profile {
        self.judge_mut().concurrency = Field::known(n, tested_by);
        self
    }
    /// H1（`text_only`，B126；宿主与测试用）
    pub fn with_text_only(mut self, v: bool, tested_by: &str) -> Profile {
        self.judge_mut()
            .assumptions
            .get_or_insert_with(ClassAssumptions::default)
            .text_only = Field::known(v, tested_by);
        self
    }
    /// `text_only: false` 时接受的模态集合（B126；宿主与测试用）
    pub fn with_modalities_accepted(mut self, v: Vec<String>, tested_by: &str) -> Profile {
        self.judge_mut()
            .assumptions
            .get_or_insert_with(ClassAssumptions::default)
            .modalities_accepted = Field::known(v, tested_by);
        self
    }
    /// H4（`one_hop`；宿主与测试用，步 24g）
    pub fn with_one_hop(mut self, v: bool, tested_by: &str) -> Profile {
        self.judge_mut()
            .assumptions
            .get_or_insert_with(ClassAssumptions::default)
            .one_hop = Field::known(v, tested_by);
        self
    }
    /// H5（`arithmetic_capable`；宿主与测试用，步 24g）
    pub fn with_arithmetic_capable(mut self, v: bool, tested_by: &str) -> Profile {
        self.judge_mut()
            .assumptions
            .get_or_insert_with(ClassAssumptions::default)
            .arithmetic_capable = Field::known(v, tested_by);
        self
    }

    /// 从档案读（`12` §1「凡是数字都是档案字段」）：先按 [`profile_schema`] 核顶层键，未知键报错并指名。
    pub fn load(path: &std::path::Path) -> Result<Profile, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("读不到档案 {}：{e}", path.display()))?;
        let j: Json = serde_json::from_str(&text).map_err(|e| format!("档案不是合法 JSON：{e}"))?;
        Profile::from_json(&j)
    }

    pub fn from_json(j: &Json) -> Result<Profile, String> {
        let obj = j.as_object().ok_or("档案要是 JSON 对象")?;
        let schema = profile_schema();
        let known = schema["properties"]
            .as_object()
            .expect("schema 有 properties");
        let mut unknown: Vec<&str> = obj
            .keys()
            .filter(|k| !known.contains_key(k.as_str()))
            .map(|k| k.as_str())
            .collect();
        if !unknown.is_empty() {
            unknown.sort();
            // 依据：21 步 15d（画像 schema 唯一来源；未知字段报错并指名，20 §3.9）
            return Err(format!(
                "档案有未知字段：{}（画像 schema 见 jpp_effects::profile_schema；拼错的字段会被静默当成未测，所以报错）",
                unknown.join("、")
            ));
        }
        let 取 = |路径: &[&str]| -> Option<f64> {
            let mut cur = j;
            for k in 路径 {
                cur = cur.get(k)?;
            }
            cur.as_f64()
        };
        // 线与 δ 先验缺即未测（步 15d-2；此前缺即报错）。三档 δ 都在才算已测。
        let safety = match (
            取(&["lines", "safety_default", "hi"]),
            取(&["lines", "safety_default", "lo"]),
        ) {
            (Some(hi), Some(lo)) => Field::known((hi, lo), "lines.safety_default"),
            _ => Field::untested(),
        };
        // choice 的 δ 取「被选中那档的概率」那一列（与 Python `delta_for` 的映射一致）
        let delta = match (
            取(&["delta", "noul", "immediate", "p99"]),
            取(&["delta", "choice_prob_chosen", "immediate", "p99"]),
            取(&["delta", "score", "immediate", "p99"]),
        ) {
            (Some(dn), Some(dc), Some(ds)) => Field::known((dn, dc, ds), "delta.*.immediate.p99"),
            _ => Field::untested(),
        };
        // 窗口缺即未测（步 15d；此前缺即报错）。两段都在才算已测。
        let text_window =
            取(&["window", "text_slots", "claim_bearing_ctx", "usable_lower"]).map(|v| v as usize);
        let json_ctx_window = j
            .get("window")
            .and_then(|w| w.get("json_slots"))
            .and_then(|s| s.get("claim_bearing_ctx"))
            .and_then(|c| c.get("flip_frac_by_ctx_tokens"))
            .and_then(|m| m.as_object())
            .and_then(|m| {
                m.keys()
                    .filter_map(|k| k.trim_start_matches('~').parse::<usize>().ok())
                    .max()
            });
        let window = match (text_window, json_ctx_window) {
            (Some(text), Some(json_ctx)) => Field::known(Window { text, json_ctx }, "window"),
            _ => Field::untested(),
        };
        let b = |k: &str| Field::from_opt(j.get(k).and_then(|v| v.as_bool()), k);
        let raw = |k: &str| Field::from_opt(j.get(k).cloned(), k);
        let assumptions = ClassAssumptions {
            text_only: b("text_only"),
            modalities_accepted: Field::from_opt(
                j.get("modalities_accepted").and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                }),
                "modalities_accepted",
            ),
            fixed_output_types: b("fixed_output_types"),
            select_sums_to_one: b("select_sums_to_one"),
            k_limit: Field::from_opt(
                取(&["k_limit", "hard_max_options"]).map(|v| v as u32),
                "k_limit.hard_max_options",
            ),
            calibration: raw("calibration"),
            one_hop: b("one_hop"),
            arithmetic_capable: b("arithmetic_capable"),
            window_bounded: b("window_bounded"),
            window,
            questions_free: b("questions_free"),
            multi_object_crosstalk: b("multi_object_crosstalk"),
            assertion_susceptible: b("assertion_susceptible"),
            reports_confidence: b("reports_confidence"),
            position_bias: raw("position_bias"),
            error_correlation: Field::from_opt(
                j.get("error_correlation").and_then(|v| v.as_f64()),
                "error_correlation",
            ),
            noise_by_candidate_class: raw("noise_by_candidate_class"),
            single_char_existence_bias: raw("single_char_existence_bias"),
        };
        let judge = EffectProfile {
            cost: Field::from_opt(
                取(&["cost", "price_usd_per_input_token"]),
                "cost.price_usd_per_input_token",
            ),
            latency_p95: Field::from_opt(
                取(&["concurrency", "latency_s", "p95"]),
                "concurrency.latency_s.p95",
            ),
            concurrency: Field::from_opt(
                取(&["concurrency", "lower_bound_ok"])
                    .filter(|x| *x >= 1.0)
                    .map(|v| v as u32),
                "concurrency.lower_bound_ok",
            ),
            timeout_s: Field::from_opt(
                取(&["transport", "timeout_s"]).filter(|x| x.is_normal() && x.is_sign_positive()),
                "transport.timeout_s",
            ),
            assumptions: Some(assumptions),
            ..EffectProfile::default()
        };
        let model = j
            .get("model_version")
            .and_then(|v| v.as_str())
            .map(String::from);
        let mut effects = BTreeMap::new();
        let judge_id = crate::find(|s| s.produces_reading).expect("注册表里有判断");
        effects.insert(
            EffectInstance {
                effect: judge_id,
                model: model.clone().unwrap_or_default(),
            },
            judge,
        );
        // gen、ask 两节（可选）：只有调度与预算输入（B37）
        for (节, pred) in [
            (
                "gen",
                (|s: &crate::EffectSpec| {
                    !s.produces_reading
                        && s.in_effect_row
                        && !s.side_effecting
                        && s.output_shape == crate::OutputShape::Mats
                }) as fn(&crate::EffectSpec) -> bool,
            ),
            ("ask", |s: &crate::EffectSpec| {
                s.output_shape == crate::OutputShape::Answer
            }),
        ] {
            if let Some(t) = j.get(节) {
                let id = crate::find(pred).expect("注册表里有该效应");
                let f = |k: &str| {
                    Field::from_opt(t.get(k).and_then(|v| v.as_f64()), &format!("{节}.{k}"))
                };
                effects.insert(
                    EffectInstance {
                        effect: id,
                        model: t
                            .get("model")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                            .unwrap_or_default(),
                    },
                    EffectProfile {
                        cost: f("price_usd_per_input_token"),
                        latency_p95: f("latency_p95_s"),
                        concurrency: Field::from_opt(
                            t.get("concurrency")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u32),
                            &format!("{节}.concurrency"),
                        ),
                        timeout_s: f("timeout_s"),
                        empty_rate: f("empty_rate"),
                        answer_rate: f("answer_rate"),
                        fail_types: t
                            .get("fail_types")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        ..EffectProfile::default()
                    },
                );
            }
        }
        let mut actions = BTreeMap::new();
        if let Some(a) = j.get("actions").and_then(|v| v.as_object()) {
            for (name, t) in a {
                let f = |k: &str| {
                    Field::from_opt(
                        t.get(k).and_then(|v| v.as_f64()),
                        &format!("actions.{name}.{k}"),
                    )
                };
                actions.insert(
                    name.clone(),
                    ActionProfile {
                        cost: f("cost"),
                        latency_p95: f("latency_p95_s"),
                        reversible: t.get("reversible").and_then(|v| v.as_bool()),
                        idempotent: t.get("idempotent").and_then(|v| v.as_bool()),
                        taint_out: t
                            .get("taint_out")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        ..ActionProfile::default()
                    },
                );
            }
        }
        Ok(Profile {
            model_id: model,
            hash: Some(profile_hash(j)),
            behavior_hash: Some(behavior_hash(j)),
            effects,
            actions,
            safety,
            delta,
        })
    }
}

/// **行为承载子集摘要**：只覆盖会进入执行的字段，不覆盖 `note`/`source`/`value` 那类说明。
///
/// 为什么要它（总控 2026-09-21 §2.10 修订）：`profile_hash` 覆盖整份档案是对的——头不同
/// 只说「不承诺重放一致」，倒向拒绝那一侧。**但它只给一个比特，而现场有两个**：
/// 一晚上两次纯文档更正把对照基准打红、**行为一个字节没变**。那会造出
/// 「**改对文档要付代价**」的反向激励，而同一个数被写错三次正是在这个激励下发生的。
///
/// **它不放行任何东西**：`profile_hash` 不同时该怎么判还怎么判。它只让看账本的人知道
/// 这次不同属于哪一种：**线和 δ 也变了，还是只改了说明。**
pub fn behavior_hash(j: &Json) -> String {
    // 会进入执行的字段。加字段时要同步这里——漏加的后果是「行为变了但摘要没变」，
    // 比多加一个字段糟得多，所以宁可多收。
    const 行为字段: &[&str] = &[
        "lines",
        "delta",
        "window",
        "k_limit",
        "position_bias",
        "anchors",
        "concurrency",
        "cost",
        "select_sums_to_one",
        "fixed_output_types",
        // H9（B154）：决定 `stat: "confidence"` 能不能用
        "reports_confidence",
        // 宿主传输策略（超时改变执行：挂起的请求变成 absent）
        "transport",
    ];
    let mut sub = serde_json::Map::new();
    for k in 行为字段 {
        if let Some(v) = j.get(*k) {
            sub.insert((*k).to_string(), strip_prose(v));
        }
    }
    hash16(&Json::Object(sub))
}

/// 去掉说明性字段：它们改了不影响执行
fn strip_prose(j: &Json) -> Json {
    const 说明字段: &[&str] = &[
        "note", "source", "by", "reason", "status", "notes", "comment",
    ];
    match j {
        Json::Object(m) => Json::Object(
            m.iter()
                .filter(|(k, _)| !说明字段.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), strip_prose(v)))
                .collect(),
        ),
        Json::Array(a) => Json::Array(a.iter().map(strip_prose).collect()),
        other => other.clone(),
    }
}

/// 规范 JSON 的 sha256 前 16 个十六进制字符（校准记录、证书地址也用它）。
pub fn hash16(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(canon(j).as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string()
}

/// 与 Python 的 `H(profile)` 同值：`sha256(canon([profile]))` 取前 16 个十六进制字符。
/// **必须同值**——它进账本头，两边算不出同一个数，跨内核的重放判定就对不上。
pub fn profile_hash(j: &Json) -> String {
    use sha2::{Digest, Sha256};
    let wrapped = Json::Array(vec![j.clone()]);
    let mut h = Sha256::new();
    h.update(canon(&wrapped).as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 发行画像() -> Json {
        let p =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/jev-1.13.0.json");
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    /// 传输超时（地基/过程记录/工程-传输超时.md）：`transport.timeout_s` 读进画像，缺则 `None`，非正数不收；
    /// 这一节进行为摘要（超时改变执行）。
    #[test]
    fn transport_timeout_字段() {
        let mut j = 发行画像();
        let with = Profile::from_json(&j).unwrap();
        assert_eq!(with.transport_timeout_s(), Some(30.0));
        let bh = with.behavior_hash.clone();
        j["transport"]["timeout_s"] = serde_json::json!(0);
        assert_eq!(Profile::from_json(&j).unwrap().transport_timeout_s(), None);
        j.as_object_mut().unwrap().remove("transport");
        let without = Profile::from_json(&j).unwrap();
        assert_eq!(without.transport_timeout_s(), None);
        assert_ne!(without.behavior_hash, bh, "transport 进行为摘要");
    }

    /// 步 15d：发行画像描述判断实例（模型 = `model_version`），生成与问人实例未测（B37、B60）
    #[test]
    fn 画像按效应实例分表() {
        let p = Profile::from_json(&发行画像()).unwrap();
        assert_eq!(p.model_id.as_deref(), Some("jev-1.13.0"));
        let judge = p.judge().expect("有判断实例");
        assert!(judge.assumptions.is_some(), "读数效应带类假设");
        assert_eq!(p.concurrency(), Some(32));
        assert!(p.price_per_input_token().is_some());
        assert!(p.latency_p95().is_some());
        assert_eq!(p.arithmetic_capable(), Tri::未测, "发行画像没测算术");
        assert!(p.window().is_some());
        for e in crate::PORTED
            .into_iter()
            .filter(|e| !crate::spec(*e).produces_reading)
        {
            assert!(p.effect(e).is_none(), "{e:?} 没有画像分表 = 未测");
        }
        assert!(p.actions.is_empty());
    }

    /// 步 15d：`profile_schema` 是唯一来源——两份现行画像都过，未知顶层字段报错并指名
    #[test]
    fn schema接受现行画像并拒绝未知字段() {
        let 源 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/foundation/profile/profiles/jev-1.13.0.json");
        Profile::load(&源).expect("foundation 来源画像要过");
        let mut j = 发行画像();
        Profile::from_json(&j).expect("发行画像要过");
        j["arithmetic_capabel"] = serde_json::json!(true);
        let e = Profile::from_json(&j).expect_err("拼错的字段要报错");
        assert!(e.contains("arithmetic_capabel"), "{e}");
    }

    /// 步 15d：非 δ 字段缺即未测（窗口、价格、并发、超时），线与 δ 仍必填（15d-2 前）
    #[test]
    fn 缺字段即未测() {
        let mut j = 发行画像();
        for k in ["window", "cost", "concurrency", "transport"] {
            j.as_object_mut().unwrap().remove(k);
        }
        let p = Profile::from_json(&j).expect("缺这些字段不报错");
        assert_eq!(p.window(), None);
        assert_eq!(p.price_per_input_token(), None);
        assert_eq!(p.concurrency(), None);
        assert_eq!(p.latency_p95(), None);
        j.as_object_mut().unwrap().remove("delta");
        j.as_object_mut().unwrap().remove("lines");
        let p = Profile::from_json(&j).expect("δ 先验与保守线缺也是未测（15d-2）");
        assert!(p.delta.get().is_none() && p.safety.get().is_none());
    }

    /// `Profile::untested()` 全部未测、没有哈希
    #[test]
    fn untested全部未测() {
        let p = Profile::untested();
        assert!(p.hash.is_none() && p.effects.is_empty() && p.actions.is_empty());
        assert_eq!(p.arithmetic_capable(), Tri::未测);
        assert_eq!(p.window(), None);
        assert_eq!(p.concurrency(), None);
        assert!(p.delta.get().is_none() && p.safety.get().is_none());
    }

    /// 可选的 gen、ask、actions 三节读进各自分表（只有调度与预算输入，没有类假设）
    #[test]
    fn 其他效应的分表() {
        let mut j = 发行画像();
        j["gen"] = serde_json::json!({"model": "gen-x", "latency_p95_s": 2.0, "concurrency": 4});
        j["actions"] = serde_json::json!({"写文件": {"cost": 0.0, "reversible": false}});
        let p = Profile::from_json(&j).unwrap();
        let g = p
            .effect(
                crate::find(|s| {
                    s.in_effect_row
                        && !s.side_effecting
                        && s.output_shape == crate::OutputShape::Mats
                })
                .unwrap(),
            )
            .expect("gen 分表");
        assert_eq!(g.latency_p95.get(), Some(&2.0));
        assert_eq!(g.concurrency.get(), Some(&4));
        assert!(g.assumptions.is_none(), "生成没有类假设（B37）");
        assert_eq!(p.actions["写文件"].reversible, Some(false));
    }
}
