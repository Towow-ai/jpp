//! 规则单元：每条规则一个文件，全部在 [`RULES`] 一处注册（`20` §2.3 `jpp-check`，D5.3）。
//!
//! 新增一条规则 = 新增 `rules/xNN.rs`，在文件里写一个 `RULE`，再在 [`RULES`] 加一行；分析器
//! （`analysis/`：名字与读数、语法遍历、效应行）不动。规则经 [`Hooks`] 挂到三处：分析前的整程序
//! 检查、分析器两趟遍历上的钩子点、分析后的整程序检查。
//!
//! **表序有意义。** 诊断最后按跨度稳定排序，同一跨度上的先后就是发出先后；同一钩子点按表序调用。
//! 表序照搬拆分前的发出顺序（步 12b）：预算 J-07、J-10、B32 在分析前；名字趟的调用点上 J-05
//! （Fn¹ 交给高阶）先于 J-01（读数入槽）；语法趟的 `loop`/`iterate` 上 J-06 先于 E7；分析后
//! J-12、B13。

use crate::*;

mod b13;
mod b32;
mod b52;
mod b76;
mod e07;
mod j01;
mod j03;
mod j04;
mod j05;
mod j06;
mod j07;
mod j08;
mod j09;
mod j10;
mod j11;
mod j12;
mod j13;
mod j14;
pub(crate) mod sites;

pub(crate) use sites::SiteFacts;

/// 全部规则单元，按发出顺序。
pub(crate) const RULES: &[&Rule] = &[
    &j07::RULE,
    &j10::RULE,
    &b32::RULE,
    &j05::RULE,
    &j01::RULE,
    &j03::RULE,
    &j04::RULE,
    &j06::RULE,
    &e07::RULE,
    &j13::RULE,
    &j14::RULE,
    &j12::RULE,
    &b13::RULE,
    &b76::RULE,
    &j08::RULE,
    &j09::RULE,
    &j11::RULE,
    &b52::RULE,
];

pub(crate) struct Rule {
    /// 规则号（文件头写依据条文）
    pub code: &'static str,
    /// 规则读哪些分析结果（`20` §2.3 `Rule::requires`，B71）。钩子点必须在所需分析之后：
    /// 名字趟上的钩子点收到的就是名字分析正在给出的视图，算「在其后」。
    pub requires: &'static [AnalysisId],
    pub hooks: Hooks,
}

/// `requires` 的取值（B71）：规则读的分析结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalysisId {
    /// 名字趟：作用域、绑定、值类别（读数）、这一轮会变的名字
    Names,
    /// 效应趟：效应行不动点、Φ（本版没有规则读它；第一条读它的规则随步 13b 或 24 进来）
    #[allow(dead_code)]
    Effects,
}

/// 规则的读面（`20` §2.3 `Cx`，B71）：IR（含站点表）、`check` 的输入、分析结果的只读视图。
/// 规则只收 `&Cx`（共享借用），改不到分析状态，也看不见彼此的诊断：`Checker` 对 `rules/` 不可达。
#[derive(Clone, Copy)]
pub(crate) struct Cx<'a> {
    pub p: &'a Program,
    /// 站点表给出的事实（所属函数、最近外层站点、方法位）：从 IR 读，不从表达式形状重算
    pub sites: &'a SiteFacts<'a>,
    pub profile: Option<&'a jpp_effects::Profile>,
    pub calib: Option<&'a dyn CalibView>,
    /// 宿主动作表（B108，步 24-0）：`Session` 执行前那次检查才有；`None` = 检查时不知动作表
    pub actions: Option<&'a crate::ActionTable>,
    /// 名字分析的只读视图：名字趟的钩子点收到正在进行的视图，其余钩子点为 `None`
    /// （本版名字趟之外的钩子点没有规则声明 `Names` 以外的读法：J-13 的「这一轮会变的名字」随
    /// [`CallSite::iter_params`] 给出）
    pub names: Option<NamesAt<'a>>,
}

/// 名字分析在某个钩子点上的只读视图（B71：分析结果经 `Cx` 给规则）。
#[derive(Clone, Copy)]
pub(crate) struct NamesAt<'a> {
    pub readings: &'a HashSet<*const Expr>,
    pub scopes: &'a [Scope],
}

impl NamesAt<'_> {
    /// 这个位置上的值是读数吗？只穿过列表 / 记录字面量（与名字趟同一口径）。
    pub(crate) fn is_reading(&self, e: &Expr) -> bool {
        is_reading_in(self.readings, e)
    }
    /// 这个名字在此处的类型标注
    pub(crate) fn annotation(&self, name: &str) -> Option<Type> {
        annotation_in(self.scopes, name)
    }
}

/// 语法趟上的一次内置调用。
pub(crate) struct CallSite<'a> {
    pub name: &'a str,
    pub args: &'a [&'a Expr],
    pub span: Span,
    /// 这次调用所在的最内层函数（IR 节点）；顶层为 `None`。与站点表 `SiteInfo.function` 同口径，
    /// 无站点的调用（例如 `stop`）也有
    pub function: Option<jpp_ir::key::NodeId>,
    /// 这一轮里会变的名字（名字分析的一部分，见 `analysis/syntax.rs`）
    pub iter_params: &'a [String],
}

type Diags = Vec<Diagnostic>;
type ReadingHook = fn(&Cx, Span, &str, bool) -> Diags;
type ScanCallHook = fn(&Cx, &str, &[&Expr]) -> Diags;
type BindHook = fn(&Cx, &Expr, &str, Span, &Block) -> Diags;

pub(crate) struct Hooks {
    /// 名字、语法、效应分析之前的整程序检查
    pub before: Option<fn(&Cx) -> Diags>,
    /// 分析之后的整程序检查
    pub after: Option<fn(&Cx) -> Diags>,
    /// 名字趟：一个读数被放到了要比、要算、要当条件的位置（跨度、说明、是否归 H5 管的算术）
    pub reading: Option<ReadingHook>,
    /// 名字趟：一次解析成内置的调用（内置名、实参）；作用域与读数在 `cx.names`
    pub scan_call: Option<ScanCallHook>,
    /// 语法趟：一次内置调用
    pub call: Option<fn(&Cx, &CallSite) -> Diags>,
    /// 语法趟：块里的一个 `let`（值、名字、跨度、所在块）
    pub bind: Option<BindHook>,
    /// 语法趟：语句位置的表达式
    pub stmt: Option<fn(&Cx, &Expr) -> Diags>,
    /// 语法趟：一个函数体走完
    pub function: Option<fn(&Cx, &Function) -> Diags>,
}

impl Hooks {
    pub const NONE: Hooks = Hooks {
        before: None,
        after: None,
        reading: None,
        scan_call: None,
        call: None,
        bind: None,
        stmt: None,
        function: None,
    };

    /// 挂了钩子的阶段：0 分析前，1 名字趟，2 语法趟，3 分析后
    #[cfg(test)]
    fn phases(&self) -> Vec<u8> {
        let mut v = vec![];
        if self.before.is_some() {
            v.push(0);
        }
        if self.reading.is_some() || self.scan_call.is_some() {
            v.push(1);
        }
        if self.call.is_some()
            || self.bind.is_some()
            || self.stmt.is_some()
            || self.function.is_some()
        {
            v.push(2);
        }
        if self.after.is_some() {
            v.push(3);
        }
        v
    }
}

/// 分析结果在哪个阶段起可用：名字在名字趟（1）起，效应在分析后（3）起
#[cfg(test)]
fn available_from(a: AnalysisId) -> u8 {
    match a {
        AnalysisId::Names => 1,
        AnalysisId::Effects => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一处注册：规则号不重复
    #[test]
    fn codes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for r in RULES {
            assert!(seen.insert(r.code), "规则 {} 注册了两次", r.code);
        }
    }

    /// 钩子点在所需分析之后（B71）：声明读效应分析的规则不能挂在效应分析结束之前
    #[test]
    fn hooks_after_required_analyses() {
        for r in RULES {
            let need = r
                .requires
                .iter()
                .map(|a| available_from(*a))
                .max()
                .unwrap_or(0);
            for ph in r.hooks.phases() {
                assert!(
                    ph >= need,
                    "规则 {} 的钩子点（阶段 {ph}）早于它声明读的分析（阶段 {need}）",
                    r.code
                );
            }
        }
    }

    /// 挂在名字趟上的规则必须声明 `Names`：它收到的就是名字分析的视图
    #[test]
    fn names_hooks_declare_names() {
        for r in RULES {
            if r.hooks.phases().contains(&1) {
                assert!(
                    r.requires.contains(&AnalysisId::Names),
                    "规则 {} 挂在名字趟却没声明 Names",
                    r.code
                );
            }
        }
    }

    /// `rules/` 目录引用不到 `Checker`（B71：只读由借用类型保证，不靠约定）
    #[test]
    fn rules_do_not_name_checker() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rules");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&path).unwrap();
            // 类型名拼开写，免得这条测试自己命中
            let ty = ["Chec", "ker"].concat();
            let hit = [
                format!("impl {ty}"),
                format!("{ty}<"),
                format!("{ty} {{"),
                format!("{ty})"),
                format!(": {ty}"),
                format!("&{ty}"),
                format!("&mut {ty}"),
            ]
            .into_iter()
            .find(|p| text.contains(p.as_str()));
            assert!(hit.is_none(), "rules/{name} 引用了分析器类型：规则只收 &Cx");
        }
    }
}
