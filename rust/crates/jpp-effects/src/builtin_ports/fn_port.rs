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
