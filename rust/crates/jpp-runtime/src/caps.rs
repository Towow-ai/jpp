//! 内核构造的准入闸门：`ConstructSpec` 注册表与能力令牌（`20` v2 §五 S3、附录 B57；`21` 步 25-2、25-2b、25-2c）。
//!
//! 一个构造只有在它必须使用 `.jpp` 拿不到的内核能力、或是批调度的刷新点时才进内核（B57、B138）。能力类是
//! 依据文本（B139，`12` §5 B57 注下的表）：十二类令牌（[`Privilege`]）加一个不发令牌的闸门项「刷新点」
//! （`ConstructSpec.refresh`）。加类、并类、撤类、改某类的令牌 API 面都须先改那张表。构造用到
//! 这些能力的 API 都要一个令牌 [`Cap`]；令牌只由本模块按 `ConstructSpec.privileges` 造出（`Cap` 的字段对本
//! 模块私有，构造文件在 `constructs/` 下，不是本模块的子模块，造不出令牌）。构造文件不许绕过令牌直接碰这些
//! API：`scripts/grep_privileges.py` 在失败模式下核（同一 crate 里 Rust 的可见性挡不住）。用了没声明的能力
//! 在运行时 panic（内核不变量）。一个构造借另一个构造的能力，只能经本模块的入口调用那个构造（`调合成`、
//! `调元素`），入口按被调构造的声明造令牌。
//!
//! 依据：B57（地基/20-架构方案-v2.md 附录 A B57；12 §5 B57 注）；B131–B133（地基/附注/2026-09-25-库层出口合成与待补批3裁定.md）；
//! 地基/过程记录/工程-步25-2.md、工程-步25-2b.md

use super::*;
use crate::constructs::compose::{self, 分量, 合成请求, 规则};
use crate::constructs::element::元素上下文;
use std::marker::PhantomData;

/// 内核能力（B57 四类，步 25-2b 按 B131–B133 细分）。构造在 [`ConstructSpec::privileges`] 里声明它要哪几类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privilege {
    /// 直接读答案：`answer_of`、答案表、按读数排序
    ReadAnswer,
    /// 签发读数与其答案（`fit`、`repeat` 的结果读数）
    IssueReading,
    /// 签发未决出口（只能 Unsure；`sieve` 的预算未观察，步 25-11 后随 22-0 由 `cut` 给出而撤）
    IssueUnsure,
    /// 签发出口，限合成：种类由封闭规则从分量算出（B131）
    IssueComposite,
    /// 签发题值：`Question`/`Form` 及其 taint、`sources`、`from_key` 的初始化（B138）
    IssueQuestion,
    /// 责任表：出口的消费标记
    Duty,
    /// 来源写，限选择边（B133、B92）
    SourceSelect,
    /// 报告 `exits` 行（B133、B120 (b)）
    ExitRow,
    /// 循环上下文：本循环内账本键重复判定（B132、J-06）
    LoopContext,
    /// 账本键收集：本段记账位置与其间的判断键（B132）
    KeyCollect,
    /// 读数表（读）：账本条目、缺席账停发标记（B139，由 25-2b 的 `Readings` 拆出）
    LedgerRead,
    /// 读数表（写）与 trace 事件（B139；持写权者也可读它写的账本）
    LedgerWrite,
}

impl Privilege {
    pub fn name(self) -> &'static str {
        match self {
            Privilege::ReadAnswer => "ReadAnswer",
            Privilege::IssueReading => "IssueReading",
            Privilege::IssueUnsure => "IssueUnsure",
            Privilege::IssueComposite => "IssueComposite",
            Privilege::IssueQuestion => "IssueQuestion",
            Privilege::Duty => "Duty",
            Privilege::SourceSelect => "SourceSelect",
            Privilege::ExitRow => "ExitRow",
            Privilege::LoopContext => "LoopContext",
            Privilege::KeyCollect => "KeyCollect",
            Privilege::LedgerRead => "LedgerRead",
            Privilege::LedgerWrite => "LedgerWrite",
        }
    }
}

/// 能力标记类型（令牌的类型参数）
pub(crate) struct ReadAnswer;
pub(crate) struct IssueReading;
pub(crate) struct IssueUnsure;
pub(crate) struct IssueComposite;
pub(crate) struct IssueQuestion;
pub(crate) struct Duty;
pub(crate) struct SourceSelect;
pub(crate) struct ExitRow;
pub(crate) struct LoopContext;
pub(crate) struct KeyCollect;
pub(crate) struct LedgerRead;
pub(crate) struct LedgerWrite;

/// 能力令牌：字段私有，只有本模块能造。
pub(crate) struct Cap<P> {
    _only_registry: PhantomData<P>,
}

impl<P> Cap<P> {
    fn mint() -> Cap<P> {
        Cap {
            _only_registry: PhantomData,
        }
    }
}

/// 一次构造调用拿到的令牌包：声明了哪类能力，哪一格才有令牌。
pub(crate) struct Caps {
    who: &'static str,
    read_answer: Option<Cap<ReadAnswer>>,
    issue_reading: Option<Cap<IssueReading>>,
    issue_unsure: Option<Cap<IssueUnsure>>,
    issue_composite: Option<Cap<IssueComposite>>,
    issue_question: Option<Cap<IssueQuestion>>,
    duty: Option<Cap<Duty>>,
    source_select: Option<Cap<SourceSelect>>,
    exit_row: Option<Cap<ExitRow>>,
    loop_context: Option<Cap<LoopContext>>,
    key_collect: Option<Cap<KeyCollect>>,
    ledger_read: Option<Cap<LedgerRead>>,
    ledger_write: Option<Cap<LedgerWrite>>,
}

fn 未声明(who: &str, p: Privilege) -> ! {
    panic!(
        "内核构造 {who} 用了没有在 ConstructSpec.privileges 声明的能力 {}（B57：能力只经注册表按声明发放）",
        p.name()
    )
}

macro_rules! 取令牌 {
    ($($f:ident: $t:ident),* $(,)?) => {
        $(
            pub(crate) fn $f(&self) -> &Cap<$t> {
                self.$f
                    .as_ref()
                    .unwrap_or_else(|| 未声明(self.who, Privilege::$t))
            }
        )*
    };
}

impl Caps {
    fn for_spec(s: &ConstructSpec) -> Caps {
        let has = |p| s.privileges.contains(&p);
        Caps {
            who: s.name,
            read_answer: has(Privilege::ReadAnswer).then(Cap::mint),
            issue_reading: has(Privilege::IssueReading).then(Cap::mint),
            issue_unsure: has(Privilege::IssueUnsure).then(Cap::mint),
            issue_composite: has(Privilege::IssueComposite).then(Cap::mint),
            issue_question: has(Privilege::IssueQuestion).then(Cap::mint),
            duty: has(Privilege::Duty).then(Cap::mint),
            source_select: has(Privilege::SourceSelect).then(Cap::mint),
            exit_row: has(Privilege::ExitRow).then(Cap::mint),
            loop_context: has(Privilege::LoopContext).then(Cap::mint),
            key_collect: has(Privilege::KeyCollect).then(Cap::mint),
            ledger_read: has(Privilege::LedgerRead).then(Cap::mint),
            ledger_write: has(Privilege::LedgerWrite).then(Cap::mint),
        }
    }
    取令牌! {
        read_answer: ReadAnswer,
        issue_reading: IssueReading,
        issue_unsure: IssueUnsure,
        issue_composite: IssueComposite,
        issue_question: IssueQuestion,
        duty: Duty,
        source_select: SourceSelect,
        exit_row: ExitRow,
        loop_context: LoopContext,
        key_collect: KeyCollect,
        ledger_read: LedgerRead,
        ledger_write: LedgerWrite,
    }
}

// ---------- 令牌上的 API：只转调现有的内部实现，不改语义 ----------

impl Cap<ReadAnswer> {
    pub(crate) fn answer_of(&self, it: &Interp, r: &Reading) -> Option<Answer> {
        it.answer_of(r)
    }
    pub(crate) fn answers<'b>(&self, it: &'b Interp) -> std::cell::Ref<'b, AnswerTable> {
        it.answers.borrow()
    }
}

impl Cap<IssueReading> {
    pub(crate) fn new_reading_id(&self, it: &Interp) -> u64 {
        it.new_reading_id()
    }
    pub(crate) fn fill_answer(&self, it: &Interp, r: &Reading, a: Answer) {
        it.fill_answer(r, a)
    }
}

impl Cap<IssueUnsure> {
    /// 签发一个未决出口：种类恒为 `Unsure(cause)`，这个令牌造不出已决出口
    pub(crate) fn new_unsure(
        &self,
        it: &mut Interp,
        cause: &str,
        op: Op,
        q_hash: &str,
        taint: Taint,
        sp: Span,
    ) -> Value {
        it.new_exit(
            ExitKind::Unsure(cause.into()),
            None,
            op,
            q_hash,
            "",
            taint,
            sp,
        )
    }
}

impl Cap<IssueComposite> {
    /// 签发合成出口（B131）：种类由封闭规则从分量算出，调用者给不了种类；taint 取有出口的分量之 ∨；
    /// 分量出口记进 `parts`；等级按冷线记——合成出口的放行派生在步 25-9 落，此前不作放行证据（步 25-1）。
    pub(crate) fn issue(
        &self,
        it: &mut Interp,
        r: &规则,
        分量: &[分量],
        op: Op,
        q_hash: &str,
        sp: Span,
    ) -> Result<Value, String> {
        let 种类: Vec<ExitKind> = 分量.iter().map(|f| f.种类.clone()).collect();
        let kind = compose::合成种类(r, &种类)?;
        let parts: Vec<Rc<Exit>> = 分量.iter().filter_map(|f| f.出口.clone()).collect();
        let taint = parts
            .iter()
            .fold(Taint::Trusted, |t, e| Taint::join(t, e.taint));
        let x = it.new_exit(kind, None, op, q_hash, "", taint, sp);
        if let Value::Exit(e) = &x {
            e.grade.set(Some(LineGrade::Cold));
            *e.parts.borrow_mut() = parts;
        }
        Ok(x)
    }
}

impl Cap<Duty> {
    /// 出口记为已消费，并写消费者（责任表）
    pub(crate) fn settle(&self, e: &Exit, by: &str) {
        e.consumed.set(true);
        *e.consumed_by.borrow_mut() = by.into();
    }
}

impl Cap<SourceSelect> {
    /// 给值并入选择依赖边 {出口的账本键}（B92：选择边只增加依赖、不进 J-02）；出口没有账本键则原样返回
    pub(crate) fn select_edge(&self, v: Value, e: &Exit) -> Value {
        let 选键 = Provenance::sources_only(Sources::select(&e.ledger_key.borrow(), &e.q_hash));
        v.with_prov(&选键)
    }
}

impl Cap<ExitRow> {
    /// 给出口在报告 `exits` 表里的那一行写元素编号（B120 (b)）；出口没有行（不来自 `cut`）则不写
    pub(crate) fn stamp_row(&self, it: &mut Interp, exit_id: usize, index: i64, pos: usize) {
        if let Some(&i) = it.exit_rows.get(&exit_id) {
            if let Some(row) = it.exit_grades.get_mut(i) {
                row["index"] = serde_json::json!(index);
                row["pos"] = serde_json::json!(pos);
            }
        }
    }
}

impl Cap<LoopContext> {
    pub(crate) fn loops<'b>(&self, it: &'b mut Interp) -> &'b mut Vec<LoopCtx> {
        &mut it.loops
    }
}

impl Cap<KeyCollect> {
    pub(crate) fn mark(&self, it: &Interp) -> (usize, u64, f64) {
        it.mark()
    }
    pub(crate) fn since(&self, it: &Interp, m: (usize, u64, f64)) -> (Vec<Value>, (i64, f64)) {
        it.since(m)
    }
}

impl Cap<LedgerRead> {
    pub(crate) fn ledger<'b>(&self, it: &'b Interp) -> &'b Ledger {
        &*it.ledger
    }
    /// 缺席账的停发标记（B93，步 22-0：预算停发的读数标 `budget`）
    pub(crate) fn absent_mark<'b>(&self, it: &'b Interp, key: &str) -> Option<&'b str> {
        it.absent_marks.get(key).map(String::as_str)
    }
}

impl Cap<LedgerWrite> {
    /// 账本的写入口（持写权者也可经它读）
    pub(crate) fn ledger_mut<'b>(&self, it: &'b mut Interp) -> &'b mut Ledger {
        &mut *it.ledger
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trace_event(
        &self,
        it: &mut Interp,
        kind: &str,
        key: &str,
        replayed: bool,
        cost: f64,
        site: Span,
        note: String,
    ) {
        it.trace.push(kind, key, replayed, cost, site, note)
    }
}

/// 还没签发的题式（`Form::new` 的结果）：字段只有本模块能写，`finish_form` 写完声明后才成为题值（B138）。
pub(crate) struct 未完成题式(jpp_value::value::Form);

impl Cap<IssueQuestion> {
    /// `test`/`select` 的题：带决定性证据槽、前提、请求与置换声明（B64）
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn question(
        &self,
        op: Op,
        text: &str,
        calib: &str,
        evidence: Vec<String>,
        presupposition: Option<String>,
        request: Option<String>,
        permute: bool,
    ) -> Value {
        let mut q = Question::with_evidence(op, text, calib, vec![], evidence);
        q.presupposition = presupposition;
        q.request = request;
        q.permute = permute;
        Value::Question(Rc::new(q))
    }
    /// `measure` 的题（刻度）
    pub(crate) fn measure_question(&self, text: &str, calib: &str, scale: Vec<String>) -> Value {
        Value::Question(Rc::new(Question::new(Op::Measure, text, calib, scale)))
    }
    /// 题式的第一步：解析模板与哈希（`Form::new` 的错在这里报，先于置换与 `over_kind` 的错）
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn form(
        &self,
        op: Op,
        template: &str,
        calib: &str,
        scale: Vec<String>,
        evidence: Vec<String>,
        presupposition: Option<String>,
        request: Option<String>,
    ) -> Result<未完成题式, String> {
        jpp_value::value::Form::new(
            op,
            template,
            calib,
            scale,
            evidence,
            presupposition,
            request,
        )
        .map(未完成题式)
    }
    /// 题式的第二步：写置换声明（B64，不进 `form_hash`）与 `over_kind`（B76，只决定题类）
    pub(crate) fn finish_form(
        &self,
        f: 未完成题式,
        permute: bool,
        over_kind: Option<jpp_value::value::OverKind>,
    ) -> Value {
        let mut f = f.0;
        f.permute = permute;
        f.over_kind = over_kind;
        Value::Form(Rc::new(f))
    }
    /// 填题式：题面 taint = 模板 ∨ 各填入值（B58），由调用者算好传入
    pub(crate) fn fill_form(
        &self,
        f: &jpp_value::value::Form,
        fill: &[(String, String)],
        taint: Taint,
    ) -> Result<Value, String> {
        let mut q = f.fill(fill)?;
        q.taint = taint;
        Ok(Value::Question(Rc::new(q)))
    }
}

// ---------- 注册表 ----------

type Run = for<'a, 'b> fn(&'b mut Interp<'a>, &Caps, &'static str, Vec<Value>, Span) -> R<Value>;

/// 一个内核构造的规格（`20` v2 §2.3 `constructs/*.rs` 行、§五 S3）。
pub struct ConstructSpec {
    /// 源码里的名字
    pub name: &'static str,
    /// 调用形式（一行，`describe` 用）
    pub params: &'static str,
    /// 返回什么
    pub returns: &'static str,
    /// 构造入口的刷新点（`readings.rs::REFRESH_POINTS` 里的原因）；空 = 不是刷新点
    pub refresh: &'static [&'static str],
    /// 用到的内核能力（B57）；空 = 按 B57 没有理由待在内核（见 `过程记录/工程-步25-2.md` Q11）
    pub privileges: &'static [Privilege],
    /// 依据条文
    pub clause: &'static str,
    /// `.jpp` 可调的构造带实现入口；`None` = 内部构造，只经本模块的入口被别的构造调用（步 25-2b：`compose`、
    /// `element` 暂不开放成 `.jpp` 名字，`过程记录/工程-步25-2b.md` Q16）
    run: Option<Run>,
}

impl ConstructSpec {
    /// 是否开放成 `.jpp` 名字（在 `BUILTINS` 里、可按名字调用）
    pub fn exposed(&self) -> bool {
        self.run.is_some()
    }
}

use Privilege::{
    Duty as P_DUTY, ExitRow as P_ROW, IssueComposite as P_COMPOSITE, IssueQuestion as P_QUESTION,
    IssueReading as P_READING, IssueUnsure as P_UNSURE, KeyCollect as P_KEYS,
    LedgerRead as P_LREAD, LedgerWrite as P_LWRITE, LoopContext as P_LOOP, ReadAnswer as P_READ,
    SourceSelect as P_SELECT,
};

static CONSTRUCTS: &[ConstructSpec] = &[
    // 依据：12 §2.2（题的三种题型）；B64（置换声明）
    ConstructSpec {
        name: "test",
        params: "test(题面: Text, calib: Text[, {evidence?, presupposition?, request?}])",
        returns: "题（是非）",
        refresh: &[],
        privileges: &[P_QUESTION],
        clause: "12 §2.2",
        run: Some(|it, c, n, a, s| it.b_test(c, n, a, s)),
    },
    // 依据：12 §2.2；B64
    ConstructSpec {
        name: "select",
        params: "select(题面: Text, calib: Text[, {evidence?, presupposition?, request?, permute?}])",
        returns: "题（K 选一）",
        refresh: &[],
        privileges: &[P_QUESTION],
        clause: "12 §2.2；B64",
        run: Some(|it, c, n, a, s| it.b_test(c, n, a, s)),
    },
    // 依据：12 §2.2
    ConstructSpec {
        name: "measure",
        params: "measure(题面, [档位…], calib)",
        returns: "题（打分）",
        refresh: &[],
        privileges: &[P_QUESTION],
        clause: "12 §2.2",
        run: Some(|it, c, n, a, s| it.b_measure(c, n, a, s)),
    },
    // 依据：12 §2.2 题式；B54（前提进 form_hash）
    ConstructSpec {
        name: "form",
        params: "form(题型: \"test\" | \"select\" | \"measure\", 模板题面: Text, {calib, scale?, evidence?, presupposition?, request?})",
        returns: "题式",
        refresh: &[],
        privileges: &[P_QUESTION],
        clause: "12 §2.2；B54",
        run: Some(|it, c, n, a, s| it.b_form(c, n, a, s)),
    },
    // 依据：12 §2.2 题式
    ConstructSpec {
        name: "fill",
        params: "fill(题式, {槽: 值, …})",
        returns: "题",
        refresh: &[],
        privileges: &[P_QUESTION],
        clause: "12 §2.2",
        run: Some(|it, c, n, a, s| it.b_fill(c, n, a, s)),
    },
    // 依据：05 §1 filter；B17；B81；B82；B57（四类能力都用）
    ConstructSpec {
        name: "sieve",
        params: "sieve(材料, 题 | [题…]) | sieve(材料, 题式, [填法…])",
        returns: "契约值 kind=sieve",
        refresh: &["sieve"],
        privileges: &[P_READ, P_UNSURE, P_DUTY, P_KEYS, P_LREAD],
        clause: "05 §1；B17；B81；B82；B133",
        run: Some(|it, c, n, a, s| it.b_sieve(c, n, a, s)),
    },
    // 依据：05 §1 pair；B17；B81。待出内核：25-6（B138 (4)；闸门测试暂豁免）
    ConstructSpec {
        name: "pair",
        params: "pair(左, 右) | pair(左, 右, fn(a, b)) | pair([[a, b], …])",
        returns: "契约值 kind=pair",
        refresh: &[],
        privileges: &[],
        clause: "05 §1；B17；B81",
        run: Some(|it, c, n, a, s| it.b_pair(c, n, a, s)),
    },
    // 依据：05 §1 agg；B17；B131（聚合出口经合成构造 compose 签发，本身不需内核能力）。待出内核：25-9（B140；闸门测试暂豁免）
    ConstructSpec {
        name: "tally",
        params: "tally(契约值)",
        returns: "契约值 kind=tally",
        refresh: &[],
        privileges: &[],
        clause: "05 §1；B17",
        run: Some(|it, c, n, a, s| it.b_tally(c, n, a, s)),
    },
    // 依据：05 §1 前 k；B3；B17；B131（出口经 compose 的 first 规则）。待出内核：25-9（B141；闸门测试暂豁免）
    ConstructSpec {
        name: "first_k",
        params: "first_k(契约值, k: Int)",
        returns: "契约值 kind=first_k",
        refresh: &[],
        privileges: &[],
        clause: "05 §1；B3；B17",
        run: Some(|it, c, n, a, s| it.b_first_k(c, n, a, s)),
    },
    // 依据：05 §1 iterate；12 §3 J-06；B17；B132（iterate 留内核：循环上下文与账本键收集）
    ConstructSpec {
        name: "iterate",
        params: "iterate(bound, 初值, fn(acc, i), measure)",
        returns: "契约值 kind=iterate",
        refresh: &[],
        privileges: &[P_LOOP, P_KEYS],
        clause: "05 §1；J-06；B17",
        run: Some(|it, c, n, a, s| it.b_iterate(c, n, a, s)),
    },
    // 依据：B17；A-2（花费由证据键从账本算）
    ConstructSpec {
        name: "outcome",
        params: "outcome({value, pending?, evidence?, resume?, purpose?, detail?})",
        returns: "契约值 kind=outcome",
        refresh: &[],
        privileges: &[P_LREAD],
        clause: "B17；A-2",
        run: Some(|it, c, n, a, s| it.b_outcome(c, n, a, s)),
    },
    // ---- 长处（G4 §7「判断力花在哪」）：读数是带校准线的随机变量，不是值。
    // 这两个构件只读校准线、不做跨题算术、返回宿主值不返回读数，所以合法（Python
    // `runtime.py:1236` allocate 的 docstring 原话）。都不花钱：不发调用、不进账本、不动预算。
    // 依据：12 §5 判断向量；J-10
    ConstructSpec {
        name: "allocate",
        params: "allocate(读数们, k: Int)",
        returns: "记录 {picked, 算不出, …}",
        refresh: &["allocate"],
        privileges: &[P_READ],
        clause: "12 §5；J-10",
        run: Some(|it, c, n, a, s| it.b_allocate(c, n, a, s)),
    },
    // 依据：12 §3 J-10
    ConstructSpec {
        name: "unsure_bound",
        params: "unsure_bound(读数们)",
        returns: "记录 {n, union_bound, independent_any, n_unknown}",
        refresh: &["unsure_bound"],
        privileges: &[],
        clause: "12 §3 J-10",
        run: Some(|it, c, n, a, s| it.b_unsure_bound(c, n, a, s)),
    },
    // 判断向量的两法（12:134「合法操作**只有两种**…其余运算不存在（J-01）」）。
    // 它们不是「读数列表上的工具函数」——正因为只有这两种，读数才不会被当成数用。
    // **同题重复读数的合并（B28）**：`repeat`（原 `agg`）只许均值或中位数，用于压抖动、不为降错；
    // 合并结果过桥用**含 n 的独立校准键**（`键·repeat(n=…)`，不借题式或模式线），账本记 n；
    // 对出口取众数（多数表决）禁止——choice 取各候选概率的均值 / 中位数，不投票。
    // 键未通过重跑分歧检验时（记录 `rerun_independent` 不为真）照常合并但告警：错误持久时重问不降错（B9）。
    // 依据：B28（同题重复读数只许均值或中位数）；B9
    ConstructSpec {
        name: "agg",
        params: "agg(读数列表[, \"mean\" | \"median\"])（已改名 repeat，W-deprecated）",
        returns: "读数（含 n 的独立校准键）",
        refresh: &["repeat"],
        privileges: &[P_READ, P_READING, P_LWRITE],
        clause: "B28；B9",
        run: Some(|it, c, n, a, s| it.b_agg(c, n, a, s)),
    },
    // 依据：B28；B9
    ConstructSpec {
        name: "repeat",
        params: "repeat(读数列表[, \"mean\" | \"median\"])",
        returns: "读数（含 n 的独立校准键）",
        refresh: &["repeat"],
        privileges: &[P_READ, P_READING, P_LWRITE],
        clause: "B28；B9",
        run: Some(|it, c, n, a, s| it.b_agg(c, n, a, s)),
    },
    // 依据：12 §5 判断向量（同题同锚跨对象偏序）；J-04
    ConstructSpec {
        name: "order",
        params: "order(读数们)",
        returns: "分档的下标列表",
        refresh: &["order"],
        privileges: &[P_READ],
        clause: "12 §5；J-04",
        run: Some(|it, c, n, a, s| it.b_order(c, n, a, s)),
    },
    // fit 桥（12 §6.0:315 `fit(名, [读数…])`，**输出仍是读数、仍要 cut**）。
    // 让什么活下来：**跨题的联合判断在类型上仍是读数**，因而仍要过线、仍可能 unsure。
    // 直接产出出口就绕过了 cut 的判序（insufficient → taint → 过线 → band）。
    // 依据：12 §2.9 fit 桥；J-16
    ConstructSpec {
        name: "fit",
        params: "fit(名字: Text, [读数…])",
        returns: "读数（仍要 cut）",
        refresh: &["fit"],
        privileges: &[P_READ, P_READING],
        clause: "12 §2.9；J-16",
        run: Some(|it, c, n, a, s| it.b_fit(c, n, a, s)),
    },
    // 依据：B131（出口合成：封闭规则集，内核算种类）；B51-R1；12 §2.3 出口合成条
    ConstructSpec {
        name: "compose",
        params: "compose(出口们, \"any\" | \"all\" | {first: k} | {sup: 格} | \"min\")（内部构造，步 25-8 开放）",
        returns: "出口（合成，带 parts）",
        refresh: &[],
        privileges: &[P_COMPOSITE, P_DUTY],
        clause: "12 §2.3 B131",
        run: None,
    },
    // 依据：B133（元素记录构造）；B81；B82；B92；B120
    ConstructSpec {
        name: "element",
        params: "element(输入, 出口, {pos, q, qi, fill?, key})（内部构造，步 25-11 开放）",
        returns: "元素记录",
        refresh: &[],
        privileges: &[P_SELECT, P_ROW],
        clause: "12 §2.12 B133",
        run: None,
    },
];

/// 全部内核构造的规格（`jpp describe` 读它，`21` 步 9b）。
pub fn construct_specs() -> &'static [ConstructSpec] {
    CONSTRUCTS
}

/// 按源码名查 `.jpp` 可调的构造（内部构造不在这里，按名字调不到）
pub(crate) fn construct(name: &str) -> Option<&'static ConstructSpec> {
    CONSTRUCTS.iter().find(|s| s.name == name && s.exposed())
}

fn 内部构造(name: &str) -> &'static ConstructSpec {
    CONSTRUCTS
        .iter()
        .find(|s| s.name == name && !s.exposed())
        .unwrap_or_else(|| panic!("内部构造 {name} 未注册"))
}

impl<'a> Interp<'a> {
    /// 调一个内核构造：按它声明的能力造令牌，再调实现。
    pub(crate) fn call_construct(
        &mut self,
        s: &'static ConstructSpec,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let caps = Caps::for_spec(s);
        let run = s.run.expect("只有开放的构造能按名字调用");
        run(self, &caps, name, args, sp)
    }

    /// 调合成构造 `compose`（B131）：令牌按 `compose` 的声明发放，调用者不需要任何能力。
    pub(crate) fn 调合成(&mut self, 请求: 合成请求, sp: Span) -> R<Value> {
        let caps = Caps::for_spec(内部构造("compose"));
        self.合成(&caps, 请求, sp)
    }

    /// 调元素构造 `element`（B133）：令牌按 `element` 的声明发放。
    pub(crate) fn 调元素(
        &mut self, input: &Value, exit: &Rc<Exit>, ctx: 元素上下文
    ) -> Value {
        let caps = Caps::for_spec(内部构造("element"));
        self.元素(&caps, input, exit, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 构造名唯一_开放的都是内置名_都不是效应名() {
        let mut seen = HashSet::new();
        for s in CONSTRUCTS {
            assert!(seen.insert(s.name), "构造名重复：{}", s.name);
            assert_eq!(
                BUILTINS.contains(&s.name),
                s.exposed(),
                "{}：开放的构造必须在 BUILTINS，内部构造不能在",
                s.name
            );
            assert!(
                jpp_effects::by_name(s.name).is_none(),
                "{} 是效应名",
                s.name
            );
        }
    }

    #[test]
    fn 注册表覆盖全部十九个构造名() {
        let mut names: Vec<&str> = CONSTRUCTS.iter().map(|s| s.name).collect();
        names.sort();
        let mut want = vec![
            "test",
            "select",
            "measure",
            "form",
            "fill",
            "sieve",
            "pair",
            "tally",
            "first_k",
            "iterate",
            "outcome",
            "allocate",
            "unsure_bound",
            "agg",
            "repeat",
            "order",
            "fit",
            "compose",
            "element",
        ];
        want.sort();
        assert_eq!(names, want);
    }

    #[test]
    fn 声明的刷新点都已登记() {
        for s in CONSTRUCTS {
            for r in s.refresh {
                assert_eq!(
                    readings::refresh_point(r),
                    Some(readings::RefreshKind::Criterion),
                    "{} 的刷新原因 {r} 未在 REFRESH_POINTS 登记为判据",
                    s.name
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "没有在 ConstructSpec.privileges 声明的能力 ReadAnswer")]
    fn 未声明的能力取不到() {
        let s = construct("pair").expect("pair 已注册");
        let caps = Caps::for_spec(s);
        let _ = caps.read_answer();
    }

    /// B131：`tally`、`first_k` 经合成构造签发出口，自己不需要任何内核能力；`compose`、`element` 是内部构造
    #[test]
    fn tally_first_k_不声明能力_合成与元素是内部构造() {
        for n in ["tally", "first_k"] {
            let s = construct(n).expect("已注册");
            assert!(s.privileges.is_empty(), "{n} 声明了 {:?}", s.privileges);
        }
        for n in ["compose", "element"] {
            assert!(construct(n).is_none(), "{n} 不能按名字调用");
            assert!(!内部构造(n).exposed());
        }
        assert_eq!(
            内部构造("compose").privileges,
            &[Privilege::IssueComposite, Privilege::Duty]
        );
        assert_eq!(
            内部构造("element").privileges,
            &[Privilege::SourceSelect, Privilege::ExitRow]
        );
        // B138 (3)：key_of 是读法内置，不在构造表
        assert!(CONSTRUCTS.iter().all(|s| s.name != "key_of"));
        assert!(BUILTINS.contains(&"key_of"));
    }

    /// B138 (2) 闸门：内核构造要么用某类内核能力，要么是批调度的刷新点，两者皆无就没有理由待在内核。
    /// 豁免的三项都已定出内核（`pair` 25-6，`tally`/`first_k` 25-9），合入时删；豁免项必须确实为空，
    /// 否则豁免名单过期。
    #[test]
    fn 每个内核构造都用能力或是刷新点() {
        const 待出内核: &[&str] = &["pair", "tally", "first_k"];
        for s in CONSTRUCTS {
            let 空 = s.privileges.is_empty() && s.refresh.is_empty();
            if 待出内核.contains(&s.name) {
                assert!(空, "{} 已不为空，从豁免名单删掉", s.name);
            } else {
                assert!(
                    !空,
                    "{} 的 privileges 与 refresh 皆空：按 B57、B138 不该在内核",
                    s.name
                );
            }
        }
    }
}
