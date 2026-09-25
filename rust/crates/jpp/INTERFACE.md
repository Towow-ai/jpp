# jpp-core 接口说明（给前端 lower 与 CLI 接线用）

版本：首包，2026-09-21。对应 crate `jpp-core 0.1.0`，edition 2024。

core 负责共同程序表示、值与环境、静态检查、解释执行、效应适配、账本与重放。源码经 `jpp-syntax`
（原 `jpp-frontend`，步 12d 改名）解析成表层 AST，再由 `jpp_core::lower` 降到 IR（`ir::Program`）；
检查器与解释器只读 IR。语义依据是 `12-IR与类契约-v0.1.md` 的六种效应形式与 J-01…J-18，诊断编号
依据 `11-语言规范-v1.md` §诊断；Rust 自己的类型系统不替代 J++ 的检查器，纪律由检查器与解释器两处把关。

## 一、程序表示（步 12d 起是 IR）

```rust
// 入口（jpp-core 外观）
pub fn lower(p: &syntax::ast::Program) -> Result<ir::Program, Vec<syntax::Diagnostic>>;
pub fn check(program: &ir::Program) -> Report;          // 以及 check_with_profile / check_with_calib / check_with_calib_actions（B108，步 24-0：多带宿主动作表）
pub fn run(program: &ir::Program, …) -> Result<Outcome, Error>;   // 以及 run_replay / run_with_fits / run_unchecked

// jpp_ir::ir（节选；完整定义见 crates/jpp-ir/src/ir/）
pub struct Program { pub version: u32, pub budget: Budget, pub body: Block, pub sites: SiteTable, pub span: Span }
pub struct Budget  { pub calls: u64, pub cost: f64, pub depth: Option<u32>, pub escalate: Option<u64>,
                     pub unsure: Option<f64>, pub absent: Option<AbsentPolicy>, pub latency_p95: Option<f64> }
pub struct Expr    { pub id: NodeId, pub node: Node, pub span: Span }
pub enum Node { State{..}, Effect{..}, Cut{..}, Fit{..}, Loop{..}, Handle{..}, Consume{..}, Construct{..}, Host(Host) }
pub struct Parameter { pub name: String, pub annotation: Option<Type>, pub span: Span }
pub enum Type {
    Named(TypeName),
    Applied(TypeName, Vec<Type>),
    Function(Vec<Type>, Box<Type>),   // 旧形式：效应行未知
    Method(MethodType),               // 方法类型：效应行与责任捕获跟着类型走
}
pub enum TypeName { Int, Decimal, Float, Bool, Text, List, Record, Unit, Fn, Method,
                    Question, Form, Outcome, Exit, Other(String) }   // 序列化为作者写的原文
pub struct MethodType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
    pub effects: Option<Vec<String>>,    // 效应行；None = 未知/待推断
    pub captures_responsibility: bool,   // true = Fn¹，false = Fnω
}
```

**降级做的事（`jpp-syntax::lower`，名字表由 `jpp_core::names::CurrentNames` 组装）。** 调用位置上的名字
按三张表解析成节点：效应表（`jpp-effects` 注册表）里的名字 → `Node::Effect`（输入按效应的槽名排列），
构造表里的名字 → `Node::Construct`，`state`/`cut`/`fit`/`loop`/`handle`/`consume`/`escalate`/`literalize`
各有自己的节点，`map`/`filter`/`fold` 是带站点的宿主调用，其余是宿主调用或用户名字；被 `let`、函数名、
形参遮蔽的名字按用户名字处理。每个站点进站点表，记所属函数与最近外层 `Loop`/高阶/构造站点。作者写的
类型标注解析成类型化的 `Type`（类型名成为 `TypeName`）；降级不推断表达式类别。函数的 `source_hash`
（闭包身份、`transform` 键与捕获指纹）按步 12c 的口径算，账本键不变。

**降级报的两条诊断**（`syntax::Diagnostic`，报文以规则号开头）：`J-07a` 缺 `budget` 或 `budget` 缺
`calls`/`cost`（预算必填，检查器不再报缺预算）；`E-form-as-value` 效应名、语言形式名或内核构造名出现在
调用位置以外（赋值、传参、放进容器；`20·B56`），只有宿主内置与用户名字可以作值。

**`Type::Method` 是给前端加的位置。** 方法一旦经参数、记录字段、返回值传递，`Function` 定义节点上的
`!{…}` 就跟不过去了，契约在边界上丢掉。`Type::Method` 把效应行放进**类型**本身，所以
`fn solve(input: Mat, method: Fn(Record) -> Record !{judge})` 里的 `method(s)` 不再是「静态判不了」。
`captures_responsibility` 区分 Codex 说的 `Fn¹`（捕获了未决责任，不可重复调用、不可丢弃）与 `Fnω`；
core 现在会拦住把 `Fn¹` 交给 `map` / `filter` 的写法。文法已支持类型位上的效应行：
`Fn(Record) -!{judge}-> Record` 降成 `Type::Method`，不写效应行的 `Fn(A) -> B` 仍是 `Type::Function`。

`effects` 里的名字目前只认 `judge` / `gen` / `do` / `ask`；`transform` 是记账变换，不是效应形式，不写进标注。

全部 IR 节点 `#[derive(Serialize, Deserialize)]`，可往返；`jpp_ir::ir::print` 给出可 diff 的文本形式。

## 二、值与环境

```rust
pub enum Value {
    Unit, Int(i64), Float(f64), Bool(bool), Text(Rc<str>),
    List(Rc<Vec<Value>>), Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>), Builtin(&'static str),
    Mat(Rc<Mat>), State(Rc<State>), Question(Rc<Question>), Reading(Rc<Reading>), Exit(Rc<Exit>),
    Duty(Rc<Exit>),         // 未决责任 U(q)：unsure 臂收到的就是它
    Fail(Rc<str>),          // do 失败是值，不是异常（J-12）
    Stop(Rc<Value>),        // loop 的显式停止
}

pub struct Closure { pub function: Function, pub env: Env, pub name: Option<String>, pub span: Span, pub hash: String,
                    pub captures: Vec<usize>, pub linear: RefCell<Vec<usize>>, pub linear_called: Cell<bool> }  // 后三项：步 21（B52），不进 hash
pub struct EnvNode { pub vars: RefCell<Vec<(String, Value)>>, pub parent: Option<Env> }
pub type Env = Rc<EnvNode>;
```

**函数值 = 参数表 + 函数体（core AST）+ 显式环境链**，没有 Rust 闭包。环境是一串名字→值的节点，
可以打印（`env_names`）、可以跟着链走。`Value::to_json()` 把函数值渲染成
`{"fn": 名字, "params": [...], "env": [[本层名字…], [上层名字…]], "hash": …}`；账本里存的是效应记录，
不是序列化的闭包，重放靠同一份程序重建环境。

一个块里的绑定共享同一个环境节点，所以自递归和「后定义、先被前面的函数体引用」都成立。检查器的名字
解析按同一口径：函数体里能看见所属块的全部绑定，语句位置的直接引用才要求先定义后使用。

读数（`Value::Reading`）没有可读的值：不能比较、不能做算术、不能进状态槽、不能取字段，只能经 `cut`
变成出口（J-01）。`Value::to_json()` 对读数只露 `{reading: 账本键, q, state, op}` 这几项元数据。

## 三、内置操作与签名

根环境里的名字（`interp::BUILTINS`）。用户可以用同名绑定盖住它们，检查器会报 `W-shadow` 提示。

**材料与状态**

| 签名 | 说明 |
| --- | --- |
| `mat(v) -> Mat` | 任意值变材料；出口也能变回材料（带 `derived_from`，J-02 禁自指） |
| `content(m: Mat) -> Value` | 取材料内容。读数进来是 J-01 错 |
| `state(on) -> State` / `state(on, {ctx, ref, over}) -> State` | `on` 恰一个判断对象（关系用一对，J-14）；`over` 是候选集 |
| `transform(f: Fn, mats…) -> Mat` | 记账变换：进槽的材料只能来自字面量、效应输出或这里（J-11）。输出不能是读数/出口/函数/状态 |

**题与判断**

| 签名 | 说明 |
| --- | --- |
| `test(题面: Text, calib: Text) -> Question` | 是非题，下沉成 noul |
| `select(题面: Text, calib: Text) -> Question` | 在 `over` 里挑一个，下沉成 choice |
| `measure(题面: Text, [档位: Text…], calib: Text) -> Question` | 分档题，至少两档，下沉成 score |
| `form(题型: Text, 模板: Text, {calib, scale?, evidence?, presupposition?, request?}) -> Form` | 题式（带 `{槽}` 的题模板）。题型为 `"test"` / `"select"` / `"measure"`；calib 必填且不能是数字（J-03） |
| `fill(题式: Form, {槽: 值…}) -> Question` | 按填法得到题。槽必须恰好填满（缺槽、多槽都报错）；值取 Text / Int / Float / Bool 的文本形式，读数不能填（J-01） |
| `judge(state, question) -> Reading` / `judge(state, [question…]) -> [Reading]` | 一状态多题一次问完 |
| `cut(reading) -> Exit` / `cut(reading, calib_key: Text) -> Exit` | 过线。`calib` 位只收校准记录的**键**，字面量线是 J-03 错 |
| `ask(state, question) -> Exit` | 问人。没答就是程序级 Pending；次数受 `budget.escalate` 管（J-07） |

`calib` 是校准记录的键，不是线。线只从 `CalibStore` 里的记录来；记录状态是「冷」或「停岗」时
`cut` 直接给 `Unsure(cold)` / `Unsure(drift)`，不看概率。

**出口**

| 签名 | 说明 |
| --- | --- |
| `handle(exit, {act, ignore, pick, at, unsure, otherwise}) -> Value` | 按题型穷尽：test 要 `act`/`ignore`/`unsure`，select 要 `pick`/`unsure`，measure 要 `at`/`unsure`；`otherwise` 兜底。臂可以是值，也可以是方法（`pick`/`at` 的方法收一个 Int，`unsure` 的收一个 Text 原因） |
| `consume(exit \| [exit…], "drop") -> Unit` | 显式丢弃并记账 |
| `exit_kind(exit) -> Text` | `act` / `ignore` / `pick(k)` / `at(l)` / `unsure(cause)` |
| `unsure(cause: Text) -> Exit` | 源码自己造一个未决出口 |
| `pending(reason: Text)` | 程序级挂起，整个程序停在这里 |

每个出口都带 `consumed` 标记。程序返回前还有未消费的 `Unsure` 就是 J-05 错；函数要把出口带出去，
返回类型必须提到 `Exit`（`-> Exit`、`-> Record<Exit>` 之类），最外层允许带出并记在
`Outcome.returned_unsure` 里。

**未决责任（`U(q)`）**

`handle` 的 `unsure` 臂收到的不是原因文本，是**未决责任本身**——一个 `Value::Duty`，与出口共享同一份
销账记录。它不可伪造（只能由 `handle` 交付），把它变成材料或 JSON 也不消除义务。**进臂不等于销账**：
臂体跑完，core 会核这份责任是不是真的交出去了，没交就是 J-05 错，报文指着那条臂。

| 签名 | 说明 |
| --- | --- |
| `unsure_cause(u) -> Text` | 读**路由键**（`band` / `cold` / `tie` / `untested` / `fail:…`）。读取不转移责任，只读不算处理 |
| `untested(u) -> Text` | 读 **J-15 那一位**：这个出口引用的判据里哪个量在本次路径上**没被测量**（`""` = 都测了）。与 `unsure_cause` 正交、同样只读不销账 |
| `escalate(u, state, question) -> Exit` | 把责任交给明确关联的人工请求，效应 `ask`。未答即程序级 Pending |
| `literalize(u, state, question) -> Exit` | 接走旧责任，按更字面的题重问，效应 `judge`。换来的新出口仍要自己处理 |
| `unsure(u) -> Exit` | 重新包装成出口，继续由调用者负责（`unsure(原因: Text)` 仍是新造一个） |
| `consume(u, "drop")` | 显式丢弃并记账。`12` §6 允许这条路，core 留一条 `W-drop-vs-escalate` 提示。步 21（B95）起：不收契约值（J-05，逐项丢或转交）；缺席类原因（`budget`、`absent`、`latency`）的出口报 `E-drop-unobserved`；丢了又出现在程序返回值里报 `W-drop-then-return` |

第四条去向是**把 `u` 放进臂的返回值**，责任随数据交给调用者。四条都不走，就是静默丢弃。

`otherwise` 兜不住 `Unsure`：三种题的出口都可能是未决，`unsure` 必须自己写一臂（通配只能替 `act` /
`ignore` / `pick` / `at`）。臂必须是收至少一个参数的方法——字面量收不下责任，这条静态就报。

**效应与失败**

| 签名 | 说明 |
| --- | --- |
| `do(动作名: Text, [参数…], iter_seq: Int) -> Mat \| Fail` | 只能触发 `ActionRegistry` 登记过的动作（J-11）。循环里 `iter_seq` 必须随轮次变（J-13） |
| `gen(prompt: Text, [ctx], n: Int, retry_seq: Int) -> [Mat]` | 同上，重试必须递增 `retry_seq` |
| `fail(reason: Text) -> Fail` / `is_fail(v) -> Bool` | Fail 是值，进账本，判断时走 `Unsure(fail)`（J-12） |

**有界控制与数据**

`loop(bound: Int, 初值, fn(acc, i))` 是唯一的循环，`bound` 必须在（J-06），体内 `stop(v)` 显式停止；
一轮之内账本键重复即停（J-06）并报 `W-noprogress`。`map(list, fn)` / `filter(list, fn)` 是纯映射，
体内不能含 `loop` / `stop`（E7）。`fold(list, 初值, fn(acc, x))` 用来表达带状态的迭代。

其余：`len`、`range(a, b)`、`append`、`concat`、`slice`、`contains`、`sum`、`reverse`、`keys`、
`has(record, key)`、`with(record, key, value)`、`text`、`join`、`print`、`min`、`max`、`abs`、`floor`。

**长处（G4「发挥不确定性长处」）**

前面几条都是纪律——不许把读数当值、不许静默丢未决。这两条相反：读数是带校准线的随机变量，
所以它有一个裸调用拿不到的量——**离决定带多远**。

| 签名 | 说明 |
| --- | --- |
| `allocate(读数们, k: Int) -> [Int]` | 把 k 份复核分给最不确定的读数，返回下标；按不确定度降序，并列按下标升序。`k` 通常取 `budget.escalate`，负数即错 |
| `unsure_bound(读数们) -> Record` | J-10：`{n, union_bound, independent_any, n_unknown}`。`union_bound` = Σuᵢ（被 n 夹住）是**判据**；`independent_any` = 1−Π(1−uᵢ) **只作参考值**（红队 06 B4：独立假设在题相关时方向不定）；uᵢ 取各题校准记录的 `unsure_rate`，非上岗或没有这个值的按 1 计并进 `n_unknown` |

**适用范围（E-ALLOC 实测，别含糊）**：`allocate` 的区分力**要求校准键是上岗的**。
键是冷的时候线取档案的保守线，带很宽——E-CAL 那 202 条真读数里 **85–89% 落在带内**，
不确定度一律并列为 0，并列按下标升序，于是 `allocate` **退化成「按文件顺序取前 k 条」**，
实测与随机分配无显著差别（noul 上甚至略差）。换成扫出来的上岗线后，三种题型在每个 k>0 上
都比随机好，noul 最稳（k=30 时错误率 0.110 对 0.178，相对少错 38%）。
**第二条适用范围，比第一条更常撞上**：即便键是上岗的，**带内并列比例高的键上区分力也弱**。
带宽是档案的 δ 与校准记录的 hi/lo 定的，不是「键冷不冷」决定的——同一批数据上，上岗线下的
并列比例是 choice 3% / noul 51% / **score 75%**，证据强度就按这个顺序排。
核对臂里 score 那 36 条**并列 100%**，那一格的 `allocate` 仍然退化成顺序选取，
它看着是正的 +0.05 只是抽样顺序的巧合，不算证据。
**所以判断能不能指望 `allocate`，看的是这个键上有多少读数落在带内，不是只看键上没上岗。**

所以：**冷键上不要指望它**；样本 202 条、未做区间估计，那个效应量不可外推。
数据与两轮对照见 `foundation/experiments/前提结论.md` 的 E-ALLOC 一节。

两个都**只接受读数**（J-01，出口传进来即错），都**不花钱**：不发调用、不进账本、不动预算。
合法性沿用 Python 内核 `allocate` 的 docstring 原话——「它像 cut 一样只读校准线，不做跨题算术；
返回宿主整数，不返回读数」。Rust 侧另有纯函数版 `jpp_core::strength::{allocate, unsure_bound,
uncertainty}`，直接收 `&[Rc<Reading>]`，不必起解释器。

**档案（`Profile`）**：数字都是**档案字段**，不是代码常数（`12` §1）。步 15d 起按效应实例分表、每个字段是
`Field`（已测给值与来源，未测为空）；步 15d-2 起**没有 `Profile::default`**，唯一默认是 `Profile::untested()`。
`Profile::load(路径)` 从档案 JSON 读 `lines.safety_default.{hi,lo}`（冷键保守线，只供 `allocate` 排序）与
`delta.{noul,choice_prob_chosen,score}.immediate.p99`（δ 先验）——**取值路径与 Python 一致，不是抄结果**。
**`cut` 用的 δ 只从校准记录取**（`20` §3.9）：选中证书记了 `selection.delta` 用它；certify 线与代价线不平移，
δ = 0（批量裁定解读 (a)）；否则用记录的 `delta`（夹具的 `calibrations[].delta`、`CalibStore::put` 第 6 参、
`set_delta`）；都没有则出口 `Unsure(untested)`、载体 `Delta`。画像的 δ 只作认证先验：`calib-import` 从
`--profile` 取，取不到报错，不回退任何默认值。已决区边界按 `jpp_value::stat::decided_up/down`（带 1e-9 往返容差）
比较，认证、`unsure_rate`、复核已决区与 `cut` 共用它。
`tests/profile.rs` 对着 Python oracle 断言同一组线与 δ、同一个 `profile_hash`。

三个决定，写明理由：
- **读不到档案就报错，不悄悄回退兜底值**——那正是「替不确定说确定」。没有兜底值可写（步 15d-2）；
  不接档案的调用方用 `Profile::untested()`，字段全部未测。
- **真机运行必须有画像（B73，步 15d-0）**：`--backend live` 的首跑与续接按 `--profile <文件>`，
  否则按 `--profiles-dir <目录>/<model>.json`，再否则按可执行文件旁的 `profiles/<model>.json`
  解析；解析不到报 `E-profile-missing` 并写明试过的路径，不回退兜底值。路径只在 CLI 解析，
  内核只收 `Profile` 值。发行附带 `profiles/jev-1.13.0.json`（`scripts/gen_profiles.py` 生成）。
  价格只从画像 `cost.price_usd_per_input_token` 读，没有价格时费用记 `Unknown` 并报
  `W-cost-unknown`。重放不发调用，不要求画像。
  单次真机请求的超时只从画像 `transport.timeout_s` 读（发行画像里是宿主策略值 30 s，非实测）；
  超时即 `E-timeout` 传输错误，属 absent，按 `budget.absent` 逐次计费重试与路由，没声明 `absent`
  即 `E-rt-client`；画像没有这个字段时不设超时、报 `W-untested: transport.timeout_s`
  （`JevClient::with_timed_transport`、`effects::timed`；过程记录 `工程-传输超时.md`）。
- **固定观察可以无画像运行**（步 15d-2 起画像字段全部未测：窗口不核并报 `W-window-untested`，线与 δ 只从
  校准记录取），「档案：未加载，画像字段全部未测」这行提示**无条件打印**。「读不到就报错」管的是给了路径而读不到；
  没给路径时，真机报 `E-profile-missing`，固定观察打印提示，两条合成一个口径。
- **兜底档案的 `hash` 是 `None`**，账本头照此记；真机运行有画像，`profile_hash` 不再为空。否则「用了兜底」与「档案恰好等于兜底」
  在账本上分不开。`profile_hash` 变化（含有→无）会报 `W-header`。
- **头在 `run()` 入口定稿**，不等跑完补齐：档案是运行前就定下的输入。

`profile_hash` 与 Python 的 `H(profile)` 同值（`sha256(canon([profile]))[:16]`）。为此
`canon` 的指数写法对齐了 Python 的 `json.dumps`（`4.2e-08` 两位指数，serde_json 原本出 `4.2e-8`）
——同一份档案在两边算出不同哈希的话，跨内核的重放判定就废了。

**不确定度**是「与决定带的距离取负」：带内 = 0（最不确定），带外越远越确定。带是
`[lo − δ, hi + δ]`；线来自校准记录（上岗）或**档案的保守线**（其余，不是记录自己的线），
δ 来自记录或档案。这些数都是**档案字段**（`12` §1「凡是数字都是档案字段」），在
`CalibStore::profile`（`Profile { safety, delta }`）里，缺省取 Python 内核代码里的同名兜底
（冷键无线 hi=1/lo=0，δ 为 noul 0.05 / choice 0.15 / score 0.15）；接线时应当从档案覆盖。
`CalibStore::set_unsure_rate(键, 率)` 与 `set_delta(键, δ)` 写记录上的那两个值。

这两个构件是**有对照的移植**：Python 内核 `foundation/jv/runtime.py:1236`（`allocate`）与
`:1246`（`unsure_bound`）是 oracle。`tests/oracle/generate.py` 把 Python 侧跑出来存成
`tests/oracle/oracle.json`，`tests/allocate.rs` 断言 Rust 选出同一批下标、给出同一组界
（含并列时的确定性顺序、`round(x, 4)` 的四舍六入五成双、冷记录的线从档案取）。

**类型名**（标注里认得的）：`Int`、`Decimal`/`Float`、`Bool`、`Text`、`List`、`Record`、`Unit`、
`Fn(…)->…`。`Mat`、`State`、`Question`、`Reading`、`Exit`、`Method` 这些标了也接受，但静态不据此
判参数——它们由运行期把关。检查器只在「标注是上面那几个基本档、实参又是字面量」时判不符（`E-type`）。

## 三·四、题与题式是一等值（施工件 b，2026-09-23）

对应 B 栏 B1（题的五件：主体、谓词、划分、请求、前提，待 Nature 批准）与 `08` §1 自指。依据文本 `12` §2.2 本体未改；本节只记实现现状。

**题的只读字段**（`q.字段`；静态检查按同一张表核字段名，写错报 `E-field` 带修法）：

| 字段 | 值 | 说明 |
|---|---|---|
| `text` / `predicate` | Text | 题面即谓词：主体（被判断的对象）在状态里，不在题面里 |
| `op` | `"test"` / `"select"` / `"measure"` | |
| `subject` | `"on"` / `"over"` | 主体读哪个槽：是非、打分读 `on`；K 选一读 `over`。关系题读一对，由状态决定 |
| `partition` | `"binary"` / `"k_ary"` / `"ordered"` | 由题型推出 |
| `request` | `"whether"` / `"one"` / `"degree"` | 本版只接受各题型的缺省请求。K 选一的 `all`（选出全部）会报错并提示：待三路过滤（件 c） |
| `presupposition` | Text 或 unit | 声明项。本版不发给判断器、不进题哈希、不改 `cut`；前提不成立的出口是 B4 的事 |
| `calib` `scale` `evidence` `hash` | | 与构造时一致 |
| `form` `template` `fill` | 题式哈希 / 模板 / 填法记录，手写的题为 unit | |

**题式的只读字段**：`template`、`op`、`slots`、`calib`、`scale`、`evidence`、`presupposition`、`request`、`partition`、`subject`、`hash`。

**不变的东西（有意为之）**：
- 题哈希只由题型、题面、档位、证据槽决定，题式来源、填法、前提都不进哈希。同题面同题型就是同一道题——账本键与校准查找不会因为「手写」与「由题式填出」而分裂。
- 校准键仍是题声明的 `calib` 字符串（B2 待第三轮实验与 Nature 裁定）。由于同一题式的所有填法共用题式声明的 `calib`，**「校准挂题式、填法继承线」在现有键上已经能表达**；题上记的 `form` 哈希留给日后若改主键时使用。
- `test` / `select` 的第三个参数记录现在也接受 `presupposition` 与 `request`（与 `evidence` 并列）。
- J-02 禁自指不变。J-10 的静态 unsure 预算按调用点计 `test`/`select`/`measure`，**尚不计 `fill` 产生的题**（遗留）。


## 三·四·一、三路过滤 `sieve`（施工件 c，2026-09-23）

对应 `05` §1 过滤 `filter(S, q)`「三条流，unsure 是第三条流」与 `16` §1、§7（过滤直接吃题）。依据文本本体未改；普通布尔 `filter` 含义不变。

```
sieve(items, q)                 → 契约值（§三·四·四）：value = 接受流，detail = {question, ignore}，pending = 未决（含 Unsure(budget)）
sieve(items, [q1, q2, …])       → 一个契约值（B82，步 25-0）：元素 = 材料 × 题（材料主序、题次序），detail = {questions, ignore}
sieve(items, form, [fill1, …])  → 同上；元素另带整条填法记录 `fill`，交给 `fill()` 的只有槽键
```

- `items`：材料、状态或任意可成材料的值；**也可以是上一次 sieve 的元素**（带 `item` / `exit` / `trail` 的记录），此时取它的 `item` 当材料、`trail` 接上上一次的出口——产物与输入同形，可再过滤。
- 每个元素（B81 (a)，步 25-0）：输入元素记录的全部字段再补或更新 `{item, pos, trail, exit, cause, q, qi, key}`；`index` 只在输入不是元素记录时赋为输入位置、此后不变；`pos` 是本次调用里的位置；`key` 是该读数的账本键（预算未观察的元素记它本该有的键，不进 `evidence`）；`source` 已撤。
- 未决条目（B81 (c)）就是带 `exit`、`cause` 的元素记录本身；`unobserved` 的条目原因是缺席类（`budget`、`absent`、`latency`，与运行时 B95 的集合相同；库谓词 `unobserved_cause`，步 25-3）。`by_q(o, k)` 取第 k 道题的子契约值。`resume.unobserved` 与 `W-sieve-budget` 数的是未观察的元素（材料 × 题）。
- `cut(读数列表)` 返回出口列表，逐读数各自切（各自的校准键与 J-15）。
- **三流不漏、互斥**：完整输入时 act ∪ ignore ∪ unsure 覆盖全部 index，各占一次；同一元素出现两次就占两席，同状态同题只问一次（账本同键）。流内保持输入顺序。
- **直接吃题，结构化批处理**：把全部元素 × 全部题登记，一次刷新（步 22-0 起入口不再先单独发出此前的登记）；同状态的题由融合 pass 合成一次调用。实测：同一材料 14 道题，逐题经函数判断 14 次调用，`sieve` 1 次（真机 token 4,228 → 536）。
- **预算提前停止**：步 22-0（B93）起预算耗尽是停发不是停程序——刷新把发不起的组标 `budget`、记缺席账（首因 `budget`），这些元素记 `Unsure(budget)` 进未决清单，`resume` 与 `W-sieve-budget` 的说明取自缺席账。`judge` 直接调用、`select` 同样给 `Unsure(budget)`；`gen`/`do` 超预算产出失败值 `budget: …`；只有 `ask` 超预算仍挂起。续跑重发停发的站点，审计重放在同一站点停发。
- **出口与责任**：act / ignore 出口由 sieve 路由，记为已消费（`consumed_by = sieve:act|ignore`）；unsure 出口放在 unsure 流里，责任随返回值转交（J-05）。
- 只收是非题（`test`）；K 选一、打分的分流待后续件。静态检查把 `sieve` 记为 `judge` 效应。

S 库 `lib/materials.jpp` 的 `review_material(opinion, about)`：把评审意见（例如 `gen` 的输出）经 `transform` 包成 `{kind: "review", about, opinion}` 材料，taint 与来源链随原材料继承（`12` §2.11），供判断器按题读（B18）。

## 三·四·二、配对 `pair`、聚合 `tally` / `first_k`、有界迭代 `iterate`（施工件 e、f、g，2026-09-23）

依据 `05` §1 的配对、聚合、递归三个算子。都是纯计算或只经 `sieve` 判断，不含领域算法。

| 构造 | 输入 | 输出 |
|---|---|---|
| `pair(左, 右)` / `pair(左, 右, fn(a, b) -> Bool)` / `pair([[a, b], …])` | 两组元素，或调用者给的候选对；第三个参数是调用者的取舍方法 | 关系记录列表 `{item: {a, b}, trail: [], left, right, at: [i, j], pos}`（`pos` 步 25-0 加，不赋 `index`）。`item` 是交给判断器的那份材料，两端对象段标为 `a` / `b`；`left` / `right` 原样保留调用者给的元素 |
| `tally(sieve 产物)` | 一次 `sieve` 的产物 | `{n, act, ignore, unsure, unobserved, count: [下界, 上界], complete, exists, all, stopped}`。计数区间 = [act, act + unsure + unobserved]；`exists` / `all` 是三值出口，结论取决于未决或未观察元素时为 `unsure`（未观察优先给 `budget`，否则取第一个未决元素的原因），责任随出口交给调用者（J-05） |
| `first_k(sieve 产物, k)` | 同上，按输入顺序（元素的 `pos`，步 25-0） | `{items, exit, blocked_at, k}`。`exit` 为 act = 凑够 k 个且此前没有未决或未观察元素；ignore = 全部观察完、确定不足 k 个；unsure = 被挡住，`blocked_at` 给位置 |
| `iterate(bound, 初值, fn(acc, i), measure)` | `measure` 是 `fn(acc) -> Int` 或 `"tokens"`（渲染后 token 估算，与窗口检查同一估法） | `{value, reason, rounds, measures}`。`reason` ∈ `stop`（step 返回 `stop(v)`）/ `bound` / `noshrink`（每层材料不再严格变少，05 §1 第二条终止线）/ `repeat`（账本键在本循环内重复，J-06） |

- 元素识别：带 `item` 与 `trail` 的记录（`sieve` 与 `pair` 的产物）取 `item` 当材料，`trail` 接上一次的出口；因此过滤与配对的产物可以再过滤、再配对（组合封闭）。`sieve` 的输出元素保留输入元素的全部字段（配对产物的 `left` / `right` / `at` 由此一路保留；原 `source` 字段步 25-0 撤）。程序构造的 `outcome({…})`：`pending` 收带 `exit` 的记录（缺 `cause` 由出口补）或裸出口（包成 `{exit, cause}`）；不收 `spent`，花费按 `evidence` 的账本键由运行时算（A-2）。
- `tally` 与同题多问取均值的 `agg` 无关，后者不变。
- 普通 `loop` 不变；需要终止原因或第二条终止线时用 `iterate`。
- 静态检查：`iterate` 与 `loop` 同受 J-06（bound 必带、正整数）与 E7 约束；`pair` 的第三参数、`iterate` 的 step / measure 按方法位置推断效应。

## 三·四·三、真值通道与题式级线（施工件 a，2026-09-23）

依据 B19（校准真值以模型标注为主，小样本人工核对一致率，分歧与敏感项转人）、B2（题式为主键；**待 Nature 批准，本版题式键只作回退层**）、B13（标注者弃权率高 = 题面外延未定）、J-18（重放一致）。全部为增量：老记录、老账本的格式与哈希不变。

| 构造 | 说明 |
|---|---|
| `jpp calib-import <labels.jsonl> --calib-out <dir> [--calib <dir>] [--alpha 0.1] [--conf-delta 0.1] [--spot-check-min 0.9] [--abstain-warn 0.1]` | 每行：`key`（校准键）或 `form`（题式规格：op、template、可选 scale / evidence / presupposition / request，哈希与 `.jpp` 的 `form` 同算法）、`item`（材料 id，人工与模型标注靠它配对）、`p`（一次实际运行的读数）、`label` ∈ `true` / `false` / `"ambiguous"`、`source` ∈ `human` / `computed`（真值由构造或程序算出）/ `model:<名>`、可选 `spot_check`（人工抽检批次）。逻辑在 `jpp_core::truth::import_labels` |
| 真值选择 | 同一 `item` 有人工或构造的真值时用它，否则用模型的；`ambiguous` 不进线，计入 `truth.abstain_rate`，超过 `--abstain-warn` 报 `W-abstain` |
| 上岗门 | 有只靠模型标注撑起的真值时，要求同键有人工抽检且一致率 ≥ `--spot-check-min`；否则记录停在 `待真值`，`truth.gate` 写「待核：原因」。门槛是参数，不写死在规则里 |
| 认证 | `CalibStore::commission_two_sided_split`（真值通道所用，B24）：带标注样本先按 `(p, 真值)` 排成规范序，再按 `splitmix64(seed ^ 规范序下标)` 最低位分成选线半与认证半（与行序无关）；选线半上按 `cut` 实际判区（`p ≥ hi + δ` 给 Act、`p ≤ lo − δ` 给 Ignore）联合选线，取两区二项上界各 ≤ α 且已决条数最多的一对；认证半上对该对两侧各检验一次。任一半每侧不足零错误所需条数时停在待核（原因以「待核」开头）。证书新增可选字段 `selection {method, seed, n_select, n_certify, candidates}`，有值时进地址。`calib-import --seed`（默认 20260923）。〔步 20g（B86、B85）：`calib-import` 缺省改为 `commission_two_sided_fixed_sequence_graded`（固定序、不拆分；候选由读数按计数生成，从严到宽逐个检验，第一次不过即停；证书 `selection` 多 `rule`、`step`、`delta`、`generated`、`stop_index`，有 `rule` 时进地址）；`--certify split` 走 `commission_two_sided_split_stratified_graded`（分层交替分半，方法 `split-stratified`）；本行所述的种子分半保留给旧证书重跑；同批选线的 `commission_two_sided` 已删除；K 元单侧同形。原 `commission` 保留不删。〕 |
| 查找顺序 | `cut`：题键上岗 → 用题键；否则题上有 `form_hash` 且题式键 `\u{1f}form\u{1f}<哈希>` 上岗 → 用题式线，报 `W-form-line`，出口 `line_source=题式级·…`；否则模式级；再否则冷。题式记录经真值通道导入但未上岗时报 `W-form-pending`（写明待核原因），按冷键处理。停岗仍提前返回，回退够不着它 |
| 账本 | 新字段 `Ledger.calib_used`（键 → `{hash, record}`）：`cut` 实际查到的记录，**本趟命中集合，每趟改写**（B83，步 7c：入口比对后清空，本趟按当前视图重填；续接后只凭账本重放复现续接趟）。`--replay` / `--resume` 时，本次没有另给的键从这里补回（stderr 列出补回的键），**只凭账本重放出口逐字节一致**。老账本无此字段，行为不变；头上的整库 `calib_hash` 因为库是子集必然不同，步 7b（B77）起只凭账本重放不比它，改比 `calib_used_hash`（命中记录集合的哈希），不再报假 `W-header`。〔账本 v3（步 18a）：载体改为 `CalibUsed` 条目，`Ledger.calib_used` 是按键取最后一条的派生视图，不再入口清空；`calib_used_hash` 移出头，改逐键比，见三·四·六〕 |

新字段：`CalibRecord.truth`（`TruthSummary`：来源计数、弃权、抽检、门、批次）、`CalibRecord.lower`（下侧证书）、`Reading.form_hash`。

已知限制：真值通道只导入是非题；`CalibStore::commission` 仍可被宿主代码直接调用，绕过上岗门（CLI 没有这个入口）；单侧 `certify` 在同一批标注上选线并给上界，没有选择校正（两侧同批选线的 `commission_two_sided` 已于步 20g 删除）。

## 三·四·四、组合封闭性契约（施工件 i，B17，2026-09-23）

依据 B17（Nature 确认进语言：「拼出来的东西本身能够当零件」）与施工前定的三条不变量。依据文本 `12` 本体未改。

**一种值，所有构造都返回它**：`sieve`（单道题）/ `pair` / `tally` / `first_k` / `iterate` / `outcome` 的结果都是

```
{kind, value, pending, evidence, resume, spent, detail, purpose}
```

| 字段 | 含义 |
|---|---|
| `kind` | 产生它的构造：sieve / pair / tally / first_k / iterate / outcome |
| `value` | 产出。sieve：接受流（元素列表）；pair：关系记录列表；tally：`{n, act, ignore, unsure, unobserved, count, complete, exists, all}`；first_k：`{items, exit, k}`；iterate：最后的累积值；outcome：调用者给的 |
| `pending` | 未决清单，每项 `{element, exit, cause}`；`exit` 是承担责任的出口。预算停机未观察的元素记为 `Unsure(budget)` 进这里（不另设字段） |
| `evidence` | 账本键（Text）。不存读数、观察或材料副本；凭账本可重建 |
| `resume` | 续接，总是记录：sieve 预算停机 `{reason: "budget", detail, unobserved}`；first_k 被挡 `{reason: "blocked", at, cause}`；iterate `{reason, rounds, measures}`；outcome 给方法时 `{reason: "continue", next}`；没有则为空记录 |
| `spent` | `{calls, usd}`：本构造判断过的账本键所属的模型调用数与费用，**按账本记录算**（`Entry::Judge.call` 调用号），融合的一次调用只计一次；重放读出同一个数 |
| `detail` | 构造特有的已决信息：sieve 为 `{question, ignore}` |
| `purpose` | 可选可读目的，供诊断，不强制 |

三条不变量及其落点：

1. **契约封闭**：`sieve`、`pair` 的输入可以是列表，也可以是契约值（取其列表产出）；`tally`、`first_k` 只收契约值。调用者用 `outcome({value, pending?, evidence?, resume?, purpose?, detail?})` 构造的契约值与内置构造同一类型，可再交给这些构造。
2. **未决随包转移**：契约值作输入时，它的未决清单并进新契约（`tally` 在结论被挡时把它们并入 `exists` / `all` 出口并记为已消费）。检查器：绑定契约值的 `let` 之后再没被提到、或契约值在语句位置被丢掉，报 J-05（`examples/errors/outcome-dropped.jpp`）；运行期原有 J-05 照常。`consume(契约值, "drop")` 显式丢整份未决清单并记账。`outcome` 的 pending 每项必须带出口。
3. **证据只存键**：内置构造写入判断的账本键；`outcome` 的 evidence 只收 Text，给副本报 `E-evidence`。`key_of(出口 | 读数 | 契约值)` 取键（出口在 `cut` 时记下来源账本键）。

读法见 `lib/outcome.jpp`：`accepted`、`ignored`、`undecided`（判过而拿不准的未决）、`unobserved`（没观察到的未决：`budget`、`absent`、`latency`，步 25-3 起）、`stopped`。现有示例一次迁移到新形状（不保留旧字段名的兼容层：旧的 `unsure` / `unobserved` 两条流与 `pending` 会让同一出口出现在两个位置，责任追踪反而含糊）。演示程序 `examples/contract.jpp`：sieve → pair → sieve → outcome → pair 第二轮 / 续接 → tally，七个阶段 `keys` 相同。

**校准进料补充（B19 修正、B24 补充）**：`calib-import` 的上岗门按一致率的单侧置信下界判（`--spot-check-min 0.9`、`--spot-check-conf 0.95`）。点估计不过 → 待核；点估计过、下界不过 → 线照常认证，`truth.gate` 为「临时上岗：…再追加 m 条全一致即转正」，`cut` 用到时报 `W-provisional`。`SpotCheck` 新增可选 `lower`、`conf`；`CalibRecord` 新增可选 `scope`（认证集的标注批次与来源计数；材料风格指纹与 `W-calib-scope` 未做）。

## 三·四·五、账本 v2（工程步 7，格式步，2026-09-24）

依据 `20` §2.3 `jpp-ledger`、§3.7、§九；`21` 步 7；B40、B55、B59、B61。落盘只经 `Ledger::encode` / `Ledger::decode`（`Ledger` 不再派生 serde，没有第二条序列化路径）。

| 项 | 内容 |
|---|---|
| 文件 | JSONL 链式。首行 `{"version":2,"header":…,"calib_used":…}`；之后每条一行 `{"seq":n,"prev":<上一行的哈希>,"entry":…}` |
| 头 | `{budget:{calls,cost}, compared:{model_id, render_version, handler_version, profile_hash, behavior_hash, calib_hash, calib_used_hash, lib_version, bank_version, ir_version, entry_hash}}`（十一字段，`calib_used_hash` 步 7b 加，B77）。比对只在 `HeaderCompared::diff_in`：只凭账本重放不比 `calib_hash`、续接全比（B77）；预算记录不比对（B61）；`W-header` 只列不同的字段。`entry_hash` 从步 14b-0 起有值：`jpp run --input <file.json>` 时为规范化 JSON 的哈希（`HostInput`，`run_with_input`），不带时为 null |
| 条目 | `Judge` 带结构化键 `jkey`（`jpp_ir::key::JudgeKey`，`digest()` 与旧 `judge_key` 相同）、`calib_ref`（题声明的校准键）、`layer`、`merged_by`（一次调用多于一题记 `fuse`）、`parents`/`hop`/`reused_from`（恒空，步 17、19 填）。`Effect` 带 `ekey`、`output_mat`（恒空，步 17/18 填）。`Ask` 带 `ekey`；**已问未答也入账**（`answer: null`），重放照记的以 `Pending` 结束，续跑问到答案另起一条（`Ledger::put_answer`，只增）。`Absent`（取代原 `kind:"absent"` 的效应条目）。`Intent`、`Halt` 已定义、本版不产生 |
| 解码 | 末行半写 → 截断到最后一条完整条目，报 `W-ledger-truncated`（CLI 打到 stderr）；完整行读不成、链断、未知字段 → `E-ledger-corrupt` 指出行号；v1（整份 JSON）→ `E-ledger-archived`，用标签 `ledger-v1-archive` 处的二进制重放 |
| `budget.escalate` | 上限数的是**已答**的 `Ask` 条目（与入账前一致） |

## 三·四·六、账本 v3（工程步 18a，格式步，2026-09-25）

依据 B124（`地基/附注/2026-09-25-待补批量裁定-2.md` §四）、B83 第 6 条、B84、B92；`21` 步 18 拆分注。上表 v2 的规则除下列各项外照旧。v2 的二进制在标签 `ledger-v2-archive`。

| 项 | 内容 |
|---|---|
| 文件 | 首行 `{"version":3,"header":…}`，在第 1 条条目之前定稿（为 18b 逐行落盘）；没有 `calib_used` |
| 头 | `compared` 十字段（去掉 `calib_used_hash`） |
| 命中记录 | 条目 `CalibUsed {key, hash, record}`：某键本趟命中时，账本里该键最后一条的哈希不同（或没有）才追加；`Ledger.calib_used` 是按键取最后一条的派生视图（只读），写入只经 `Ledger::note_calib_used`；该条目不进键索引 |
| 比对 | `Ledger::set_header_checked(头, 场合, 视图)`：十字段按场合比（重放不比 `calib_hash`），另把 `calib_used` 与视图逐键比；报文与 v2 相同（`calib_used_hash 旧 X 新 Y；变化的键：…（共 n 条）`） |
| 效应输出 | `do` 的材料输出：`output` = 内容，`output_mat` = `{addr, origin, taint, sources}`（`sources` 每条 `{key, kind: value\|select, q}`，为空不写）；`derived_from` 不写，读回由值依赖边重算；`taint` 必填、坏值即拒。`gen`、`transform` 仍只写内容 |
| `calib_ref` | 留位 `key`、`kind`、`fill`（为空不写，20a-2 填） |
| 迁移 | v2 → `E-ledger-v2`；`jpp ledger-migrate <v2> <v3>` 改写，CLI 读 v2 时在内存里迁移并提示；`jpp::store::migrations::ledger_v2`（核 v2 链、截断照报；`calib_used` 各键作 `CalibUsed` 接在末尾；`__mat` 拆成 `output` 与 `output_mat`，`derived_from` 丢弃） |

## 三·五、执行模型：惰性登记 + 刷新点 + 分层

`judge` **登记后不发**（`12` §2.2:129）。`Reading` 造出来时答案是空的；到一个**刷新点**，
把所有已登记的判断**按状态哈希分组**，一组一次调用发出——这就是 P5「一次调用 = 一状态多题」
（`12` §10 G2）落地的地方。实测：三个状态各两道题，即时执行 6 次调用，惰性 + 融合 **3 次**。

**记账粒度是题，调用粒度是状态**：账本按题记条目（重放按题命中），`cost.calls` 按调用计。

**分层是天然的，不是另做的一步。** 依赖前一条出口的判断，只可能在前一次刷新**之后**才登记得上
——要拿到出口就得先 `cut`，而 `cut` 本身就是刷新点。所以「**一次刷新 = 一层**」，层内按状态分组。
这是从刷新点的定义直接推出来的性质。

**哪些是刷新点，用判据不用清单。** `12` §2.2:129 列了 `cut` / `fit` / `match`/`if` / 读内容 /
程序结束，但清单形式会漏：每加一个读答案的操作就要记得补一项，而**漏补的失效方式是静默给出
错误结果**——`allocate` 漏了刷新时选出的是 `[0,1,2,3]` 而不是 `[2,4,6,8]`，不报错也不告警。
所以判据是：**凡结果依赖于答案的操作，皆是刷新点**，清单只作例子。现有刷新点：
`cut`、`if`、`content`、`allocate`、`unsure_bound`、程序结束；后两个属清单里「宿主读内容」一类，
不是新增的第七种。（已作附注提议给 `12`，待裁定；core 按判据实现。）

读答案的入口只有一个方法，名字就叫 `Reading::answer_after_flush()`——**判据摆在每个调用点上**，
而不是指望作者记得去查清单。

**编译 pass 的开关（`12` §4「每个一个开关，给消融留门」）**：`Interp.passes: Passes`，
七个字段对应 §4 表里的七个 pass。**开关是消融的唯一载体**——没有它，「关掉融合成本涨多少」
这类账没法再核一次。实测：三个状态各两道题，`fuse` 开 3 次调用、关 6 次，**关掉成本涨 100%**
（§4 表里那一栏写的是「E8 成本 +45%」，那是另一个程序上的数，形状不同不可直接比）。

**pass 是 9 个不是 7 个——依据文本自己不一致，记在这里。** `12` §4 的表写了 7 行，
但同文件 v0.1.1 修订记录 1（第 610 行）写着「§4 **增两个 pass**」：judge 推测提升与
循环向量化——**而那张表从没改过**。同一过期数字在三处独立写着（依据建造顺序、依据对照表、
Python 模块 docstring）。判定为**文档缺陷不是设计分歧**，core 按 9 个算；总控另走附注提请裁定。

9 个里 core 落地两个：`fuse`（同状态同层合成一次调用）、`ledger`（账本键与重放）。
其余七个 `lift` / `fission` / `lower` / `schedule` / `plan` / `speculate` / `vectorize`
**字段留着但 `enabled()` 恒 false**——开关开着也不谎称在工作，否则「开关开着」会被读成
「这个 pass 在工作」。`Passes::landed()` 给出真正落地的名字，`Passes::none()` 是消融的对照臂。

**每个 pass 落地时要报得出「关掉它，调用数/层数变成多少」**——那个差值就是这个 pass 的存在理由，
报不出差值的 pass 不该留。已有的两笔：`fuse` 关掉，三状态各两题的程序从 3 次调用涨到 6 次；
`lift` 关掉，「读一个判一个」写法的三题同状态程序从 1 次调用 / 1 层涨到 3 次 / 3 层。

**`lift` 的范围是「同状态」，这是依据里的限制不是实现的缩水**：`12`:610 修订记录 1 的推测提升
原文写「**同状态**、静态可达、中间无 `do`/`gen`/`ask`/`transform` 且不改写状态名的 `judge` 站点
随首个站点一起发；只推测 `judge`，零成本零副作用故不回滚」。不跨分支同样是依据（`12`:13
「§4 删『跨分支提升』，提升只在直线段内」，:275 的理由是跨分支需要静态图，且会为少一层花任意多的钱）。

**不同状态的判断合并到同一层，core 没做**。技术上做得到，但**层数现在没有消费者**——
`budget.layers` 还没有（等 Codex），`schedule` 也没落地，所以合并只会让 `layers.len()` 变小，
不省任何调用、不影响任何判定。等其中之一有了消费者再做，那时差值才有人消费。

## 四、运行 API

```rust
pub fn run(
    program: &Program,
    ports: Ports<'_>,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error>;

pub enum Error { Check(Report), Runtime(RtError) }   // 都带 Span，都能 render()
```

`run` 先静态检查，有错就不执行。要绕过检查器单独试解释器用 `run_unchecked`（只给 core 自己的对照
测试用）。也可以直接 `Interp::new(ports, ledger, calib, actions, budget).run(program)`——CLI 现在
走的就是这条，预算要自己从 `program.budget` 取。

```rust
pub struct Outcome {
    pub value: Option<Value>,        // 挂起时为 None
    pub pending: Vec<Pending>,       // 程序级挂起：ask 未答（含 escalate 上限、calls 预算）、显式 pending、缺席 escalate
    pub budget: Option<BudgetStop>,  // 步 22-0（B93）：预算耗尽后停发的记账 {exhausted, unsent, first_site}；CLI 报告只在耗尽时出 budget 段
    pub trace: Trace,
    pub cost: Cost,                  // calls / replayed / tokens / usd / asks
    pub returned_unsure: Vec<String>,
}
pub struct Pending { pub cause: String, pub key: String, pub site: Span, pub detail: String }
```

**两种未完成要分开看**：`Outcome.pending` 是程序被挂起了；算法自己的未决候选是程序**返回值**里的
普通数据，程序照常跑完。部分结果能交付，不等于每道题都有答案。

**效应端口**（观察的来源；步 15b、15c 起取代旧 `Client` trait，`20` §2.3）：

```rust
pub trait EffectPort {
    fn instance(&self) -> EffectInstance;                                  // (效应, 模型)，一个端口一个实例
    fn submit(&mut self, calls: Vec<EffectCall>) -> Result<Vec<Ticket>, EffectError>;
    fn poll(&mut self, t: &Ticket) -> Poll<Result<EffectOut, EffectError>>;
}
pub struct Ports<'a>;   // 按效应实例索引的端口表：Ports::new().with(端口).with(端口)
```

运行时只经 `Ports` 发 `judge`、`gen`、`ask` 调用（`do` 走动作登记处，`transform` 是宿主函数）。程序用到
哪个效应，宿主就为它注册一个端口；没注册的效应被调用时报「没有端口服务效应 …」。

- `FixedPorts`：固定观察，由判断、生成、问人三个端口组成，`fp.ports()` 借出端口表。键是
  `obs_key(state, question)` = 状态规范 JSON 的哈希 + 题面 + 档位。**未命中就是错，不猜答案**。
  `observe(&state, &q, answer)` 登记一条；模型 id 是 `"fixed-0"`。
- `NoCallPorts::ports()`：拒绝一切调用，重放验证用，模型 id 同为 `"fixed-0"`，所以同一本账本能直接接上；
  `ReplayPorts::ports(模型)` 用账本头记的模型。
- `FnPort::judge / generate / ask(模型, 闭包)`：由闭包给出结果的端口，测试桩与嵌入宿主的小后端用。
- `JevPorts::new(JevClient)`：真机，POST `/v1/systemone`，密钥只从 `~/.typesafe-key` 读。本包不发调用，
  HTTP 走 `live` 特性。JEV 不生成（`gen` 实例报错），没有人工通道（`ask` 实例「还没答」）。

> ⚠️ Rust 侧生成用 `generate`（edition 2024 把 `gen` 收成保留字）。J++ 里的内置名字仍然是 `"gen"`。

**动作登记**：

```rust
let mut actions = ActionRegistry::new();
actions.register("record_check", 0.0, true, TaintOut::Inherit, |args| Ok(args[0].clone()));
//               名字            成本  可逆  输出 taint     实现
```
没登记的动作被 `do` 触发是 J-11 错。`TaintOut` 是 `Trusted` / `Untrusted` / `Inherit`；按 §2.11，
`trusted` 是**显式标记**，语言保证它可见可追，不设审核方。

**校准记录**：`CalibStore::put(键, hi, lo, n, 状态)`，状态是 `上岗` / `冷` / `停岗` / `待真值`；
`上岗` 必须 `n > 0`。没有记录的键回退成「冷」，`cut` 给 `Unsure(cold)`——这是最容易静默走偏的地方，
接线时 `put` 的返回值要 unwrap，别吞掉。

**`gen` 的费用要报**：`GenResult { outputs, tokens, cost }`，与 `JudgeResult` 同形。
以前 `generate` 只返回 `Vec<Json>`，于是 `budget.cost` 对 gen 整条路**失效**
——一个只 gen 不 judge 的程序花多少钱都不会被拦住。现在费用进 `cost.usd`、进账本条目、
参与预算核（顺序照 `13` §5：后端返回即记事实，再决定下一步）。

**账本键带调用点**：`judge_key(…, site)`，`site` 是 `.jpp` 里的字节偏移。与 Python
`foundation/jv/store.py:26` 同一组成分。缺了它，**同状态同题的两个不同站点会撞键**——
第二个站点从账本里命中第一个站点的答案，那不是漏记，是命中一条本不该命中的记录。

**账本与重放**：`Ledger` 既是输出也是输入。把上一次的账本传回来即重放，命中的键零调用；配
`NoCallPorts` 可以验证「同程序重放零调用」。账本头（预算、model_id、渲染版本、handler 版本）不同
会报 `W-header`，进 `Trace.warnings`，不承诺重放一致（J-18）。反序列化出来的账本要先
`rebuild_index()`。

## 四·二·五、为什么 J-12 的静态面没有共用 J-05 那套机制

建议过：`Fail` 与 `Duty` 在「必须被承接、不得无声消失」这一点上是同一类东西，
能共用就比各写各的可靠。**实测下来没共用，理由是它们的「承接」不是一回事**：

- **`Duty` 是一份义务，必须有去向**。它要被追踪到**每一条执行路径**上——所以 J-05 那套是
  **运行期**的：`Exit.consumed` 标记、帧退出时核、程序结束前核。静态只判得住三种确定情形。
- **`Fail` 是一个值，问题只在「有没有人看过它」**。它不需要去向——把它丢掉完全合法
  （一个失败的 `do` 结果没人用，程序照样对）。它唯一的问题是**被当成成功的结果交出去**。

所以两者要查的东西不同：`Duty` 查「所有路径上是否都被处理」，`Fail` 查「交出去之前有没有被查过」。
硬共用一套，就得把 `Fail` 也做成运行期追踪——**而那会把「丢弃一个 Fail」变成错，那是误伤**。

实际落点也不同：J-05 的核在 `interp.rs`（运行期），J-12 静态面在 `check.rs`（编译期）。
J-12 这条判得住是因为它只看一件事：**这个名字裸着出现在结果里，而且从没被 `is_fail` 查过**。

**共用机制的前提是「要保证的东西相同」，不是「听起来像同一类」。**

## 四·三、`on_truth`：实测 Python 侧那条回路也没闭合，所以不移植

`12`:345 列了 `jv.on_truth("键", fn)`，`12`:307 的骨架里写「选型（预测 + 回填 `on_truth`）」。
计划把它排进 `fit` 那包。**实测之后判定不做，理由是它在 oracle 侧就不成立**：

`runtime.py:1404` 的 `on_truth` 只做两件事——往 `stats` 里记一个键名、把回调塞进
`self._truth_hooks`。**而 `_truth_hooks` 全仓再没有第二处引用**（grep 确认：只有写入那两行）。
也就是说 Python 侧的真值回填**登记了但从不触发**，`CalibRecord.samples` 也没有任何运行期写入口。

所以移植它得到的会是：一个能登记、但没有东西会读它的注册表——**正是「没有生产者也没有消费者的壳」**
那一类。按已立的交付要求（每件落地要说出它拦住了什么或省了什么），它一条也说不出。

**要做实需要的不是这个入口，是三样它依赖而两边都没有的东西**：真值到达的通道（谁、何时、
以什么身份送真值进来）、`CalibRecord` 的运行期写入口与并发/顺序语义、以及重算线之后
**已经发出的出口怎么算**（线变了，昨天的 `act` 今天可能是 `unsure`——账本要不要跟着改？
`12` 没有规定）。第三条尤其要先有裁定，不是实现问题。

记在这里而不是默默跳过：**`on_truth` 未做，且短期不该做**；要做先定第三条。

## 四·二·七、推测执行：省的是层数，不是调用数

`speculate`（`12`:610「judge 推测提升」）：刷新点向前，把 `if` 两侧分支体里**此刻已能求值**的
`judge` 站点一起登记，并入本层。

**为什么在 Jev 上不必回滚**（`00-宪法.md` 第 46 行）：CPU 分支预测必须回滚，因为分支两侧都有
副作用；**我们推测的只有 `judge`——模型只分配概率、不触世界（I1），读数记账可重放（P3）**。
猜错只多花一个调用。这能力来自 Jev 的公理，不是从 CPU 那边抄的。

**实测的账，两个形状分开记——它们给出的是完全不同的工程建议。**

| 形状 | 关推测 | 开推测 |
| --- | --- | --- |
| **异状态**（两侧 judge 各问各的状态） | 2 层 / **2 次调用** | 1 层 / **3 次调用** |
| **共状态**（两侧与条件问同一状态、不同的题） | 2 层 / **2 次调用** | 1 层 / **1 次调用**（一次带 3 道题） |

**共状态时推测是零边际的，甚至倒赚一次调用。** 理由是 P5——**状态收费、题免费**：
被推测的那道题骑在一个本来就要发的状态上，融合把它并进同一次调用，**白搭**。

**异状态时才是「拿一次调用换一层」**：两侧各自需要一个新状态，那是真的多花。
n 个分支体各一个异状态站点，最多多花 n−1 次。

**所以建议不是「调用贵就关掉它」，而是**：
- **共状态**（同一份材料问不同的题）——**开着，它白赚**。
- **异状态** + 调用贵而时延不要紧（批处理、离线）——**关掉**。
- 异状态 + 时延要紧（交互）——开着，用一次调用买一层往返。

**这个区分是总控问出来的**：我先前只测了异状态那一个形状，就把结论写成了「拿钱换时延」——
**那句话对异状态成立，对共状态是错的，而共状态恰恰是这门语言最常见的写法**
（一份材料问一串题）。`12`:610 把它列为提升层数的 pass 没错，但它没说**共状态时连调用也省**。

**账有两栏，只报第一栏是在报一半**（`12` §4 第 8 行：「推错记 `W-spec-unused`；
超预算先丢推测」）。异状态那个形状的完整账：**省 1 层；白花 1 次调用**（关 2 次 → 开 3 次）。

**这个数不能跟 `fuse` 那笔（3 次 → 6 次）比：程序形状不同**——那边是同状态多题，
这边是异状态分支。**我们已经吃过一次亏：一个形状的数被当成了整条 pass 的定性。**

**判定不推的四类**（理由跟着，不写下来下一个人会当成漏了）：
1. **依赖分支体内才产生的名字** —— 名字不在，状态算不出来。最常见的一类。
2. **`do` / `gen` / `ask`**（红队 06 A1）—— 有代价或触世界，**猜错要回滚而我们没有回滚**。
   整棵含它们的子树都不推，不只是那个调用。
3. **禁自指的站点**（J-02）—— 推了也会在真站点被拒，白花。
4. **状态含 `Fail` 的站点** —— 那条路上读数本来就是空的（J-12）。

**搬过去的站点，来源还是原来那份**：状态由 `make_state` 在**当前环境**里算出来，taint 与
`derived_from` 跟着材料走——**推测搬的是站点不是值，值仍在原环境里算**，所以没有新边界。

**`W-spec-unused` 进 trace 不进账本**：推测的读数**进账本**（它是一次真实发生的调用，花了钱，
账本是审计物）；而「这次推测没被用上」是**本次运行的事实**，不是那条读数的属性——同一条读数
下次运行可能就被用上了。账本里那条与真站点写的**一模一样**（同键同答案），重放照常命中。

## 四·二·七·五、`select` 的置换：默认不开，以及那笔账

`12`:151「`Pick` 要求置换众数一致（`profile.position_bias`）」。**置换默认不开**，三条理由连成一个形状：

**一、K=2，理由不是「够便宜」，是那道门本身是二值的。** `Pick` 是一道**门**不是一次测量，
门要的是一个布尔。而且这个 K=2 是**正序与逆序**——**对首位偏置的极大对抗对**：
若存在位置偏置，正逆两序最容易让它现形；两个随机置换反而可能都没碰到那个位置。
**所以这里的 2 比「2」这个数字看起来的强。** （Python 的 `position_bias` 用
`runs_per_set: 15` 不是反例——它测的是**偏置本身有多大**，需要分辨率；我们测的是
**这一次 select 稳不稳**，只需要判定。**K 由这个数拿来干什么定，不由「越多越准」定。**）

**二、`mode_share` 不许裸记：它和 `perms` 成对进账本。** K 是这个测量**身份的一部分**——
K=2 的 1.0 与 K=15 的 1.0 是两个不同的测量。裸记的话改 K 会把不同 K 下的值合进同一格、
第二个覆盖第一个，**而它长得像一次观察**（与 `literal_mode` 缺维、`judge_key` 缺 `site` 同族）。
**这条的证据是那个被写错三次的数**：最终能定下来靠的正是原始数据里的 `perms: 2`
——没有它，「测出来的 1.0」与「写死的 1.0」到今天还是不可判的。

**三、默认不开，因为这笔钱不能替作者花。** `P5` 是「状态收费、**题**免费」，
而置换是**两次不同的调用**，不是同一状态多问一题。**开置换 = select 的调用数 ×2**
（E-CAL 那批 97 条若全走原生路径：**开 194 次 vs 不开 97 次**）。

不开的后果是**有痕迹的失败关闭**：`mode_share == None` → J-15 那一位亮 →
取保守项（不给 `Pick`，出口是 `unsure(untested:permutation)`）+ `W-untested`。
作者看见告警，知道「这一格要更强的出口就得付这笔钱」。**反过来默认开的代价是：
每一道 select 都悄悄花了双倍，而作者不知道自己买了什么。**

**连起来**：不付钱 → 没测 → J-15 亮 → 保守出口 + 告警；付钱 → 测了 → `mode_share` 连同
`perms` 进账本 → `Pick` 可给。**这让「线只能来自校准记录」在 select 这一格也成立：
你不付证据的钱，就拿不到强出口。**

**那个数的限制要跟着它走**：判据在 K=2 上是二值的（`1.0` 或 `0.5`），
它**测得出「不一致」，测不出「多不一致」**。这是**那个数**的限制，不是**那道门**的限制。

**`untested` 不是 `tie`**（J-15 加宽后的措辞：「引用任何被声明为判据、但在本次路径上
没有被测量的量」）。`tie` 的语义是「**测了，不一致**」，路由去向是「逐候选 noul」；
而没测过的那条路（K-noul）**本来就是逐候选 noul，路过去是空转**。两者在**账本**里
也不能是同一个值——账本是审计物，这与「兜底档案的 `hash` 必须是 `None`」同源。

## 四·二·七·六、J-15 落成：「测没测过」是**正交的一位**，不是新的 `cause`

**判据先于结论**（`12` §2.11）：一个新冒出来的区别，该加一个 `cause`，还是加一个维度？
**看它是「这一格特有的」还是「会在很多格上重复出现的」。** 数一数今天已经有几个量正处在
「没测过」的状态——**五个**，同一个性质。所以是维度，不是 `cause`。

按 `cause` 的路子走会得到 `cold` / `no_perm` / `no_ece` / `no_klimit`……而 **`cold` 的存在
正是证据：那条路已经走过一次，然后停了。** 每多一个 `cause`，handler 库就多一条要记得加的
路由，**而漏加路由的失效方式是静默的**——`otherwise` 兜住，没人知道。

**落法**：`Exit` 上加一个与 `kind` 正交的字段 `untested: Option<String>`（载体名只作诊断，
不作路由），`cause` 的本分不动。对 handler 可见的入口是新内置 **`untested(u)`**
（`12` §2.11 硬要求一：那一位**不能只进 trace**）。

| 情形 | `unsure_cause(u)` | `untested(u)` | `exit_kind(e)` |
|---|---|---|---|
| 测了置换、正逆两序不一致 | `tie` | `""` | `unsure(tie)` |
| 这条路上没测过置换 | `untested` | `permutation` | `unsure(untested:permutation)` |
| 线未测（校准记录 `冷`） | `cold` | `calib_line` | `unsure(cold\|untested:calib_line)` |

**两类的分别**：`cold` **骑既有路由**（`cold → 保守线 + 标记` 是 §5 已有的行，动它就是在拆
一条已经接好的线），那一位挂在旁边；置换未测**没有既有路由可骑**，所以给**通用 cause
`untested`**——**一条**路由给五个载体共用，不是每个载体一条。

**这一格 `12` 没写，是 core 的判断，提请总控裁。** `12` §2.11 写的是「§5 那张表不新增行，
每条既有路由各加一句『若该量未测，取保守项并告警』」。对 `cold` 成立，照做了；对置换未测
**找不到可骑的行**：`tie` 被 `12`:151 指派给「测了，不一致」，骑过去就是让 `tie` 兼职（正是
这次要消除的东西）；`band` 说的是 p 落在带内，**而这一格 p 可能远在 `hi` 之上，写进审计物
就是一句假话**。两条都比新增一行更糟，所以新增了一行 `untested → 补测该量 / escalate`。
**若总控判定不该新增行，改法是把 `Unsure("untested")` 换成裁定的那条路由，`Exit.untested`
这一位与 `untested(u)` 不动**——它们与 `cause` 正交，正是为此。

**每条既有路由加的那句「告警」是有实测代价的**：`cold` 那条现在每命中一次就多一条
`W-untested`。理由是**没有告警，「用了保守线」与「线本来就这么宽」在痕迹上分不开**，
与「兜底档案的 `hash` 必须是 `None`」同源。实测：全工作区 188 项绿、0 warning，
既有断言无一被这条新告警打红。

**五个载体，今天只接了两个——另外三个是判定「不做」，不是漏了：**

| 载体 | 状态 | 不做的理由 / 要做需要什么 |
|---|---|---|
| 线未测（`cold`） | **已接**，`untested = "calib_line"` | — |
| 置换未测（`mode_share == None`） | **已接**，`untested = "permutation"` | — |
| 档案字段未测（`choice_same_call_perm_crosstalk`） | **未做** | Rust 侧 `Profile` 只读 `lines` / `delta` / `window` 三组，**这个字段在结构体里根本不存在**。要接先得给 `Profile` 加该字段并定「读不到算未测还是算缺档案」——后者是另一条纪律（`load` 不许悄悄回退），不该顺手一起改 |
| `k_limit` 120–250 档未测 | **未做** | 同上：`k_limit` 只出现在 `behavior_hash` 的字段名单里，没有任何一处读它的值。要接先得让下沉阶段真的按 `k_limit` 分档（`12`:370 的 `select → choice 或 K-noul`），**而那一步今天整个不存在** |
| ECE 未过检 | **未做** | 需要校准记录上带 `ece` 与 `n ≥ 100` 的过检标记；`CalibRecord` 今天只有 `hi/lo/n/status/delta/unsure_rate/set_id` |

**不写下来的代价**：下一个人会把这三条当成漏了，从零补一遍，然后在 `Profile::load` 那里
撞上「读不到该报错还是该算未测」——那正是这三条今天不做的理由。

## 四·二·八、J-08 拦什么、不拦什么

宪法 IFC 那行的纪律只有一句：**不可信材料上的判断不得单独放行不可逆 `do`**（`12`:265 给形式：
守卫表达式中至少一个合取项来自 taint=trusted 的状态；untrusted 项的数量不改变这一要求；或经 `ask`）。

**守卫就是包着那个 `do` 的 `if` 链**，不是新语法。条件求值成 `Bool` 之后 taint 就没了，
所以在**求值条件的那一刻**记下这层的来源。

**判定不拦的四种，理由跟着**（不写下来，下一个人会当成漏了）：

1. **无条件执行的不可逆 `do`** —— 它**没有守卫可查**。作者直接写 `do` 是他自己的决定，
   J-08 管的是「放行的守卫」，不是「该不该有守卫」。要管那个是另一条规则。
2. **可逆的 `do`** —— `12`:265 只说「不可逆」。可逆动作撤得回，不在这条的射程内。
   **不可逆从 `register_action` 的登记里读，J-08 不现推**。
3. **`trusted` 声明本身的真伪** —— `12`:649 Nature 裁定：`taint_out="trusted"` 是作者的
   **显式标记**，语言保证它可见可追，**不设审核方**。一旦开始审它，这条规则就从可判变成不可判。
4. **`||` 的两侧** —— `12`:265 说的是**合取**。`a || b` 成立时不知道是哪一侧成立，
   **不能声称 trusted 那一侧放的行**。所以只按 `&&` 拆合取项。

**静态子面「守卫全不可信」（B108，步 24-0）**：包住不可逆 `do` 的每一层守卫都**确定**来自不可信状态上的判断（入口未声明可信的条目、登记为输出不可信的动作；经函数、容器、闭包、构造的一律按可信计），且路径上没有 `ask` 时，检查期就报。`Session` 执行前那次检查带动作表，报 `J-08`（error），在花调用之前停下；`explain` / CLI `jpp check` 不带动作表，报 `W-guard-untrusted`（warn）。规则在 `jpp-check/src/rules/j08.rs`，是运行期面的子集，运行期面照旧兜底。

**来源跟到「值」，不是名字**。记录的**每个字段各有各的来源**：
`{脏字段: 脏判, 净字段: 净判}` 里，用 `包.脏字段` 当守卫被拦、用 `包.净字段` 通过。

这一条起初做错了，错法值得记：我在**守卫那一层**专门拒绝追析取（`a || b` 成立时不知道
是哪一侧成立），**却在绑定那一层把一次求值里的所有出口按析取折成了一个来源**。
于是守卫里不许写 `脏 || 净`，**但包一层记录再取字段就过**——**同一份谨慎，隔一层被自己拆掉**。
根子是 `12`:265 要的「至少一个**合取项**来自 trusted 状态」里，**合取项是值级的概念，
而实现做成了名字级**。

**来源通道的作用域由环境给**（键里嵌一个源码写不出的分隔符）：随块退出而消失、被重新绑定
而覆盖、在 helper 的帧里查不到外层的。此前是一张按名字索引的旁路表，**没有作用域**，
于是它**同时会漏、也会串**——漏是 `lifecycle.jpp` 被误拦（失败关闭），串是一次不相关的 `ask`
污染任意后续布尔（**失败开放**）。**只修漏那一面，串那一面会留下，而且更严重。**

**追不到来源就当不可信**（兜底往拒绝那边倒）。

**一处方向记反的订正**：这里原先写的是「**已知的假拒绝方向**：来源存进容器再取出时追不到」。
**实测是假放行**——`mat({outer: content(脏)})` 三行就能击穿，`do("发邮件")` 真的执行了。
**记成假拒绝的东西没有人会急着修**；假拒绝只是烦，假放行是宪法第 44 行那条纪律被绕过。
**一个缺口记错方向，比没记还危险。**

已修，而且**没有再打第三个窄补丁**：前两个（`mat(content(脏))` 原样直传、`==` 吃掉不可比）
都是按具体形状堵的，这次又是同一个洞换个包装。**按值精确匹配这条路本身是错的——包装方式
是无穷的，穷举它永远落后一步。** 改成记**拆出来的内容及其所有子结构**，判定变成
「**这个新材料里含不含从 untrusted 材料拆出来的东西**」，容器由递归比对认出来。

**为什么没走「让 `content()` 的产物本身带来源」那条**（另一个选项）：试过了——
让 `content(脏)` 返回一个 untrusted 的**材料**而不是裸值，taint 就跟着值走、不必查表。
但它当场打红了 `examples/lifecycle.jpp`：下游 `content(脏).字段` 这类取字段拿到的是材料
不是裸值，**整条读取路径都要改**。**代价落在每一个用 `content` 的程序上，而收益只在
洗白这一路**——不成比例，所以没选。记在这里，因为它是个真选项，只是这次不划算。

**仍未覆盖的方向**（老实说）：来源穿过 `map`/`fold` 的**回调**时追不到——回调里的值不经过
`content`，也就不进那张表。那是**假放行**不是假拒绝，已知，未修。

## 四·三·五、「在同一把尺子上吗」：五个多读数操作逐个过一遍

判据（总控立的）：**凡是把多个读数放到一起的操作，都要先问「它们在同一把尺子上吗」。**
core 里有五个这样的操作，按判据扫了一遍，结论**不是一刀切**：

| 操作 | 要同尺吗 | 为什么 |
| --- | --- | --- |
| `agg` | **要**（同题） | 它求均值/众数——**跨题求均值是把两把尺子的刻度加起来** |
| `order` | **要**（同题 + 同候选集 + 同刻度） | 排序就是比。「问题一 0.9 高于问题二 0.5」这句话本身不成立 |
| `fit` | **要**（逐项与注册特征相同，J-04） | 它按训练时的特征位喂值，喂错题就是喂错特征 |
| `allocate` | **不要** | 它不比较读数之间的值，它比的是**各自离自己那条线多远**——那个量是无量纲的（每条读数用自己的校准记录算），跨题可比正是它的用途：一批不同的题里挑最不确定的几条去复核 |
| `unsure_bound` | **不要** | 它把各题的 `unsure_rate` **相加**成联合界。每个 uᵢ 取自**各自**的校准记录，跨题求和正是 J-10 的定义 |

**后两个如果按「跨题即拒」处理，会拦掉它们唯一的用途**——`allocate` 的意义就是在一批题里分配
复核名额，`unsure_bound` 的意义就是给一整批不同的题算联合上界。那是假拒绝。

**区别在哪**：`agg`/`order`/`fit` 拿**读数的值**互相比较或合成，值只有在同一把尺子上才有意义；
`allocate`/`unsure_bound` 先把每条读数**各自归一**（离线距离、各自的 unsure 率）再放到一起，
归一之后已经脱离了各自的尺子。**判据要问的是「比较的那个量是否同尺」，不是「读数是否同题」。**

## 四·四、判断向量的两法：它们拦不住什么

`agg` / `order` 已落地（`12`:134「合法操作只有两种…其余运算不存在（J-01）」）。
**但实测下来，「其余运算不存在」这半句在 Rust 侧主要不是靠它们成立的**，这点要写明，
免得把它们的价值说高。

实测拿一批读数去做本不该做的事，结果是：`==` 比大小、`+` 进算术、`len`、`contains`
**都已经被 J-01 拦住**（那几条缝上一包修过）。**没拦住的是 `fold` 与 `map`**——
`fold(读数们, 0, fn(acc, x) { acc + 1 })` 跑得通（数个数，没碰读数的值），
把读数塞进记录返回也跑得通。**这两处不是漏，是它们没有读值**：J-01 禁的是
「读数没有可读的值」，数个数和搬运并不读值。

所以两法的价值**不在「拦住运算」**，在两件具体的事：

- **`order` 拦的是「把抖动当成排名」**。作者能绕开它的真实路径不是 `sort`（读数不可比，
  排不了），是**先 `cut` 再按出口排**——那条路跑得通，而且看不出问题：实测 p=0.90 与 p=0.88
  都过线，手搓出来是 `["act", "act"]`，**两个对象看起来一样好**；`order` 给的是 `[[0, 1]]`，
  明说「这两个在 δ 之内不可分」。**前者丢掉了「不可分」这个信息，后者把它变成结构。**
- **`agg` 拦的是「合并之后就不是读数了」**。`fold` 求平均得到裸 `f64`，进不了 `cut`、
  不带校准键；`agg` 的结果仍是读数，还能 `cut`。

**说不出「拦住了什么」的那部分，就照实说没拦住。**

## 四·五、分诊：错误结果优先于漏记

缺口排序用这条判据，不用「哪个看起来重要」：

> **会让程序算出错误结果的，优先于会让程序少记一笔的。**

理由是失效方式不同。漏记（费用不进预算、事件不进 trace）是**账面不全**——数字偏小，
但每个出口本身仍是对的，事后还能从别处补。错误结果（撞键命中别人的记录、taint 被洗白、
三值被压成两值）是**出口本身就错了**，而且**不报错**，之后所有基于它的判断都建在错的地基上，
没有任何地方会再发现。

今晚用它排过三次：档案加载路径（两内核切出不同出口）先于 `schedule`（少一个优化）；
`judge_key` 缺 `site`（命中本不该命中的记录）先于 `gen` 费用不记（漏记）；
`mat(content(脏))` 洗白（三行源码就走到）先于账本往返洗白（要序列化往返才触发）。

同一族里再按**触发条件**排：不需要特殊条件就能走到的，优先于要凑巧的。

## 五、诊断

```rust
pub fn check(program: &Program) -> Report;

pub struct Report { pub diagnostics: Vec<Diagnostic> }
impl Report {
    pub fn errors(&self) -> Vec<&Diagnostic>;     // 非空即不应运行
    pub fn warnings(&self) -> Vec<&Diagnostic>;
    pub fn is_ok(&self) -> bool;                  // 没有错；warning 不拦
    pub fn find(&self, rule: &str) -> Option<&Diagnostic>;
    pub fn render(&self) -> String;
}

pub struct Diagnostic { pub rule: String, pub severity: Severity, pub message: String, pub span: Span }
pub enum Severity { Error, Warning }
```

诊断按 `span.start` 排序。报文格式是「一句话说错在哪。修法：…」，前端拿 `span` 渲染到 `.jpp` 的
文件/行/列。`Diagnostic` 与 `Report` 都可 serde 序列化。

**静态检查的错**

| 规则 | 判什么 |
| --- | --- |
| `J-06` | `loop` 缺 bound，或 bound 是非正整数字面量 |
| `J-07a` | 降级报：程序缺 `budget`，或 `budget` 缺 `calls` / `cost`（步 12d 起缺预算不再到检查器） |
| `E-form-as-value` | 降级报：效应名、语言形式名或内核构造名出现在调用位置以外（`20·B56`） |
| `E-kind-conflict` | 题式槽声明与可见结构矛盾（B76，步 12e-1）：`over_kind` 声明在非 `select` 题式上，或与判断站点上可见的 `over` 形状不符（`labels`/`actions` 对计算材料，`candidates` 对字面文本，`questions` 对非题值）。题类由 `jpp_ir::question_kind` 推出，作者不可写；`over` 形状看不见时不报 |
| `J-07` | 程序里有 `ask` / `escalate` 而 `budget.escalate` 是 0 |
| `E7` | `map` / `filter`（`for…yield`）的体内含 `loop` / `stop` |
| `J-01` | 读数进状态槽、做算术、做比较、取字段、当 `if` 条件、当 `handle` 的第一个参数 |
| `J-03` | `cut` / `test` / `select` / `measure` 的 calib 位是数字字面量（线不可字面） |
| `J-05` | 出口绑定后再没被提到；`handle` 的字面臂表缺 `unsure` 与 `otherwise`；函数直接返回出口而返回类型没提 `Exit` |
| `J-13` | 循环体内 `do` / `gen` 用常量序号 |
| `J-14` | `state` 的 `on` 槽字面量超过两个对象 |
| `E-name` | 未定义的名字；语句位置引用了后面才定义的绑定 |
| `E-arity` | 具名方法的参数个数不对 |
| `E-type` | 实参字面量与参数标注的基本类型不符 |
| `E-effect` | `!{…}` 标注少了函数体里实际会发生的效应 |
| `E-effect-name` | `!{…}` 或类型位上的效应行里有认不得的名字（只有 `judge` / `gen` / `do` / `ask`）。名字不认识时那条标注不再参与差集核对——否则作者会收到一条指向错误方向的「少了 judge」 |

**提示**（不拦程序）：`W-shadow` 盖住内置名、`W-bound` loop 的 bound 不是字面量、`W-seq-const`
循环里的常量序号但键还不碰撞、`W-effect` 标了用不上的效应。

诊断编号按 `12` 的 J 表走：预算相关一律 `J-07`，有界循环相关一律 `J-06`；`11` §诊断的 `E10` / `E12`
作为历史编号不再发出。`E7` 暂时还留在 `11` 的编号上（`12` 的 J 表里没有对应条目），见 §七。

**Int 的边界**（`13` §6）：算术溢出、除零、取模零、最小整数取负 / 取绝对值，一律是**指向 `.jpp`
源码的运行错误**，不是 Rust panic，也不是 release 下悄悄回绕——debug 与 release 两种构建同一规则。
Int 是有符号 64 位；要大整数另行扩展，不在本轮顺便改数值系统。

**运行期的错**（`RtError`，也带 `Span` 与规则号）：J-02 禁自指、J-05 的 `consumed` 返回前核、
**J-05 的 unsure 臂销账核**（臂体没把责任交出去、臂收不下责任、`otherwise` 想兜 Unsure、责任被当材料）、
J-06 键重复即停与调用深度超限、J-11 动作未登记、类型不符、固定观察未命中。运行期的提示进
`Trace.warnings`：`W-header`、`W-bound`、`W-noprogress`、`W-drop-vs-escalate`、`W-drop-then-return`（步 21）、`returned_unsure`。

**运行期编号**（步 9a）：不属于某条依据规则的运行期错误一律带 `E-rt-<名>`，`RtError.rule` 不再为空。
编号与类型说明的唯一来源是 `crates/jpp-cli/src/diag_json.rs` 的 `RT_CODES`（单元测试核对 `interp/` 用到的编号都在表里），
CI 脚本 `scripts/grep_rt_codes.py` 计不带编号的站点（基线 0）。

| 编号 | 类型说明 |
| --- | --- |
| `E-rt-arity` | 内置或构造收到的参数个数不对 |
| `E-rt-arg` | 实参的类型或形状不对；报文给出正确写法（如 `slice(list, a, b)`） |
| `E-rt-type` | 运算、条件或谓词的值类型不对（二元 / 一元运算、`if` 条件、`filter` 谓词、`len`） |
| `E-rt-name` | 未定义的名字、未知内置、调用了不可调用的值 |
| `E-rt-field` | 记录、材料、题、题式、出口上没有这个字段 |
| `E-rt-index` | 下标越界或值不可索引 |
| `E-rt-int` | Int 溢出、除以零、取模零（`13` §6） |
| `E-rt-question` | 题或题式构造不合法（题型、档位、`evidence` 槽名、`request`、`presupposition`、`fill` 的槽） |
| `E-rt-client` | 外部组件报错（判断器客户端、`gen`、`ask`） |
| `E-rt-answer` | 判断器答案的形状或条数与题不符 |
| `E-rt-absent` | 判断器缺席且缺席策略为 `fail` |

**机读出口**（步 9a）：`jpp check <f> --json` 在 stdout 出一个文档 `{file, ok, errors, warnings, diagnostics}`；
`jpp run … --json` 报告不变，诊断（静态检查、运行期错误、`trace.warnings` 里带编号的告警）以 JSON Lines 写到 stderr，
每行以 `{` 开头。每条诊断 `{code, level, span: {file, line, col, start, end}, message, fix, applicability, count}`，
运行期编号另带 `explain`；`applicability` 为 `manual`、`wiring`（修法标了【需接线人】）或 `null`。
编号、位置、报文三者相同的诊断折叠为一条，文本输出在报文后加「（同码同址 ×N）」；报告与 `trace.warnings` 不折叠。

检查器的口径是**宁可漏报也不误报**：静态判不准的一律交给运行期，`Type` 里 core 认不得的名字不参与
`E-type`。

**效应推断有两条纪律，它们是 `E-effect` 能在高阶处判出东西的原因。**

*创建方法不等于执行方法。* 只有落在已知高阶位上的方法体才算会发生——`map` / `filter` 的第 2 位、
`fold` / `loop` 的第 3 位、`transform` 的第 1 位、`handle` 的臂，以及当场造当场调。被创建、被返回、
被存进记录的 lambda 一律不算进外层：`ε` 是调用时的潜在效应，创建的效应是 ∅。少了这一条，
`packet` 里那个 `fn(strategy) { … next(current, strategy) … }` 会把 `advance` 的效应算到每个调用者头上。

*解析不了的被调者只让推断变成下界。* 「标注少了 X」只要 X 确实看得见就报——这个方向上漏报是安全的。
只有反方向的「标了却看不到」（`W-effect`）需要完整信息，所以函数体里一旦有解析不了的被调者，
那条提示就不发。

在这两条之上，参数的效应行按**调用点实例化**：扫全程序的调用点，把实参的效应行灌给形参，取并集。
`solve(mat({target: 731}), step)` 于是把 `solve` 的 `method` 参数实例化成 `{judge}`，
`fn solve(…) !{}` 这种错标注在高阶处也拦得住。类型标注（`Type::Method` 的效应行）是作者给的上界，
压过调用点实例化。

## 六、core AST 的外部表示

`budget {calls: 10, cost: 0, depth: 64}; fn twice(x: Int) -> Int !{} { x * 2 } twice(21)` 完整 JSON：

```json
{
  "budget": {"calls": 10, "cost": 0.0, "depth": 64, "escalate": null},
  "body": {
    "statements": [
      {"Function": {
        "name": "twice",
        "function": {
          "parameters": [{"name": "x", "annotation": {"Named": "Int"}, "span": {"start": 49, "end": 55}}],
          "result_type": {"Named": "Int"},
          "effects": [],
          "body": {
            "statements": [],
            "result": {"kind": {"Binary": {
              "op": "*",
              "left":  {"kind": {"Name": "x"},    "span": {"start": 70, "end": 71}},
              "right": {"kind": {"Integer": 2},   "span": {"start": 74, "end": 75}}
            }}, "span": {"start": 70, "end": 75}},
            "span": {"start": 68, "end": 77}
          }
        },
        "span": {"start": 40, "end": 77}
      }}
    ],
    "result": {"kind": {"Call": {
      "function": {"kind": {"Name": "twice"}, "span": {"start": 78, "end": 83}},
      "arguments": [{"kind": {"Integer": 21}, "span": {"start": 84, "end": 86}}]
    }}, "span": {"start": 78, "end": 87}},
    "span": {"start": 0, "end": 87}
  },
  "span": {"start": 0, "end": 87}
}
```

两个必保留程序里的关键节点（省略 `span`，实际都带）：

```jsonc
// cut(judge(state(m), q))
{"kind":{"Call":{"function":{"kind":{"Name":"cut"}},"arguments":[
  {"kind":{"Call":{"function":{"kind":{"Name":"judge"}},"arguments":[
    {"kind":{"Call":{"function":{"kind":{"Name":"state"}},"arguments":[{"kind":{"Name":"m"}}]}},
    {"kind":{"Name":"q"}}]}}}]}}

// handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(cause) { cause }})
{"kind":{"Call":{"function":{"kind":{"Name":"handle"}},"arguments":[
  {"kind":{"Name":"e"}},
  {"kind":{"Record":[
    ["act",    {"kind":{"Function":{"parameters":[],"result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Integer":1}}}}}}],
    ["ignore", {"kind":{"Function":{"parameters":[],"result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Integer":0}}}}}}],
    ["unsure", {"kind":{"Function":{"parameters":[{"name":"cause","annotation":null}],
                                    "result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Name":"cause"}}}}}}]
  ]}}]}}

// loop(10, s, fn(acc, i) { stop(acc) })
{"kind":{"Call":{"function":{"kind":{"Name":"loop"}},"arguments":[
  {"kind":{"Integer":10}}, {"kind":{"Name":"s"}},
  {"kind":{"Function":{"parameters":[{"name":"acc","annotation":null},{"name":"i","annotation":null}],
   "result_type":null,"effects":null,
   "body":{"statements":[],"result":{"kind":{"Call":{"function":{"kind":{"Name":"stop"}},
                                               "arguments":[{"kind":{"Name":"acc"}}]}}}}}}}]}}

// do("record_check", [r], i)
{"kind":{"Call":{"function":{"kind":{"Name":"do"}},"arguments":[
  {"kind":{"Text":"record_check"}}, {"kind":{"List":[{"kind":{"Name":"r"}}]}}, {"kind":{"Name":"i"}}]}}

// transform(fn(old) { with(content(old), "k", 1) }, m)
{"kind":{"Call":{"function":{"kind":{"Name":"transform"}},"arguments":[
  {"kind":{"Function":{"parameters":[{"name":"old","annotation":null}],"result_type":null,"effects":null,
   "body":{"statements":[],"result":{"kind":{"Call":{"function":{"kind":{"Name":"with"}},"arguments":[
     {"kind":{"Call":{"function":{"kind":{"Name":"content"}},"arguments":[{"kind":{"Name":"old"}}]}},
     {"kind":{"Text":"k"}}, {"kind":{"Integer":1}}]}}}}}},
  {"kind":{"Name":"m"}}]}}

// measure("多大把握", ["low", "high"], "conf")
{"kind":{"Call":{"function":{"kind":{"Name":"measure"}},"arguments":[
  {"kind":{"Text":"多大把握"}},
  {"kind":{"List":[{"kind":{"Text":"low"}},{"kind":{"Text":"high"}}]}},
  {"kind":{"Text":"conf"}}]}}

// if xs[0].cost <= 2 { [1, 2] } else { {a: 1} }
{"kind":{"If":{
  "condition":{"kind":{"Binary":{"op":"<=",
    "left":{"kind":{"Field":{"value":{"kind":{"Index":{"value":{"kind":{"Name":"xs"}},
                                              "index":{"kind":{"Integer":0}}}}},"field":"cost"}}},
    "right":{"kind":{"Integer":2}}}}},
  "yes":{"statements":[],"result":{"kind":{"List":[{"kind":{"Integer":1}},{"kind":{"Integer":2}}]}}},
  "no": {"statements":[],"result":{"kind":{"Record":[["a",{"kind":{"Integer":1}}]]}}}}}}
```

要看完整的一份，把 lower 的结果 `serde_json::to_string_pretty(&program)` 打出来即可；两个程序的
手工构造版本在 `crates/jpp-core/tests/adaptive.rs` 与 `tests/partial.rs`，文件头的注释里有对应的
J++ 源码写法。

## 七、待定项

以下几条 core 按当前团队约定落地了，但与依据文本或简报有出入，需要 Nature / 总控裁定。core 这边改
一个常量就能换口径，不会返工。

**2026-09-21 夜逐条复核过一遍**（这一份是 Codex 接线的依据，**过期条目的代价比内核里的高**）。
第 7、10、11 三条当时记的是「没做」，今夜都做了，已改写成落地说明；第 8 条见下，**已于 05:10 订正——
那条写于 03:29，而 J-08 落地在 03:45，它过期了 16 分钟就开始误导人**。**另外两件不在原清单里、今夜判定「不做」的，也记在这里**，
免得下一个人以为是漏了：

- **`on_truth`（真值回填）不做**，理由在 oracle 侧：Python 的 `_truth_hooks` 写了从不读，那条回路
  本身没闭合；移植只会得到一个没有生产者也没有消费者的注册表。缺的三样见 §四·三。
- **`select` 的「先验策略通过」那半不做**：`12`:317 那一行自己写着「**策略参数，不是语义**」
  ——**依据自己划了界，内核不该越过去**。策略参数属于库或调用方，放进内核就是把一个可替换的
  选择固化成语言的一部分。`Pick` 的另一半（置换众数一致）已做。
- **跨程序缓存键不做**（`12`:211）：它不是「会算错」，是少一个优化；而且方向有风险——
  缓存键是**跨程序**复用，那一行说它「目的无关的中间状态」，做错了会把不该复用的复用过来，
  **那才是会算错**。收益是省钱、风险是引入错误结果，方向不对。

1. **诊断编号已按 `12` 统一（2026-09-21 总控裁定）。** 预算缺失与「`escalate` 出现而预算为 0」都发
   `J-07`，有界循环的 bound 发 `J-06`；`11-语言规范-v1.md` §诊断的 `E10` / `E12` 作为历史编号不再
   发出。这条已经定了，留在这里只是记明改动，CLI 里还写着 `E12` 的地方要跟着改。

2. **「静态可估的最省计划超预算」没做**（`11` 原来的 E12 本义）。它要 J-07 的符号成本签名，而且与
   账本重放相互作用——重放命中的调用不花钱，静态的直线段调用计数会变成假错。列为后续，不假称已有。

3. **E7 的范围。** `11-语言规范-v1.md` §4 写的是「`for … yield` 体内不得赋值（E7）」，并**明确允许**
   体内含 `do`。core 没有赋值语法，所以 E7 落成「`map` / `filter` 体内禁 `loop` / `stop`」。简报里
   「体内禁副作用」比依据严，core 没有按简报收紧（`examples/partial.jpp` 的 `advance` 正是在 `map`
   体内做 `judge` 与 `transform`）。要不要收紧，请裁定。

4. **J-05 在源码语言下的构造。** `12` §J-05 的 v0.1.1 修订说「Python 里由返回注解构造」，并注明
   「Rust/OCaml 类宿主可静态得到同一纪律」。core 现在是两半：静态判三种确定情形（绑定后从未被提到、
   字面臂表缺 `unsure`、函数结果就是出口名而返回类型没提 `Exit`），其余仍由运行期的 `consumed` 标记
   在返回前核。要不要把「静态得到同一纪律」写成更强的要求，请裁定。

5. **core 本地诊断码。** `E-name` / `E-arity` / `E-type` / `E-effect` / `E-effect-name` / `W-shadow` /
   `W-bound` / `W-seq-const` / `W-effect` / `W-no-cache` / `W-untyped-transfer` 都不在 11 的 E 表与 12 的 J 表里。它们带命名空间前缀，不占用共享编号。
   要不要收进依据文本，请裁定。效应标注一致性尤其需要一个正式编号：J-07 是预算/成本签名，不是标注。

   **效应扫描不是效应推断。** `check.rs` 走 AST 收集调用点，遇到解析不了的被调者退成 ⊤ 并跳过标注
   校验。创建或传递一个方法不等于执行它，真正执行由被调者的高阶签名决定。所以这份扫描结果只能当
   **提示**，不能当融合证明；融合器要的是另一件东西（判断站点、状态/题的输入依赖、结果需求点、分支、
   顺序、循环与屏障的结构摘要），core 目前没有产出它。

   **`E-effect` 在高阶处判得动了，而且是多态的。** 效应行是 `Row { concrete, vars, opaque }`：
   `concrete` 是确定会发生的效应（下界），`vars` 是**效应变量**——被调者是本函数的方法参数时留下的
   `(函数 id, 参数下标)`，`opaque` 是连名字都拿不到的被调者。用户**不写** ε；显式 `!{…}` 是要求检查
   的上界，缺省表示推断。rank-1 + 受限泛化，不追求任意阶完全推断（Codex 答 (b)：
   `map : ∀ A B ε. (Fnω(A ⊸ε B) ⊗ List<A>) ⊸ε List<B>`）。

   **两件事分开算，这是多态与单态并集的分水岭**：效应行**按调用点实例化**往上传（`apply(m, plain)`
   这个调用点不背 `apply(m, peek)` 那个调用点的 judge）；而核 `!{…}` 时取**所有调用点的并集**
   ——标注是上界，必须盖住这个函数的所有用法——诊断落在**定义处**的 Span。混作一谈就会出现
   「一处传纯方法、一处传带 judge 的方法，纯的那处被误报少标 judge」。

   四条纪律：创建方法 ≠ 执行方法（只有落在已知高阶位上的方法体才算会发生：`map`/`filter` 第 2 位、
   `fold`/`loop` 第 3 位、`transform` 第 1 位、`handle` 的臂、当场造当场调）；解析不了的被调者只让
   推断变成下界、不压住「标注少了 X」；效应变量按调用点实例化；标注核验取并集。

   **多态只在高阶函数自己不标注时生效——这是 `13` §2 的规定，不是实现的将就。** `13` §2 原文：
   「显式效应集合是调用者可依赖的上界，**不能因为某次实参恰为纯而悄悄缩小作者声明**」。
   所以作者一旦写了 `fn apply(m, f) -> Record !{judge} { f(m) }`，
   `fn pure_user(m) !{} { apply(m, plain) }` 仍会被报「少了 judge」，这是对的。
   实践上的推论：**想要效应多态的高阶助手就别写标注、让它推断**；写了标注就是给所有调用者的承诺。
   `13` §2 另写明「效应变量的显式语法留待实际需求，**不作为本版前置**」，所以 `!{ε}` 这类写法本版
   不做——真写了会得到 `E-effect-name`，报文里会说清楚缺省标注就是推断。

   现在核得住而以前核不住的路径（都有测试钉着，`tests/effect_rows.rs` 里那四条在上一版基线上是红的）：
   同一个高阶函数两处用法互不污染（以前是**误报**）；方法经**返回值**传递（`maker()(m)`）；
   方法经**函数结果记录的字段**传递（`boxed().go(m)`）；**递归**把方法参数自己传回去
   （以前递归那次认不出的调用点会把整个形参的行毒成未知，把顶层那次真实参的实例化一起冲掉）。
   加上原本就核得住的 `solve` / `advance` / `probe` / `search` / `supplement` / `grade` / `validate`。

   **仍然退成未知的情形**（实测五类，全是漏报方向，不会误报）：具名方法经中间绑定传递
   （`let g = peek; apply(m, g)`）；`let` 绑定的记录再取字段调用（`let r = boxed(); r.go(m)`）；
   记录字面量先绑名字再取字段调用（`let r = {go: peek}; r.go(m)`）；方法存进列表再按下标取出调用
   （`xs[0](m)`）；形参上的记录字段（`box_.go(m)`）。共同的缺口是一样的：静态解析只跟
   **直接的函数结果**（`f()(x)`、`f().字段(x)`）与**名字**走，不把方法值沿 `let` 绑定与容器传播。
   代价是这些地方的 `W-effect`（标了却看不到）提示也不发。`examples/partial.jpp` 的
   `first.more(supplement)` 属于第二类；就算把它解析出来，`more` 那个 lambda 调的是它自己的参数
   `strategy`，行本身仍是 `opaque`，所以那条路上的 ε 补不齐**不只是**解析的问题。

   误报方向上修过一处：形参被块内同名的 `let` / `fn` 盖住时（`fn f(cb) !{} { let cb = fn() {…}; cb() }`
   配 `f(peek)`），调用点实例化出来的效应行会顺着名字被算到那个其实是纯的函数头上，发出一条挡程序的
   假 `E-effect`。现在 `row_of_block` 进块先摘掉本块重新绑定的名字，名字落回已知函数表；
   `tests/effect_rows.rs` 的「形参被同名局部绑定盖住时不算它的效应」钉着，同一条测试里配了没遮蔽时
   仍被拦下的对照。代价写明：遮蔽物是**具名方法的别名**时（`let f = peek`，不是函数字面量，
   `push_function` 没登记它）整条退成未知——并进上面第一类，仍是只漏报不误报。

   **效应行现在也能写在类型位上**（前端已落地，`crates/jpp-frontend/tests/method_types.rs` 钉着）：
   `Fn(Record) -!{judge}-> Record` lower 成 `Type::Method`，`Fn1(…)` 是捕获了未决责任的 `Fn¹`；
   不写效应行的 `Fn(A) -> B` 仍 lower 成 `Type::Function`，按「效应行未知」处理，旧源码不受影响。
   标了之后契约跟着**类型**走，有两条推断给不了的能力：一是**不需要调用点**——
   `fn apply(m, f: Fn(Record) -!{judge}-> Record) !{}` 当场就报；二是参数类型上的效应行
   是**对实参的契约**——`f: Fn(Record) -!{}-> Record` 却传了个会 judge 的方法，报在**实参**的
   Span 上（`E-effect`，见 `tests/effect_rows.rs` 的「实参必须在参数类型的效应行之内」）。

6. **`Client::gen` 改名 `generate`。** edition 2024 的保留字问题，J++ 内置名 `"gen"` 不变。

6a. **现行依据是 `12` + `13` 两份。** `13-Rust实践反馈设计修订-v0.2.md` 声明「本文只替换下述冲突点，
   其余沿用 `12-IR与类契约-v0.1.md`」。六条里 core 侧的落地状态（都有验收测试，
   `tests/v13_rules.rs`，测试名对应节号）：§1 造方法不执行方法体 ✓（本来就成立，补了运行期实测）；
   §2 效应在调用处实例化 ✓（第四包做的；`13` 明确「显式效应集合是上界，不能因某次实参恰为纯而
   悄悄缩小作者声明」，与现行实现一致，已钉成测试）；§3 未决按实际去向传递 ——**半落地**，见第 9 条；
   §4 方法身份包括实际捕获状态 ✓；§5 调用前预算与调用后事实记账分开 ✓；§6 整数行为不随构建模式改变 ✓。

   §4 的做法：可复用结果的身份 = 代码哈希 + **实际捕获状态的指纹**（函数体引用到的名字在创建环境里
   的值，嵌套方法按其自身身份递归、限深 3）。指纹取不到时（捕获里有读数/出口/未决责任/状态/题，
   或嵌套太深）**禁用这一项的跨运行缓存**并发 `W-no-cache` 提示，照常执行——依 `13` §4 原文
   「不返回另一方法的结果」，宁可不缓存。

6b. **J-10 只落地了一半：界算得出，「超 `budget.unsure` 即报」没做。** `12` §5 J-10 原文是
   「…无标注集时用联合界 Σuᵢ 作上界；独立假设估计 1−(1−u)^k 只作参考值。**超 `budget.unsure` 即报**」。
   前半截（`unsure_bound`）已落地并与 Python 对照通过；后半截缺一个 `Budget.unsure` 字段——
   Python 的 `ir.Budget` 有它（`unsure: float | None`），Rust 的 `ast::Budget` 没有。加字段会
   同时改到前端的 `lower.rs`（它按 `c::Budget { calls, cost, depth, escalate }` 构造），
   属于 Codex 归口，已写进 `COORDINATION.md` 请他们加；字段落地后 core 这边补一条运行期检查即可。
   在那之前 `unsure_bound` 的结果是**数据**，源码可以自己拿它做分支，但没有自动的判据。

7. **`cut` 的 taint 继承——已做（2026-09-21）。** `Reading` 加了 `state_taint`，`cut` 读它
   （`12`:150「出口 taint 继承状态 taint」、§2.11「cut 继承」）。此前是 `let taint = Trusted;`，
   **不是没实现，是 `State::new` 算好了又被丢掉**。有测试钉着，另配反面测试（可信材料的出口仍可信）。

8. **〔2026-09-21 05:10 订正。原文写「J-02 / J-08 / J-15 / J-17 仍没有」，其中两条已经不对。
   订正的理由不是为了准确，是因为这张表是接线时会先翻到的那一处**——独立校准顾问指出：
   一份文件里两个互相矛盾的说法，**实际生效的永远是读者先翻到的那一个**，而索引表总是先翻到。
   同一份文件的 §四·二·八 把 J-08 写得准确无误，但没有人会两处都读。
   照原样留着的具体代价：有人为了定 scope 查这张表，读到「J-08 还没做」，于是**从零实现一遍，
   盖掉 `guard.rs` 现有的十一条**——包括「来源要穿过 helper 函数」那条防误伤回归，
   **而那条回归正是粒度问题的另一端；冲掉它，粒度改完会立刻误伤，然后有人再放宽一次，绕回原地。**〕

   **四条各自的真实状态（逐条实测，2026-09-21 05:10）：**
   - **J-08：已落地，十一条测试全绿**（`tests/guard.rs`）。首版 `ac7dd85`，随后两次返工：
     来源**作用域**（连同值绑进环境，随块退出而消失）与来源**粒度**（`记录的每个字段各有各的来源`）。
   - **J-02：有代码（`interp.rs:757`），并已在 `tests/invariants.rs` 有测试**——它曾是 J 表里
     唯一「有代码、零断言」的一条，测试是修 `derived_from` 恒清零那一包顺带补的。
   - **J-15：〔2026-09-21 08:4x 订正：已落地两个载体，见 §四·二·七·六。〕** 原文写「零实现」，
     那是 05:10 的实测。此后按加宽后的措辞落成**正交的一位**（`Exit.untested` + 内置 `untested(u)`），
     接了**线未测**（`cold`）与**置换未测**（`mode_share == None`）两个载体；另外三个载体
     （档案字段、`k_limit` 档、ECE）**判定不做**，逐条理由在 §四·二·七·六 的表里。
     **`tie` 不再兼职**：`tie` 只留给「测了，不一致」。
   - **J-17：零实现。** 全树零命中，包括本文件在内没有任何一处描述它的落点。

   J-09 `insufficient` 是 `cut` 判序第一步（`Q.evidence` + `Reading.missing_evidence`）；
   J-04 在 `fit` 的输入指纹与 `order` 的同尺检查两处；J-16 的四条约束逐条落地
   （未注册、指纹逐项相同、训练集 ≠ 保形集、`n ≥ max(50, 20×特征数)`）。
   J-06、J-07、J-12、J-18 在运行期有。**J-17 首包没做，不假称已等价于 Python 检查器。**
   **〔2026-09-21 订正：J-15 已落地两个载体（线未测、置换未测），另外三个判定不做，见 §四·二·七·六。〕**

9. **未决责任的两个案例（`13` §3，总控 2026-09-21 裁定的粒度）。** `unsure` 臂把责任包进结果
   是**转交**不是了结——不在臂里销账，交给函数返回检查与程序结束前检查去核。那两处把两件事分开：

   - **责任没出现在返回值里** = 最后一份承接信息被丢了（取字段、过滤、切片扔掉了它）→ **报 J-05**。
     这是 `13` §3 验收第二句要堵的洞。
   - **责任如实出现在返回值里、只是返回类型没提 `Exit`** → **发 `W-untyped-transfer` 警告**，
     报文直接给修法（`-> Record<Exit>`），责任继续往上挂。它是**标注缺失，不是责任丢失**。

   步 21（B115，A-12）起只对具名函数报；函数字面量没有调用者从签名读它，帧弹出时照旧把未决交给外层帧、不报。

   **案例 2 目前是 warning，样例的返回类型标注补齐后升为 error**——写在这里免得它变成永久宽松。
   已请 Codex 给 `examples/partial.jpp` 的 `observe`（另核 `advance` / `validate`）加含 `Exit` 的
   返回类型；`12` §J-05 原文「被 `match`、handler、**返回类型消费时置真**」本来就把「返回类型消费」
   列为合法去向，而它成立的前提就是类型真的提到 `Exit`，所以这不是放宽规则，是分阶段收紧。

   仍然追不到的：`stop` / Fail 短路 / 预算退出这几条路径上的责任（Codex 陷阱 4），
   以及 `Pending` 没有承接尚存责任（第 12 条）。

9b. **`Fn¹`（`MethodType.captures_responsibility`）现在只拦一处。** 它只在「把捕获了未决责任的方法
   交给 `map` / `filter` / `fold` / `loop` 的方法位」时报 J-05；**被丢弃、被存进记录、被复制都不拦**。
   照 `13` §3 的原话记在这里：「捕获未决的方法能否复制/重复调用，必须以责任转移规则明确处理，
   **不能因一个布尔标记就声称完整线性系统已经建立**」；同节也写了「不要求所有值都使用线性类型」。
   所以这是**已知的部分覆盖**，不是完整线性系统，也不假称是。

   **运行期面（步 21，B52）。** `Closure.captures` 记创建时捕获的未决责任；创建责任的帧返回时，
   不直接出现在返回值、只经一个闭包可达的责任，那个闭包记为 `Fn¹`（`Closure.linear`）。`Fn¹`
   第二次调用报 J-05；没调用也没交出（丢弃）由返回前可达性核报 J-05 并点名该方法；调用过的 `Fn¹`
   不再算那几条责任的路径。多路径可达（`{pending, resume}`，B17）不触发。复制按别名处理（调用计数按身份）；
   调用者帧的责任不判（判不了唯一性），见 `地基/过程记录/工程-步21.md` §二。

10. **`collect_exit_ids` 的函数捕获环境——已做（2026-09-21）。** 补了 `Value::Fn` 那一臂，
    按方法体**实际引用到**的名字找责任（不把共享环境链里所有可达名字都算成捕获，Codex 陷阱 5），
    限深 3 防递归环境链。于是 `13` §3 明列的「随返回值/继续方法交给调用者」这条合法路径
    表达得出来了——**此前那是内核两半打架导致的假拒绝**：类型侧已用
    `captures_responsibility` 认了方法能捕获责任，扫描侧却不进环境。

11. **函数哈希只来自 AST——已做（2026-09-21，`13` §4）。** `transform` 的键现在是
    `(site, 代码哈希, 捕获状态指纹, 入料哈希)`。指纹取不到时（捕获里有读数/出口/未决/状态/题，
    或嵌套超过 3 层）**禁用这一项的跨运行缓存**并发 `W-no-cache`，照常执行——
    依 `13` §4 原文「不返回另一方法的结果」，宁可不缓存。
    此前是**会算错**：工厂造的两个方法正文相同、捕获不同时，第二个会复用第一个的结果。

12. **`Pending` 没有承接尚存的责任。** 预算耗尽、`ask` 未答、显式 `pending` 都是绕过函数返回检查
    直接挂起，挂起状态里没有「还欠哪些未决」。可重放的挂起要把它们一起带上。

13. **`E7` 还挂在 `11` 的编号上。** 其余诊断已按 `12` 的 J 表走（预算 → `J-07`，有界循环 → `J-06`），
    但「`for…yield` 体内含 `loop` / `stop`」在 `12` 的 J 表里没有对应条目，硬塞一个 J 号比留着
    `E7` 更糟。请裁定给它一个 J 号还是保留 `E7`。

14. **`escalate` / `literalize` / `unsure_cause` / `untested` 是 core 为这套责任协议新加的内置**，`11` 的标准库表
    和 `12` 的 §6 速查里都没有。名字与参数顺序按 Codex 给的概念签名定
    （`escalate : U(q) ⊗ HumanRequest(q) ⊸{ask} Exit(q)`、`literalize : U(q) ⊗ LiteralPlan(q,q') ⊸{judge} Exit(q')`）。
    要不要收进依据文本、要不要改名，请裁定。另外「问题确实更字面」core 只保证走了 `literalize` 这条
    受检路径，**不**保证新题真的更字面——那要构造约束加校准依据，不是包一层 `Text` 就算证明。


## 四·六 校准记录的运行期写入口（A 部分）

`12`:347 亲口把它标为「未定」，并说 `on_truth` 要等它：**「真值到达的通道、`CalibRecord`
的运行期写入口与并发/顺序语义，两边（Rust 与 Python）都没有……所以 `on_truth` 现在移植
过去只会得到一个没有生产者也没有消费者的注册表。」** 这一包做的是其中第二样。

**产出端是缓冲区，不是写库**：`Outcome.evidence: Vec<(校准键, Sample)>`。程序发出证据，
**宿主决定折不折进 `CalibStore`**（`CalibStore::absorb`）。这样 I4「程序里不可写线」在
字面上仍然成立——程序连库的可变引用都拿不到。

**必须知道的一条：无标注的观察不进 `n`。** `samples` 是 `[[p, label]]`，而一次读数只有
`p`、没有 label。`put` 自己的报错写着「上岗记录必须带 n > 0（**线只从标注记录来**）」，
**标注**二字是承重的。所以无标注的观察进 `samples`、把冷记录推到 `待真值`，**`n` 不动**。
`待真值` 本来就是四个状态里为这件事留的那一格。

**来源是算出来的，不是填出来的**（`CalibRecord::provenance()` → 空 / 宿主手填 / 程序积累 /
混合）。**Python 的 `CalibRecord.source: str = ""` 全仓零引用**——没人写也没人读。一个
自由字符串正是「给没有类型的东西补来源」那个形状：替身会把它填成让检查恰好通过的值。

**重放不重复计数**：积累点挂在 `flush` 的发出路径上，而重放走的是 `ledger.get` 分支，
**根本不经过那里**。不需要再加一个「是不是重放」的开关——少一个开关就少一处可关错的地方。

**没有 Python oracle 可对照**：Python 有 `samples`/`source` 字段但**无任何运行期写入口**，
模式级回退也零实现。这一格的正确性只能靠依据条文和这里的测试撑着，下一个人有权知道。


## 四·七 模式级键回退（B 部分）

`12`:136：「题级样本不够时用模式级校准做先验收缩（jev-f6）」。**只做查找那一半。**

**模式键 = `(phys, literal_mode)`**，`CalibStore::mode_key`。五元组里去掉 `q_text_hash`
就是模式键，**但不能只剩 `literal_mode`**：那样 noul 与 choice 的线会并到一格，
而「不同尺不可比」是本项目自己的判据，`delta_for` 也早就按 `op.phys()` 分。
键落在 `\u{1f}` 开头的保留命名空间，与任何题级键名都撞不上。

**留痕对 handler 可见**：`Exit.line_source`，内置 `line_source(出口)` →
`""` / `"题级"` / `"模式级"`。**模式级的线不能冒充题级的线**——看不见来源，handler 就只能把
「这道题测过 200 条」和「这类题测过 200 条、这道题一条没有」当同一件事办。

**三条不放行**：
- **`停岗` 够不着回退**（提前返回 `drift`）。停岗是人下的判断，用类级先验把它放行是假放行。
- **只有 `上岗` 才供线。** `冷` 与 `待真值` 都算「没有题级线」——`get` 命不中时合成的
  `0.65/0.35` 是**缺省值不是线**。
- **两级都没上岗 → 还是 `cold`。** 回退没有把所有冷键都放行。

**这里修掉了 A 与 B 交界处的一个洞**：`absorb` 把冷记录推到 `待真值`，而改造前
`待真值` 会落进过线比较、用上那个 `0.65/0.35` 缺省值。于是「程序积累了几条无标注观察」
会**静默地**把一个原本 `Unsure(cold)` 的键变成 0.65 就放行。**积累证据不该改变判定。**
两包分开看都看不见它，合起来才看得见（`mode_fallback.rs::待真值不供线`）。

### 回退够不着的三处（各有测试）

- **`fit` 的结果不借模式级先验。** `cut(fit结果)` 不带第二参时 `key = "fit:{名}"`，
  那条记录几乎从不上岗（fit 的校准住在 `error_rate` 里，不是一条线）。掉到
  `mode_key("noul", …)` 上就是**跨种借线**——fit 的可靠性与「裸 noul 判断这一类的
  可靠性」毫无关系。**实测过**：修之前那段程序返回 `{"出口":"act","线源":"模式级"}`。
- **`停岗`**（见上）。
- **Fail 读数**：先算线只会白告警一句「借用了模式级先验」而其实什么也没借。
  与「没用上线的出口不留来源」同一条——审计物上留一句没发生的事，和留一个没用上的
  来源是同一种假话。**我在字段上防住了，在告警上没防。**

### `literal_mode` 目前在执行路径上够不着（重要）

**键里有这一维，库里有这一维，但没有任何执行路径选得中默认档之外的格。**
`cut` 的回退写死 `LiteralMode::default()`，`flush` 里 `Sample.mode` 也写死默认——
因为 **`Reading` / `Question` 根本不带 `literal_mode` 字段**。

**后果**：宿主给 `mode_key("noul", CodeLiteral)` 写一条好线，会得到**静默无效**。
`tests/calib_key.rs::键字符串带得上模式` 断言的是**键字符串**不同，它在「有没有人去读
这些键」这件事上什么也不保证——**那是一个构造测试，读起来像功能测试**。

要接通需要 `Q` / `Reading` 带上这一维，那是前端语法那侧（已写进 `COORDINATION.md`）。

### 判定不做：先验收缩估计器（未定范围）

`12`:136 那句还有后半截「做先验收缩」。**收缩强度怎么定、拿什么标定，是研究不是工程**：
要选估计量（James–Stein 一族还是经验贝叶斯）、要定题级 n 到多少才不收缩、
要在标注集上验证收缩后覆盖率没掉。**没有标注集就验不了，而真值通道还不存在**
（`12`:347 未定的第一样）。**现在写一个收缩公式，就是写一个没法证伪的数。**
落地的是**回退查找**：借线、留痕、可被 handler 分辨。收缩留白。


## 四·八 账本头记校准库的哈希

**出口 = f(读数, 线)。读数进了账本，线以前没有。** 头里的 `profile_hash` 记的是档案里的
**缺省线**，而 `cut` 用的是**按键的线**——同一份账本换一批校准记录重放，
**读数一样、出口可以不一样，而账本上看不出**。实测过：同一份账本，`k` 从 0.60/0.30
改成 0.80/0.20 重放，`cost.calls = 0`（读数确实从账本读回）、出口从 `act` 变成 `band`、
**修前一条告警都没有**。

`Header.calib_hash` + `effects::calib_hash(store)`，与 `profile_hash` 同形：不同即报
`W-header`，**不报错、不阻止程序**——它只说「这两次不可比」。

**用排除法，不用列举法。** 覆盖整条记录，只减去一张封闭的「说明字段」清单。
**今天那张表是空的**——`CalibRecord` 每个字段都承载行为。**空表不是摆设**：
它是加 `note` 那类字段时该动的那一处，有了它，**新字段的默认归宿是「进哈希」而不是
「被忘掉」**。反面教材就在隔壁：`behavior_hash` 是列举法，**它自己的注释承认了失效方式**。

**`None` 只有一个意思：这份账本早于这个字段。** 空库也有自己的哈希，否则「一条记录都没有」
与「老账本」在头上分不开。**老账本重放因此必报 `W-header`，这是对的**——那次运行用了
哪些线，我们确实不知道。

### 「哪一刻的哈希」这个问题在当前内核里不成立

`Interp` 拿的是 `&CalibStore`，**一次运行之内它长不了**；运行开始与运行结束是同一个值。
增长只发生在**两次运行之间**（宿主拿 `Outcome.evidence` 去 `absorb`）。
所以头记的就是这次运行的全部真相，运行中的增长由 `Outcome.evidence` 自己交代。
**哪天 `Interp` 改成拿 `&mut CalibStore`，这个问题才开始成立**，那时要重新裁。

### 判定不做：第二个「行为承载子集」哈希

档案那一对（`profile_hash` + `behavior_hash`）是**实测顶出来的**：一晚上两次纯文档更正
把对照基准打红而行为一个字节没变。**这里没有对应的实测**——唯一能指名的分歧是 `samples`
长了，而 `absorb` 同时把 `冷` 推到 `待真值`，`cut` 就是按 `status` 分支的，
**所以行为子集在第一次 `absorb` 时照样会动**。一个与第一个哈希在同一事件上触发的第二个
哈希，不是第二个比特。等真出现噪声再拆，那时有事故撑着。


## 四·九 上岗要一张证书

缺口的形状（`rust-core-4` 实测）：`absorb` 收 73 条带标注的样本，记录停在 `待真值`
——**这是设计不是缺陷**，记录不该自己让自己上岗。但全库唯一能上岗的入口 `put`
**只核 `n > 0`**，于是 `put(0.78, 0.22, 73, "上岗")` 直接通过，**而同一批数据的证书是拒绝**。
**把生产端补对了，门就空在下一格。**

**正门是 `CalibStore::commission(key, alpha, conf_delta, cluster_unit)`**：拿这条键
积累来的标注样本跑保形认证，**认过才上岗，线由证书定**。`conformal` 模块是从
`foundation/experiments/conformal-proto` **原样搬过来的**（那三道「空放行区不算解」的
保护**一道不少**——原型实测单独去掉任何一道都不会变红，三道一起去掉才红）。

**`put` 只挡一条路**：记录上有积累来的样本、又没有证书时，`status = "上岗"` 被拒。
**宿主手填（一条样本也没有）照旧可上岗、不要证书**——I4「线来自程序之外」，
宿主为它负责，而 `provenance()` 让责任归属可查。**责任归属可查，就不必再加一道门。**

**拒绝分两种，不能混成一种**（`Refusal`）：
- **认证不过**：带着 `n_needed`——**它是唯一告诉作者「这条路有终点」的东西**。
- **跑不成**（α 越界、没有标注样本、声明了簇却没有簇 id）：**没有终点可报**。
  报一个假的 `n_needed` 比不报更糟——作者会照着那个数去凑样本，**而问题根本不在样本数上**。

**`cluster_unit` 由调用方声明，不许从数据推断。** 推断出来的默认会造出一张写着
「按条核过」的证书，**而真相是没人说过簇是什么**。声明「对象段」而样本没有簇 id
是**错，不是降级**。

**`lo = 0.0`**：证书只管放行那一侧（损失 = 放行区里的假放行），弃权那一侧它一个字也没说。
没有凭据就不放行任何 `Ignore`，带宽最大、`Unsure` 最多。保留记录原有的 `lo` 是不行的
——实测 `hi = 0.295` 而缺省 `lo = 0.35`，**带会翻，而 `put` 的 `lo ≤ hi` 会被自己人违反**。

**证书跟着进 `calib_hash`**：两条记录可以有**一模一样的 `hi`/`lo`**，背后却是两张不同的
证书。**只哈希线，就覆盖了线、没覆盖线的凭据。** 这是上一包选排除法而不是列举法的红利
——证书是新字段，**不动哈希一行代码就自动进去了**。

**`conf_delta` 不是 `CalibRecord.delta`。** 前者是二项上界的置信水平，后者是带宽 δ。
两个不同的保证不共用一个名字。

### 未标定：簇级的判定规则

簇级用 200 次重采样，**我取「全过才算过」**（说不准往拒绝那边倒）。**这条规则未标定**
——原型只报了「200 次里有解几次」，没有定「几次算过」。`Cert.resample` 把 R 和规则
一起记下来，**是为了它可审，不是因为它被标定过**。实测这条规则会把 `choice`（55/200）
也判拒。


## 四·十 类假设 + 降级（`12` §1）：管道修通，H5 走完全程

§1 自己的「为什么」写着：把 jev-1.13 的性质降为绑字段的假设并写死降级规则，
**「换版本程序不改」才从口号变成机制**。此前三层都缺，**最要命的是第三层**：
`check(program: &Program)` 没有 `Profile`——**就算降级逻辑写好了，档案字段也没有路径
能到达检查器**。没填是缺料，**没有管道是缺结构**。

**管道**：新增 `check_with_profile(program, profile)`，`check(program)` 是它「没档案」
那一路的别名。**`check` 的签名一个字没动**——`jpp-cli` 是不能碰的 crate，它那行调用原样还在。
`run` / `run_with_fits` 改为传 `&calib.profile`，**否则管道只在测试里通，
而一个只在测试里通的管道是构造不是功能**（`assumptions.rs::run这条路上档案也到得了检查器`
把 `run` 那一行改回 `check(program)` 就红）。

**判据是 `profile.hash.is_none()`，不是「传没传参数」。** `Profile::default()` 是兜底，
不是一份档案；按参数判，每个默认库都会被读成「有档案这么说过」。

**走完全程的那一条是 H5**（`arithmetic_capable`），不是总控建议的 H7。理由：
**H7 的降级落在 `fuse`（运行期），而运行期早就拿得到 `calib.profile`——它走不到缺的那根管道。**
H5 的降级落在 J-01，而 J-01 在 `check.rs`。两条的档案字段今天都不存在，这一轴上不吃亏。

**三态而不是 `Option<bool>`**（`Tri::真/假/未测`）：`未测` 是 §1.3 末行亲口规定过的
**合法状态**，有自己的降级规则。写成 `Option` 会招来 `unwrap_or(false)`。

**降级只降算术那一面。** `12`:328 J-01 的依赖假设栏写的是「H5（为假时**「算术在宿主」**
降 warn，桥不变）」——降的是算术，不是比较；比较是 I3 的事。听起来像同一类，要保证的东西不同。

**四种状态在报告上两两分得开**：

| 档案状态 | J-01 算术面 | 另报 |
|---|---|---|
| `arithmetic_capable: true`（H5 不成立） | **warn**，并说出是哪个字段让它降的 | — |
| `false`（H5 成立） | error | **不报未测** |
| 字段缺失（未测） | error（J-15 保守项） | `W-untested`：「档案里…未测」 |
| **没有档案** | error（保守项） | `W-untested`：「本次没有加载档案」 |

**不另存一份档案 JSON**：一份写着 `arithmetic_capable: true` 的文件摆在真实测量旁边，
**迟早被人读成一次测量**。测试读真档案、在内存里注入这一个字段。

### 与总控指令不符的一处，以依据为准并已上报

总控的反面要求写的是「**没加载档案时不许默认成「假设成立」**」。**这一句对 H5 是反的**：
H5 成立 = `arithmetic_capable: false` = J-01 **保持 error**；不默认成「成立」就等于默认成
`true` → 降 warn → **fail-open**。

那句话对 H7 是对的（「成立」把 `fuse` 打开，那是优化不是守卫）。所以 **`12` §1.3 末行
那句一刀切的「取该假设为真（保守）」本身不够精确**：**保守的默认是逐条的，
由「哪一侧保住守卫」决定，不由统一的「取为真」决定。** 这是「这个默认值在替谁说话」
往上抬了一层。总控的**硬要求**（未加载必须与「档案说成立」区分得开）两种读法下都成立，
已如实照做。

### 另外七条这一包不接，逐条理由（都已核过）

| 假设 | 不接的理由 | 核法 |
|---|---|---|
| H1 `text_only` | 档案字段不存在；`modalities_accepted` 在，但降级表说的是渲染类 `do` 可为空操作，**core 里没有渲染类 `do`** | grep |
| H2 `fixed_output_types` / `select_sums_to_one` / `k_limit` | **字段都在**，但 `k_limit` **全树没有任何一处读它的值**（只出现在 `behavior_hash` 的字段名表和一句注释里）；降级要接的「下沉 pass」不存在 | grep `k_limit` |
| H3 `calibration.ece_by_source` | **`ece` 在内核零命中**；`CalibRecord` 上没有这一项 | grep `ece` |
| H4 `one_hop` | **§1.3 的降级表里根本没有 `one_hop` 这一行**——字段有了也没有规定好的降级可落。**这是依据文本的缺口，不是代码的** | 数 §1.3 表 |
| H6 `window_bounded` | 字段不存在；`window.*` 在。降级是「裂变 pass 关闭」，而裂变 pass 今天是空壳 | grep |
| H7 `questions_free` | 字段不存在；**且降级落在运行期，走不到这一包要证的那根管道** | 见上 |
| H8 `multi_object_crosstalk` / `assertion_susceptible` | 两个字段都不存在 | grep 真档案 |

**`12`:614 那行 G3 从「满足（设计）」往前挪，是总控的事**（`12` 是依据文本）。
这一包只负责把依据备齐：**管道通了，H5 走完了全程，删掉消费方会红。**


## 四·十一 三道门互相夹死之后的修订（一号 / 二号 / 三号）

### 一号：程序判过的任何一个键曾被永久锁在 `待真值`

两扇门互相指向对方：`put` 拦「有 `samples`」→ 指向 `commission`；`commission` 要
「有带标注的样本」→ 而**运行期写入口推的每一条 `Sample` 的 `label` 恒为 `None`**
（真值通道还不存在）。**跑程序收读数 → 人写线 → 上岗，这条最常规的路被锁死**。

**判据：`put` 那道门取错了量。** 它取 `samples.len()`，该取的是 **`labeled()`**——
**一条只有无标注观察的记录没有积累任何「关于线的证据」，它只是判过几次**；
拿它挡 `put`，是把**观察**当成了**证据**。

**`provenance()` 也分错了类**：`(n=0, obs=1)` 落进 `n == labeled()` 即 `0 == 0`，
判成 `程序积累`——**而门正按这个判**。新增 `Provenance::只有观察`，并改成按
`labeled()` **显式分档**，不靠一个在 0 上偶然成立的等式。
**「算出来的答案填不错」的前提是那个算法对。**

**`put` 的失败开放**：拦住之后记录保持原状，**旧线继续放行而人已经不能收紧它**。
判据是**方向**：`hi` 更高、`lo` 更低 = 带更宽 = `Unsure` 更多 = 往拒绝那边倒，
**这个方向不需要任何凭据**。所以 **收紧永远放行，放宽才要凭据**。

> **我的第一版改成「拦住就停岗」，更糟**：`commission` 明写着不经由它复岗，
> 于是那个键**永久死掉**——**正是这一包在修的那一类锁**。写下来当路标。

### 二号：手填的线免检那条豁免被撤销（不阻塞，但让它可见）

原豁免的两条理由都不成立：(a) 这道门**拦不住那个被用来论证要建它的例子**——
`cost_line` 算出来的线进库只能走 `put`、不带 `Sample`，于是免检；
(b)「责任归属可查」的确切含义曾经是「一个人可以在 Rust 里自己调一次那个函数」
（`provenance()` 在 `src/` 下**零消费**）；(c) **就算真可查，那也不是证书回答的那个问题
——证书答的是「这个键在这批数据上假放行上界多少」，那是键和数据的性质，
不是写线的人的性质。责任归属不能替代证据。**

**`line_source` 现在带两件正交的事**：`题级 / 模式级`（**哪一层**）· `手填 / 证书:α=…`
（**凭什么**）。第一版我拿后者盖掉了前者，层级信息就没了——`题级有线时用自己的` 当场红。

**强出口建在未认证的线上 → `W-uncertified`**，不阻塞。**判据是 `certs.is_empty()`，
不是 `n` 的大小**：出货示例拿 `n = 1` 的手填线拿到强出口、零告警，而 `n = 22` 还不够——
**同一个字段，一条路上 1 就够、另一条路上 22 还不够，中间没有任何东西说出这个差别**。
说出它的是「有没有证书」，不是那个数。

### 三号：证书按限定寻址，不再覆盖

实测：同一个键认证两次，α=0.60 线 0.195 → α=0.80 线 **0.000**（**从「过线才放行」
变成「全放行」**），而被覆盖过的记录与只认证过一次的记录**逐字段相同**。

**哈希是封条，不是地址。** 它答「变了没有」，不答「这是哪一批材料、哪个 α 上的」。
**要防的不是篡改，是误用与合并**——没有东西被改，是**两个不同的测量占了同一个格子**。

`CalibRecord.certs: BTreeMap<地址, Cert>`，地址 = `(α, conf_delta, 簇单位)`。
**选择规则：α 最小的那张**——α 越小 = 风险目标越严 = 线越高 = 越保守，
于是**后认一个更松的 α 永远不会把线放宽**，覆盖那个坑从规则上就不存在了。
`hi`/`lo` 是那条规则算出来的**视图**，`line_source` 报的那张出自**同一个函数**，
不是第二处真相。

### 验收多的那一条（总控立，从这包起常设）

**把这包和上一包合起来，有没有哪条路被两道门夹死。** 这已经是第二次
——第一次是 `absorb` 推 `待真值` 撞上 `cut` 的缺省线，第二次是这一包。
**逐包验收按定义看不见这一类。**

### 顺带：两处「不许有任何告警」的断言改窄了

`adaptive.rs` 与 `partial.rs` 原来断 `warnings.is_empty()`，而它们真正要断的是
「不该有 W-bound / W-header」。**一个「不许有任何告警」的断言，会逼着后来的人
去掐掉正确的告警。** 改成按前缀过滤。


## 四·十二 `cut(cost={fp, fn})`：代价矩阵定线定在哪

那条裁定此前只落地了一半。证书那半在；**「代价矩阵决定线定在哪」这半在语言里根本不存在**
——门装好了，旋钮没装：作者能被告知「你这条线没凭据」，却**没有任何办法告诉语言
「我这一格的误放行比漏放行贵十倍」**。

### 验收数字（noul，73 条真机读数，与 Python `cost_line` 逐值一致）

| 代价矩阵 | 线 | 放行 | 误放行 | **经验假放行率** |
|---|---|---|---|---|
| `fp=1, fn=1` | 0.590 | 50/73 | 15 | **0.300** |
| `fp=10, fn=1` | 0.780 | 6/73 | 0 | 0.000 |
| `fp=1, fn=10` | 0.385 | 63/73 | 22 | 0.349 |

**0.300 是难看的那个数，它就在这儿**：代价对称时这批数据上的线就是这么烂，
**而在这一包之前没有人能把它调成不对称**。

**率必须和放行集合一起看**：`fp` 重那组率降到 0.000，**而放行集合从 50 缩到 6**。
**一个缩小的放行集合把率压低了，那不是变好。** 所以 `CostLine` 同时带 `n_accepted`，
且**放行集合为空时 `false_accept_rate` 是 `None` 不是 0**——与 `binomial_upper` 的
`n == 0 → 1.0` 同一条：**空放行区上的「零错」不是证据**。

### 两半怎么合（裁定的字面实现）

`commission_costed(key, α, conf_delta, 簇单位, (fp, fn))`：
**线由 `cost_line` 在标注集上定，证书只回答「这条线的假放行上界够不够 α」**——
`certify` 自己找线那条路在给了代价矩阵时不走。**代价决定线定在哪，证书决定能不能上岗，
不是二选一。** 实测这条路走得通（`cost_line.rs::代价定线证书定能不能上岗`），
**没有被两道门夹死**。

**J-16 照 Python 拦**：`label_set_id` 必须给出且 ≠ `set_id`，否则代价线与保形线同源。
`CalibRecord` 为此新增 `label_set_id`。**`cut(cost=)` 只对 test 题有定义**
（Python `runtime.py:1147` 直接 raise）——让它悄悄什么也不做，比报错糟。

**代价矩阵进证书地址**：换代价矩阵 = 换一个测量，**不是覆盖同一个**。
选择规则相应加了一格：**先按 α 最小，同 α 时取线更高的那张**。
这一格是代价矩阵进来之后才有内容的——一张 `fn` 重的（线 0.385、放行 63）和一张
`fp` 重的（线 0.780、放行 6）可以在同一个 α 上都认得住，**按地址字符串挑就可能挑中宽松的那张**。

**`line_source` 加了第三轴**：`题级·证书:α=0.45·代价(fp=10,fn=1)`。
「哪一层」「凭什么」「怎么算的」是三件事——**把代价折进「凭什么」那一格，
就是把一小时前刚红过的那次合并再做一遍**。

### 一处我自己造的分叉，记下来

我第一版顺手抄了 `certify` 的 `ps.dedup()`，**而 `cost_line` 的候选表不去重**
（Python 里重复值之间的「中点」就是那个值本身，**候选表因此含样本点本身**）。
线从 0.590 掉到 0.585，**而放行集合恰好没变——三个数里有两个仍然对得上**。
是跑 Python oracle 逐值对照才抓住的。**两个函数在同一个文件里、长得像，候选规则不一样。**

### 判定不做：保形版的代价线

`12`:159 写着「给 `cost` 时线由**保形风险控制**在标注集上按代价求出」。
**那要保形与代价同时成立，而保形今天在这批数据上拿不到有意义的证书**
（noul α=0.10 被拒，`best_ucb 0.319`）。先把经验版做出来，**保形版等有证书的键出现再说**。

### 顺带修掉：`仅供参考` 的类型闸此前只拦 Rust 调用者

`strength.rs` 的 `仅供参考(f64)` 不实现 `PartialOrd`，**Rust 侧比不了**。
而内核把它转成 `Value::Float` 交给 J++ 程序——**这门语言唯一的用户拿到裸浮点，
可以直接当判据，没有任何东西会红**。「替身上成立、真机上失效」的又一张脸，
**只是这次的「替身」是宿主语言**。

改成交 `Text`：`"0.4880（仅供参考，不可作判据；判据是 union_bound）"`。
**看得见、打得出，比不了大小、做不了算术**——两侧同一条纪律。
测试逐条断了：`union_bound` 照常能比，`independent_any` 比不了也算不了，
但取出来时那句话跟着。


## 四·十三 J-08 在 handler 臂上此前完全不生效（急修）

`self.guards` **只在 `ExprKind::If` 一处 push**，而判定是
`if !action.reversible && !self.guards.is_empty()`——**空栈直接跳过整条检查**。
于是写在 `handle` 臂里的不可逆 `do`，在 J-08 眼里是「无条件执行」，**完全不受管**。
**而那不是怪写法，它就是 §5 与 J-05 推荐的写法。**

**`tests/guard.rs` 十一条全绿照不出来，因为十一条全走 `if`。**

**判据（总控订正）**：**handler 的 `act` 臂根本不是无条件的——它之所以执行，
正是因为那次 `cut` 切出了 `Act`。那个出口就是守卫。**

**修法**：`handle` 在执行臂体之前压一层 `GuardInfo { trusted: e.taint == Trusted,
asked: e.from_ask.get() }`，**出错路径也弹**（早返回会把脏栈留给这次运行的其余部分）。

- **`asked` 不从 taint 推**：`trusted`（状态可信）与 `asked`（经人拍板）是 J-08:265
  亲口并列的两个析取项。
- **`act`/`ignore`/`pick`/`at` 全走同一条**。总控只点名了前两个，**漏掉后两个
  就是把同一个洞挪过去一个枚举分支**。
- **`unsure` 臂也压。** 那里拿到的是未决责任，**本来就不是一个放行判定**，
  所以它不提供 `trusted` 合取项；但**不压就是空栈 → 整条 J-08 跳过**，同一个洞。

**要留意的回归面**：改完之后**每个 `handle` 都会压一层守卫**，`guards.is_empty()`
在一大批以前为真的地方变成了假。`可信状态上的臂照常通过` 是这次的回归闸——
**没有它，就会发出一个把正常程序也拦住的规则，而且要等别人的测试才发现。**

### 钉住的失败开放形状：外层可信守卫会让内层脏臂通过

`if 可信判断 { handle(脏出口, {act: fn(){ do(不可逆) }}) }` **放行**——
`guards.iter().any(…)` 在外层那一项上就满足了。**这与 `if` 今天的合取语义一致**
（任一可信合取项即放行），所以不是这次引入的；**但它是失败开放的形状，
写明并钉了测试，不留给日后发现**。

## 四·十四 证书只界定放行那一侧（`Cert.bounded_side`）

`commission` 置 `lo = 0.0`，于是**任何拿到证书的键，`Ignore` 实际不可达**
（`p <= lo` 只有 `p` 恰为 0 才出），**而 J-05 仍然强制作者为那条永不执行的分支写代码**。

原注释给的理由是「说不准往拒绝那边倒」，**而那句话把「拒绝」默认等同于「不给 `Act`」**。
**`Act` 与 `Ignore` 在出口代数里是对称的两个判定**，哪一个安全取决于程序怎么用——
写 `if 不安全(x) { 拦下 }` 的时候，**`Ignore` 才是放行的那个答案**。

**这件事现在是数据不是注释**：`Cert.bounded_side` =
「证书只界定 `p ≥ hi` 一侧的假放行；`p ≤ lo` 一侧没有界，且 `lo = 0` 使该出口实际不可达」。
它进记录、进 `calib_hash`，作者读得到。

**`lo` 的值这一包不动**——改它要重新想另一侧的界，那是另一包。
**`bounded_side` 今天不进证书地址**：只有一个取值，进去也分不出任何东西，
只会把现有地址全改一遍。**哪天有双侧证书，它必须进地址**——那时两侧界不同就是两个测量。

**不在运行期告警**：它会在每个已认证的键上响，那正是刚从两个测试文件里滤掉的那种噪声。


## 四·十五 `label_source` 进证书地址

今晚最重的测量发现：E-CAL 的校准集**在定义上就是「两个模型都同意」的那个子集**
（202 条真值两模型一致 202/202 = 100%，未标注的 95 条里只有 77/95 = 81%），
而「是否同意」与「对不对」**已实测相关**（noul +0.527、choice +0.418）——
**所以这个集合上算出来的每个数都同向乐观有偏**，包括 ECE 0.057、AUC 0.748，
**以及今后每一张证书**。

这件事横跨数据/代码/流程**写在六处**，那个数照样丢掉一半（限定留存率 ECE 47%、AUC 46%）。
**加第七处会得到第七个 50%。** 所以：**限定进地址，不进注释。**

判据与 `cluster_unit` 同源：**不记「簇是怎么分的」，`n` 这个数就没有意义；
不记「标签是怎么选的」，那张证书也没有意义。**

### 两处判断与它们的代价

**一、带载荷的枚举，不是自由字符串。**
`provenance()` 那次的理由是「算出来的答案填不错」，**但这里算不出来**——
标签怎么选的是系统之外的事实。枚举给的是自由字符串给不了的那一样：
**`全体` 是一个要有人明确声明的断言，不是一个谁都会漂进去的默认。**

**代价落在有一种结构上全新的标签来源的人身上**，他必须来改 core——
**而那正是该改的地方，因为一种新的结构改变了证书的含义**。具体判据写在
`选择子集 { 判据, 与对错相关 }` 的载荷里，不用改 core。

**`与对错相关: None` 是「没测」，不是「不相关」**——与 `Tri::未测` 同一条。
**一个没测过相关性的选择子集，比一个声明了 0.0 的更可疑，不是更不可疑。**

**二、旧记录：`未声明` + 强出口留痕。** 缺失值**不许默认成 `全体`**
（那是替不确定说了确定，方向还是乐观那一侧），但也不一律作废：记录照常可用，
**而它给出强出口时报 `W-label-source`**。

**为什么这里告警、而 `bounded_side` 不告警**——这是一条可复用的分界：
**`bounded_side` 今天只有一个取值，一条在每个已认证键上都响的告警承载零信息**；
`label_source` 区分得开记录，**它响的时候是在说一件别的记录不成立的事**。
而且**这条告警的正确稳态是「消失」**（声明过就不响）——
**一条稳态为零的告警，与一条永久噪声不是一回事。**

### 一处总控的验收里隐含、我替它定了的

验收写的是「`hi`/`lo`/`n`/`status`/`cluster_unit` 全同、只有 `label_source` 不同的两条记录，
`calib_hash` 必须不同」。**但 `label_source` 进不进证书地址，这句话没说，而答案不同结论就不同**：
- **进地址**（选了这个，与 `cost` 一致）：同一个库里认两次得到**两格**，
  所以验收必须用**两个各只有一张证书的库**去比——否则比的不是「同一张证书换了标签来源」。
- **不进地址**：后一张**盖掉**前一张——**刚修好的覆盖那个坑换了一根轴重演**。

与 `bounded_side` **不进地址**的分界：那个今天只有一个取值，进去分不出任何东西；
**这个有好几个。不同规则，同一条理由。**

**证书冻结认证那一刻的声明**：后来改 `set_label_source` **不追改已发的证书**，
与「线重算之后已经发出的出口不改」同一条。

## 四·十六 判定不做：给 `CalibRecord` 留 `drift_stat` 的位置

`conformal::drift` 有实现、**零调用点**；`CalibRecord` 没有 `drift_stat`；
`cut` 的「停岗」分支只读一个人手填的字符串。

**不留那个位置。** 加一个没有生产者也没有消费者的字段，正是
`on_truth`（`_truth_hooks` 写了从不读）与 `CalibRecord.source`（Python 全仓零引用）
**同一个形状，这会是第三次**。那两次都是按这条理由判的不做，这次一致。

**要接就整条接**：谁来算 `drift`（每次运行？按窗口？）、`underpowered` 时谁不许停岗、
停岗之后怎么复岗（`commission` 明写不经由它复岗）。**那是一包，不是一个字段。**


## 四·十七 `CalibStore::load`：内核第一次读得到 E-CAL 那三条真记录

**「把那个数真的记上去」此前没有目标可记。** `CalibStore` **没有任何落盘/装载**
（`effects.rs` 里 `fn load` 只有 `Profile::load` 一处），**CLI 也从不构造 `CalibStore`**。
E-CAL 那三条记录（`foundation/runs/jv/e-cal/calib/*.json`，`上岗`，n = 73 / 74 / 55）
**只活在 Python 的格式里，Rust 内核一次也没读过**。

**装载不许静默丢字段**（与 `Profile::load` 缺字段就报错同一条）：文件里有而内核没地方放的
字段，要么报错，要么在一张**具名**表里「知道但不映射」——今天是
`source` / `cost_matrix` / `drift_stat`。**无声吞掉一个字段，和把限定写进注释是同一件事。**

**`samples: null` 映射成空表**：敢合并这两者，是因为 **Python 自己的消费方
`runtime.py:1149` 是 `if not rec.samples`，`None` 与 `[]` 走同一条路**——
合并的是一个本来就没有行为差别的区分。

### 那三条记录里，`label_source` 已经写着了——埋在 `source` 散文里

> `label_source=模型双标+人抽检（Fable+Opus 标签一致且至少一方置信高；两方一致但都非
> 高置信的条目未并入，另列核对臂）`

**而 `source` 正是那个「Python 全仓零引用」的自由字符串字段。**
这就是「写在六处、那个数照样丢掉一半」的第六处，一字不差。装载读出来是 `未声明`，
**因为它在散文里不是字段**——`W-label-source` 会响，这是对的。

### 判断：`与对错相关` 是标量，够用；而它的前提此前没有东西保证

相关性按题型分叉（noul **+0.527** / choice **+0.418** / score **−0.062，反向**）。
**一条记录一个标量是对的——因为一条记录就是一个题型**（键里有 `phys`）。

**但那是约定不是保证**：`absorb` 收任何 `phys` 字符串，而 **`commission` 的非代价路径
从来不查题型是否同质**（代价路径查）。**「不同尺不可比」是本项目自己的判据，
而认证路径上漏了一处。** 这一包补上：混题型的样本集不许认证。
**于是标量正确是因为有东西保证它，不是因为大家都这么用。**

**它在哪种情形下不够用**：当同一个选择判据的相关性**在一个题型之内**还要分叉
（按领域、按语料来源、按时间）。今天没有东西表达得了这个。
**`score −0.062` 与 `noul +0.527` 反号本身就是警号**——
**一个偏差会跨题型变号的选择规则，在一个题型之内也未必稳。**

### 没做的那半，和为什么

**没有手改那三个 JSON。** `label_source` 是一个**关于测量的声明**，不是伪造读数，
所以不违反「不改测量」；但那三个文件有生产者脚本 `e_cal_写校准记录.py`，
**手改会让树和脚本不一致，下一次跑脚本就覆盖掉**。
已在 `COORDINATION.md` 请那边把 `label_source` 作为**结构化字段**写出来
（而不是继续埋在 `source` 散文里），装载这边已经认得那个字段。


## 四·十八 标注集：**算需要数据，审计只需要身份**

问题：那 202 条标注该整份进校准记录，还是只进摘要？**两边都不对**，
而分岔在于把两件不同的需要混成了一件：

- **算一条线**需要真的 `(p, label)` 数据——**摘要算不出线**。
- **事后审计 / 重放比对**只需要知道**是哪一份**——**不需要材料本身**。

**第三条路**：证书里记**标注集指纹**，记录里记它的 `id` 与**去处**，材料本身不进记录。

**指纹是算出来的，不是填的**，而 `label_source` **必须声明**——两者是同一条理由的两侧：
**算得出来的不许填**（填得错），**算不出来的必须有人明确说**（标签怎么选的是系统之外的事实）。

### 三处边界，说明白而不是日后发现

**一、重放成立，而且比「整份进记录」更好。** 若 202 行进记录，
**任何一次重新标注都会静默移动 `calib_hash`，而「重标了」与「重采样了」分不开**。
指纹只记身份，两份库一致当且仅当它们指的是同一份标注集——**那正是重放该断的事**。

**二、跨机器：验得了、重认不了，而那是诚实的边界不是缺口。**
没有数据的机器上，线读得到、证书的主张核得了、指纹比得了；**只是重新认证需要数据，
而重新认证本来就该需要数据。**

**三、指纹算的是「用到的那些 `(p, label)` 对的规范形」，不是源文件字节。**
源文件会被重新导出、重新排序——**按字节算会在数据没变时乱跳，
而一个会乱跳的检查会教会人绕过它。** 排序后取 `canon` 再 sha256 前 16 位，
**对写入顺序不敏感**（有测试）。

**去处只给人看，检查只信指纹。** 去处解析不了是「这台机器上没有那份数据」，
**不是「检查失败」**——别让定位符进验证路径。

**指纹进证书地址**：两份不同的标注集是两个测量，哪怕 α、簇单位、`label_source` 全同。
与 `label_source` 的分工：**那个说「怎么选的」，这个说「是哪一份」，互相替代不了。**

### 顺带核过的一件（因为要断言才去核的）

`tests/ecal_fixture.json` 的 52 个对象段**全部 ≤ 24 字**，
邮箱 / 电话 / URL / 公司后缀 / 姓氏+称谓 **各 0 命中**。
本来只是要论证「材料已经在树里了」，**核完发现那个论证不成立也不需要**——
但核过比假设过好，结论照记。


## 四·十九 `allocate` 的退化：根不是「冷键」，是**没有档案**（急修）

报上来的现象对：`allocate` 会静默返回「按下标取前 k 个」，零告警——
**而这正是「按不确定性分配预算」最该起作用的那一类键。**

**但根的定位要改一格，而差分夹具自己就是反例。**

`uncertainty` 对非上岗记录取**档案保守线**。`oracle.json` 的 `safety_lines = [0.75, 0.25]`
是一条**测出来的**线，冷键在它上面照样分得出 0.52 与 0.93——**Python 给 `[1, 0]`，
不是下标序**。**所以「冷键就退化」不成立。**

退化的是 **`Profile::default()` 的兜底 `hi = 1 / lo = 0`**：带是整个 `[0, 1]`，
**每一个 p 都在带内、每一条都得 `0.0`**，降序并列按下标 → 「前 k 个」。
**而 CLI 从不加载档案**（`--profile` 仍在 `COORDINATION` 里等），
所以从 `.jpp` + CLI 这条路进来的人，**百分之百撞上它**。

判据还是那条：**「这个默认值在替谁说话」**。一条测出来的保守线是**粗，但是真的**；
一条代码兜底线什么也没说，**而它说出来的那个 `0.0` 恰好排在最前面**。

**修法**：`uncertainty -> Option<f64>`，`None` = **记录不上岗且档案没加载**
（`profile.hash.is_none()`，与账本头那条同源）。**算不出来不是一个值，
更不能是一个恰好排在最前面的值。**

`allocate_report` 给两栏：`picked` 与 **`算不出`**。
**排不了的不是「最不确定」也不是「最确定」——它没有位置**，
混进同一张榜就是拿「离全局兜底多远」和「离这道题的线多远」比大小，**那是两把尺子**。

**`.jpp` 侧 `allocate` 改为返回记录** `{picked, 算不出}`——那一栏必须对程序可见，
不能只进 trace。同时报 `W-untested`（载体 `uncertainty`）。

### Python 侧同一个洞，这次 Rust 先补

`runtime.py:1243` 同式。**这不是移植分叉。** 差分测试里 `cold_record` 一例
在 Rust 这边现在仍与 Python 一致——**因为那个夹具代表「档案加载过了」**
（`store()` 里补了 `hash = Some(...)`：那两行填的是 oracle 测出来的线，不是兜底，
不标上就会被自己的新规则当成兜底）。**真正分叉的是「没有档案」那一格，
而那一格 oracle 里没有用例。**


## 四·二十 从外面走 `.jpp` 这条路撞到的四件

共同形状：**内核手里有答案，只是没说出口。**
**「说不准」与「说不出」是两回事——这几条全是后者，而后者是免费可修的。**

**一、固定观察未命中，现在打出内核自己算的那份状态与题。**
以前只给哈希，于是作者猜 `measure` 的档位字段名猜了四次
（`bands` / `levels` / `grades` / `options` 全不中，最后是 `scale`），
**每次拿到同一句话、同一个哈希，没有任何梯度**。那份 JSON 就在手边。

**二、fixture 加载器的字段纪律：不在这边**（`crates/jpp-cli/src/fixture.rs`），
已写进 `COORDINATION.md`。**两个加载器一个有纪律一个没有**——
`CalibStore::load` 报错并指名，fixture 加载器静默吃掉，
于是「**我写错了字段名**」与「**我写对了字段名但状态真的不一样**」在报错上分不开。

**三、`allocate` 在自己那一步说话**（`df0a9c2` 已落地）。
要点是**位置**：冷键那条 `W-untested` 是在 `cut` 时发的，
**而 `allocate` 的典型用法正是「挑 k 条送人工、其余不过线」——那条路上一句告警都没有。**

**四、修法要标明谁能执行**：`【作者可改】` / `【需接线人】`。
`W-uncertified` 的「走 `commission`」、`W-untested(permutation)` 的「开置换」
**读者是 `.jpp` 作者、执行者是 Rust 接线人**；而同一个前缀下
`W-untested(cold)` 的「写一条上岗记录」作者做得到——**一条可执行、一条不可执行，
以前长得一模一样。**

### 顺带：哪些动作不可逆，现在作者查得到

`do` 打错名字时，J-11 的报错**列出本次登记的全部动作及其可逆性**。
**J-08 保护的是不可逆动作，而作者此前没有任何办法知道哪些动作不可逆**——
**一个作者无法查询的安全边界，等于没有边界。**

### 那位作者「一次没见过 J-08」的原因：**两个，都不是 taint 漏了**

1. **CLI 只登记三个动作，其中只有 `write_json` 不可逆**（`runner.rs:45`）。
   J-08 只管不可逆动作，另外两个压根不在它管辖内——**这是设计不是缺陷**。
2. **`gen` 的输出 taint = ∨ ctx.taint**（`12`:269）。**没有 ctx 时结果是 `trusted`**，
   于是状态可信、出口可信、守卫可信，**J-08 照规矩放行**。
   而那个 `trusted` 是 **∨ 的单位元不是兜底值**——`12`:282 亲口点名过这个区分，
   **把它「修」成 untrusted 才是引入假拒绝**。

两条各自独立成立，有测试分别钉住。


## 四·二十一 「收紧」要按出口种类算，不按 `Act` 一种算（急修）

活口子：`put` 的收紧判据写的是 `hi >= old.hi && lo <= old.lo`，
**于是 `lo` 降到 `0.0` 算「收紧」、免凭据——而 `p <= 0` 意味着 `Ignore` 在那个键上不可达。**
**而那正是 `commission` 自己做的操作**（两处 `r.lo = 0.0`）。

**正确形态**（总控已写进 `12`）：
> **一个改动若使某个出口种类变得不可达，它就不是「收紧」，不论方向。**
> 免凭据的只有「两侧都不扩大放行」的改动，而「放行」要**按出口种类算**
> ——**语言不知道哪一侧对这个程序才是安全的那一侧。**

`CalibRecord::不可达出口()`：**算出来的不是填的**（`hi >= 1` → `act` 没了，
`lo <= 0` → `ignore` 没了）。`put` 现在比对改动前后的这一栏，**新塌一种就要凭据**，
报错点名是哪一种。

**`commission` 照办不变**：它**有凭据**（那张证书），而证书只界定放行那一侧
——所以 `lo = 0` 是证据诚实的值，不是疏忽。留痕在 `Cert.bounded_side` 与
`不可达出口()` 两处，**后者是算出来的，可以被核对**。

## 四·二十二 出口的 taint 对 `.jpp` 可见（`taint(出口) -> Text`）

**判断：要补。** 理由三条：

1. **宪法第 44 行唯一那条 IFC 纪律建在 taint 上，而 J-08 只会在 `do` 那里「拒绝」**
   ——作者拿不到任何**在被拒绝之前**读得到的东西。`taint` 从 `cause` 删掉之后，
   `.jpp` 侧**再没有任何东西**能说出「这个判断站在不可信材料上」。
2. **形状与既有两位完全同族**：`untested`（J-15 那一位）、`line_source`——
   都是「关于这个出口怎么来的、与 `kind` 正交、对 handler 可见」。
   **而 taint 比那两位承重。** 两条裁定相隔六小时，**重的那个待遇更弱**。
3. **代价极小**：`Exit.taint` 本来就在，缺的只是一个内置。

**一条要写明的边界**：**读得到 taint 不等于据它分支就合规。**
J-08 问的是「守卫里有没有一个**来自可信状态的判断**」，
而 `taint(e) == "trusted"` 这个比较结果的来源是**比较本身**，不是那个可信状态。
**别把「我读过 taint」当成「我满足了 J-08」。** 这条我没有构造反例去撞，**标未核**。


## 四·二十三 读 taint 不能替代可信判断（已核，今天就拦得住）

总控裁定：**`if taint(e) == "…" { do(不可逆) }` 不满足 J-08。**
理由：J-08 要的是**一个在可信状态上做出的判断**（一次 `judge`，可信度来自被判断的材料）；
而 taint 比较是**一次字符串比较**，可信度来自「我读了一个字段」。
**两者在守卫里都是 `Bool`，但一个说「可信材料支持这件事」，另一个只说「我看过标签」。**
允许它，**J-08 就变成可以自证的**——那个 `trusted` 恰恰是它要去查的东西本身。

**实测：今天已经拦得住**，三种写法都拦。机制是 `walk_conjuncts` **只认 `&&`**，
**任何别的 `Binary` 都不贡献可信项**，`let` 绑一层也不传递。
已钉测试——**免得有人「顺手」让比较表达式也传递来源，那一改就是开这个口子**。

**一处我自己踩的坑，记下来**：第一版按总控举的字面写 `== "trusted"`，
**而脏材料上 `taint(e)` 就是 `"untrusted"`，守卫恒假、`do` 根本不执行**，
拿到的 `"没做"` 是「分支没走到」不是「J-08 拦住了」。**一个因为错误的理由而通过的测试。**
改成 `== "untrusted"` 才真的走到那一步。

### `taint` 的两条路：同一个问题，但拼法不一致

- `m.taint`（`Mat` 的**字段**，`interp.rs:595`）
- `taint(e)`（`Exit` / `Duty` 的**内置**）
- **`e.taint` 不存在**（`Exit` 只暴露 `.kind`）

**语义是同一个问题**（「这东西背后的材料可不可信」），**取值域也相同**（`trusted`/`untrusted`），
**但拼法不一致**：材料用字段、出口用调用、出口的字段写法报「Exit 没有字段 taint」。
**这不是语义分叉，是接口不齐**——记在这里，统一与否由前端那侧定（属 `jpp-frontend`）。


## 四·二十四 料库：**判定本版不做**，三个问题各有答案，三处发现要先记下

`12`:594 自己把它判成**「漏」不是「不做」**，所以这条判定要带足理由。

### 问一：它和账本是什么关系？——**内容上账本是超集，寿命上不是**

`Entry::Effect { key, kind, output, cost }` **已经存着每一次 `do`/`gen`/`transform` 的输出内容**。
所以**再建一个存内容的库是重复**。

**但账本是「一次运行一个文件」，而 `12`:581 的用例明写是跨会话**（谈判 agent、长期监控、
延迟真值回填）。**一个 view 只看得见一本账本，交付不了那个用例。**

**所以「只做一个」成立，但要说清 view 是架在什么之上**：`MatStore::from_ledgers(&[Ledger])`，
**由宿主负责把哪几本账本凑到一起**。这一句不能留成隐含的——总控问的正是
「会不会一个是另一个的子集」，而诚实的答案是**内容上是，寿命上不是**。

### 问二：「输出默认入库」怎么处理？——**它已经为真，但有一个我自己没堵的例外**

不需要「第三条路」，因为**不存在额外增长**：内容本来就在账本里。

**例外**（本轮查出）：`transform` 的 `W-no-cache` 那条路**在 `ledger.put` 之前就返回**
（`interp.rs:1958`），于是那份材料**根本不进账本**。
**而告警原文只说「不进跨运行缓存」——它少说了一半。** 本轮把告警补全了。
**`12`:182 那句对这条路是假的**，这是依据与实现的一处真冲突，记在此处。

### 问三：跨会话取回时怎么知道是同一份？——**身份那半可以直接搬，取回句柄那半不存在**

- **身份**（「是不是同一份材料」）：标注集那套**原样成立**——`Mat.hash = hash_of(["mat",
  canon(content), addr])` 已经是内容规范形哈希，与 `标注集指纹` 同一条纪律。
- **取回句柄**（「那次会话里的哪一份」）：**不存在**。`12`:581 把「地址」列为料库四件之一，
  **而 `addr` 今天是退化的**：`transform` 恒为 `"transform"`，出口 / `Fail` / 拆包恒为 `""`，
  `gen` 的 n 个输出共用 `"gen:{prompt}"`。**六个生产者里三个的地址不携带任何信息。**

### 为什么本版不做：**两个各自独立的阻塞**

1. **修 `addr` 会打断旧账本重放。** `gen` 的账本键含 `ctx_hash`，而 `ctx_hash` 就是
   `Mat.hash`（`interp.rs:1845`），`Mat.hash` 含 `addr`。**改地址 = 改键 = 旧账本失配。**
   那是一包带迁移故事的活，**不能塞进这一包**。
2. **料库读账本，而账本到 `.jpp` 只有 `--replay`/`--resume` 一条路，且一次一本。**
   **这会是那条合并请求的第四行**——再做一个够不着的机制，是这一夜已经拆过三次的形状。

### 顺带一处「算好了又丢掉」，本版不修但要记

**账本存 `output: Json`，不存 taint**；重放时 taint 由**当前的动作登记表**重算
（`do_` / `generate` 两处）。**于是「取回当初的材料」会拿到旧内容配今天的信任标签。**
对一个明写「携带 taint」的库，这是承重的缺口——**而它今天看不见，因为没人跨会话取过。**


## 四·二十五 漂移监控接线（`conformal::drift` 不再是零调用点）

### 三问先答

**一、谁来调：内核在 `cut` 那一步算，因为数据已经在记录里。**
**参照分布 = 带标注的那些**（线是在它们上面定的）；
**近期分布 = 运行期积累的无标注观察**（`absorb` 从 `Outcome.evidence` 收来的）。
**不需要新的数据源，也不需要宿主配合**——所以**它不是那条合并请求的第五行**。

`CalibStore::drift_of(key) -> Option<DriftReport>`。**一侧为空返回 `None`，
不是「漂移为 0」**——与 `binomial_upper` 的 `n == 0 → 1.0` 同一条。

**消费方是「用这条校准记录」这个动作**（`W-drift`），**不只是 `cut`**：
`cut`、`allocate`、`unsure_bound` 三处共用 `Interp::报漂移`，**每个键每次运行只报一次**（跨消费方去重）。
〔**2026-09-21 16:42 改**：原来只挂在 `cut` 上。而 `uncertainty` 读 `lines_for`、
`unsure_bound` 读**只认「上岗」记录**的 `unsure_rate`（它交出去的是 **J-10 的联合上界**）——
**只 `allocate`/`unsure_bound` 不 `cut` 的程序照样在用这条线，漂了却零告警，失败开放。**
实测：`tests/drift.rs::不经cut的消费方也要报漂移`，修前告警表是 `[]`。
**`delta_for`（`order`/`uncertainty` 的 δ）没接进来，那是一条判断不是实测**：δ 是档案的迟滞带宽，不是线。〕
**否则 `drift_of` 就成了第二个「有实现没调用点」的东西，而那正是这一包要修的毛病。**

**没实现的那一半，写在这里**：`12`:649 的原话是「漂移监控（无标签：**读数分布偏移 + 保形覆盖跌落告警**）」，
**`drift_of` 只算前一项**（KS/PSI）；**保形覆盖跌落告警全树零实现**（`certify` 算的是认证那一刻的覆盖率，不是运行期）。

**二、`underpowered`：照报，但不构成停岗的依据**（`DriftReport::可停岗()`）。
当初加那一位的理由是「一次 20 条的抽样不该把一个键停岗」；
**而更硬的理由是今晚长出来的：复岗今天不存在（`commission` 明写不经由它复岗），
所以一次假停岗是永久的。** **两种错的代价不对称，而不对称的那一侧是不可逆的那一侧**
——这比「往拒绝倒」更准，因为「往拒绝倒」在这里会指向自动停岗。

**三、停岗之后怎么复岗：这一问不成立，因为漂移不停岗。**
`12`:396 写的是**告警**。停岗仍是人下的判断（走 `put`）。
**一个只报不动的机制造不出永久锁**——而我今晚正是在这条上栽过一次
（把「拦住」改成「停岗」造出永久锁），所以这一问必须先答。

### 顺带：我那条「不留没有生产者也没有消费者的字段」是**有条件的**

判定不给 `CalibRecord` 留 `drift_stat` 时，理由是「加一个没有生产者也没有消费者的字段」。
**这一包把那个条件解掉了**——现在有生产者（`drift_of`）也有消费者（`cut`）。
**而我仍然没有加那个字段**，因为报告是**算出来的**（`算得出来的不许填`），
存一份只会多一处可以和事实不一致的地方。
**判定不做的理由消失时，要回头看那条判定——而不是让它继续挂着。**


## 四·二十六 `responses` 未命中不再静默；报文里那两份 JSON 现在能直接粘贴

### 一、`responses` 未命中

根在 `effects.rs`：`self.asks.get(&key).cloned().unwrap_or(None)`——**又一个 `unwrap_or`**。
「键不在表里」（你的 `responses` 没命中）与「键在表里、值是 `None`」（人还没答）
**压成了同一个 `None`**，于是在唯一的输出上**逐字段一模一样**。

**判据**（与 `tie` / `untested` 那次同形）：
**「人还没答」与「你的 `responses` 没命中」是两件事。**

**而「在等人答」是合法状态，不能把它也报成错**，所以分三档：

| 表里 | 含义 | 行为 |
|---|---|---|
| 有键、有答案 | 人答了 | 照给 |
| 有键、值 `None` | **真的在等人** | 静默挂起 |
| 没有键、**而表是空的** | **第一趟，本来就没人答** | 静默挂起 |
| 没有键、**表非空** | **给了 `responses` 却没命中** | **报错，并吐出两份 JSON** |

**第 4 问（合起来看）实测**：`library_lifecycle` 的 pending → `--resume` 整条路照常绿，
**「正常的等待」没有被报成错**。

### 二、那两份 JSON 以前粘不进去

作者实测照抄跑不起来，两处：
- **`on` 打成对象而夹具要列表**（`invalid type: map, expected a sequence`）
  → 新增 `State::as_fixture_json()`，`on` 恒为列表。
  与 `to_json()` 的差别只有这一处（送给模型时摊成对象更自然，**而夹具只收列表**）。
- **`op` 打成内核名 `noul` 而夹具只认 `test`**
  → 新增 `Op::fixture_name()`。**`phys()` 是物理形式，`fixture_name()` 是题式，两件事。**

**「报文说『照抄这两份 JSON』，而那句话是假的」——一句假的修法比没有修法更糟**，
它让人以为是自己没照做。

### 三、`iter_seq` 认的是「这一轮会不会变」，不是「写没写成字面量」

`seq_const` 原来只认 `ExprKind::Integer`，于是 `let s = 0;` 放循环外再传进去
**四种写法都撞不出告警**——**而那个键每轮一模一样，后果与写字面量 `0` 完全相同**。
**「常量」是一个语义性质，不是一个语法形状。**

**同时防住了反向的假拒绝**：`iter_params` 的含义改成「**这一轮里会变的名字**」，
块里 `let x = f(i)` 之后 `x` 也算进去——否则 `let 序号 = i + 1` 会被误报成常量。
**两个方向各有一条测试。**


## 四·二十七 `vectorize` 落地（第五条 pass），外加一条它逼出来的去重

### 三问先答

**一、「无 loop-carried 名字」怎么判——`map`/`filter` 由构造排除，所以只接这两个。**

我在 `iter_seq` 上做的那套答的是「**这一轮会不会变**」；登记表问的是
「**这一轮写的名字，下一轮读不读**」。**两个问题不同**，但在这门语言里第二个**塌掉了**：
**没有赋值，跨轮的唯一通道是显式累加器。** 而 `map`/`filter` 的体只收一个元素、没有累加器
——**条件由构造满足**。**`fold`/`loop` 有累加器，所以它们不在这条路上。**

`do`/`ask` 那一半不用另写：`speculate_expr` 只登记 `judge`，遇副作用即停。

**二、与 `speculate` 会不会踩——会，而且实测是「同一道题付两次钱」。**

接上之后实测 `[1,1,1]` → **`[2,2,1]`**：提前登记与真站点登记了同一个账本键，
`flush` 老老实实问了两遍。**这不是 `vectorize` 独有的——`speculate` 走同一条路，
只是以前没量到。**

**修在 `flush`：同一个账本键只问一次，而同键的每个读数都要填上答案**
（真站点那份与提前登记那份是两个 `Reading` 对象，只填一个另一个会停在「没有答案」）。
**这条去重本身就是一笔省**：`同状态·无 cut` 从 `[3]` 降到 `[1]`——
那三轮问的本来就是同一道题。

**三、形状：量了四个，只有一个有余量。**

| 形状 | 关 | 开 | 余量 |
|---|---|---|---|
| 异状态·无 `cut` | calls 3 / layers 1 | 3 / 1 | **无**——惰性已把三轮并进一层 |
| 同状态·无 `cut` | calls 1 / layers 1 | 1 / 1 | **无**——`fuse` 已合成一次 |
| **异状态·带 `cut`** | **3 / layers 3** | **3 / layers 1** | **层 3 → 1** |
| 同状态·带 `cut` | 1 / 1 | 1 / 1 | **无**——后两轮同键，账本重放 |

**够不够**：这两轴（状态同异 × 体内有无 `cut`）覆盖了 `map` 体的四种组合。
**没覆盖的是「体内有 `if`」**——那时 `judge` 在分支里，`speculate_expr` 走直线段遇分支即停，
**所以 `vectorize` 对它无效**，而这正是 `speculate` 的活。**两条 pass 的分工由此是清楚的。**

### **它没有「白花」那一栏**

与 `speculate` 不同：`map` 对**每个**元素都会调 `f`，**提前登记的站点没有一个是猜的**。
`speculate` 那一栏之所以存在，是因为分支只走一侧。
**所以 `vectorize` 默认开，而 `speculate` 的账必须两栏。**

### 一条旧消融臂被新 pass 污染了，而它当场红

`关掉融合就逐题发` 那条：`vectorize` 落地后关臂从 **6 变 10**——
提前登记的题在融合关着时各自一次调用，**差值里混进了别人的账**。
**「一个没关干净的消融臂，差值是假的」这次是反过来：新 pass 污染了旧消融臂。**
修法是量融合时把 `vectorize` 按住。**旧臂的断言当场红，这是它该有的样子。**


## 四·二十八 账本键对齐：**先别做，因为限制 2 的答案是「没同值，而且不是编码问题」**

总控的限制 2 写的是：**「`state_hash` / `q_hash` 自己就是分量——它们若不同值，
键怎么编码都对不上。先查清是不是已经同值。」**

**查清了：没同值，而且差的不是编码，是分量本身。**

| | Python | Rust |
|---|---|---|
| `state_hash` | `H(slots(), repr)`——**槽种类字符串 + 渲染文本** | `hash_of(["state", canon(to_json())])`——**槽内容的 JSON** |
| `q_hash` | `H(op, text, scale)`，`op` = **`"test"`** | `hash_of(["q", op.phys(), text, scale, evidence])`，`phys` = **`"noul"`**，**外加 `evidence`** |

**三处实质差异，都不是编码**：
1. **`q_hash` 的分量数不同**——Rust 多一个 `evidence`（J-09 的决定性证据槽）。
   **那是 Rust 这边后加的一维**，Python 没有。
2. **同一个位置放的是不同的字符串**——Python 放题式（`test`），Rust 放物理形式（`noul`）。
3. **`state_hash` 喂的东西根本不同**——Python 喂「槽结构 + 渲染出来的文本」，
   Rust 喂「槽内容的规范 JSON」。**换一种渲染，Python 的键变、Rust 的不变。**

**所以把 `hash_of` 对齐成 `canon + 前 16` 不会让键对上**，只会**让外层长得一样而里面仍然不同**
——**那比现在更糟：现在对不上是显然的，对齐之后对不上是隐蔽的。**

**判定：这一包不做对齐。** 要做的是先定「两内核对『什么是同一次观察』要不要有同一个答案」
——**那是语义裁定，不是编码活**。总控量的三个迁移面数字（证书地址不受影响、
Rust 写死 24 位键 1 处、Python 写死 16 位 0 处）**我复量过，都对**
（那 1 处就是我上一包钉的 `cross_kernel.rs:41`）。**迁移面确实很小，
而这一包的阻塞不在迁移面上。**

### 顺带核出来的一条，比对齐本身更值

**`profile_hash` 对上了而账本键没对上，不只是「有人写过要求」——**
**`profile_hash` 两边喂的是同一个对象（那份档案 JSON），而键喂的是各自内部算出来的中间量。**
**一个比的是外部输入，一个比的是内部表示。** 外部输入天然同源，内部表示天然各自演化——
**而「都叫 hash、都进账本头」把这个区别盖住了。**

## 四·二十九 `Cert::addr()` 的两颗雷（已拆，$0）

**一、`{:.4}` 让第五位小数不同的两张证书同址**，而 `certs` 是 `BTreeMap<addr, Cert>`
——**后写的静默覆盖先写的**，正是这套寻址要消除的那件事。
**今天不可达**：α 的实际取值全是调用方写的字面量（实测全集 0.01 / 0.10 / 0.35 / 0.40 /
0.45 / 0.60 / 0.80，没有任何一处把算出来的数传进 α）。
**但 α 是公开 API 的参数，「调用方传一个算出来的 α」是完全正常的事**——改用 `{:?}`（f64 往返精度）。

**二、地址里嵌了自由文本**（`cluster_unit`、以及 `LabelSource::选择子集` 的 `判据: String`）
**——文本里含 `\u{1f}` 就撞地址。** 同样今天不可达（判据现值是中文散文），
同样是外部输入。**改成先把那两段哈成定长，分隔符就再也不可能出现在段内。**

**两条都是「按参数取值决定生死」的雷**——**一行换掉，比把条件写进注释便宜。**


## 四·三十 跨内核出口对照：照出 `cut` 整条 **δ 带缺失**（$0）

**做法**：**不跑端到端真机。** 出口不是纯函数（依赖读数），**但「给定读数 → 出口」是纯函数**
——所以扫一整条 `p` 网格（101 格），**比一次真机采样彻底得多，而且没有抖动要排除**。
用的正是那条已记的规律：**「一个可以用纯函数比的东西，不要用端到端跑来比。」**

**金标向量 `tests/py_exits.json`** 由 Python `runtime.py::_decide` 的 `test` 臂当场跑出，
签进仓库，**两边都读它**——**一句注释跨不了两种语言，一个向量文件可以。**

### 照出来的：`cut` 的 ±δ 带整条缺失

`12`:167 写着「再过线，再 `band`（**线附近 ±δ**）」。
Python：`p >= hi + δ → Act`、`p <= lo - δ → Ignore`。
**Rust 原来是裸的 `p >= hi` / `p <= lo`**——**δ 一次都没用上**
（`delta_for` 在 `order` 与 `strength::uncertainty` 里用，**唯独 `cut` 没用**）。

**后果是失败开放，而且在最核心的那个判定上**：线的邻域里本该 `Unsure` 的读数拿到了强出口。
**消融实测：去掉 δ，101 格里 9 格分岔。**

**E-JPP-LIVE 那次真机读数正好落在分岔格里**：`p = 0.56`、`lo = 0.56`、`δ = 0.04`
——**修前 Rust 给 `Ignore`，Python 给 `Unsure(band)`。那一条花过钱的实测数据本身就分岔，
而当时没人看出来**，因为当时没有东西把两边放在一起看。

### 这条为什么以前没被发现

**它在固定观察下完全看不见**：替身给什么 p，我就断言什么出口——**断言是照着实现写的**。
**要照出它，必须有一个独立于实现的第二个来源**，而那个来源就是 Python。
**「留着 Python 当行为对照」这件事，到今天第一次真的兑现了一次。**

### 跨内核对照的正确顺序（实测得出，与原设计相反）

1. **纯函数比**（本节）：$0，覆盖全网格，**照出了一条真 bug**。
2. **账本键比**：$0，**结论是「分量都不同，先别对齐」**（四·二十八）。
3. **端到端真机跨内核**：**照 1、2 的结果，它的边际信息量很低**——
   身份比不了（键分量不同），出口已经逐格比过了。


## 四·三十一 保形那一族：用**定义与保证**当独立来源（$0）

**背景**：`binomial_upper` / `certify` / `drift` / `n_needed_zero_error` /
`cluster_subsample` **Python 一个都没有**——**它们的断言至今全是照着实现写的，
而差分法救不了它们**。**而这一族正是「长处」那一侧。**

**所以改用定义与保证**（都 $0，都不需要 Python）：

| 函数 | 独立来源 | 结果 |
|---|---|---|
| `binomial_upper` | **按定义反查**：返回的 `p*` 要满足 `P(X ≤ k\|n,p*) = δ`，**CDF 用阶乘直算**（与生产代码的递推是两种写法） | **20 组全部满足** ✓ |
| `n_needed_zero_error` | **闭式手算**：`⌈ln δ / ln(1−α)⌉` | 22 / 59 / 1 全对 ✓ |
| `certify` | **蒙特卡洛覆盖率**——保形唯一要保证的东西 | **300 次重复，认证成功 297 次，真实错误率超过 `ucb` 的 24 次 = 0.081 ≤ δ=0.10** ✓ |
| `cluster_subsample` | 性质检验 | 5 簇各取 1、同种子可复现、**50 个种子采出 50 种组合** ✓ |
| `drift` | 性质检验 | **照出一条真的**，见下 |

**`certify` 那条值得单说**：它扫阈值取第一个 `ucb ≤ α`，**那是一次多重比较，
而 Clopper–Pearson 是按单次算的**——**所以覆盖率完全可能不成立**。
**实测它成立**（0.081 ≤ 0.10）。**判据是我定的**：300 次重复、放到 2δ，
**因为 300 次本身有 ±1.7% 噪声，我要区分「略超」与「结构性超」**。

### `drift` 照出来的：`underpowered` 把两件事压成了一位

**原来 `underpowered = ks < crit`**——而 **200 条对 200 条的同分布数据 `ks = 0 < crit`，
于是它报「功效不足」，而真相是功效充足、没有漂移。**
**「测不出来」与「测出来没有漂」被压成了同一位**——今晚这个形状的第 N 次。

**拆成两位**：
- **`significant`** = `ks >= crit`（**这个 KS 显著吗**）
- **`underpowered`** = `crit > 1.0`（**样本小到连完全分离都不显著**——`ks` 上限是 1，
  所以这才是真的功效不足；实测要 `n ≤ 3`）
- `可停岗()` = `significant && !underpowered`

### 一处**我的检验设计错了，不是实现错**

我第一版断言「4v4 时必须亮 `underpowered`」——**而 4v4 完全分离的 `KS = 1 > crit = 0.962`，
那是统计上正确的显著**（精确检验 `p = 2/C(8,4) = 0.029`）。
**总控那条「分岔了先报不自裁，先确认是实现错还是检验设计错」当场生效了一次，而答案是后者。**

## 四·三十二 「声称存在 vs 从外面调用过」的清单

`jpp-core` 的 `pub fn` **共 134 个**：

| | 个数 |
|---|---|
| **被 `jpp-cli` / `jpp-frontend` 真正调用过** | **28** |
| 只在测试里出现 | **72** |
| **连测试里都没有** | **34** |

**第二、三栏合起来 106 个，就是我们所有「已落地」声明的诚实边界。**
**`Profile::load` 那次（写好了、零调用点、测试全绿）不是个例，是这 106 个里的一个。**

**这份清单的用法不是「去把它们都接上」**——很多是内部工具（`env_child` / `binary` / `encode`）。
**用法是：任何一条「X 已落地」的声明，先查 X 在不在第一栏。**


## 四·三十三 语言表面 vs 宿主侧：**校准与保形族没有语言表面**（核准 + 两处更正）

### 总控的核心判断：**属实，而且比他说的更宽**

`BUILTINS`（`interp.rs:323`）**共 53 个**。以下**一个都不在里面**：

`commission` / `put` / `absorb` / `certify` / `drift` / `binomial_upper` / `cost_line` /
`cluster_subsample` / `profile`

> **整个校准与保形族没有语言表面。它在宿主侧是完整的、测过的、覆盖率刚验过——
> 而在这门语言里它不存在。**

### 两处更正（总控说「我错了就直接说，别顺着我」）

**一、`speculate` / `vectorize` / `lift` / `fuse` / `plan` / `ledger` 也不在 `BUILTINS`。**
总控把它们列进了「语言表面」，**而它们是 `Passes` 的开关名**（`interp.rs:288` 的 `enabled`），
**由 Rust 侧设在 `Interp` 上，`.jpp` 一样碰不到**。
**所以不可达的范围比他说的更宽**——**更正的方向对他不利，不是替他找补。**

**二、「五个示例只用到五个内建」不对，实测是 21 个。**
逐个 `grep '\b<名>\s*('` 五份 `examples/*.jpp`：
`map 11 / len 6 / filter 6 / test 5 / fold 4 / content 4 / append 4 / text 3 / do 3 /
contains 3 / concat 3 / state 2 / pending 2 / mat 2 / judge 2 / handle 2 / cut 2 /
transform 1 / stop 1 / loop 1`（+1 个低频）。
**总控报的 `judge 10 / do 9` 与我数的 `judge 2 / do 3` 差很多**——他大概把统计范围算宽了。
**结论方向不变**（21/53 仍是不到一半），**但那个「五个」不能进结论。**

### 没被任何示例用到的 32 个

`select` `measure` `consume` `ask` `unsure` `fail` `is_fail` `unsure_cause` `untested`
`line_source` `taint` `escalate` `literalize` `allocate` `unsure_bound` `agg` `order` `fit`
`range` `slice` `sum` `reverse` `keys` `with` `has` `join` `print` `min` `max` `abs` `floor` `exit_kind`

**总控问的是「哪几个是写不出示例来，而不是还没写」。答案：**

- **写得出、只是没写**：后半那些数据内建（`range`/`slice`/`sum`/…）、`select`/`measure`/
  `consume`/`unsure_cause`/`untested`/`line_source`/`taint`/`exit_kind`——**都是纯语言，
  写三行就能示范。**
- **写得出但示例会「假"**：`ask`/`escalate`/`literalize` 要人答，示例得配 `responses` 夹具
  ——**能写，而且 `lifecycle.jpp` 已经走了 `ask` 那条**（它用的是 `handle` 不是裸 `ask`）。
- **真正写不出的只有两个**：**`allocate` 与 `unsure_bound`**。
  - `allocate` 今天在真实路径上**排不出任何东西**——除非 `--profile`/`--calib` 带线进来，
    而那是本轮刚接的；**示例目录里没有夹具能把一条上岗线喂进去**（夹具的
    `calibrations` 能，但那是夹具不是示例）。
  - `unsure_bound` 要 `unsure_rate`，**而那个字段只能从 Rust 侧写**——
    **示例写出来只会得到 `union_bound == n` 的常量**（第二个作者实测过）。

> **「写不出示例」与「没写示例」的分界线，正好落在「这个内建要不要宿主先喂东西」上。**
> **而要宿主先喂的那两个，恰好就是「长处那一侧」的两个。**

### 三问（只答，不动手）

**一、该以什么形式进语言表面？——不是内建函数。**

**理由是 I4/J-03**：「**线只从校准记录来，程序里不可写线**」。
把 `commission`/`put` 做成内建，**等于把写线的能力交给程序**——**那正是 J-03 禁的**。

**正确形式是「入料/出料」那条路，不是「调用」那条路**：
程序**声明**它用哪个校准键（今天已经有：`test(题面, "键")`），
**而线与证书由宿主在运行前后进出**。**`--calib` 就是入料那一半的正确形状。**

**二、最小的那条路：只缺「出料」一个入口，不缺内建。**

端到端「判过的读数 → 上线 → 拿到带证书的出口」，逐段看：
- **判 → 读数**：`judge` ✓
- **读数 → 证据**：`Outcome.evidence` ✓（本轮已有，运行期写入口）
- **证据 → 上线**：`absorb` + `commission` —— **宿主侧 ✓，而 CLI 没有出口**
- **线 → 出口**：`cut` ✓，`line_source(e)` 能读出「证书:α=…」✓

**所以不可省的是一个 CLI 出口**（把 `Outcome.evidence` 写回 `--calib` 目录），
**不是一个新内建**。`commission` 仍留宿主侧——**因为认证是校准过程，不是程序行为。**

**三、与 `--calib` 怎么接：入料有了，出料没有。**

`--calib` 读目录 ✓。**写回去那一半不存在**——`Outcome.evidence` 今天只能被 Rust 调用方拿到。
**最小改动是一个 `--calib-out <dir>`**：跑完把 `evidence` 折进记录并落盘。
**那是一行接线，和上次那三行同一性质**，**而且它让「跑程序 → 积累证据 → 认证 → 用上」
这条环第一次闭合**。


## 四·三十四 `--calib-out` 的产物读不回去（修）+ 那条环到底解锁了什么

### 接缝零覆盖

`--calib-out` 写出来的记录带 `label_fp` / `label_locator`，**而 `--calib` 的装载器拒收**。
**写的和读的对不上，而两边各自都有测试。**

**根因**：装载器的「认得的字段」表是**手写的列举**
——**我给 `calib_hash` 选了排除法、给这里留了列举法，而它在几小时内就漂了。**

**改法不是把两个名字补上**（那只是重置了漂移的计时器）：
**改成从结构体自己问**——序列化一条记录，它的键就是认得的全集。
**「算得出来的不许填」这条我今晚用了三次，唯独在这里没用。**

### 我的验收为什么照不出来

`wiring.rs` 那条 `校准环闭合` 在两趟之间**插了一行手写 JSON**模拟宿主认证。
**于是第二趟读的是我手写的那份，不是 `--calib-out` 产出的那份**
——**「写」有覆盖、「读」有覆盖，而接缝零覆盖。**

**模拟宿主认证本身没错**（线确实该由校准过程给），
**错的是它同时把「出料的产物能不能被入料读回去」这个断言一起抹掉了，
而那正是那一包唯一新增的东西。**

> **判别法（总控给的，很具体）：一个「写出来 → 读回去」的验收，
> 中间不许有任何一行去改那个文件。**

新增两条：`calib_load.rs::save写出来的load读得回去`（**填满每一个字段**——
只填一半的记录，往返测不出漏掉的那一半）、
`wiring.rs::calib_out的产物能原样喂回calib`（**CLI 层，中间一个字节不改，且累积一轮之后再验一次**）。
**原来那条 `校准环闭合` 保留**——它验的是另一件事。

### 那条环解锁了什么（总控要的全查，实测）

同一组程序，**只差有没有 `--calib`**：

| 内建 | 无线 | 有线 |
|---|---|---|
| `allocate` | `{picked: [], 算不出: [0,1,2]}` | **`{picked: [1,2], 算不出: []}`** |
| `unsure_bound` | `union_bound 2.0 = n`、`n_unknown 2` | **`0.2`、`n_unknown 0`** |
| `cut` | **程序跑不完**（`Unsure(cold)` 未消费 → J-05） | `act` |
| `line_source` | **程序跑不完** | `题级·手填` |
| `untested` | `calib_line` | **`""`** |
| `agg` | `cold` | **`band`** |
| `order` | `[[0],[1]]` | **同**——它不看线 |

**七个里六个不同。** 而 `order` 相同是对的：它按 δ 分档，**不读线**。

**一处我差点报错的**：`untested` 与 `agg` 第一版探针两边都空——
**而「两边都空」是「我的探针没跑起来」，不是「行为相同」。**
我差一点把它们记成「同」。**修好探针后，两个都是「异」。**
**「两个空值相等」是今晚那条恒真断言的又一张脸。**

## 规则批（2026-09-23，B 栏裁定落代码）

### B29 夹具线与代价参数

- `CalibStore::put` 只写夹具记录（`CalibRecord.fixture = true`，为 `false` 时不序列化）。认证程序（`commission*`、`calib-import`）上岗时清掉这一位。
- 夹具线 = `fixture` 位为真，或记录没有一张证书（`CalibRecord::fixture_line()`）。凭夹具线得到的出口 `Exit.fixture_line` 为真；强出口时报 `W-fixture-line`（取代原 `W-uncertified`）。
- J-08：夹具线出口不算放行不可逆 `do` 的可信合取项（`Exit::guard_trusted()`）。
- `cut(r, {cost: [fp, fn]})`、`cut(r, key, {cost: [fp, fn]})`：只取该记录（先题级、后题式级）上按这个代价矩阵认证的证书的线；没有 → `Unsure(cold)`，J-15 载体 `cost_line`。线仍只来自记录。
- 测试若要走 J-08 放行路径，用 `tests/common::certified` 附一张明写为「测试合成证书」的证书；语言与 CLI 没有这条路。

### B25 停岗候选

- 记录状态加「停岗候选」（`STATUSES` 五个）。候选线照常用于路由，出口 `Exit.suspend_candidate` 为真，强出口报 `W-suspend-candidate`，J-08 不算可信合取项。
- 自动标记：`cut` 等消费方调用漂移检查（`报漂移`）；`drift_of(key).可停岗()` 成立时，本趟起该键即为候选（`W-drift` 写明），`Outcome.suspend_candidates` 列出。内核不改记录。
- CLI：`jpp run … --calib-out <目录>` 把候选键（原上岗）写成「停岗候选」；`jpp calib-confirm <目录> <键> --suspend | --keep` 由人确认，只处理候选状态。

### B28 重复读数合并 `repeat`

- `repeat(读数列表[, "mean" | "median"])`（原 `agg`，旧名一个版本内可用并报 `W-deprecated`）：同题同状态重复读数逐分量取均值或中位数，choice 与 score 也按概率向量逐分量合并；`"mode"`（众数、投票）报错。
- 合并结果仍是读数，校准键为 `原键·repeat(n=…)`，不借题式或模式线，没有该键的认证记录就是冷；账本写一条 `Effect{kind:"repeat"}` 记 n 与方式，trace 记 `repeat` 事件。
- `CalibRecord.rerun_independent`（可选）：重跑分歧检验是否通过。未通过或未测时，`repeat` 报 `W-repeat-persistent`（重复只压抖动，不降错，B9）。库里目前没有 `band → 重跑` 处理器；以后加时以这一位为启用条件。

### B32 判断力缺席与时延

- `budget {absent: {retry, backoff, then, breaker}, latency_p95: 秒, unsure: …}`：前端接上三格（`unsure` 以前写不出）。`absent` 缺省 `retry 2, backoff 1, then "escalate", breaker 3`。
- 判断器调用失败：按 `retry` 次重试，等待 `backoff` 秒起每次翻倍；用尽后：`escalate` → 程序挂起（Pending cause=absent，恢复后 `--resume`）；`conservative` → 该组题出口 `Unsure(absent)`，程序继续；`fail` → 运行期错误。连续缺席达 `breaker` 次后熔断，不再发出。**未声明 `absent` 时沿用旧行为**（客户端错误即运行期错误）。
- `latency_p95`：本趟判断调用累计耗时上限。某次调用后超出 → 本组题 `Unsure(latency)`；之后的站点不再发，直接 `Unsure(latency)`。
- 缺席与超时事件记账（`Effect{kind:"absent"}`，键 `absent:<题键>`），只凭账本重放时照记的原因给出，不再发。
- 检查：`budget.latency_p95` 与档案 `concurrency.latency_s.p95` 比，一层的 p95 已超 → `E-latency`；无循环时「站点数 × p95」超 → `E-latency`；有循环里的站点只能给下界 → `W-latency`；无档案 → `W-untested`。

### B3 未决原因 `rejected_all` 与 `no_candidate`

- `first_k`：没有候选（输入为空）→ `Unsure(no_candidate)`；有候选、全部观察完、一个都没接受 → `Unsure(rejected_all)`；接受了一些但不足 k 个 → `Ignore`（确定不足）。
- `select` 的 `over` 为空：不发调用，出口 `Unsure(no_candidate)`，报 `W-no-candidate`。
- 去向示例在 `lib/handlers.jpp`：`retry_with(材料, 题)`（全被否决 → 换材料或换前提）、`generate_then_find(提示, n, 题)`（没有候选 → 调生成器）。演示：`examples/unsure-causes.jpp`（夹具 `fixtures/unsure-causes.json`）。

## B33 值级 taint（2026-09-23）

宿主标量 `Value::Int / Float / Bool / Text` 各带一位 `Taint`（`Value::Int(i, t)` 等；构造可信值用 `Value::int / float / bool / text`）。容器不带位，`Value::taint()` 递归取 ∨；`Value::tainted(t)` 把 `t` ∨ 进所有标量叶子。

- **读出规则**：`content(m)`、`m.content`、`text(m)` 从 untrusted 材料读出的值，所有叶子标 untrusted；不可信 `do` 的 `Fail` 带动作输出位，`fail(reason)` 带理由文字的位。
- **宿主内传播**：一元、二元运算与内置输出 = ∨ 输入，在 `binop` 与 `apply` 的内置分派处统一计算。例外表 `不做数据流合取的内置`：效应边界与自带规则的内置（按 §2.11 表赋值），以及只搬运元素的 `map / filter / fold / loop / append / concat / slice / reverse / with`（元素保留自身的位）；列表 `+` 同理。不在表上的内置一律 ∨。取字段、下标返回叶子自身的位；用户函数返回值自带位。**控制流不传播**。
- **进材料**：`mat(v)` 等计算值进槽时 taint = `v.taint()`；成分不可信时 origin 记 `computed`（可信时仍记 `literal`，材料哈希不含 origin）。旧的旁路表 `unwrapped_untrusted` / `untrusted_texts` 与四个辅助函数已删除。
- `do` 的 `taint_in` 取 `taint_of`（即 `Value::taint()`），裸值经 `inherit` 动作不再洗白。
- J-08 报错：守卫出口所在状态含成分不可信的计算值材料时，追加「该材料由计算值构成，成分含不可信内容」（按当前各帧产生过的出口近似判定）。
- 账本格式不变；宿主值不进账本。回归：`crates/jpp-core/tests/value_taint.rs`（探针 A–E2 与控制流、取字段、inherit、J-08 诊断）。

## B84 值级来源（步 17c，2026-09-24）

B33 的那一位推广为来源标签 `Provenance { taint, sources }`（`jpp_value::prov`）：`sources` 是直接来源读数的账本键集合。标量第二字段改为 `Provenance`（构造可信值仍用 `Value::int / float / bool / text`；手写时 `Taint::Trusted.into()`）。`Value::prov()` 递归合并，`Value::taint()` 是它的 taint 分量、结果与 B33 逐值相同；`Value::with_prov(p)` 是 `tainted` 的推广（taint 分量只进标量叶子；sources 分量进标量叶子与材料、题的 `from_key`）。合并只在 `prov::join`（taint ∨、sources ∪）。

- 与 B33 同一张边界表：内置分派、二元与一元运算 join 输入；`content()`/`m.content`/`text(m)` 读出带材料的标签；`mat(v)` 计算值的 `from_key` = `v` 的 sources；`do`/`gen`/`transform` 输出承接输入；`sieve` 元素的 `item` 带选中它的出口键；`pick`/`at` 臂的 k 带（出口 taint，{出口键}）。
- **唯一不同的一行**：按计算下标或字段取值（`xs[k]`、`r[name]`），sources 并入键的 sources，taint 仍取元素自身的位。控制流不传播。
- `test`/`select`/`measure`/`fill` 以计算出的文本或填入值造题时，题的 `from_key` 并入其 sources（`fill` 为裁定原文；前三者为步 17c 的解释登记）；`literalize` 的题带未决出口键。
- `Mat`/`State`/`Question` 分存 `taint` 与 `from_key`/`parents` 两个字段（报告 JSON 不变），传播只经 `prov()`/`with_prov()`。`derived_from` 本步保持现行传播（J-02 不变），是 sources 题哈希投影的子集；投影化在步 18。
- 账本 `Judge.parents`/`hop` 由此按值级来源计（`iterate` 三层 hop 1/2/3）。账本编码只写 taint 分量，格式不变。回归：`crates/jpp-core/tests/bypass_b84_provenance.rs`。

## B76 题类（运行时半，步 12e-2，2026-09-24）

- `form(op, 模板, {…, over_kind: "labels" | "candidates" | "questions" | "actions"})`：题式对 `over` 的声明，经 `fill` 带到题（`Form.over_kind`、`Question.over_kind`）；不进 `form_hash`、`q_hash`，空不序列化；非法值报 `E-rt-question`。
- `Question::kind()`：基础类（不看状态）；`Question::kind_on(&SlotShape)`：给定槽形时的精化类；`State::slot_shape()`：`on` 单对象 / 一对，`over` 空 / 字面标签 / 计算材料。推断函数只在 `jpp-ir::question_kind`。
- 运行时在登记读数时按「题 × 状态槽形」记精化类（按读数句柄，`21` 写作 `ReadingMeta.kind`）。报告、账本不打印题类。校准记录的 `kind` 分类字段随步 20a（校准格式步）落地。
