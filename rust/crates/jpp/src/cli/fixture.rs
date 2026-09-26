//! File adapter to the core FixedPorts. No algorithm execution or model calls.
use jpp::{
    effects::{CalibStore, FixedPorts},
    value::{Answer, Mat, Op, Question, State},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Fixture {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub calibrations: Vec<Calibration>,
    #[serde(default)]
    pub observations: Vec<Observation>,
    #[serde(default)]
    pub generations: Vec<Generation>,
    #[serde(default)]
    pub responses: Vec<Response>,
}

#[derive(Deserialize)]
pub struct Generation {
    pub prompt: String,
    pub retry_seq: u64,
    pub output: Vec<Value>,
    /// 可选：这份输出只对这组上下文成立（步 4d）。缺省 = 不看上下文（改前行为）
    #[serde(default)]
    pub ctx: Option<Vec<Value>>,
}

#[derive(Deserialize)]
pub struct Response {
    #[serde(flatten)]
    pub question: Observation,
}

#[derive(Deserialize)]
pub struct Calibration {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    pub status: String,
    /// 这条线的 δ（步 15d-2：夹具线显式声明；缺则该线出口 `Unsure(untested)`，载体 `Delta`）
    #[serde(default)]
    pub delta: Option<f64>,
}

#[derive(Deserialize)]
pub struct Observation {
    pub on: Vec<Value>,
    #[serde(default)]
    pub ctx: Vec<Value>,
    #[serde(default)]
    pub r#ref: Vec<Value>,
    #[serde(default)]
    pub over: Vec<Value>,
    pub op: String,
    pub text: String,
    pub calib: String,
    #[serde(default)]
    pub scale: Vec<String>,
    pub answer: Answer,
    /// 可选：这条 select 观察测过的置换数与众数占比（步 4d）。缺省 = 没测过置换
    #[serde(default)]
    pub perms: Option<usize>,
    #[serde(default)]
    pub mode_share: Option<f64>,
    /// 可选：判断器随这条答案报的自报置信度（B154，步 20j-3；`cut` 的 `stat: "confidence"` 读它）。缺省 = 没报，
    /// 桥按 p_max 取夹具缺省；给了就进账本判断条目，重放照取。由 `scripts/fixture_from_ledger.py` 从真机账本写。
    #[serde(default)]
    pub confidence: Option<f64>,
    /// 可选：是非题的答案标签 `{yes, no}`（B155，步 15i）。题带标签时观察键追加它；缺省与步 15i 前相同
    #[serde(default)]
    pub labels: Option<jpp::value::TestLabels>,
}

impl Observation {
    fn checked_question(&self) -> Result<Question, String> {
        let (op, expected, compatible) = match self.op.as_str() {
            "test" => (Op::Test, "Noul", matches!(&self.answer, Answer::Noul(_))),
            "select" => (
                Op::Select,
                "Choice",
                matches!(&self.answer, Answer::Choice(_)),
            ),
            "measure" => (
                Op::Measure,
                "Score",
                matches!(&self.answer, Answer::Score(_)),
            ),
            other => return Err(format!("unknown fixture question kind '{other}'")),
        };
        if !compatible {
            return Err(format!(
                "fixture {} question {:?} requires a {expected} answer",
                self.op, self.text
            ));
        }
        Ok(
            Question::new(op, &self.text, &self.calib, self.scale.clone())
                .with_labels(self.labels.clone()),
        )
    }
}

impl Fixture {
    pub fn build(&self) -> Result<(FixedPorts, CalibStore), String> {
        let mut client = FixedPorts::new();
        let mut calibrations = CalibStore::new();
        for c in &self.calibrations {
            calibrations.put(&c.key, c.hi, c.lo, c.n, &c.status, c.delta)?;
        }
        let mats = |items: &[Value]| items.iter().cloned().map(Mat::literal).collect();
        for o in &self.observations {
            let state = State::new(
                mats(&o.on),
                mats(&o.ctx),
                mats(&o.r#ref),
                mats(&o.over),
                false,
            );
            let question = o.checked_question()?;
            let key = client.observe(&state, &question, o.answer.clone());
            if let Some(c) = o.confidence {
                if !(0.0..=1.0).contains(&c) {
                    return Err(format!(
                        "fixture observation {:?}: confidence must be in [0, 1], got {c}",
                        o.text
                    ));
                }
                client.fix_confidence(&key, c);
            }
            match (o.perms, o.mode_share) {
                (Some(k), Some(s)) => client.fix_perms(&key, k, s),
                (None, None) => {}
                _ => {
                    return Err(format!(
                        "fixture observation {:?}: perms and mode_share go together",
                        o.text
                    ));
                }
            }
        }
        for g in &self.generations {
            match &g.ctx {
                Some(ctx) => client.fix_gen_ctx(&g.prompt, g.retry_seq, ctx, g.output.clone()),
                None => client.fix_gen(&g.prompt, g.retry_seq, g.output.clone()),
            }
        }
        for r in &self.responses {
            let o = &r.question;
            let state = State::new(
                mats(&o.on),
                mats(&o.ctx),
                mats(&o.r#ref),
                mats(&o.over),
                false,
            );
            let question = o.checked_question()?;
            client.fix_ask(&state, &question, Some(o.answer.clone()));
        }
        Ok((client, calibrations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_question_kinds_validate_observation_and_response_variants() {
        for field in ["observations", "responses"] {
            for (op, expected) in [("test", "Noul"), ("select", "Choice"), ("measure", "Score")] {
                for (variant, answer) in [
                    ("Noul", json!({"Noul":0.9})),
                    ("Choice", json!({"Choice":[0.9,0.1]})),
                    ("Score", json!({"Score":[0.9,0.1]})),
                ] {
                    let fixture: Fixture = serde_json::from_value(json!({field: [{"on":["x"],"op":op,"text":"q","calib":"c","answer":answer}]})).unwrap();
                    assert_eq!(
                        fixture.build().is_ok(),
                        variant == expected,
                        "{field}: {op}/{variant}"
                    );
                }
            }
        }
    }
}
