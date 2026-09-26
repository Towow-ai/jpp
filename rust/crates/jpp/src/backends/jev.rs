//! 真机 JEV 客户端（`live` 特性开 HTTP）。（步 4c 自 `effects.rs` 原样搬出）

use std::collections::HashMap;

use serde_json::{Value as Json, json};

use crate::effects::*;
use crate::value::{Answer, Op, Question, State};
use jpp_effects::builtin_ports::{UnansweredPort, UnservedPort};
use jpp_effects::port::{CallInput, Completed, EffectCall, EffectOut, EffectPort, Ports, Ticket};

/// 真机客户端：与 Python `foundation/clients/eye_client.py` 同协议（POST /v1/systemone）。
/// 密钥只从 `~/.typesafe-key` 读，不写进任何文件或日志。本包不用它发调用；`live` 特性开启 HTTP。
pub struct JevClient {
    /// 发不发置换（`12`:151 的 `Pick` 判据要它）。**默认 false**：它让 select 的调用数 ×2。
    /// `K=2` 且是**正序与逆序**——那是对首位偏置的**极大对抗对**，不是「图便宜的 2」：
    /// 若存在位置偏置，正逆两序最容易让它现形；两个随机置换反而可能都没碰到那个位置。
    pub permute: bool,
    pub model: String,
    /// 每个 input token 的价格，**只从画像读**（`cost.price_usd_per_input_token`；B73、`19` 冲突 #25）。
    /// `None` = 画像没给价格：实际费用未知，这里按 0 计入预算（B42 不拒），由宿主报 `W-cost-unknown`。
    pub usd_per_input_token: Option<f64>,
    pub n_calls: u64,
    pub transport: Transport,
    /// 可跨线程调用的同一传输（步 15e）：有它，[`JevPort`] 才能在端口内并发发出一批请求体；
    /// 没有（测试注入的 `FnMut` 传输）就逐个发。
    pub shared: Option<Attempt>,
}

impl JevClient {
    pub fn with_transport(model: &str, transport: Transport) -> JevClient {
        JevClient {
            permute: false,
            model: model.to_string(),
            usd_per_input_token: None,
            n_calls: 0,
            transport,
            shared: None,
        }
    }
    /// 可跨线程的传输（步 15e）：串行路径与端口内并发用同一个函数。
    pub fn with_shared_transport(model: &str, attempt: Attempt) -> JevClient {
        let a = attempt.clone();
        let mut c = JevClient::with_transport(model, Box::new(move |b: &Json| a(b)));
        c.shared = Some(attempt);
        c
    }
    /// 带超时的传输层（真机传输超时，依据：地基/过程记录/工程-传输超时.md）。`attempt` 是**单次**请求；
    /// 给了 `timeout` 就在工作线程里跑它、主线程最多等 `timeout`，到时返回 `E-timeout` 错误，工作线程
    /// 留给请求自己结束。超时不在这里重试：它是 absent（`12` B35），交给 `budget.absent` 的逐次计费重试。
    /// `timeout == None`（画像没给 `transport.timeout_s`）直接调用，行为同 [`JevClient::with_transport`]。
    pub fn with_timed_transport(
        model: &str,
        timeout: Option<std::time::Duration>,
        attempt: Attempt,
    ) -> JevClient {
        JevClient::with_transport(model, timed(timeout, attempt))
    }
    /// 价格从画像来（B73）：宿主装载画像后把 `Profile::price_per_input_token` 传进来。
    pub fn with_price(mut self, usd_per_input_token: Option<f64>) -> JevClient {
        self.usd_per_input_token = usd_per_input_token;
        self
    }
    /// `timeout`：单次请求的超时，只从画像 `transport.timeout_s` 来（宿主传入）；`None` 不设超时。
    #[cfg(feature = "live")]
    pub fn live(
        model: &str,
        usd_per_input_token: Option<f64>,
        timeout: Option<std::time::Duration>,
    ) -> Result<JevClient, EffectError> {
        let key = std::fs::read_to_string(
            std::env::var("HOME")
                .map(|h| format!("{h}/.typesafe-key"))
                .unwrap_or_default(),
        )
        .map_err(|e| EffectError(format!("读不到 ~/.typesafe-key：{e}")))?;
        let key = key.trim().to_string();
        // ureq 也设同值的连接与读超时：让被 `timed` 放弃的工作线程能自己结束
        let mut agent = ureq::AgentBuilder::new();
        if let Some(t) = timeout {
            agent = agent.timeout_connect(t).timeout_read(t);
        }
        let agent = agent.build();
        // 单次请求。状态码结果交给下面的退避循环；超时与其他传输错误原样返回
        let one = move |body: &Json| -> Result<Json, EffectError> {
            match agent
                .post("https://api.typesafe.ai/v1/systemone")
                .set("Authorization", &format!("Bearer {key}"))
                .send_json(body.clone())
            {
                Ok(resp) => resp
                    .into_json::<Json>()
                    .map_err(|e| EffectError(e.to_string())),
                Err(ureq::Error::Status(code, _)) => Err(EffectError(format!("HTTP {code}"))),
                Err(e) => Err(EffectError(e.to_string())),
            }
        };
        let one = timed_attempt(timeout, std::sync::Arc::new(one));
        let t = move |body: &Json| -> Result<Json, EffectError> {
            let mut last = String::new();
            // 429/5xx 退避重试（服务端明确拒绝，未处理、不计费）；超时不在这里重试（见 `timed`）
            for attempt in 0..5u32 {
                match one(body) {
                    Err(EffectError(m))
                        if ["HTTP 429", "HTTP 500", "HTTP 502", "HTTP 503", "HTTP 529"]
                            .contains(&m.as_str()) =>
                    {
                        last = m;
                        std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
                    }
                    r => return r,
                }
            }
            Err(EffectError(format!("Jev 调用失败：{last}")))
        };
        Ok(
            JevClient::with_shared_transport(model, std::sync::Arc::new(t))
                .with_price(usd_per_input_token),
        )
    }

    /// 一道 `select` 的候选标签（B155，步 15i）：规则在 [`State::over_labels`]（渲染 `r2` 的一部分）。
    pub fn candidate_labels(state: &State) -> Option<Vec<(String, Json)>> {
        state.over_labels()
    }

    /// 一道题这一遍发出的 `criteria` 与「发出键 → 原候选下标」（B155）。`逆序` 只对 `c{k}` 键的 `select`
    /// 起作用：标签键的对象按键名排序发出，顺序换不了（见 [`JevClient::可置换`]）。
    fn criteria(
        state: &State, q: &Question, 逆序: bool
    ) -> (Option<Json>, HashMap<String, usize>) {
        match q.op {
            Op::Select => {
                if let Some(ls) = JevClient::candidate_labels(state) {
                    let 键表 = ls
                        .iter()
                        .enumerate()
                        .map(|(k, (l, _))| (l.clone(), k))
                        .collect();
                    let crit: serde_json::Map<String, Json> = ls.into_iter().collect();
                    return (Some(Json::Object(crit)), 键表);
                }
                let n = state.over.len();
                let 序: Vec<usize> = if 逆序 {
                    (0..n).rev().collect()
                } else {
                    (0..n).collect()
                };
                let mut crit = serde_json::Map::new();
                let mut 键表 = HashMap::new();
                // 键仍是 c0..cK（按发出位），值是该位上的候选原文
                for (发出位, 原下标) in 序.into_iter().enumerate() {
                    let k = format!("c{发出位}");
                    crit.insert(k.clone(), state.over[原下标].content.clone());
                    键表.insert(k, 原下标);
                }
                (Some(Json::Object(crit)), 键表)
            }
            Op::Measure => (Some(json!(q.scale)), HashMap::new()),
            // B155：是非题带答案标签时发 `criteria: {"true": yes, "false": no}`，不带不发（B155 前的形状）
            Op::Test => (
                q.labels
                    .as_ref()
                    .map(|l| json!({"true": l.yes, "false": l.no})),
                HashMap::new(),
            ),
        }
    }

    /// 一遍的请求体（B155）：`state` 只发材料（`wire_json`，一次调用里各题材料相同，取第一道的）；
    /// 题按原题号 `q{i}` 发，每道 `select` 的候选随题走。返回请求体与各题的键表（与 `遍.题` 对齐）。
    fn body_of(
        model: &str,
        items: &[(&State, &Question)],
        遍: &一遍,
    ) -> (Json, Vec<HashMap<String, usize>>) {
        let mut qs = serde_json::Map::new();
        let mut 键表 = vec![];
        for &i in &遍.题 {
            let (state, q) = items[i];
            let mut o = serde_json::Map::new();
            o.insert("type".into(), json!(q.op.phys()));
            o.insert("instructions".into(), json!(q.text));
            let (crit, 表) = JevClient::criteria(state, q, 遍.逆序);
            if let Some(c) = crit {
                o.insert("criteria".into(), c);
            }
            qs.insert(format!("q{i}"), Json::Object(o));
            键表.push(表);
        }
        let state = items
            .first()
            .map(|(s, _)| s.wire_json())
            .unwrap_or_else(|| json!({}));
        (
            json!({"state": state, "model": model, "questions": Json::Object(qs)}),
            键表,
        )
    }

    /// 单状态的请求体（原序一遍、全部题）。B155 起 `state` 不含 `over`，候选只在 `select` 的 `criteria`。
    pub fn request_body(model: &str, state: &State, questions: &[&Question]) -> Json {
        let items: Vec<(&State, &Question)> = questions.iter().map(|q| (state, *q)).collect();
        JevClient::request_body_items(model, &items)
    }

    /// 逐题带状态的请求体（原序一遍、全部题；B155：同材料上候选集不同的题一次调用）。
    pub fn request_body_items(model: &str, items: &[(&State, &Question)]) -> Json {
        let 遍 = 一遍 {
            题: (0..items.len()).collect(),
            逆序: false,
        };
        JevClient::body_of(model, items, &遍).0
    }

    /// 返回体校验（原序一遍、全部题）：键不合即错，不静默变 Unsure（与 Python `validate_answers` 同纪律）。
    pub fn parse_answers(
        resp: &Json,
        state: &State,
        questions: &[&Question],
    ) -> Result<Vec<Answer>, EffectError> {
        let items: Vec<(&State, &Question)> = questions.iter().map(|q| (state, *q)).collect();
        let 遍 = 一遍 {
            题: (0..items.len()).collect(),
            逆序: false,
        };
        let 键表: Vec<HashMap<String, usize>> = items
            .iter()
            .map(|(s, q)| JevClient::criteria(s, q, false).1)
            .collect();
        JevClient::parse_pass(resp, &items, &遍, &键表).map(|(a, _)| a)
    }

    /// 一遍的返回体：校验、按这一遍**实际发出的键表**映回原候选下标（B155：不剥 `c` 前缀，标签名
    /// `c1` 不会被当成下标 1），连同 input token 一起返回。答案与 `遍.题` 对齐。
    fn parse_pass(
        resp: &Json,
        items: &[(&State, &Question)],
        遍: &一遍,
        键表: &[HashMap<String, usize>],
    ) -> Result<(Vec<Answer>, u64), EffectError> {
        let answers = resp
            .get("answers")
            .and_then(|a| a.as_object())
            .ok_or_else(|| EffectError("返回体缺 answers".into()))?;
        let mut out = vec![];
        for (j, &i) in 遍.题.iter().enumerate() {
            let (state, q) = items[i];
            let a = answers
                .get(&format!("q{i}"))
                .ok_or_else(|| EffectError(format!("Jev 没有回答 q{i}")))?;
            match q.op {
                Op::Test => {
                    let p = a
                        .get("noul")
                        .and_then(|v| v.as_f64())
                        .filter(|p| (0.0..=1.0).contains(p))
                        .ok_or_else(|| {
                            EffectError(format!("noul 题 q{i} 返回体要 {{noul: p}}，收到 {a}"))
                        })?;
                    out.push(Answer::Noul(p));
                }
                Op::Select => {
                    let probs = a
                        .get("probabilities")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| EffectError(format!("choice 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; state.over.len()];
                    for (k, p) in probs {
                        let idx = *键表[j].get(k).ok_or_else(|| {
                            EffectError(format!("choice 题 q{i} 的键 {k} 不是这次发出的候选键"))
                        })?;
                        v[idx] = p.as_f64().unwrap_or(0.0);
                    }
                    out.push(Answer::Choice(v));
                }
                Op::Measure => {
                    let probs = a
                        .get("probabilities")
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| EffectError(format!("score 题 q{i} 缺 probabilities")))?;
                    let mut v = vec![0.0; q.scale.len()];
                    for (k, p) in probs {
                        let idx: usize = k
                            .parse()
                            .map_err(|_| EffectError(format!("score 键 {k} 不是档位下标")))?;
                        if idx >= v.len() {
                            return Err(EffectError(format!("score 键 {k} 越界")));
                        }
                        v[idx] = p.as_f64().unwrap_or(0.0);
                    }
                    out.push(Answer::Score(v));
                }
            }
        }
        let tokens = resp
            .get("usage")
            .and_then(|u| u.get("input_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        Ok((out, tokens))
    }
}

/// 一次判断里的一遍请求（B155 起置换按题）：发哪些题（原题号）、`c{k}` 候选是否逆序。
#[derive(Clone, Debug)]
struct 一遍 {
    题: Vec<usize>,
    逆序: bool,
}

/// 单次请求：可跨线程调用，给 [`timed`] 包超时用。
pub type Attempt = std::sync::Arc<dyn Fn(&Json) -> Result<Json, EffectError> + Send + Sync>;
/// 传输层：客户端每次发请求调它一次。
pub type Transport = Box<dyn FnMut(&Json) -> Result<Json, EffectError>>;

/// 单次请求加超时（真机传输超时；依据：地基/过程记录/工程-传输超时.md）。超时即返回 `E-timeout` 错误，
/// 不重试、不兜底：之后按 `budget.absent` 路由（`12` B35：超时属 absent），重试由 `flush.rs` 逐次计费。
pub fn timed(timeout: Option<std::time::Duration>, attempt: Attempt) -> Transport {
    let a = timed_attempt(timeout, attempt);
    Box::new(move |body: &Json| a(body))
}

/// [`timed`] 的可跨线程版本（步 15e：端口内并发的工作线程各自调用它）。
pub fn timed_attempt(timeout: Option<std::time::Duration>, attempt: Attempt) -> Attempt {
    match timeout {
        None => attempt,
        Some(t) => std::sync::Arc::new(move |body: &Json| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (f, b) = (attempt.clone(), body.clone());
            std::thread::spawn(move || {
                // 主线程已超时离开时接收端已丢，发送失败无妨
                let _ = tx.send(f(&b));
            });
            match rx.recv_timeout(t) {
                Ok(r) => r,
                Err(_) => Err(EffectError(format!(
                    // 依据：地基/过程记录/工程-传输超时.md（画像 transport.timeout_s）
                    "E-timeout: 传输层 {}s 内没有返回（画像 transport.timeout_s）",
                    t.as_secs_f64()
                ))),
            }
        }),
    }
}

/// 判断的传输与合并（步 15c 起是 `JevClient` 的固有方法，经 [`JevPort`] 接成端口；旧 `Client` 已删）。
impl JevClient {
    pub fn model_id(&self) -> String {
        self.model.clone()
    }
    /// 一状态多题一次问完（P5），经 [`JevClient::judge_items`]。
    pub fn judge(
        &mut self,
        state: &State,
        questions: &[&Question],
    ) -> Result<JudgeResult, EffectError> {
        let items: Vec<(&State, &Question)> = questions.iter().map(|q| (state, *q)).collect();
        self.judge_items(&items)
    }

    /// 一次判断，逐题带状态（B155，步 15i：同材料上候选集不同的题一次调用）；声明了置换的 `select`
    /// 另发一遍逆序。
    pub fn judge_items(
        &mut self,
        items: &[(&State, &Question)],
    ) -> Result<JudgeResult, EffectError> {
        // `select` 要发**两个置换**（候选正序与逆序），按众数占比算 `mode_share`
        // ——`12`:151「`Pick` 要求置换众数一致」的**数据来源**。与 Python 同口径：
        // `runtime.py:949` 的 `perms = [正序, 逆序]`、`:1069` 的 `cnt / len(picks)`。
        //
        // 此前这里硬写 `mode_share: vec![]`，于是用真模型跑**每一道 select 都切成
        // `Unsure(tie)`**，而全绿的测试一盏灯都不亮——绿灯全是替身给的。
        // （那个 `tie` 现在已经改成 J-15 那一位；这一段留着记的是**怎么发现的**。）
        // **默认不开**（总控裁定三）：置换是两次不同的调用，不是同一状态多问一题——
        // 它实实在在让 select 的调用数 ×2。**这笔钱不能默认替作者花掉，更不能悄悄花。**
        //
        // 不开的后果是**有痕迹的失败关闭**：`mode_share == None` → J-15 那一位亮 →
        // 取保守项（不给 `Pick`）+ `W-untested`。作者看见告警，知道「这一格要更强的出口
        // 就得付这笔钱」，然后自己决定。反过来默认开的代价是：每一道 select 都悄悄花了双倍，
        // 而作者不知道自己买了什么。**兜底往拒绝那边倒**——「不给强出口」是拒绝。
        // 步 15f（B64）起，作者在 select 题或题式上声明 `{permute: true}` 即开；宿主字段 `permute`
        // 仍可整体打开（测试用）。依据：B64（地基/附注/2026-09-24-探针首轮裁定.md，I-1(b)）
        // B155（步 15i）起置换按题：只有可置换的 `select` 进第二遍，其余题的读数与它们单发时相同（I2）。
        let 各遍 = self.passes(items);
        let mut 每遍答案: Vec<Vec<Answer>> = vec![];
        let mut tokens = 0u64;
        for 遍 in &各遍 {
            let (body, 键表) = JevClient::body_of(&self.model, items, 遍);
            let resp = (self.transport)(&body)?;
            let (answers, t) = JevClient::parse_pass(&resp, items, 遍, &键表)?;
            tokens += t;
            每遍答案.push(answers);
            self.n_calls += 1;
        }
        Ok(self.merge(items, &各遍, 每遍答案, tokens))
    }

    /// 这道题可置换（B155 起按题）：要求了置换（宿主字段 `permute` 或题上声明，B64）、候选多于一个、
    /// 且候选用 `c{k}` 键。标签键的对象按键名排序发出，正逆两序是同一份请求，测出的众数占比恒为 1——
    /// 那是假的 `Pick`（放行方向），所以标签键的 `select` 不置换：`mode_share` 为空，出口按 J-15 走
    /// `untested`。依据：B155、B64
    fn 可置换(&self, state: &State, q: &Question) -> bool {
        q.op == Op::Select
            && (self.permute || q.permute)
            && state.over.len() > 1
            && JevClient::candidate_labels(state).is_none()
    }

    /// 一次判断要发的遍（步 15e 拆出；B155 起置换按题）：第一遍原序发全部题；有可置换的 `select` 时
    /// 第二遍逆序只发这些题，题号沿用第一遍。
    fn passes(&self, items: &[(&State, &Question)]) -> Vec<一遍> {
        let mut v = vec![一遍 {
            题: (0..items.len()).collect(),
            逆序: false,
        }];
        let 置换题: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, (s, q))| self.可置换(s, q))
            .map(|(i, _)| i)
            .collect();
        if !置换题.is_empty() {
            v.push(一遍 {
                题: 置换题,
                逆序: true,
            });
        }
        v
    }

    /// 各遍的答案合成一次判断的结果（步 15e 拆出；B155 起按题）：进了两遍的 `select` 取众数占比、
    /// 概率取各遍均值（与 Python `p_ = sum(...) / len(probs_all)` 同）；其余题取第一遍答案，
    /// `mode_share` 为空（没测过置换是 `None` 不是 0）。
    fn merge(
        &self,
        items: &[(&State, &Question)],
        各遍: &[一遍],
        每遍答案: Vec<Vec<Answer>>,
        tokens: u64,
    ) -> JudgeResult {
        let n = items.len();
        let mut 各题: Vec<Vec<Answer>> = vec![vec![]; n];
        for (遍, 答) in 各遍.iter().zip(每遍答案) {
            for (&i, a) in 遍.题.iter().zip(答) {
                各题[i].push(a);
            }
        }
        let mut answers = vec![];
        let mut mode_share = vec![];
        let mut perms = vec![];
        for (i, 答) in 各题.into_iter().enumerate() {
            let (state, q) = items[i];
            let 次 = 答.len();
            if q.op == Op::Select && 次 > 1 {
                let vs: Vec<&Vec<f64>> = 答
                    .iter()
                    .filter_map(|a| match a {
                        Answer::Choice(v) => Some(v),
                        _ => None,
                    })
                    .collect();
                let mut 票 = HashMap::new();
                for v in &vs {
                    *票.entry(argmax_index(v)).or_insert(0usize) += 1;
                }
                let 众数次数 = 票.values().copied().max().unwrap_or(0);
                let mut 均值 = vec![0.0; state.over.len()];
                for v in &vs {
                    for (k, x) in v.iter().enumerate() {
                        if k < 均值.len() {
                            均值[k] += x;
                        }
                    }
                }
                if !vs.is_empty() {
                    let m = vs.len() as f64;
                    均值.iter_mut().for_each(|x| *x /= m);
                }
                answers.push(Answer::Choice(均值));
                mode_share.push(Some(众数次数 as f64 / vs.len().max(1) as f64));
            } else {
                answers.push(答.into_iter().next().expect("第一遍发全部题"));
                // 没进第二遍：没有「置换」这回事，是 None 不是 0
                mode_share.push(None);
            }
            perms.push(次);
        }
        JudgeResult {
            answers,
            tokens,
            cost: self
                .usd_per_input_token
                .map(|p| tokens as f64 * p)
                .unwrap_or_default(),
            mode_share,
            perms,
            // 真机返回体里的自报置信度本版不读（B154 的读取随 18a 追加项；此处与 main 相同为空）
            confidence: vec![],
        }
    }
    /// 已发出的请求数（置换算两次）
    pub fn calls(&self) -> u64 {
        self.n_calls
    }
}

/// JEV 不生成：真机端口表里 `gen` 实例的报错（步 15c 前 `JevClient::generate` 的同一句）。
const JEV_NO_GEN: &str = "JevClient 不生成：gen 用生成器客户端或枚举器";

/// 真机判断端口（步 15b，`20` §2.3 `impl EffectPort for JevPort`）：服务产出读数的效应实例，模型是
/// `--model` 解析后的 id。本步逐个当场发出（与 `JevClient::judge` 同一段代码）；端口内并发在步 15e。
pub struct JevPort {
    client: JevClient,
    done: Completed,
    /// 端口内并发上限（步 15e）：宿主从画像 `concurrency` 取，未测为 1（`20` §3.9），1 即逐个发出
    concurrency: usize,
}

impl JevPort {
    pub fn new(client: JevClient) -> JevPort {
        JevPort {
            client,
            done: Completed::default(),
            concurrency: 1,
        }
    }
    /// 端口内并发上限（步 15e），只从画像来；0 按 1。
    pub fn with_concurrency(mut self, n: usize) -> JevPort {
        self.concurrency = n.max(1);
        self
    }
    /// 一批调用并发发出（步 15e）：主线程把每个调用的每个置换转成请求体，工作线程只碰 `Json`
    /// （值类型含 `Rc` 不跨线程，15b 记的做法）；返回体回到主线程按调用、按置换顺序解析与合并，
    /// 计数与串行同口径（该置换请求成功且解析成功才加一；一个置换失败，这个调用就报那个错）。
    fn submit_concurrent(&mut self, calls: Vec<EffectCall>, shared: Attempt) -> Vec<Ticket> {
        let client = &self.client;
        let mut 请求体: Vec<Json> = vec![];
        // 每个调用：逐题材料、各遍、各遍的键表、它的第一份请求体在 `请求体` 里的位置
        let mut 各调用: Vec<Result<待合并, EffectError>> = vec![];
        for c in calls {
            match 判断输入::from(c.input) {
                Ok(输入) => {
                    let items = 输入.items();
                    let 各遍 = client.passes(&items);
                    let 起 = 请求体.len();
                    let mut 键表 = vec![];
                    for 遍 in &各遍 {
                        let (b, 表) = JevClient::body_of(&client.model, &items, 遍);
                        请求体.push(b);
                        键表.push(表);
                    }
                    各调用.push(Ok((输入, 各遍, 键表, 起)));
                }
                Err(e) => 各调用.push(Err(e)),
            }
        }
        let mut 返回 = 并发发出(&shared, &请求体, self.concurrency)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        let mut 结果 = vec![];
        for c in 各调用 {
            结果.push(c.and_then(|(输入, 各遍, 键表, 起)| {
                let items = 输入.items();
                let mut 每遍答案 = vec![];
                let mut tokens = 0u64;
                for (k, 遍) in 各遍.iter().enumerate() {
                    let resp = 返回[起 + k]
                        .take()
                        .unwrap_or_else(|| Err(EffectError("并发发出：返回体缺失".into())))?;
                    let (answers, t) = JevClient::parse_pass(&resp, &items, 遍, &键表[k])?;
                    tokens += t;
                    每遍答案.push(answers);
                    self.client.n_calls += 1;
                }
                Ok(EffectOut::Readings(self.client.merge(
                    &items,
                    &各遍,
                    每遍答案,
                    tokens,
                )))
            }));
        }
        self.done.submit_results(结果)
    }
    /// 已发出的请求数（置换算两次）
    pub fn calls(&self) -> u64 {
        self.client.n_calls
    }
}

impl EffectPort for JevPort {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: jpp_effects::find(|s| s.produces_reading).expect("注册表里有判断"),
            model: self.client.model.clone(),
        }
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        if self.concurrency > 1
            && let Some(shared) = self.client.shared.clone()
        {
            return Ok(self.submit_concurrent(calls, shared));
        }
        let client = &mut self.client;
        Ok(self.done.submit_with(calls, |c| {
            let 输入 = 判断输入::from(c.input)?;
            client.judge_items(&输入.items()).map(EffectOut::Readings)
        }))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 真机端口表：判断走 [`JevPort`]；JEV 不生成（`gen` 实例每次报错）；没有人工通道（`ask` 实例每次
/// 「还没答」，程序以 Pending 结束）。与原 `JevClient` 的三个方法逐一对应。
pub struct JevPorts {
    pub judge: JevPort,
    rest: Vec<Box<dyn EffectPort>>,
}

/// 并发发出时一个调用等合并的材料：逐题材料、各遍、各遍的键表、首份请求体的位置（步 15e；B155 改逐题）
type 待合并 = (判断输入, Vec<一遍>, Vec<Vec<HashMap<String, usize>>>, usize);

/// 一次判断调用的逐题材料（B155，步 15i）：不同状态各存一份，题按下标指向自己的状态。
/// 两种判断输入（一个状态加一组题；同材料、逐题带状态）都转成它。
struct 判断输入 {
    states: Vec<State>,
    属: Vec<usize>,
    questions: Vec<Question>,
}

impl 判断输入 {
    fn from(input: CallInput) -> Result<判断输入, EffectError> {
        match input {
            CallInput::StateQuestions { state, questions } => Ok(判断输入 {
                属: vec![0; questions.len()],
                states: vec![state],
                questions,
            }),
            CallInput::MaterialQuestions { states, questions } => {
                let mut 各态: Vec<State> = vec![];
                let mut 属 = vec![];
                for s in states {
                    match 各态.iter().position(|u| u.hash == s.hash) {
                        Some(k) => 属.push(k),
                        None => {
                            属.push(各态.len());
                            各态.push(s);
                        }
                    }
                }
                Ok(判断输入 {
                    states: 各态,
                    属,
                    questions,
                })
            }
            _ => Err(EffectError("JEV 判断端口只收状态加题组".into())),
        }
    }
    fn items(&self) -> Vec<(&State, &Question)> {
        self.questions
            .iter()
            .zip(&self.属)
            .map(|(q, k)| (&self.states[*k], q))
            .collect()
    }
}

/// 请求体按上限开工作线程发出，结果按原位置放回（步 15e）。线程数 `min(上限, 请求体数)`。
fn 并发发出(t: &Attempt, 请求体: &[Json], 上限: usize) -> Vec<Result<Json, EffectError>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let n = 请求体.len();
    let 下一个 = AtomicUsize::new(0);
    let 槽: Vec<Mutex<Option<Result<Json, EffectError>>>> =
        (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..上限.min(n) {
            s.spawn(|| {
                loop {
                    let i = 下一个.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = t(&请求体[i]);
                    if let Ok(mut g) = 槽[i].lock() {
                        *g = Some(r);
                    }
                }
            });
        }
    });
    槽.into_iter()
        .map(|m| {
            m.into_inner()
                .ok()
                .flatten()
                .unwrap_or_else(|| Err(EffectError("并发发出：工作线程没有放回结果".into())))
        })
        .collect()
}

impl JevPorts {
    /// 判断端口的端口内并发上限（步 15e；宿主从画像 `concurrency` 取，未测为 1）。
    pub fn with_concurrency(mut self, n: usize) -> JevPorts {
        self.judge = self.judge.with_concurrency(n);
        self
    }
    pub fn new(client: JevClient) -> JevPorts {
        let model = client.model.clone();
        let rest = jpp_effects::PORTED
            .iter()
            .map(|e| (*e, jpp_effects::spec(*e)))
            .filter(|(_, s)| !s.produces_reading)
            .map(|(effect, s)| {
                let instance = EffectInstance {
                    effect,
                    model: model.clone(),
                };
                if s.output_shape == jpp_effects::OutputShape::Answer {
                    Box::new(UnansweredPort::new(instance)) as Box<dyn EffectPort>
                } else {
                    Box::new(UnservedPort::new(instance, JEV_NO_GEN))
                }
            })
            .collect();
        JevPorts {
            judge: JevPort::new(client),
            rest,
        }
    }
    pub fn model_id(&self) -> String {
        self.judge.client.model.clone()
    }
    /// 端口表，借用本对象。
    pub fn ports(&mut self) -> Ports<'_> {
        let mut p = Ports::new().with(&mut self.judge);
        for r in self.rest.iter_mut() {
            p = p.with(&mut **r);
        }
        p
    }
}

impl super::BackendPorts for JevPorts {
    fn ports(&mut self) -> Ports<'_> {
        JevPorts::ports(self)
    }
    fn model_id(&self) -> String {
        JevPorts::model_id(self)
    }
}

/// 注册表条目（步 15g-0）：`--backend live` 接真机 JEV。默认模型名与发行画像 `profiles/jev-1.13.0.json`
/// （由 `地基/foundation/profile/profiles/jev-1.13.0.json` 经 `scripts/gen_profiles.py` 生成）的文件名、
/// `tests/e_jpp_live.rs`、`tests/jev_client.rs` 一致（步 15g-0 前是 `cli/options.rs::DEFAULT_LIVE_MODEL`）。
pub const SPEC: super::BackendSpec = super::BackendSpec {
    name: "live",
    default_model: "jev-1.13.0",
    mode_label: "live model backend (~/.typesafe-key)",
    transport: true,
    calib: true,
    build: build_live,
};

/// `--backend live`：只在 `live` feature 开着时才真的接 `JevClient::live`
/// （凭据只从 `~/.typesafe-key` 读，这条路上不碰任何 CLI 参数或日志）。
/// 价格从画像来（B73）：画像已由宿主解析，这里不会是没有画像的情形。（步 15g-0 自 `cli/run_io.rs` 搬来）
#[cfg(feature = "live")]
fn build_live(model: &str, profile: &Profile) -> Result<Box<dyn super::BackendPorts>, String> {
    JevClient::live(
        model,
        profile.price_per_input_token(),
        transport_timeout(profile),
    )
    // 端口内并发上限只从画像来，未测取 1（`20` §3.9；步 15e）
    .map(|c| {
        Box::new(JevPorts::new(c).with_concurrency(profile.concurrency().unwrap_or(1) as usize))
            as Box<dyn super::BackendPorts>
    })
    .map_err(|e| e.0)
}

#[cfg(not(feature = "live"))]
fn build_live(_model: &str, _profile: &Profile) -> Result<Box<dyn super::BackendPorts>, String> {
    Err("--backend live requires jpp built with `--features live`".into())
}

/// 单次真机请求的超时：只从画像 `transport.timeout_s` 来，代码里不编秒数（B73）。
/// 依据：地基/过程记录/工程-传输超时.md。（步 15g-0 自 `cli/run_io.rs` 搬来）
pub fn transport_timeout(profile: &Profile) -> Option<std::time::Duration> {
    profile
        .transport_timeout_s()
        .map(std::time::Duration::from_secs_f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Mat;

    fn 客户端() -> JevClient {
        JevClient::with_transport(
            "jev-1.13.0",
            Box::new(|_: &Json| {
                Ok(json!({"answers": {"q0": {"noul": 0.7}}, "usage": {"input_tokens": 10}}))
            }),
        )
        .with_price(Some(0.5))
    }

    fn 读(rs: &[Result<EffectOut, EffectError>]) -> Vec<String> {
        rs.iter()
            .map(|r| match r {
                Ok(EffectOut::Readings(r)) => format!(
                    "{:?} {:?} {:?} {} {}",
                    r.answers, r.mode_share, r.perms, r.tokens, r.cost
                ),
                Ok(_) => "非读数".into(),
                Err(e) => e.0.clone(),
            })
            .collect()
    }

    /// 步 15e：端口内并发。假传输每次睡 100 ms、按请求体里的材料回读数；12 个调用在并发 12 下一批发完，
    /// 墙钟远小于串行；答案按提交顺序；select 开置换时两份请求体也并发；`calls()` 等于成功请求数。
    #[test]
    fn 端口内并发按提交顺序返回() {
        let t: Attempt = std::sync::Arc::new(|b: &Json| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let s = b["state"].to_string();
            let p = if s.contains("材料7") { 0.2 } else { 0.8 };
            Ok(json!({"answers": {"q0": {"noul": p}}, "usage": {"input_tokens": 10}}))
        });
        let 调用: Vec<CallInput> = (0..12)
            .map(|i| CallInput::StateQuestions {
                state: State::new(
                    vec![Mat::literal(json!(format!("材料{i}")))],
                    vec![],
                    vec![],
                    vec![],
                    false,
                ),
                questions: vec![Question::new(Op::Test, "行吗", "k", vec![])],
            })
            .collect();
        let judge = jpp_effects::find(|s| s.produces_reading).unwrap();
        let mut jp = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", t.clone()))
            .with_concurrency(12);
        let 起 = std::time::Instant::now();
        let rs = jp.ports().call_many(judge, 调用.clone()).unwrap();
        let 并发用时 = 起.elapsed();
        assert!(
            并发用时 < std::time::Duration::from_millis(400),
            "{并发用时:?}"
        );
        let ps = 读(&rs);
        assert!(ps[7].contains("0.2") && ps[6].contains("0.8"), "{ps:?}");
        assert_eq!(jp.judge.calls(), 12);
        // 同一批串行（并发 1）答案相同
        let mut 串 = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", t));
        let rs1 = 串.ports().call_many(judge, 调用).unwrap();
        assert_eq!(ps, 读(&rs1));

        // select 开置换：一个调用的两份请求体并发，合并与串行相同
        let 选: Attempt = std::sync::Arc::new(|_: &Json| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            // 总把第一位候选判高：正逆两序的众数不一致
            Ok(
                json!({"answers": {"q0": {"probabilities": {"c0": 0.7, "c1": 0.3}}}, "usage": {"input_tokens": 5}}),
            )
        });
        let s2 = State::new(
            vec![],
            vec![],
            vec![],
            vec![Mat::literal(json!("甲")), Mat::literal(json!("乙"))],
            false,
        );
        let mut q = Question::new(Op::Select, "哪个", "k2", vec![]);
        q.permute = true;
        let 入 = vec![CallInput::StateQuestions {
            state: s2,
            questions: vec![q],
        }];
        let mut 并 = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", 选.clone()))
            .with_concurrency(4);
        let 起 = std::time::Instant::now();
        let a = 并.ports().call_many(judge, 入.clone()).unwrap();
        assert!(起.elapsed() < std::time::Duration::from_millis(180));
        let mut 串 = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", 选));
        let b = 串.ports().call_many(judge, 入).unwrap();
        assert_eq!(读(&a), 读(&b));
        assert_eq!((并.judge.calls(), 串.judge.calls()), (2, 2));
    }

    /// 步 15i（B155）：同材料合批的调用在并发路径上也是一份请求体——`state` 不含 `over`，两道 `select`
    /// 各带自己的候选；答案按各自的候选数还原；与串行路径相同。
    #[test]
    fn 并发路径收同材料合批() {
        let 记: std::sync::Arc<std::sync::Mutex<Vec<Json>>> = Default::default();
        let 记2 = 记.clone();
        let t: Attempt = std::sync::Arc::new(move |b: &Json| {
            记2.lock().unwrap().push(b.clone());
            let mut ans = serde_json::Map::new();
            for (qid, q) in b["questions"].as_object().unwrap() {
                let n = q["criteria"].as_object().unwrap().len();
                let probs: serde_json::Map<String, Json> = (0..n)
                    .map(|k| (format!("c{k}"), json!(if k == n - 1 { 0.9 } else { 0.0 })))
                    .collect();
                ans.insert(qid.clone(), json!({"probabilities": probs}));
            }
            Ok(json!({"answers": ans, "usage": {"input_tokens": 3}}))
        });
        let 态 = |n: usize| {
            State::new(
                vec![Mat::literal(json!("材料"))],
                vec![],
                vec![],
                (0..n)
                    .map(|i| Mat::literal(json!(format!("候选{i}"))))
                    .collect(),
                false,
            )
        };
        let q = Question::new(Op::Select, "哪个", "k", vec![]);
        let 入 = vec![CallInput::MaterialQuestions {
            states: vec![态(2), 态(3)],
            questions: vec![q.clone(), q],
        }];
        let judge = jpp_effects::find(|s| s.produces_reading).unwrap();
        let mut 并 = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", t.clone()))
            .with_concurrency(4);
        let a = 并.ports().call_many(judge, 入.clone()).unwrap();
        let mut 串 = JevPorts::new(JevClient::with_shared_transport("jev-1.13.0", t));
        let b = 串.ports().call_many(judge, 入).unwrap();
        assert_eq!(读(&a), 读(&b));
        assert!(
            读(&a)[0].starts_with("[Choice([0.0, 0.9]), Choice([0.0, 0.0, 0.9])]"),
            "{:?}",
            读(&a)
        );
        assert_eq!((并.judge.calls(), 串.judge.calls()), (1, 1));
        let 发 = 记.lock().unwrap();
        assert!(发[0]["state"].get("over").is_none(), "{}", 发[0]);
        assert_eq!(
            发[0]["questions"]["q1"]["criteria"]
                .as_object()
                .unwrap()
                .len(),
            3
        );
    }

    /// 步 15b（R）：真机端口表与 `JevClient` 的三个方法逐一对应——判断同答案同费用，生成报同一句错，
    /// 问人是「还没答」
    #[test]
    fn 真机端口表与客户端逐项相同() {
        let s = State::new(
            vec![Mat::literal(json!("x"))],
            vec![],
            vec![],
            vec![],
            false,
        );
        let q = Question::new(Op::Test, "行吗", "k", vec![]);
        let mut c = 客户端();
        let direct = c.judge(&s, &[&q]).unwrap();
        let mut jp = JevPorts::new(客户端());
        assert_eq!(jp.model_id(), "jev-1.13.0");
        let mut ports = jp.ports();
        let judge = jpp_effects::find(|s| s.produces_reading).unwrap();
        let EffectOut::Readings(r) = ports
            .call(
                judge,
                CallInput::StateQuestions {
                    state: s.clone(),
                    questions: vec![q.clone()],
                },
            )
            .unwrap()
        else {
            panic!("判断端口要返回读数")
        };
        assert_eq!(format!("{:?}", r.answers), format!("{:?}", direct.answers));
        assert_eq!((r.tokens, r.cost), (direct.tokens, direct.cost));
        let gen_err = JEV_NO_GEN;
        let insts = ports.instances();
        let gen_ = insts
            .iter()
            .find(|i| {
                let s = jpp_effects::spec(i.effect);
                !s.produces_reading && s.output_shape != jpp_effects::OutputShape::Answer
            })
            .unwrap()
            .effect;
        let e = ports
            .call(
                gen_,
                CallInput::Prompt {
                    prompt: "p".into(),
                    ctx: vec![],
                    n: 1,
                    retry_seq: 0,
                },
            )
            .err()
            .unwrap()
            .0;
        assert_eq!(e, gen_err);
        let ask = insts
            .iter()
            .find(|i| jpp_effects::spec(i.effect).output_shape == jpp_effects::OutputShape::Answer)
            .unwrap()
            .effect;
        let a = ports.call(
            ask,
            CallInput::StateQuestion {
                state: s,
                question: q,
            },
        );
        assert!(matches!(a, Ok(EffectOut::Answer(None))));
        drop(ports);
        assert_eq!(jp.judge.calls(), 1);
    }
}
