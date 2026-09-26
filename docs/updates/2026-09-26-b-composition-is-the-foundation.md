# Composition is the foundation: JEV paired with a generator, an executor, and exact graph algorithms -- and the pairings nest / 搭配是底层：JEV 分别与生成器、执行器、精确图算法搭配——而且这些搭配还能互相嵌套

2026-09-26 (second sync of the day). Research-workspace construction, research-tree commits `2eb748dc` through the current private main head. Part of today's second [daily sync](../progress.md).

## English

J++'s stated design intent is that JEV -- the cheap, fast, calibrated judgment primitive -- is not the whole language; the language's actual foundation is JEV *paired with* a generator, an executor, or an exact algorithm, and that a pairing itself becomes an element that can be composed into something larger, without limit. Today's sync lands the concrete evidence for that claim as running code: three library modules, each pairing JEV with a different kind of component, and one example program where all three pairings nest inside a single closed loop.

**JEV paired with a generator (`gen`), now backed by a real model.** The minimal shape of this pairing already existed as `gen` returning a list of candidate materials that `sieve` then judges (see `examples/gen-choose.jpp`: propose three coffee-shop names, keep the ones JEV judges to fit). What changed today is that `gen` now has a real backend: `--gen-model` wired to `claude -p` through a non-blocking submit/poll thread pool, so a program's other judgments and executions keep moving while a generation call is in flight rather than blocking on it -- the concrete form of this project's standing rule that "the language schedules; the author does not." A generation call is registered where it's written but only actually sent out at the next refresh point in its layer, and only *waited on* when something reads its result -- so a generated value that a program branch never ends up reading is never paid for, and a judgment scheduled after a `gen` call in the same layer doesn't wait for the generation to resolve to be sent out. Generated output carries an explicit trust tag (`untrusted`, stated in the generator's own capability profile) that propagates through everything built from it, the same taint mechanism every other value in the language already carries. `--gen-cache` lets repeated runs against the same generation site skip paying for it again. A one-time live run cost `$0.00004` (one `claude -p` call producing three candidates, three JEV judgments) and reproduced exactly on replay from the recorded ledger with zero new calls.

**JEV paired with an executor (`ground`), closing an execute-then-judge loop.** `lib/compose/ground.jpp` wraps any registered executor action (`exec_py`, `check_tests`, or any future one) into a single function: run the action, render its output and the input together into the literal text a question can be asked about, and hand the result to JEV. If the action fails -- including, now, "no sandbox available" -- `ground` returns the failure value rather than crashing the program, and `sieve` treats that candidate as undecided rather than stopping everything. `verify(candidates, ground, question)` is the resulting one-line "run this, then judge whether it worked" primitive: `sieve(map(candidates, ground), question)`.

**JEV paired with exact graph algorithms (`lib/compose/graph.jpp`), where the graph's edges are themselves judgment outcomes.** A program can ask JEV to judge which pairs of nodes should be connected, treat the accepted pairs as a graph's edges, and run an exact algorithm (matching, shortest path, a clique, and so on -- the six `graph:*` actions this repository already carries) on that judged graph. Edges JEV hasn't decided on yet are handled by running the exact algorithm twice -- once assuming they're absent, once assuming an "optimistic" graph where they're present -- and the two results are compared: wherever they disagree is exactly the set of edges whose resolution would change the answer, which is what a program should ask about next rather than everything. The graph algorithms never see a raw probability as an edge weight (a project rule stated from the start: "judgment produces decisions, not numbers that flow into arithmetic"); a judged output's exit -- the record of which judgments the graph algorithm's result actually depended on, act/ignore/unsure and all -- is now produced by a general-purpose primitive opened today as part of this work: `compose(exits, rule)` combines a list of individual judgment exits into one exit under a rule (`any`, `all`, `min`, or the first/highest-ranked one), the way a hand-written aggregation would fold several booleans into one, except the combined result still carries the same three-way undecided semantics and the same statistical error-bound bookkeeping as any single judgment -- readable back out with the also newly opened `cert(exit)`, which reports a certified error bound, an outstanding-uncertainty count, and the underlying certification grade for any exit, single or composed.

**The pairings nest -- generator, executor, and JEV together in one closed loop.** `lib/compose/search.jpp`'s `search` function is JEV paired with the generator in a different shape than `gen-choose`: instead of asking for candidates once, it runs propose-then-judge in rounds, keeping every candidate judged good so far, feeding the not-yet-good ones back to the generator with an instruction to try again, and stopping once enough good candidates have accumulated or the generator stops producing new ones. `search` accepts an optional `ground` function in its options -- and passing `ground` from the executor pairing above turns "generate candidates, then judge them" into "generate candidates, *run* them, then judge the *results*." The example this sync adds, `examples/search-ground.jpp`, does exactly this for a small, concrete task:

```jpp
import "../lib/compose/ground.jpp";
import "../lib/compose/search.jpp";
budget {calls: 20, cost: 0.01, depth: 64};

let brief = mat("写一段 Python 代码，打印 1 到 10 的平方和");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 段 Python 代码，每段是一个字符串；需求之后的上下文是上一轮留下的候选（代码与输出）",
        concat([brief], frontier), 3, i)
};
let runner = ground("exec_py", fn(c) { [content(c), "", 5] },
                    fn(c, out) { "代码：" + content(c) + "\n输出：" + content(out).stdout });
let correct = test("这段代码的输出是否等于 1 到 10 的平方和（385）？", "ground-correct");

let r = search([], propose, correct, unit, 3, {width: 1, unsure_to: "refine", ground: runner});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }),
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 undecided: map(r.pending, fn(p) { {item: p.item, cause: p.cause, via: p.via} }),
 reason: r.detail.reason}
```

In English: "ask an LLM to write Python code that prints the sum of squares from 1 to 10, run each candidate, ask JEV whether the output is right (385), and if none are, show the generator its own failed attempts and let it try again, up to 3 rounds, until one candidate is accepted." On the recorded run behind this example's test fixture, round 1 produced three candidates (all wrong, one landing close enough to the threshold that it was kept as context rather than discarded), round 2's revised attempt produced the correct output, and the loop stopped -- 14 total calls (2 generations, 6 executions, 6 judgments), reproduced from the recorded ledger with zero new calls on replay, and confirmed to behave identically whether or not this machine's OS-level sandbox is available (this sync's test suite runs it both ways). This is three of this project's foundational pairings -- JEV+generator, JEV+executor, and the generator's own multi-round search behavior -- composed into one closed loop, in nineteen lines of `.jpp` source, with none of the retry logic, prompt-history bookkeeping, or process-management code an equivalent hand-written script would need to write itself.

**What else landed alongside the composition-layer work.** An author's declared policy line on `cut` (from the previous sync's design ruling, `declare:{hi, lo?}`) is now backed by code end to end: a host can explicitly accept a declared line for licensing an irreversible action (`--release-on-declared`, hashed into the run's `entry_hash` so accepting it is itself an auditable fact), and `cut` gained a `stat` option so a declared line can also gate on a judgment's aggregate statistic (its expected value across score buckets, or the total probability mass in a chosen subset) rather than only its single top answer, with open or closed comparison at either end of the line. Ledger writes are now durable line by line rather than only at the end of a run, and an irreversible action's intent is recorded before it executes, so a killed or crashed process leaves a resumable, honest record instead of ambiguity about whether it actually ran (`E-ledger-required` when a host tries to run without ledger durability at all). 22 new built-in functions cover text and data operations (string splitting/case/trim/replace, regex, sorting, JSON parse/serialize, hashing, date parsing/formatting) plus seeded, reproducible randomness (`rand`, `rand_int`, `shuffle`), all following the same trust-propagation rule as everything else in the language. Judgments on the same material that ask different questions are now batched into a single request whenever they can be, instead of the previous scheme that occasionally required issuing more calls than a hand-written program would for the same task.

**A worked guide, checked to run.** `rust/GUIDE.md` gained a full chapter, "Pairings: element -> composition -> nesting" ("搭配：元素 → 组合 → 嵌套"), walking through every element-level pairing (JEV with a generator, an executor, retrieval, judged text, a declared line and its accept/reject/stat variants), the composition-level constructs this entry describes (`search`, `ground`+`verify`, judged graphs, `compose`+`cert`), and two nesting examples (searching over multiple independent topics, and running one judged graph's output through a second graph). Every code sample in that chapter is a real, checked-in file under `rust/examples/guide/`, and `rust/scripts/guide_check.py` runs every one of them (`check`, and `run` where the example is meant to execute) and diffs the chapter's prose against the actual file contents so the two cannot drift apart silently -- 20 of 20 example cases pass, and the chapter's 13 embedded code blocks match their source files byte for byte, confirmed as part of this sync's own verification pass.

**Also landed today: score-type judgments get a real ranking rule.** `order`, which turns a set of judgment readings into ranked tiers, previously ranked a scored (multiple-choice-by-degree) judgment by its single most-probable bucket's probability -- the right rule for a yes/no judgment, but not for "how good is this on a scale," where a confident answer in the lowest bucket would rank ahead of a middling answer spread across two adjacent buckets. `order` now takes the same statistic option `cut` does (an expected value across buckets, the bucket index itself, or the judgment's self-reported confidence), and defaults to ranking by bucket position for scored judgments rather than by top-bucket probability -- the ranking rule `search`'s objective-based sorting was blocked on until this landed (research step 25-8a's open question). A single select-type judgment (as opposed to a set of them) can also now be tiered directly into groups of equally-good candidates, without going through a full aggregate construct.

**Not covered here (already landed in [PR #36](https://github.com/Towow-ai/jpp/pull/36)):** the OS-level sandbox for `exec_py`/`check_tests`/`exec_sql` and the other executor-security fixes -- see that PR and the [previous update](2026-09-26-composition-layer-actions.md) for the full account, not repeated in this entry.

**Verification.** See [progress.md](../progress.md) for the combined pass count, fmt/clippy results, and CI status for this sync as a whole.

## 中文

J++ 明说的设计意图是：JEV——那个便宜、快、带校准置信度的判断原语——不是整门语言，语言真正的底层是 JEV **和**生成器、执行器或精确算法**搭配**，而且一次搭配本身能成为一个元素，被无限地组合进更大的构造里。今天这次同步把这条主张落成了跑得起来的代码：三个库模块，各自把 JEV 与一类不同的组件搭配起来，还有一个示例程序，把三种搭配全部嵌进同一个闭环。

**JEV 与生成器（`gen`）搭配，现在接了真的模型。** 这种搭配的最小形态早就有——`gen` 返回一批候选材料，`sieve` 拿去判（见 `examples/gen-choose.jpp`：提三个咖啡馆店名，留下 JEV 判定合适的那些）。今天改变的是 `gen` 有了真的后端：`--gen-model` 接到 `claude -p`，经一个非阻塞的提交/轮询线程池——程序里其余的判断与执行在一次生成调用飞着的时候照常往前走，不会被它卡住，这正是本项目一贯规则「语言来调度，作者不用管」的具体落地。一次生成调用在写下的地方就登记，但只在它所在层的下一个刷新点才真正发出；只有当有东西读它的结果时才会真的**等**它——所以一个程序分支最终没读到的生成值不会花钱，同一层里排在 `gen` 调用之后的判断也不需要等生成解析完才能发出。生成出来的材料带着明确的可信标签（`untrusted`，写在生成器自己的能力画像里），这个标签会随着用它构造出的一切一路传播下去，和语言里其他任何值走的是同一套 taint 机制。`--gen-cache` 让针对同一个生成站点的重复运行不用再花一次钱。一次真机运行花费 0.00004 美元（一次 `claude -p` 调用产出三个候选、三次 JEV 判断），从记录的账本重放时零新增调用、结果完全一致。

**JEV 与执行器（`ground`）搭配，接成一个「跑完再判」的闭环。** `lib/compose/ground.jpp` 把任何已登记的执行器动作（`exec_py`、`check_tests`，或者以后任何新加的）包成一个函数：跑这个动作，把它的输出与输入一起渲染成能被问一道题的字面文本，再把结果交给 JEV。动作失败时——现在也包括「没有可用沙箱」——`ground` 原样交出失败值而不是让程序崩掉，`sieve` 把这个候选当未决处理，不会因此停掉整个程序。`verify(候选集, ground, 题)` 就是由此得到的一行原语——「跑一遍，再判它对不对」：`sieve(map(候选集, ground), 题)`。

**JEV 与精确图算法（`lib/compose/graph.jpp`）搭配，图的边本身就是判断的产物。** 程序可以让 JEV 判哪些节点对该相连，把判定为「有」的那些当成图的边，再在这张判出来的图上跑一个精确算法（匹配、最短路、找团……本仓库已有的六个 `graph:*` 动作）。JEV 还没判完的边，做法是把精确算法跑两遍——一遍假设这些边不存在，一遍假设一张「乐观图」（这些边都存在）——再比较两次的产物：哪里不一致，哪里就恰好是「判定结果会改变答案」的那批边，这才是程序接下来该去问的，而不是不分青红皂白地全问一遍。图算法从不把一个原始概率当成边权来用（这是项目从一开始就定下的规则：「判断产出的是决定，不是能流进算术的数字」）；一个判出来的产物的出口——记录着这个图算法的结果到底依赖了哪些判断、act/ignore/unsure 各是什么——现在由今天一并开放的一个通用原语产生：`compose(exits, rule)` 按一条规则（`any`、`all`、`min`，或取第一个/排名最高的那个）把一批独立的判断出口合成一个出口，就像手写代码会把好几个布尔值折叠成一个那样，只是这个合成结果仍然带着和单个判断完全一样的三值未决语义，也一样带着统计误差界的记账——用同样今天开放的 `cert(出口)` 能把它读出来，对任何出口（单个的或合成的）都能报出经过认证的误差界、还剩多少不确定性，以及这个认证属于哪个等级。

**搭配还能互相嵌套——生成器、执行器与 JEV 一起进同一个闭环。** `lib/compose/search.jpp` 的 `search` 函数是 JEV 与生成器的另一种搭配形态：不是一次性要一批候选，而是分轮跑「提出 → 判」，把至今判好的候选都留着，把还没好的候选连同上一轮的结果一起喂回生成器让它重提，直到攒够好候选或生成器不再产出新的为止。`search` 的选项里能给一个 `ground` 函数——把上面执行器搭配得到的 `ground` 传进去，「提出候选再判」就变成了「提出候选、**跑**它、再判**跑出来的结果**」。这次同步加的示例 `examples/search-ground.jpp` 就是照这个思路做一件具体小事：

```jpp
import "../lib/compose/ground.jpp";
import "../lib/compose/search.jpp";
budget {calls: 20, cost: 0.01, depth: 64};

let brief = mat("写一段 Python 代码，打印 1 到 10 的平方和");
let propose = fn(frontier, i) {
    gen("按下面的需求提出 3 段 Python 代码，每段是一个字符串；需求之后的上下文是上一轮留下的候选（代码与输出）",
        concat([brief], frontier), 3, i)
};
let runner = ground("exec_py", fn(c) { [content(c), "", 5] },
                    fn(c, out) { "代码：" + content(c) + "\n输出：" + content(out).stdout });
let correct = test("这段代码的输出是否等于 1 到 10 的平方和（385）？", "ground-correct");

let r = search([], propose, correct, unit, 3, {width: 1, unsure_to: "refine", ground: runner});
{kept: map(r.value, fn(e) { e.item }), found_in: map(r.value, fn(e) { e.round }),
 rejected: map(r.detail.ignore, fn(e) { e.item }),
 undecided: map(r.pending, fn(p) { {item: p.item, cause: p.cause, via: p.via} }),
 reason: r.detail.reason}
```

翻成大白话：「让大模型写打印 1 到 10 平方和的 Python 代码，把每份代码都跑一遍，问 JEV 输出对不对（385），如果都不对，就把生成器自己失败的尝试原样给它看、让它再试一次，最多三轮，直到有一个候选被接受为止。」在这个示例测试夹具背后录下的那次运行里，第 1 轮产出三个候选（全错，其中一个恰好落在阈值附近，被留作下一轮的上下文而不是直接丢弃），第 2 轮改写的尝试产出了正确输出，循环停止——一共 14 次调用（2 次生成、6 次执行、6 次判断），从记录的账本重放时零新增调用，而且不管本机有没有操作系统级沙箱，行为都一样（本次同步的测试套件两种情况都跑过）。这是本项目三个底层搭配——JEV+生成器、JEV+执行器，加上生成器自身的多轮搜索行为——嵌进同一个闭环里，只用了十九行 `.jpp` 源码，不需要写任何重试逻辑、提示词历史记账或进程管理代码——等价的手写脚本这些都要自己写。

**搭配层施工之外，同一批还落地的东西。** 作者在 `cut` 上声明的策略线（上次同步的设计裁定，`declare:{hi, lo?}`）现在有代码从头到尾接通：宿主可以显式接受一条声明线，用它放行不可逆动作（`--release-on-declared`，进这次运行的 `entry_hash`，接受本身就是一个可审计的事实）；`cut` 加了 `stat` 选项，让一条声明线除了只按单一最高档答案判定外，也能按一个判断的汇总统计量（跨打分档位的期望值，或某个候选子集上的概率总和）判定，线的两端各自可开可闭。账本写入现在逐行落盘而不是只在一次运行结束时才写，一个不可逆动作在执行前先记下意向，进程被杀掉或崩溃后留下的是一份可续接的、如实的记录，而不是「到底跑没跑」的悬念（宿主完全不支持账本落盘时报 `E-ledger-required`）。22 个新增内置函数覆盖文本与数据操作（字符串切分/大小写/去空白/替换、正则、排序、JSON 解析/序列化、哈希、日期解析/格式化），加上带种子、可复现的随机数（`rand`、`rand_int`、`shuffle`），全部遵循语言里其他值同一套可信传播规则。对同一份材料问不同题的判断，现在只要能合并就会被批进同一次请求，不再像此前那样偶尔会比手写程序发出更多次调用。

**一份跑得通的指南。** `rust/GUIDE.md` 新增完整一章「搭配：元素 → 组合 → 嵌套」，逐一讲清元素级搭配（JEV 与生成器、执行器、检索、判过的文本、声明线及其接受/拒绝/统计量三种变体）、本文讲的组合级构造（`search`、`ground`+`verify`、判出来的图、`compose`+`cert`），以及两个嵌套例子（对多个互相独立的话题分别搜索、把一张判出来的图的产物再喂进第二张图）。这一章里的每段代码都是仓库里真实签入的文件（`rust/examples/guide/`），`rust/scripts/guide_check.py` 把每一段都跑一遍（`check`，示例意在真跑的再 `run`），并把这一章的正文与文件实际内容逐字比对，防止两者悄悄脱节——20 个示例用例全部通过，这一章嵌入的 13 段代码块与各自源文件逐字节相同，本次同步自己的验证里核实过。

**同一天还落地的：打分型判断有了真正的排序规则。** `order` 把一批判断读数变成有排名的档位；此前它对一个打分（有序多选）判断只按它最高概率那一档的概率排——这对是非题是对的规则，但对「这个东西打几分」这类问题不对：一个自信落在最低档的答案会排在一个横跨相邻两档、态度中庸的答案前面。`order` 现在接受与 `cut` 同一个统计量选项（跨档位的期望值、档位本身，或判断器自报的置信度），并且对打分判断缺省按档位排、不再按最高档概率排——这正是 `search` 按目标题排序此前一直被卡住等待的那条规则（研究步 25-8a 留下的悬问）。单条选择题判断（而不是一批）现在也能直接分档成若干组同等好的候选，不用绕经完整的聚合构造。

**本文没有覆盖的部分（已经在 [PR #36](https://github.com/Towow-ai/jpp/pull/36) 落地）：** `exec_py`/`check_tests`/`exec_sql` 的操作系统级沙箱与其余执行器安全修复——完整说明见那个 PR 与[上一篇更新](2026-09-26-composition-layer-actions.md)，本文不重复。

**验证。** 本次同步整体的合计通过数、fmt/clippy 结果与 CI 状态见 [progress.md](../progress.md)。
