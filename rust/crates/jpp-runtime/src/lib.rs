//! 解释器：逐语句即时执行；效应即时发出（惰性融合是后续优化，未迁移）。
//! 纪律由这里与检查器把关：J-01 读数不进槽、J-02 禁自指、J-03 线来自校准记录、J-05 unsure 必消费、
//! J-06 有界循环 + 键重复即停、J-07 预算超即停（Pending）、J-12 Fail 是值、J-13 序号、J-18 账本头。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use serde_json::{Value as Json, json};

// 运行时读 IR（步 12c；步 12d 删除核心语法树后只剩 IR）
use jpp_effects::port::{CallInput, EffectError, EffectOut, Ports};
use jpp_effects::view::{self, Callee, K, kind};
use jpp_effects::views::{CalibView, Lookup};
use jpp_effects::views::{FitRecord, FitView};
use jpp_ir::ir::{Block, Budget, Expr, Function, Program, Span, Stmt};
use jpp_ledger::{
    CalibRef, EffectKey, Entry, Header, HeaderCompare, JudgeKey, Ledger, MatMeta, RENDER_VERSION,
    SourceEdge, Trace,
};
use jpp_value::value::*;

pub const HANDLER_VERSION: &str = "h0.1-rs";
pub const DEFAULT_DEPTH: u32 = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct RtError {
    pub rule: Option<String>,
    pub message: String,
    pub span: Span,
}

impl RtError {
    pub fn new(rule: Option<&str>, msg: impl Into<String>, span: Span) -> RtError {
        RtError {
            rule: rule.map(|s| s.to_string()),
            message: msg.into(),
            span,
        }
    }
    pub fn render(&self) -> String {
        match &self.rule {
            Some(r) => format!(
                "[{r}] {} @{}..{}",
                self.message, self.span.start, self.span.end
            ),
            None => format!("{} @{}..{}", self.message, self.span.start, self.span.end),
        }
    }
}

/// 控制流信号：错误，或程序级挂起（预算、ask 未答、显式 pending）。
#[derive(Debug)]
pub enum Fault {
    Error(RtError),
    Halt(Pending),
}

impl From<RtError> for Fault {
    fn from(e: RtError) -> Fault {
        Fault::Error(e)
    }
}

type R<T> = Result<T, Fault>;

fn err<T>(rule: Option<&str>, msg: impl Into<String>, span: Span) -> R<T> {
    Err(Fault::Error(RtError::new(rule, msg, span)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaintOut {
    Trusted,
    Untrusted,
    Inherit,
}

/// 登记的动作（`do` 只能触发登记过的动作；§2.5）。
#[derive(Clone)]
pub struct Action {
    pub name: String,
    pub cost: f64,
    pub reversible: bool,
    pub taint_out: TaintOut,
    pub f: Rc<dyn Fn(&[Value]) -> Result<Value, String>>,
    /// 声明的输出形状（B51-R2 候选字段，步 15d）：`None` = 未声明，运行期不核
    pub mat_shape: Option<jpp_effects::MatShape>,
}

#[derive(Default)]
pub struct ActionRegistry {
    pub actions: HashMap<String, Rc<Action>>,
}

impl ActionRegistry {
    pub fn new() -> ActionRegistry {
        ActionRegistry::default()
    }
    pub fn register(
        &mut self,
        name: &str,
        cost: f64,
        reversible: bool,
        taint_out: TaintOut,
        f: impl Fn(&[Value]) -> Result<Value, String> + 'static,
    ) {
        self.actions.insert(
            name.to_string(),
            Rc::new(Action {
                name: name.to_string(),
                cost,
                reversible,
                taint_out,
                f: Rc::new(f),
                mat_shape: None,
            }),
        );
    }

    /// 给已登记的动作声明输出形状（B51-R2，步 15d）：`do` 返回后运行期核基数与单项尺寸，违反出
    /// `Fail(ShapeMismatch)`。动作没登记时返回 `false`。依据：B51-R2（20 附录 A）
    pub fn shape(&mut self, name: &str, shape: jpp_effects::MatShape) -> bool {
        match self.actions.get_mut(name) {
            Some(a) => {
                Rc::make_mut(a).mat_shape = Some(shape);
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cost {
    pub calls: u64,
    pub replayed: u64,
    pub tokens: u64,
    pub usd: f64,
    pub asks: u64,
}

/// 审计重放的记账（B35）：账本里记过的调用在重放时计入预算，但不算新调用、不进报告的 `cost`。
#[derive(Clone, Debug, Default)]
struct ReplayAudit {
    on: bool,
    /// 按账本记录折算的调用次数与费用（融合后多道题同属一次调用，按 `call` 去重）
    calls: u64,
    usd: f64,
    seen_calls: HashSet<u64>,
    /// 最近一次从账本取答的站点：费用超预算时停在这里（首跑的调用后核预算停在同一站点）
    last_site: Option<Span>,
}

/// 这条线**凭什么**：`手填` 还是某一张证书。与「哪一层」正交。
fn 凭据(rec: &Lookup) -> String {
    match &rec.selected {
        // **第三轴：这个数是怎么算出来的。**「哪一层」「凭什么」「怎么算的」是三件事——
        // 把代价折进「凭什么」那一格，就是把一小时前刚红过的那次合并再做一遍。
        // **一条由代价矩阵算出来的线，和一条人拍脑袋写的线，不是一回事。**
        Some(c) => match c.cost {
            // B72：试用证书在凭据里说出来（handler 经 `line_source` 看得见）
            Some((fp, fn_)) => format!(
                "{}证书:α={:.2}·代价(fp={fp},fn={fn_})",
                试用前缀(c),
                c.alpha
            ),
            None => format!("{}证书:α={:.2}", 试用前缀(c), c.alpha),
        },
        None => "手填".to_string(),
    }
}

fn 试用前缀(c: &jpp_effects::views::CertView) -> &'static str {
    if c.trial { "试用" } else { "" }
}

fn e_line_empty(s: &str) -> bool {
    s.is_empty()
}

/// 取前 n 个**字符**做诊断摘要。
///
/// **不能按字节切**：`q_hash` 平时是十六进制，但 `fit` 的结果把 `fit:{名}` 放在那一位，
/// 名字是中文时按字节切会切在字符中间——**`&s[..8]` 直接 panic**。
/// 一句告警文字把整个程序打崩，是比它想报的问题严重得多的事。
///
/// **同族全部走这里**，包括那些「现在一定是十六进制所以按字节切也安全」的地方
/// （账本键、`state.hash`）。安全靠的是一条没写下来的不变量，
/// **而下一个人得重新验一遍才知道安全**——统一成一种形状就不用验。
fn 头(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug)]
pub struct Outcome {
    /// 程序值；挂起时为 None
    pub value: Option<Value>,
    pub pending: Vec<Pending>,
    pub trace: Trace,
    pub cost: Cost,
    /// 最外层带出的未消费 Unsure（v0.1.1：允许并记）
    pub returned_unsure: Vec<String>,
    /// 每次刷新发出的一层（12 §2.2 的分层结果）；层数是 lift / fuse 的量具
    pub layers: Vec<Layer>,
    /// **这一趟跑出来的证据**（`12`:347 的「运行期写入口」）：`(校准键, 观察)`。
    ///
    /// **它是缓冲区，不是写库。** 程序发出证据，宿主决定折不折进 `CalibStore`——
    /// 这样 I4「程序里不可写线」在字面上仍然成立：程序连库的可变引用都拿不到。
    /// **重放不进这里**：重放读的是既有事实，不是新观察。
    pub evidence: Vec<(String, jpp_effects::views::Sample)>,
    /// **停岗候选**（B25）：本趟自动标记的校准键（漂移信号超线）。CLI 的 `--calib-out`
    /// 把它们写成「停岗候选」；正式停岗由人确认（`jpp calib-confirm`）。
    pub suspend_candidates: Vec<String>,
    /// **逐 `cut` 出口的线等级**（步 20f，总账待补「逐出口记线等级」）：每条
    /// `{site, exit, grade, releases, key?, scope_out?, suspend_candidate?}`，`grade` 用 `20` §3.4 的
    /// `LineGrade` 变体名（`Certified` `Form` `Trial` `Provisional` `Class` `Fixture` `Cold`）。
    /// 出口不进账本（`20` §3.7(1)）：等级随校准记录的证书进账本的 `CalibUsed` 条目（账本 v3；v2 在头行 `calib_used`），只凭账本重放时重算出同一张表。
    pub exits: Vec<Json>,
    /// **本趟问过的题**（B107、B120 (a)，步 20h-2）：每个不同的题哈希一行 `{q, form_hash?, template?, fill?, kind}`，
    /// `kind` 是登记读数时算出的精化类（B76）；不带读数与出口。`calib-import --list-out --report` 据此给清单行附题面，
    /// 回填时据此给标注行附题类。
    pub questions: Vec<Json>,
    /// 预算停机（B93，步 22-0）：预算耗尽后未发的判断与效应数、首个未发站点。没耗尽为 `None`
    /// （报告不出 `budget` 段，默认输出逐字节不变）。
    pub budget: Option<BudgetStop>,
}

/// 预算停机的记账（B93）：停发，不停程序。
#[derive(Clone, Debug, serde::Serialize)]
pub struct BudgetStop {
    pub exhausted: bool,
    /// 预算耗尽后没发出的判断（按账本键计）与效应（`gen`、`do`）个数
    pub unsent: u64,
    /// 首个未发站点的起始字节偏移
    pub first_site: usize,
}

impl Outcome {
    pub fn value_json(&self) -> Json {
        self.value
            .as_ref()
            .map(|v| v.to_json())
            .unwrap_or(Json::Null)
    }
}

struct Frame {
    name: String,
    exits: Vec<Rc<Exit>>,
    /// 本帧登记、尚未解析的惰性出口（B94，步 23c）：帧返回前全部解析，出口挂回本帧
    cuts: Vec<Rc<PendingCut>>,
    /// 返回类型提到 Exit：未消费的 Unsure 由调用者接手
    returns_exit: bool,
}

struct LoopCtx {
    seen_keys: HashSet<String>,
    repeated: Option<String>,
}

pub struct Interp<'a> {
    /// 按效应实例索引的端口表（步 15b，`20` §2.3 `Ports.effects`）：判断、生成、问人只经它发调用
    ports: Ports<'a>,
    ledger: &'a mut Ledger,
    /// 校准只经只读视图读（步 11b，`20` §2.3：运行时不依赖 `CalibStore` 具体类型）
    calib: &'a dyn CalibView,
    /// 私有读数表：读数是句柄，答案只在这里（步 11b-3）。写只经 `flush.rs::fill_answer`，
    /// 读只经 `readings.rs::answer_of`。
    answers: std::cell::RefCell<AnswerTable>,
    next_reading: std::cell::Cell<u64>,
    actions: &'a ActionRegistry,
    fits: Fits<'a>,
    budget: Budget,
    pub trace: Trace,
    pub cost: Cost,
    frames: Vec<Frame>,
    loops: Vec<LoopCtx>,
    next_exit: usize,
    depth: u32,
    run_seq: u64,
    model_id: String,
    /// 已登记但还没发出的判断（`12` §2.2「登记后不发」）
    pending: Vec<PendingJudge>,
    /// **审计重放**（B35；21 步 3）：只凭账本重现首跑。账本里记过的调用照记录计入预算，
    /// 使首跑在哪里预算停机，重放就在哪里停；账本缺的记录报 `E-replay`（致命，不进 cause）。
    /// 续跑（`--resume`）不开：已记录的不付费、继续往下。依据：12 §2.3 B35 注；21 §三·2 步 3。
    audit: ReplayAudit,
    /// 宿主入口参数（B105；步 14b-0 起 `--input`）：运行入口绑定进环境，哈希进账本头 `entry_hash`
    entry: EntryArgs,
    /// 入口材料的哈希 → 入口名（J-08 报文「该材料是宿主入口 <名>」用；B105）
    entry_mat_names: HashMap<String, String>,
    /// 每次刷新发出的一层，供测试与 Trace 看分层结果
    pub layers: Vec<Layer>,
    /// 本次运行的计划（`jpp-plan` 写，这里只读）。步 14a 起由宿主算好经 [`Interp::run`] 交进来
    /// （`20` §2.2 第 3 条：运行时不依赖 `jpp-plan`；pass 开关随之留在宿主，`jpp::interp::Interp`）
    plan: jpp_ir::plan::Plan,
    /// 规划的运行期钩子（`20` §2.3 `Ports.hooks`）。步 14a 起由宿主经 [`Interp::run`] 注入
    hooks: &'a dyn jpp_ir::plan::PlanHooks,
    /// 当前所处的条件链：每层是「这个条件里有没有一个 **trusted 来源的合取项**」。
    ///
    /// J-08（`12`:265）说的「放行不可逆 `do` 的**守卫表达式**」，在一个有 `if` 的语言里
    /// 就是**包着这个 `do` 的那些条件**——不必是 `do` 的一个参数。条件求值成 `Bool` 之后
    /// taint 就没了，所以在**求值条件的那一刻**记下来。
    guards: Vec<GuardEv>,
    /// 推测登记过的账本键：程序真走到时会命中账本，没走到的留 `W-spec-unused`
    speculated: HashSet<String>,
    /// 推测的键里，真被程序用上的
    speculation_used: HashSet<String>,
    /// 布尔绑定的来源：`let x = …` 求值过程中产生的出口带着状态 taint，据此答
    /// 「这个布尔是不是由**可信状态上的判断**决定的」（J-08）。名字被重新绑定时覆盖。
    /// 来源通道**的作用域由环境给**：`let x = …` 时把来源连同值一起绑进 `env`，
    /// 名字叫 `x\u{1f}prov`。于是它天然随块退出而消失、被重新绑定而覆盖、
    /// 在 helper 的帧里查不到外层的——**和它描述的那个值同一个作用域**。
    ///
    /// 此前这里是一张按名字索引的 `HashMap` 旁路表，**没有作用域**：
    /// 一次「返回值不是 Bool/Record、内部走过 ask」的调用会把「经过 ask」留在传送带上，
    /// 被之后**任意一条不相关的 `let`** 继承——连 `let ok = true;` 都会被污染。
    /// 而它**同时**会漏（`lifecycle.jpp` 那次假拒绝）：**漏和串是同一个病的两种表现**。
    ///
    /// 判别法（`12` §2.11 第五条）：**这条来源通道，它的作用域是谁给的？**
    /// 账本里已有的 `ask` 条数：只参与核 `budget.escalate` 总上限，**不进 `Cost.asks`**
    /// （那个报的是「这次运行实际问了几次人」，重放时该是 0）。
    asks_in_ledger: u64,
    /// **缺席 / 超时标记**（B32）：账本键 → `absent` / `latency`。`cut` 据此给 `Unsure(原因)`。
    absent_marks: HashMap<String, String>,
    /// 连续缺席次数（熔断用）
    consecutive_absent: u32,
    /// 本趟算出的判断键：账本键 → 结构化键（账本 v2 条目记结构化键，步 7）
    judge_keys: HashMap<String, JudgeKey>,
    /// 本趟算出的效应键：账本键 → 结构化键（步 7）
    effect_keys: HashMap<String, EffectKey>,
    /// 本趟判断调用累计耗时（秒，B32 时延预算）
    latency_spent: f64,
    /// 本趟见过的单次判断调用最大费用（步 15e：cost 预算按它限窗；`None` = 还没观察到）
    c_max: Option<f64>,
    /// 预算停机（B93，步 22-0）：首次停发时置上；此后登记的站点同样停发、费用为零
    预算停: Option<BudgetStop>,
    /// 解析惰性出口时借用的出口号与所属帧（B94，步 23c）：`new_exit_from` 取用一次即清
    出口预定: Option<(usize, usize)>,
    /// 已停发过的判断键（`unsent` 按键去重：同一键经提前登记与真站点各进一次刷新时只计一次）
    停发键: HashSet<String>,
    /// 这一轮里被 `content()` 从 **untrusted 材料**拆出来的内容（规范 JSON）。
    /// `mat()` 拿到其中之一时不能当字面量洗成 trusted——见 `as_mat` 的兜底臂。
    /// 从 untrusted 来源拆出的**文本叶子**（K-182 / K-203，2026-09-23）：派生出的新字符串
    /// （拼接、join、slice、text）按子串关系认回来，不再只做整值精确匹配。
    /// 含有「成分不可信的计算值」材料的状态哈希（J-08 诊断用，B33 第 8 点）
    computed_untrusted_states: std::cell::RefCell<HashSet<String>>,
    /// 含「宿主入口、未声明可信」材料的状态哈希 → 入口名（J-08 诊断用，B105）
    input_untrusted_states: std::cell::RefCell<HashMap<String, String>>,
    /// 本趟已记下命中的校准键（`note_calib` 去重；账本 v3 起 `calib_used` 是账本条目的派生视图，
    /// 跨趟保留，不再在入口清空）
    本趟已记校准: HashSet<String>,
    /// 谱系放行（B72-4，步 17b）：本趟切过的出口，按账本键记「是否全部已决且放行」与第一个不放行者的说明。
    /// 同一键切多次时须全部放行（17b 解释登记 (c)）。
    出口放行表: HashMap<String, (bool, String)>,
    /// 谱系断的出口：出口 id → 说明（J-08 报文补「该材料由 … 的出口选出」用）
    谱系断: std::cell::RefCell<HashMap<usize, String>>,
    /// B52（步 21）：判为 Fn¹ 的责任 → 那个闭包的名字（J-05 报文说明「唯一路径是哪个闭包」用）
    fn1_of: HashMap<usize, String>,
    /// B95（步 21）：本趟显式 drop 过的未决出口（返回前核 `W-drop-then-return` 用）
    dropped: Vec<Rc<Exit>>,
    /// 已报过 `W-lineage-unknown` 的祖先键（每键报一次）
    谱系缺键已报: HashSet<String>,
    /// 这次运行已经报过漂移的键：**一条天天响的告警等于没有告警**
    drift_reported: HashSet<String>,
    /// B104：`W-delta-unknown` / `W-scope-unknown` 每键每趟只报一次（值为「告警码\u{1f}键」）
    unknown_reported: HashSet<String>,
    evidence: Vec<(String, jpp_effects::views::Sample)>,
    /// 逐 `cut` 出口的线等级（步 20f，报告 `exits`；出口不进账本，重放时重算）
    exit_grades: Vec<Json>,
    /// 出口 id → 它在 `exit_grades` 里的行（步 25-2b，B133）：`cut` 写完一行时记；元素构造 `element` 按它找行写
    /// `index`/`pos`（B120 (b)），不依赖「最后一行」。
    exit_rows: HashMap<usize, usize>,
    /// 读数的精化题类（B76，步 12e-2；`21` 写作 `ReadingMeta.kind`）：按读数句柄记，登记读数时算。
    /// 放在这里而不放 `Reading` 上，是为了不改 `Reading` 的字面量构造（宿主与测试各处）。
    reading_kinds: HashMap<u64, QuestionKind>,
    /// 报告的 `questions` 表（B107、B120 (a)，步 20h-2）：每个不同的题哈希一行，按首次登记的顺序；不带读数
    questions: Vec<Json>,
}

/// 一次 `judge` 登记：一个状态 + 它那几道题
struct PendingJudge {
    state: Rc<State>,
    items: Vec<(Rc<Question>, Rc<Reading>, String)>,
    site: Span,
    /// 推测登记的（不是程序真走到的站点）。超预算时**先丢它**，再动真站点。
    speculative: bool,
    /// 直线段提升登记的（B94 下半，审查修复 3b）：刷新分组按组内第一条非提升登记的位置排序，
    /// 只有提升登记的组排在最后，不挤掉程序序更靠前的真站点
    lifted: bool,
}

/// 一次刷新发出的一层
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// 触发这次刷新的刷新点：`cut` / `if` / `end` / `content` …
    pub reason: String,
    pub calls: u64,
    pub questions: usize,
}

pub const BUILTINS: &[&str] = &[
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "key_of",
    "cut",
    "handle",
    "consume",
    "gen",
    "do",
    "ask",
    "transform",
    "mat",
    "content",
    "unsure",
    "pending",
    "fail",
    "is_fail",
    "loop",
    "stop",
    "unsure_cause",
    "untested",
    "line_source",
    "taint",
    "escalate",
    "literalize",
    "allocate",
    "unsure_bound",
    "agg",
    "repeat",
    "order",
    "fit",
    "len",
    "map",
    "filter",
    "fold",
    "range",
    "append",
    "concat",
    "slice",
    "contains",
    "sum",
    "reverse",
    "keys",
    "with",
    "has",
    "text",
    "join",
    "print",
    "min",
    "max",
    "abs",
    "floor",
    "exit_kind",
];

pub fn root_env() -> Env {
    let env = env_root();
    for b in BUILTINS {
        env_define(&env, b, Value::Builtin(b));
    }
    env
}

/// 空的 fit 表：不用 fit 的程序共用这一个（步 14a 前是线程局部 leak 的一份空 `FitRegistry`；
/// 运行时不依赖 `jpp-calib`，改为恒空的 `FitView` 实现，查什么都查不到，与空注册表同义）。
pub struct NoFits;
impl FitView for NoFits {
    type Record = Rc<FitRecord>;
    fn get(&self, _fit_ref: &str) -> Option<&Rc<FitRecord>> {
        None
    }
}

/// `fit` 注册表的只读视图（`20` §2.3 `Ports.fits`；`jpp-calib::FitRegistry` 实现它）。
pub type Fits<'a> = &'a dyn FitView<Record = Rc<FitRecord>>;

/// 构造到 [`Interp::run`] 之间的占位钩子：计划与钩子只在 `run` 入口注入，此前运行时不求值，
/// 所以它不会被调用（步 14a）。
struct Unplanned;
impl jpp_ir::plan::PlanHooks for Unplanned {
    fn instantiate(
        &self,
        _: &jpp_ir::plan::Plan,
        _: &Function,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<jpp_ir::plan::Target> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn speculate<'b>(
        &self,
        _: &jpp_ir::plan::Plan,
        _: jpp_ir::key::NodeId,
        _: &'b Block,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<&'b Expr> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn segment(
        &self,
        _: &jpp_ir::plan::Plan,
        _: jpp_ir::key::NodeId,
        _: &Block,
        _: &dyn jpp_ir::plan::EnvView,
    ) -> Vec<jpp_ir::plan::Target> {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
    fn may_effect(&self, _: &Expr, _: &dyn jpp_ir::plan::EnvView, _: jpp_ir::plan::Reach) -> bool {
        unreachable!("计划与钩子在 Interp::run 入口注入")
    }
}

mod bridge;
mod budget;
mod caps;
mod constructs;
mod duty;
mod effects_exec;
mod entry;
mod eval;
mod flush;
mod guard;
mod host_builtins;
mod outcome;
mod plan_view;
mod readings;
mod register;
mod schedule;
pub mod strength;
use caps::Caps;
pub use caps::{ConstructSpec, Privilege, construct_specs};
pub use entry::{EntryArgs, EntryMat, EntryValue};

use readings::refresh_point;

impl<'a> Interp<'a> {
    /// 不带 fit 注册表的入口（绝大多数程序不用 fit）。要用 fit 走 `with_fits`。
    /// 按端口表构造（步 15b；步 15c 起唯一入口）：运行时只经端口发调用。不带 fit 注册表（绝大多数
    /// 程序不用 fit），要用 fit 走 `with_fits`。
    pub fn new(
        ports: Ports<'a>,
        ledger: &'a mut Ledger,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        Interp::with_fits(ports, ledger, calib, actions, &NoFits, budget)
    }

    /// 账本头与判断键的模型 id 取服务「产出读数的效应」的那个实例的模型（B60；I5）；没有注册判断端口时
    /// 取第一个已注册实例的模型（这时任何判断调用都会报没有端口）。
    pub fn with_fits(
        ports: Ports<'a>,
        ledger: &'a mut Ledger,
        calib: &'a dyn CalibView,
        actions: &'a ActionRegistry,
        fits: Fits<'a>,
        budget: Budget,
    ) -> Interp<'a> {
        let model_id = jpp_effects::find(|s| s.produces_reading)
            .and_then(|e| ports.instance_of(e))
            .or_else(|| ports.instances().into_iter().next())
            .map(|i| i.model)
            .unwrap_or_default();
        Interp {
            ports,
            ledger,
            calib,
            answers: Default::default(),
            next_reading: Default::default(),
            actions,
            fits,
            budget,
            trace: Trace::default(),
            cost: Cost::default(),
            frames: vec![],
            loops: vec![],
            next_exit: 0,
            depth: 0,
            run_seq: 0,
            model_id,
            pending: vec![],
            layers: vec![],
            plan: jpp_ir::plan::Plan::empty(),
            hooks: &Unplanned,
            guards: vec![],
            speculated: HashSet::new(),
            speculation_used: HashSet::new(),
            asks_in_ledger: 0,
            computed_untrusted_states: std::cell::RefCell::new(HashSet::new()),
            input_untrusted_states: std::cell::RefCell::new(HashMap::new()),
            本趟已记校准: HashSet::new(),
            出口放行表: HashMap::new(),
            谱系断: std::cell::RefCell::new(HashMap::new()),
            fn1_of: HashMap::new(),
            dropped: vec![],
            谱系缺键已报: HashSet::new(),
            drift_reported: HashSet::new(),
            unknown_reported: HashSet::new(),
            exit_grades: vec![],
            exit_rows: HashMap::new(),
            reading_kinds: HashMap::new(),
            questions: vec![],
            evidence: vec![],
            absent_marks: HashMap::new(),
            consecutive_absent: 0,
            judge_keys: HashMap::new(),
            effect_keys: HashMap::new(),
            latency_spent: 0.0,
            c_max: None,
            预算停: None,
            出口预定: None,
            停发键: HashSet::new(),
            audit: ReplayAudit::default(),
            entry: EntryArgs::default(),
            entry_mat_names: HashMap::new(),
        }
    }
}

/// 函数体里引用到的名字（含嵌套 lambda 与参数名）。**宁可多收**——多收只是少复用一点缓存，
/// 少收会把别人的结果当成自己的。语言形式与效应节点的名字也收（与步 12c 前按源码树收集同口径）。
fn referenced_names(f: &Function) -> BTreeSet<String> {
    fn go_block(b: &Block, out: &mut BTreeSet<String>) {
        for s in &b.statements {
            match s {
                Stmt::Let { value, .. } => go(value, out),
                Stmt::Function { function, .. } => go_block(&function.body, out),
                Stmt::Expr(e) => go(e, out),
            }
        }
        if let Some(r) = &b.result {
            go(r, out);
        }
    }
    fn go(e: &Expr, out: &mut BTreeSet<String>) {
        match kind(e) {
            K::Name(n) => {
                out.insert(n.to_string());
            }
            K::List(items) => items.iter().for_each(|x| go(x, out)),
            K::Record(fields) => fields.iter().for_each(|(_, x)| go(x, out)),
            K::Function(inner) => go_block(&inner.body, out),
            K::Call { callee, args } => {
                match callee {
                    Callee::Name(n) => {
                        out.insert(n.to_string());
                    }
                    Callee::Expr(c) => go(c, out),
                }
                args.iter().for_each(|x| go(x, out));
            }
            K::Field { value, .. } => go(value, out),
            K::Index { value, index } => {
                go(value, out);
                go(index, out);
            }
            K::Unary { value, .. } => go(value, out),
            K::Binary { left, right, .. } => {
                go(left, out);
                go(right, out);
            }
            K::If { condition, yes, no } => {
                go(condition, out);
                go_block(yes, out);
                go_block(no, out);
            }
            K::Block(b) => go_block(b, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    go_block(&f.body, &mut out);
    out
}

/// 读数用来排序的那个值；`None` = 失败或没答（J-12，排最后）
fn rank_value(ans: &dyn Answers, r: &Reading) -> Option<f64> {
    if r.fail.is_some() {
        return None;
    }
    match ans.answer_of(r)? {
        Answer::Noul(p) => Some(p),
        Answer::Choice(v) | Answer::Score(v) => Some(argmax(&v).1),
    }
}

/// 同题跨运行合并：noul / score 取均值，choice 取众数（`12`:134）
fn merge_runs(ans: &dyn Answers, rs: &[Rc<Reading>], method: &str, sp: Span) -> R<Answer> {
    let answers: Vec<Answer> = rs.iter().filter_map(|r| ans.answer_of(r)).collect();
    if answers.is_empty() {
        return err(
            Some("J-12"),
            "repeat 收到的读数全是失败或未答，没有可合并的",
            sp,
        );
    }
    // 逐分量取均值或中位数（B28）。choice / score 都是概率向量，逐分量合并，不投票。
    let 合 = |xs: &mut Vec<f64>| -> f64 {
        if xs.is_empty() {
            return 0.0;
        }
        if method == "median" {
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let m = xs.len() / 2;
            if xs.len() % 2 == 1 {
                xs[m]
            } else {
                (xs[m - 1] + xs[m]) / 2.0
            }
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let 向量 = |pick: &dyn Fn(&Answer) -> Option<Vec<f64>>, len: usize| -> Vec<f64> {
        (0..len)
            .map(|i| {
                let mut xs: Vec<f64> = answers
                    .iter()
                    .filter_map(|a| pick(a).and_then(|v| v.get(i).copied()))
                    .collect();
                合(&mut xs)
            })
            .collect()
    };
    Ok(match &answers[0] {
        Answer::Noul(_) => {
            let mut ps: Vec<f64> = answers
                .iter()
                .filter_map(|a| {
                    if let Answer::Noul(p) = a {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .collect();
            Answer::Noul(合(&mut ps))
        }
        Answer::Score(v0) => Answer::Score(向量(
            &|a| {
                if let Answer::Score(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
        Answer::Choice(v0) => Answer::Choice(向量(
            &|a| {
                if let Answer::Choice(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
    })
}

/// 这道题声明了、而状态里空着的证据槽（J-09）。与 Python `runtime.py:1133` 同口径：
/// 只查「槽不存在或为空」，不查内容。
fn missing_evidence(state: &State, q: &Question) -> Vec<String> {
    q.evidence
        .iter()
        .filter(|slot| {
            let v = match slot.as_str() {
                "on" => &state.on,
                "ctx" => &state.ctx,
                "ref" => &state.r#ref,
                "over" => &state.over,
                _ => return false,
            };
            v.is_empty()
        })
        .cloned()
        .collect()
}

/// `test`/`select`/`measure` 的可选第三参：`{evidence: [槽名…]}`（J-09）
fn evidence_of(v: Option<&Value>, sp: Span) -> R<Vec<String>> {
    let Some(v) = v else { return Ok(vec![]) };
    let Value::Record(fields) = v else {
        return err(
            Some("E-rt-question"),
            "题的第三个参数要是记录：{evidence: [\"ctx\", …]}",
            sp,
        );
    };
    let Some((_, slots)) = fields.iter().find(|(k, _)| k == "evidence") else {
        return Ok(vec![]);
    };
    let Value::List(l) = slots else {
        return err(Some("E-rt-question"), "evidence 要是槽名的列表", sp);
    };
    let mut out = vec![];
    for s in l.iter() {
        let Value::Text(t, _) = s else {
            return err(Some("E-rt-question"), "evidence 里要是槽名（文本）", sp);
        };
        if !matches!(t.as_ref(), "on" | "ctx" | "ref" | "over") {
            return err(
                Some("E-rt-question"),
                format!("evidence 里的 {t} 不是槽名；状态只有 on / ctx / ref / over 四个槽"),
                sp,
            );
        }
        out.push(t.to_string());
    }
    Ok(out)
}

/// 题上可读的字段（只读）。静态检查（check.rs）用同一张表核字段名。
pub const QUESTION_FIELDS: &[&str] = &[
    "text",
    "op",
    "calib",
    "scale",
    "evidence",
    "hash",
    "subject",
    "predicate",
    "partition",
    "request",
    "presupposition",
    "form",
    "template",
    "fill",
];
/// 题式上可读的字段（只读）。
pub const FORM_FIELDS: &[&str] = &[
    "template",
    "op",
    "slots",
    "calib",
    "scale",
    "evidence",
    "presupposition",
    "request",
    "partition",
    "subject",
    "hash",
];

/// 组合封闭性契约（B17，施工件 i）的字段。每个构造（`sieve` / `pair` / `tally` / `first_k` /
/// `iterate` / `outcome`）返回同一形状的记录，检查器据此核字段名。
/// - `kind`：产生它的构造；
/// - `value`：产出；
/// - `pending`：未决清单，每项 `{element, exit, cause}`，`exit` 承担责任（J-05 / 13 §3）；
/// - `evidence`：账本键（Text），不存读数或材料的副本；
/// - `resume`：续接——停在哪里、为什么、可选的继续方法 `next`；
/// - `spent`：本构造新增的调用与费用 `{calls, usd}`；
/// - `detail`：构造特有的已决信息（例如 sieve 的 `question`、`ignore`）；
/// - `purpose`：可选的可读目的，供诊断。
pub const OUTCOME_FIELDS: &[&str] = &[
    "kind", "value", "pending", "evidence", "resume", "spent", "detail", "purpose",
];

/// 这个值是不是一个契约值（字段集合与 `OUTCOME_FIELDS` 一致）
pub fn is_outcome(v: &Value) -> bool {
    match v {
        Value::Record(r) => {
            r.len() == OUTCOME_FIELDS.len()
                && OUTCOME_FIELDS.iter().all(|f| r.iter().any(|(k, _)| k == f))
        }
        _ => false,
    }
}

/// 证据列表去重追加（证据都是账本键 Text）
fn push_key(v: &mut Vec<Value>, k: Value) {
    let same = |a: &Value| matches!((a, &k), (Value::Text(x, _), Value::Text(y, _)) if x == y);
    if !v.iter().any(same) {
        v.push(k);
    }
}

fn list_of(v: Option<Value>) -> Vec<Value> {
    match v {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    }
}

fn texts(v: &[String]) -> Value {
    Value::list(v.iter().map(|x| Value::text(x)).collect())
}
fn opt_text(v: &Option<String>) -> Value {
    v.as_deref().map(Value::text).unwrap_or(Value::Unit)
}

/// B1 五件与题的元数据。`predicate` 就是题面：主体（被判断的对象）在状态里，不在题面里，
/// 题面说的是对它判断什么。由题式填出的题另有 `template`（带槽的谓词）与 `fill`（填法）。
fn question_field(q: &Question, field: &str) -> Option<Value> {
    Some(match field {
        "text" | "predicate" => Value::text(&q.text),
        "op" => Value::text(q.op.fixture_name()),
        "calib" => Value::text(&q.calib),
        "scale" => texts(&q.scale),
        "evidence" => texts(&q.evidence),
        "hash" => Value::text(&q.hash),
        "subject" => Value::text(q.subject()),
        "partition" => Value::text(q.partition()),
        "request" => Value::text(&q.request()),
        "presupposition" => opt_text(&q.presupposition),
        "form" => opt_text(&q.form_hash),
        "template" => opt_text(&q.template),
        "fill" => match &q.fill {
            Some(f) => Value::Record(Rc::new(
                f.iter().map(|(k, v)| (k.clone(), Value::text(v))).collect(),
            )),
            None => Value::Unit,
        },
        _ => return None,
    })
}

fn form_field(f: &jpp_value::value::Form, field: &str) -> Option<Value> {
    Some(match field {
        "template" => Value::text(&f.template),
        "op" => Value::text(f.op.fixture_name()),
        "slots" => texts(&f.slots),
        "calib" => Value::text(&f.calib),
        "scale" => texts(&f.scale),
        "evidence" => texts(&f.evidence),
        "presupposition" => opt_text(&f.presupposition),
        "request" => Value::text(
            f.request
                .as_deref()
                .unwrap_or(jpp_value::value::default_request(f.op)),
        ),
        "partition" => Value::text(match f.op {
            Op::Test => "binary",
            Op::Select => "k_ary",
            Op::Measure => "ordered",
        }),
        "subject" => Value::text(match f.op {
            Op::Select => "over",
            _ => "on",
        }),
        "hash" => Value::text(&f.hash),
        _ => return None,
    })
}

/// 题与题式的置换声明 `{permute: true}`（B64，步 15f）：只对 K 选一（`select`）有意义，值须为布尔。
/// 依据：B64（地基/附注/2026-09-24-探针首轮裁定.md，I-1(b)）
fn permute_of(v: Option<&Value>, op: Op, sp: Span) -> R<bool> {
    let declared = match v.and_then(|v| v.get("permute")) {
        None | Some(Value::Unit) => return Ok(false),
        Some(Value::Bool(b, _, _)) => b,
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!(
                    "permute 要是布尔（{{permute: true}}），收到 {}（依据：B64）",
                    other.type_name()
                ),
                sp,
            );
        }
    };
    if declared && op != Op::Select {
        return err(
            Some("E-rt-question"),
            format!(
                "permute 只用于 select（K 选一）：{} 题没有候选顺序可换（依据：B64）",
                op.fixture_name()
            ),
            sp,
        );
    }
    Ok(declared)
}

/// 题的声明项：前提（可选文本）与请求（本版只接受各题型的缺省请求，见下）。
fn question_decl_of(v: Option<&Value>, op: Op, sp: Span) -> R<(Option<String>, Option<String>)> {
    let Some(v) = v else { return Ok((None, None)) };
    let presupposition = match v.get("presupposition") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => Some(t.to_string()),
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("presupposition 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    let request = match v.get("request") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => {
            // 本版 `cut` 只实现每个题型的缺省请求。「K 选一、选出全部」（all）要由三路过滤
            // 与子集判断承担（施工件 c），在那之前声明它只会被静默当成 one——所以拒绝，而不是收下不管。
            if t.as_ref() != jpp_value::value::default_request(op) {
                let hint = if op == Op::Select && t.as_ref() == "all" {
                    "；「选出全部」待三路过滤（施工件 c）实现后可用，现在用 map + test 逐个判"
                } else {
                    ""
                };
                return err(
                    Some("E-rt-question"),
                    format!(
                        "request 「{t}」不适用于 {} 题：本版只支持缺省请求 {}{hint}",
                        op.fixture_name(),
                        jpp_value::value::default_request(op)
                    ),
                    sp,
                );
            }
            Some(t.to_string())
        }
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("request 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    Ok((presupposition, request))
}

/// 一组实参里各材料的来源出口键（`Mat.from_key`）的并（B59，步 17a；容器递归看，与 `derived_of` 同）。
/// 步 17c（B84）起取值级标签的 sources：标量叶子、材料、出口、题都算。
/// 步 18c（B92）起带边的种类：效应输出承接输入边，种类不变。
fn from_keys_of(args: &[Value]) -> Sources {
    args.iter()
        .fold(Provenance::trusted(), |p, a| prov_join(&p, &a.prov()))
        .sources
}

/// 出口交给 `pick`/`at` 臂的标签（B84）：出口 taint，sources = {出口键}，值依赖边（B92：k 由读数算出）。
fn 出口标签(e: &Exit) -> Provenance {
    Provenance::new(e.taint, Sources::value(&e.ledger_key.borrow(), &e.q_hash))
}

/// 元素记录的直接来源（B59，步 17a，结构通道）：`sieve` 元素有 `exit` 且账本键非空 → 该键；
/// `pair` 元素（无 `exit`、有 `left`/`right`）→ 两侧来源的并；其余为空。
/// 只取直接来源，更早的祖先经它们自己的 `hop` 计入。经普通值（`e.item` 取出）的依赖不在此列（候选 B84）。
fn element_lineage(it: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !matches!(it, Value::Record(_)) {
        return out;
    }
    match it.get("exit") {
        Some(Value::Exit(e)) | Some(Value::Duty(e)) => {
            let k = e.ledger_key.borrow().clone();
            if !k.is_empty() {
                out.insert(k);
            }
        }
        _ => {
            for side in ["left", "right"] {
                if let Some(v) = it.get(side) {
                    out.extend(element_lineage(&v));
                }
            }
        }
    }
    out
}

/// 一组实参里各材料的 `derived_from` 的并（容器要递归看，与 `taint_of` 同）
fn derived_of(args: &[Value]) -> BTreeSet<String> {
    fn go(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Mat(m) => out.extend(m.derived_from.iter().cloned()),
            Value::List(l) => l.iter().for_each(|x| go(x, out)),
            Value::Record(fs) => fs.iter().for_each(|(_, x)| go(x, out)),
            Value::Stop(x) => go(x, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    args.iter().for_each(|a| go(a, &mut out));
    out
}

/// 不在分派处做「输出 ∨ 输入」的内置（B33 第 3 点）。两类：
/// 1. **效应边界与自带规则**：taint 按 `12` §2.11 表在这里赋值（`state`/`mat`/`content`/`judge`/
///    `cut`/`do`/`gen`/`ask`/`transform`…），或输出本身就带着该有的位（出口、材料、契约值）；
/// 2. **只搬运元素**：输出的元素就是输入的元素（或用户函数的返回值），各带自身的位；
///    整体 ∨ 会把一个不可信元素的位抹到所有元素上（取字段 / 下标返回叶子自身的位，同一原则）。
/// 不在表上的内置（含将来新增的）一律按 ∨ 输入处理——**兜底往拒绝那边倒**。
const 不做数据流合取的内置: &[&str] = &[
    // 效应边界与自带规则
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "cut",
    "handle",
    "consume",
    "do",
    "gen",
    "ask",
    "transform",
    "mat",
    "content",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "repeat",
    "agg",
    "allocate",
    "unsure_bound",
    "fit",
    "order",
    "escalate",
    "literalize",
    "unsure",
    "pending",
    "print",
    "stop",
    "fail",
    // 只搬运元素
    "map",
    "filter",
    "fold",
    "loop",
    "append",
    "concat",
    "slice",
    "reverse",
    "with",
];

/// 一个值携带的 taint（B33：标量自带位，容器递归 ∨）
fn taint_of(v: &Value) -> Taint {
    v.taint()
}

/// 13 §6 的运行错误：说清是哪一步越界、越的是哪个界，并带 `.jpp` 的 Span
fn overflow(what: &str, a: i64, b: i64, sp: Span) -> Fault {
    Fault::Error(RtError::new(
        Some("E-rt-int"),
        format!(
            "Int {what}溢出：{a} 与 {b} 的结果超出有符号 64 位范围（{} … {}）。Int 是 64 位有符号整数，溢出是错误不是回绕",
            i64::MIN,
            i64::MAX
        ),
        sp,
    ))
}

use jpp_value::bridge::argmax;

pub fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Unit,
        Json::Bool(b) => Value::Bool(*b, Taint::Trusted.into(), GuardEv::EMPTY),
        Json::Number(n) => n
            .as_i64()
            .map(Value::int)
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0), Taint::Trusted.into())),
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(json_to_value).collect()),
        Json::Object(o) => Value::record(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

/// 效应输出写进账本（账本 v3，步 18a）：`(output, output_mat)`。材料输出的内容进 `output`，
/// 地址、来源链、taint 与来源边（带种类，B92）进 `output_mat`；`derived_from` 不写（B84：值依赖边的投影，
/// 读回时重算）。失败值写 `{"__fail", "taint"}`，其余写值本身、`output_mat` 为空。
pub fn effect_value_to_entry(v: &Value) -> (Json, Option<MatMeta>) {
    match v {
        Value::Fail(s, t) => (json!({"__fail": s.as_ref(), "taint": t.taint}), None),
        Value::Mat(m) => {
            let sources = m
                .prov()
                .sources
                .edges()
                .map(|(k, e)| SourceEdge {
                    key: k.clone(),
                    kind: e.kind,
                    q: e.q.clone(),
                })
                .collect();
            (
                m.content.clone(),
                Some(MatMeta {
                    addr: m.addr.clone(),
                    origin: m.origin.clone(),
                    taint: m.taint,
                    sources,
                }),
            )
        }
        other => (other.to_json(), None),
    }
}

/// 从账本读回效应输出（账本 v3）。`output_mat` 在即为材料：来源边按种类还原，`derived_from` 由值依赖边重算。
pub fn entry_to_effect_value(output: &Json, output_mat: Option<&MatMeta>) -> Value {
    if let Some(meta) = output_mat {
        let edges = meta
            .sources
            .iter()
            .map(|e| {
                (
                    e.key.clone(),
                    jpp_value::prov::Edge {
                        kind: e.kind,
                        q: e.q.clone(),
                    },
                )
            })
            .collect();
        return Value::Mat(Rc::new(
            Mat::new(
                output.clone(),
                &meta.addr,
                meta.origin.clone(),
                meta.taint,
                BTreeSet::new(),
            )
            .with_sources(&Sources::from_map(edges)),
        ));
    }
    if let Some(f) = output.get("__fail").and_then(|x| x.as_str()) {
        // 失败值的 taint 缺了或坏了：兜底往拒绝那边倒（untrusted），与材料的反序列化同一纪律
        let t = output
            .get("taint")
            .and_then(|x| serde_json::from_value::<Taint>(x.clone()).ok())
            .unwrap_or(Taint::Untrusted);
        return Value::Fail(Rc::from(f), t.into());
    }
    json_to_value(output)
}

/// 缺席类未决原因（B95）：没观察到的项，不是判过而拿不准的项；不能 drop（`E-drop-unobserved`）
const 缺席类原因: &[&str] = &["budget", "absent", "latency"];

/// 返回值里带着哪些出口 / 未决责任（可达性核，`20` v2 §3.5）。
fn collect_exit_ids(v: &Value, out: &mut HashSet<usize>) {
    visit_exits(v, &mut |e| {
        out.insert(e.id);
    });
}

/// 按可达性走一个值里的出口与未决责任，逐个交给 `f`（同一出口可能经多条路径被交多次）。
fn visit_exits(v: &Value, f: &mut dyn FnMut(&Rc<Exit>)) {
    match v {
        Value::Exit(e) | Value::Duty(e) => f(e),
        // 惰性出口（B94）：解析了按出口算；未解析的只会在它自己那一帧还活着时出现，帧返回前必解析
        Value::Cut(c) => {
            if let Some(e) = c.exit() {
                f(&e)
            }
        }
        Value::List(l) => l.iter().for_each(|x| visit_exits(x, f)),
        Value::Record(r) => r.iter().for_each(|(_, x)| visit_exits(x, f)),
        Value::Stop(x) => visit_exits(x, f),
        // 方法的**捕获环境**里也可能装着责任。`13` §3 明列「随返回值/继续方法交给调用者」
        // 是合法去向，而类型侧早就用 `captures_responsibility` 认了方法能捕获责任——
        // 扫描侧不进环境，就成了内核两半打架：合法的续接方法被判成「责任丢了」。
        Value::Fn(c) => visit_closure(c, 3, f),
        _ => {}
    }
}

/// 经闭包捕获环境可达的责任（B52：可达性延伸进闭包捕获）。
/// 已用掉的 Fn¹（判为唯一路径后调用过一次）不再是它那几条责任的路径：责任在那次调用里已按
/// 实际去向处置，闭包不能再调用，经它「可达」只是字面上的（依据：B52、`13` §3）。
fn visit_closure(c: &Closure, depth: u32, f: &mut dyn FnMut(&Rc<Exit>)) {
    let spent: Vec<usize> = if c.linear_called.get() {
        c.linear.borrow().clone()
    } else {
        vec![]
    };
    let mut g = |e: &Rc<Exit>| {
        if !spent.contains(&e.id) {
            f(e)
        }
    };
    visit_env(&c.env, &referenced_names(&c.function), depth, &mut g);
}

/// 从捕获环境里找责任。只看方法体**实际引用到**的名字，不把共享环境链里所有可达名字都算成捕获
/// （Codex 陷阱 5 的后半句）；`depth` 防递归环境链无限展开。
fn visit_env(env: &Env, names: &BTreeSet<String>, depth: u32, f: &mut dyn FnMut(&Rc<Exit>)) {
    if depth == 0 {
        return;
    }
    for n in names {
        let Some(v) = env_lookup(env, n) else {
            continue;
        };
        match &v {
            Value::Fn(c) => {
                if depth > 1 {
                    visit_closure(c, depth - 1, f)
                }
            }
            other => visit_exits(other, f),
        }
    }
}

/// 闭包创建时经捕获环境可达的未销账未决责任（`Closure.captures`，B52，步 21）
fn captured_duties(f: &Function, env: &Env) -> Vec<usize> {
    let mut out = vec![];
    visit_env(env, &referenced_names(f), 3, &mut |e| {
        if e.is_unsure() && !e.consumed.get() && !out.contains(&e.id) {
            out.push(e.id);
        }
    });
    // 未解析的惰性出口（B94）：种类还不知道，按可能是未决记下它预分配的出口号（多记只让 Fn¹ 判定
    // 多一个候选，是否真是未决在帧返回时按出口核）
    for n in referenced_names(f) {
        if let Some(v) = env_lookup(env, &n) {
            未解析出口号(&v, &mut out);
        }
    }
    out
}

fn 未解析出口号(v: &Value, out: &mut Vec<usize>) {
    match v {
        Value::Cut(c) if c.exit().is_none() => {
            if !out.contains(&c.id) {
                out.push(c.id)
            }
        }
        Value::List(l) => l.iter().for_each(|x| 未解析出口号(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| 未解析出口号(x, out)),
        Value::Stop(x) => 未解析出口号(x, out),
        _ => {}
    }
}

/// 返回值里每条责任的可达路径（B52 的 Fn¹ 判定用）：`direct` 是不经任何闭包可达的出口 id；
/// `via` 是经闭包可达的出口 id → 途经的**值层**闭包（按身份去重；嵌在闭包环境里的闭包算作外层那个）。
fn exit_paths(v: &Value, direct: &mut HashSet<usize>, via: &mut HashMap<usize, Vec<Rc<Closure>>>) {
    match v {
        Value::Exit(e) | Value::Duty(e) => {
            direct.insert(e.id);
        }
        Value::Cut(c) => {
            direct.insert(c.id);
        }
        Value::List(l) => l.iter().for_each(|x| exit_paths(x, direct, via)),
        Value::Record(r) => r.iter().for_each(|(_, x)| exit_paths(x, direct, via)),
        Value::Stop(x) => exit_paths(x, direct, via),
        Value::Fn(c) => visit_closure(c, 3, &mut |e| {
            let cs = via.entry(e.id).or_default();
            if !cs.iter().any(|x| Rc::ptr_eq(x, c)) {
                cs.push(c.clone());
            }
        }),
        _ => {}
    }
}

/// 一个元素交给判断器的那份材料与来路：
/// 过滤或配对的产物（带 `item` 与 `trail` 的记录）取 `item` 当材料，`trail` 接上上一次的出口；
/// 其余值原样当材料、来路为空。产物与输入同形，可再过滤、再配对（组合封闭）。
/// （输出元素的构造 `element_out`、`is_element` 在步 25-2b 搬进 `constructs/element.rs`，B133。）
fn element_parts(it: &Value) -> (Value, Value) {
    let is_elem =
        matches!(it, Value::Record(_)) && it.get("item").is_some() && it.get("trail").is_some();
    if !is_elem {
        return (it.clone(), Value::list(vec![]));
    }
    let mut t: Vec<Value> = match it.get("trail") {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    };
    if let Some(e) = it.get("exit") {
        if !matches!(e, Value::Unit) {
            t.push(e);
        }
    }
    (it.get("item").unwrap_or(Value::Unit), Value::list(t))
}
