//! 替身判断器 `stub`（步 15g，`20` §五 S1 的第二个判断器）：不走网络、不开 `live` 特性，CI 覆盖。
//!
//! 读数由 `(模型 id, 状态哈希, 题型, 题面, 候选或档位下标)` 的哈希确定：同一输入永远同一读数，不同输入
//! 在 [0, 1] 上散开。它不模仿任何真实判断器的性质，只用来检验「换判断器程序不改」这条路径：换了模型 id，
//! 账本键、画像、校准查找都要跟着换（`12` B60）。画像见 `tests/profile_swap/stub-0.json`（替身，按构造，
//! 非测量；不放发行目录 `profiles/`，主会话 2026-09-25 确认）。
//!
//! 与 `JevPorts` 同形：判断走闭包端口；不生成（`gen` 实例每次报错）；没有人工通道（`ask` 每次「还没答」）。

use serde_json::json;

use crate::effects::{EffectError, EffectPort, FnPort, JudgeResult, Ports, Profile};
use crate::value::{Answer, Op, Question, State};
use jpp_effects::EffectInstance;
use jpp_effects::builtin_ports::{UnansweredPort, UnservedPort};

/// 注册表条目（步 15g）
pub const SPEC: super::BackendSpec = super::BackendSpec {
    name: "stub",
    default_model: "stub-0",
    mode_label: "stub judge backend (deterministic, no model API requests)",
    transport: false,
    calib: false,
    build,
};

fn build(model: &str, _profile: &Profile) -> Result<Box<dyn super::BackendPorts>, String> {
    Ok(Box::new(StubPorts::new(model)))
}

/// `stub` 不生成：`gen` 实例的报错
const STUB_NO_GEN: &str = "stub 判断器不生成：gen 用生成器端口或夹具";

/// `(键…)` 的哈希映到 [0, 1]
fn 散(parts: serde_json::Value) -> f64 {
    let h = jpp_effects::hash16(&parts);
    u64::from_str_radix(&h, 16).unwrap_or_default() as f64 / u64::MAX as f64
}

/// 一组权重归一成概率（全为 0 时均分）
fn 归一(w: Vec<f64>) -> Vec<f64> {
    let s: f64 = w.iter().sum();
    if s > 0.0 {
        w.iter().map(|x| x / s).collect()
    } else {
        let n = w.len().max(1) as f64;
        w.iter().map(|_| 1.0 / n).collect()
    }
}

fn 判(model: &str, state: &State, questions: &[&Question]) -> Result<JudgeResult, EffectError> {
    let answers = questions
        .iter()
        .map(|q| {
            let 键 = |i: usize| 散(json!([model, state.hash, q.op.phys(), q.text, i]));
            match q.op {
                Op::Test => Answer::Noul(键(0)),
                Op::Select => Answer::Choice(归一((0..state.over.len()).map(键).collect())),
                Op::Measure => Answer::Score(归一((0..q.scale.len()).map(键).collect())),
            }
        })
        .collect();
    Ok(JudgeResult {
        answers,
        tokens: 0,
        cost: 0.0,
        mode_share: vec![None; questions.len()],
        perms: vec![0; questions.len()],
        confidence: vec![],
    })
}

/// 替身端口表
pub struct StubPorts {
    model: String,
    judge: FnPort<'static>,
    rest: Vec<Box<dyn EffectPort>>,
}

impl StubPorts {
    pub fn new(model: &str) -> StubPorts {
        let m = model.to_string();
        let judge = FnPort::judge(model, move |s: &State, qs: &[&Question]| 判(&m, s, qs));
        let rest = jpp_effects::PORTED
            .iter()
            .map(|e| (*e, jpp_effects::spec(*e)))
            .filter(|(_, s)| !s.produces_reading)
            .map(|(effect, s)| {
                let instance = EffectInstance {
                    effect,
                    model: model.to_string(),
                };
                if s.output_shape == jpp_effects::OutputShape::Answer {
                    Box::new(UnansweredPort::new(instance)) as Box<dyn EffectPort>
                } else {
                    Box::new(UnservedPort::new(instance, STUB_NO_GEN))
                }
            })
            .collect();
        StubPorts {
            model: model.to_string(),
            judge,
            rest,
        }
    }
}

impl super::BackendPorts for StubPorts {
    fn ports(&mut self) -> Ports<'_> {
        let mut p = Ports::new().with(&mut self.judge);
        for r in self.rest.iter_mut() {
            p = p.with(&mut **r);
        }
        p
    }
    fn model_id(&self) -> String {
        self.model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Mat;

    /// 同一输入同一读数；test 与 select 形状合法（概率和为 1）；换模型 id 读数跟着换
    #[test]
    fn 读数确定且形状合法() {
        let s = State::new(
            vec![Mat::literal(json!("材料"))],
            vec![],
            vec![],
            vec![Mat::literal(json!("甲")), Mat::literal(json!("乙"))],
            false,
        );
        let t = Question::new(Op::Test, "行吗", "k", vec![]);
        let c = Question::new(Op::Select, "哪个", "k2", vec![]);
        let a = 判("stub-0", &s, &[&t, &c]).unwrap();
        let b = 判("stub-0", &s, &[&t, &c]).unwrap();
        assert_eq!(format!("{:?}", a.answers), format!("{:?}", b.answers));
        let Answer::Noul(p) = a.answers[0] else {
            panic!("test 给 Noul")
        };
        assert!((0.0..=1.0).contains(&p));
        let Answer::Choice(v) = &a.answers[1] else {
            panic!("select 给 Choice")
        };
        assert_eq!(v.len(), 2);
        assert!((v.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        let o = 判("stub-1", &s, &[&t]).unwrap();
        assert_ne!(format!("{:?}", o.answers[0]), format!("{:?}", a.answers[0]));
    }
}
