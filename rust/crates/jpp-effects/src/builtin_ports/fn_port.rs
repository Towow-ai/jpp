//! 由闭包给出结果的端口 `FnPort`（`21` 步 15c）：测试桩与嵌入宿主的小后端用它，只注册用到的实例
//! （`20` §2.3：「端口按效应实例注册，测试桩只注册用到的实例」）。当场完成：`submit` 时调用闭包，
//! `poll` 取走结果。

use serde_json::Value as Json;

use crate::port::{
    CallInput, Completed, EffectCall, EffectError, EffectOut, EffectPort, GenResult, JudgeResult,
    Ticket,
};
use crate::{EffectId, EffectInstance};
use jpp_value::value::{Answer, Question, State};

type Handler<'f> = Box<dyn FnMut(CallInput) -> Result<EffectOut, EffectError> + 'f>;

/// 一个效应实例，结果由闭包给出。
pub struct FnPort<'f> {
    instance: EffectInstance,
    f: Handler<'f>,
    done: Completed,
}

fn wrong_input() -> EffectError {
    EffectError("端口收到的输入形状与它服务的效应不符".into())
}

impl<'f> FnPort<'f> {
    /// 任意实例，闭包收原样的调用输入。
    pub fn new(
        instance: EffectInstance,
        f: impl FnMut(CallInput) -> Result<EffectOut, EffectError> + 'f,
    ) -> FnPort<'f> {
        FnPort {
            instance,
            f: Box::new(f),
            done: Completed::default(),
        }
    }

    /// 判断实例：闭包收一个状态与一组题，返回读数。
    ///
    /// 同材料合批的调用（B155，[`CallInput::MaterialQuestions`]）按状态分段：连续同一 `StateHash`
    /// 的题一段，逐段调闭包，答案、置换测量按题序拼回，token 与费用相加。闭包签名不变，
    /// 嵌入宿主的小后端不用改；这一次调用在运行时仍计 1 次。
    pub fn judge(
        model: &str,
        mut f: impl FnMut(&State, &[&Question]) -> Result<JudgeResult, EffectError> + 'f,
    ) -> FnPort<'f> {
        FnPort::new(
            EffectInstance {
                effect: EffectId::Judge,
                model: model.to_string(),
            },
            move |input| match input {
                CallInput::StateQuestions { state, questions } => {
                    let qs: Vec<&Question> = questions.iter().collect();
                    f(&state, &qs).map(EffectOut::Readings)
                }
                CallInput::MaterialQuestions { states, questions } => {
                    分段判断(&mut f, &states, &questions).map(EffectOut::Readings)
                }
                _ => Err(wrong_input()),
            },
        )
    }

    /// 生成实例：闭包收提示、上下文材料内容、份数与轮次序号。
    pub fn generate(
        model: &str,
        mut f: impl FnMut(&str, &[Json], usize, u64) -> Result<GenResult, EffectError> + 'f,
    ) -> FnPort<'f> {
        FnPort::new(
            EffectInstance {
                effect: EffectId::Gen,
                model: model.to_string(),
            },
            move |input| match input {
                CallInput::Prompt {
                    prompt,
                    ctx,
                    n,
                    retry_seq,
                } => f(&prompt, &ctx, n, retry_seq).map(EffectOut::Mats),
                _ => Err(wrong_input()),
            },
        )
    }

    /// 问人实例：闭包收一个状态与一道题，返回回答（`None` = 还没答）。
    pub fn ask(
        model: &str,
        mut f: impl FnMut(&State, &Question) -> Result<Option<Answer>, EffectError> + 'f,
    ) -> FnPort<'f> {
        FnPort::new(
            EffectInstance {
                effect: EffectId::Ask,
                model: model.to_string(),
            },
            move |input| match input {
                CallInput::StateQuestion { state, question } => {
                    f(&state, &question).map(EffectOut::Answer)
                }
                _ => Err(wrong_input()),
            },
        )
    }
}

impl EffectPort for FnPort<'_> {
    fn instance(&self) -> EffectInstance {
        self.instance.clone()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let f = &mut self.f;
        Ok(self.done.submit_with(calls, |c| f(c.input)))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 单状态判断闭包：收一个状态与一组题，返回读数
type 单状态判断<'a> = dyn FnMut(&State, &[&Question]) -> Result<JudgeResult, EffectError> + 'a;

/// 同材料合批的调用按状态分段交给单状态闭包（B155）：连续同一 `StateHash` 的题一段；
/// 各段结果按题序拼回。置换测量与自报置信度只要有一段给了就按题补齐（没给的题 `None`、`perms` 0）。
fn 分段判断(
    f: &mut 单状态判断<'_>,
    states: &[State],
    questions: &[Question],
) -> Result<JudgeResult, EffectError> {
    let mut out = JudgeResult {
        answers: vec![],
        tokens: 0,
        cost: 0.0,
        mode_share: vec![],
        perms: vec![],
        confidence: vec![],
    };
    let mut 测过 = false;
    // 自报置信度（B154）同法：有一段给了就按题补齐，没给的题 `None`
    let mut 各题置信: Vec<Option<f64>> = vec![];
    let mut 各题测量: Vec<(Option<f64>, usize)> = vec![];
    let mut i = 0;
    while i < questions.len() {
        let mut j = i + 1;
        while j < questions.len() && states[j].hash == states[i].hash {
            j += 1;
        }
        let qs: Vec<&Question> = questions[i..j].iter().collect();
        let r = f(&states[i], &qs)?;
        out.tokens += r.tokens;
        out.cost += r.cost;
        for k in 0..qs.len() {
            let ms = r.mode_share.get(k).copied().flatten();
            测过 |= ms.is_some();
            各题测量.push((ms, r.perms.get(k).copied().unwrap_or(0)));
            各题置信.push(r.confidence.get(k).copied().flatten());
        }
        out.answers.extend(r.answers);
        i = j;
    }
    if 测过 {
        out.mode_share = 各题测量.iter().map(|(m, _)| *m).collect();
        out.perms = 各题测量.iter().map(|(_, p)| *p).collect();
    }
    if 各题置信.iter().any(Option::is_some) {
        out.confidence = 各题置信;
    }
    Ok(out)
}
