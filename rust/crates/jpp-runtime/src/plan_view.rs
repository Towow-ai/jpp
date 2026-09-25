//! 运行时给规划钩子的环境视图（步 13a）：`jpp_ir::plan::EnvView` 在运行时环境上的实现。
//!
//! 钩子（`jpp_plan::Hooks`）判「会不会产生效应」时要按名字查值：名字绑定的是内置、方法值还是数据，
//! 方法值的体与捕获环境是什么。这里只把值翻成摘要，不做任何判断（`20` T3：运行时不自行判定许可）。

use super::*;
use jpp_ir::plan::{EnvView, FnView, ValueSummary};

pub(crate) struct RtEnv(pub Env);

impl EnvView for RtEnv {
    fn lookup(&self, name: &str) -> Option<ValueSummary> {
        env_lookup(&self.0, name).map(|v| summary(&v))
    }
}

struct RtFn(Rc<Closure>);

impl FnView for RtFn {
    fn function(&self) -> &Function {
        &self.0.function
    }
    fn identity(&self) -> &str {
        &self.0.hash
    }
    fn env(&self) -> Rc<dyn EnvView> {
        Rc::new(RtEnv(self.0.env.clone()))
    }
}

fn summary(v: &Value) -> ValueSummary {
    match v {
        Value::Builtin(b) => ValueSummary::Builtin(b.to_string()),
        Value::Fn(c) => ValueSummary::Fn(Rc::new(RtFn(c.clone()))),
        Value::List(l) => {
            let l = l.clone();
            ValueSummary::Container(Rc::new(move || l.iter().map(summary).collect()))
        }
        Value::Record(fs) => {
            let fs = fs.clone();
            ValueSummary::Container(Rc::new(move || {
                fs.iter().map(|(_, x)| summary(x)).collect()
            }))
        }
        _ => ValueSummary::Data,
    }
}
