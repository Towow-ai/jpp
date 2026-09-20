# J++ progress / 项目进度

Updated: 2026-09-21. This is a dated report, not an automatically updated dashboard.

## 2026-09-21: co-construct language capabilities and algorithms / 语言能力与算法共同构造

修正[总计划](towow-discovery-master-plan.zh-CN.md)中的推进前提：不要求先用成熟工具实现完整算法再迁移J++。所需的构造能力可能正是语言建设要创造的部分，应与最小算法片段共同设计。已有工具按需复用，缺少完整旧实现时保留构造阻碍与新程序的对照。本轮保存两条原话续记和本地理解记录，只更新设计，不启动实现或模型实验。

The plan no longer assumes a complete host-language algorithm must precede J++. Missing construction capabilities and algorithm fragments may be developed together. Existing tools remain available where useful; this update records the design correction and local prompt provenance, with no new runtime or model-performance claim.

## 2026-09-21: build language improvements from discovery / 从发现应用落实语言改进

补充[总计划](towow-discovery-master-plan.zh-CN.md)：交付发现组件之外，必须产出已经实现、能被另一算法复用的J++构造改进，不能停在语言好不好用的评估。新增具体能力与源码对应表、L0～L4建设循环及改进前后验收。当前只有计划、代码依据和候选方向，尚未宣称改进已实现。另在本地按真实分叉边界保存43条用户原始提示词/界面回复，保留原话与推导的区分，未公开整段会话。

The deliverable now includes implemented language/library/tooling improvements, before/after programs and cross-algorithm reuse, alongside the discovery component. The capability inventory distinguishes native source examples from the Python application. Original user prompts are archived locally with their source boundaries; no new model experiment or runtime change is claimed.

## 2026-09-21: discovery goal, evidence and parallel implementation plan / 发现目标、验收与并行实施总计划

新增[通用发现能力总计划](towow-discovery-master-plan.zh-CN.md)，重新对齐通爻原始问题：接收方依局部上下文提出关系，中间问题与组合参与后续发现。分开J++语言、通爻协议、发现程序与展示的交付；整理已有真实语料、强基线、行为/质量/费用/动态验收及P0～P5依赖。核查并索引先前讨论记录，明确尚非完整逐字归档。当前成果是规划，没有新的模型运行或发现效果结论。

The plan enables parallel work against the published Rust snapshot, with source-owned discovery logic and a minimal input/observation/capability contract. It reuses prior research assets without treating incomplete Gold, synthetic examples or historical proxy edges as completed discovery evaluation. Numerical targets remain proposals to freeze before formal evaluation; corpus splitting and the receiver-local loop are still unimplemented.

## 2026-09-21: align all project entry points / 同步项目各入口的交付状态

The source package was already merged in [PR #12](https://github.com/Towow-ai/jpp/pull/12).
This update corrects stale Python-only and future-syntax wording across current scope,
roadmap, contributor/developer guides, project motivation and verification records.
Original dated results remain labeled as history. The next outcomes are source-library
reuse, consistent composition rules and a bounded application using `.jpp`.

代码已在主分支，本次补齐此前没跟上的介绍：明确独立源码已经交付、Python 指南的
适用范围，以及后续工作怎样验收。检查文档链接与现状表述；不改运行代码，也不发起
模型实验。

## 2026-09-21: standalone source runs on Rust / 独立源码到 Rust 执行贯通

The [native Rust package](../rust/README.md) now parses `.jpp` source, lowers it to
the shared program representation, checks language rules and executes it through
one kernel. The CLI includes `parse`, `check`, `run`, fixed observation loading,
JSON reports and ledger replay. No Python interpreter is used on this path.

现在可以直接写 `.jpp` 文件并运行。源码里的方法可以作为参数、返回方法，再参与
下一次组合。前端只负责表达和转换，执行规则由共同 Rust 内核承担。原 Python 包和
通爻实验保留可用，本次没有扩展 Python 正式内核。

| Executed program / 实际程序 | Observed result / 结果 |
| --- | --- |
| Nested composition / 组合再组合 | 20 → 43; source and direct core program return identical JSON / 源码与直接内核构造结果相同 |
| Adaptive questions / 自适应选问 | 731 located among 1,000 candidates in 10 fixed observations / 十题定位 731 |
| Partial continuation / 部分结果继续求解 | Cost 9 with C/D pending → cost 2 with D pending → complete; 6 observations, A/B/C checked once / 保留旧观察和检查 |
| Replay / 重放 | Same value; zero fresh judgment calls and zero repeated local checks / 返回值相同，无新调用和重复动作 |

Validation in an independent publication checkout: **41 Rust tests passed**, with
one documentation snippet explicitly ignored. Tests cover source execution,
direct core algorithms, syntax/source positions, known literal argument type
errors, budget stopping and replay. A native installation outside the research
checkout ran the complete programs with an empty PATH; Python and Cargo were not
available to the executable. Formatting checks passed. CI also runs the retained
Python suites; its result is recorded on the pull request.

独立发布目录 41 项 Rust 测试通过。原生安装在项目外、PATH 为空时仍可运行；明显
参数类型错误会定位到 `.jpp` 文件行列并提前停止。静态检查覆盖明确子集，动态规则
仍由运行时检查。固定观察与合成校准验证执行机制，不是模型准确率实验；方法闭包
在源码重跑时重建，账本没有被描述为任意闭包的跨进程序列化。

[Grammar / 文法](../rust/FRONTEND.md) · [Equivalent source and core usage / 等价用法](../rust/COMPARISON.md).

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/d39b036).

## 2026-09-20: composable discovery application / 可续接的发现应用

新增[可替换计划的发现组件](towow-discovery-iteration.zh-CN.md)：同一 J++ 组合接受不同意图、问题、路由和组合函数；提议保留原成员与上下文，可以作为下一轮输入。216 个合成主体上的三个真实运行分别完成 20、20、12 个判断，最后一个将后端两人提议续接为含安全检查成员的三人提议。三个案例在安装后的 wheel 中用录制完全复现，重复执行新增请求 0。公开[交互图谱](https://towow-ai.github.io/jpp/demos/towow/teams/)与调用接口，未将其称为通用规划器或分布式网络。

The new application accepts caller-authored plans and replaceable routing/combination functions; nominations preserve their members and can seed later discovery. Three live synthetic runs used the same implementation, followed by installed-wheel replay, zero-request warm reruns and a no-combination comparison. Existing J++ composition and observation mechanisms are reused; the formal Rust kernel direction is unchanged.

325 人固定二十名额的来源探索对照得到 681 条前十命中，低于原 J++ 的 697；7 条跨来源种子进入候选池后仍没有留在前十，因此没有替换默认方法。The fixed-slot exploration control regressed and remains visible rather than replacing the default. 本轮新增估算 JEV 费用约 $0.041700；累计约 $1.530903（已授权总预算 $5）。本地 544 项检查通过，包含一次子进程导入环境修正后的定向复查；浏览器与线上部署另按发布记录核验。

## 2026-09-20: focused OCaml experiments alongside Rust / Rust 主线允许具体 OCaml 实验

The [Rust ADR](adr/0001-rust-kernel.md) now permits OCaml experiments around
specific grammar and semantic questions. Selected rules must enter the shared
specification and be reproduced in Rust; any retained formal OCaml component
needs an explicit interface and verified build/run path. Rust implementation
continues. This is a policy-only change, with links and wording checked; no OCaml
installation, experiment or mixed-language runtime has been performed.

新增“Rust正式实现＋围绕具体问题开展OCaml实验”规范。实验、已选规则、正式实现分别
标注；不因实验暂停Rust，不默认增加用户安装依赖。当前只同步约定，下一次实验由
真实设计问题触发。中英ADR、设计入口和进展一并更新。

[Policy commit / 规范提交](https://github.com/Towow-ai/jpp/commit/35ec2d0)

## 2026-09-20: Rust and standalone source selected / 选定 Rust 与独立源码

The formal language implementation now moves to one Rust kernel with a `.jpp`
parser, J++ type/effect checker and CLI. This replaces the previous indefinite
deferral of independent syntax. [Decision and first acceptance milestone](adr/0001-rust-kernel.md).

正式语言建设转到 Rust；Python已发布成果保留作行为对照、实验与必要适配，不再
继续扩大正式 Python 内核或接口。本次只更新路线、设计入口和中英 README，检查了
链接及现状措辞；尚未交付 Rust 执行能力。下一项可验证成果是源码→检查→运行的
完整程序，覆盖自适应选问及部分结果继续求解，不只是语法展示。

This documentation-only update does not change the existing runnable examples.
Rust construction is starting; subsequent updates will include actual build and
source-program execution evidence.

[Decision commit / 路线提交](https://github.com/Towow-ai/jpp/commit/e04265e)

## 2026-09-20: language design and grammar entry / 语言设计与文法入口

The [design page](design.md) now links directly to the already published current IR/class contract and historical language specification, including its lexical rules and full EBNF. The READMEs and research index expose these links; the historical specification has a bilingual archive notice. Current source remains Python builder code: the old grammar has no delivered standalone parser/compiler, and the current contract defers independent surface syntax without a delivery date; this does not replace the intent to develop an independent language after validating examples and algorithms. The guide distinguishes six IR forms, typed exits, composition and runtime behavior from design requirements.

[设计页](design.md)现在直接链接已经公开的现行 IR/类契约和历史语言规范，包含词法与完整 EBNF；中英文首页和研究索引都提供入口，历史规范顶部增加双语存档说明。当前源码仍使用 Python 构建器，旧文法没有已交付的独立解析器/编译器，现行契约未排独立表面文法的交付日期；这不替代先跑通实例和算法、后发展独立语言的意图。说明区分六形式 IR、类型出口、组合与执行机制和设计要求。

Validation: checked the added relative links and referenced API names against the current source; no runtime code changed and no live model calls were made. The archive notice is also retained in the workspace source so the existing documentation sync preserves it. / 验证：检查新增相对链接和当前源码中的 API 名称；没有修改运行时代码或调用真实模型。工作区原件同步保留同一存档说明，现有同步流程不会覆盖该说明。

## Available now / 现在可以使用

The public `0.1.0a1` snapshot contains a Python embedded implementation, judgment runtime, composition library and installed `jpp demo` command. [Download the alpha](https://github.com/Towow-ai/jpp/releases/tag/v0.1.0-alpha.1).

| Capability / 能力 | Evidence in this repository / 本仓库依据 |
|---|---|
| Questions as values / 问题可保存恢复 | `question_to_dict`, `question_from_dict`; composition tests |
| Compositions remain components / 组合继续参与组合 | Sequential, dynamic and nested component tests |
| Adaptive inquiry / 自适应提问 | Installed demo: 1,000 candidates, target 731, 10 synthetic answers |
| Candidate/check/feedback / 候选与反例反馈 | Installed demo: 3 trials, all 9 declared inputs checked |
| Local uncertainty / 局部未决 | Conditional count bounds and refinement tests |
| Independent method / 新方法接入 | `examples/independent_method.py` and its tests |
| Reproducible distribution / 可安装版本 | Wheel installation and offline demo verified outside the source tree |

The initial public commit passed 25 tests locally. Linux CI passed on Python 3.12 and 3.13. [Recorded CI run](https://github.com/Towow-ai/jpp/actions/runs/35500401184). These are mechanism and synthetic-observation tests, not a live-model accuracy evaluation.

## Work in the research workspace / 研究中的工作

### 2026-09-20: real-source relation recovery / 真实来源关系恢复

[325 人关系对照](https://towow-ai.github.io/jpp/demos/towow/real/)将七种组合放在同一批资料与 963 条旧标签上比较。固定每人前十，本地融合命中 675 条；J++ 相对排序命中 697 条，新增 52、丢失 30，净增 22。原始等级排序的持平与退步也完整保留。J++ relative ordering recovered 697/963 historical proxy relations at ten candidates per person versus RRF's 675; 52 added and 30 lost. All seven variants, including regressions, remain visible.

独立配对判断 6,500 次，内容去重后 5,353 个请求，175.796 秒，新增估算 $0.153491。全部真实来源实验共估算 $1.447708。有向前十仅改善 3 次，24 条跨来源种子关联仍未找到；这不是准确率、盲测或未来合作预测。[方法、结果及复现 / Method, results and reproduction](towow-real-relations-results.zh-CN.md)。

### 2026-09-20: executable discovery lab / 可执行的发现实验台

发布 216 个合成主体、20 种旧实验意图的[发现实验](https://towow-ai.github.io/jpp/demos/towow/population/)，以及可开关转介、组合、联系人和复用的[十人实验台](https://towow-ai.github.io/jpp/demos/towow/lab/)。网页通过浏览器 Python 执行现有 J++ 源码，模型层使用真实录制；完整介绍和动画保留。The discovery lab now covers 216 synthetic participants and 20 historical intents, with a separate component-intervention experiment. Both browser pages execute the repository's J++ Python implementation against exact recorded model responses.

真实运行：4,320 个判断，216 个后端请求，29.823 秒，估算费用 $0.039198，重复运行新增请求 0。第一版语义层级排序命中旧预期名单 33 次，BM25 43 次；在查看开发结果后加入“语义分组 + 词面排序”，得到 56 次。原始失败、改进及全部 20 条结果均保留；不是盲测或完整准确率。Live execution: 4,320 judgments in 216 requests, 29.823 seconds, estimated $0.039198, and zero additional requests on identical rerun. The initial semantic-tier ranking retrieved 33 expected aliases versus BM25's 43; the documented development revision combining semantic groups with lexical ordering retrieved 56. Original results and all queries remain available.

验证：本次全部 422 项测试通过；浏览器实际执行十人对照与百人程序，百人重复执行复用全部 4,320 个判断。构建 wheel，在源码目录外安装后，两项案例均成功运行。复现 `python -m jpp.towow_population`、`python -m jpp.towow_lab`；默认无模型调用。All 422 tests passed; both experiments ran in the browser and from an installed wheel outside the source tree. The repeated population run reused all 4,320 judgments. 下一步需要新增、未参与开发的意图和完整的人工关联标注 / Next: unseen intents and independently reviewed relevance labels. [方法与记录 / Method and records](towow-demo.zh-CN.md).

同池重跑旧 MiniLM 方法：整段向量 32 / 89，分字段向量 44 / 89；J++ 56 / 89 个已知关系被前十候选找回。分字段对照中 J++ 为 7 胜、10 平、3 负；向量查询阶段更快且无 API 费。Same-pool MiniLM reruns retrieved 32 and 44 of 89 known pairs, versus J++'s 56; query/index costs and losses remain visible. [对照说明 / Comparison](towow-discovery-comparison.zh-CN.md).

发布前同步并行更新的语言内核，重新打包与验证，全部 502 项测试通过。修复新增命令探针中的 macOS 专用 `sed -i ''`，使其同样能在 Linux 执行。Synced the concurrent kernel update and rebuilt the browser bundle; all 502 tests passed locally. The command probe now uses portable sed output replacement on macOS and Linux. 已定位用户提到的真实关系实验，新的真实资料调用尚未执行 / The historical real-source relation experiment has been located; a new live run is not yet performed. [下一组案例 / Next case](towow-real-relations-plan.zh-CN.md).

### 2026-09-20: animated graph with full explanations / 图谱动画与完整说明

`jpp towow` 生成的页面保留完整十人图谱和固定人物位置，通过节点发光与沿线移动的光点讲解四个阶段；图旁和下方保留完整段落介绍。支持暂停、重播、调速、选择阶段和人物依据查看。模型记录与计算方法不变，页面不产生新的调用。The viewer keeps the complete graph and fixed node positions, animating processing nodes and particles along edges through four stages. Full prose remains beside and below the graph, with playback controls and inspectable evidence. It reuses the existing execution record without changing the discovery method or making new model calls. [运行方法 / Run it](towow-demo.zh-CN.md).

已检查窄屏图谱、阶段切换、暂停时光点冻结、恢复后推进、人物资料与未确定候选；离线案例检查通过，wheel 在源码之外安装后能生成完整页面。本次验证覆盖录制案例的展示，不是新的模型评估。The graph layout, stage selection, frozen particles while paused, resumed progression, person evidence, and unresolved candidates were checked in the browser. The offline example check passed, and a wheel installed outside the source tree generated the complete page. This verifies the recorded example's presentation, not new model quality. 下一步关注首次观看者能否结合图谱与文字理解转介和组合 / Next: check whether first-time viewers understand referrals and composition using the graph and prose together.

[实现与检查 / Implementation and checks](https://github.com/Towow-ai/jpp/pull/2).

### 2026-09-20: first application example / 首个应用案例

The current main branch adds `jpp towow`: receiver-local judgments, a referral, a candidate combination, and subsequent discovery in ten fictional participants. The shipped recording comes from real `jev-1.13.0` requests; default replay is offline. The three-stage live run took 5.215 seconds, identical-input reuse made zero new calls, and changing one participant reused 16 of 32 judgments. A separate single-run comparison of identical first-stage request bodies measured 7.847 seconds sequentially and 1.042 seconds concurrently. These are demonstration measurements, not accuracy or large-network claims. [Proposal](first-problem-towow.zh-CN.md) · [Run and inspect the example](towow-demo.zh-CN.md).

Verification / 验收：本次案例在公开基础版本的独立检出中通过全部 31 项测试；构建 wheel 后，在源码目录之外安装并执行 `jpp towow` 成功，默认路径未访问网络。All 31 tests passed in an isolated checkout of the published baseline. The built wheel was installed outside the source tree and its offline `jpp towow` command completed successfully. This evidence does not cover a concurrent kernel upgrade / 本次验收不覆盖并行施工中的内核升级。

Runtime and composition development continue in a separate research workspace. The status below comes from maintainers' implementation notes inspected on the date above; it does not mean that new code has shipped here.

运行内核这条线正在完善三种题型的接口、执行与成本分析，并根据首次使用者暴露的疑问补全文档。组合这条线已实现两种算法构造器和第三个独立方法，正在完善演示、材料说明与接入交接。研究中的新结果会经过发布仓库的安装和测试后再同步。

## Next questions / 接下来要弄清楚

| Work / 工作 | Desired outcome / 想得到的结果 |
|---|---|
| Clearer semantics / 讲清运行规则 | A new reader can construct a method without guessing question, material or result formats |
| More algorithm constructions / 更多算法构造 | Different methods reuse the same primitives; repeated glue code becomes visible |
| Real judgment backend / 真实判断后端 | A reproducible example reports evaluation data, calibration, quality and cost |
| Composition rules / 组合规则 | Examples show which transformations preserve behavior and which effects constrain them |
| Surface syntax / 表层语法 | A small notation expresses demonstrated needs and runs against shared examples |

The full language is still taking shape. We do not assign a completion percentage. The current milestone is an executable alpha; the next evidence we want is broader reuse by people who did not design it.

## Update practice / 后续更新方式

Each update should identify the user-visible change, a command or example demonstrating it, its verification, and the next design question. Keep publication results separate from ongoing research. Date each update; old test counts do not automatically cover new commits.

### 2026-09-20: bilingual progress updates / 双语进展更新

We now publish each completed, verified advance to GitHub with Chinese and English commit messages and progress notes. Follow the [commit history](https://github.com/Towow-ai/jpp/commits/main/) to see what changed and the [maintenance practice](maintaining.md) for how updates are prepared. This update adds documentation only; the runtime and existing verification results are unchanged. The next entries will link completed implementation milestones to their examples and checks.

今后每完成一项可验证的实际进展，就同步 GitHub，并提供中英双语提交说明和进度记录。关注者可以通过[提交历史](https://github.com/Towow-ai/jpp/commits/main/)了解改变，通过[维护约定](maintaining.md)了解更新方式。本次只更新文档，运行代码未变；后续进展会附上对应成果、用法和验证依据。

### 2026-09-21: fusion by mechanism, exits across frames, ten runnable probes / 融合按机制成立、出口跨帧、十条可运行探针

Two compiler passes (`speculate`, `vectorize`) make judgment fusion independent of loop style; unresolved exits can be returned from annotated programs; ten runnable probes ship with offline and live modes. 494 tests pass on Python 3.12 and 3.13 in this repository. Details, commands and numbers: [2026-09-21 update](updates/2026-09-21-mechanized-fusion-and-probes.md); the previous kernel sync is in the [2026-09-20 update](updates/2026-09-20-kernel-research-sync.md).

两个新编译 pass 让判断融合不再取决于循环写法；带返回注解的程序可以把「拿不准」交给调用者；十条可运行探针带离线与真机两种模式。本仓库在 Python 3.12 与 3.13 上各通过 494 项测试。细节见 [2026-09-21 更新](updates/2026-09-21-mechanized-fusion-and-probes.md)，上一次内核同步见 [2026-09-20 更新](updates/2026-09-20-kernel-research-sync.md)。

### 2026-09-21: a falsified scenario, a calibration reading, and the Rust kernel starting / 一个被证伪的场景、一次校准读数、Rust 内核开工

No new runtime capability this round. E9f-2b′ **failed**: predicting which paragraphs an author will ask to change is not decidable in one literal hop (n = 1,564 paragraphs, AUC 0.541 against a bet of 0.70; a length-and-digits heuristic scored higher at ~~0.638~~ **[0.643 — see the correction entry below]**, and a `haiku` judge sat on the random line too). E-CAL's final run passed none of its falsification criteria and about half its bets: Chinese `noul` readings are usable as probabilities (ECE 0.057), ~~`choice` showed no first-position bias (permutation consistency 1.000)~~ **[withdrawn 2026-09-21 — measurement artefact; see the correction entry below]**, `score` hit the adjacent band 0.964 — while `noul` AUC (0.748), `choice` argmax (0.757) and `score` MAE (0.553) all came in under their bets, and the 300 items turned out to be built from about 45 independent paragraphs, so effective n is 17–26 rather than 100. The formal kernel moves to Rust ([ADR 0001](adr/0001-rust-kernel.md)); its core is still being built in the research workspace and ~~**no Rust source is published yet**~~ **[overtaken the same day — a Rust workspace landed under `rust/` on the front-end line; see the Status block in that update]**. 535 tests pass on Python 3.12 and 3.13 in this repository. Details and every number: [2026-09-21 update](updates/2026-09-21-two-experiments-and-rust-start.md).

本轮没有新的运行能力。E9f-2b′ **失败**：段级预测「作者会要求改这一段吗」在一跳字面下不可判（n = 1,564 段，AUC 0.541，赌的是 0.70；长度加数字的启发式基线反而更高，~~0.638~~**〔更正为 0.643，见下方更正条目〕**；haiku 裁判同样在随机线上）。E-CAL 正式版三条证伪判据一条都没触发、赌值对了一半：中文 `noul` 读数可以当概率用（ECE 0.057），~~`choice` 无首位偏置（置换一致 1.000）~~**〔2026-09-21 作废：测量假象，见下方更正条目〕**，`score` 相邻档 0.964；但 `noul` AUC 0.748、`choice` argmax 0.757、`score` MAE 0.553 全部低于赌值，且 300 条题面只由约 45 个独立段落重组而成，有效 n 在 17–26 之间而不是 100。正式内核转 Rust（[ADR 0001](adr/0001-rust-kernel.md)），core 仍在研究工作区建设中，~~**Rust 源码尚未公开同步**~~**〔当日即被事实追上：前端那条线已把 Rust 工作区推到 `rust/` 下，见该更新的「现状」块〕**。本仓库在 Python 3.12 与 3.13 上各通过 535 项测试。细节与全部数字见 [2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)。

### 2026-09-21: correcting a published result / 更正一条已发布的结论

Two `choice` results published in the entry above are withdrawn. Of the 97
`select` items in E-CAL, only 8 emitted a `choice` physical question; the rest
had long candidates and were lowered by the compiler to per-candidate `noul`,
whose `mode_share` is hard-coded to 1.0 — which is exactly the permutation
consistency test, so 67 of 74 items were vacuously consistent. The real
measurement is 7 items, and "no first-position bias" is void because per-candidate
`noul` has no position at all; on the 8 items that really ran `choice` the first
candidate was chosen 3 times against a ground-truth rate of 1, pointing toward
bias rather than away from it. The same round corrects E9f-2b′'s accounting:
cost $0.0027 → **$0.045** over 1,633 calls (a telescoping `reset_stats()` bug in
the experiment script, not an overspend — both caps held), C_S ≈ 5.5 s withdrawn,
the random arm **indistinguishable** from Jev rather than better, and the `haiku`
price a range of $1.3–$12.9. **Both falsification verdicts stand**; E9f-2b′ is
still a FAIL. The wrong sentences are kept in place and marked, not deleted:
[2026-09-21 update](updates/2026-09-21-two-experiments-and-rust-start.md) carries
a Correction block in each section, and four research documents were re-synced,
including the pre-registrations for the two follow-up experiments. Documentation only — `src/` unchanged; 544 tests pass on Python 3.12
and 3.13.

上一条里两句 `choice` 结论作废。E-CAL 的 97 条 `select` 只有 8 条真的发出 `choice`
物理题，其余因候选过长被编译器下沉成逐候选 noul，而 K-noul 的 `mode_share` 被写死为
1.0——「置换一致」的判据恰好就是它，所以 74 条里 67 条是恒真项，真测量只有 7 条；
「无首位偏置」直接无效，因为逐候选 noul 根本没有位置，在真跑了 choice 的 8 条上首位
被选 3 次、真值首位 1 次，方向反而朝着有偏置。同一轮还更正 E9f-2b′ 的账：花费
$0.0027 → **$0.045**、调用 1,633 次（实验脚本的 `reset_stats()` 望远镜求和，不是超
支，两条预算上限都没破）、C_S ≈ 5.5 s 作废、随机臂与 Jev **不可分辨**而非更好、haiku
价格改为区间 $1.3–$12.9。**两个证伪结论都不变**，E9f-2b′ 仍是 FAIL。错的句子留在原处
标明，不删：[2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)
每节加了更正块，四个研究文件一并重新同步（含两个后续实验的预注册）。本次只改文档，`src/` 未动；Python 3.12 与
3.13 各通过 544 项测试。

# 2026-09-20 — Install, compose and inspect complete methods / 安装并使用完整方法

The `0.1.0a2` developer package adds `jpp methods --output report.json`, the
accepted common planner support, and a [developer guide](developer-guide.md).
The command runs dynamic method construction, nesting and internal replacement
from an installed wheel; no research paths or snapshot are required.

`0.1.0a2` 将已验收完整/动态组合接入安装包。新增命令输出共同计划、实际生成结构与结果；明确文件清单防止未验收语义函数、提示和原始生成记录混入发布。

Verified in a fresh Python 3.12 environment outside the checkout: installed
module paths point to site-packages, `jpp demo` and `jpp methods` execute, and the
wheel excludes unreviewed modules. Focused compatibility and delivery checks:
41 + 33 passed; the full release suite also passed all 512 tests in the installed
Python 3.12 environment. An independent author used only the public guide and installed
package to write [a new flaky-test method](../examples/flaky_method.py), then
passed and nested the complete method without changing the library. Three inputs
passed; the guide now includes the JSON-export example that author needed.

干净安装、命令与新作者程序均实际运行。示例使用固定观察或精确计算，不是模型准确率、通用性覆盖或真实增益实验。下一步继续依据实际表达缺口与原计划推进。

[Installed-package evidence](demos/methods/installation.json) ·
[Method execution](demos/methods/method-report.json) ·
[Independent author output](demos/methods/independent-output.json)

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/fbf2456)

Linux CI exposed a BSD-only `sed -i ''` in an existing shell probe. The candidate
now uses portable output-and-rename syntax; the goal and truth assertions remain
unchanged. A direct file-content regression passes. The final wheel includes
[this portability fix / 可移植修复](https://github.com/Towow-ai/jpp/commit/567c742).

The concurrent browser-demo update is preserved. Its existing source-bundle
builder was rerun so browser code and the installed package use the same reviewed
sources; the artifact-consistency checks pass.
并发浏览器演示已保留，沿原脚本重建源码包，避免网页继续加载旧计划器。

## 2026-09-20: composable partial results and continuation / 可组合的部分结果与继续求解

`0.1.0a3` adds `Partial`, `checkpoint`, `map_partial` and `continue_with` to the
existing composition library. A caller can use a sufficient candidate combination
while some judgments remain unresolved, then change strategy and continue through
the same exact combination algorithm. No new runtime or kernel changes.

现在可以先拿到成本9的可用组合，C/D仍未决；只补问C就得到成本2的组合，之后换策略
处理D。A/B/C各检查一次。`jpp partial --output partial-report.json` 实际运行整个过程，
再与直接手写控制程序对照，结果、证据及观察/动作次数一致。

[Guide and direct comparison / 用法与对照](partial-results.md) ·
[Execution record / 执行记录](demos/partial/partial-report.json) ·
[Installation / 安装记录](demos/partial/installation.json)

Verified: the full release suite passed 534 tests in Python 3.12; the subsequently
added independent-author regression passed separately. The wheel was installed
and run outside the source tree, including the documented caller. An independent
author read only the public docs and wrote an expensive-first, one-at-a-time
strategy, composed with then/product/iterate/bind; it uses the same algorithms and
protocol. Its new caller requires both cost <= 2 and no remaining questions.

验证覆盖干净安装、完整程序、内核预算、观察/材料身份、已执行工作保留及新策略再组合。
独立作者的统计字段疑问已补入文档。固定观察下总计6次调用、3次本地检查、2次材料
更新；精确组合投影仍会重算。这是接口复用与执行语义验证，不是模型质量提升实验。
下一步由新的算法使用需求决定，跨进程继续方法持久化尚未加入。

[Independent strategy / 独立策略](../examples/priority_resume.py) ·
[Its output / 实际输出](demos/partial/independent-output.json)

[Implementation and evidence commit / 实现与依据提交](https://github.com/Towow-ai/jpp/commit/4e19377)
