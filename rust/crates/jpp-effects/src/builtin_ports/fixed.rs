//! 固定观察端口 `FixedPorts`（`20` §2.3 三个内置实现之一；`21` 步 15b）：只按观察键命中，未命中即错
//! （确定性对照，不是模型）。查表逻辑自原 `FixedClient` 搬来（步 15c 删除 `FixedClient`），行为逐字节
//! 相同。步 4d 加的两项夹具能力（置换测量、带上下文的固定生成）原样保留。

use std::collections::HashMap;

use serde_json::{Value as Json, json};

use crate::port::{
    CallInput, Completed, EffectCall, EffectError, EffectOut, EffectPort, GenResult, JudgeResult,
    Ports, Ticket,
};
use crate::{EffectId, EffectInstance};
use jpp_value::value::{Answer, Question, State, canon, hash_of};

/// 固定观察的模型 id（账本头与判断键用它）
pub const FIXED_MODEL: &str = "fixed-0";

/// 观察键 = 状态规范 JSON 哈希 + 题（固定观察表按它命中）。
pub fn obs_key(state: &State, q: &Question) -> String {
    hash_of(&[
        "obs",
        &canon(&state.to_json()),
        q.op.phys(),
        &q.text,
        &q.scale.join("\u{1e}"),
    ])
}

/// 带上下文的固定生成键（步 4d）：提示、retry_seq 与 ctx 规范 JSON 的哈希。
fn gen_ctx_key(prompt: &str, retry_seq: u64, ctx: &[Json]) -> String {
    format!(
        "{prompt}\u{1f}{retry_seq}\u{1f}{}",
        hash_of(&["gen-ctx", &canon(&Json::Array(ctx.to_vec()))])
    )
}

/// 固定观察的判断：逐题按观察键查表；没有一条固定过置换测量时 `perms`/`mode_share` 为空。
/// `log` 记每道命中的题。未命中即错（报文给出内核算出的状态与题）。
fn fixed_judge(
    table: &HashMap<String, Answer>,
    perms: &HashMap<String, (usize, f64)>,
    log: &mut Vec<(String, String)>,
    state: &State,
    questions: &[&Question],
) -> Result<JudgeResult, EffectError> {
    let mut answers = vec![];
    let mut measured = vec![];
    for q in questions {
        let k = obs_key(state, q);
        measured.push(perms.get(&k).copied());
        let a = table.get(&k).cloned().ok_or_else(|| {
            // **把自己算出来的那份状态与题打出来。**
            //
            // 以前只给哈希。于是作者猜 `measure` 的档位字段名猜了四次
            // （`bands`/`levels`/`grades`/`options` 全不中，最后是 `scale`），
            // **每次拿到同一句话、同一个哈希，没有任何梯度**。
            // **那份 JSON 就在手边，只是没打出来**——这是整个任务里浪费时间最多的单点，
            // 而且是免费可修的：「说不准」与「说不出」是两回事，这里属于后者。
            EffectError(format!(
                "固定观察未命中：题「{}」× 状态 {}（键 {k}）；固定观察不是模型，缺记录即错。内核这次算出来的状态 = {}；题 = {}。夹具里要有一条 observation，其 on/ctx/ref/over 与上面的状态逐字段相同，op/text/calib/scale/evidence 与上面的题相同",
                q.text,
                state.hash.chars().take(8).collect::<String>(),
                canon(&state.as_fixture_json()),
                canon(&json!({"op": q.op.fixture_name(), "text": q.text, "calib": q.calib, "scale": q.scale})),
            ))
        })?;
        log.push((k, q.text.clone()));
        answers.push(a);
    }
    // 没有一条固定过置换测量时与改前逐字节相同（空向量）
    let (mode_share, perms) = if measured.iter().all(Option::is_none) {
        (vec![], vec![])
    } else {
        (
            measured.iter().map(|m| m.map(|(_, s)| s)).collect(),
            measured.iter().map(|m| m.map_or(0, |(k, _)| k)).collect(),
        )
    };
    Ok(JudgeResult {
        answers,
        tokens: 0,
        cost: 0.0,
        mode_share,
        perms,
    })
}

/// 固定生成：先按 (提示, retry_seq, ctx 哈希) 查，查不到再退到不带 ctx 的旧键。
fn fixed_gen(
    gens: &HashMap<String, Vec<Json>>,
    prompt: &str,
    ctx: &[Json],
    retry_seq: u64,
) -> Result<GenResult, EffectError> {
    let outputs = gens
        .get(&gen_ctx_key(prompt, retry_seq, ctx))
        .or_else(|| gens.get(&format!("{prompt}\u{1f}{retry_seq}")))
        .cloned()
        .ok_or_else(|| EffectError(format!("固定生成未命中：{prompt} retry_seq={retry_seq}")))?;
    Ok(GenResult {
        outputs,
        tokens: 0,
        cost: 0.0,
    })
}

/// 固定的人工回答：登记过的照给（`None` = 明说还没答）；一条都没登记 = 第一趟，静默挂起；
/// 登记了却没命中即错。
fn fixed_ask(
    asks: &HashMap<String, Option<Answer>>,
    state: &State,
    q: &Question,
) -> Result<Option<Answer>, EffectError> {
    let k = obs_key(state, q);
    match asks.get(&k) {
        // 登记过：有答案就给，明说「还没答」就挂起——**那是真的在等人**
        Some(a) => Ok(a.clone()),
        // **一条 responses 都没给 = 这是第一趟，本来就没人答**，静默挂起
        None if asks.is_empty() => Ok(None),
        // **给了 responses 却没命中**——以前这里是 `unwrap_or(None)`，
        // 于是它与「人还没答」在唯一的输出上**逐字段一模一样**。
        // 「人还没答」与「你的 responses 没命中」是两件事。
        None => Err(EffectError(format!(
            "responses 未命中：题「{}」× 状态 {}（键 {k}）；夹具给了 {} 条 responses，没有一条对得上。\
             内核这次算出来的状态 = {}；题 = {}。responses 里要有一条，其 on/ctx/ref/over 与上面的状态逐字段相同，op/text/calib/scale 与上面的题相同",
            q.text,
            state.hash.chars().take(8).collect::<String>(),
            asks.len(),
            canon(&state.as_fixture_json()),
            canon(
                &json!({"op": q.op.fixture_name(), "text": q.text, "calib": q.calib, "scale": q.scale})
            ),
        ))),
    }
}

fn wrong_input(port: &str) -> EffectError {
    EffectError(format!("{port} 收到的输入形状不对"))
}

/// 固定观察的判断端口。
#[derive(Default)]
pub struct FixedJudge {
    pub table: HashMap<String, Answer>,
    /// 观察键 → 固定下来的置换测量 `(perms, mode_share)`（步 4d，原型 K6）。
    /// 缺省 = 没测过置换，与改前相同：`select` 得不到 `Pick`。
    pub perms: HashMap<String, (usize, f64)>,
    pub n_calls: u64,
    pub n_questions: u64,
    pub log: Vec<(String, String)>,
    done: Completed,
}

impl EffectPort for FixedJudge {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: EffectId::Judge,
            model: FIXED_MODEL.into(),
        }
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let (table, perms, log) = (&self.table, &self.perms, &mut self.log);
        let (n_calls, n_questions) = (&mut self.n_calls, &mut self.n_questions);
        Ok(self.done.submit_with(calls, |c| match c.input {
            CallInput::StateQuestions { state, questions } => {
                let qs: Vec<&Question> = questions.iter().collect();
                let r = fixed_judge(table, perms, log, &state, &qs)?;
                *n_calls += 1;
                *n_questions += qs.len() as u64;
                Ok(EffectOut::Readings(r))
            }
            _ => Err(wrong_input("固定判断端口")),
        }))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 固定生成端口。
#[derive(Default)]
pub struct FixedGen {
    pub gens: HashMap<String, Vec<Json>>,
    pub n_calls: u64,
    done: Completed,
}

impl EffectPort for FixedGen {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: EffectId::Gen,
            model: FIXED_MODEL.into(),
        }
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let (gens, n_calls) = (&self.gens, &mut self.n_calls);
        Ok(self.done.submit_with(calls, |c| match c.input {
            CallInput::Prompt {
                prompt,
                ctx,
                retry_seq,
                ..
            } => {
                *n_calls += 1;
                fixed_gen(gens, &prompt, &ctx, retry_seq).map(EffectOut::Mats)
            }
            _ => Err(wrong_input("固定生成端口")),
        }))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 固定的问人端口。
#[derive(Default)]
pub struct FixedAsk {
    pub asks: HashMap<String, Option<Answer>>,
    done: Completed,
}

impl EffectPort for FixedAsk {
    fn instance(&self) -> EffectInstance {
        EffectInstance {
            effect: EffectId::Ask,
            model: FIXED_MODEL.into(),
        }
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let asks = &self.asks;
        Ok(self.done.submit_with(calls, |c| match c.input {
            CallInput::StateQuestion { state, question } => {
                fixed_ask(asks, &state, &question).map(EffectOut::Answer)
            }
            _ => Err(wrong_input("固定问人端口")),
        }))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 固定观察：判断、生成、问人三个端口，覆盖今天经端口的全部效应实例（`do` 走动作登记处、`transform`
/// 是宿主函数，都不经端口）。
#[derive(Default)]
pub struct FixedPorts {
    pub judge: FixedJudge,
    pub generate: FixedGen,
    pub ask: FixedAsk,
}

impl FixedPorts {
    pub fn new() -> FixedPorts {
        FixedPorts::default()
    }
    pub fn observe(&mut self, state: &State, q: &Question, a: Answer) -> String {
        let k = obs_key(state, q);
        self.judge.table.insert(k.clone(), a);
        k
    }
    pub fn observe_key(&mut self, key: &str, a: Answer) {
        self.judge.table.insert(key.to_string(), a);
    }
    pub fn fix_gen(&mut self, prompt: &str, retry_seq: u64, out: Vec<Json>) {
        self.generate
            .gens
            .insert(format!("{prompt}\u{1f}{retry_seq}"), out);
    }
    /// 带上下文的固定生成（步 4d，原型 K7）：同提示不同 ctx 可以各给一份输出。
    pub fn fix_gen_ctx(&mut self, prompt: &str, retry_seq: u64, ctx: &[Json], out: Vec<Json>) {
        self.generate
            .gens
            .insert(gen_ctx_key(prompt, retry_seq, ctx), out);
    }
    /// 给一条观察固定置换测量（步 4d，原型 K6）。
    pub fn fix_perms(&mut self, key: &str, perms: usize, mode_share: f64) {
        self.judge
            .perms
            .insert(key.to_string(), (perms, mode_share));
    }
    pub fn fix_ask(&mut self, state: &State, q: &Question, a: Option<Answer>) {
        self.ask.asks.insert(obs_key(state, q), a);
    }
    /// 判断与生成的调用总数（与原 `FixedClient::calls` 同口径）
    pub fn calls(&self) -> u64 {
        self.judge.n_calls + self.generate.n_calls
    }
    /// 三个端口组成的端口表，借用本对象。
    pub fn ports(&mut self) -> Ports<'_> {
        Ports::new()
            .with(&mut self.judge)
            .with(&mut self.generate)
            .with(&mut self.ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpp_value::value::{Mat, Op};

    fn 状态(x: &str) -> State {
        State::new(vec![Mat::literal(json!(x))], vec![], vec![], vec![], false)
    }

    /// 固定观察端口：命中给登记的答案与置换测量，未命中报出内核算出的状态与题；生成按提示查；
    /// 问人没登记任何回答时是「还没答」；计数与日志按原 `FixedClient` 口径（判断与生成都计调用）
    #[test]
    fn 固定端口按观察键命中() {
        let (s, q) = (状态("a"), Question::new(Op::Test, "行吗", "k", vec![]));
        let (s2, q2) = (状态("b"), Question::new(Op::Test, "没登记", "k", vec![]));
        let mut fp = FixedPorts::new();
        let k = fp.observe(&s, &q, Answer::Noul(0.9));
        fp.fix_perms(&k, 2, 1.0);
        fp.fix_gen("写", 0, vec![json!("x")]);
        let mut ports = fp.ports();
        let EffectOut::Readings(r) = ports
            .call(
                EffectId::Judge,
                CallInput::StateQuestions {
                    state: s.clone(),
                    questions: vec![q.clone()],
                },
            )
            .unwrap()
        else {
            panic!("判断端口要返回读数")
        };
        assert!(matches!(r.answers[..], [Answer::Noul(p)] if p == 0.9));
        assert_eq!((r.perms, r.mode_share), (vec![2], vec![Some(1.0)]));
        let miss = ports.call(
            EffectId::Judge,
            CallInput::StateQuestions {
                state: s2,
                questions: vec![q2],
            },
        );
        assert!(
            miss.err()
                .unwrap()
                .0
                .starts_with("固定观察未命中：题「没登记」")
        );
        let EffectOut::Mats(g) = ports
            .call(
                EffectId::Gen,
                CallInput::Prompt {
                    prompt: "写".into(),
                    ctx: vec![],
                    n: 1,
                    retry_seq: 0,
                },
            )
            .unwrap()
        else {
            panic!("生成端口要返回材料")
        };
        assert_eq!(g.outputs, vec![json!("x")]);
        let a = ports.call(
            EffectId::Ask,
            CallInput::StateQuestion {
                state: s,
                question: q,
            },
        );
        assert!(matches!(a, Ok(EffectOut::Answer(None))));
        drop(ports);
        assert_eq!(fp.calls(), 2);
        assert_eq!(fp.judge.log.len(), 1);
    }

    /// 本版每种效应一个实例（B60）：同一效应注册两次报错；没有端口的效应调用报错
    #[test]
    fn 端口表按效应实例() {
        let mut a = FixedPorts::new();
        let mut b = FixedPorts::new();
        let mut p = Ports::new().with(&mut a.judge);
        assert!(p.register(Box::new(&mut b.judge)).is_err());
        let r = p.call(
            EffectId::Ask,
            CallInput::StateQuestion {
                state: 状态("a"),
                question: Question::new(Op::Test, "q", "k", vec![]),
            },
        );
        assert!(r.err().unwrap().0.contains("没有端口"));
    }
}
