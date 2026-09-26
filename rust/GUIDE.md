# J++ 新开发者一页纸

一页够用的入门；完整接口在 `crates/jpp/INTERFACE.md`（现行接口与施工日志混排，
按小节标题找），语法在 `FRONTEND.md`，观察/重放的可运行教程在 `METHODS-AND-LIFECYCLE.md`。
本文全部命令已在本仓库实跑验证（2026-09-24）。

## 装、跑

```sh
cargo build --workspace          # 需要支持 edition 2024 的 Rust 工具链
cargo test --workspace           # 含金样与重放对照
cargo run -p jpp -- parse examples/composition.jpp --ast
cargo run -p jpp -- check examples/composition.jpp
cargo run -p jpp -- run examples/composition.jpp
cargo run -p jpp -- run examples/adaptive.jpp \
  --fixtures examples/fixtures/adaptive.json --output report.json
```

`run` 默认吃固定观察（fixture），不发真实模型请求；不带 `--fixtures` 时只能跑不含
判断效应的程序（如 `composition.jpp`）。`--output` 落报告 JSON，`--ledger-out` 另落账本，
`--replay <ledger.json>` 用账本重放（拒绝任何新调用）、`--resume <ledger.json>` 续跑。

脱离本仓库单独安装：

```sh
cargo install --locked --path crates/jpp --root /tmp/jpp-native
/tmp/jpp-native/bin/jpp run examples/composition.jpp
```

装出来的可执行文件不需要 Python 或 Cargo；固定观察运行不需要账号、密钥或网络。

## 夹具（fixture）格式

一份 JSON，键为 `description`（自由文本）、`calibrations`（该次运行要用到的校准记录，
测试用途可手写，正式线不许手写，见下）、`observations`（材料 × 题 → 读数的穷举表，
缺一条就是错误而不是瞎猜）。

```json
{
 "description": "……",
 "calibrations": [
   {"key": "cs-refund", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗", "delta": 0.05}
 ],
 "observations": [
   {"on": ["材料文本或状态记录"], "op": "test", "text": "题面文字",
    "calib": "cs-refund", "answer": {"Noul": 0.92}},
   {"on": ["材料"], "over": ["候选一", "候选二", "候选三"], "op": "select",
    "text": "……", "calib": "……", "answer": {"Choice": [0.9, 0.07, 0.03]}},
   {"on": [{"a": "…", "b": "…"}], "op": "measure", "scale": ["档位一", "档位二", "档位三"],
    "text": "……", "calib": "……", "answer": {"Score": [0.02, 0.08, 0.9]}}
 ]
}
```

`op` 对应三种题型：`test`（是非，`answer.Noul` 是一个概率）、`select`（K 选一，`over`
给候选，`answer.Choice` 是逐候选概率向量）、`measure`（打分，`scale` 给有序档位，
`answer.Score` 是逐档概率向量）。`on` 是材料（是非/打分题一个元素；关系题一对，如
`{"a":…, "b":…}`）。找不到公开示例时抄 `probes/winnow/make_fixture.py`（是非题）、
`probes/folio/fixture.json`（select）、`probes/entity-align/fixture.json`（measure）。

**`calibrations` 里的 `delta` 不能省。** 校准记录没有 `delta` 字段时，J-15 按「这条判据没测过」处理，
出口一律 `unsure(untested:Delta)`，不会到 `act`/`ignore`（δ 只能从校准记录取，不能由内核猜，B104、
步 15d-2）。手写测试夹具时把 `delta` 当必填字段，不要只抄 `hi`/`lo`/`n`/`status` 四项。

## 元素字段：`index` / `pos` / `trail`（步 25-0，B81/B82）

`sieve` 的输出元素保留输入元素的全部字段，再补或更新 `{item, pos, trail, exit, cause, q, qi, key}`
（填法形式另带 `fill`）；`pair` 的产物是 `{item: {a, b}, trail, left, right, at, pos}`。

- **`index`** 是原始编号：只在输入不是元素记录时赋为输入位置，此后经任何过滤、配对不变。
  链式过滤时第二层元素的 `index` 仍是最初列表里的位置，直接用 `e.index`。配对产物没有 `index`。
- **`pos`** 是该元素在**这一次调用的输入列表**里的位置（`first_k` 按它排）。
- **`trail`** 是接上来的历次出口列表。配对的 `left` / `right` / `at` 经过滤后仍在元素上（`e.left`），
  原来的 `source` 字段已撤。
- **未决条目就是元素本身**（带 `exit` 与 `cause`），`p.index`、`p.left` 直接读；整体返回
  `undecided(o)` 与 `unobserved(o)` 即转交责任。只返回投影（去掉 `exit` 的记录）不算转交，运行期报 J-05。
- **多题与多填法**：`sieve(items, [q1, …])`、`sieve(items, form, [fill1, …])` 返回**一个**契约值，
  元素 = 材料 × 题（材料主序、题次序），元素带 `q`、`qi`（填法另带整条 `fill` 记录，槽以外的键也在）；
  `by_q(o, k)`（`lib/outcome.jpp`，别名 `by_fill`）取第 k 道题的子契约值。
- 程序构造的契约值 `outcome({…})` 不收 `spent`，花费由运行时按 `evidence` 的账本键算（A-2）。

## 内置函数名单

以 `crates/jpp-runtime/src/lib.rs::BUILTINS` 为准（`env` 里同名字会被用户绑定
遮蔽，静态检查报 `W-shadow`，允许但会提示）。

| 类别 | 名字 |
|---|---|
| 材料与状态 | `mat` `content` `state` `transform` |
| 出题 | `test`（是非） `select`（K 选一） `measure`（打分） `form`（题式模板） `fill`（按题式填题） |
| 判断与过桥 | `judge` `cut` `ask` `key_of` |
| 出口处理 | `handle` `consume` `exit_kind` `unsure` `pending` `line_source` `taint` |
| 未决责任 | `unsure_cause` `untested` `escalate` `literalize` |
| 效应与失败 | `do` `gen` `fail` `is_fail` |
| 三路过滤/配对/聚合/迭代 | `sieve` `pair` `tally` `first_k` `iterate` `outcome` |
| 有界控制 | `loop` `stop` `map` `filter` `fold` |
| 读数长处（只读校准线，不花钱、不进账本） | `allocate` `unsure_bound` `repeat`（同题重复读数取均值/中位数，旧名 `agg` 一个版本内仍可用并报 `W-deprecated`） `order`（同尺度排序） `fit`（喂已注册特征） |
| 数据与文本 | `len` `range` `append` `concat` `slice` `contains` `sum` `reverse` `keys` `has` `with` `text` `join` `print` `min` `max` `abs` `floor` |
| 文本与数据（确定性，B157）| `split` `lower` `upper` `trim` `replace` `starts_with` `ends_with` `index_of` `chars` `regex_match` `regex_find` `sort` `sort_by` `parse_json` `to_json` `hash` `date_parse` `date_format` `date_add` |
| 带种子伪随机（B158）| `rand(seed, k)` `rand_int(seed, k, n)` `shuffle(list, seed)` |

`accepted` / `ignored` / `undecided` / `unobserved` / `stopped` **不是内置**，是
`lib/outcome.jpp` 里按契约值字段取值的库函数（`import "lib/outcome.jpp";` 后可用），
契约值形状见 `INTERFACE.md` §三·四·四。

## 确定性处理在 `.jpp` 里写

文本切分、正则、排序、JSON 解析/序列化、哈希、日期解析/格式化这类**纯确定性**处理直接
在程序里写（上表「文本与数据」行），不必再交给宿主或用 `fold` 逐字符拼：它们不记账、
不是刷新点，taint 按「结果携带全部输入 taint 的并集」统一算——任一输入不可信，结果就不可信。

`parse_json`/`date_parse` 解析失败返回 `Fail`（`is_fail(v)` 判），不是运行期错误：

```jpp
let v = parse_json(text);
if is_fail(v) { "格式不对" } else { v.field }
```

**`now` 不是内置，无种子随机也不提供**（B157 (2)）：不确定性只经端口进入，
两次运行同样的程序不能因为读了当前时间或随机数就走出不同的控制流，账本上看不出为什么。
当前时间的两种取法：

- 宿主经 `--input` 给一个时间戳字段（进 `entry_hash`，账本能看到用的是哪个时间）；
- 登记动作 `do("clock:now")`（可逆、成本 0、`taint_out: trusted`，同样进账本，重放拿到同一个值）。

要「每次不同」的随机数，把种子作入口交进来（同样进 `entry_hash`）；`rand`/`rand_int`/`shuffle`
本身对同一个 `(seed, k)` 恒给同一个值（splitmix64，算法与版本号写死，参见测试），
不是「看起来随机」，是「给定种子就是确定的可重放计算」。

## 环境循环三种写法

游戏、交互类程序的「环境」（棋盘、蛇身、agent 位置……）不需要语言加新构造（B159）：

1. **纯函数环境**：环境是一个值（列表/记录），转移函数 `step(env, action) → env'` 是纯
   `.jpp` 函数，配合 `iterate(bound, init, step, measure)` 或 `loop` 跑循环；每轮的判断以
   渲染后的环境为材料（`mat(render(env))`）。小环境、不依赖外部模拟器时用这条——
   参见 `examples/env-snake.jpp`。
2. **宿主动作**：环境状态留在宿主，登记一个动作 `env:step(env_json, action) → env_json'`
   （`reversible: true`、成本 0），结果进账本、重放不重算；物理引擎、第三方模拟器这类
   宿主已有的东西走这条，不必在 `.jpp` 里重写一遍它的转移规则。
3. **入口序列**：环境转移由 `--input` 里逐步给出的观测序列决定，程序只做每步的判断与
   动作选择，宿主按拍调用程序——不需要程序自己知道「下一步会怎样」。

实时时钟、渲染、物理都在宿主：宿主每拍调一次程序（同一账本 `--resume`，或每拍一份账本），
程序里没有时间（见上一节）。

## 线从哪里来

`cut` 把读数（概率）变成出口，要一条线。线有四个来路，按作者要花的功夫从少到多：

1. **题库**：`bank/bank.json` 里已认证的同题型题式，`fill(题式, {…})` 填上就用，作者一条不标。
2. **真值可算**：标签由程序算出（`source: computed`），经 `calib-import` 导入认证，零人工。
3. **标注或代标**：带真值样本经 `calib-import` 认证（可由强模型代标加人工复核）。前三种是**认证线**：语言对它担保错误率，出口可以放行不可逆动作。
4. **作者声明线**：作者自己写数，`cut(r, {declare: {hi: 0.7, lo: 0.3}})`。按写的数切（读数 ≥ hi 为 act，≤ lo 为 ignore，其间 `unsure(band)`），不平移、不被同键记录替换，也不作错误率保证。等级 `Declared`，路由照常，报告 `exits` 行给出同键标注里按这条线切错几条、本趟有多少读数落在线 ±0.2 内。

`cut` 的第二、三位还有三种写法：`{cost: [fp, fn]}` 按两种错的代价在已有证书里选线，`{alpha: a}` 在 α ≤ a 的证书里选已决最多的一张，二者的线都来自记录。声明线另有两个选项：`stat` 指定线切在读数的哪个统计量上（`"expect"` 期望档位、`{mass: [0, 1]}` 几个候选的概率和、`"confidence"` 判断器自报的置信度；`expect` 还可写 `{cuts: [0.5, 1.5, 2.5]}` 分桶出 `at`），`closed: {lo: false}` 让下端变开（读数等于 lo 归中间带，复刻 `elif p >= lo`）。

**声明线放行不可逆动作要宿主接受。** 不带开关时，声明线的出口能路由、不能单独放行不可逆 `do`：`check` 在守卫全来自声明线时报 J-08，运行期同样拒绝，报文写出开关名。宿主确认这些线由自己担责后带 `--release-on-declared`（`run` 与 `check` 都收；库宿主用 `EntryArgs.accept.declared_lines`）：出口的 `releases` 变为真，报告多一个 `accept: {declared_lines: true}`，开关进账本头 `entry_hash`，换开关状态重放报 `W-header: entry_hash`。这个开关的意思是「这些线由我担责」，不是「这些线是对的」；它也不改材料的可信与否，不可信材料上的判断带了开关照样不放行。

推荐写法是两侧线加 unsure 臂转人工，读数落在两线之间的由人拍板：

```jpp
handle(cut(judge(state(m), refund), {declare: {hi: 0.7, lo: 0.3}}), {
    act: fn() { 退款(chat) },
    ignore: fn() { "不退" },
    unsure: fn(u) { consume(u, "drop"); handle(ask(state(m), refund), {…}) }})
```

## 用判断守卫不可逆动作（J-08）

登记为不可逆的动作（CLI 的 `write_json`）只有在守卫里至少有一项来自**可信材料上、线放行的已决判断**，或来自 `ask` 时才执行。这份证据只在 `handle` 分派出口时产生：所选臂的守卫栈压上它，臂返回值里的每个 `Bool` 也带上它，经 `let`、字段、下标、函数返回原样带走；`&&` 保留两侧的证据，`||`、`!`、比较（`==`、`>` 等）和其他运算都不带；`if` 不把条件的证据传给分支里造出来的值；未决出口不给证据（B121）。三种写法：

```jpp
// 逐项：do 写进臂里，每个动作由选出它的那次判断放行
map(accepted(r), fn(e) { handle(e.exit, {act: fn() { do("write_json", ["out.json", e.item], 0) },
                                         ignore: fn() { unit }, unsure: fn(u) { consume(u, "drop"); unit }}) })

// 批量：取一个被接受元素的出口作见证（accepted 里的元素都由 Act 选出）
if len(xs) > 0 { handle(accepted(r)[0].exit, {act: fn() { do("write_json", ["out.json", xs], 0) },
                                              ignore: fn() { unit }, unsure: fn(u) { consume(u, "drop"); unit }}) } else { unit }

// 不放行：比较式没有证据；fold 里用 || 累积也没有
if len(xs) > 0 { do("write_json", ["out.json", xs], 0) } else { unit }
```

`let ok = handle(cut(judge(…)), {act: fn() { true }, ignore: fn() { false }, unsure: fn(u) { …; false }}); if ok { do(…) }` 同样合法：`ok` 带着那次判断的证据。

两条会让可信材料上的判断也不放行（步 17b）：题面里填进了不可信文本（`fill` 的槽值、拼出来的题面、`purpose`、`gen` 或入口给的模板），出口随题面不可信（B58）；被判断的材料是由试用线、夹具线等不放行的线或未决出口一路选出来的，出口也不算证据，报文写「该材料由 {等级} 线的出口选出」（谱系放行，B72-4）。

作者声明线（`declare`）的出口要宿主带 `--release-on-declared` 才算放行（见上节「线从哪里来」）。

`--input-trusted`（步 14b-1，B108）让 CLI `--input` 材料上的判断也能作可信合取项；它只是说「这份材料来自我信任的来源」，不是说「内容正确」——出口仍要经认证过的线放行才算证据。这个声明进 `entry_hash`（像其他入口条目的 taint 一样），换一次 `--input-trusted` 状态重放会报 `W-header: entry_hash`。

## 真机运行

固定观察只验证程序结构，不产出真实读数；要拿真实判断，需要 `--features live` 编译并
带 `~/.typesafe-key`：

```sh
cargo build -p jpp --features live --release
./target/release/jpp run examples/sieve.jpp --backend live --profiles-dir profiles --calib calib --ledger-out ledger.json
./target/release/jpp run examples/sieve.jpp --replay ledger.json   # 事后离线重放，逐字节一致
```

真机运行必须带能力画像（B73）：`--profile <文件>`，否则 `--profiles-dir <目录>/<model>.json`，再否则 `jpp` 可执行文件旁的 `profiles/`；找不到报 `E-profile-missing`。仓库附带 `profiles/jev-1.13.0.json`，价格与 δ 都从它读，它的哈希进账本头。重放不发调用。证书记下认证时的 δ（B104）：带画像时判区按证书 δ 与画像 δ 中更严者取，不带画像时按证书的 δ 取，线不会因 δ 不一致而放宽；两者不同时报 `W-delta-mismatch`。要逐字节复现真机运行，重放时给同一份画像。没有记下 δ 的旧证书、没有范围指纹的记录（认证集没有材料文本），出口照常路由但不放行不可逆 `do`（`W-delta-unknown`、`W-scope-unknown`），重新导入（标注行带 `text`）即可。

真机只给读数（概率），不给出口：`cut` 要把读数变成 act/ignore/pick/at 必须有一条
**认证过的线**（J-03 禁止程序自己写线），新题第一次跑一律 `Unsure(cold)`。上线的
唯一路径是标真值再导入：

```sh
# labels.jsonl 每行：{"key": "cs-refund", "item": "c1", "p": 0.99,
#                     "label": true, "source": "computed"}
./target/release/jpp calib-import labels.jsonl --calib-out calib
```

**样本量**：默认 `--alpha 0.1 --conf-delta 0.1` 下，认证的每一侧（act 一侧、ignore
一侧）各自至少需要 **22 条零错误的已判定样本**。缺省的固定序认证（B86）不拆分样本，
一道字面题式约 **60 条**、语义题式约 **60–80 条**带真值的标注即可正式上岗；试用线
（`--alpha-trial 0.25`，只路由）约 32 条（`jpp calib-import --help` 里写明，出处是离线对照
`地基/评估/2026-09-24-新题标注门槛-对照/results.md`）。`--certify split` 是旧的拆分认证，条数约 2–3 倍。
样本不够时不报错、也不瞎放行，而是停在「待核」，例如 5 条正例、5 条负例的构造真值：

```
"gate": "待核：样本不足（正例 5、负例 5，固定序零错误也需每侧已决 ≥ 22）"
```

**待标清单与标注包（B88、B107、B120）。** 用首跑账本导出清单：
`jpp calib-import --from-ledger 账本 --key 键 --list-out 清单.jsonl --report report.json [--materials 材料.json]`。
清单每行带 `item`（材料的状态哈希）、`q`（题哈希）和组号；带 `--report` 时另附题面 `template` 与填法 `fill`，
标注者看得见问的是哪道题。一道题式多个填法问同一批材料时，同一个 `item` 会出现多次，每次 `q` 不同：
标注行要原样带上 `q`，回填按 `(item, q)` 接回读数；不带 `q` 而该材料有多个读数时报 `E-list-ambiguous`。
清单、标注包、复核包里的编号、行序、文件名**不得携带材料生成者的任何信息**（设计意图、真值、分组、正反例后缀）：
清单的状态哈希满足这一条；手工出包要用不透明编号并打乱顺序，否则整批作废重标（`地基/题库/规范.md` §1.3）。
回填时带 `--report`，记录的题类取运行时算出的精化类（例如一对材料上的题是关系类）。

样本不足时该题的出口仍会路由（按冷键处理），但不能放行不可逆的 `do`。只有模型自己
标注、没有人工真值时，还需要同一题式的人工抽检一致率过 `--spot-check-min`（默认
0.9）才准临时上岗（`W-provisional`），否则线停在「待真值」。

## 搭配：元素 → 组合 → 嵌套

「搭配」是判断器（JEV）与生成器（大模型）、执行器、精确算法、检索这些组件搭在一起的写法。它是
J++ 的底层，不是可选功能：判断是唯一产出读数的效应，别的组件各带自己的画像（成本、时延、是否
确定、错不错），搭配层规定它们怎么互相喂材料、怎么互相接未决责任。这一章按三层展开，对应
`00-Nature意图汇编.md` 第 4、5 条：**最小的搭配是一个元素**（一次判断、一次生成、一次执行……），
**元素组合成更大的搭配**（搜索、判出来的图、接地核验……），**组合本身还能再组合**（搜索里嵌接地、
图的产物再判、再建一张图、循环里跑多个独立的搜索）——组合的产物与元素同一种契约值形状，能再
交给下一层 `sieve`、`pair`、`map`，这是「组合封闭性」在搭配层的体现，不是额外规定的接口。

跨层时，未决的去向、误差界、预算、账本与重放、放行把关都由语言与库自动带着走，不需要在每一层
重新手写：`sieve`/`search`/`graph.jpp` 的产物都自带 `pending`，出口都能 `compose`，账本键跨层
仍然唯一。这正是「换判断器程序不改一个字」「一句顶一百句」这两条验收在搭配层的落点：写一次
`search`，判断器从固定观察换成真机、生成器从占位换成 `claude -p`，程序不用动。

本节的每个签名以 `lib/compose/*.jpp` 源码为准。**`地基/比赛/搭配用法手册.md`（施工前写的草稿）
里的部分签名已经过时**——最常见的两处：`ground` 手册写成两个参数 `ground(action, render)`，
源码是三个参数 `ground(action, args_of, render)`（执行器要三个实参：代码、标准输入、超时秒数）；
`search` 手册把 `rounds` 塞进 `opts` 里，源码里 `rounds` 是单独的位置参数、写在 `opts` 前面。
本节之后，手册以本章为准。

每段代码都在 `examples/guide/` 下有对应的可运行文件，配好了固定观察夹具；`scripts/guide_check.py`
逐个用 `jpp check`、能跑的再 `jpp run --fixtures` 实际跑过一遍，不是誊抄示意伪代码。

### 元素

元素是搭配里不可再拆的一次调用：一次判断、一次生成、一次执行、一次精确算法、一次检索，或者
纯确定性的文本与数据处理。它们各自的成本、是否确定、要不要沙箱都不一样，写法上却有一个共同点：
输入输出都是普通值或材料，不夹带读数（数字不流，`00-目标与动机` §四）。

**一次判断（judge + cut）。** 一道题（`test`/`select`/`measure`）问在一份材料（`state(mat(...))`）
上，`judge` 拿到读数（概率分布），`cut` 按线把读数切成三值出口（act / ignore / unsure），`handle`
把三个出口分派到三段代码。线从哪里来、`declare`/`stat`/`closed`/`--release-on-declared` 怎么用，
本文件前面「线从哪里来」「用判断守卫不可逆动作」两节已经讲透，这里不重复，只给最小可运行的样子：

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let overdue = test("这条工单是否已经超过约定的响应时限？", "ticket-overdue");
let r = judge(state(mat("工单 #391：约定 4 小时响应，已过去 9 小时，客户尚未收到回复。")), overdue);
handle(cut(r), {
    act: fn() { "超时：升级" },
    ignore: fn() { "未超时" },
    unsure: fn(u) { consume(u, "drop"); "线附近，转人工" }})
```
（`examples/guide/elem-judge.jpp`）统计量线（`stat: "expect"`/`{mass: […]}`/`"confidence"`）与
两端开闭（`closed`）见 `examples/guide/elem-declare-stat.jpp`；作者声明线与宿主接受见
`examples/guide/elem-declare-accept.jpp`（两份夹具：不带 `--release-on-declared` 时声明线出口
只路由不放行 `write_json`，带上之后才放行）。

**生成（gen）。** 需要解空间里本来没有的东西（候选、模板、测试、分类）时才调，已有的东西里挑
用判断，不调生成器（`00-Nature意图汇编` 第 6 条）。`gen(题面, ctx, n, retry_seq)` 是非阻塞的：
调用在本层刷新点登记、发出去，程序照常往下跑，结果只在第一次真正被读到的地方等（惰性求值），
所以同一层里独立的几个 `gen` 与判断会重叠执行，不必手动并发。产物默认 `taint: untrusted`（生成器
画像声明），失败时返回 `Fail`，不中断程序：

```jpp
import "../../lib/outcome.jpp";
budget {calls: 4, cost: 0.01, depth: 64};

let brief = mat("为一家社区咖啡馆起名：温暖、好记、不超过四个汉字");
let names = gen("按下面的需求提出 3 个候选店名，每个候选是一个字符串", [brief], 3, 0);
let fits = test("这个名字适合做一家社区咖啡馆的店名吗？", "gen-choose-fit");

if is_fail(names) {
    {failed: text(names), chosen: [], rejected: [], pending: []}
} else {
    let r = sieve(names, fits);
    {chosen: map(accepted(r), fn(e) { e.item }),
     rejected: map(ignored(r), fn(e) { e.item }),
     taints: map(names, fn(m) { m.taint }),
     pending: r.pending}
}
```
（`examples/guide/elem-gen.jpp`）真机下把固定观察换成 `--gen-model sonnet`（`claude -p`），程序
不改；重复调用按账本键（题面、语境哈希、`n`、`retry_seq`）复用，配 `--gen-cache` 跨运行也不重发。
生成物应当按缓存复用，日常运行只判、不再生成——搭建一次、日常只判的完整写法见「搭配用法手册」
§六（未过时的部分）。

**执行（exec_py / check_tests / exec_sql）。** 判断只看字面（〇.1）：代码本身不是字面材料，只有
跑出来的输出才是。三个执行动作都在操作系统沙箱里跑（B164）：临时目录是唯一可写处、断网、有
超时。没有沙箱时，顶层直接调用（本节的写法）得到 `Fail(NoSandbox)`，不是运行期错误，`is_fail`
分流；但如果这个 `do` 处在由不可信判断的出口决定要不要走的分支里（例如放进 `search` 每一轮的
`opts.ground`），执行器登记为不可逆，会先撞上 J-08（不可信材料不能单独放行不可逆动作），不是
直接拿到 `Fail`——两条路径不同，写「组合：ground / verify」那种闭环时要分清。`exec_sql` 只放行
只读语句（`query_only`），要 `SELECT` 一个已经建好的库，不能建表；写数据、改库不属于这三个动作。

```jpp
budget {calls: 4, cost: 0, depth: 8};
let out = content(do("exec_py", ["print(sum(range(1, 11)))", "", 5], 0));
{stdout: out.stdout, exit_code: out.exit_code, timed_out: out.timed_out}
```
（`examples/guide/elem-exec.jpp`）把执行输出交给判断，是下一节「组合：ground / verify」做的事，
这里只演示执行本身。`exec_py`/`check_tests` 的实参形状是 `[代码, 标准输入或测试, 超时秒数]`。

**精确算法（graph:\*）。** 六个动作：`graph:matching`（带权最大匹配）、`graph:shortest_path`
（Dijkstra）、`graph:max_clique`（Bron–Kerbosch）、`graph:components`（并查集）、`graph:set_cover`、
`graph:max_flow`（Dinic）。全部纯函数、可逆、成本 0。边权是程序自己给的常量，不是判断读数——
数字不流；判断出来的图（读数决定「有没有这条边」）在下一节。

```jpp
budget {calls: 4, cost: 0, depth: 8};
content(do("graph:matching", [{edges: [{u: 0, v: 1, w: 3}, {u: 1, v: 2, w: 5}, {u: 0, v: 2, w: 1}], nodes: [0, 1, 2]}], 0))
```
（`examples/guide/elem-graph-algo.jpp`）`set_cover`、`max_flow` 的产物形状与另外四个不同，
`lib/compose/graph.jpp` 的 `on_graph` 不接这两个，要用就直接 `do`。

**检索（embed_topk / bm25_topk）。** 检索只负责召回，不负责精度：候选数万级时先粗筛缩到几百，
再交给判断逐条精判（「搭配用法手册」§一「先便宜后贵」）。`bm25_topk(query, corpus, k)` 纯 Rust
实现，任何机器上都能跑，不需要联网或本地模型：

```jpp
budget {calls: 4, cost: 0, depth: 8};
content(do("bm25_topk", ["苹果", ["苹果 香蕉 橙子", "今天天气很好", "苹果派配咖啡"], 2], 0))
```
（`examples/guide/elem-retrieval-bm25.jpp`，返回 `[{id, score}, …]` 按分数降序。）`embed_topk
(texts, query, k)` 子进程调离线的 sentence-transformers（默认模型
`paraphrase-multilingual-MiniLM-L12-v2`），语义相近但字面不重合时才需要它；子进程要本机已经
缓存过这个模型，比赛现场的机器不一定联网下载过，`examples/guide/elem-retrieval-embed.jpp` 只
`check` 不 `run`（静态检查不执行动作，不依赖模型是否在本机）。字面能重合就优先用 `bm25_topk`。

**文本与数据内置（确定性，B157）。** `split`、`trim`、`regex_match`/`regex_find`、`parse_json`、
`hash`、`date_parse`/`date_format` 这类纯确定性处理直接写在 `.jpp` 里，不进账本、不是刷新点。
`regex_find` 返回全部整段匹配组成的列表，**不返回捕获组**（`re.find_iter` 逐个整段匹配，不看
括号）；要提取子串就把要的部分整段写进正则，不要指望括号里的内容单独出现在结果里：

```jpp
budget {calls: 0, cost: 0, depth: 8};
let raw = "  订单号=A203, 金额=199.50, 状态=待发货 ";
let trimmed = trim(raw);
let parts = split(trimmed, ", ");
let amount = parse_json("199.50");
let has_ship = index_of(trimmed, "待发货") >= 0;
let m = regex_find(trimmed, "订单号=[A-Z0-9]+");
{trimmed: trimmed, parts: parts, amount: amount, has_ship: has_ship, matched: m, fingerprint: hash(trimmed)}
```
（`examples/guide/elem-text.jpp`）完整名单与 `now`/随机数的处理见本文件前面「确定性处理在 .jpp
里写」一节，这里不重复。

### 组合

组合是库函数（`lib/compose/*.jpp`），不是新的语言构造：全部写在 `pair`、`sieve`、`outcome`、
`compose`、`element` 这些已经开放的内核构造之上，`import` 之后当普通函数用，能作参数传、能嵌进
`map`/`filter`，能嵌进另一个组合。每个组合把「元素怎么循环、未决怎么去向、出口怎么合成」收进一个
函数，调用者只管给判断题、给候选来源、给要跑的算法。

**search（提出 → 判 → 再提出）。** 解空间列不完，但能从好的部分解长出更好的解时用：写方案、写
代码、起名字。生成器负责提，判断器负责判，有界迭代负责收敛。

```
search(seed, propose, feasible, objective, rounds, opts) -> 契约值
  seed      第一轮前沿（材料列表）
  propose   fn(前沿, 轮次) -> [Mat]，通常包 gen；返回 Fail 时这一轮零候选，记一条 Unsure(fail)
  feasible  是非题：候选在可行域里吗
  objective 是非题，或 unit（只按可行域筛；打分排序要等 15k/B167，现在只能是非题）
  rounds    轮数上限，写字面量或顶层 let 常量（调用点核，B111）；喂一个不可判定的表达式
            （函数参数、`len(...)` 之类）不是硬错误，是 `W-bound` 警告：静态估不出上界
  opts      {width, unsure_to?: "carry"|"refine"|fn, ground?: fn(Mat) -> Mat}
```
好候选按轮累积：每轮的新候选去重（按材料哈希，判过的不再判、不花钱）、`opts.ground` 给了就先接地、
`sieve(可行域)`、`objective` 不是 `unit` 再 `sieve(目标)`，新判好的并进累积池，下一轮前沿取池的前
`width` 个。终止用 `iterate` 的线：池凑够 `width` 即 `stop`；一轮没有新的好候选（含只有重提旧候选）
即 `noshrink`；轮数用尽即 `bound`。未决按 `opts.unsure_to`：`"carry"`（缺省）并进 `pending`；
`"refine"` 另把未决材料并进下一轮前沿，让生成器据此改进（出口照样进 `pending`，不因细化销账）；
给一个函数就逐轮交给它，交人用 `lib/compose/ask.jpp` 的 `ask_human`（程序要写 `budget {escalate: N}`，
J-07 会静态查）。返回值 `value` 是累积前沿（每个元素带 `round`：第一次判好的轮次），出口是
`trail` 上全部出口与自己出口的 `compose(…, "all")`；`detail = {ignore, rounds, reason, measures,
fails, duplicates}`。

```jpp
import "../../lib/compose/search.jpp";
budget {calls: 8, cost: 0.01, depth: 64};

let brief = mat("为一家社区咖啡馆起名：温暖、好记、不超过四个汉字");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 个候选店名，每个候选是一个字符串；需求之后的上下文是上一轮留下的候选", concat([brief], frontier), 3, i)
};
let fits = test("这个名字适合做一家社区咖啡馆的店名吗？", "search-fit");

let r = search([], propose, fits, unit, 3, {width: 2});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }), duplicates: r.detail.duplicates,
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 reason: r.detail.reason, rounds: r.detail.rounds, measures: r.detail.measures,
 pending: r.pending}
```
（`examples/guide/comp-search.jpp`，第一轮就有两个合适的候选，凑够 `width = 2`，`reason: "stop"`。）
轮数用尽、一轮不再有新候选的两种收尾分别见 `examples/search-bound.jpp`、`examples/search-noshrink.jpp`
（未收进 `examples/guide/`，行为与上面同一族，读法一样）。

**ground / verify（执行 → 判）。** 要判的东西不是字面的：代码对不对、查询答没答对。

```
ground(action, args_of, render) -> fn(候选材料) -> 材料 | Fail
verify(cands, grounder, q) = sieve(map(cands, grounder), q)
```
`ground` 返回一个函数值：对每个候选跑 `do(action, args_of(候选), 0)`，失败（含没有沙箱的
`NoSandbox`）原样返回失败值，否则把候选与执行输出一起渲染成字面材料（`render(候选, 输出)`），
taint 承接执行输出（执行器为 `untrusted`）。`ground` 的返回值可以直接放进 `search` 的
`opts.ground`——这就是「提出 → 执行 → 判 → 再提出」的闭环，不是第四个原语：

```jpp
import "../../lib/compose/ground.jpp";
import "../../lib/compose/search.jpp";
budget {calls: 20, cost: 0.01, depth: 64};

let brief = mat("写一段 Python 代码，打印 1 到 10 的平方和");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 段 Python 代码，每段是一个字符串；需求之后的上下文是上一轮留下的候选（代码与输出）", concat([brief], frontier), 3, i)
};
let runner = ground("exec_py", fn(c) { [content(c), "", 5] },
                    fn(c, out) { "代码：" + content(c) + "\n输出：" + content(out).stdout });
let correct = test("这段代码的输出是否等于 1 到 10 的平方和（385）？", "ground-correct");

let r = search([], propose, correct, unit, 3, {width: 1, unsure_to: "refine", ground: runner});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }),
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 undecided: map(r.pending, fn(p) { {item: p.item, cause: p.cause, via: p.via} }),
 reason: r.detail.reason, rounds: r.detail.rounds, measures: r.detail.measures, duplicates: r.detail.duplicates,
 pending: r.pending}
```
（`examples/guide/comp-ground-verify.jpp`：第一轮三段代码都不对，`refine` 宽限一轮，第二轮凑够
`width = 1` 停止。金样不在检查期执行代码——本例的 `guide_check.py` 从已经在有 `sandbox-exec` 的
机器上真跑一次录下的种子账本 `tests/golden/search-ground/seed.ledger.jsonl` 用 `--resume` 续跑，
什么都不需要重新执行——种子账本里已经带着两轮全部的 `gen`/`exec_py`/`transform` 记录（`judge`
不在种子里，固定观察下始终从 `--fixtures` 取，不进种子）。种子是在有 `sandbox-exec` 的机器上把
整个程序跑一遍录下的，`--resume` 只是把这些记录接上。**这一条只在本机（有 `sandbox-exec`）验证过：
续跑不再执行任何一次。没有沙箱的机器上是否能跑通没有验证过**——`ground` 里的 `do` 处在由判断
出口决定要不要走的分支里（`search` 每一轮），没有沙箱时执行器登记为不可逆，运行期能不能走到、
是先按账本键取历史结果还是先经 J-08 拦下，这条路径本文没有在无沙箱环境实测，不写成结论。
`ground` 内部的 `do(action, …)` 里 `action` 是形参、不是字面量，CLI 因此把它当作可能不可逆而
要求 `--ledger-out`——库外自己直接 `do(变量, …)` 时也是同一条规则，下面「常见报错」有专门一条。

**judged_graph / judged_bipartite / on_graph / interval（判出来的图）。** 决策是全局的、不是逐条
的：配对、分组、找路时用图算法；但边要判断给出。边有三种状态：已决有、已决无、未决。算法只在
已决边上跑；未决边另跑一次「乐观图」（已决 ∪ 未决），两次产物之差（`differs`）就是该先问人的边。

```
judged_graph(nodes, edge_q, {over?, prune?})        单集合、无向：候选取 i < j
judged_bipartite(left, right, edge_q, {over?, prune?})  两组节点，右侧节点号从 len(left) 起
on_graph(g, algo, args, mode)   mode: "decided"（只用已决边）| "optimistic"（已决 ∪ 未决）
interval(g, algo, args) -> {lo, hi, exit, differs, complete, same, alpha_bound, n_unknown}
```
`prune(nodes)`（或 `prune(left, right)`）给候选下标对，通常来自向量粗筛；不给就全配对（`over`
可以再筛一遍原节点）。`on_graph` 的 `algo` 只认 `matching`/`shortest_path`/`max_clique`/
`components`；`args.weight: fn(边) -> 数` 只能给非判断来源的权，缺省 1——数字不流，边权不能是
读数。产物元素带 `item`，能再交给 `sieve`、`pair`，或作下一张图的节点。

```jpp
import "../../lib/compose/graph.jpp";
budget {calls: 16, cost: 0.01, depth: 64};

let tasks = ["任务：写登录页", "任务：设计数据库表", "任务：写接口文档", "任务：搭部署脚本", "任务：做数据看板", "任务：写单元测试"];
let people = ["小林：前端三年，做过登录与注册", "小周：后端，主写数据库迁移", "小吴：技术写作，维护过接口文档", "小郑：运维，写过部署流水线", "小王：数据分析，常做报表", "小陈：测试开发，熟悉单元测试框架"];

// 候选对由调用者给（通常来自向量粗筛）；这里写死十对
let cand = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 0], [5, 1], [0, 4], [1, 5], [4, 4], [5, 5]];
let can_do = test("a 描述的这项任务，b 这个人能接下来吗？", "graph-can-do");

let g = judged_bipartite(tasks, people, can_do, {prune: fn(l, r) { cand }});
let t = interval(g, "matching", {});

{assigned: map(t.lo.value, fn(p) { p.nodes }),
 if_all_yes: map(concat(t.hi.value, t.hi.pending), fn(p) { p.nodes }),
 ask_first: map(t.differs, fn(e) { [e.left, e.right] }),
 same: t.same, complete: t.complete, alpha_bound: t.alpha_bound, n_unknown: t.n_unknown,
 edges: {yes: len(g.edges), no: len(g.rejected), open: len(g.pending)},
 pending: concat(t.hi.pending, g.pending)}
```
（`examples/guide/comp-graph-interval.jpp`：十条候选边里两条落在带内，只信已决边能分出 4 项，
把未决边都当有能分出 6 项，`differs` 恰是那两条。）`g.pending` 一定要整体交出去——只带
`t.hi.pending` 不够，没被乐观产物用到的未决边会漏掉（同一条边在几处的责任是同一份，B162）。
`alpha_bound` 读作「这组结果里至少一条边判错的概率上界」：未经认证的线（夹具线、声明线、试用线、
类线、冷线）一律按 1 计，`n_unknown` 就是有几条这样的边，`alpha_bound = 1` 说明还没有能担保的线。
六个 `do("graph:*")` 动作名是拼出来的（`join(["graph:", algo], "")`），CLI 按可能不可逆处理，
运行要带 `--ledger-out`。

**compose 与 cert（出口合成的原语）。** `search`、`graph.jpp` 内部都靠这两个原语把多个出口合成
一个、读出联合的误差界；直接用它们，是搭一个新组合（不是套现成的 `search`/`graph`）时才需要的
写法。

```
compose(出口们, "any" | "all" | "min" | {first: k} | {sup: 候选下标…}) -> Exit
cert(出口) -> {grade, alpha, alpha_bound, n_unknown, line}
```
`compose(…, "all")` 只在全部分量都放行时才放行，结果未决时吸收全部未决分量的责任。`cert` 只读，
不销账：单个出口的 `alpha` 是它自己的置信度上界，合成出口的 `alpha` 是 `unit`、界读 `alpha_bound`。

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let a = test("材料 a 是否满足条件一？", "cond-a");
let b = test("材料 b 是否满足条件二？", "cond-b");
let ea = cut(judge(state(mat("材料 a 的内容")), a));
let eb = cut(judge(state(mat("材料 b 的内容")), b));
let joined = compose([ea, eb], "all");
{joined: exit_kind(joined), cert: cert(joined)}
```
（`examples/guide/comp-compose-cert.jpp`：两条都是夹具线，`cert.grade` 是 `"Cold"`、
`alpha_bound` 是 `1.0`——没有认证过的线，界只能报到最保守。）

**element（元素记录构造的原语）。** `sieve`、`search`、`ground` 内部都用它把「材料 + 出口」造成
一条标准形状的元素记录（带 `item`/`pos`/`trail`/`exit`/`cause`/`q`/`qi`/`key`）。它做两件
`.jpp` 代码本身做不到的事：`item` 只加一条「选择依赖边」（读数只是选中了这份材料，不代表内容由
这道题派生出来，J-02 因此不会误拦「同一材料再问一道题」）；报告的 `exits` 表里补上这一行的
`index`/`pos`。库作者自己搭新组合时才需要直接用它：

```jpp
budget {calls: 4, cost: 0.01, depth: 8};
let q = test("这份材料是否合格？", "elem-ok");
let e = cut(judge(state(mat("样品 A")), q));
let el = element(mat("样品 A"), e, {pos: 0, q: q, key: "elem-ok"});
{item: content(el.item), exit: exit_kind(el.exit), pos: el.pos, cause: el.cause}
```
（`examples/guide/comp-element.jpp`）

### 嵌套

三条已经跑通的嵌套写法，对应 `00-Nature意图汇编` 第 5 条「元素之间继续组合，组合的组合再组合」：

**search 接 ground 的闭环。** 上一节 `comp-ground-verify.jpp` 本身就是这条：`ground` 的返回值是
一个普通函数值，直接填进 `search` 的 `opts.ground`，外层看不出内层在执行代码——闭环不是新增的
第三个原语，是「组合能当参数传」这条规则的直接结果。

**图上分工的产物再判、或再建一张图。** `judged_bipartite`/`judged_graph` 的产物元素带 `item`，
和 `sieve`/`pair` 的产物是同一种形状，能原样再交给下一层：

```jpp
import "../../lib/compose/graph.jpp";
budget {calls: 40, cost: 0.01, depth: 64};

let tasks = ["任务：写登录页", "任务：设计数据库表", "任务：写接口文档", "任务：搭部署脚本", "任务：做数据看板", "任务：写单元测试"];
let people = ["小林：前端三年，做过登录与注册", "小周：后端，主写数据库迁移", "小吴：技术写作，维护过接口文档", "小郑：运维，写过部署流水线", "小王：数据分析，常做报表", "小陈：测试开发，熟悉单元测试框架"];
let cand = [[0, 0], [1, 1], [2, 2], [3, 3], [4, 0], [5, 1], [0, 4], [1, 5], [4, 4], [5, 5]];

// 第一层：人与任务的图，只信已决边的分工
let g = judged_bipartite(tasks, people, test("a 描述的这项任务，b 这个人能接下来吗？", "graph-can-do"), {prune: fn(l, r) { cand }});
let t = interval(g, "matching", {});

// 嵌套 (1)：分工产物的 item 就是判边时那一对材料，直接交给 sieve
let ok = sieve(t.lo.value, test("这一组分工（a 是任务，b 是接手的人）能在一周内交付吗？", "graph-team"));

// 嵌套 (2)：分工产物当节点，判两组能否合并，再在第二张图上匹配
let g2 = judged_graph(t.lo.value, test("a 与 b 两组分工能合并成一个小组、互相补位吗？", "graph-merge"), {});
let t2 = interval(g2, "matching", {});

{deliverable: map(ok.value, fn(e) { e.nodes }),
 not_in_a_week: map(ok.detail.ignore, fn(e) { e.nodes }),
 groups: map(t2.lo.value, fn(p) { map(p.nodes, fn(team) { team.nodes }) }),
 layer2_same: t2.same,
 ask_first: map(t.differs, fn(e) { [e.left, e.right] }),
 pending: concat(concat(ok.pending, g2.pending), concat(t.hi.pending, g.pending))}
```
（`examples/guide/nest-graph-then-graph.jpp`）两层都不手写批、预算或未决处理：每层一次刷新，
两层的未决都交回调用者，`pending` 的 `concat` 顺序不影响正确性、只影响读者看到的顺序。
「图上分工再交给 search 出方案」是同一族写法（`map(t.lo.value, fn(p) { search([p.item], …) })`），
本章没有单独收一份可运行示例，因为它就是上面两条的直接叠加，不引入新机制。

**map 里跑多个独立的 search。** 需要对一批需求各自搜索一遍（例如给每个需求各出一个方案）时，
`map` 里嵌 `search` 是最直接的写法：

```jpp
import "../../lib/compose/search.jpp";
budget {calls: 8, cost: 0.01, depth: 64};

let briefs = ["二手书店", "宠物美容店"];
let propose = fn(topic) { fn(frontier, i) { gen("为「" + topic + "」起一个不超过四个字的名字，输出 1 个候选，字符串", frontier, 1, i) } };
let fits = test("这个名字符合需求吗？", "map-search-fit");

let plans = map(briefs, fn(b) { search([], propose(b), fits, unit, 1, {width: 1}) });
{named: map(plans, fn(r) { map(r.value, fn(e) { e.item }) }),
 reasons: map(plans, fn(r) { r.detail.reason }),
 pending: concat(plans[0].pending, plans[1].pending)}
```
（`examples/guide/nest-map-search.jpp`）**这里的多个 `search` 现在按构造串行生成**：`map` 本身
不并行调度，每次 `search` 内部的 `gen` 虽然非阻塞，但轮次之间仍按 `iterate` 的顺序推进，两个
`search` 之间也不交叠（INTERFACE.md §三·四·四「已知限制」）。跨 `map` 的调度交给以后的施工步，
现在写法上没有别的选择，也不需要为此改变程序结构。固定观察下有一个实测细节：两次 `propose` 的
题面必须带上能区分彼此的文字（这里是 `topic`），不能只靠上下文材料不同——夹具按「题面文字 +
`retry_seq`」匹配生成记录，题面相同时后一条会覆盖前一条，返回同一个生成结果（本例的
`examples/guide/fixtures/nest-map-search.json` 就是按这条改过一次才对上）；真机运行没有这个
限制，因为真实生成器会按完整上下文各自生成。

### 什么时候用哪个

| 情形 | 用什么 | 为什么 |
|---|---|---|
| 判断某个东西对不对、分不分类 | 一次判断（`judge`/`cut`） | 语义判断只能由 JEV 做，其余组件不带校准置信度 |
| 候选集不存在、需要新的东西 | 生成（`gen`），产物缓存复用 | 已有的东西里挑不需要生成；生成一次、判断反复用 |
| 候选集已在手上（文件、数据库、枚举器） | 直接判断，不调生成器 | 调生成器是每题贵几十倍、没有校准线的做法 |
| 要判的东西不是字面的（代码、查询、配置） | 先执行（`do`/`ground`）再判 | 字面化定律：判断只看字面，先把结果变成字面 |
| 解空间能列出来，几十到几万个 | 枚举 + `sieve` | 广度免费，先列全再判比边猜边判更省 |
| 解空间列不完，要从部分解长出更好的解 | `search` | 判断给可行域与目标两道题，生成器负责提，收敛靠迭代 |
| 要闭环验证（写代码要跑测试） | `search` 的 `opts.ground` | 提出、执行、判、再提出是同一个原语，不是拼出来的 |
| 决策是全局的（配对、分组、找路） | `judged_graph`/`judged_bipartite` + `interval` | 逐条判只给边，全局最优要图算法；边由判断给，算法只吃已决/未决两组 |
| 候选数万级 | 先 `bm25_topk`/`embed_topk` 粗筛，再判 | 检索只负责召回，判断负责精度；全配对成本按平方增长 |
| 判不清（`unsure`） | 不调大模型，走细化 / `ask` / 记账丢弃 | `unsure` 是深度的使能机制，不是要靠多问一次模型来消灭的失败 |
| 搭一个新组合（不是套现成的） | `compose`/`cert`/`element` | 这三个是搭配库自己用的原语，日常写程序很少直接碰 |

### 常见报错与修法

报文以 `examples/guide/err-*.jpp` 实际跑出来的原文为准；标了「未在本机复现」的两条引自源码，
本机条件（有 `sandbox-exec`、没有 r1 格式的旧账本）碰不到触发条件，没有假装跑过。

| 诊断码 | 触发场景 | 修法 |
|---|---|---|
| `J-05`（运行期） | `sieve`/契约值的 `pending` 被悄悄丢弃：只返回 `accepted(r)`，没提到 `undecided(r)`/`unobserved(r)`/`r.pending` | 把 `r.pending` 放进返回值，或 `consume(u, "drop")` 显式丢弃并记账；检查期会先给一条 `W-pending-unreturned` 警告，运行期真的丢了才是错误（`examples/guide/err-j05.jpp`） |
| `J-08`（静态子面） | 不可逆动作（如 `write_json`）的每一层守卫都只来自作者声明线，且宿主没带 `--release-on-declared` | 带 `--release-on-declared`（确认这些线由自己担责），或在条件里再合取一个认证线上的判断，或改走 `ask`，或把动作登记成可逆（`examples/guide/err-j08.jpp`） |
| `E-stat-unavailable` | `cut` 的 `stat: "confidence"` 用在没有声明 `reports_confidence` 的画像上 | 换一个声明了 `reports_confidence: true` 的画像，或改用缺省的 `stat: "max"`；固定观察下可以在夹具观察里直接给 `confidence` 字段（`examples/guide/err-stat-unavailable.jpp`，用 `--profile profiles/jev-1.13.0.json` 触发，这份发行画像没有该字段） |
| `E-ledger-required` | 程序里出现登记为不可逆的 `do`（字面动作名，如 `write_json`），或动作名不是字面量的 `do`（如 `graph.jpp` 内部 `join(["graph:", algo], "")` 拼出来的名字，或 `ground.jpp` 里的形参 `action`）——两种情况 CLI 都一律按可能不可逆处理 | 加 `--ledger-out <文件>`；即使该动作实际可逆也要加，这是 CLI 的保守判定，不是逐个动作核实后才要求（`examples/guide/comp-graph-interval.jpp`、`nest-graph-then-graph.jpp`、`elem-declare-accept.jpp` 都要带） |
| `J-03` | 程序里手写了数字线（`if p > 0.7`） | 校准键 + `calib-import`（正式线）；临时线用 `--alpha-trial`（只路由）或作者声明线 `declare` |
| `W-untested`（J-15，未测按保守项） | 校准记录 `delta` 缺失时，δ 这一位没测过，读数一律 `unsure(untested:Delta)`，不会到 `act`/`ignore`；报文原文：`W-untested: Delta 在本次路径上没有被测量，按 J-15 取保守项（出口 untested）` | 测试夹具的 `calibrations` 必须给 `delta` 字段，缺了不是报错而是全部出口卡在未决态，容易被误当成「判断器判不出来」；正式线经 `calib-import` 认证自带 δ |
| `E-render-version` | `--resume` 一份用旧渲染版本（`r1`）记的账本；15i 把线上材料形状升到 `r2` 后键分量变了 | 换一份同渲染版本的账本，或重新从 `r2` 起跑；**未在本机复现**——本仓库现存账本都已是 `r2`，没有 `r1` 账本可以拿来触发（引自 `crates/jpp-runtime/src/outcome.rs:36`） |
| `E-action-no-sandbox` | 检查期发现字面动作名（`exec_py`/`check_tests`/`exec_sql`）在没有沙箱工具的机器上 | 装 `sandbox-exec`（macOS 自带）或对应的沙箱工具；**未在本机复现**——这台机器有 `sandbox-exec`，触发条件本身碰不到（引自 `crates/jpp-check/src/rules/e_action_no_sandbox.rs`） |
| `E-list-ambiguous` / `W-no-candidate` / `E-kind-conflict` | `calib-import`/标注流程相关，与「真机运行」一节「待标清单与标注包」同一批 | 见本文件前面「真机运行」一节，不在搭配层重复 |
