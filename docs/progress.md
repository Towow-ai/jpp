# J++ progress / 项目进度

Updated: 2026-09-26. This is a dated report, not an automatically updated dashboard.

## 2026-09-26 (second daily sync): composition primitives open up (search, ground, judged graphs), a real generator backend, durable ledger writes, declared-line maturation, batching / 第二次每日同步：搭配原语开放（搜索、接地、判出来的图）、生成器接真后端、账本落盘、声明线成熟、判断合批

Second sync of the day (see this same day's first entry, further below in this file, for the executor/graph action library and its PR #36 security hardening, not repeated here). This one takes private main from `2eb748dc` (where the first sync of the day stopped) through the current private main head, syncing everything else that landed today. A dedicated write-up on the composition layer -- the main substance of this entry -- with a worked example nesting three pairings in one closed loop, is in [today's second update](updates/2026-09-26-b-composition-is-the-foundation.md); this entry covers the same ground plus everything else.

**The composition layer opens: `search`, `ground`, judged graphs, and two new general-purpose primitives underneath them.** `lib/compose/ground.jpp` pairs JEV with any registered executor action (`exec_py`, `check_tests`, future ones) into a single "run it, then judge whether it worked" function; a failing action (including "no sandbox available") returns a failure value that `sieve` treats as undecided rather than a crash. `lib/compose/search.jpp`'s `search` runs propose-judge-repropose in rounds, accumulating good candidates across rounds (a same-day design correction over the initial per-round-only accumulation), handing not-yet-decided candidates to a `carry`/`refine`/hand-to-a-human policy, and terminating via the same bounded-iteration machinery every other construct in the language uses. `search` accepts an optional `ground` function, so pairing the two turns "generate, then judge" into "generate, run, then judge the outcome" -- demonstrated in a new example, `examples/search-ground.jpp`, where a generator proposes Python code, `exec_py` runs it, and JEV checks the output, looping until a correct candidate is found. `lib/compose/graph.jpp` treats JEV-judged pairs of nodes as a graph's edges and runs an exact algorithm (the six `graph:*` actions) on it, running the algorithm twice for edges not yet decided (once absent, once present) so the two results' disagreement marks exactly which edges are worth asking about next. Underneath all three, two general-purpose primitives opened today: `compose(exits, rule)` folds a list of judgment exits into one under a rule (`any`/`all`/`min`/first-or-highest), keeping the same three-way undecided semantics and statistical error-bound bookkeeping a single judgment carries; `cert(exit)` reads a certified error bound, an outstanding-uncertainty count, and the certification grade off any exit, single or composed. `element(input, exit, ctx)` was opened as a further primitive for building a single graph/search element from a judgment outcome without going through a full aggregate construct.

**A real generator backend, and it doesn't block the rest of the program.** `gen` now has a working port to `claude -p` (`--gen-model`), dispatched through a non-blocking submit/poll thread pool: a program's other judgments and executions keep moving while a generation call is outstanding. A generation call registers where it's written but is only sent at the next refresh point in its layer, and is only waited on when its result is actually read -- so a generated value a branch never reads is never paid for. Generated output carries an explicit `untrusted` trust tag from the generator's capability profile, propagated through everything derived from it by the same mechanism every other value's trust bit uses. `--gen-cache` skips paying for a repeated generation at the same site on a later run. One live run of the minimal `gen`+`sieve` example (`examples/gen-choose.jpp`) cost $0.00004 (one `claude -p` call, three JEV judgments) and replayed identically from the recorded ledger with zero new calls.

**Author-declared lines gain a host-acceptance path and a richer statistic.** A host can now explicitly accept an author-declared line (from the previous sync's ruling) to license an irreversible action (`--release-on-declared`, hashed into the run's `entry_hash` so accepting it is itself an auditable fact) instead of the run refusing outright. `cut` gained a `stat` option so a declared line can gate on a judgment's aggregate statistic -- an expected value across score buckets, or the total probability mass across a chosen subset of a multiple-choice answer -- rather than only its single top answer, with the comparison open or closed independently at each end of the line; a hand-audited default (rather than a derived one) keeps both ends closed unless explicitly opened, after a review flagged that a derived default would have silently done the opposite.

**Ledger writes are now durable line by line, not only at the end of a run.** Previously a crashed or killed process could leave no usable record of what it had done. Every ledger entry is now flushed to disk as it's written rather than batched until the run finishes, and an irreversible action's intent is recorded before it executes so a later resume can tell what was attempted even if the process died mid-action; a host that can't provide durable ledger storage at all is refused outright (`E-ledger-required`) rather than silently running without the safety property.

**22 new built-in functions for text and data, plus seeded randomness.** String operations (split, case conversion, trim, replace, prefix/suffix checks, character indexing), regular expressions, sorting (plain and by a custom key, both stable), JSON parse/serialize round-tripping through the same canonical form the ledger uses, content hashing through the same function ledger keys use, and date parsing/formatting/arithmetic -- plus `rand`/`rand_int`/`shuffle`, seeded and reproducible (documented as a fixed, versioned algorithm rather than "whatever the standard library does today," so a program's behavior doesn't silently drift under a future compiler/library upgrade). All follow the same trust-propagation rule as every other value in the language: untrusted input in, untrusted output out.

**Judgments on the same material batch together more often.** Previously, a yes/no question and a multiple-choice question about the same underlying material could end up issued as separate calls even when a hand-written program would combine them into one; candidates now travel with the question they're being asked about and get grouped by the material they're actually judging, closing that gap. This is a rendering-format change (bumped to a new render version, `r2`) that only affects the judgment request's on-the-wire shape, not any answer's meaning; an older recorded ledger written under the previous format still replays correctly (flagged, not silently reinterpreted), but cannot be resumed with new calls under the new format without an explicit re-recording step.

**A checked-in, checked-to-run guide, and a ranking fix it was waiting on.** `rust/GUIDE.md` gained a full chapter, "Pairings: element -> composition -> nesting," walking through every pairing this entry describes with real, checked-in example files under `rust/examples/guide/`; `rust/scripts/guide_check.py` runs and cross-checks every one of them against the chapter's prose so the two can't silently drift apart -- 20 of 20 example cases pass. Alongside it, `order` (which turns a set of judgment readings into ranked tiers) gained the statistic option `cut` already has and now defaults to ranking a scored judgment by its bucket position rather than its single most-probable bucket's raw probability -- the correct-ranking behavior `search`'s objective-based sorting had been left without since the composition layer opened. Full account of both in [today's second update](updates/2026-09-26-b-composition-is-the-foundation.md).

**Verification.** `cd rust && cargo fmt` (one file needed reformatting) then `cargo build --locked --workspace` succeed. `cargo clippy --workspace --all-targets --keep-going`: 72 warnings, matching this repository's current baseline (no new warnings anywhere in this sync). `rust/scripts/guide_check.py`: 20/20 example cases pass, GUIDE.md's 13 embedded code blocks match their source files byte for byte. `cargo test --locked --workspace`: **1221 passed, 0 failed, 10 ignored**.

今天第二次同步（同一天第一条同步的执行器/图算法动作库与它在 PR #36 上的安全加固见本文件更靠下的那一条，不重复）。这次把私有 main 从 `2eb748dc`（当天第一次同步停下的地方）推进到私有 main 当前 HEAD，把当天落地的其余全部内容同步过来。搭配层——本条目的主要内容——的专题写法，含把三种搭配嵌进同一个闭环的可跑例子，见[当天第二篇更新](updates/2026-09-26-b-composition-is-the-foundation.md)；本条目讲同样的内容，外加其余全部改动。

**搭配层开放：`search`、`ground`、判出来的图，以及它们底下的两个通用原语。** `lib/compose/ground.jpp` 把 JEV 与任何已登记的执行器动作（`exec_py`、`check_tests`，以后新加的也算）搭成一个「跑一遍再判对不对」的函数；动作失败（含「没有可用沙箱」）时返回一个失败值，`sieve` 把它当未决处理，不当崩溃。`lib/compose/search.jpp` 的 `search` 分轮跑「提出-判-再提出」，好候选跨轮累积（对首版「只累积本轮」的当天设计订正），把还没判定的候选交给 `carry`/`refine`/交人三种策略之一处置，终止用的是语言里其他构造都在用的同一套有界迭代机制。`search` 的选项里能给一个 `ground` 函数，把两者搭起来，「生成再判」就变成了「生成、跑、再判结果」——新增的示例 `examples/search-ground.jpp` 演示了这个用法：生成器提 Python 代码、`exec_py` 跑它、JEV 核对输出，循环直到找到一个对的候选。`lib/compose/graph.jpp` 把 JEV 判定为「有」的节点对当成图的边，在这张图上跑一个精确算法（六个 `graph:*` 动作），对还没判完的边把算法跑两遍（一遍当它不存在、一遍当它存在），两次结果不一致的地方正好标出接下来最该问哪些边。这三者底下，今天开放了两个通用原语：`compose(exits, rule)` 按一条规则（`any`/`all`/`min`/取第一个或最高排名的那个）把一批判断出口折成一个，仍然带着单个判断同样的三值未决语义与统计误差界记账；`cert(exit)` 能从任何出口（单个的或合成的）上读出经认证的误差界、剩余不确定性的计数，以及认证等级。`element(input, exit, ctx)` 作为又一个原语开放，让人能从一个判断产物直接造出一个图/搜索元素，不必经过完整的聚合构造。

**生成器接上真后端，而且不会卡住程序其余部分。** `gen` 现在接到 `claude -p`（`--gen-model`）的真实端口，经一个非阻塞的提交/轮询线程池调度：一次生成调用飞着的时候，程序里其余的判断与执行照常往前走。生成调用在写下的地方登记，但只在它所在层的下一个刷新点才真正发出，只有真被读到结果时才等它——所以一个分支根本没读到的生成值不花钱。生成出的材料带明确的 `untrusted` 可信标签（来自生成器的能力画像），这个标签沿用语言里其他值同一套传播机制。`--gen-cache` 让后续跑同一个生成站点不用再花一次钱。一次最小 `gen`+`sieve` 示例（`examples/gen-choose.jpp`）的真机运行花费 0.00004 美元（一次 `claude -p` 调用、三次 JEV 判断），从记录的账本重放时结果完全一致、零新增调用。

**作者声明线加了宿主接受路径，也能判更丰富的统计量。** 宿主现在可以显式接受一条作者声明线（上次同步的裁定）来放行不可逆动作（`--release-on-declared`，进这次运行的 `entry_hash`，接受本身就是可审计的事实），不再是直接拒绝运行。`cut` 加了 `stat` 选项，让声明线除了只按单一最高档答案判定外，也能判一个汇总统计量——跨打分档位的期望值，或多选题某个候选子集上的概率总和——线的两端各自独立可开可闭；一处评审发现「派生的缺省值会悄悄把端点变开」之后，改成手写的缺省值（两端都闭），不再靠派生。

**账本写入现在逐行落盘，不再只在一次运行结束时才写。** 此前一个被杀掉或崩溃的进程可能什么可用记录都留不下。现在每条账本条目写下的时候就落盘，不再攒到运行结束才写；一个不可逆动作在执行之前先记下意向，这样即便进程在动作执行中途死掉，后续续接也能看出当时到底试图做了什么；完全没法提供落盘能力的宿主直接被拒绝（`E-ledger-required`），不会悄悄在没有这层安全保障的情况下运行。

**22 个新的文本与数据内置函数，加带种子的随机数。** 字符串操作（切分、大小写转换、去空白、替换、前后缀判断、按字符取下标）、正则表达式、排序（普通与按自定义键，都稳定）、JSON 解析/序列化（往返用的是账本同一套规范形式）、内容哈希（用的是账本键同一个哈希函数）、日期解析/格式化/加减——加上 `rand`/`rand_int`/`shuffle`，带种子、可复现（文档写明是一个固定的、有版本号的算法，不是「标准库今天恰好怎么实现」，这样以后编译器/库升级不会让程序行为悄悄漂移）。全部遵循语言里其他值同一套可信传播规则：不可信输入进去，输出也不可信。

**同一份材料上的判断更常合批。** 此前，对同一份材料问的一道是非题和一道选择题，即便手写程序会把它们合成一次调用，J++ 有时也会分开发出。候选现在随着被问的那道题一起走，按它实际要判的材料分组，补上了这个缺口。这是一次请求格式的改动（渲染版本升到 `r2`），只影响判断请求在线上的形状，不改变任何答案的含义；旧格式录的账本仍能正确重放（会被标出来，不是悄悄按新格式重新解读），但不能在新格式下直接续接发新调用，需要一步显式的重录。

**一份签进仓库、能跑起来验的指南，加它一直在等的一处排序修复。** `rust/GUIDE.md` 新增完整一章「搭配：元素 → 组合 → 嵌套」，把本条目讲的每种搭配都配上真实签入的示例文件（`rust/examples/guide/`）；`rust/scripts/guide_check.py` 把每一段都跑一遍并与正文交叉核对，防止两者悄悄脱节——20 个示例用例全部通过。同一批还落地了：`order`（把一批判断读数变成有排名的档位）加了 `cut` 已有的那个统计量选项，现在对打分判断缺省按档位排、不再按最高档的原始概率排——这正是搭配层开放以来 `search` 按目标题排序一直缺的那个正确排序行为。两者完整说明见[当天第二篇更新](updates/2026-09-26-b-composition-is-the-foundation.md)。

**验证。** `cd rust && cargo fmt`（一个文件需要重新排版）之后 `cargo build --locked --workspace` 成功。`cargo clippy --workspace --all-targets --keep-going`：72 条告警，与本仓库当前基线相同（本次同步没有引入任何新告警）。`rust/scripts/guide_check.py`：20/20 示例用例通过，GUIDE.md 嵌入的 13 段代码块与源文件逐字节相同。`cargo test --locked --workspace`：**1221 passed, 0 failed, 10 ignored**。

## 2026-09-26 (daily sync): a composition-layer action library (host executors, graph algorithms, retrieval), lazy cut bridging, and two Codex-flagged calibration fixes / 搭配层动作库（宿主执行器、图算法、检索）、惰性过桥、两处 Codex 指出的校准修复

This is the daily sync from the research workspace (`tools/sync-rust-from-research.sh`, `tools/sync-from-workspace.sh`), taking private main from `85e28bfc` (the commit the previous sync, [PR #35](https://github.com/Towow-ai/jpp/pull/35), landed) through `2eb748dc` (2026-09-26) -- 111 research-workspace commits, most of them process record, blackboard and design-ledger entries around a smaller set of code changes grouped below.

**A single host action table, and 11 new entries in it.** The CLI's built-in `do`-actions (`record_check`, `read_json`, `write_json`) used to have their facts written in three places that could drift apart -- the registration table, the per-action closures, and a hand-written sentence in the help text. Two steps collapsed this to one table: C-1 moved the three facts (name, reversibility, `taint_out`, cost, the run function) into a single `crates/jpp/src/cli/actions/` table that both `check` and `run` read from (950 tests passed, help text byte-identical to `main` across three CLI invocations); C-1b then moved that table into the `jpp` crate's lib target (`crates/jpp/src/actions/`) so library-side consumers, not just the CLI binary, can see it. Two further blocks then just added rows: R2b added six exact-algorithm actions under a `graph:` prefix (`matching` -- Kuhn-Munkres for bipartite graphs plus an exact bitmask DP for general graphs up to 20 nodes, `shortest_path` -- Dijkstra, `max_clique` -- Bron-Kerbosch up to 60 nodes, `components` -- union-find, `set_cover` -- exact DP under 20 elements or greedy above, `max_flow` -- Dinic), each checked against a brute-force reference on 200 random graphs with zero mismatches and timed on realistic sizes (325-node components in 4.2ms, 325-node max-flow in 11.1ms; `max_clique` is NP-hard and was timed at its n=60 design limit instead, 0.49ms, not at 325 nodes -- recorded as a deliberate scope decision, not a shortcut). R2a added four more: `exec_py` and `check_tests` run arbitrary Python in a subprocess with environment stripped to a minimal `PATH`, a static import-and-`eval`/`exec` reject list, and a runtime monkeypatch on `socket.socket.connect`/`getaddrinfo` that blocks network access at the standard-library layer (explicitly documented as not a sandbox -- it does not stop `ctypes` or other routes around the Python `socket` module); `embed_topk` calls a local MiniLM model through a configurable interpreter path (`JPP_EMBED_PYTHON`, no hard-coded machine path -- an earlier draft that shipped one was corrected after review) and caches corpus embeddings by content hash; `bm25_topk` is a dependency-free Rust implementation. A follow-up commit added a fifth, `exec_sql`, which the design texts (`12`, `19`, `21` step 24e-1) list in the same block as the other four but the original task order omitted -- a read-only SQLite connection (writes rejected by SQLite's own `mode=ro` open flag, verified against a real write attempt rather than assumed), 3-second timeout, same network-disabling patch. The table now lists 14 actions total.

**Lazy cut bridging and straight-line lifting through function calls (ruling B94, research step 23c).** Previously, `cut` resolved a pending judgment's exit (checked it against a threshold, wrote the calibration-usage ledger entry) at the point it was written; this step makes `cut` return an unresolved `Value::Cut` immediately and defers resolution to a small set of "inspection points" (builtin arguments, `if` conditions, both sides of `&&`/`||`, field/index access, function boundaries) that force it when the value is actually needed. Combined with lifting `judge` calls (and now-lazy function calls with only name/literal arguments) to the top of a straight-line segment when they share the same state -- stopping at branches, loops, and short-circuit operators, so no call fires on a path that might not execute -- two of three constructed benchmark programs dropped from 4 judgment-call layers to 2 with the same 11 calls, and a frozen T1 acceptance case (`winnow`, hop-count criterion) went from failing to passing. All 42 fixed-observation golden programs stayed byte-identical (their `cut` results are already inspected before the next registration in every case), and a same-day review round found and fixed five follow-on issues before merge: a lineage-resolution error that was silently swallowed instead of propagated, a batching bug where fused-off flushing dropped the `speculative`/`lifted` flags on split registrations, a case where lifted registrations and their real-site siblings sharing a ledger key were asked as two separate questions instead of one, and straight-line lifting reaching into a callee's body across a short-circuit operator's unevaluated side. The final gate on this step: 947 passed, 0 failed, 10 ignored; 42 golden replay groups, 0 diffs.

**Two smaller static-check and CLI steps.** Step 20a-2b added `jpp check --questions-out <file>` (ruling B116 (5)): it walks a program's literal `test`/`select`/`measure` calls and exports each one's zero-slot form hash, verified to match byte-for-byte the hash the same question produces at runtime via `form(...)` -- the check this step exists to make possible, since a later migration step needs to key legacy per-author calibration records by this hash. The same step also narrowed four public legacy sample-splitting certification entry points down to one, explicitly named and documented as a test-only reproduction of the old method (a certified line built this way carries no delta bound and cannot be used to license an irreversible action). Step 24h closed out three of six previously-deferred "needs a closer look" checks from an earlier design review, and explicitly left the other three (a rendering-provenance warning, a scale-anchor check, and a loop-termination-measure check) in the backlog because each would require a language-level feature that does not exist yet, not just a new diagnostic rule -- written up rather than quietly attempted: a static shape precheck on `cut`'s `{cost: [fp, fn]}` argument, a new `W-legacy-fn-type` warning for old-style (`Fn(A) -> B`, no declared effects) function-typed parameters that are actually called inside the function body (this immediately flagged two of the project's own example programs, `adaptive.jpp` and `composition.jpp`, as a real, expected finding under the new rule, not a regression), and a new `W-unsure-not-bound` warning that fires even when a program's summed judgment budget is under its declared limit, if any of the judgment sites contributing to that sum sit inside a loop or a function body (the existing check only warned about this when the budget was already exceeded, missing the case a budget-under-limit program with an under-counted loop site is the one this check exists to catch).

**Two Codex review fixes on PR #35, now in this sync.** [PR #35](https://github.com/Towow-ai/jpp/pull/35)'s review flagged two issues after that sync had already landed; both are fixed as of this sync. First, `calib-import --from-ledger` built its sampling frame by parsing a ledger file's lines directly, without checking the hash chain between them -- a ledger with one answer edited (and the rest of the chain left alone) would be accepted and could seed a certificate from a tampered reading. It now decodes the file through the same chain-validating reader the `--replay`/`--resume` paths already used, and rejects a broken chain with `E-ledger-corrupt` before anything is written. Second, `calib-import --cost fp,fn` picked a cost-minimizing threshold from the full labeled sample and then certified that same threshold's error rate against the same sample -- a binomial confidence bound only holds for a threshold fixed before looking at the data, so certifying a data-selected threshold against the data that selected it can understate the true false-accept rate. It now splits the labeled sample the same way the project's other threshold-selection methods already do (alternating stratified halves, ruling B85): one half picks the cost-minimizing line, the other half certifies it, and the resulting certificate records which method produced it (`cost-split-stratified`) so it can be distinguished from a non-split certificate on reload. Both fixes required re-tuning four existing test fixtures whose sample counts had been sized for full-sample certification and no longer cleared the certification bar at half the effective sample size.

**A public-only test fixture the sync tooling had not caught.** One of step 20a-2b's four new tests (`b116_questions_out.rs`) reads a fixed example program from `../../../评估/2026-09-24-V7固定序/refund-do.jpp` -- a path that resolves inside the research workspace, one level above where this repository's `rust/` directory sits, and does not exist in this repository. This is the same class of problem the previous sync's `calib_load.rs` fix addressed (a test built against a path only the private research tree has), just not yet caught for this newer file: `cargo test --workspace` failed with `No such file or directory` on first run of this sync. Fixed the same way -- the exact program (a 9-line fixture, content unchanged) is now checked into `crates/jpp/tests/fixtures/refund-do.jpp`, and `tools/sync-rust-from-research.sh` rewrites the test's path to it on every future sync, the same pattern already used for `calib_load.rs`'s legacy-record fixture.

**A same-day security-hardening round on the new executor actions, added to this same PR after review.** PR review of the action library above (an external reviewer plus the coordinating session's own testing) found that `exec_py`/`check_tests`'s `reversible: true` claim rested only on a static code-text scan and a runtime network patch, neither of which stops every way to write a file (`pathlib.Path(...).write_text(...)` matches no keyword in the reject list) -- and, more severely, that `exec_sql`'s read-only connection does not stop `VACUUM INTO` or `ATTACH DATABASE` from creating new files on disk. Five issues in total (four from review, one found by the coordinator) are fixed: the three code-running actions now run inside an OS-level sandbox (macOS `sandbox-exec`, Linux `bwrap`, probed once at startup with an actual smoke test rather than a presence check, since a CI runner can have `bwrap` installed yet unable to use it) that is the layer actually enforcing "no writes outside a fresh temp directory, no network" -- explicitly not enforcing "no reading the host's files" or any CPU/memory/process-count limit, stated in the code rather than left implicit; when no usable sandbox is found, the three actions are marked non-reversible and a new static check, `E-action-no-sandbox`, fails `check`/`run` unconditionally. `exec_sql` gains `PRAGMA query_only` plus a SQLite authorizer callback allow-listing only read operations, denied statements now fail outright instead of reporting through the same field used for ordinary syntax errors. A `truncate` helper that could panic mid multi-byte character now backs off to a character boundary, and several graph actions' fixed absolute floating-point thresholds (which silently dropped small-magnitude edges while still claiming an exact result) are now either relative to the input's own scale or removed where an exact comparison suffices. Full account, including what the isolation still does not cover, in the [action-library update](updates/2026-09-26-composition-layer-actions.md). This round's own verification: `cargo build`/`cargo fmt --check` clean, `cargo clippy --workspace --all-targets --keep-going` at 72 warnings (down from 73, no new warnings), and the affected plus neighboring test files pass on this machine (macOS, where the sandbox is genuinely available). Pushing this and watching CI surfaced one more gap: 13 of `actions/exec.rs`'s own module-level unit tests called `exec_py_core`/`check_tests_core`/`exec_sql_core` directly without checking sandbox availability first, unlike the integration tests in `crates/jpp/tests/`, which the ported commits had already taught to skip gracefully -- these 13 failed outright on the public CI's Linux runner (no `bwrap`) with a `NoSandbox` panic instead of skipping. Added the same skip-and-print-reason guard (`if sandbox::tool().is_none() { ...; return; }`) already used by one sibling test in the same file to all 13, verified locally by forcing the no-sandbox path with `JPP_FORCE_NO_SANDBOX=1` (all 13 skip cleanly, 0 failures) and then confirming the normal, sandboxed run is unaffected: `cargo test --locked --workspace` **1052 passed, 0 failed, 10 ignored**, `cargo fmt --check`/`cargo clippy --workspace --all-targets --keep-going` (72 warnings, unchanged) still clean.

**Verification.** `cd rust && cargo fmt` (nine files needed reformatting -- pre-existing drift already present on the private main branch before this sync, not introduced by it) then `cargo build --locked --workspace` succeed. `cargo clippy --workspace --all-targets --keep-going`: 73 warnings, unchanged from this repository's recorded baseline (no new warnings from anything in this sync). `cargo test --locked --workspace`: **1033 passed, 0 failed, 10 ignored** (after the fixture fix above; the first run before that fix reported 1 failure in exactly the fixture-path test; this count predates the security-hardening round described above).

这是从研究工作区做的每日同步（`tools/sync-rust-from-research.sh`、`tools/sync-from-workspace.sh`），把私有 main 从 `85e28bfc`（上一次同步 [PR #35](https://github.com/Towow-ai/jpp/pull/35) 落地的那个提交）推进到 `2eb748dc`（2026-09-26）——111 个研究工作区提交，大多是过程记录、黑板与设计总账条目，围绕下面这几组代码改动展开。

**宿主动作表收成一张，新增 11 行。** CLI 的内置 `do` 动作（`record_check`、`read_json`、`write_json`）过去把事实分写在三处，容易漂移——注册表、各动作自己的闭包、帮助文本里手写的一句话。两步把它收成一张表：C-1 把三处事实（名字、是否可逆、`taint_out`、成本、执行函数）收进 `crates/jpp/src/cli/actions/` 的一张表，`check` 与 `run` 都从这里读（950 条测试通过，三种 CLI 调用的帮助文本与 main 逐字节相同）；C-1b 接着把这张表挪进 `jpp` crate 的 lib 目标（`crates/jpp/src/actions/`），让库层的调用方（不只是 CLI 二进制）也能看到它。随后两块施工只是往表里加行：R2b 加了六个带 `graph:` 前缀的精确算法动作（`matching`——二分图用 Kuhn-Munkres、一般图用精确位掩码 DP（n≤20）；`shortest_path`——Dijkstra；`max_clique`——Bron-Kerbosch（n≤60）；`components`——并查集；`set_cover`——20 个元素以内精确 DP、以上贪心；`max_flow`——Dinic），每个都对暴力解法跑了 200 组随机图对拍、零分歧，并在真实规模上计过时（325 节点的 `components` 4.2 毫秒、325 节点的 `max_flow` 11.1 毫秒；`max_clique` 是 NP-hard，按它 n=60 的算法设计上限计时（0.49 毫秒），没有硬凑到 325 节点——这是记录在案的范围决定，不是抄近路）。R2a 又加了四个：`exec_py`、`check_tests` 在子进程里跑任意 Python 代码，环境清到只剩最小 `PATH`，加一张静态的 import/`eval`/`exec` 拒绝表，外加运行期给 `socket.socket.connect`/`getaddrinfo` 打补丁挡住标准库层面的网络访问（文档里写明这不是沙箱——挡不住 `ctypes` 等绕开 Python `socket` 模块的路径）；`embed_topk` 经可配置的解释器路径（`JPP_EMBED_PYTHON`，不写死本机路径——早期草稿写死过，复核后已改正）调用本地 MiniLM 模型，按内容哈希缓存语料向量；`bm25_topk` 是不依赖新库的纯 Rust 实现。一次后续提交补上第五个 `exec_sql`——设计文本（`12`、`19`、`21` 步 24e-1）把它和前四个列在同一块，最初的任务指令漏列了——只读 SQLite 连接（写操作被 SQLite 自身的 `mode=ro` 打开位拒绝，用一次真实的写入尝试验证过，不是假设），3 秒超时，同一套网络禁用补丁。动作表现在共 14 行。

**惰性过桥与直线段提升穿过函数调用（B94 裁定，研究树步 23c）。** 此前 `cut` 在写下的那一刻就解析待定判断的出口（对阈值判断、写校准使用账本条目）；这一步把 `cut` 改成立即返回一个未解析的 `Value::Cut`，把解析推迟到少数几个「检视点」（内置函数实参、`if` 条件、`&&`/`||` 两侧、取字段/下标、函数边界）——真正用到这个值时才强制解析。配合把状态相同的 `judge` 调用（以及现在惰性化的、实参只有名字或字面量的函数调用）提升到直线段的段首——遇分支、循环、短路运算符即停，不让任何一条不一定会走到的路径上多发一次调用——三个构造出的基准程序里有两个从 4 层判断调用降到 2 层、调用数不变（仍是 11 次），一个此前失败的冻结验收用例（`winnow`，按跳数判据）由不过变为通过。42 个固定观察金样全部逐字节不变（它们的 `cut` 结果在下一次登记之前都已经被检视过），合入前的同日复核又发现并修了五处问题：谱系解析出错被悄悄吞掉而不是往上传、融合关时 flush 分组丢掉了拆分登记的 `speculative`/`lifted` 标记、提升登记与共享同一账本键的真站点被问成两道题而不是一道、直线段提升穿进被调函数体时没有绕开短路运算符不一定求值的那一侧。这一步最终门禁：947 通过、0 失败、10 忽略；42 组金样重放，0 差异。

**两个较小的静态检查与 CLI 步骤。** 步 20a-2b 新增 `jpp check --questions-out <file>`（B116 (5) 裁定）：遍历程序里的字面 `test`/`select`/`measure` 调用，导出每道题的零槽题式哈希，并核实过它与同一道题在运行时经 `form(...)` 算出的哈希逐字节相等——这正是本步要为之铺路的检查，因为后续一步迁移作者串的旧校准记录需要按这个哈希做主键。同一步还把四个公开的旧法种子分半认证入口收窄成一个，明确命名并写明它只用于测试、复现旧方法（这样认证出的线不带 δ 界，不能用于放行不可逆动作）。步 24h 从此前一次设计复核留下的六项「待核」里做完了三项，另外三项（渲染来源告警、刻度锚点检查、循环终止量检查）明确留在问题清单——因为每一项都需要一个当前还不存在的语言级特性，不只是缺一条诊断规则，如实写明而不是悄悄硬做：给 `cut` 的 `{cost: [fp, fn]}` 参数加了静态形状预检；新增 `W-legacy-fn-type` 告警，抓旧式（`Fn(A) -> B`，不带效应声明）的函数类型参数在函数体内被真的调用的情形（这条新规则立刻命中了项目自己的两个示例程序 `adaptive.jpp` 与 `composition.jpp`——这是预期内的真发现，不是回归）；新增 `W-unsure-not-bound` 告警，在程序的判断预算联合和没有超过声明上限时也会触发——只要贡献这个和的判断站点里有任何一个落在循环或函数体内（既有检查只在预算已经超限时才提示这一点，漏掉了「和算出来没超、但被循环站点低估了」这个本该被这条检查逮住的情形）。

**PR #35 上 Codex 评审指出的两处修复，现已并入本次同步。** [PR #35](https://github.com/Towow-ai/jpp/pull/35) 合入后，评审指出两处问题，本次同步时都已修复。其一，`calib-import --from-ledger` 直接逐行解析账本文件建抽样框，不校验行间哈希链——一份被改过一条答案（其余链未动）的账本会被照单全收，可能用一条未经认证的读数出证书。现在改用与 `--replay`/`--resume` 相同的校验链的解码器读整份文件，链断了在写任何东西之前就报 `E-ledger-corrupt` 拒绝。其二，`calib-import --cost fp,fn` 在全量标注样本上挑一条经验代价最小的阈值，再在同一批样本上认证这条阈值的错误率——二项置信界只对「看数据之前就已固定」的阈值成立，用挑出这条阈值的同一批数据去认证它，会低估真实的假放行率。现在按项目其余阈值选择方法早已在用的做法（分层交替分半，B85 裁定）拆分标注样本：一半选出代价最小的线，另一半认证它，证书上记录选线方法（`cost-split-stratified`），装载时能与未分半的证书区分开。两处修复都需要重新调整四个既有测试夹具的样本量——它们原先是按全量样本认证的门槛配的，样本量减半后原有数字不够用了。

**一处同步工具此前没接住的公开侧夹具问题。** 步 20a-2b 新增的四条测试里有一条（`b116_questions_out.rs`）读一个固定示例程序 `../../../评估/2026-09-24-V7固定序/refund-do.jpp`——这个路径解析到研究工作区内部、`rust/` 目录所在位置的上一层，本仓库里没有这个目录。这与上一次同步给 `calib_load.rs` 修的问题同一类（测试指向只有研究树才有的路径），只是这个更新的文件此前还没被接住：本次同步第一次跑 `cargo test --workspace` 时报 `No such file or directory` 失败。修法相同——把这个程序原样（9 行，内容未改）收进仓库 `crates/jpp/tests/fixtures/refund-do.jpp`，并让 `tools/sync-rust-from-research.sh` 在每次同步时把测试的路径改写指向它，与 `calib_load.rs` 那份旧格式记录夹具走的是同一套机制。

**同一天在这个 PR 上又加的一轮安全加固，针对新加的执行器动作。** 上面这批动作库的评审（一名外部评审加协调会话自己的实测）发现：`exec_py`/`check_tests` 的 `reversible: true` 断言只靠静态代码文本扫描与运行期网络补丁撑着，两者都挡不住所有写文件的写法（`pathlib.Path(...).write_text(...)` 不含拒绝表里的任何关键字）；更严重的是，`exec_sql` 的只读连接挡不住 `VACUUM INTO` 或 `ATTACH DATABASE` 在磁盘上新建文件。一共修了五条（评审四条、协调会话自己查出一条）：三个跑代码的动作现在跑在操作系统级沙箱里（macOS `sandbox-exec`、Linux `bwrap`，启动时用一次真实冒烟测试而不是只看文件存不存在来探测，因为 CI 跑的机器可能装了 `bwrap` 却用不了它），沙箱才是真正强制「不许写临时目录之外、不许联网」的那一层——明确不强制「不许读宿主文件」或任何 CPU/内存/进程数限制，这一点写在代码里、不是留给人猜；探测不到能用的沙箱时，这三个动作被标为不可逆，一条新的静态检查 `E-action-no-sandbox` 会无条件让 `check`/`run` 失败。`exec_sql` 加了 `PRAGMA query_only` 加一个只放行读操作的 SQLite 授权回调，被拒绝的语句现在直接让调用失败，不再走普通语法错误共用的那个字段。一个可能在多字节字符中间截断而 panic 的 `truncate` 函数改成退到字符边界；几个图算法动作原先写死的绝对浮点阈值（会悄悄丢掉数值很小的边、却仍报精确）现在改成按输入量纲取相对阈值，或者在本来就不需要容忍浮点噪声的地方直接去掉阈值。完整说明，包括隔离到底还盖不住什么，见[动作库专题更新](updates/2026-09-26-composition-layer-actions.md)。这一轮自己的验证：`cargo build`/`cargo fmt --check` 干净，`cargo clippy --workspace --all-targets --keep-going` 72 条告警（比之前的 73 少，没有新增），受影响与相邻的测试文件在本机（macOS，真的有沙箱可用）全部通过。推上去看 CI 又抓出一处缺口：`actions/exec.rs` 里有 13 条模块级单元测试直接调 `exec_py_core`/`check_tests_core`/`exec_sql_core`，没有先判沙箱可用性——不像 `crates/jpp/tests/` 里的集成测试，移植进来的提交已经教会它们探测不到就跳过——这 13 条在公开仓库 CI 的 Linux runner（没有 `bwrap`）上直接因为 `NoSandbox` panic 失败，没有跳过。给这 13 条都加上同文件里已有一条兄弟测试用的同一套判断（`if sandbox::tool().is_none() { ...; return; }`），本机用 `JPP_FORCE_NO_SANDBOX=1` 强制走无沙箱路径验证过（13 条全部干净跳过，0 失败），再确认正常的（有沙箱的）跑法不受影响：`cargo test --locked --workspace` **1052 通过、0 失败、10 忽略**，`cargo fmt --check`/`cargo clippy --workspace --all-targets --keep-going`（72 条告警，未变）仍然干净。

**验证。** `cd rust && cargo fmt`（九个文件需要重新排版——这是私有 main 分支本就有的既存格式漂移，不是这次同步引入的）之后 `cargo build --locked --workspace` 成功。`cargo clippy --workspace --all-targets --keep-going`：73 条告警，与本仓库记录的基线相同（本次同步没有引入任何新告警）。`cargo test --locked --workspace`：**1033 通过，0 失败，10 忽略**（上面那处夹具修复之后；修复前的第一次跑，唯一的失败正好是那处路径夹具的测试；这个数字是安全加固那一轮之前的）。

## 2026-09-25 (daily sync): the architecture refactor lands in `main`, ledger v3, host-side entry typing, live fixed-sequence certification, and a batch of static checks / 架构重构合入 `main`：账本 v3、宿主入口分型、真机固定序认证上线，外加一批静态检查

This is the daily sync from the research workspace (`tools/sync-rust-from-research.sh`, `tools/sync-from-workspace.sh`), taking research-tree commit `85e28bfc` (2026-09-25). It supersedes the "pending, on a same-day branch" language in the previous `docs/status.md`: that branch is the one landing here, plus a full day of further work on top of it. `git log --oneline 9716e61b..85e28bfc -- 地基/rust-jpp` lists 161 research commits since the last rust sync (2026-09-24, `9716e61b`); this entry groups them by effect rather than listing each one.

**A crate rename that this sync's tooling had to be taught about.** Research-tree step 14a (ruling B74) split the interpreter out into its own crate (`jpp-runtime`: budget, bridge, construct evaluation, host builtins) and merged the old `jpp-core` (checker, effects, ledger façade) with `jpp-cli` (the binary) into a single crate, `jpp` (a lib target plus a bin target). `rust/` now has 10 crates: `jpp`, `jpp-calib`, `jpp-check`, `jpp-effects`, `jpp-ir`, `jpp-ledger`, `jpp-plan`, `jpp-runtime`, `jpp-syntax`, `jpp-value`. Two files exist only on the public side (a legacy-format calibration fixture directory and an overflow-assertion regression test) and had to move by hand from `crates/jpp-core/tests/` to `crates/jpp/tests/` before the sync tool's protect-list could find them again; `tools/sync-rust-from-research.sh` and `tools/sync-composition.py`'s KEEP paths are updated to match, and `-p jpp-cli` references in the CI workflow, `README.md`/`README.zh-CN.md`, `rust/METHODS-AND-LIFECYCLE.md` and `rust/scripts/doc_snippets.py` are now `-p jpp`. Full account of what moved and what stayed public-only: [`rust/PUBLIC-SNAPSHOT.md`](../rust/PUBLIC-SNAPSHOT.md).

**Ledger v3 and typed host entry.** The ledger format gained a `CalibUsed` entry kind, source-typed edges on `output_mat` (value-dependency vs. selection-dependency, so `J-02`'s static reachability check reads only the value-dependency projection -- ruling B92), and reserved fields for a calibration reference (`calib_ref.key`/`kind`/`fill`) that later steps fill in; a `ledger-migrate` subcommand converts v2 ledgers (checked against 36 archived v2 ledgers, replayed with no diffs). Program entry is now typed: a host passes named value entries and material entries (`Value::Mat`, tagged `origin=input`) instead of one untyped blob, `--input-trusted` lets a CLI caller declare an entry's trust bit explicitly (naming the flag in the resulting `J-08` runtime message when it matters), and `entry_hash` covers the whole typed entry rather than a loose value. Numeric promotion (integer/float mixing) landed as its own step with an explicit rule rather than an implicit cast.

**Fixed-sequence certification is live, not just designed.** The previous sync's `docs/status.md` described a fixed-sequence/sequential certification method (cutting the labeled-evidence threshold for a formally certified line from roughly 160 examples toward roughly 60) as "validated offline, not implemented in any branch." It is implemented now: research step 20h-1 built the B104/B87 revision (cut bandwidth never below a certificate's recorded delta; sequential candidates aligned to a shared ordering; a fingerprint check that keeps random arrival label-independent), and step 20i finished `alpha_eff` and the truth-value baseline (ruling B89) and put the fixed-sequence tier into formal service after an 80-row blind review with zero disagreements. A stand-in judge and a backend registry (step 15g) let a program swap which capability profile executes it without changing the program text; of the 8 tracked backend-swap hypotheses in `crates/jpp/tests/profile_swap.rs`, step 24d removed the `ignore` marker from 4 more (one, H2, stays deferred to a later step) on top of the 1 that already passed, an increase this sync can state with the caveat below.

**Budget, library synthesis and a batch of static checks.** Budget exhaustion now degrades at the next refresh point instead of halting the program outright (`B93`, step 22-0): calls stop being issued, but a program already in flight keeps running with the results it has, and the ledger records `budget` as the cause on entries it could not reach. Windowed dispatch and in-port concurrency (step 15e) let one backend port serve several judgments from the same refresh window concurrently instead of one call at a time. `tally`/`first_k` gained a shared synthesis path (`compose`, `element`, rulings B131-B133, step 25-2b) and their aggregate exits stopped counting as static-check release evidence (`step 25-1`, a same-day hotfix in the release-direction that a follow-on step, `25-9`, still owes a matching change to `releases()`). Seven static-check additions landed as steps 24a through 24g: two pending-output warnings under `J-06` (`B69`/`B111`), a `J-14` runtime face for multi-object crosstalk, two deferred `J-08` static-face items, four `J-04`/`J-03`/`J-09`/`J-14` untested-profile-field warnings, a `J-04` comparative-fingerprint static face plus a `J-11` unregistered-action-name face, and a `J-08` diagnostics-layer static consumer for shape mismatches (`B51`-R2). `calib-import --cost fp,fn` (step 20a-2a, ruling B129) lets an author certify an action-space cost line from labeled evidence, alongside the existing reading-space `declare` and certificate-only `alpha` forms.

**Ruled today, not yet in this sync's code.** Rulings B122 through B146 went into the reference texts (`12`, `20` v2, `21`, `19`) today, including the author-sovereignty and declared-line batch (B128-B130): an author may write `declare:{hi, lo?}` directly on a `cut` as a stated policy line -- used verbatim, never smoothed toward a certified threshold, never written into the calibration store -- and an irreversible `do` gated on a declared line requires the host to explicitly accept it (`--release-on-declared` / `EntryArgs.accept`, hashed into `entry_hash`, visible in the report) or the run stops with a fix-it message instead of silently downgrading. This is ruled and reasoned through in `地基/附注/2026-09-25-作者主权与策略表达裁定.md`, not code: the construction step that wires it in, 20j-1, is an active work-in-progress branch as of this sync and is not part of what landed in `rust/` today.

**Honest numbers, not all improved.** The expressiveness-ratio reading did move today, but by a measurement-method change, not a backend improvement: step 31-1b (ruling B96) replaced point-based T1 grading with a held-out, property-based acceptance check and re-ran it against a second baseline set per implementation (9 of 10 new baselines passed on the first acceptance run). The resulting reading is T1 3.69x counting only implementations that pass the new acceptance check, 4.48x counting all implementations, and 4.97x under the old frozen grading kept for comparison (`t1-strict`) -- still well below the project's 9x-20x reference band, and not directly comparable to the `~4.1x`/`~3.4x` figures the previous `docs/status.md` carried from the pending branch, because the acceptance method under them changed. The backend-swap dashboard item (`item_profile_swap` in `地基/rust-jpp/scripts/dashboard.py`) reads 1 of 8 tracked hypotheses fully passing as of the last dashboard run recorded on the blackboard (step 15g); step 24d's un-ignoring of 4 more hypothesis groups (above) had not been re-run through the dashboard script as of this sync, so this entry does not claim a new fraction without rerunning it -- see verification below. Depth evidence is still fixed-observation hop counts only (22/12/4 judgments at hop one/two/three); a live depth curve is not measured yet.

**Verification.** `cd rust && cargo fmt` (two public-only files needed reformatting after the port and the crate rename -- `crates/jpp/tests/known_defects.rs`, `crates/jpp/tests/wiring.rs`) then `cargo build --locked --workspace` succeed. `cargo test --locked --workspace`: **884 passed, 0 failed, 9 ignored**. `crates/jpp/tests/known_defects.rs` needed a second, manual fix beyond the automatic port: it imported `jpp_core::*` and called the pre-14a `run(&program, &mut client, ...)` signature; both are now `jpp::*` and `run(&program, fixed.ports(), ...)` against `FixedPorts` (`FixedClient` itself was retired in an earlier, already-synced step, 15c). The full test run surfaced a third gap the port script's own checks missed: `crates/jpp-effects/src/profile.rs`'s inline unit test still joined the pre-rename `foundation/profile/profiles/...` path, because the tool's path-rewrite loop only scanned `crates/*/tests/*.rs`, not a crate's `src/` tree, where this particular test happens to live inline. Fixed both the test file and the tool (`tools/sync-rust-from-research.sh` now scans `crates/*/{src,tests}/**/*.rs`), and re-ran `cargo test -p jpp-effects --lib --offline` to confirm (16 passed, 0 failed) rather than repeating the full workspace run. To reproduce the backend-swap count claimed above, run `cargo test -p jpp --test profile_swap --offline -- h --test-threads=2` from `rust/` and count `... ok` groups the way `item_profile_swap()` does in `地基/rust-jpp/scripts/dashboard.py`.

这是从研究工作区做的每日同步（`tools/sync-rust-from-research.sh`、`tools/sync-from-workspace.sh`），取研究树提交 `85e28bfc`（2026-09-25）。它取代了此前 `docs/status.md` 里"在同日分支上待合入"的说法——那个分支连同它之后一整天的后续工作，今天一起进了这里。`git log --oneline 9716e61b..85e28bfc -- 地基/rust-jpp` 显示自上次 rust 同步（2026-09-24，`9716e61b`）以来研究树有 161 个提交；这里按效果分组叙述，不逐条列出。

**一次连累同步工具的改名。** 研究树步 14a（B74 裁定）把解释器独立成一个新 crate（`jpp-runtime`：预算、桥、构造求值、宿主内置函数），并把原 `jpp-core`（检查器、效应、账本外观）与 `jpp-cli`（二进制）合并为单个 crate `jpp`（lib 目标 + bin 目标）。`rust/` 现在是 10 个 crate：`jpp`、`jpp-calib`、`jpp-check`、`jpp-effects`、`jpp-ir`、`jpp-ledger`、`jpp-plan`、`jpp-runtime`、`jpp-syntax`、`jpp-value`。两个只在公开侧存在的文件（旧格式校准夹具目录、一条溢出断言回归测试）先手工从 `crates/jpp-core/tests/` 搬到 `crates/jpp/tests/`，同步工具的保护名单才能重新认得它们；`tools/sync-rust-from-research.sh` 与 `tools/sync-composition.py` 的 KEEP 路径已同步改过，CI 工作流、`README.md`/`README.zh-CN.md`、`rust/METHODS-AND-LIFECYCLE.md`、`rust/scripts/doc_snippets.py` 里的 `-p jpp-cli` 全部改成 `-p jpp`。搬了什么、什么只留在公开侧，完整记录见 [`rust/PUBLIC-SNAPSHOT.md`](../rust/PUBLIC-SNAPSHOT.md)。

**账本 v3 与类型化的宿主入口。** 账本格式加了 `CalibUsed` 条目种类、`output_mat` 的来源边带上种类（值依赖 / 选择依赖两种，`J-02` 的静态可达性检查因此只读值依赖投影——B92 裁定）、以及给后续步骤填的校准引用留位字段（`calib_ref.key`/`kind`/`fill`）；新增 `ledger-migrate` 子命令做 v2 账本迁移（对 36 份归档的 v2 账本核对过，重放零差异）。程序入口现在分了型：宿主传入具名的值条目与材料条目（`Value::Mat`，标 `origin=input`），不再是一整团无类型数据；`--input-trusted` 让 CLI 调用方显式声明某条入口的可信位（在相关的 `J-08` 运行期报文里点名这个开关）；`entry_hash` 现在覆盖整份类型化入口，不再只是一个零散值。数值提升（整数/浮点混算）作为独立一步落地，有明确规则而非隐式转换。

**固定序认证已经真机上线，不只是设计。** 上一次同步的 `docs/status.md` 把固定序/序贯认证方法（把正式档门槛从约 160 条标注压到约 60 条左右）描述成"离线验证过、任何分支都没实现"。现在实现了：研究树步 20h-1 造出 B104/B87 修订（判区带宽不小于证书记录的 δ；序贯候选对齐同一顺序；指纹检查保证随机到达与标签无关），步 20i 完成 `alpha_eff` 与真值基准（B89 裁定），80 条盲复核零分歧后把固定序档正式推上岗。替身判断器加后端注册表（步 15g）让程序换一个能力画像执行、程序文本不用改；`crates/jpp/tests/profile_swap.rs` 里追踪的 8 条换后端假设中，步 24d 在此前已过的 1 条之上又摘掉 4 条的 `ignore`（其中 H2 仍推迟到后续步骤）——这个增量本条目带着下面的保留说明一起写。

**预算、库层合成与一批静态检查。** 预算耗尽现在在下一个刷新点降级，不再直接停机整个程序（`B93`，步 22-0）：不再发新调用，但已经在跑的程序继续用手头已有的结果往下走，账本对够不到的条目记 `budget` 作缺席原因。按窗口发出与端口内并发（步 15e）让一个后端端口能同时服务同一刷新窗口里的几个判断，而不是一次一个。`tally`/`first_k` 有了共用的合成路径（`compose`、`element`，B131–B133 裁定，步 25-2b），它们的聚合出口不再算作静态检查的放行证据（步 25-1，当天的放行方向热修，后续步骤 25-9 还欠 `releases()` 的配套改动）。七项静态检查以步 24a 到 24g 落地：`J-06` 下两条未决输出告警（`B69`/`B111`）、`J-14` 运行期面处理多对象串扰、两处推迟的 `J-08` 静态面、四条 `J-04`/`J-03`/`J-09`/`J-14` 画像字段未测告警、`J-04` 比较性指纹静态面加 `J-11` 未登记动作名静态面、以及 `J-08` 诊断层对形状不匹配的静态消费者（`B51`-R2）。`calib-import --cost fp,fn`（步 20a-2a，B129 裁定）让作者能从标注证据认证一条动作空间的代价线，与既有的读数空间 `declare` 和只选证书的 `alpha` 两式并列。

**今天裁定了，但还没进这次同步的代码。** B122 到 B146 一批裁定今天写进了依据文本（`12`、`20` v2、`21`、`19`），包括作者主权与声明线一批（B128–B130）：作者可以直接在 `cut` 上写 `declare:{hi, lo?}` 作为明说的策略线——按写的数字原样用，不向认证阈值平移或取严，也不写进校准库；放行不可逆 `do` 的声明线需要宿主显式接受（`--release-on-declared` / `EntryArgs.accept`，进 `entry_hash`、报告里看得见），不接受就在 `check`/`release` 处停下并给出修法提示，不会静默降级。这些论证与裁定写在 `地基/附注/2026-09-25-作者主权与策略表达裁定.md`，还不是代码：把它接进去的施工步 20j-1，在本次同步时还是一个进行中的 worktree 分支，没有进入今天 `rust/` 里落地的内容。

**如实的数字，不是全都变好了。** 表达量比读数今天确实变了，但变的是量法，不是后端效果：步 31-1b（B96 裁定）把 T1 的逐点打分改成留出集、按性质验收，并对每个实现重新跑了第二套基线（十份新基线里九份第一次验收就过）。得到的读数是：只计通过新验收方法的实现 3.69×，计入全部实现 4.48×，按旧冻结打分法（留作对照的 `t1-strict`）4.97×——仍远低于项目 9–20× 的参考带，也不能直接拿来和此前 `docs/status.md` 从待合并分支里带的 `约 4.1×`/`约 3.4×` 相比，因为两者背后的验收方法本身变了。换后端仪表项（`地基/rust-jpp/scripts/dashboard.py` 的 `item_profile_swap`）按黑板记录的最近一次仪表读数（步 15g）是 8 条追踪假设里 1 条全过；步 24d 对另外 4 组假设摘掉 ignore（见上）之后，本次同步前没有重新跑过仪表脚本，所以这条不在没有重新验证的情况下声称新的分数——复现方法见下面「验证」。深度证据仍只有固定观察下的跳数分布（一/二/三跳 22/12/4 个判断），真机深度曲线还没有测量。

**验证。** `cd rust && cargo fmt`（两个只在公开侧的文件在改写和改名之后需要重新排版——`crates/jpp/tests/known_defects.rs`、`crates/jpp/tests/wiring.rs`）之后 `cargo build --locked --workspace` 成功。`cargo test --locked --workspace`：**884 passed, 0 failed, 9 ignored**。`crates/jpp/tests/known_defects.rs` 除了自动改写还需要一处手工修：它原来 `use jpp_core::*` 并调用 14a 之前的 `run(&program, &mut client, ...)` 签名；两处现在都改成 `jpp::*` 与针对 `FixedPorts` 的 `run(&program, fixed.ports(), ...)`（`FixedClient` 本身在更早、已经同步过的步骤 15c 里就退役了）。全量测试还揪出同步工具自己的检查没盖到的第三处：`crates/jpp-effects/src/profile.rs` 里一处内嵌单元测试仍拼着改名前的 `foundation/profile/profiles/...` 路径——工具的路径改写循环此前只扫 `crates/*/tests/*.rs`，扫不到这个测试实际所在的 crate `src/` 树。已经改了这个测试文件，也改了工具本身（`tools/sync-rust-from-research.sh` 现在扫 `crates/*/{src,tests}/**/*.rs`），改完后跑 `cargo test -p jpp-effects --lib --offline` 确认（16 通过、0 失败），没有为此再跑一遍全量。要复现上面提到的换后端计数，在 `rust/` 下跑 `cargo test -p jpp --test profile_swap --offline -- h --test-threads=2`，按 `地基/rust-jpp/scripts/dashboard.py` 里 `item_profile_swap()` 的算法数 `... ok` 组。

## 2026-09-25 (later): real-source rerun complete, $1.43 spent, all 10 pre-registered predictions confirmed / 真实来源重跑完成，花费 $1.43，10 条预注册预测全部命中

Closes out the entry directly below, which described the abstract trim without a rerun. Three more things happened after it, in order: the generic "The paper is about &lt;field&gt;." sentence was replaced with 40 hand-written, &lt;=25-word per-paper descriptions (`scripts/towow_real_paper_descriptions.json`, read by `scripts/trim_towow_real_abstracts.py`, not embedded in the script); a falsifiable prediction was pre-registered and committed (`docs/towow-real-rerun-preregistration-2026-09-25.md`) before any paid call; then the full pipeline ran live.

**Descriptions, finalized once.** 40 distinct paper titles across the 178 academic entries each got one hand-written English sentence describing what that paper does, paraphrased from (not copied out of) the original abstract -- checked for 6-word verbatim overlap with the source text; the only overlaps found were unavoidable technical-term phrases (e.g. "genomic foundation models trained on DNA sequences"), not lifted prose. This was treated as the last wording pass before spending, since any further edit to these 178 contexts invalidates the JEV request cache again.

**Pre-registration, committed before spend.** Built on `research/地基/DECISIONS.md:1005`'s hypothesis (removing shared-abstract text weakens `coauthor_same_field`/`github_same_org` recall) but went further by inspecting the actual data first: all 599 `coauthor_same_field` edges connect people who share an *exact, verbatim-preserved* paper title (title trimming was never part of this change), so the prediction diverged from the blanket framing -- this relation type was predicted to hold steady, not drop, because its strongest signal survives untouched. `github_same_org` (307 edges) and `same_team_*` (33 edges) connect endpoints whose text this edit never touched at all, predicted to stay within baseline noise. `cross_source_*` (24 edges) was already at a 0/24 floor pre-trim. Ten falsifiable numeric ranges were committed for `order20` (the page's default method) at K=1/5/10/20 plus `source_backed`, each with an explicit line stating what result would falsify it.

**The rerun.** Local retrieval first (`benchmark_towow_real.py`, BM25 + MiniLM + RRF fusion, 0 API calls): a confound check reran it with the exact recorded environment (torch 2.14.0, sentence-transformers 6.1.0, transformers 5.17.0, MiniLM revision `e8f8c211…`) against the *pre-trim* data and reproduced the published `retrieval.json`'s rankings identically (325/325 for all three methods) -- confirming zero environment drift, so the rerun's numbers reflect the data change alone. Then the same script ran against the trimmed data and its output became the new `retrieval.json`. Then live JEV: `cut20` ($0.080) -> full-pairwise `cut324` ($1.165 more, cumulative $1.245) -> `order20` ($0.150 more, cumulative $1.395) -> `exploration` ($0.039 more, cumulative **$1.4336** of the $2.00 cap authorized for this rerun). Credentials came only from `~/.typesafe-key`, read by the existing client code; verified afterward that no output or recording contains the key or an `Authorization` header. Live-call concurrency was temporarily capped at `max_workers=4` (down from this program's usual 8 for cut mode, 32 for order/explore) after the machine hit a resource limit mid-task from unrelated concurrent load; the two source files were reverted to their committed state immediately after the live calls finished (clean `git diff`).

**All 10 predictions confirmed, none falsified.** `order20` @ K=10: `coauthor_same_field` recall 0.8965 (predicted 0.83-0.95); `github_same_org` 0.3974, an *identical* hit count (122/307) to the pre-trim baseline, exactly as predicted for an untouched relation type; `same_team_*` 1.0000 and `cross_source_*` 0.0000, both as predicted. Overall: undirected recall@10 0.7186 (predicted 0.68-0.76), directed@10 0.6153 (predicted 0.57-0.66), undirected @1/@5/@20 0.1859/0.5306/0.8619 (all within the committed +/-0.05 band of baseline), `source_backed`@10 0.7369 (predicted 0.71-0.79). The specific divergence from `DECISIONS.md:1005`'s framing -- that `coauthor_same_field` would hold steady because the shared title, not the abstract, carries the signal -- held.

**Published and verified.** `results.json`, `reports.json.gz`, `recording.jsonl.gz` (rebuilt from the live run, 9,987 records), `exploration.json`, `exploration-recording.jsonl.gz` (the 1,095 records the exploration step added, extracted from the shared journal by line offset so the main run's records aren't duplicated into it), `diagnostics.json` (regenerated locally, 0 API calls). A grep for the old abstract text across every new artifact, including inside the gzipped recordings, found zero hits. All four `data_sha256` fields (`results`/`retrieval`/`diagnostics`/`exploration`) match the current `data.json`. `real/index.html`'s privacy notice now says the rankings and recordings are current as of this rerun. Full test suite: **561 passed, 0 failed** (up from 560 passed / 1 known failure before this rerun) -- `test_real_source_public_results_recompute_from_rankings` passes now that `results.json` matches `data.json`.

[Rerun commit / 重跑提交](https://github.com/Towow-ai/jpp/commit/35bd350), [40-description commit](https://github.com/Towow-ai/jpp/commit/7bc65aa), [retrieval + build-script commit](https://github.com/Towow-ai/jpp/commit/5618aaa), [pre-registration commit](https://github.com/Towow-ai/jpp/commit/d8d180d), [PR #33](https://github.com/Towow-ai/jpp/pull/33)

## 2026-09-25: real-source demo profiles - abstracts trimmed, source disclosed, rerun not done / 真实来源演示资料：删除摘要、公开来源，重跑未做

**Decision this responds to.** The 325 real-source profiles in `docs/demos/towow/real/data.json` (academic, GitHub and YC founder records) stay on public sources, are not treated as de-identified, and their git history is not rewritten. Two things changed going forward instead: the paper abstracts in the academic entries are trimmed, and the demo page states plainly where the data comes from and how to ask for removal.

**What changed.** In `data.json`, all 178 academic-source `context` fields had their paper abstract (the long passage after the title) removed; the institution line, the paper title and the trailing OpenAlex concept tags are kept verbatim, and the abstract is replaced with one short "The paper is about &lt;field&gt;." sentence using the field already named in that entry. The other 147 GitHub/startup records, and every top-level field (`schema`, `known_relations`, `provenance`, `scope`), are untouched -- confirmed with a full structural diff, not just a visual check. `real/index.html` now carries a bilingual notice: public sources (OpenAlex paper/author records, GitHub, YC founder pages), names replaced by IDs, real signals such as shared papers/institutions kept on purpose for the J++ matching demo, and a GitHub-issue path to request removal.

**What a repo-wide grep for anonymization language turned up.** `git grep -niE 'anonym|de-?identif|去标识|去识别|脱敏|匿名|identifier'` across all tracked files found no public-facing claim that this dataset is anonymized or de-identified beyond ordinary uses of "identifier" as a language term and one already-correct disclaimer in `data.json`'s own `scope` field ("not anonymous"). The private research-tree mirror under `research/地基/` (confirmed via `tools/sync-from-workspace.sh`, which copies from `~/个人项目/jev/地基` and is not edited from this repo) has three stale spots worth closing out at the source, in `jev/地基/` itself, not here: `待Nature裁定清单.md:62` and `14-实施计划-把语言做完整-v1.md:88` both still describe the de-identification question as undecided even though it was decided 2026-09-25, and `DECISIONS.md:1002` records a since-superseded "去标识提案完成" (de-identification proposal) framing that a reader could mistake for the final state. None of these are edited here since the next sync from the private workspace would overwrite an in-place fix; they need either a closing note or a `<!-- 公开替换 -->` marker at the source, then a resync.

**Title-integrity check on all 178 trimmed entries, not just a sample.** The extraction regex stops at the first `'.` after `includes '`, which would silently truncate a title containing that exact substring. Checked all 178: 40 distinct titles, all read complete against the source data, none contain an internal `'.`; and for every entry, confirmed the old (pre-trim) text right after the captured title actually starts the abstract (checked for an uppercase letter or "Background:" immediately following -- the only four apparent exceptions were abstracts starting with the digit "4" in "4D-STEM", verified by hand as correct, not truncated).

**What did not run: the demo results.** `results.json`, `reports.json.gz`, `recording.jsonl.gz`, `exploration.json` and `exploration-recording.jsonl.gz` are untouched and still reflect the pre-trim text. Every JEV request is cache-keyed by a hash of its exact input text (`fingerprint()` in `towow.py`), so trimming 178 of 325 profiles invalidates essentially the whole cache -- a real rerun needs live, paid JEV calls, which this change does not make. Two known consequences, left as-is on purpose: `tests/test_towow_public_artifacts.py::test_real_source_public_results_recompute_from_rankings` now fails because `results.json`'s recorded `data_sha256` no longer matches the trimmed `data.json` (confirmed both locally and in this PR's CI: the `offline (3.12)`/`offline (3.13)` jobs on PR #33 fail with exactly this one assertion, 1 failed / 560 passed; `rust-checks` and `rust-source` pass); and `recording.jsonl.gz` / `exploration-recording.jsonl.gz` still contain the full old abstract text in their recorded request bodies until a rerun happens, which `real/index.html`'s new notice now says explicitly. **PR #33 should not merge silently with this failure; pick one of: (a) full rerun, (b) reduced rerun, (c) merge knowingly with this one test red.** The live-call spend either (a) or (b) needs is within the project's standing $5-without-asking threshold, so the coordinating session can pick one directly rather than escalating to Nature; this task's own instructions were the specific reason no call was made without reporting the cost first. One thing to weigh for (c): once merged, `main`'s CI shows this one `offline` job red for every other agent's PR until a rerun lands, so (c) should come with a scheduled (a) or (b), not sit indefinitely.

**Rerun cost, not spent, with the prerequisites this estimate depends on.** Based on the existing recorded run: the full real-source pipeline (`cut20` + the full-pairwise `cut324` + `order20` + the `exploration` cross-source variant) previously cost about $1.49 cumulative for roughly 11,100 recorded requests, of which the full-pairwise `cut324` variant (105,300 judge sub-questions across all 324 candidates per person) is about $1.21 of that by itself. Trimmed text is about 16.7% shorter corpus-wide (25% shorter for the 178 academic entries: 77,774 to 58,297 characters); treat that as an upper bound on the discount, since prompt and question text are unchanged and only the profile text shrank, so a rerun would likely cost at or somewhat below these historical figures. Three things any rerun needs first, none of them optional: (1) `towow_real.py` hard-asserts the retrieval file's `data_sha256` matches `data.json` before it will run at all, so local retrieval (`benchmark_towow_real.py`: BM25 + MiniLM + RRF fusion, itself free, zero API calls) must be regenerated first -- installing `torch`/`sentence-transformers` and the MiniLM model weights (revision `e8f8c211…`) is free and local, it simply was not done here because the next step after it is the paid one this task's instructions said to stop before; (2) the "skip `cut324`" cheaper path is not a drop-in flag -- `build_towow_real.py` hardcodes `report-324.json` and the `cut324` method, and `real/index.html`'s "finding" sentence names it, so that option means editing the build script and the page copy, not just running fewer commands; `exploration.json` (~$0.04) stays cheap even after a rerun, since `towow_explore.py --journal` replays from whichever `order20` recording it is pointed at and only the ~2,172 genuinely cross-source questions need to go live -- with it included, that reduced path is roughly $0.27-0.28 against the historical baseline, for about 6,700 requests. (3) Per this project's own experiment discipline (地基 §5.2), the system-level hypothesis to pre-register before spending is already on record in this same public repository's `research/地基/DECISIONS.md:1005` (synced from the private workspace, not private itself): removing the shared-abstract text is expected to weaken exactly the `coauthor_same_field` and `github_same_org` relation-recovery signal, since those labels were largely inferred from the now-removed shared paper/employer text. A rerun should commit that prediction before looking at the new numbers, not after.

**The live public site is still serving the untrimmed data today.** GitHub Pages for this repository serves from `main:/docs` (confirmed via `gh api repos/Towow-ai/jpp/pages`). Until PR #33 merges, `https://towow-ai.github.io/jpp/demos/towow/real/` continues to serve the pre-trim `data.json` with full abstracts. Merging with the one known test failure (option (c) in the PR) is therefore the only option that removes the abstracts from the live site today; options (a)/(b) additionally require the paid rerun above first. This is within the project's standing $5-without-asking threshold (地基 §5.2), so the coordinating session can pick one directly rather than waiting on Nature.

[Implementation commit / 实现提交](https://github.com/Towow-ai/jpp/commit/b6738fa), [checked-in trim script](https://github.com/Towow-ai/jpp/commit/bb4a59a), [progress-log commits](https://github.com/Towow-ai/jpp/commit/1c1a22e) / [4592f18](https://github.com/Towow-ai/jpp/commit/4592f18) / [43b076e](https://github.com/Towow-ai/jpp/commit/43b076e), [PR #33](https://github.com/Towow-ai/jpp/pull/33)

## 2026-09-24: PRs #27-30 merged; new findings on whether the language has an effect yet / PR #27–30 合入；「有没有效果」的新发现

**What landed in this public repository today.** PRs [#27](https://github.com/Towow-ai/jpp/pull/27), [#28](https://github.com/Towow-ai/jpp/pull/28), [#29](https://github.com/Towow-ai/jpp/pull/29) and [#30](https://github.com/Towow-ai/jpp/pull/30) merged into `main` in dependency order; #30 itself includes two follow-up fixes from its Codex review (retry-billing correctness, and making the cost-reporting branch read the calibration record that was actually selected rather than an assumed one). `cargo test --workspace --offline` on the resulting `main`: **394 passed, 0 failed, 3 ignored**. This closes out everything summarized in the three 2026-09-23 entries below (rule batch, value-level taint, live backend, template-level calibration): those are in `main` now, not just synced from the research tree.

**A same-day architecture refactor, on a sync branch pending review.** The research tree spent today splitting its Rust kernel from 3 crates into 10 (`jpp-ir`, `jpp-value`, `jpp-effects`, `jpp-ledger`, `jpp-calib`, `jpp-check`, `jpp-plan`, `jpp-core`, `jpp-syntax` -- renamed from `jpp-frontend` -- and `jpp-cli`; the ruled cap is 11), rebuilding the intermediate representation and the ledger format, and adding structural provenance tracking (which judgment produced which downstream value, and how many chained layers deep a result sits). The code is on the same-day sync branch `sync/2026-09-24-architecture`, synced through research-tree commit `9716e61b` (steps 0 through 12e-2 of the refactor sequence, plus the interleaved steps 7b, 7c, 9a, 13a, 13b, 15d-0, 17a, 17c, 20b, 20d-1, 20e and 20f, the verification-dashboard scripts and `probes/`, and a new `GUIDE.md`); `cargo test --locked --workspace` on that branch: 579 passed, 0 failed, 3 ignored. It is waiting on review before being pushed and opened as a pull request; it is not yet in this repository's `main`. Anything below that names a crate, a step number, or a research-tree test count describes that branch.

**That branch's CI.** A new `rust-checks (report)` job runs the subset of the research tree's `scripts/ci.sh` that this public repository can run without a live backend -- formatting, lint, the dependency table, line-count and pattern checks, capability-profile cross-checks, and the equivalent-rewrite call-count pairs -- plus an actual run of the code fragments in `GUIDE.md`, `README.md` and `METHODS-AND-LIFECYCLE.md` and all bundled examples. It reports rather than blocks. First local run: 13 of 13 checks exited 0; of the documentation code fragments and examples, 40 passed, 1 did not, and 6 lines were skipped because they need the live backend. The one failure is `METHODS-AND-LIFECYCLE.md` still telling readers to run `cargo test -p jpp-frontend`, a crate that this refactor renamed to `jpp-syntax`. Two checks read above their recorded baseline: `grep_constants` at 140 (baseline 132) and `cargo fmt --check` flagging 75 files (baseline 74). All of this is on the pending sync branch, not in `main`.

**The finding that shaped today's design work.** An independent review of the research tree, timed right after the refactor's first milestone, found that the kernel's semantics hold up under controlled tests, but on the real JEV backend, every new question came back undecided -- zero decided outcomes, because no question had a calibrated threshold yet. The review also found the project's own fallback behavior (when a run has no capability profile) silently deciding outcomes through hardcoded constants, which the project's own design rules forbid. Three pieces of design work responded to this, each written up separately since each stands on its own:

- **A seven-item verification dashboard, and a fix to how the "how many times shorter" number is measured.** The project's target reference band (9x-20x shorter than hand-written code) turned out to apply only when the hand-written baseline also has to implement its own audit log, spending cap, request batching and calibrated threshold -- duties the project's early test tasks let the baseline skip. Task comparisons are now run in two tiers (a minimal brief and a fuller one that requires those four duties), and the line-count comparison itself is now normalized for line-wrapping width instead of counted raw, since J++ source runs measurably denser per line than the Python baselines. **Built, on the pending sync branch: the dashboard scripts. A first two-tier multi-implementation measurement has run in the research workspace.** The first reading under this rule: T1 (the deciding tier), wrap-normalized median 4.97x; T0, reported alongside, 1.36x. Both are below their reference bands (9x-20x for T1, 2.7x-4.3x for T0). The same day's independent diagnosis (ruling B96) traced about 0.9x-1.6x of the T1 figure to the measurement itself: the T1 task brief required the baseline's certification line (duty (d)) to reproduce `jpp calib-import` bit for bit, which raised the ratio by about 0.9x, and three J++ implementations that failed acceptance were still counted, which raised it by about 0.7x more. With both corrected, the T1 ratio is about 4.1x over all implementations and about 3.4x counting only implementations that pass acceptance. A re-measurement under the corrected rule (step 31-1b) is in progress. Other readings: probe comparisons at 2x-5x, two programs written during the review at 1.2x-1.5x, a live decided-exit share for new questions of 0.255 (14 of 55, counting earlier runs that were all cold), and a hop distribution of 22/12/4 judgments at one/two/three chained layers from fixed observations -- with these in, all seven dashboard items now have numbers. See [dashboard and expressiveness reference](updates/2026-09-24-dashboard-and-expressiveness-reference.md).
- **A "trial" calibration tier that unblocks the live backend.** Formal certification needed about 160 labeled examples; a new looser tier certifies at a wider (but still statistically bounded) error tolerance from far fewer, enough to route a program's control flow but explicitly barred from authorizing irreversible actions. **Built, on the pending sync branch**, along with per-exit reporting of which grade of calibration line backed each outcome and a rule requiring a capability profile on every live run. A live test with 80 constructed-truth customer-service dialogues got every outcome decided for the first time and reached a second chained judgment layer live for the first time, for under a fifth of a cent. A self-review caught and fixed a real gap before it shipped: trial recertification could have silently overwritten an already-suspended formal line. See [live trial-grade calibration lines](updates/2026-09-24-live-trial-lines.md).
- **Cutting the 160-example labeling threshold itself, without loosening the error bound.** Breaking the number apart found that roughly half the gap above the statistical minimum was an accident of a fixed random seed unevenly splitting the sample pool, not a real requirement. A different, literature-standard certification method (checking a fixed sequence of candidate thresholds against the same labeled sample instead of splitting it) was validated for free against all existing labeled data and cuts the requirement to about 60 examples. **Ruled, not built:** this method change, a companion sequential variant, and a fix to a separate specification gap in how a certified line's scope may be extended are written decisions, not yet implemented in the calibration code; live runs today still use the older, more expensive method. See [cutting the labeling threshold](updates/2026-09-24-labeling-threshold.md).

**State of the three acceptance criteria** ("shorter to write," "how deep a judgment chain can run," "swap the judge, program doesn't change"): expressiveness has partial evidence at 2x-5x on isolated probe comparisons, short of the 9x-20x target; the first T0/T1 multi-implementation reading is T1 4.97x and T0 1.36x, both below their reference bands; the same day's diagnosis (ruling B96) traced about 0.9x-1.6x of the T1 figure to the measurement, and the corrected T1 ratio is about 4.1x over all implementations and 3.4x over those that pass acceptance, with a re-measurement (step 31-1b) in progress. Depth evidence is a hop distribution from fixed observations: 22, 12 and 4 judgments at hops one, two and three; the per-hop undecided rate and the live depth curve are not measured yet. Capability-swap testing has exercised one of eight tracked capability assumptions.

---

**今天在这个公开仓库里合入的东西。** PR [#27](https://github.com/Towow-ai/jpp/pull/27)、[#28](https://github.com/Towow-ai/jpp/pull/28)、[#29](https://github.com/Towow-ai/jpp/pull/29)、[#30](https://github.com/Towow-ai/jpp/pull/30) 按依赖顺序合入 `main`；#30 本身就包含它评审后的两处后续修复（重试计费的正确性；让代价上报分支读取实际被选中的校准记录，而不是假定的那条）。合入后的 `main` 上 `cargo test --workspace --offline`：**394 通过、0 失败、3 忽略**。这收尾了下面 2026-09-23 三条条目里总结的全部内容（规则批、值级 taint、真实后端、题式级校准）——这些现在已经在 `main` 里，不只是同步自研究树。

**同一天的架构重构，在待审核的同步分支上。** 研究树今天把 Rust 内核从 3 个 crate 拆成 10 个（`jpp-ir`、`jpp-value`、`jpp-effects`、`jpp-ledger`、`jpp-calib`、`jpp-check`、`jpp-plan`、`jpp-core`、由 `jpp-frontend` 改名的 `jpp-syntax`、`jpp-cli`；已裁定的上限是 11 个），重建中间表示与账本格式，并加入结构化来源追踪（哪次判断产出了哪个下游值、一个结果链式地叠了几层）。代码在同日同步分支 `sync/2026-09-24-architecture` 上，同步到研究树提交 `9716e61b`（重构序列步 0 到 12e-2，加上交错插入的步 7b、7c、9a、13a、13b、15d-0、17a、17c、20b、20d-1、20e、20f、验收仪表脚本与 `probes/`、新增的 `GUIDE.md`）；该分支上 `cargo test --locked --workspace`：579 通过、0 失败、3 忽略。它在等待审核后推送并开 PR，还没有进本仓库 `main`。下文提到的 crate 名称、步骤编号或研究树测试数，说的都是这个分支。

**这个分支上的 CI。** 新增的 `rust-checks (report)` 作业跑研究树 `scripts/ci.sh` 里公开仓库不用真机也能跑的子集——格式、lint、依赖表、行数与模式类检查、能力画像核对、等价写法调用数对——并实际跑 `GUIDE.md`、`README.md`、`METHODS-AND-LIFECYCLE.md` 里的代码片段与全部随附示例。报告模式，不拦截。本地首跑：13 项检查全部退出 0；文档代码片段与示例里，40 项通过、1 项未通过、6 行因为需要真机而跳过。未通过的那一项是 `METHODS-AND-LIFECYCLE.md` 仍写着 `cargo test -p jpp-frontend`，而这次重构已经把这个 crate 改名为 `jpp-syntax`。两项读数高于记录的基线：`grep_constants` 140（基线 132）、`cargo fmt --check` 标出 75 个文件（基线 74）。以上全部在待合入的同步分支上，不在 `main` 里。

**今天设计工作的起点。** 一轮针对研究树重构第一个里程碑之后的独立复核发现：内核语义在受控测试下成立，但接上真实 JEV 后端后，任何新题的出口都是未决——已决出口是零个，因为还没有题有校准阈值。复核还发现项目自己的兜底行为（运行没有能力画像时）正在悄悄用硬编码常数决定出口，这正是项目自己的设计规则明令禁止的事。三项设计工作对此做出回应，各自成篇，因为各自独立成立：

- **一套七项验收仪表，以及对「短多少倍」这个数字量法的修正。** 项目的目标参考带（比手写代码短 9–20 倍）原来只在手写基线也要自己实现审计日志、花费上限、请求合批和校准阈值——这些是项目早期测试任务允许基线跳过的职责——时才适用。任务对照现在分两档运行（一份最小任务书，一份要求这四件事的完整任务书），行数对照本身也改成按折行宽度归一后再数，而不是数原始行数，因为 J++ 源码每行的密度明显高于 Python 基线。**已造出，在待合入的同步分支上：仪表脚本。第一次两档多实现测量已在研究工作区跑过。**按这条规则的第一次读数：T1（判定档）折行归一中位 4.97×，T0（并列报告）1.36×，都低于各自的参考带（T1 对 9–20×，T0 对 2.7–4.3×）。当天的独立诊断（裁定 B96）查出 T1 读数里约 0.9–1.6 倍来自量法本身：一是 T1 任务书要求基线的认证线（职责 (d)）按 `jpp calib-import` 逐位复刻，使比值抬高约 0.9；二是三份没通过验收的 J++ 实现也计入了，又抬高约 0.7。两处都纠正后，全部实现约 4.1×，只算通过验收的约 3.4×。按新口径的重测（步 31-1b）正在进行。其余读数：孤立探针对照 2–5 倍、评估时试写的两个程序 1.2–1.5 倍、真机新题已决出口占比 0.255（14/55，含此前全冷的运行）、固定观察下一/二/三跳的判断数 22/12/4——加上这些，仪表七项现在全部有数。见[验收仪表与表达量参考系](updates/2026-09-24-dashboard-and-expressiveness-reference.md)。
- **一档解除真机阻塞的「试用」校准等级。** 正式认证原来要约 160 条标注；新增的更松等级用更宽（但仍受统计边界约束）的错误容忍、少得多的样本即可认证，够给程序控制流分派路由，但明确不许用来放行不可逆动作。**已造出，在待合入的同步分支上**，随附逐出口记录背后校准线等级的报告，以及真机运行必须带能力画像的规则。一次用 80 段构造真值客服对话的真机测试第一次让全部出口都得到已决结果，也第一次在真机上走到链式判断的第二层，花费不到千分之二美元。上线前的一次自查抓到并修复了一个真实缺口：试用档的重新认证本可能悄悄覆盖一条已停岗的正式线。见[真机试用档校准线](updates/2026-09-24-live-trial-lines.md)。
- **把 160 条标注门槛本身降下来，安全边界不放松。** 把这个数字拆开后发现，超出统计最小值之上的差距里大约一半是固定随机种子把样本池切得不均匀造成的意外，不是真正的要求。一种不同的、文献里的标准认证方法（在同一批标注样本上检验一个固定的候选阈值序列，而不是切分样本）用项目已有的全部标注数据做了零成本离线验证，把门槛降到约 60 条。**已裁定，未造出：** 这项方法改动、一个配套的序贯变体，以及另一处「已认证线扩展适用范围」规格缺口的修法，都是已写下的决定，还没有在校准代码里实现；今天的真机运行仍在用较旧、更费样本的方法。见[压低标注门槛](updates/2026-09-24-labeling-threshold.md)。

**三条验收标准的现状**（「写得更短」「判断链能跑多深」「换判断器程序不改」）：表达量在孤立探针对照上有 2–5 倍的部分证据，还没到 9–20 倍的目标；按 T0/T1 做的第一次多实现测量读数是 T1 4.97×、T0 1.36×，都低于参考带；当天诊断（裁定 B96）查出 T1 读数里约 0.9–1.6 倍来自量法，纠正后约 4.1×（全部实现）、3.4×（只算通过验收的），按新口径的重测（步 31-1b）正在进行。深度证据是固定观察下的跳数分布：一、二、三跳的判断数为 22、12、4；逐跳未决率与真机深度曲线还没测。能力更换测试目前走通了追踪的八类能力假设中的一类。

## 2026-09-23: fail-open fixes, value-level taint, and five rule-batch items / 放行缺陷修复、值级 taint 与规则批五项

Synced from the research tree through commit `a17596a` (rules batch B29/B25/B28/B32/B3 and the
B33 value-level taint switch; B24 split-sample certification was already synced in an earlier
sync). Fixed two fail-open defects: `speculate`/`vectorize` executed `do` on a branch's state
expression before the branch's own judgement had decided the branch should run (effect analysis
treated calls to user-defined functions as pure); and untrusted content was laundered "trusted"
through concatenation (`+`), `join`, `text()`, `m.content`, or a failed untrusted `do`'s error
text, letting a `mat()` built from it pass the irreversible-`do` gate. The first fix used a side
table of untrusted text leaves matched by substring; measured against ten probe cases it was
wrong in both directions -- a one-digit untrusted number survived `text()` concatenation and
still laundered clean (false accept), while program literals that merely shared a substring with
previously-read untrusted text were rejected (false reject). It was replaced with value-level
taint: every scalar `Value` carries a taint bit, propagated through binary/unary operators and
built-in dispatch (an explicit exception list exempts pass-through built-ins such as `map` and
`filter`), and read out at `content()`, `m.content`, `text()`, and `Fail`. Five rule-batch items
landed: B29 makes host `put` write fixture-only records that cannot license an irreversible `do`;
B25 auto-flags suspension candidates on drift and adds a `calib-confirm` command for the human
decision; B28 replaces `agg` with `repeat` (mean or median only, no majority vote) and keys
merged readings by sample count; B32 adds judgement-absence handling (retry, backoff, escalate,
circuit breaker) and a static latency budget; B3 gives `unsure` two named causes, `rejected_all`
and `no_candidate`, with library routes for both. B30 (a deterministic tuple-based calibration
key) was ruled necessary but is not built -- the language surface already lets authors write
their own calibration keys (`test(question, "k")`), and reconciling that with a mandated tuple
serialization needs a scope decision first; it is paused, not silently dropped. The Codex review
comments on PRs [#27](https://github.com/Towow-ai/jpp/pull/27) and
[#28](https://github.com/Towow-ai/jpp/pull/28) were addressed with local, test-backed fixes (see
the PR threads for the itemized replies). `cargo build` and `cargo test --workspace --offline`:
382 passed, 0 failed, 3 ignored. Architecture and engineering reorganization plans are still
being finalized in the research workspace and are not part of this sync. Full account:
[rules batch and value-level taint](updates/2026-09-23-rules-and-taint.md).

同步来源是研究树，截至提交 `a17596a`（规则批 B29/B25/B28/B32/B3 与 B33 值级 taint 切换；B24
拆分样本认证此前已在更早一次同步中带过）。修复了两类放行方向缺陷：`speculate`/`vectorize`
在分支自己的判断决定要不要走之前，提前对分支状态表达式求值并执行了里面的 `do`（效应分析把
调用用户函数一律当纯）；不可信内容经 `+` 拼接、`join`、`text()`、`m.content`，或不可信 `do`
失败后的错误文本，被洗成「可信」，凭它构造的 `mat()` 越过了不可逆 `do` 的关卡。第一版修复用
旁路表（记不可信文本叶子做子串匹配），十个探针案例实测两个方向都错——一位数不可信数值经
`text()` 拼接后仍被洗白（假放行），程序自己的字面量因与读过的不可信文本共享子串被误拦（假
拒绝）。换成值级 taint：每个标量 `Value` 自带一位 taint，经二元/一元运算与内置分派传播
（`map`/`filter` 等只搬运元素的内置单列例外表），在 `content()`/`m.content`/`text()`/`Fail`
处读出。规则批五项落地：B29 让宿主 `put` 只写夹具记录，夹具线不得放行不可逆 `do`；B25 按漂移
自动标记停岗候选，加 `calib-confirm` 命令交人确认；B28 用 `repeat`（只许均值或中位数，禁众数）
取代 `agg`，合并读数按样本数独立开校准键；B32 加判断力缺席处理（重试/退避/升级/熔断）与静态
时延预算；B3 给 `unsure` 补 `rejected_all` 与 `no_candidate` 两个具名原因及库内去向。B30（确定
性元组校准键）已裁定要做但未造——语言表层已经让作者自己写校准键（`test(题面, "k")`），要与
「代码键必须是元组序列化」的条文对齐，得先裁定作者键的地位，此项暂停、不是被悄悄丢下。Codex
在 PR [#27](https://github.com/Towow-ai/jpp/pull/27) 与
[#28](https://github.com/Towow-ai/jpp/pull/28) 上的评审意见已逐条本地修复并配回归测试（逐条
回复见两个 PR 讨论串）。`cargo build` 与 `cargo test --workspace --offline`：382 通过、0 失败、
3 忽略。架构与工程方案的重整仍在研究区定稿中，未随本次同步公开。完整说明见
[规则批与值级 taint](updates/2026-09-23-rules-and-taint.md)。

## 2026-09-23: constructs, live backend and template-level calibration / 构造施工、真实后端与题式级校准

Synced from the research tree through commit `42988c5`. Built: the real JEV backend
(`--backend live`), questions as first-class values (`form`/`fill`), a three-way sieve that
takes questions directly plus review-opinion material, pairing (`pair`), set aggregation
(`tally`/`first_k`), bounded iteration with a shrink line (`iterate`), calibration intake
(`calib-import`, the truth channel) with form-level line fallback and split-sample two-sided
certification (B24), and the composition-closure contract (B17) shared by every set-level
construct. Found: the live backend returns correct readings but every exit is
`unsure(cold)` without a calibration record; literal question templates read bimodally and
need only a global line; semantic templates need real calibration, misclassifications
persist across reruns, and re-asking does not help; human spot-checking showed the
disagreement was an undefined question scope, not labelling noise — splitting the template
resolved it, and the topic-relevance template reached 30/30 spot-check agreement and is
certified. Designed in response: the question template, not the literal question, is the
calibration primary key (B2); split-sample certification with the gate read off a one-sided
95% confidence lower bound, not the raw agreement rate (B19/B24); the composition-closure
contract; and several changes carried from a four-line question-theory literature review.
Unfinished: of 415 design-ledger items, 89 are built; two classes of fail-open defects
(`speculate` executing `do` on a branch that should not run; untrusted content becoming
"trusted" through concatenation/join/failure paths) are ruled to need fixes but are not
fixed yet; B28–B32 are decided but not yet implemented. `cargo test --workspace --offline`:
341 passed, 0 failed, 3 ignored. Full account: [constructs and calibration](updates/2026-09-23-constructs-and-calibration.md).

同步来源是研究树，截至提交 `42988c5`。做成了：真实 JEV 后端（`--backend live`）、题成为
一等值（`form`/`fill`）、三路过滤直接吃题并把评审意见渲染成材料、配对 `pair`、聚合
`tally`/`first_k`、带收缩终止线的迭代 `iterate`、校准进料 `calib-import`（真值通道，带
题式级线回退与拆分样本两侧认证 B24）、以及所有集合级构造共用的组合封闭性契约（B17）。
发现了：真机读数本身正确，但没有校准记录时出口全是 `unsure(cold)`；字面题式读数两极，
一条全局线就够；语义题式需要真正的校准，错判在重跑间持续存在，重复提问无效；人工抽检
揭示分歧来自题面外延未定，不是标注噪声——拆题后话题相关题式抽检 30/30 一致并转正上岗。
针对问题设计了：题式而非字面题作校准主键（B2）；拆分样本认证，上岗门槛看抽检一致率的
单侧 95% 置信下界而不是原始一致率（B19/B24）；组合封闭性契约；以及四线问题理论调研带来
的多处改动。未完成：设计总账 415 条中已造出 89 条；两类放行方向缺陷（`speculate` 在不该
执行的分支上执行 `do`；不可信内容经拼接/join/失败路径变「可信」后越过不可逆 `do` 关卡）
已裁定要修但尚未修好；B28–B32 已定未造。`cargo test --workspace --offline`：341 通过、
0 失败、3 忽略。完整说明见[构造施工与校准](updates/2026-09-23-constructs-and-calibration.md)。

## 2026-09-23: sync research-tree runtime increments / 同步研究树运行时增量

Port the research tree's later Rust increments that the public tree lacked: the static half of J-10 (`budget.unsure` in the AST and a pre-call warning that sums each judge site's `unsure_rate`), a real producer for `CalibRecord.unsure_rate` when `commission` certifies a line, and drift warnings on `allocate`/`unsure_bound` as well as `cut`. The public review fixes (log-space binomial upper bound, the all-reject threshold, portable test paths and fixtures, honest certificate disclosures) are kept, not overwritten. `.jpp` source still cannot write `budget.unsure`; the frontend lowers it as absent. `cargo test --workspace`: 318 passed, 0 failed, 3 ignored.

把研究树里公开仓库缺少的 Rust 增量搬过来：J-10 的静态部分（AST 增加 `budget.unsure`，在任何模型调用之前按各判断位置的 `unsure_rate` 求和并告警）；`commission` 认证一条线时真正写出 `CalibRecord.unsure_rate`；漂移告警从 `cut` 扩到 `allocate` 与 `unsure_bound`。公开侧已有的审查修复（对数空间二项上界、全拒绝阈值、可移植的测试路径与夹具、如实的证书说明）全部保留，未被覆盖。`.jpp` 源码目前还写不出 `budget.unsure`，前端按缺省处理。`cargo test --workspace`：318 通过、0 失败、3 忽略。

## 2026-09-23: review and integrate the open PRs / 审查并整合待合入 PR

#17's community examples, #25's Rust synchronization and #20's design-revision record
are merged. #25 received public-test portability repairs, a rebuilt browser bundle and
two reproduced numerical fixes; #20 keeps an enabled exact overflow-span regression.
Rust and Python 3.12/3.13 CI pass. #26 reconciles the current design map and implementation
handoff with that public baseline; local links in its changed documents were checked.
See the [review record](updates/2026-09-23-pr-integration.md).

#17 社区示例、#25 Rust 同步、#20 设计记录已合入。修复公开测试路径、浏览器源码包
及两处数值边界错误，补上整数溢出准确位置回归；Rust 和 Python 双版本 CI 通过。
#26 将总设计图与下一段实施任务对齐这一公开基线。统计选线的一般保证、真机 CLI
和后续研究增量仍分别列为未完成工作，不混成已交付能力。

## 2026-09-23: correct stable documentation drift / 校正稳定文档漂移

Check stable documents against code and available execution evidence. Correct source-example and `ask` coverage descriptions in the research tree, distinguish old backlog snapshots from current work, and record the bounded OCaml paper exploration. Retain unfinished experiments and the research/public-runtime boundary. No runtime changed. [Correction details](updates/2026-09-23-documentation-drift.md).

对照代码与已有执行证据，修正研究区示例数量和 `ask` 覆盖表述，标清旧待办快照，补记 OCaml 纸面探索的阶段边界。未验证实验与研究/公开版本区别保留，不修改运行代码。[校正明细](updates/2026-09-23-documentation-drift.md)。

## 2026-09-23: concrete implementation handoff / 下一段具体施工交接

Prepare the next implementation package: shared native source constructions, two programs whose results compose again, followed by real JEV CLI integration and a consolidated release. Identify existing code and observable acceptance behavior; retire stale waiting instructions in the local handoff. This prepares work without launching an executor or changing runtime behavior. [Implementation task](implementation-handoff-2026-09-23.zh-CN.md).

明确下一段施工：共同原生源码构造、两份组合结果能再次组合的程序，随后真实 JEV CLI 接线与整版交付。列出现有代码及行为验收，并在本地入口更新历史等待状态。本轮准备任务，尚未启动执行或修改运行代码。[具体实施任务](implementation-handoff-2026-09-23.zh-CN.md)。

## 2026-09-23: overall design and delivery map / 总设计与交付地图

Connect the original five-step implementation plan to the current language layers, verified example behavior, remaining construction work and publication status. Distinguish semantic collection capabilities from similarly named list/reading operations, and propose completion packages without replacing the current contracts. This is documentation only: existing verification records and selected source were inspected; no new runtime change or model experiment was performed. [Read the map](design-and-delivery-map.zh-CN.md).

将原五步实施计划、语言各层、已有程序效果、剩余建设及公开状态连成一张图。澄清语义集合能力与同名列表/读数操作的区别，整理后续交付包，继续沿用现行契约。本轮只更新文档，读取已有验证记录并核对部分源码，没有改运行代码或进行新模型实验。[总设计与交付地图](design-and-delivery-map.zh-CN.md)。

## 2026-09-23: complex composition and a verified foundation / 复杂组合定位与地基现状核对

Clarify the target: a small set of underlying constructs should generate many complex algorithms, and composed results should compose again. Consolidate the external research as reference material. Re-run the local research Rust workspace at `520fef2`: 315 tests passed, zero failed, three ignored; direct examples reproduce nested methods, adaptive questions, partial continuation and zero-call replay. These are research-workspace results, not a new public runtime release or a live-model benchmark. The first complete language body remains unfinished. [Modules, effects, work distribution and remaining scope](updates/2026-09-23-composable-foundation-status.md).

明确目标：少量底层构造支撑多种复杂算法，组合结果继续组合；外部调查整理为参考依据。本地研究 Rust `520fef2` 复跑315项通过、0失败、3忽略，直接复现方法再组合、自适应选问、部分结果续接和零调用重放。这是研究工作区验证，不是新公开运行版本或模型基准；第一版完整主体仍未完成。[模块、效果、工作分布与剩余范围](updates/2026-09-23-composable-foundation-status.md)。

## 2026-09-23: source-based ecosystem reassessment / 根据公开源码重新核对生态需求

Fresh public-source collection and static reviews revise earlier ecosystem claims: typed question composition, adaptive algorithms, caching, budgets and several fallback policies already have implementations. Developer requests, implemented responses and inferred needs are separated; public issue counts are not treated as independent demand votes. Compatible local endpoints differ in capability and confidence semantics. The next proposed comparisons test complex composition, evidence coverage, budgets, cache costs and real effects. No downloaded project or model benchmark was executed, and no runtime changed in this publication. [Evidence and implications](updates/2026-09-23-ecosystem-reassessment.md).

重新采集公开源码并深读，订正此前判断：类型化题集、自适应算法、缓存、预算和多种失败处理已有实现。明确请求、已有应对与推断需求分开记录，不将 issue 数量当独立需求票数；本地兼容接口也不能抹平能力及置信度含义的差异。后续对照围绕复杂组合、输入覆盖、预算、缓存代价和真实动作。本轮未执行第三方项目或模型基准，公开同步未改运行代码。[证据与建设影响](updates/2026-09-23-ecosystem-reassessment.md)。
## 2026-09-23: reconcile the design-revision PR / 整理设计修订 PR

PR #20 now preserves the current `12`/`13` authority documents while retaining its dated
design report. The three defects are already fixed by #25 and covered in `v13_rules.rs`;
obsolete ignored reproducers are replaced by an enabled overflow regression that checks
the runtime error variant and exact source span. A checker rejection cannot pass this
test. Current status text now reflects the merged implementation.

保留现行规范和原始设计报告，移除已被后续回归覆盖的三条过期忽略测试；补验整数溢出
确实返回运行错误并指向准确源码位置，不会因其他静态错误而误判通过。当前状态说明同步
到已合入的 Rust 实现，历史数字保留并标明日期。

## 2026-09-23: review the native kernel sync / 审查原生内核同步

PR #25 brings the later Rust checker/runtime and calibration host APIs into the public
tree. Review fixed test paths that depended on the private research layout, supplied
minimal legacy-schema fixtures, rebuilt the browser source bundle, and fixed two numeric
boundary defects: large-sample binomial underflow and a reject-all threshold that could
accidentally accept score 1. Each numeric failure was reproduced before its fix.
The review checkout passed 304 Rust tests (3 ignored) and 544 Python tests on Python 3.12
before integrating the separately reviewed community examples from #17. GitHub CI checks
the combined branch. Private-data probes and synthetic tests do not establish live model
accuracy. Selected-threshold risk certification remains experimental; see the precise
limitations in [Rust status](../rust/README-status.md).

本次将后续 Rust 实现同步到公开仓库，修正测试对私有目录的依赖，补齐旧记录格式夹具，
更新浏览器源码包，并修复大样本二项上界下溢、全拒绝阈值误放行满分样本两处边界错误。
两处数值问题均先复现失败再修复。隔离副本通过 304 项 Rust 测试（3 项忽略）和
Python 3.12 的 544 项测试；随后纳入已单独验证的 #17 社区示例，组合结果由 CI 复查。
当前统计选线仍属实验实现，不能把单一合成分布测试称为一般风险保证。

## 2026-09-21: a second authority text and a plan to finish the language / 第二份依据与把语言做完整的实施计划

Research documents only this round; no runtime in this repository changed. `13-Rust实践反馈设计修订-v0.2.md` is published for the first time — six rules that a day of building in Rust forced onto the design, three of them correctness defects (a method's identity omitted its captured state and reused the wrong result; going over budget discarded a call that had already been paid for; integer overflow behaved differently in debug and release). `14-实施计划-把语言做完整-v1.md` sets out what remains, including what this version deliberately does not do and why. The inventory rounds behind it are reported with their own two defects: the new authority text was missing from the first round's material list, so that round's "conflict" findings are not used, and adversarial review failed all nine audits. The most useful output was 22 places where taste had been recorded as mechanism, several of them ours — the test being whether a rule can say what would turn what red. Applying that test found three seams where the kernel silently flattened a three-valued fact to two, the worst of them laundering a material's provenance in two lines of ordinary source; all three are fixed, and the Python reference turned out not to have the taint hole — the port introduced it. 544 tests pass on Python 3.12 and 3.13 in this repository, unchanged by the sync. The kernel work described is in the research workspace and **is not in this repository's `rust/`**; merging the two lines is a separate item. Details, numbers and what was verified where: [2026-09-21 update](updates/2026-09-21-language-completion-plan.md).

本轮只同步研究文档，本仓库运行代码未变。`13-Rust实践反馈设计修订-v0.2.md` 首次公开——六条由一天 Rust 施工逼出来的局部修订，其中三条是正确性缺陷（方法身份不含捕获状态，复用了错的结果；超预算时把已经付过钱的调用丢掉；整数溢出在 debug 与 release 行为不同）。`14-实施计划-把语言做完整-v1.md` 排出剩下要做的，并写明本版有意不做哪些、为什么。支撑它的两轮盘点连同自身的两处缺陷一起公开：新依据缺席于第一轮的材料清单，故该轮「冲突」类结论不采信；对抗复核把九份审计全部判为不通过。最有价值的产出是 22 处把 taste 记成机制的地方，其中几处是我们自己写的——判据是这条规则能不能说出「什么情况下它会让什么东西变红」。照这条判据又查出三处把三值静默压成两值的缝，最重的一条两行普通源码就能洗白材料的来源可信度；三条都已修，且 Python 参照实现没有那个 taint 洞——是移植时新引入的。本仓库在 Python 3.12 与 3.13 上各通过 544 项测试，同步未改变这个数。文中描述的内核工作在研究工作区，**不在本仓库的 `rust/` 里**，两条线的合并是单独一项。细节、数字与「哪个数在哪里验的」见 [2026-09-21 更新](updates/2026-09-21-language-completion-plan.md)。


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

No new runtime capability this round. E9f-2b′ **failed**: predicting which paragraphs an author will ask to change is not decidable in one literal hop (n = 1,564 paragraphs, AUC 0.541 against a bet of 0.70; a length-and-digits heuristic scored higher at ~~0.638~~ **[0.643 — see the correction entry below]**, and a `haiku` judge sat on the random line too). E-CAL's final run passed none of its falsification criteria and about half its bets: Chinese `noul` readings are usable as probabilities (ECE 0.057), ~~`choice` showed no first-position bias (permutation consistency 1.000)~~ **[withdrawn 2026-09-21 — measurement artefact; see the correction entry below]**, `score` hit the adjacent band 0.964 — while `noul` AUC (0.748), `choice` argmax (0.757) and `score` MAE (0.553) all came in under their bets, and the 300 items turned out to be built from about 45 independent paragraphs, so effective n is 17–26 rather than 100. **[Scope added 2026-09-21: the calibration set behind every number in this paragraph is the both-models-agree subset, not a random sample, and is optimistically biased by an amount now measured for two of the three question types — see the entry below.]** The formal kernel moves to Rust ([ADR 0001](adr/0001-rust-kernel.md)); its core is still being built in the research workspace and ~~**no Rust source is published yet**~~ **[overtaken the same day — a Rust workspace landed under `rust/` on the front-end line; see the Status block in that update]**. 535 tests pass on Python 3.12 and 3.13 in this repository. Details and every number: [2026-09-21 update](updates/2026-09-21-two-experiments-and-rust-start.md).

本轮没有新的运行能力。E9f-2b′ **失败**：段级预测「作者会要求改这一段吗」在一跳字面下不可判（n = 1,564 段，AUC 0.541，赌的是 0.70；长度加数字的启发式基线反而更高，~~0.638~~**〔更正为 0.643，见下方更正条目〕**；haiku 裁判同样在随机线上）。E-CAL 正式版三条证伪判据一条都没触发、赌值对了一半：中文 `noul` 读数可以当概率用（ECE 0.057），~~`choice` 无首位偏置（置换一致 1.000）~~**〔2026-09-21 作废：测量假象，见下方更正条目〕**，`score` 相邻档 0.964；但 `noul` AUC 0.748、`choice` argmax 0.757、`score` MAE 0.553 全部低于赌值，且 300 条题面只由约 45 个独立段落重组而成，有效 n 在 17–26 之间而不是 100。**〔范围补注，2026-09-21：本段每个数字背后的校准集都是「两模型都同意」的子集，不是随机样本，同向乐观有偏，偏多少对三种题型里的两种已经测出来——见下方条目。〕**正式内核转 Rust（[ADR 0001](adr/0001-rust-kernel.md)），core 仍在研究工作区建设中，~~**Rust 源码尚未公开同步**~~**〔当日即被事实追上：前端那条线已把 Rust 工作区推到 `rust/` 下，见该更新的「现状」块〕**。本仓库在 Python 3.12 与 3.13 上各通过 535 项测试。细节与全部数字见 [2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)。

### 2026-09-21: correcting a published result / 更正一条已发布的结论

Two `choice` results published in the entry above are withdrawn. Of the 97
`select` items in E-CAL, only 8 emitted a `choice` physical question; the rest
had long candidates and were lowered by the compiler to per-candidate `noul`,
whose `mode_share` is hard-coded to 1.0 — which is exactly the permutation
consistency test, so 67 of 74 items were vacuously consistent. The real
measurement is 7 items **[this figure was itself wrong, corrected twice more the
same day — the real measurement is 8, not 7 and not 0; see
`docs/updates/2026-09-21-second-correction-and-kernel-progress.md` §1]**, and "no first-position bias" is void because per-candidate
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
1.0——「置换一致」的判据恰好就是它，所以 74 条里 67 条是恒真项，真测量只有 7 条
**〔这个数本身也是错的，同一天又更正了两次，最终值是 8，不是 7 也不是 0，见
`docs/updates/2026-09-21-second-correction-and-kernel-progress.md` §1〕**；
「无首位偏置」直接无效，因为逐候选 noul 根本没有位置，在真跑了 choice 的 8 条上首位
被选 3 次、真值首位 1 次，方向反而朝着有偏置。同一轮还更正 E9f-2b′ 的账：花费
$0.0027 → **$0.045**、调用 1,633 次（实验脚本的 `reset_stats()` 望远镜求和，不是超
支，两条预算上限都没破）、C_S ≈ 5.5 s 作废、随机臂与 Jev **不可分辨**而非更好、haiku
价格改为区间 $1.3–$12.9。**两个证伪结论都不变**，E9f-2b′ 仍是 FAIL。错的句子留在原处
标明，不删：[2026-09-21 更新](updates/2026-09-21-two-experiments-and-rust-start.md)
每节加了更正块，四个研究文件一并重新同步（含两个后续实验的预注册）。本次只改文档，`src/` 未动；Python 3.12 与
3.13 各通过 544 项测试。

### 2026-09-21: a scope for four published numbers, a conformal design result, and a withdrawn labelling plan / 四个已发布数字的适用范围、一个保形设计结论、一条被撤回的标注计划

The `noul` ECE 0.057, `noul` AUC 0.748, `choice` argmax 0.757 and `score`
adjacent-band 0.964 numbers above need a scope they did not have: E-CAL's
ground truth is dual-model labelling, and the 202 items that carry it agree
100.0% of the time between the two labelling models, against 81.1% among the
95 that do not — the calibration set is, by construction, the subset the
models agree on, not a random sample, and every number computed on it is
optimistically biased. **This does not withdraw the numbers**; it scopes them.
The bias direction is now measured, not just hypothesized, for two of the
three question types — `noul`'s error rate reads as **at least 0.392, not
0.301** — while the same free predictor **reverses on `score`**, exactly the
type with the least labelling coverage. A new design,
`设计/保形弃权域-设计-2026-09-21.md`, asks whether conformal risk control can
turn these thresholds into finite-sample guarantees on the 297 already-paid-for
readings: it **cannot**, for any of the three question types (tightest bounds
0.319 / 0.269 / 0.251, so any ≤20% target is infeasible), with the same scope
above applying to those three bounds too. A prototype crate
(`foundation/experiments/conformal-proto/`) ships alongside it — not part of
`rust/`, not built by this repository's own tests — currently 11 passed, 0
failed (two tests that once demonstrated gaps since closed by the
certificate gate and the certificate-addressing fix it argues for, rewritten
as regression guards). The pre-registered plan to close the scope
by labelling 22 more `noul` items is **withdrawn**: those 22 items do not
exist in the material; a real path (`E-NOUL-HI`) is pre-registered in their
place. Documentation only — `src/` unchanged. Full account:
[2026-09-21 update](updates/2026-09-21-scope-note-and-conformal-fail.md).

上面 noul ECE 0.057、noul AUC 0.748、choice argmax 0.757、score 相邻档 0.964
这四个数需要一个此前没写的适用范围：E-CAL 的真值是模型双标，有真值的 202 条里两个标
注模型一致率 100.0%，没有真值的 95 条里只有 81.1%——校准集在定义上就是「两模型都同
意」的子集，不是随机样本，算在它上面的每个数都同向乐观有偏。**这不是把那些数作
废**，是给它们加范围。偏倚方向现在对三种题型里的两种已经测出来，不再只是假设——
noul 的错误率要读成**至少 0.392，不是 0.301**——而同一个免费预测器在 **score 上反
向**，恰恰是标注覆盖最低的那一型。新设计 `设计/保形弃权域-设计-2026-09-21.md` 问：
保形风险控制能不能把这些阈值变成带有限样本保证的数字，用的是已付费的 297 条读数：
**不能**，三种题型都不能（最紧上界 0.319 / 0.269 / 0.251，任何 ≤20% 目标都无解），
上面同一条范围同样适用于这三个上界。配套的原型 crate
（`foundation/experiments/conformal-proto/`）不属于 `rust/`，也不在本仓库自己的测试
范围内——当前 11 通过、0 失败（两条当初测缺口的测试，缺口分别被证书门和证书寻址修
法堵上后，已改写成回归保护）。原定
靠再标 22 条 `noul` 来拆掉范围的计划**已撤回**：那 22 条在材料里不存在；换成预注册
`E-NOUL-HI` 里的真路径。本次只改文档，`src/` 未动。完整内容见
[2026-09-21 更新](updates/2026-09-21-scope-note-and-conformal-fail.md)。

### 2026-09-21: a second reader, a silent divergence, and a sync tool that now covers what it publishes / 第二个读者、一处静默分叉、一个学会覆盖自己发布内容的同步工具

Three small follow-ups to the entry above. `conformal-proto` carried its own
copy of eight items also defined in `jpp_core::conformal`, and the copy had
already silently diverged from the kernel (a parameter renamed `delta` →
`conf_delta` in the kernel — a deliberate distinction between the conformal
bound's confidence level and the calibration archive's hysteresis bandwidth —
never propagated to the copy, and never able to, since a rename in one file
cannot break compilation in an unrelated one). Fixed by deleting the eight
copies and re-exporting the kernel's module instead: **11 passed, 0 failed**,
verified item-by-item byte-identical first. `设计/G3b-零上下文读者第三次-迟到副本.md`
is added — a second, independent zero-context reader given the identical
exercise as the already-published `G3`, held back only because it finished
later; publishing only the faster of two readings is an unchosen selection
rule with the same shape as this round's calibration-set finding. And
`tools/sync-from-workspace.sh` now mirrors `conformal-proto/` itself, instead
of that directory needing a hand copy every round. Documentation and tooling
only — `src/` unchanged. Full account:
[2026-09-21 update](updates/2026-09-21-scope-note-and-conformal-fail.md) §6.

对上一条的三处小追加。`conformal-proto` 曾自带八项与 `jpp_core::conformal` 同名的
拷贝，而这份拷贝已经静默分叉（内核把一个参数从 `delta` 改名为 `conf_delta`——这是
一条刻意的区分：保形阈值的置信水平与档案的迟滞带宽是两个不同的保证——但这条判断从
未传到拷贝上，而且永远不会传过去，因为一个文件里的改名不会让另一个无关文件编译不
过）。修法是删掉八份拷贝，改成引用内核的模块：**11 通过、0 失败**，改之前先逐项核
对确认逐字节相同。`设计/G3b-零上下文读者第三次-迟到副本.md` 本轮加入——一次给了
和已发布的 `G3` 完全相同题目的第二个独立零上下文读者，只是交得晚，此前一直没发。只
发两次阅读里跑得快的那一份，是一条没人选过的选择规则，和这一轮校准集那条发现是同一
个形状。`tools/sync-from-workspace.sh` 现在自己会镜像 `conformal-proto/`，不用每轮
手动复制。本次只改文档与同步工具，`src/` 未动。完整内容见
[2026-09-21 更新](updates/2026-09-21-scope-note-and-conformal-fail.md) 第六节。

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
