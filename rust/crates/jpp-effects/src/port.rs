//! 统一效应端口 `EffectPort`（`20` §2.3 L2、T2、D12.4；`21` 步 15b）。
//!
//! 一个端口服务一个效应实例（B60：`EffectInstance = (效应, 模型)`），按实例注册进 [`Ports`]。运行时只经
//! `Ports` 发调用：`submit` 交出一批调用、拿回票据，`poll` 按票据取结果。层内并发发生在端口实现里
//! （步 15e）；本步的端口都是当场完成的，`submit` 返回时结果已在端口里，`poll` 立即 `Ready`。账本按
//! 登记顺序追加，不按返回顺序（运行时负责，端口不写账本）。
//!
//! 与 `20` §2.3 字面的差异（记 `过程记录/工程-步15b.md`）：`EffectCall.input` 是按输入槽形状分的
//! 三种带类型输入（状态加题组、提示加上下文、状态加一题），不是 `Vec<OwnedValue>`；`site`、`phys`、`key`
//! 三个字段今天没有端口读，未加；`EffectPort` 不要求 `Send`（值类型含 `Rc`；并发端口在内部把调用转成
//! 可跨线程的请求体，步 15e）。

use std::collections::BTreeMap;
use std::task::Poll;

use serde_json::Value as Json;

use crate::{EffectId, EffectInstance};
use jpp_value::value::{Answer, Question, State};

#[derive(Debug, Clone)]
pub struct EffectError(pub String);

/// 概率向量里最大的那一档的下标
pub fn argmax_index(v: &[f64]) -> usize {
    v.iter()
        .enumerate()
        .fold(
            (0usize, f64::MIN),
            |best, (i, p)| if *p > best.1 { (i, *p) } else { best },
        )
        .0
}

/// `gen` 的返回：**带实际费用与 token**。以前只返回 `Vec<Json>`，于是 `budget.cost`
/// 对 gen 整条路失效——一个只 gen 不 judge 的程序花多少钱都不会被拦住。
/// 形状与 `JudgeResult` 对齐，理由是同一条纪律（`13` §5：后端返回即记事实，再决定下一步）。
#[derive(Default)]
pub struct GenResult {
    pub outputs: Vec<Json>,
    pub tokens: u64,
    pub cost: f64,
    /// 生成器报的失败（类型与说明，如 `timeout: …`、`malformed: …`）：运行时产出 `Fail` 值、照记账本，
    /// 不当运行期错误（步 15h-1；B149「失败类型进失败位」）。`None` = 成功。
    pub failure: Option<String>,
    /// 端口声明的输出 taint（B149：生成器端口画像 `taint_out` 缺省 `untrusted`）。`None` = 不声明，
    /// 按 B37 缺省 `inherit`（∨ ctx），夹具与确定性枚举器照旧。
    pub taint_out: Option<jpp_value::value::Taint>,
}

pub struct JudgeResult {
    pub answers: Vec<Answer>,
    pub tokens: u64,
    pub cost: f64,
    /// 每条答案用了几个置换。**`mode_share` 不许裸记**——K 是这个测量**身份的一部分**：
    /// K=2 的 1.0 与 K=15 的 1.0 是两个不同的测量。裸记的话改 K 就会把不同 K 下的值
    /// 合进同一格、第二个覆盖第一个，**而它长得像一次观察**（与 `literal_mode` 缺维、
    /// `judge_key` 缺 `site` 同族）。
    ///
    /// 这条的证据就是那个被写错三次的数：最终能定下来**靠的正是原始数据里的 `perms: 2`**
    /// ——没有它，「测出来的 1.0」与「写死的 1.0」到今天还是不可判的。
    pub perms: Vec<usize>,
    /// 每条答案的**置换众数占比**（`12`:151：`select` 的 `Pick` 要求置换众数一致）。
    ///
    /// 缺省 `None` = **这条路上没测过置换**，`cut` 因此不给 `Pick`。**出口不是 `Unsure(tie)`**
    /// ——`tie` 只留给「测了，不一致」；没测过走 J-15 那一位（`Unsure("untested")` +
    /// `Exit.untested = Some("permutation")`），见 `value.rs` 的 `Exit::untested`。
    /// K-noul 路径就是这个情形：它把一道 select 拆成 K 道独立的 noul 再取 argmax，
    /// **那条路上根本没有「置换」这回事**，候选顺序不参与。
    ///
    /// 放在 `JudgeResult` 而不是给 `Answer` 加变体：后者会逼每一处 `match Answer` 都改，
    /// 而那些地方（渲染、验证形状、合并）跟置换无关——**改动面比它该有的大**。
    #[allow(clippy::type_complexity)]
    pub mode_share: Vec<Option<f64>>,
    /// 每条答案随附的**自报置信度**（B154：判断器对本题答案的自报量，`cut` 的 `stat: "confidence"` 读它）。
    /// 空 = 这次调用没报（与 `perms`/`mode_share` 同约定）；逐条 `None` = 这一条没报。不进 `Answer`，理由同
    /// `mode_share`。固定观察端口只在夹具给了时报；真机端口读返回体随 18a 追加项。
    pub confidence: Vec<Option<f64>>,
}

/// 一次效应调用的输入，按 `EffectSpec.input_schema` 的槽形状分三种（不按效应名）。
/// 变体大小不齐（`State` 大）：调用逐个构造、随即交给端口，不成批存放，不装箱。
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum CallInput {
    /// 一个状态、一组题（槽 `state`、`questions`）：一状态多题一次问完（P5）
    StateQuestions {
        state: State,
        questions: Vec<Question>,
    },
    /// 提示、上下文材料内容、份数、轮次序号（槽 `prompt`、`ctx`、`n`、`retry_seq`）
    Prompt {
        prompt: String,
        ctx: Vec<Json>,
        n: usize,
        retry_seq: u64,
    },
    /// 一个状态、一道题（槽 `state`、`question`）
    StateQuestion { state: State, question: Question },
    /// 一份材料、一组题，每题带自己的状态（B155，步 15i）：`states[i]` 是 `questions[i]` 的状态，
    /// 各状态材料哈希（`on`/`ctx`/`ref`）相等、`over` 可不同——同材料上候选集不同的题合成一次调用。
    /// 运行时只在一组题跨多个 `StateHash` 时发它；全组同一状态仍发 [`CallInput::StateQuestions`]。
    /// 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）
    MaterialQuestions {
        states: Vec<State>,
        questions: Vec<Question>,
    },
}

impl CallInput {
    /// 判断输入的逐题 `(状态, 题)`（B155）：两种判断输入都转成这一形；不是判断输入为 `None`。
    pub fn judge_items(&self) -> Option<Vec<(&State, &Question)>> {
        match self {
            CallInput::StateQuestions { state, questions } => {
                Some(questions.iter().map(|q| (state, q)).collect())
            }
            CallInput::MaterialQuestions { states, questions } => {
                Some(states.iter().zip(questions).collect())
            }
            _ => None,
        }
    }
}

/// 一次效应调用：发给哪个实例，带什么输入。
#[derive(Clone, Debug)]
pub struct EffectCall {
    pub instance: EffectInstance,
    pub input: CallInput,
}

/// 端口的返回，按 `EffectSpec.output_shape` 分：读数、材料、人的回答（`None` = 还没答，挂起）。
pub enum EffectOut {
    Readings(JudgeResult),
    Mats(GenResult),
    Answer(Option<Answer>),
}

/// 票据：`submit` 发出，`poll` 凭它取结果。只在发出它的端口内有意义。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

/// 统一效应端口（`20` §2.3）：一个端口服务一个效应实例。
pub trait EffectPort {
    /// 本端口服务的效应实例（I5：模型是解析后的不可变 id）。
    fn instance(&self) -> EffectInstance;
    /// 交出一批调用，按调用顺序返回票据。并发（若有）在端口内做，上限从画像读（步 15e）。
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError>;
    /// 按票据取结果；取走后票据失效。
    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>>;
}

/// 借用的端口也是端口：宿主持有端口对象，把 `&mut` 注册进 [`Ports`]。
impl<T: EffectPort + ?Sized> EffectPort for &mut T {
    fn instance(&self) -> EffectInstance {
        (**self).instance()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        (**self).submit(calls)
    }
    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        (**self).poll(t)
    }
}

impl<T: EffectPort + ?Sized> EffectPort for Box<T> {
    fn instance(&self) -> EffectInstance {
        (**self).instance()
    }
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError> {
        (**self).submit(calls)
    }
    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        (**self).poll(t)
    }
}

/// 当场完成的端口共用的结果缓冲：`submit` 时逐个算出结果，`poll` 取走。
#[derive(Default)]
pub struct Completed {
    next: u64,
    done: BTreeMap<u64, Result<EffectOut, EffectError>>,
}

impl Completed {
    /// 按顺序对每个调用算出结果并登记票据。
    pub fn submit_with(
        &mut self,
        calls: Vec<EffectCall>,
        mut f: impl FnMut(EffectCall) -> Result<EffectOut, EffectError>,
    ) -> Vec<Ticket> {
        calls
            .into_iter()
            .map(|c| {
                let t = self.next;
                self.next += 1;
                let r = f(c);
                self.done.insert(t, r);
                Ticket(t)
            })
            .collect()
    }
    /// 登记一批已算好的结果（并发端口先在内部把一批调用算完，再按提交顺序登记票据；步 15e）。
    pub fn submit_results(&mut self, results: Vec<Result<EffectOut, EffectError>>) -> Vec<Ticket> {
        results
            .into_iter()
            .map(|r| {
                let t = self.next;
                self.next += 1;
                self.done.insert(t, r);
                Ticket(t)
            })
            .collect()
    }

    pub fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>> {
        match self.done.remove(&t.0) {
            Some(r) => Poll::Ready(r),
            None => Poll::Ready(Err(EffectError(format!("票据 {} 不存在或已取走", t.0)))),
        }
    }
}

/// 按效应实例索引的端口表（`20` §2.3 `Ports.effects`）。本版每种效应至多一个实例（B60）。
#[derive(Default)]
pub struct Ports<'a> {
    ports: BTreeMap<EffectInstance, Box<dyn EffectPort + 'a>>,
}

impl<'a> Ports<'a> {
    pub fn new() -> Ports<'a> {
        Ports {
            ports: BTreeMap::new(),
        }
    }

    /// 注册一个端口。同一效应已有端口时报错（本版每种效应一个实例）。
    pub fn register(&mut self, port: Box<dyn EffectPort + 'a>) -> Result<(), EffectError> {
        let inst = port.instance();
        if self.ports.keys().any(|k| k.effect == inst.effect) {
            return Err(EffectError(format!(
                "效应 {} 已注册过端口（本版每种效应一个实例，B60）",
                crate::spec(inst.effect).name
            )));
        }
        self.ports.insert(inst, port);
        Ok(())
    }

    /// 注册一个端口（链式写法）；重复注册是宿主的编程错，直接 panic。
    pub fn with(mut self, port: impl EffectPort + 'a) -> Ports<'a> {
        if let Err(e) = self.register(Box::new(port)) {
            panic!("{}", e.0);
        }
        self
    }

    /// 换掉服务同一效应的端口（没有就注册）。宿主用它把占位的 `gen` 实例换成生成器端口（步 15h-1）。
    pub fn replace(&mut self, port: Box<dyn EffectPort + 'a>) {
        let inst = port.instance();
        self.ports.retain(|k, _| k.effect != inst.effect);
        self.ports.insert(inst, port);
    }

    /// 已注册的实例（按实例序）。
    pub fn instances(&self) -> Vec<EffectInstance> {
        self.ports.keys().cloned().collect()
    }

    /// 服务 `effect` 的实例。
    pub fn instance_of(&self, effect: EffectId) -> Option<EffectInstance> {
        self.ports.keys().find(|k| k.effect == effect).cloned()
    }

    /// 服务 `effect` 的端口。
    pub fn port(&mut self, effect: EffectId) -> Option<&mut (dyn EffectPort + 'a)> {
        self.ports
            .iter_mut()
            .find(|(k, _)| k.effect == effect)
            .map(|(_, p)| &mut **p)
    }

    /// 一批调用交端口一次 `submit`，按票据顺序取回（步 15e：运行时按窗口发出一层，并发在端口内）。
    /// 外层 `Err` 是端口拒收整批；内层逐个调用的成败按提交顺序排列。
    pub fn call_many(
        &mut self,
        effect: EffectId,
        inputs: Vec<CallInput>,
    ) -> Result<Vec<Result<EffectOut, EffectError>>, EffectError> {
        let Some(port) = self.port(effect) else {
            return Err(EffectError(format!(
                "没有端口服务效应 {}：宿主没有为它注册端口",
                crate::spec(effect).name
            )));
        };
        let instance = port.instance();
        let calls = inputs
            .into_iter()
            .map(|input| EffectCall {
                instance: instance.clone(),
                input,
            })
            .collect();
        let ts = port.submit(calls)?;
        Ok(ts
            .iter()
            .map(|t| {
                loop {
                    if let Poll::Ready(r) = port.poll(t) {
                        break r;
                    }
                    // 非阻塞端口（步 15h-1 生成器）要等几秒：让出 CPU，不空转
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            })
            .collect())
    }

    /// 发一个调用并等到结果（本步各端口当场完成，这里是 `submit` 加一次 `poll`）。
    pub fn call(&mut self, effect: EffectId, input: CallInput) -> Result<EffectOut, EffectError> {
        let Some(port) = self.port(effect) else {
            return Err(EffectError(format!(
                "没有端口服务效应 {}：宿主没有为它注册端口",
                crate::spec(effect).name
            )));
        };
        let call = EffectCall {
            instance: port.instance(),
            input,
        };
        let t = port.submit(vec![call])?;
        let t = t
            .into_iter()
            .next()
            .ok_or_else(|| EffectError("端口没有给出票据".into()))?;
        loop {
            if let Poll::Ready(r) = port.poll(&t) {
                return r;
            }
            // 非阻塞端口（步 15h-1 生成器）要等几秒：让出 CPU，不空转
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}
