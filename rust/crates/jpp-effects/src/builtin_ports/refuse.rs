//! 拒绝调用的端口（`20` §2.3 三个内置实现中的 `NoCallPorts`、`ReplayPorts`；`21` 步 15b），以及宿主
//! 给没有后端的效应实例占位用的两个端口。
//!
//! 重放不发调用：任何 `submit` 都得到错误，报文按输入形状给出（与原 `NoCallClient` 和 CLI 的
//! `ReplayClient` 逐字相同）。两者只差模型 id：`NoCallPorts` 固定为固定观察的 `fixed-0`，`ReplayPorts`
//! 用账本头记的模型（重放的查表键含模型 id，必须与记录时相同）。

use crate::port::{
    CallInput, Completed, EffectCall, EffectError, EffectOut, EffectPort, Ports, Ticket,
};
use crate::{EffectId, EffectInstance};

use super::fixed::FIXED_MODEL;

/// 拒绝一切调用的端口（一个实例）。
pub struct RefusePort {
    instance: EffectInstance,
    done: Completed,
}

impl RefusePort {
    pub fn new(instance: EffectInstance) -> RefusePort {
        RefusePort {
            instance,
            done: Completed::default(),
        }
    }
}

fn refusal(input: &CallInput) -> EffectError {
    match input {
        CallInput::StateQuestions { questions, .. } => EffectError(format!(
            "重放中不应发调用（题 {}）",
            questions
                .iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>()
                .join("|")
        )),
        CallInput::Prompt { prompt, .. } => EffectError(format!("重放中不应发 gen：{prompt}")),
        CallInput::StateQuestion { .. } => EffectError("重放中不应发 ask".into()),
    }
}

impl EffectPort for RefusePort {
    fn instance(&self) -> EffectInstance {
        self.instance.clone()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        Ok(self.done.submit_with(calls, |c| Err(refusal(&c.input))))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 经端口的三种效应实例（`judge`、`gen`、`ask`）各一个拒绝端口。
fn refuse_all(model: &str) -> Ports<'static> {
    [EffectId::Judge, EffectId::Gen, EffectId::Ask]
        .into_iter()
        .fold(Ports::new(), |p, effect| {
            p.with(RefusePort::new(EffectInstance {
                effect,
                model: model.to_string(),
            }))
        })
}

/// 拒绝一切调用：重放验证用，模型 id 为固定观察的 `fixed-0`。
pub struct NoCallPorts;

impl NoCallPorts {
    pub fn ports() -> Ports<'static> {
        refuse_all(FIXED_MODEL)
    }
}

/// 只凭账本重放：拒绝一切调用，模型 id 用账本头记的那个（`Session::replay_model_id`）。
pub struct ReplayPorts;

impl ReplayPorts {
    pub fn ports(model: &str) -> Ports<'static> {
        refuse_all(model)
    }
}

/// 没有后端的生成实例：每次调用都报宿主给的错误。真机判断器不生成时，宿主用它占 `gen` 的位。
pub struct UnservedPort {
    instance: EffectInstance,
    message: String,
    done: Completed,
}

impl UnservedPort {
    pub fn new(instance: EffectInstance, message: &str) -> UnservedPort {
        UnservedPort {
            instance,
            message: message.to_string(),
            done: Completed::default(),
        }
    }
}

impl EffectPort for UnservedPort {
    fn instance(&self) -> EffectInstance {
        self.instance.clone()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        let m = self.message.clone();
        Ok(self
            .done
            .submit_with(calls, |_| Err(EffectError(m.clone()))))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}

/// 没有人工通道的问人实例：每次都「还没答」，程序以 Pending 结束，由宿主续跑时补答案。
pub struct UnansweredPort {
    instance: EffectInstance,
    done: Completed,
}

impl UnansweredPort {
    pub fn new(instance: EffectInstance) -> UnansweredPort {
        UnansweredPort {
            instance,
            done: Completed::default(),
        }
    }
}

impl EffectPort for UnansweredPort {
    fn instance(&self) -> EffectInstance {
        self.instance.clone()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        Ok(self
            .done
            .submit_with(calls, |_| Ok(EffectOut::Answer(None))))
    }
    fn poll(&mut self, t: &Ticket) -> std::task::Poll<Result<EffectOut, EffectError>> {
        self.done.poll(t)
    }
}
