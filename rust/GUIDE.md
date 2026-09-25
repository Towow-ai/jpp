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
   {"key": "cs-refund", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗"}
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

`accepted` / `ignored` / `undecided` / `unobserved` / `stopped` **不是内置**，是
`lib/outcome.jpp` 里按契约值字段取值的库函数（`import "lib/outcome.jpp";` 后可用），
契约值形状见 `INTERFACE.md` §三·四·四。

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
