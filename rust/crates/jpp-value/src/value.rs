//! 运行期值。`Mat`（材料）与 `Reading`（读数）是不同变体：读数只能经 `cut` 离开（J-01）。
//! 函数值带显式环境链 `Env`，没有 Rust 闭包，可打印、可序列化成名字→值。

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use jpp_ir::ir::Span;

pub use crate::guard_ev::GuardEv;
// 闭包持有 IR 函数（步 12c：运行时读 IR）
use jpp_ir::ir::Function;

pub use jpp_ir::key::{LineGrade, canon, hash_of};

pub use crate::prov::{Edge, EdgeKind, Provenance, Sources, join as prov_join};
pub use jpp_ir::question_kind::{
    OnShape, OverKind, OverShape, QuestionKind, Request, SlotDecls, SlotShape, question_kind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Taint {
    Trusted,
    // 反序列化缺字段时按保守那边兜底（与 json_to_effect_value 同一条判据）
    #[default]
    Untrusted,
}

impl Taint {
    pub fn join(a: Taint, b: Taint) -> Taint {
        if a == Taint::Untrusted || b == Taint::Untrusted {
            Taint::Untrusted
        } else {
            Taint::Trusted
        }
    }
}

/// 唯一材料类型（§2）。外部 crate 只能经 `Mat::new` / `Mat::literal` 构造（`#[non_exhaustive]`）；
/// 反序列化经 `raw::MatRaw` 重新走 `Mat::new`（I6）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "crate::raw::MatRaw")]
#[non_exhaustive]
pub struct Mat {
    pub content: Json,
    pub addr: String,
    pub modality: String,
    /// 来源链：literal / gen:<key> / do:<key> / ask:<key> / transform:<key> / exit:<q>
    pub origin: Vec<String>,
    pub taint: Taint,
    /// 由哪些题派生（J-02 禁自指）
    pub derived_from: BTreeSet<String>,
    pub hash: String,
    /// 来源出口的账本键（B59，步 17a）：这份材料由哪些判断的出口选出或转成。**不进 `hash`**，
    /// 空集不序列化。本版只在结构通道上打（出口转材料、元素记录转材料、效应输出承接输入），
    /// 经普通值（`content()` 读出、`e.item` 取出）的依赖不计，待候选 B84（值级来源）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub from_key: BTreeSet<String>,
    /// `from_key` 里的值依赖边：键 → 题哈希（B92，步 18c）。不在这里的键是选择依赖边。
    /// 并入值依赖边时其题哈希同步并进 `derived_from`，所以 `derived_from` 就是 J-02 的投影
    /// （∪ 从账本解码的效应输出自带的 `derived_from`）。不进 `hash`，空不序列化。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub value_q: BTreeMap<String, String>,
}

impl Mat {
    pub fn new(
        content: Json,
        addr: &str,
        origin: Vec<String>,
        taint: Taint,
        derived_from: BTreeSet<String>,
    ) -> Mat {
        let hash = hash_of(&["mat", &canon(&content), addr]);
        Mat {
            content,
            addr: addr.to_string(),
            modality: "text".into(),
            origin,
            taint,
            derived_from,
            hash,
            from_key: BTreeSet::new(),
            value_q: BTreeMap::new(),
        }
    }
    /// 材料的来源标签（B84）：`(taint, from_key)`。两个字段分存（报告 JSON 与 `.taint` 读取不变），
    /// 传播只经这里与 [`Value::with_prov`]。
    pub fn prov(&self) -> Provenance {
        let map = self
            .from_key
            .iter()
            .map(|k| {
                let e = match self.value_q.get(k) {
                    Some(q) => Edge {
                        kind: EdgeKind::Value,
                        q: q.clone(),
                    },
                    None => Edge {
                        kind: EdgeKind::Select,
                        q: String::new(),
                    },
                };
                (k.clone(), e)
            })
            .collect();
        Provenance::new(self.taint, Sources::from_map(map))
    }
    /// 并入来源出口的账本键（B59），作选择依赖边（只有键）。空键不记。不改哈希。
    pub fn with_from_key<I: IntoIterator<Item = String>>(mut self, keys: I) -> Mat {
        self.from_key
            .extend(keys.into_iter().filter(|k| !k.is_empty()));
        self
    }
    /// 并入来源边（B92，步 18c）：键进 `from_key`；值依赖边另记题哈希并进 `derived_from`
    /// （J-02 的投影）。同键已有值边不降为选择边。不改哈希。
    pub fn with_sources(mut self, s: &Sources) -> Mat {
        for (k, e) in s.edges() {
            if k.is_empty() {
                continue;
            }
            self.from_key.insert(k.clone());
            if e.kind == EdgeKind::Value {
                self.value_q.insert(k.clone(), e.q.clone());
                if !e.q.is_empty() {
                    self.derived_from.insert(e.q.clone());
                }
            }
        }
        self
    }
    pub fn literal(content: Json) -> Mat {
        Mat::new(
            content,
            "",
            vec!["literal".into()],
            Taint::Trusted,
            BTreeSet::new(),
        )
    }
    /// 估算 token。与 Python 的 `ir.py:84` **同一个估法**（`int(len(canon)/1.3) + 1`，
    /// 依据是档案 `cost.regression` 的 state_char_coef≈1、1.3 字符/token）——
    /// 两边估法不同，窗口检查就会在不同的地方触发。
    ///
    /// 窗口检查（J-14 / `12`:117）要它：对象槽内单段按 `window.text_slots…usable_lower`，
    /// 槽间按 `window.json_slots`。超窗的后果不是算错，是**读数被语境接管而无人察觉**
    /// ——档案原话「≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管」。
    pub fn tokens(&self) -> usize {
        (canon(&self.content).chars().count() as f64 / 1.3) as usize + 1
    }
    pub fn text(&self) -> String {
        match &self.content {
            Json::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

pub use jpp_ir::key::Op;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Question {
    pub op: Op,
    pub text: String,
    /// 校准键；线只从校准记录来（I4 / J-03）
    pub calib: String,
    /// measure 的档位
    pub scale: Vec<String>,
    /// J-09 的**决定性证据槽**名（`on` / `ctx` / `ref` / `over`）。
    /// `cut` 判序第一步：这些槽不在状态里就**不信任 p**（`12`:148、:244）。
    #[serde(default)]
    pub evidence: Vec<String>,
    pub hash: String,
    /// B1 的「前提」：题预设为真的命题（Belnap & Steel 1976）。**只作声明**：
    /// 本版不发给判断器、不进题哈希、不改 `cut`；前提不成立的出口位置（insufficient）是 B4 的事。
    #[serde(default)]
    pub presupposition: Option<String>,
    /// B1 的「请求」：对 K 元划分，要选一个还是全部。`None` = 按题型取缺省（见 `request()`）。
    #[serde(default)]
    pub request: Option<String>,
    /// 来自哪个题式（题式哈希）。**不进题哈希**：同题面同题型就是同一道题，
    /// 无论它是手写的还是由题式填出来的。校准键暂不改（B2 待裁），这里只留痕以便日后切换。
    #[serde(default)]
    pub form_hash: Option<String>,
    /// 题式的模板题面（带 `{槽}`）
    #[serde(default)]
    pub template: Option<String>,
    /// 填法：槽名 → 填入的文本
    #[serde(default)]
    pub fill: Option<Vec<(String, String)>>,
    /// 派生题的来源出口账本键（B59，步 17a）。**不进题哈希**，空集不序列化。
    /// 本步只落消费一侧（判断时并入 `parents`）；没有结构通道产生它（`select` 的 `pick` 臂只交出下标），
    /// 生产者是 B45 派生（步 28）与候选 B84。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub from_key: BTreeSet<String>,
    /// 题式对 `over` 的声明（B76，步 12e-2），由 [`Form::fill`] 带过来。不进题哈希，空不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_kind: Option<OverKind>,
    /// 置换声明（B64，步 15f）：`select` 站点要求判断器按正逆两序各读一次，`cut` 据置换众数一致给
    /// `Pick`。测量声明，**不进题哈希**（换不换置换是同一道题、同一条校准键），为假不序列化。
    #[serde(default, skip_serializing_if = "is_false")]
    pub permute: bool,
    /// 题面 taint（B58，步 17b）：各槽填入值、计算出的题面文本与题式模板的 taint 之 ∨，字面题为 Trusted。
    /// 判断器读到的题面与读到的材料同样可能被注入，读数与出口 taint = 状态 ∨ 题。
    /// **不进题哈希、不进任何账本键**（同一题面就是同一道题）；Trusted 不序列化。
    /// 依据：B58（`12` §2.11 第 4 条；`20` v2 §3.10「文本 → 题面」行）
    #[serde(default, skip_serializing_if = "is_trusted")]
    pub taint: Taint,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_trusted(t: &Taint) -> bool {
    *t == Taint::Trusted
}

impl Question {
    /// 基础题类（B76，步 12e-2）：不看状态，`question_kind(op, request, 未知槽形, 题式声明)`。
    /// 派生只读、不序列化、不进任何哈希；精化类（看状态槽形）在登记读数时算。
    pub fn kind(&self) -> QuestionKind {
        self.kind_on(&SlotShape::UNKNOWN)
    }
    /// 给定状态槽形时的题类（精化类）。
    pub fn kind_on(&self, shape: &SlotShape) -> QuestionKind {
        let request = self.request.as_deref().and_then(Request::parse);
        let decl = SlotDecls {
            over_kind: self.over_kind,
            accepts: None,
        };
        question_kind(self.op, request, shape, &decl).0
    }
    pub fn new(op: Op, text: &str, calib: &str, scale: Vec<String>) -> Question {
        Question::with_evidence(op, text, calib, scale, vec![])
    }
    /// 带决定性证据槽的题（J-09）。`evidence` 进题的哈希——**声明了证据的题和没声明的
    /// 不是同一道题**，账本键按题哈希走，不能让它们共用一条记录。
    pub fn with_evidence(
        op: Op,
        text: &str,
        calib: &str,
        scale: Vec<String>,
        evidence: Vec<String>,
    ) -> Question {
        let hash = hash_of(&[
            "q",
            op.phys(),
            text,
            &scale.join("\u{1e}"),
            &evidence.join("\u{1e}"),
        ]);
        Question {
            op,
            text: text.to_string(),
            calib: calib.to_string(),
            scale,
            evidence,
            hash,
            presupposition: None,
            request: None,
            form_hash: None,
            template: None,
            fill: None,
            from_key: BTreeSet::new(),
            over_kind: None,
            permute: false,
            taint: Taint::Trusted,
        }
    }

    /// B1 的「主体」：判断读状态的哪个槽、几个对象。由题型推出——
    /// 是非与打分读 `on` 里的一个对象（关系题是一对，由状态决定，不由题决定）；K 选一读 `over` 里的 K 个候选。
    pub fn subject(&self) -> &'static str {
        match self.op {
            Op::Select => "over",
            Op::Test | Op::Measure => "on",
        }
    }
    /// B1 的「划分」：是非 = 二元划分，K 选一 = K 元划分，打分 = 有序划分（Groenendijk & Stokhof 1984）。
    pub fn partition(&self) -> &'static str {
        match self.op {
            Op::Test => "binary",
            Op::Select => "k_ary",
            Op::Measure => "ordered",
        }
    }
    /// B1 的「请求」。缺省：是非题 `whether`（问是否），K 选一 `one`（恰选一个），打分 `degree`（取一档）。
    pub fn request(&self) -> String {
        self.request
            .clone()
            .unwrap_or_else(|| default_request(self.op).to_string())
    }
}

pub fn default_request(op: Op) -> &'static str {
    match op {
        Op::Test => "whether",
        Op::Select => "one",
        Op::Measure => "degree",
    }
}

/// 题式：带参数槽的题模板（B1、施工件 b）。`fill` 给每个槽填上文本，得到一道题。
///
/// 题式本身不能被判断——`judge` 只收题。它是题的来源，不是题。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Form {
    pub op: Op,
    /// 模板题面，槽写成 `{名字}`
    pub template: String,
    /// 模板里出现的槽名，按首次出现的顺序
    pub slots: Vec<String>,
    pub calib: String,
    pub scale: Vec<String>,
    pub evidence: Vec<String>,
    pub presupposition: Option<String>,
    pub request: Option<String>,
    pub hash: String,
    /// `over` 的声明（B76）：`labels`/`candidates`/`questions`/`actions`。只决定题类，不进 `form_hash`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over_kind: Option<OverKind>,
    /// 置换声明（B64，步 15f），由 `fill` 带到题上。不进 `form_hash`，为假不序列化。
    #[serde(default, skip_serializing_if = "is_false")]
    pub permute: bool,
    /// 模板文本的 taint（B58 的解释，步 17b）：`form(计算文本, …)` 的模板也是判断器读到的题面，
    /// `fill` 时并进题。不进 `form_hash`；Trusted 不序列化。
    #[serde(default, skip_serializing_if = "is_trusted")]
    pub taint: Taint,
}

impl Form {
    /// 解析模板里的 `{槽}`。`{{` / `}}` 不作转义——模板里不支持字面花括号，出现未闭合的 `{` 报错。
    pub fn slots_of(template: &str) -> Result<Vec<String>, String> {
        let mut out: Vec<String> = vec![];
        let mut rest = template;
        while let Some(i) = rest.find('{') {
            let after = &rest[i + 1..];
            let Some(j) = after.find('}') else {
                return Err(format!("模板「{template}」里有未闭合的 {{"));
            };
            let name = after[..j].trim();
            if name.is_empty() {
                return Err(format!("模板「{template}」里有空槽 {{}}"));
            }
            if !out.iter().any(|x| x == name) {
                out.push(name.to_string());
            }
            rest = &after[j + 1..];
        }
        Ok(out)
    }
    pub fn new(
        op: Op,
        template: &str,
        calib: &str,
        scale: Vec<String>,
        evidence: Vec<String>,
        presupposition: Option<String>,
        request: Option<String>,
    ) -> Result<Form, String> {
        let slots = Form::slots_of(template)?;
        let hash = hash_of(&[
            "form",
            op.phys(),
            template,
            &scale.join("\u{1e}"),
            &evidence.join("\u{1e}"),
            presupposition.as_deref().unwrap_or(""),
            request.as_deref().unwrap_or(""),
        ]);
        Ok(Form {
            op,
            template: template.to_string(),
            slots,
            calib: calib.to_string(),
            scale,
            evidence,
            presupposition,
            request,
            hash,
            over_kind: None,
            permute: false,
            taint: Taint::Trusted,
        })
    }
    /// 按填法得到一道题。槽必须恰好填满：缺槽、多槽都是错——多出来的键多半是拼错的槽名。
    pub fn fill(&self, fill: &[(String, String)]) -> Result<Question, String> {
        for s in &self.slots {
            if !fill.iter().any(|(k, _)| k == s) {
                return Err(format!("题式「{}」的槽 {s} 没有填", self.template));
            }
        }
        for (k, _) in fill {
            if !self.slots.iter().any(|s| s == k) {
                return Err(format!(
                    "题式「{}」没有槽 {k}（它的槽是 {}）",
                    self.template,
                    self.slots.join("、")
                ));
            }
        }
        let mut text = self.template.clone();
        for (k, v) in fill {
            text = text.replace(&format!("{{{k}}}"), v);
        }
        let mut q = Question::with_evidence(
            self.op,
            &text,
            &self.calib,
            self.scale.clone(),
            self.evidence.clone(),
        );
        q.presupposition = self.presupposition.clone();
        q.request = self.request.clone();
        q.form_hash = Some(self.hash.clone());
        q.over_kind = self.over_kind;
        q.permute = self.permute;
        q.template = Some(self.template.clone());
        // B58（步 17b）：模板的 taint 带到题上；填入值的 taint 由调用处（`fill` 内置）并入
        q.taint = self.taint;
        q.fill = Some(
            self.slots
                .iter()
                .filter_map(|s| fill.iter().find(|(k, _)| k == s).cloned())
                .collect(),
        );
        Ok(q)
    }
}

/// 状态 = 具名槽（§2.1）。`on` 恰一个对象（或一对），`over` 是候选。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct State {
    pub on: Vec<Mat>,
    pub ctx: Vec<Mat>,
    pub r#ref: Vec<Mat>,
    pub over: Vec<Mat>,
    pub taint: Taint,
    pub derived_from: BTreeSet<String>,
    /// 含槽结构的规范化 JSON 的哈希（§2.10）
    pub hash: String,
    pub has_fail: bool,
    /// 来源读数的账本键（B59，步 17a）：各槽材料 `from_key` 的并，加上由元素记录构造状态时的元素出口。
    /// **不进 `hash`**（`StateHash` 不变，旧账本重放不受影响），空集不序列化。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub parents: BTreeSet<String>,
}

impl State {
    pub fn new(
        on: Vec<Mat>,
        ctx: Vec<Mat>,
        r#ref: Vec<Mat>,
        over: Vec<Mat>,
        has_fail: bool,
    ) -> State {
        let all = on.iter().chain(&ctx).chain(&r#ref).chain(&over);
        let mut taint = Taint::Trusted;
        let mut derived = BTreeSet::new();
        let mut parents = BTreeSet::new();
        for m in all {
            taint = Taint::join(taint, m.taint);
            derived.extend(m.derived_from.iter().cloned());
            parents.extend(m.from_key.iter().cloned());
        }
        let mut s = State {
            on,
            ctx,
            r#ref,
            over,
            taint,
            derived_from: derived,
            hash: String::new(),
            has_fail,
            parents,
        };
        s.hash = hash_of(&["state", &canon(&s.to_json())]);
        s
    }
    /// 并入来源读数的账本键（B59）。空键不记。不改哈希。
    pub fn with_parents<I: IntoIterator<Item = String>>(mut self, keys: I) -> State {
        self.parents
            .extend(keys.into_iter().filter(|k| !k.is_empty()));
        self
    }
    /// 状态的槽形（B76，步 12e-2），供精化题类。`over` 全是字面材料 → 标签，否则 → 计算材料；
    /// 题值渲染成材料（B4）今天没有构造，`question_material` 恒假。
    pub fn slot_shape(&self) -> SlotShape {
        SlotShape {
            on: if self.on.len() == 2 {
                OnShape::Pair
            } else {
                OnShape::One
            },
            over: if self.over.is_empty() {
                OverShape::Empty
            } else if self
                .over
                .iter()
                .all(|m| m.origin.iter().all(|o| o == "literal"))
            {
                OverShape::Labels
            } else {
                OverShape::Materials
            },
            question_material: false,
        }
    }
    /// **夹具形状**的状态 JSON：`on` 恒为列表。
    ///
    /// 与 [`State::to_json`] 的差别只有一处：那个在 `on` 只有一个对象时**摊成对象**
    /// （因为送给模型时那样更自然），**而夹具只收列表**
    /// （`invalid type: map, expected a sequence`）。
    /// 未命中报文说「照抄这两份 JSON」，**打 `to_json` 那句话就是假的**。
    pub fn as_fixture_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert("on".into(), m(&self.on));
        for (k, v) in [
            ("ctx", &self.ctx),
            ("ref", &self.r#ref),
            ("over", &self.over),
        ] {
            if !v.is_empty() {
                o.insert(k.into(), m(v));
            }
        }
        Json::Object(o)
    }

    /// 被判断对象（`on` 槽）的文本，供认证范围的材料指纹用（B68）：字符串材料取原文，
    /// 其余取紧凑 JSON；多份材料以换行相连。
    pub fn on_text(&self) -> String {
        self.on
            .iter()
            .map(|m| match &m.content {
                Json::String(t) => t.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 送给模型的状态 JSON（H1：题面按路径引用槽）。
    pub fn to_json(&self) -> Json {
        let m = |v: &Vec<Mat>| Json::Array(v.iter().map(|x| x.content.clone()).collect());
        let mut o = serde_json::Map::new();
        o.insert(
            "on".into(),
            if self.on.len() == 1 {
                self.on[0].content.clone()
            } else {
                m(&self.on)
            },
        );
        if !self.ctx.is_empty() {
            o.insert("ctx".into(), m(&self.ctx));
        }
        if !self.r#ref.is_empty() {
            o.insert("ref".into(), m(&self.r#ref));
        }
        if !self.over.is_empty() {
            o.insert("over".into(), m(&self.over));
        }
        Json::Object(o)
    }
}

/// 模型对一题的回答（校验后的规范形）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Answer {
    Noul(f64),
    /// 按 over 下标的概率
    Choice(Vec<f64>),
    /// 按档位下标的概率
    Score(Vec<f64>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub q_hash: String,
    pub state_hash: String,
    pub op: Op,
    pub calib: String,
    /// 读数句柄（步 11b-3）：答案不在读数上，在持有者的答案表里（运行时的私有读数表；
    /// 宿主与测试用 [`AnswerTable`]）。同一次运行内唯一，由登记处分配。
    #[serde(default)]
    pub id: u64,
    /// 状态含 Fail 材料时读数为空（J-12 → Unsure(fail)）
    pub fail: Option<String>,
    pub model_id: String,
    pub ledger_key: String,
    pub over_len: usize,
    pub scale: Vec<String>,
    /// 用了几个置换。**与 `mode_share` 成对**——K 是那个测量身份的一部分。
    #[serde(default)]
    pub perms: std::cell::Cell<usize>,
    /// 置换众数占比（`12`:151）。`None` = 这条路上没测过置换 → `cut` 不给 `Pick`。
    /// 与 `answer` 同样是刷新时才填上的，所以是 `Cell`。
    #[serde(default, skip)]
    pub mode_share: std::cell::Cell<Option<f64>>,
    /// 这道题声明了、而状态里没有的决定性证据槽（J-09）。`cut` 判序第一步据此给
    /// `Unsure(insufficient)`——**在看 p 之前**。登记时就算好，因为那时状态还在手上。
    #[serde(default)]
    pub missing_evidence: Vec<String>,
    /// 被判断的那个状态的 taint。`cut` 据此给出口定 taint
    /// （`12`:150「出口 taint 继承状态 taint」、§2.11「cut 继承」）。
    /// 以前没有这个字段，`cut` 只好一律给 `Trusted`——**状态算好的 taint 被丢掉了**。
    /// 步 17b（B58）起是「状态 ∨ 题面」：判断器读到的题面与材料同样计入，字段名沿用。
    #[serde(default)]
    pub state_taint: Taint,
    /// 题来自哪个题式（件 b 的 `form_hash`）。`cut` 在题键没有上岗记录时据此退到题式键
    /// （B2 待裁，本版只作回退层）。不是由题式填出的题为 `None`。
    #[serde(default)]
    pub form_hash: Option<String>,
    /// 被判断材料（`on` 槽）的指纹（B68）。登记时算好——那时状态还在手上；`cut` 据此核认证范围。
    /// 由材料推得出，不进序列化；`fit`、`repeat` 合成的读数为 `None`（不核范围）。
    #[serde(default, skip)]
    pub fp: Option<[f64; 7]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExitKind {
    Act,
    Ignore,
    Pick(usize),
    At(usize),
    Unsure(String),
}

/// 未解析的出口（B94，步 23c）。出口号在 `cut` 时分配（与改前同序），解析出的出口挂回登记它的那一帧。
#[derive(Debug)]
pub struct PendingCut {
    pub reading: Rc<Reading>,
    /// `cut` 的第二位：校准键（Text）
    pub calib: Option<String>,
    /// `cut` 的代价记录 `{cost: [fp, fn]}`（B29）
    pub cost: Option<(f64, f64)>,
    pub site: Span,
    /// `cut` 时分配的出口号
    pub id: usize,
    /// 登记它的帧（运行时帧栈的下标）
    pub frame: usize,
    pub resolved: RefCell<Option<Rc<Exit>>>,
}

impl PendingCut {
    pub fn exit(&self) -> Option<Rc<Exit>> {
        self.resolved.borrow().clone()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Exit {
    pub id: usize,
    /// 出口来自哪种题（handle 的穷尽分支按它定）
    pub op: Op,
    pub kind: ExitKind,
    pub q_hash: String,
    pub state_hash: String,
    pub taint: Taint,
    pub site: Span,
    /// 这个出口是不是经 `ask` 来的（J-08「或经 `ask`」；人答是 trusted，§2.11）
    pub from_ask: Cell<bool>,
    pub consumed: Cell<bool>,
    pub consumed_by: RefCell<String>,
    /// **这条线是哪一级的**：`""` = 没用上线（冷 / 停岗 / fail）、`"题级"` = 这道题自己的、
    /// `"模式级"` = 借的同类先验（`12`:136）。
    ///
    /// **它必须对 handler 可见**，理由与 J-15 那一位同：**模式级的线不能冒充题级的线**。
    /// 看不见来源，handler 就只能把「这道题测过 200 条」和「这类题测过 200 条、这道题
    /// 一条没有」当同一件事办——**那是把两种证据强度压平**。
    pub line_source: String,
    /// **J-15 的那一位**：本次路径上有没有一个**被声明为判据、却没有被测量的量**。
    /// `None` = 该出口引用的判据都测过；`Some(载体)` = 那个量没测过（载体只作诊断）。
    ///
    /// **它不是 `cause`，这是要点。** `cause` 的本分是**路由键**（§5 handler 库按它分流）。
    /// 把「测没测过」塞进 `cause` 会得到 `cold` / `no_perm` / `no_ece` / `no_klimit`……
    /// 而 **`cold` 的存在正是证据：那条路已经走过一次，然后停了**。每多一个 `cause`，
    /// handler 库就多一条要记得加的路由，**而漏加路由的失效方式是静默的**——`otherwise`
    /// 兜住，没人知道。所以它是**正交的一位**：跟着出口走，`cause` 不动。
    ///
    /// 判别法（`12` §2.11）：这个区别是「这一格特有的」还是「会在很多格上重复出现的」？
    /// 今天已经有五个载体同处「没测过」：线未测（`cold`）、置换未测（`mode_share == None`）、
    /// 档案字段未测（`choice_same_call_perm_crosstalk`）、`k_limit` 120–250 档未测、
    /// ECE 未过检。重复出现 → 加维度，不加 `cause`。
    pub untested: Option<String>,
    /// 这个出口来自哪一条账本记录（`cut` 时写入；其余出口为空）。
    /// 组合封闭性契约的证据只存这个键，不存读数或材料的副本（B17 不变量 3）。
    pub ledger_key: RefCell<String>,
    /// **这个出口的线等级**（步 20a-1，`LineGrade`，`20` v2 §3.4）：`cut` 出口一律有值（没用上线即
    /// `Cold`）；`None` = 出口不来自 `cut`（`ask` 与构造派生的出口），没有「线」可谈，放行由它自己的
    /// 来源决定（taint、`from_ask`）。取代原来的 `fixture_line`（B29）、`class_line`（B75）、
    /// `trial_line`（B72）三个布尔位；等级派生读有效 α（B89，见 `jpp-calib::cert_view`）。
    pub grade: Cell<Option<LineGrade>>,
    /// **这条线是停岗候选**（B25）：正交位，可与任何等级叠加；出口照常路由，不放行不可逆 `do`。
    pub suspend_candidate: Cell<bool>,
    /// **材料在这条线的认证范围之外**（B68）：正交位（补遗 12(a)：范围外不是等级，是这次使用失去了保证）；
    /// 出口照常路由，不放行不可逆 `do`。
    pub scope_out: Cell<bool>,
    /// **线的证书没有记录认证带宽**（B104-1：按 δ 平移过的旧证书，`selection` 在而 `selection.delta` 缺）：
    /// 出口照常路由，不算放行不可逆 `do` 的可信合取项，直到 `load`（步 20c）重跑写回 δ 或重新导入。
    pub delta_unknown: Cell<bool>,
    /// **线的认证范围未知**（B104-2：记录没有材料指纹）：出口照常路由，不算放行不可逆 `do` 的可信合取项，
    /// 直到带文本重新导入或经 B91 扩展并入。
    pub scope_unknown: Cell<bool>,
    /// **合成出口的分量**（B131，步 25-2b）：由内核合成构造 `compose` 从这些出口按封闭规则派生，是合成出口的
    /// 谱系入口；非合成出口为空。不序列化。放行合取与谱系穿过它在步 25-9 落（此前合成出口一律按冷线不放行，25-1）。
    pub parts: RefCell<Vec<Rc<Exit>>>,
}

impl Exit {
    /// **放行的唯一判定点**（步 20a-1；补遗 12(a)）：
    /// `releases() = grade ∈ {Certified, Form} ∧ ¬scope_out ∧ ¬suspend_candidate ∧ untested = None
    /// ∧ ¬delta_unknown ∧ ¬scope_unknown`。
    ///
    /// 不看 taint：taint 是材料的属性，由 [`Exit::guard_trusted`] 合取。报告 `exits` 表的 `releases`
    /// 就是这个函数的值。`grade` 为 `None` 的出口（不来自 `cut`）没有线，等级一项不适用，只看正交位。
    pub fn releases(&self) -> bool {
        self.grade.get().is_none_or(LineGrade::releases)
            && !self.scope_out.get()
            && !self.suspend_candidate.get()
            // J-15：本次路径上有未测的判据，不放行（步 20a-1 起；此前不查）
            && self.untested.is_none()
            // B104：认证带宽未记录、认证范围未知都不放行
            && !self.delta_unknown.get()
            && !self.scope_unknown.get()
    }
    /// J-08 的可信合取项：出口已决，状态可信，且 [`Exit::releases`]。
    ///
    /// 未决出口不是放行判定：unsure 臂拿到的是责任，不是判定，不论 taint 与等级（B121-2，
    /// 步 16-0）。此前这里不看出口种类，正式线上的 `Unsure(band)` 也 `releases()`，于是可信
    /// 材料上 unsure 臂里的不可逆 `do` 被放行（`tests/bypass_j08_unsure_arm.rs`）。
    /// 依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md §二）
    pub fn guard_trusted(&self) -> bool {
        !self.is_unsure() && self.taint == Taint::Trusted && self.releases()
    }
    pub fn is_unsure(&self) -> bool {
        matches!(self.kind, ExitKind::Unsure(_))
    }
    /// 这个出口引用的判据里没被测量的那个量（J-15 的那一位）。**读取不转移责任。**
    /// 与 `cause()` 正交：`cause()` 回答「往哪条路由走」，这个回答「那条路上的判据测没测过」。
    pub fn untested(&self) -> Option<&str> {
        self.untested.as_deref()
    }
    /// 未决的原因（`band` / `cold` / `tie` / `untested` / `fail:…`）。**它是路由键**
    /// （§5 handler 库按它分流），不承载「测没测过」——那一位见 [`Exit::untested`]。
    /// 读取不转移责任。
    pub fn cause(&self) -> String {
        match &self.kind {
            ExitKind::Unsure(c) => c.clone(),
            other => format!("{other:?}"),
        }
    }
    pub fn label(&self) -> String {
        match &self.kind {
            ExitKind::Act => "act".into(),
            ExitKind::Ignore => "ignore".into(),
            ExitKind::Pick(k) => format!("pick({k})"),
            ExitKind::At(l) => format!("at({l})"),
            // 那一位进 label，因为 `exit_kind(e)` 是程序**自己带进返回值**的审计面
            // （出口不进 `Ledger`——那里只有 Judge/Effect/Ask 三种条目）。
            ExitKind::Unsure(c) => match &self.untested {
                // 通用 cause `untested`（没有既有路由可骑的那一类）：载体直接跟在后面
                Some(carrier) if c == "untested" => format!("unsure(untested:{carrier})"),
                // 骑既有路由的那一类（如 `cold`）：**路由键不动**，那一位挂在后面
                Some(carrier) => format!("unsure({c}|untested:{carrier})"),
                None => format!("unsure({c})"),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub cause: String,
    pub key: String,
    pub site: Span,
    pub detail: String,
}

/// 显式环境链：名字→值，可打印。
#[derive(Debug)]
pub struct EnvNode {
    pub vars: RefCell<Vec<(String, Value)>>,
    pub parent: Option<Env>,
}
pub type Env = Rc<EnvNode>;

pub fn env_root() -> Env {
    Rc::new(EnvNode {
        vars: RefCell::new(vec![]),
        parent: None,
    })
}
pub fn env_child(parent: &Env) -> Env {
    Rc::new(EnvNode {
        vars: RefCell::new(vec![]),
        parent: Some(parent.clone()),
    })
}
impl Reading {
    pub fn set_mode_share(&self, ms: f64, perms: usize) {
        self.mode_share.set(Some(ms));
        self.perms.set(perms);
    }
}

/// 答案源：持有答案表的一方实现它（步 11b-3，`20` §2.3「答案进运行时私有读数表」）。
///
/// **判据（比清单更可靠）：凡结果依赖于答案的操作，都是刷新点**——读答案之前必须先 `flush`。
/// `12` §2.2:129 列了 `cut`/`fit`/`match`/`if`/读内容五项，但清单形式会漏：每加一个读答案的操作
/// 就要记得补一项，而**漏补的失效方式是静默给出错误结果**（`allocate` 漏了刷新时选出 `[0,1,2,3]`
/// 而不是 `[2,4,6,8]`，不报错也不告警）。运行时的实现是 `readings.rs::answer_of`，唯一入口。
pub trait Answers {
    /// 已经填上的答案；`None` = 还没刷新（或失败）。
    fn answer_of(&self, r: &Reading) -> Option<Answer>;
}

/// 按读数句柄存答案的表。运行时持一张私有的；宿主与测试（`strength` 的纯函数入口）用它自带答案。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerTable(std::collections::BTreeMap<u64, Answer>);

impl AnswerTable {
    pub fn new() -> AnswerTable {
        AnswerTable::default()
    }
    /// 记下一个读数的答案（同一句柄再写即覆盖）
    pub fn insert(&mut self, r: &Reading, a: Answer) {
        self.0.insert(r.id, a);
    }
}

impl Answers for AnswerTable {
    fn answer_of(&self, r: &Reading) -> Option<Answer> {
        self.0.get(&r.id).cloned()
    }
}

pub fn env_lookup(env: &Env, name: &str) -> Option<Value> {
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        if let Some((_, v)) = e.vars.borrow().iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        cur = e.parent.clone();
    }
    None
}
pub fn env_define(env: &Env, name: &str, v: Value) {
    env.vars.borrow_mut().push((name.to_string(), v));
}
/// 环境的可读形式（只列名字，避免打印巨大值；给 INTERFACE 的「函数值环境表示」）
pub fn env_names(env: &Env) -> Vec<Vec<String>> {
    let mut out = vec![];
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        out.push(e.vars.borrow().iter().map(|(n, _)| n.clone()).collect());
        cur = e.parent.clone();
    }
    out
}

#[derive(Debug)]
pub struct Closure {
    pub function: Function,
    pub env: Env,
    pub name: Option<String>,
    pub span: Span,
    /// 函数体的结构哈希（transform 的键用）
    pub hash: String,
    /// 创建时经捕获环境可达的未销账未决责任（出口 id；B52，步 21）。只作运行期记账，
    /// 不进 `hash`、不序列化。
    pub captures: Vec<usize>,
    /// Fn¹（B52）：本闭包是其中每条责任的**唯一可达路径**，由创建它的帧返回时判定；非空即 Fn¹。
    pub linear: RefCell<Vec<usize>>,
    /// 判为 Fn¹ 之后是否已被调用过一次（第二次调用是 J-05）
    pub linear_called: Cell<bool>,
}

#[derive(Clone, Debug)]
pub enum Value {
    Unit,
    /// 宿主标量各带一位 taint（B33，`12` §2.11「宿主内传播」）：含义与材料相同，
    /// trusted = 有人为其内容担保。语法字面量求值即 trusted；从 untrusted 材料读出的叶子
    /// 为 untrusted；内置与运算符的输出取 ∨ 输入；容器不带位，由 `taint_of` 递归取 ∨。
    /// 步 17c（B84）起第二字段是来源标签 `Provenance = (taint, sources)`，taint 分量的规则同 B33。
    Int(i64, Provenance),
    Float(f64, Provenance),
    /// 第三字段是守卫证据（J-08，`20` v2 §3.1；步 16），不参与相等与序列化。
    Bool(bool, Provenance, GuardEv),
    Text(Rc<str>, Provenance),
    List(Rc<Vec<Value>>),
    Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>),
    Builtin(&'static str),
    Mat(Rc<Mat>),
    State(Rc<State>),
    Question(Rc<Question>),
    /// 题式（带槽的题模板）
    Form(Rc<Form>),
    Reading(Rc<Reading>),
    Exit(Rc<Exit>),
    /// 惰性过桥（B94，步 23c）：`cut` 只把读数与线绑定，出口在第一次被检视时才解析。
    /// 运行时在检视点（内置与构造的实参、`if` 条件、运算、取字段、函数与程序返回）把它换成 `Exit`；
    /// 解析一次、缓存出口，复制出去的各份共享同一个出口（同一份责任）。
    Cut(Rc<PendingCut>),
    /// 未决责任 `U(q)`：`handle` 的 unsure 臂收到的就是它。不可伪造（只能由 handle 交付）、
    /// 不能默默变成材料或 JSON 就算销账。与出口共享同一个 `Rc<Exit>`，销账记录是同一份。
    Duty(Rc<Exit>),
    /// `do` 的失败值（J-12）。**失败信息也是外部世界的输出**：它的 taint 取产生它的动作的
    /// 输出 taint（`fail()` 由程序自己写，trusted）。此前没有这一位，装进材料时一律 trusted，
    /// 不可信动作的失败值因此能放行不可逆 do（K-182 / K-203，2026-09-23 修）。
    Fail(Rc<str>, Provenance),
    /// `stop(v)`：有界循环的显式停止
    Stop(Rc<Value>),
}

impl Value {
    /// 可信文本（程序自己造的）
    pub fn text(s: &str) -> Value {
        Value::Text(Rc::from(s), Provenance::trusted())
    }
    pub fn int(i: i64) -> Value {
        Value::Int(i, Provenance::trusted())
    }
    pub fn float(f: f64) -> Value {
        Value::Float(f, Provenance::trusted())
    }
    pub fn bool(b: bool) -> Value {
        Value::Bool(b, Provenance::trusted(), GuardEv::EMPTY)
    }
    /// 这个值携带的来源标签（B84）：标量取自身；容器递归 join；材料取 `(taint, from_key)`；
    /// 出口取 `(taint, {账本键})`；题取 `(trusted, from_key)`；其余 `(trusted, ∅)`。
    /// taint 分量与 [`Value::taint`] 逐值相同（B33 第 1 点）。
    pub fn prov(&self) -> Provenance {
        match self {
            Value::Int(_, p) | Value::Float(_, p) | Value::Bool(_, p, _) | Value::Text(_, p) => {
                p.clone()
            }
            Value::Fail(_, p) => p.clone(),
            Value::Mat(m) => m.prov(),
            Value::List(l) => l
                .iter()
                .fold(Provenance::trusted(), |a, x| prov_join(&a, &x.prov())),
            Value::Record(fs) => fs
                .iter()
                .fold(Provenance::trusted(), |a, (_, x)| prov_join(&a, &x.prov())),
            // B92：出口读出是值依赖边
            Value::Exit(e) => {
                Provenance::new(e.taint, Sources::value(&e.ledger_key.borrow(), &e.q_hash))
            }
            // 未解析的出口（B94）：解析后取出口的；解析前取读数的 taint 与同一条值依赖边
            Value::Cut(c) => match c.exit() {
                Some(e) => Value::Exit(e).prov(),
                None => Provenance::new(
                    c.reading.state_taint,
                    Sources::value(&c.reading.ledger_key, &c.reading.q_hash),
                ),
            },
            // B58（步 17b）：题带题面 taint
            Value::Question(q) => Provenance::new(q.taint, Sources::from_set(q.from_key.clone())),
            Value::Form(f) => Provenance::from(f.taint),
            Value::Stop(x) => x.prov(),
            _ => Provenance::trusted(),
        }
    }
    /// 这个值自带的守卫证据：`Bool` 取自身，其他为空（B121-1）。
    pub fn guard_ev(&self) -> GuardEv {
        match self {
            Value::Bool(_, _, g) => *g,
            _ => GuardEv::EMPTY,
        }
    }
    /// 把守卫证据并进值里每个 `Bool` 叶子（递归 `List`、`Record`，其他原样）。
    /// 只由 `handle` 分派调用：臂返回值带分派出口的证据（B121-1）。
    pub fn stamp(self, ev: GuardEv) -> Value {
        if ev.is_empty() {
            return self;
        }
        match self {
            Value::Bool(b, p, g) => Value::Bool(b, p, g.join(ev)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.stamp(ev)).collect()),
            Value::Record(r) => {
                Value::record(r.iter().cloned().map(|(k, v)| (k, v.stamp(ev))).collect())
            }
            other => other,
        }
    }
    /// 把标签 join 进值（B33 `tainted` 的推广）。taint 分量进标量叶子与题、题式的题面 taint（B58，步 17b；
    /// 材料的位不动）；sources 分量进标量叶子、材料的 `from_key`、题的 `from_key`。单位元原样返回。
    pub fn with_prov(self, p: &Provenance) -> Value {
        if p.is_unit() {
            return self;
        }
        match self {
            Value::Int(i, q) => Value::Int(i, prov_join(&q, p)),
            Value::Float(f, q) => Value::Float(f, prov_join(&q, p)),
            Value::Bool(b, q, g) => Value::Bool(b, prov_join(&q, p), g),
            Value::Text(s, q) => Value::Text(s, prov_join(&q, p)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.with_prov(p)).collect()),
            Value::Record(r) => Value::record(
                r.iter()
                    .cloned()
                    .map(|(k, v)| (k, v.with_prov(p)))
                    .collect(),
            ),
            Value::Mat(m) if !p.sources.is_empty() => {
                Value::Mat(Rc::new((*m).clone().with_sources(&p.sources)))
            }
            Value::Question(q) => {
                let mut q = (*q).clone();
                q.from_key.extend(p.sources.iter().cloned());
                q.taint = Taint::join(q.taint, p.taint);
                Value::Question(Rc::new(q))
            }
            Value::Form(f) if p.taint == Taint::Untrusted => {
                let mut f = (*f).clone();
                f.taint = Taint::Untrusted;
                Value::Form(Rc::new(f))
            }
            other => other,
        }
    }
    /// 这个值携带的 taint：标量取自身位，容器递归取 ∨，材料 / 出口 / 失败值取其位；
    /// 其余（函数、题、状态……）是程序自己造的，按 trusted（B33 第 1 点）。
    pub fn taint(&self) -> Taint {
        match self {
            Value::Int(_, t) | Value::Float(_, t) | Value::Bool(_, t, _) | Value::Text(_, t) => {
                t.taint
            }
            Value::Mat(m) => m.taint,
            Value::List(l) => l
                .iter()
                .fold(Taint::Trusted, |t, x| Taint::join(t, x.taint())),
            Value::Record(fs) => fs
                .iter()
                .fold(Taint::Trusted, |t, (_, x)| Taint::join(t, x.taint())),
            Value::Exit(e) => e.taint,
            Value::Cut(c) => c.exit().map_or(c.reading.state_taint, |e| e.taint),
            Value::Stop(x) => x.taint(),
            Value::Fail(_, t) => t.taint,
            Value::Question(q) => q.taint,
            Value::Form(f) => f.taint,
            _ => Taint::Trusted,
        }
    }
    /// 把 `t` ∨ 进所有标量叶子（容器递归）。`t` 为 trusted 时原样返回。
    /// 用于读出规则（从 untrusted 材料读出的全部叶子标 untrusted）与显式数据流（输出 ∨ 输入）。
    pub fn tainted(self, t: Taint) -> Value {
        if t == Taint::Trusted {
            return self;
        }
        // B84：只改 taint 分量，sources 保留
        let set = |q: Provenance| Provenance::new(t, q.sources);
        match self {
            Value::Int(i, q) => Value::Int(i, set(q)),
            Value::Float(f, q) => Value::Float(f, set(q)),
            Value::Bool(b, q, g) => Value::Bool(b, set(q), g),
            Value::Text(s, q) => Value::Text(s, set(q)),
            Value::List(l) => Value::list(l.iter().cloned().map(|x| x.tainted(t)).collect()),
            Value::Record(r) => {
                Value::record(r.iter().cloned().map(|(k, v)| (k, v.tainted(t))).collect())
            }
            other => other,
        }
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(v))
    }
    pub fn record(v: Vec<(String, Value)>) -> Value {
        Value::Record(Rc::new(v))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Int(_, _) => "Int",
            Value::Float(_, _) => "Float",
            Value::Bool(_, _, _) => "Bool",
            Value::Text(_, _) => "Text",
            Value::List(_) => "List",
            Value::Record(_) => "Record",
            Value::Fn(_) => "Fn",
            Value::Builtin(_) => "Builtin",
            Value::Mat(_) => "Mat",
            Value::State(_) => "State",
            Value::Question(_) => "Question",
            Value::Form(_) => "Form",
            Value::Reading(_) => "Reading",
            Value::Exit(_) | Value::Cut(_) => "Exit",
            Value::Duty(_) => "Unsure",
            Value::Fail(..) => "Fail",
            Value::Stop(_) => "Stop",
        }
    }
    pub fn get(&self, key: &str) -> Option<Value> {
        match self {
            Value::Record(r) => r.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
            _ => None,
        }
    }
    /// 可打印 / 可序列化形式。读数只露元数据不露概率（读数不能被宿主当数用，J-01）。
    pub fn to_json(&self) -> Json {
        match self {
            Value::Unit => Json::Null,
            Value::Int(i, _) => json!(i),
            Value::Float(f, _) => json!(f),
            Value::Bool(b, _, _) => json!(b),
            Value::Text(s, _) => json!(s.as_ref()),
            Value::List(l) => Json::Array(l.iter().map(|v| v.to_json()).collect()),
            Value::Record(r) => {
                let mut m = serde_json::Map::new();
                for (k, v) in r.iter() {
                    m.insert(k.clone(), v.to_json());
                }
                Json::Object(m)
            }
            Value::Fn(c) => {
                json!({"fn": c.name, "params": c.function.parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "env": env_names(&c.env), "hash": c.hash})
            }
            Value::Builtin(n) => json!({"builtin": n}),
            Value::Mat(m) => {
                json!({"mat": m.hash, "content": m.content, "taint": m.taint, "origin": m.origin})
            }
            Value::State(s) => json!({"state": s.hash, "slots": s.to_json(), "taint": s.taint}),
            Value::Question(q) => {
                json!({"question": q.hash, "op": q.op.phys(), "text": q.text, "calib": q.calib})
            }
            Value::Form(f) => {
                json!({"form": f.hash, "op": f.op.phys(), "template": f.template, "slots": f.slots, "calib": f.calib})
            }
            Value::Reading(r) => {
                json!({"reading": r.ledger_key, "q": r.q_hash, "state": r.state_hash, "op": r.op.phys()})
            }
            Value::Exit(e) => {
                json!({"exit": e.label(), "id": e.id, "consumed": e.consumed.get(), "q": e.q_hash})
            }
            Value::Cut(c) => match c.exit() {
                Some(e) => Value::Exit(e).to_json(),
                None => json!({"exit": "unresolved", "id": c.id, "q": c.reading.q_hash}),
            },
            Value::Duty(e) => json!({"unsure": e.cause(), "duty": e.id, "q": e.q_hash}),
            Value::Fail(s, _) => json!({"fail": s.as_ref()}),
            Value::Stop(v) => json!({"stop": v.to_json()}),
        }
    }
    /// 这个值可比吗；`None` = 里面有读数（J-01）。容器要递归看，装进列表或记录不改变这件事。
    pub fn comparable(&self) -> Option<()> {
        match self {
            Value::Reading(_) => None,
            Value::List(l) => l.iter().try_for_each(|x| x.comparable()),
            Value::Record(fs) => fs.iter().try_for_each(|(_, v)| v.comparable()),
            _ => Some(()),
        }
    }

    /// 结构相等（函数按哈希）；读数不可比（返回 None，调用方报 J-01）
    pub fn equals(&self, other: &Value) -> Option<bool> {
        Some(match (self, other) {
            (Value::Reading(_), _) | (_, Value::Reading(_)) => return None,
            (Value::Unit, Value::Unit) => true,
            (Value::Int(a, _), Value::Int(b, _)) => a == b,
            (Value::Float(a, _), Value::Float(b, _)) => a == b,
            (Value::Int(a, _), Value::Float(b, _)) | (Value::Float(b, _), Value::Int(a, _)) => {
                (*a as f64) == *b
            }
            (Value::Bool(a, _, _), Value::Bool(b, _, _)) => a == b,
            (Value::Text(a, _), Value::Text(b, _)) => a == b,
            // 容器里的「不可比」要传上来，不能被 `== Some(true)` 悄悄吃成 false：
            // 读数装进列表或记录还是读数，没有可读的值（J-01）。
            (Value::List(a), Value::List(b)) => {
                if a.iter().chain(b.iter()).any(|x| x.comparable().is_none()) {
                    return None;
                }
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.equals(y) == Some(true))
            }
            (Value::Record(a), Value::Record(b)) => {
                if a.iter()
                    .chain(b.iter())
                    .any(|(_, v)| v.comparable().is_none())
                {
                    return None;
                }
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter()
                            .any(|(k2, v2)| k == k2 && v.equals(v2) == Some(true))
                    })
            }
            (Value::Mat(a), Value::Mat(b)) => a.hash == b.hash,
            (Value::State(a), Value::State(b)) => a.hash == b.hash,
            (Value::Question(a), Value::Question(b)) => a.hash == b.hash,
            (Value::Form(a), Value::Form(b)) => a.hash == b.hash,
            (Value::Exit(a), Value::Exit(b)) => a.kind == b.kind,
            // 运行时在运算前已把未解析出口解析掉；这里只剩已解析的
            (Value::Cut(a), _) => Value::Exit(a.exit()?).equals(other)?,
            (_, Value::Cut(b)) => self.equals(&Value::Exit(b.exit()?))?,
            // 责任按身份比：同一道题的两个未决是两份责任
            (Value::Duty(a), Value::Duty(b)) => a.id == b.id,
            (Value::Fn(a), Value::Fn(b)) => a.hash == b.hash,
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            (Value::Fail(a, _), Value::Fail(b, _)) => a == b,
            _ => false,
        })
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;

    #[test]
    fn 填法必须恰好填满槽_同题面即同一道题() {
        let f = Form::new(
            Op::Test,
            "{a} 是否早于 {b}？",
            "k",
            vec![],
            vec![],
            None,
            None,
        )
        .unwrap();
        assert_eq!(f.slots, vec!["a", "b"]);
        let fill = |xs: &[(&str, &str)]| {
            f.fill(
                &xs.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>(),
            )
        };
        assert!(fill(&[("a", "周一")]).unwrap_err().contains("槽 b 没有填"));
        assert!(
            fill(&[("a", "周一"), ("b", "周二"), ("c", "x")])
                .unwrap_err()
                .contains("没有槽 c")
        );
        let q = fill(&[("b", "周二"), ("a", "周一")]).unwrap();
        assert_eq!(q.text, "周一 是否早于 周二？");
        assert_eq!(q.fill.as_ref().unwrap()[0].0, "a");
        // 题式来源不进题哈希：同题面同题型就是同一道题（账本键与校准键不因写法不同而分裂）
        assert_eq!(
            q.hash,
            Question::new(Op::Test, "周一 是否早于 周二？", "k", vec![]).hash
        );
        assert!(Form::slots_of("未闭合 {a").is_err());
    }
}
