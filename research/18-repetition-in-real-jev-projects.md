# 18. How people actually use JEV: the repeated parts

2026-09-27. Per item 22 of `research/地基/00-Nature意图汇编.md` (Nature's compiled intent notes): "Analyze how everyone is actually using JEV today, and find where the repetition is... I can't just say abstractly 'you need to write what to judge' — that gives no feel for it. We need examples... so people can see that across this many projects, they're all doing the same thing, and here's where the repetition is. Then why does the mechanism we're proposing... let it not do that repetition? And what's the effect?" "You have to argue it — you have to show 'how much of their development effort this actually took up.'"

Method and judgment criteria: `research/地基/00-定位与方法论-v1.md` (positioning and methodology), `research/地基/00-目标与动机-v1.md` (goals and motivation). Material comes from the original source code of 84 real open-source projects and their J++ rewrites (selection method and per-project rewrite records: `research/2026-09-26-rewrite-study.zh-CN.md` (full text in Chinese, abridged English at `research/2026-09-26-rewrite-study.md`; a live interactive version is at https://jpp.towow.net/rewrite-study/); the per-project original and rewritten code itself is an internal record, not published with this release) and an internal ecosystem snapshot (4,041 public repository snapshots, not published with this release). Scripts: `research/scripts/busywork_classify.py` (classifies the 84 projects), `research/scripts/busywork_ecosystem_sample.py` (ecosystem sampling, written by a sub-agent). Data: `research/data/2026-09-27-repetition/`.

Terminology note: where this document says "the eight things the runtime takes over," it refers to what the glossary in `00-定位与方法论-v1.md` used to call a "seam" (缝) — renamed as of 2026-09-27 to drop that metaphor (per addendum 16 to the compiled-intent notes).

---

## 1. Hypothesis and predictions

Nature's hypothesis: the "repeated busywork" written around JEV calls accounts for most of the development effort in these projects. Split into three falsifiable predictions, stated here before measurement; the numbers are reported below in "3. The numbers" — the predictions are not adjusted after the fact to fit the results.

**Order of writing**: the three predictions in this section were fixed before any statistic was run (operationalized directly from the examples in the task brief). The classification script itself was tuned while looking at aggregate numbers as it was being written — after the first version of the regexes produced "4/12 categories clear the core-scope bar, 6/12 clear the file-scope bar" at ≥30%, a `scope=host` range and a "union of the three ranges" scoring rule were added, pulling the hit count up to 8/12. A later self-check then found two more problems: first, naive line-by-line keyword matching would score a whole 30-plus-line block of retry logic as only two or three "hit" lines (this is why the script was changed from "line-by-line keywords" to "block-level attribution + proximity to a judgment-call context," v2, described below); second, the regex for the "calibration and labeling" category matched the word `annotation`, which also matches the extremely common Python idiom `from __future__ import annotations` (nothing to do with labeled data) — this false positive inflated the hit count for that category in nearly every Python project. Fixing it dropped that category's hit rate from 34/81 (42.0%) to 8/81 (9.9%), and dropped P2's category count from 8/12 to 7/12. All three changes happened after the predictions were written and before the final numbers came out, and all three moved in the direction of fixing a visible flaw in the method — but the discipline of "write the prediction first, then leave the method alone" was not fully kept at the level of method detail. The last fix flipped P2's conclusion from "holds" to "does not hold," and that has to be stated plainly — the earlier "holds" cannot be quietly carried forward just because it was written down once.

**P1 (line share)**: across the judgment core of the 84 projects (the exact line ranges registered under `[[core]]` in `measure.toml`), the median share of lines that are busywork is ≥ 50%.

**P2 (category breadth)**: of the 12 busywork categories, at least 8 each appear in ≥ 30% of projects (presence/absence only, not line count).

**P3 (ecosystem extrapolation)**: sampling 100 repositories that genuinely call JEV out of the 4,041 public repository snapshots, and looking only at presence/absence, the ranking of the 12 categories' occurrence rates roughly matches the 84 hand-picked projects (within a rank or two), showing the 84 hand-picked projects were not cherry-picked to produce this result.

### A note on scope (stated up front, so the predictions themselves aren't distorted)

The rewrite-study report defines "judgment core" as: "code that defines a question, calls the judgment interface, routes by threshold, or combines the results of several questions — that is the 'core.' Command-line parsing, network requests, logging, retries — code unrelated to judgment logic — do not count as core." Under this definition, busywork categories 4, 6, and 7 (retry/failure, cost counting, concurrency limits) are excluded from "core" by definition, and can only land in "host-shared" code or in the part of a core file that falls outside the registered core line range. Scoring P1 against the narrowest core-only scope would systematically undercount these three categories, making P1 look easier to falsify than it really is.

So this document reports counts under three scopes side by side, not just one number:

- **scope=core**: the exact line ranges registered under `[[core]]` in `measure.toml`. This is the "judgment core" definition used by the rewrite-study report — the narrowest.
- **scope=file**: the whole file that a `[[core]]` entry points to (not truncated to the registered lines). This recovers busywork that the core definition excludes but that is actually written in the same file.
- **scope=host**: the whole file(s) registered under `[[host_shared]]` (registered for 51 of 84 projects). This is the glue code the rewrite process explicitly defines as "unrelated to judgment logic, but both sides have to write it" — command-line parsing, network requests, logging, and retries mostly belong here by definition.

P1 is scored only against scope=core (the operationalized definition given in the task brief, closest to "judgment core" itself). P2 is scored against the union of all three scopes (a category counts as "present" in a project if it appears in the core, the core file, or the host file), because the narrow scope would artificially exclude categories that, by definition, belong in host code.

## 2. Definitions and detection rules for the twelve busywork categories

"Busywork" means: code unrelated to what this project is specifically judging (the business logic itself — e.g., the literal wording of "should this ticket be escalated to a human," or domain-specific rules) — code that has to be written once just to plug into JEV, and then written again for the next project. Detection is mostly keyword/regex-based (the regexes used by the script are in `research/scripts/busywork_classify.py`), allowing for language-specific variation. Each line is assigned to exactly one category for computing the line-share ratio (the first category to match, in the priority order below), but "presence/absence" is multi-label — if a line looks like it belongs to two categories, both categories are marked "present."

| # | Category | Detection rule |
|---|---|---|
| 1 | Call loops and batching | A loop that calls the judgment interface item by item; `Promise.all`/`asyncio.gather`/thread pools/process pools; manual batch accumulation (packing several items into a list/queue before sending them together) |
| 2 | Thresholds | Comparing a probability/confidence/score against a hardcoded constant (`confidence > 0.7`); such constants defined as literals in the calling code |
| 3 | Handling uncertainty | An `else` branch taken when the probability lands in the middle ground; tagging a result "uncertain/unsure/ambiguous/needs_review"; discarding an uncertain result outright |
| 4 | Retry and failure handling | try/except (try/catch) wrapping the judgment call; retries, backoff, timeouts; a default value used after a call fails |
| 5 | Caching | A dict/Map/lru_cache/file/database caching judgment results, keyed or hashed by the question and material content |
| 6 | Cost and call counting | A call counter, cost/token accounting, a budget-cap check, logging kept specifically for this purpose |
| 7 | Concurrency limits | Semaphores, rate limiters, a concurrency-cap parameter |
| 8 | Question assembly | String concatenation/templates/f-strings assembling a question and its candidates into text to send to JEV |
| 9 | Material trimming | Truncation, chunking, taking only the first *k* segments, concatenating a context window — all to fit inside a request |
| 10 | Result feedback | Writing a previous judgment's result back in as material or part of the question for the next judgment (multi-round loops, progressive judgment) |
| 11 | Combining multiple judgments | Voting, weighting, or cascading (cheap first, then expensive) over the results of several questions |
| 12 | Calibration and labeling | Hand-labeled dataset files, threshold-tuning scripts, ROC/AUC/precision-recall style calibration/evaluation code |

Residual category, "the business itself": domain code unrelated to judgment logic (the literal wording of a verdict, arithmetic unrelated to JEV, logic specific to this project but unrelated to "how to plug into JEV"). This category is not counted in the busywork-share numerator; it only serves as a denominator reference and is not reported on its own.

### Classification method: two versions, v2 is the primary scope

The script has two implementations, both kept in `research/scripts/busywork_classify.py`; the report gives numbers from both versions and does not hide the old one:

- **v1 (naive line-by-line keywords)**: each line of code is run against 12 sets of regexes independently; a match assigns that line to that category. The problem only became visible during calibration: a complete 31-line retry block covering a deadline, error classification, and backoff calculation (`jev_commit/jev.py:118-148`, used as Example 7 below) — v1 only matched the two or three lines containing `try:`/`except:`, and everything else in the block (the business logic, the error wrapping) fell into "the business itself," scoring an entire block of busywork as a handful of lines. Conversely, tokens like `for x in y:`, `try:`, `.catch(` are extremely common in any code, and many matches have nothing to do with a JEV call at all (e.g., in `sathariels/jevtriage`, project B1-05, `except (TypeError, ValueError)` wraps argument parsing, not a judgment call). v1's computed "busywork line share" is therefore systematically too low, and all three scopes (core/file/host) came out to nearly the same median (6.1%) — that coincidence is itself a signal that the method is flawed, not a coincidence.
- **v2 (block-level attribution + proximity to a judgment-call context; used throughout the body of this document)**: for three categories whose real unit is a control-flow block, not a single line — "call loops and batching," "retry and failure," and "concurrency limits" — the script first finds the line that opens the block (`for`/`try`/`with Semaphore`, etc.), and only assigns the whole block (found by indentation in Python, by matching braces in brace-delimited languages) to that category if a judgment-call context marker (`system_one`, `Choice(`, `noul(`, `confidence`, `criteria=`, etc.) appears near the opening line (within ±20 lines). Five categories — call loops, retry, cost counting, concurrency limits, and question assembly — all require a nearby judgment-call context to reduce false positives from "some unrelated try/except block somewhere in the code." Verified against the same block of code from B3-01: v1 recorded 4 busywork lines out of 225 core lines (1.8%); v2 recorded 66 lines (29.3%) — the definition of busywork was not widened, the whole block that should have counted was simply recovered.

Limitations: v2's brace-block-end detection uses naive bracket counting and doesn't handle braces inside strings/comments, so there is some noise; "judgment-call context" is itself a bag of keyword proxies, not semantic understanding — this remains a cheap proxy, not ground truth. To independently check the classification script's accuracy, a separate sub-agent that had not seen the script or its output was tasked with blind manual annotation of 10 projects (covering six languages: Python/JS/TS/Go/Rust/Dart); progress and results are in section 3.3.

## 3. The numbers

### 3.1 The 84 projects (P1, P2)

81 of the 84 projects have a `measure.toml` (3 do not: `B2-05-hermes` has a missing record, `B3-07-botcraft` was ruled "not comparable" because of a genuine defect in the original project itself, and `Q-05-jev-torneo-animales` has a missing record). Of those 81, a further 4 (`B1-11`, `B4-02`, `B6-11`, `B7-01`) have no `[[core]]` entry in `measure.toml` because the original repository has no license, so `original/` does not carry a copy of the code (core line counts in the rewrite record are hand-entered constants — see each project's `result.json`); the script cannot read local source for these and cannot participate in classification. **The number of projects that actually could be classified is 77**, 92% of the 84-project sample; line-share summaries omit records without countable lines. Presence/absence rates, however, retain all 81 CSV records (including those four zero-line records), following the published script's summary rule; they are not rates over only the 77 classifiable projects.

**P1 (line share ≥ 50%): both method versions fail; v2 (the more accurate one) narrows the gap but it's still far off.**

| Version | scope=core median | scope=file median | scope=host median |
|---|---|---|---|
| v1 (naive line-by-line, known to undercount) | 6.1% | 6.1% | 6.1% |
| v2 (block-level + context; this document's scope) | **11.1%** | 12.6% | 8.2% |

The fact that v1 gives nearly the same number across all three scopes is not a coincidence — it is a signal the method is flawed (see "Classification method" above). After the v2 correction, the median rises noticeably (core goes from 6.1% to 11.1%, the mean rises to 15.9%, the interquartile range is [3.8%, 22.0%] — a wide range, meaning busywork share varies a great deal from project to project; the median does not represent everything). The median is still far below 50%, but the distribution has a long tail: in `research/data/2026-09-27-repetition/84项目-scope核心-v2块级.csv`, the top projects are genuinely over half (B6-06 `diluteoxygen/JevMood` at 64.4%, Q-04 `siroccomask/snake-jev` at 58.6%, B1-10 `prantikmedhi/anchorlint` at 56.6%, B5-10 `lalitsonawane/jev-snake` at 54.5%, B5-06b `eachann1024/pi-jev-route` at 50.6%). The B3-01 (`jev-commit`) case used for Example 7 in this document sits at 29.3% — not the heaviest project in this batch, just the one where the busywork is written most completely and is easiest to explain clearly. Most projects (35 of 77 below 10%, 55 below 20%) really do have a judgment core dominated by verdict-writing business text, but 5 projects are over half — the median, "what most projects look like," and "all projects are far below half" are three separate claims; the first two hold, the third does not, and none of the three can stand in for the others.

**This result is not the same thing as what Nature wants to argue — "accounts for most of the development effort" — and the relationship between the two is worth spelling out.** Line share answers "how many lines of the judgment core's text are busywork"; development effort is a different dimension, including thinking through what a threshold should be set to, whether one constant will end up shadowing another, and how much time it takes to re-hit the same bug in every project. The public rewrite-study report cites Jones's caveat: "coding accounts for only about 30% of a large project's total effort." The debugging and pitfall time behind one line of "threshold comparison" code is not something this section's line counts can measure. What this data can honestly support is: **busywork does not make up most of the judgment core's code lines, but nearly every project has to write it over again** (see P2 below) — "repetition" itself holds up; "accounts for most of the development effort" currently has no credible quantitative evidence behind it in this document, and this document does not vouch for that claim.

**P2 (≥ 8 categories each present in ≥ 30% of projects): does not hold, 7/12.** The first-version number (8/12) counted "calibration and labeling" at 42.0%; that number was later found in a self-check to be a false positive (`from __future__ import annotations`, an extremely common Python idiom, matched the word `annotation`, unrelated to labeled data). After fixing the false positive, the real occurrence rate for "calibration and labeling" is 9.9%, dropping the category count from 8 to 7 — short of the line of 8 set in the task brief.

v2, union scope (a category counts as present in a project if it appears in any of core/file/host; the "calibration and labeling" false positive already corrected):

| Category | Present in (denominator 81 projects) | Share |
|---|---|---|
| 4 Retry and failure | 64 | **79.0%** |
| 1 Call loops and batching | 61 | **75.3%** |
| 2 Thresholds | 53 | **65.4%** |
| 9 Material trimming | 44 | **54.3%** |
| 8 Question assembly | 41 | **50.6%** |
| 5 Caching | 27 | **33.3%** |
| 3 Handling uncertainty | 30 | **37.0%** |
| 10 Result feedback | 18 | 22.2% |
| 12 Calibration and labeling | 8 | 9.9% |
| 6 Cost and call counting | 10 | 12.3% |
| 7 Concurrency limits | 9 | 11.1% |
| 11 Combining multiple judgments | 5 | 6.2% |

The three most common categories: **retry and failure (79.0%), call loops and batching (75.3%), thresholds (65.4%)**. Nearly every project, when plugging into JEV, has to write, on its own, "what to do when a call fails," "how to send a batch of questions," and "how far apart a probability and a constant have to be to cross the line." 7 categories clear the 30% bar (4, 1, 2, 9, 8, 5, 3); 5 do not (10, 12, 6, 7, 11). The two rarest categories are "combining multiple judgments" (6.2%) and "concurrency limits" (11.1%) — most of these projects only do a single round of judgment (classification, moderation, routing), and multi-question voting/cascading and explicit concurrency control are genuinely rare in real projects.

Per-project data: v1 in `84项目-scope核心.csv`, `84项目-scope整文件.csv`, `84项目-scope宿主.csv`; v2 in the same directory with the `-v2块级` suffix. Reproduction command: `python3 research/scripts/busywork_classify.py` (running with `--ids` to test a subset overwrites the full-run CSV — don't pass it when trying to reproduce the full-sample numbers).

### 3.2 Sampling 100 repositories from the ecosystem (P3)

From the 4,041 repository snapshots in an internal ecosystem snapshot (not published with this release), ripgrep was first used to find repositories with genuine evidence of a JEV/TypeSafe call in the source, excluding official SDK organizations, third-party client libraries with "SDK" in the name, tutorial/template repositories below a file-count threshold, and repositories with the same name as any of the 84 hand-picked projects — leaving a candidate pool of 1,804; a fixed seed (`random.seed(42)`) drew 100 of them. Only the files within each repository that actually contain a call site (not the whole repository) were run against the 12-category regex, recording presence/absence only, using the same regex set as v1 (naive line-by-line keywords, no block-level/context filtering). Script: `research/scripts/busywork_ecosystem_sample.py`. Data: `research/data/2026-09-27-repetition/生态抽样100.csv`.

Because this uses v1's regex and reads whole files, the comparable number from the 84 projects is v1's scope=file (same method, same scope) — not v2 or the three-scope union. The table below puts the two side by side. The hand-picked sample uses all 81 records in `research/data/2026-09-27-repetition/84项目-scope整文件.csv`, including the four zero-line records described in 3.1; the ecosystem sample uses all 100 records. Each rate is the sum of its `present_*` column divided by the number of records. Differences are ecosystem minus hand-picked rates in percentage points, calculated before rounding to one decimal place. This preserves the published denominator rule; a zero-line record does not establish that the category is absent from the original source.

**CSV recheck (2026-09-30):** all 12 rows were recalculated from the published CSVs, correcting four hand-picked rates and their differences. No source classification was rerun: the underlying corpus is not published here. Reproduce the table check with `python3 -m pytest -q tests/test_research_repetition.py`; it checks both language versions without that corpus.

| Category | Hand-picked, v1 scope=file (n=81) | Ecosystem sample (n=100) | Difference (percentage points) |
|---|---|---|---|
| 4 Retry and failure | 79.0% | **85.0%** | +6.0 |
| 1 Call loops and batching | 71.6% | **72.0%** | +0.4 |
| 2 Thresholds | 65.4% | 50.0% | −15.4 |
| 8 Question assembly | 48.1% | 58.0% | +9.9 |
| 9 Material trimming | 45.7% | **60.0%** | +14.3 |
| 3 Handling uncertainty | 35.8% | 21.0% | −14.8 |
| 5 Caching | 25.9% | 29.0% | +3.1 |
| 6 Cost and call counting | 16.0% | 28.0% | +12.0 |
| 7 Concurrency limits | 11.1% | 25.0% | +13.9 |
| 12 Calibration and labeling | 8.6% (false positive corrected, see 3.3) | 37.0% (uncorrected, see below) | not comparable |
| 10 Result feedback | 21.0% | 6.0% | −15.0 |
| 11 Combining multiple judgments | 4.9% | 14.0% | +9.1 |

**The "calibration and labeling" row cannot be compared directly.** The ecosystem-sampling script copied the classification script's first-version regex, and likewise counted `from __future__ import annotations` (a Python idiom unrelated to labeled data) as a hit for "calibration and labeling" — this false positive was already found and fixed on the 84-project side (42.0% became 8.6%, see 3.3), but the ecosystem-sampling script was never re-run with the fix, so the 37.0% figure is very likely inflated by the same false positive. This document does not trust that number, and does not use it to judge whether "calibration and labeling" is more or less common in the ecosystem sample than in the 84 projects. This is a known gap in this document's material, not a problem being hidden.

**P3 (rankings roughly consistent): partly holds, and the "calibration and labeling" cell has to be set aside.** The categories ranked first and second are exactly the same (retry and failure, call loops and batching). From third place down, the rankings diverge noticeably: "material trimming" ranks third in the ecosystem sample (60.0%), while "thresholds," third in the hand-picked sample (65.4%), ranks fifth in the ecosystem sample (50.0%). Excluding calibration and labeling, the largest absolute differences are thresholds (−15.4 percentage points), result feedback (−15.0), handling uncertainty (−14.8), material trimming (+14.3), and concurrency limits (+13.9); cost and call counting is also higher by 12.0 points. Result feedback ranks eighth of 12 in the hand-picked sample (17/81, 21.0%) but last in the ecosystem sample (6/100, 6.0%) — it is not among the two rarest in both. Combining multiple judgments ranks last in the hand-picked sample and second-last in the ecosystem sample; its ecosystem rate is 14.0% versus 4/81 (4.9%), a +9.1-point difference and about 2.8 times the hand-picked rate (ratio calculated before rounding).

One reason for this difference, as noted in the sampling script's own stated limitations: relevance is judged by keyword evidence, and the "files containing a call site" it catches include some code that looks more like an SDK adapter layer, loosely related to actual business judgment logic (e.g., wrapping JEV as one of many interchangeable providers in a generic framework adapter) — such files naturally have a heavier "infrastructure smell," inflating categories tied to "how to plug into an external service" — retry, caching, cost counting, concurrency limits. In addition, the ecosystem-sampling statistics' `matched_files` column shows that some repositories (e.g. `Alberto-Codes/judgevet`) counted test files under `tests/` as "files containing a call site" — test code naturally has a higher density of `try/except`/`assert`, which further inflates "retry and failure" in the ecosystem sample; this is another known but unquantified source of bias. The 84 projects are a sample chosen after deep manual reading and extracting the "judgment core," which naturally filters out some adapter layers and test code; the ecosystem sample is a coarse keyword pass — the two sides are not looking at "relevant code" at the same level. **The part of the conclusion that holds**: the two most common busywork categories (retry, call loops) are not an artifact of sample selection — they are equally the most common in an unbiased sample. **The part that does not hold**: specific rankings and specific percentages cannot be applied across samples, especially since the middle categories' rank and magnitude swing noticeably with the sampling method — and the ecosystem number in the "calibration and labeling" row still contains the known false positive.

### 3.3 Calibration: is the script accurate?

The original plan was to task an independent sub-agent — one that had not seen the script or its output — with blind manual annotation of 10 projects, then compare against the script. That sub-agent did finish the run (confirmed by the coordinator), but its final results never arrived — a follow-up request still did not produce the complete content within the time limit. Per the rule (a sub-agent that doesn't deliver within 20 minutes is treated as failed and the work is done directly), this section instead became a post-hoc check I ran myself, on the same 10 projects whose source I had already read line by line for the "4. Examples" section: this is not a fully independent blind annotation (I already know the script's classification logic), but every entry was checked against the real source line by line, not paraphrased from the script's output.

Method: the script's per-line detail dump (`--dump-detail-for`) for v2 was checked, item by item, against the judgments I had recorded while reading the source, for the same project and the same scope (scope=core) — not simply trusting "the script says it's there." Four issues were found; three have already been fixed in code (the improved rules are used in every number above); one is a hard ceiling on what the method can do:

- **For-loops that unpack multiple variables, like `for index, _item in enumerate(items):`, were missed by the script's original regex, which only recognized the single-variable form `for x in y:`, so "call loops and batching" was under-detected** (`B1-07 pulso-nps`, `B2-04 pi-jev-compaction`). Fixed (`for\s+[\w\s,]+\s+in`); re-running `--dump-detail-for B1-07:core` confirms both `for` loops inside the `classify_batch` function (`app/classifier.py:66-112`, which falls exactly within this project's registered `[[core]]` range of `66-112`) are now correctly tagged "call loops and batching" — not, as an earlier draft of this document wrongly stated, "still not triggered individually"; that sentence was wrong and has been deleted and rewritten.
- **The structural pattern `abs(m.p_a - 0.5) < gate_threshold` ("too close to 0.5 counts as uncertain") was completely missed, because the script's original "handling uncertainty" regex only recognized literal words like `unsure`/`uncertain`/`needs_review`** (`B3-10 jev-bracket`, "approach one" in Example 2 below). Fixed (added an `abs(...-0.5)` pattern); after re-running, B3-10's core scope is now correctly tagged "handling uncertainty" — this fix is also the main reason "handling uncertainty" rose from 30.9% to 37.0% in section 3.1.
- **`from __future__ import annotations` (an extremely common Python idiom, unrelated to labeled data) matched the `annotation` keyword in the "calibration and labeling" category, causing widespread false positives** (first found in `B4-01 RoboJEV`; a project-by-project check confirmed nearly every Python project using this line was affected). Fixed (excluding this specific line); after re-running, the "calibration and labeling" occurrence rate dropped from 42.0% to 9.9% — the single largest-impact correction from this check, and it directly changed P2's conclusion (see 3.1).
- **Project B4-13 (`yodablocks/commitjev`)'s SQLite caching code (Example 4 below) is outside this project's registered `measure.toml` range, so the script simply cannot see it — this is not a "missed detection," it is a scope-boundary problem, not a regex problem, and was not fixed, nor should it be**: `measure.toml` registers only `rules.py` as core, and part of `rules.py` plus `gitio.py` as host; the `client.py` referenced in Example 4 (the `Cache` class, the `Usage` cost-tracking dataclass) never appears in any `[[core]]` or `[[host_shared]]` entry at all. This shows that the measurement scope of "3.1" in this document itself has a boundary — some real, genuine busywork code, because it was never registered in a core/host range by whoever did the rewrite, never enters any number in this document, so the occurrence rates here may still be systematically low overall — this is not purely a question of how accurate the script is.
- **One issue found but not fixed, and not fixable: `B1-09 antispam`'s `parseAssessment` uses `entries.reduce((strongest, current) => current[1] > strongest[1] ? current : strongest)` to pick the highest-probability entry as `strongestSignal` — this is genuinely "combining multiple judgments" (taking a max), but this style, expressed through `reduce` plus a comparison operator with no dedicated vocabulary at all, cannot in principle be recognized by a keyword regex.** This is a ceiling of the method itself: the 6.2% figure for "11 combining multiple judgments" should be read as a lower bound, not an exact value.

One further apparent inconsistency was checked and found to be the script behaving correctly: `Q-03 jev-corrective-rag`'s core scope was not tagged "retry and failure," but reading the code shows this project's actual `if self.offline: ... else: response = self.client.system_one(...)` branch (`gates.py:91-104`) falls within this project's registered `[[host_shared]]` range (`64-78,83-125`), not `[[core]]` (`79-80,129-238`) — the script does correctly tag this category under scope=host; not tagging it under scope=core is correct, not a miss.

**Conclusion**: the script's judgment is largely trustworthy for categories with dedicated vocabulary that fall inside a registered range (thresholds, caching, material trimming, handling uncertainty); three systematic issues — arithmetic combination expressed through generic language constructs, collisions with common language idioms, and the measurement scope itself not covering all real code — belong to three different kinds of problem ("method ceiling," "fixed," "scope boundary") and should not be lumped together. Readers should treat this document's "occurrence rate" as the best estimate the current measurement scope and method can give — not an exact statistic, and quite possibly a lower bound.

## 4. Examples: the same busywork, written over and over by different projects, each time differently, each time incompletely

The examples in this section all come from real, unmodified open-source code (the original file and line numbers are noted with each example; all corresponding repositories are public open-source projects — the internal rewrite corpus's directory structure is not published with this release). The selection criterion is "the same kind of busywork shows up in more than one project, written differently each time, each time leaving a gap" — this is exactly the kind of example Nature asked for, one that gives a "feel" for it: not the abstract claim "you have to write a call loop," but showing the reader three or four pieces of code that look completely different but are doing the exact same thing.

### Example 1: threshold constants scattered at the call site — every extra constant is another place two thresholds can silently shadow each other

Repository `web_attack_detection_jev` (project B1-03), `config.py`:

```python
ATTACK_THRESHOLD = 0.65
REVIEW_THRESHOLD = 0.40
BLOCK_THRESHOLD = 0.70
```

Repository `web_attack_detection_jev` (project B1-03), `detector.py:44-51`:

```python
def decide_action(is_attack_prob, attack_type, should_block_prob):
    if is_attack_prob >= ATTACK_THRESHOLD and attack_type != "benign":
        if should_block_prob >= BLOCK_THRESHOLD:
            return "block"
        return "block" if should_block_prob >= REVIEW_THRESHOLD else "review"
    if is_attack_prob >= ATTACK_THRESHOLD and attack_type == "benign":
        return "review"
```

What it's doing: once the judgment result comes back, the author wrote three threshold constants and a set of nested if/else logic deciding whether to allow, review, or block. What's missing: the "defects found" section of the public rewrite-study report already points out that the range covered by `REVIEW_THRESHOLD` (0.40) completely swallows the range `BLOCK_THRESHOLD` (0.70) was meant to cover — by the time the line `should_block_prob >= REVIEW_THRESHOLD` executes, `BLOCK_THRESHOLD` has already been checked, so this stricter constant never actually takes effect on its own. Three constants, one nested if — a reader might not spot this overlap on a first read, and the author didn't either.

### Example 2: how to handle "not sure" — three projects, three different structures, each with its own gap

The same problem — what to do when the judge's probability isn't confident enough — solved with a bespoke structure invented independently by each of three projects, with no shared lessons between them.

**Approach one, too close to 0.5 counts as uncertain** (`jev_bracket/bracket.py:77-93`, repository `meetr1912/jev-bracket`):

```python
def choose_winners(matchups, league, gate_threshold=0.0):
    gated = 0
    for m in matchups:
        a, b = league.by_tid(m.a_tid), league.by_tid(m.b_tid)
        better = a if a.seed < b.seed else b
        if abs(m.p_a - 0.5) < gate_threshold:
            m.gated = True
            m.winner_tid = better.tid
            m.method = "gated-chalk"
            gated += 1
        else:
            m.winner_tid = a.tid if m.p_a >= 0.5 else b.tid
```

What it's doing: if the probability is too close to 0.5 (the difference is below a threshold), the match counts as "uncertain," and falls back to "the lower-seeded side advances." What's missing: subtracting two decimal numbers under double-precision floating point is not an exact operation — the public rewrite-study report's "defects found" section records a real consequence in this repository: subtracting 0.45 from 0.5 and taking the absolute value produces a result slightly smaller than the intended boundary, so a probability that should have landed exactly on the boundary is unintentionally included in or excluded from the "uncertain" range by floating-point error — not the closed interval the author designed, but a side effect of numerical precision.

**Approach two, two independent gates (probability + confidence) — getting this right requires thinking through when each one fails** (`triage.js:79-96`, repository `emreozyoruk/hush`, excerpted):

```javascript
const lab = answers.label;
if (lab?.choice) {
  const p = lab.probabilities?.[lab.choice] ?? 0;
  const conf = typeof lab.confidence === "number" ? lab.confidence : 0;
  // Two gates, because they fail differently: a flat distribution means the
  // options overlap, low confidence means the model does not trust its own read.
  const sure = p >= thresholds.label && conf >= thresholds.label_confidence;
  if (lab.choice === "none") {
    out.push({ kind: "label", action: ABSTAIN, confidence: p, why: `best fit was "none" (${pct(p)})` });
  } else if (!labels[lab.choice]) {
    out.push({ kind: "label", action: ABSTAIN, confidence: p, why: `model returned an unknown option "${lab.choice}"` });
  } else if (sure) {
    out.push({ kind: "label", action: "label", value: lab.choice, confidence: p,
      why: `${lab.choice} ${pct(p)} ≥ ${pct(thresholds.label)}, confidence ${pct(conf)} ≥ ${pct(thresholds.label_confidence)}` });
  } else {
    out.push({ kind: "label", action: ABSTAIN, confidence: p,
      why: `${lab.choice} ${pct(p)} / confidence ${pct(conf)} — below ${pct(thresholds.label)} / ${pct(thresholds.label_confidence)}` });
  }
```

What it's doing: the source's own comment says it plainly — "two gates, because they fail differently: a flat distribution means the options overlap, low confidence means the model does not trust its own read." `p` (probability) and `conf` (confidence) are two different quantities, each with its own threshold; both have to clear the line to count as "confident," and everything else abstains (`ABSTAIN`) — with a separate branch for the case where the model picks a candidate that doesn't exist at all. This is the clearest distinction between `confidence` and `probability` among all nine examples in this document — no confusion of the kind in Examples 3 and 8. What's missing: how the two gates should be combined (`&&` or `||`), and how the three special branches (`none`, an off-menu answer, failing both gates) should be prioritized — these design decisions had to be worked out entirely from scratch by this one project's author; a different project, with a different candidate structure, a different call on whether off-menu answers need special handling, a different boolean combination of the two gates, would have to redesign all of it, with no existing structure to reuse.

**Approach three, a confidence gate that can override the judgment result itself** (`jevtriage/gate.py:23-31`, repository `sathariels/jevtriage`; code quoted verbatim — nothing in the code itself marks the intent of "override whatever the verdict says," which the reader has to work out by following the order of the `if` statements):

```python
def gate_exit(verdict: str, confidence: float, *, min_confidence: float) -> int:
    if verdict == "risky":
        return EXIT_RISKY_OR_ERROR
    if confidence < min_confidence:
        return EXIT_NEEDS_REVIEW
    if verdict == "ready":
        return EXIT_READY
    if verdict == "needs_review":
        return EXIT_NEEDS_REVIEW
    return EXIT_RISKY_OR_ERROR
```

What it's doing: this is a three-way judgment (ready/needs_review/risky), but the author added an independent confidence check that can force a result that is already "ready" into "escalate to a human" — in the source's own docstring, this is "fail closed" (better to over-escalate than to miss a case). What's missing: the confidence check and the three-way judgment are two independently written pieces of logic; which one takes precedence, and when they should override each other, was worked out by hand, once, inside this one project — a different project would have to design this priority ordering all over again.

The three approaches have entirely different structures (a symmetric interval, dual gates, an override check) — none can be dropped directly into another project — but they are solving the same problem: what the program should do when the judge isn't confident enough.

### Example 3: a confidence gate that "looks like it's gatekeeping," but always lets everything through because of how the SDK's return value defaults

`src/gates.py:112-114`:

```python
else:
    answers[key] = bool(ans.noul)
confidence[key] = float(getattr(ans, "confidence", 1.0))
```

`src/pipeline.py:181-183`:

```python
passes = grade.answers.get("answers_it") and grade.confidence.get(
    "answers_it", 0.0
) >= RELEVANCE_CONF_FLOOR
```

What it's doing: the official SDK's type for a yes/no question, `NoulAnswer.noul`, is a float between 0 and 1; here it's coerced to a boolean with `bool(...)`. The confidence field is read with `getattr(ans, "confidence", 1.0)` — if it's missing, confidence is assumed to be perfect. What's missing: `bool(p)` is only false when `p` is exactly 0.0 — on a real judge, a probability is almost never exactly 0, so this "yes/no question" is nearly always true in practice; and `NoulAnswer` doesn't even have a `confidence` field at all, so `getattr`'s default of 1.0 always applies — the "confidence gate" in `pipeline.py` is a dead letter, and has never actually blocked a single judgment. This is a real defect found in the original project during this rewrite, not something introduced by the J++ rewrite.

### Example 4: every project writes its own cache-plus-cost-tracking layer for the judgment interface

`client.py:76-99`:

```python
@dataclass
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    requests: int = 0
    cache_hits: int = 0

    @property
    def cost_usd(self) -> float:
        return self.input_tokens * USD_PER_INPUT_TOKEN


class Cache:
    def __init__(self, path):
        ...
        with self._conn() as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS answers ("
                "key TEXT PRIMARY KEY, model TEXT, payload TEXT, "
                "input_tokens INTEGER, created_at REAL)"
            )

    @staticmethod
    def key(model, state, questions):
        blob = json.dumps({"model": model, "state": state, "questions": questions},
                           sort_keys=True, ensure_ascii=False)
```

The top of this file has its own comment: "Jev transport: the official SDK, plus a cache and a thread pool." What it's doing: on top of the official SDK, the author built a SQLite table to store judgment results, wrote their own JSON-based cache-key construction, and maintained their own cost-tracking dataclass. What's missing: none of this caching and counting logic has anything to do with what this project is actually judging ("should this commit be blocked") — it is purely development effort that comes from the act of "plugging into JEV" itself. Project B1-10's (`prantikmedhi/anchorlint`) rewrite record also notes that the original project's content-based memoization cache has no equivalent — the same problem, two projects, two different hand-rolled implementations, neither reusable by the other.

### Example 5: when the candidate set changes, two rounds of calls have to be hand-written, stitched together with a dictionary keyed by index

`app/classifier.py:66-100` (excerpted):

```python
def classify_batch(items, taxonomy):
    ...
    first_questions = {}
    for index, _item in enumerate(items):
        first_questions[f"area_{index}"] = Choice(instructions=..., criteria=area_criteria)
        first_questions[f"tone_{index}"] = Choice(instructions=..., criteria=TONE_CRITERIA)
    with TypeSafeClient(model="jev-latest") as client:
        first = client.system_one(state=state, questions=first_questions)
        second_questions = {}
        for index, _item in enumerate(items):
            area_answer = _choice_payload(first.choices[f"area_{index}"])
            area = next((e for e in taxonomy if e["name"] == area_answer[0]), taxonomy[-1])
            criteria = {c["name"]: c.get("description") for c in area["categories"]}
            second_questions[f"category_{index}"] = Choice(instructions=..., criteria=criteria)
        second = client.system_one(state=state, questions=second_questions)
```

What it's doing: first ask which broad category a batch of tickets belongs to, then use the first round's answer (`area`) to decide which candidate sub-categories to offer in the second round, manually matching the two rounds with string-built keys (`f"area_{index}"`, `f"category_{index}"`). What's missing: this is a textbook case of two busywork categories stacked together — "result feedback" plus "separately handling choice questions with different candidate sets" — the second round's candidate set is entirely determined by the first round's result, and the author has to work out, by hand, how to align the two rounds' answers with a dictionary without ever getting an index wrong. The public rewrite-study report's "examples that saved the least" section lists several projects where "J++ actually ends up longer" (`jesusvillamarin/pulso-nps` is one of them, 64 lines versus 80) — for the same reason: this kind of hand-rolled two-round stitching is handled today in J++ by treating each candidate as its own state, and the pattern of "use the previous round's result to determine the next round's candidate set" has not yet been compressed into a single statement (gap B155/B156).

### Example 6: material trimming — every project picks its own number, whatever feels like "enough"

`triage.js:9-21`:

```javascript
export function buildState({ title, body, author, isFirstTimeContributor, openTitles }) {
  const lines = [
    `New issue title: ${title}`,
    `New issue body:\n${(body || "(empty)").slice(0, 6000)}`,
    `Author: ${author}${isFirstTimeContributor ? " (first-time contributor to this repo)" : ""}`,
  ];
  if (openTitles?.length) {
    lines.push("", "Titles of other open issues in this repository:",
      ...openTitles.slice(0, 40).map((t, i) => `${i + 1}. ${t}`));
  }
  return lines.join("\n");
}
```

What it's doing: the issue body is truncated to 6,000 characters, the list of candidate titles is truncated to 40 entries, out of concern that too much material won't fit in a request or will hurt judgment quality. What's missing: the numbers 6,000 and 40 have no rationale written anywhere near them — they're purely the author's guess. In a different project, the public rewrite-study report's `gap_titles` for projects like `MarissaFamularo/citation-verifier` (citation checking) and `amitvijapur/cortex` each contain their own, independently invented material-trimming/chunking logic (diff parsing, chunking, token estimation done on the host side) — every project has to re-answer "what to do when the material is too long" on its own, with a different number each time, and no shared lesson to draw on.

### Example 7: what to do when the judgment interface goes down — three projects, three hand-written approaches

**Approach one, a full retry infrastructure** (`jev_commit/jev.py:118-148`, repository `valentynkit/jev-commit`):

```python
for attempt in range(RETRIES + 1):
    left = stop - time.monotonic()
    if left <= 0:
        raise JevError("deadline reached: " + last)
    try:
        with urllib.request.urlopen(request, timeout=left) as response:
            body = json.loads(_read_by(response, stop).decode("utf-8", "replace"))
        break
    except urllib.error.HTTPError as err:
        text = err.read().decode("utf-8", "replace")[:300]
        if err.code == 413 or (err.code in (400, 422) and any(h in text.lower() for h in TOO_BIG_HINTS)):
            raise TooBig(last) from err
        if err.code != 429 and err.code < 500:
            raise JevError(last) from err
        wait = _retry_after(err.headers, delay)
    except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException) as err:
        last, wait = str(err), delay
    if attempt == RETRIES:
        raise JevError(last)
```

What it's doing: behind a single judgment call, the author hand-wrote a retry cap, an overall deadline, branching on the HTTP status code to decide whether to retry, parsing the `retry-after` header, and a special `TooBig` exception for "the payload was too large." What's missing: this logic has a lot of branches, and the gap list recorded for this project during the rewrite (the `gap_titles` field in the rewrite study's per-project data, `research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv`) notes "the binary-search retry path for TooBig was never triggered" — the constructed test inputs never managed to reach that branch, showing that this kind of code is not only expensive to write, but expensive to test fully.

**Approach two, different backoff multipliers by error type** (`jev_bracket/jev.py:77-93`, repository `meetr1912/jev-bracket`; quoted verbatim, no inline comments at all):

```python
for attempt in range(attempts):
    try:
        response = CLIENT.post(ENDPOINT, json=body, headers={"Authorization": f"Bearer {key}"})
    except httpx.HTTPError as error:
        last_error = error
        time.sleep(0.4 * 2**attempt)
        continue
    if response.status_code in {429, 529, 503} and attempt < attempts - 1:
        time.sleep(0.5 * 2**attempt)
        continue
    if response.is_error:
        raise JevError(f"TypeSafe returned HTTP {response.status_code}")
    try:
        payload = response.json()
        _validate(payload["answers"], questions)
    except (KeyError, TypeError, ValueError, JevError) as error:
        last_error = error
        time.sleep(0.3 * 2**attempt)
        continue
```

What it's doing: similar in spirit to Example 7's first approach (exponential backoff), but this project invented a different set of concrete numbers of its own — a 0.4-second starting wait for network errors, 0.5 seconds for rate-limiting/overload, 0.3 seconds for a response-parsing failure, all with a base of 2 — none of the three starting values has any stated rationale, and a different project would very likely pick three different numbers.

**Approach three, no retries after repeated "illegal answer" judgments — fall back directly to a safe default action** (`system_one_poker/jev.py:198-204`, repository `dperezcabrera/system-one-poker`):

```python
try:
    answer = await self._llm.choose(gateway, self._upstream, state, INSTRUCTIONS, criteria, attempts)
except IllegalAnswers as e:
    by_kind = {option.kind: option for option in options}
    safe = by_kind.get("fold") or by_kind.get("check") or options[0]
    return Decision(safe.key, {}, e.input_tokens, e.output_tokens, e.cost_usd, e.seconds, forfeit=True)
```

What it's doing: in this poker game, an answer that isn't one of the candidate actions counts as "illegal"; once retries are exhausted, it doesn't retry again — it picks a "safe" action directly (fold, or check if there's no fold option, or the first option if there's neither), and marks the move as a forfeit. This is a completely different structure from the first two approaches — those ask again at a different time; this one stops asking and falls back to a hardcoded default.

The three approaches represent three different philosophies for handling this (a full retry infrastructure, per-error-type backoff, forfeit to a safe default once the limit is hit); the data in `research/data/2026-09-27-repetition/` shows 79.0% (v2 scope, section 3.1) of projects wrote some version of retry/failure handling, but with no shared approach — every project designs its own from scratch.

### Example 8: the confidence field — 24 projects all misread the same thing

The public rewrite-study report's "call counts" section records: "24 of the 84 projects read the judge's returned `confidence` field directly and set a threshold on it... existing sampling checks show that this field diverges from the 'top probability' for about 7% of readings on choice questions, and for nearly all readings on scored questions." What it's doing: 24 different projects each independently assumed that the SDK's `confidence` field means "how trustworthy this answer is," and compared it against their own threshold. What's missing: `confidence` is not the same as the largest probability in the reading's distribution — the two often disagree on a real judge; in Example 3, Q-03's `NoulAnswer` doesn't even have a `confidence` field at all, and `getattr`'s default of 1.0 makes the threshold meaningless. 24 projects, 24 different pieces of code — what's repeated here is the same misunderstanding, not the same block of code — which shows exactly that "repetition" isn't only the copy-paste kind; it's the same category of conceptual pitfall, independently stepped into 24 times.

### Example 9: question assembly — a question's wording has to change with context, so the author hand-writes a template to handle the variation

`policy.py:64-76` (repository `lykycy123/RoboJEV`, a robot-arm grasping task):

```python
def axis_criteria(axis):
    """Describe task-conditioned alternatives; the model selects the branch and direction."""
    if axis in "xy":
        return {
            "zero": "No horizontal motion if cube is released and resting inside target. "
                    "When holding: zero if target XY already aligned OR cube_clear_of_table_for_transport is false. "
                    f"Otherwise zero if target_from_cube.directions.{axis} is zero. "
                    f"When not holding: zero if grasp_tcp_from_tcp.directions.{axis} is zero.",
            **{v: f"Move {axis.upper()} {v} when NOT holding and grasp_tcp_from_tcp.directions.{axis} is {v}. "
                  f"When holding, move {v} ONLY if cube_clear_of_table_for_transport is true AND "
                  f"target_from_cube.directions.{axis} is {v}. Never transport before lifting clear. "
               for v in ("negative", "positive")},
        }
```

What it's doing: the robot arm needs one question asked per axis — X, Y, Z — about "which direction should it move," and each axis's candidate descriptions say almost the same thing (just substituting the axis name); the author writes a dedicated function, one layer of f-string interpolation, and a dict comprehension to generate these three near-identical-but-slightly-different sets of candidate descriptions, to avoid copying the same English text three times. What's missing: this isn't a particularly complex judgment — the rule for all three axes is essentially the same rule with a different variable substituted in — but the author still has to write a dedicated function and work out exactly which part should be templated and which part varies by axis; this layer of "how to separate what varies from what doesn't" has nothing to do with whether this project can successfully grip a cube — it is purely the development effort that comes from the act of "assembling material and candidates into text you can send to the judge." Set alongside Example 5 (`pulso-nps`, using string interpolation to assemble a two-round question) and Example 6 (`hush`, using template literals to assemble an issue-status description): three projects, three tasks (robot arm, customer-support triage, issue triage) — question assembly gets written from scratch every single time, and none of them recognizes anyone else's approach.

## 5. Mapping busywork onto our mechanisms

Each busywork category is mapped to the J++ mechanism — a language rule (checked at compile time) or a runtime mechanism — that takes it over. "Measured effect" only states numbers that already have a real record somewhere in the repository; where there is no number, it says "no number yet," and nothing is invented.

| Busywork category | J++ mechanism (name, where) | What it takes over, what the author still has to write | Measured effect |
|---|---|---|---|
| 1 Call loops and batching | The runtime auto-batches by material/question-form (build step 15i, internal record B0303, internal record); `00-目标与动机-v1.md` §3, item one, "batch scheduling" | The author only writes "ask this question of each item in this batch" — no loop, no `Promise.all`, no manual batch-accumulation queue; the runtime decides which questions ride together in one call | B0303 (recorded when step 15i first landed): 58 values identical, J++'s total calls 1022 → 883, matching the original project's hand-written 883 calls exactly; `B1-06` 20 → 10 (1:1); `B1-07` was recorded at the time as 7 → 6 (a second hop needed a program-structure change to reach 2 calls). The finalized data from a later version (the rewrite study's per-project data, `research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv`) shows `B1-07` as 2 → 2 — both numbers are cited honestly; the difference comes from other build steps landing in between, not a contradiction — the same project measured at two different points in time, with the later one superseding. Counterexample below: for the 4 cross-candidate-comparison projects (`Q-04`, `B5-09`, `B5-12`, `B6-07`), J++ splits each candidate into its own judgment — 18 → 174 calls; gaps B155/B156 have not been closed yet, and this category of project runs noticeably more expensive than the original |
| 2 Thresholds | Author-declared lines via `cut(r, {declare: {hi, lo}})`; certified lines from a calibration record (B128, internal record) | One statement replaces a scattered set of constant comparisons and nested if/else at the call site (the "constants silently shadow each other" pitfall in Example 1 — because the line is written in exactly one place, there is no second constant to shadow it) | No cross-project accuracy comparison for the thresholding mechanism itself yet; an internal record measured the lower bound for a trial line (60–80 literal examples, 80–100 semantic examples) — that measures the cost of the line-setting process, not the effect of the thresholding mechanism itself |
| 3 Handling uncertainty | The `unsure` three-way exit + J-05, "an undecided result must be consumed" (`00-目标与动机-v1.md` §4; every `unsure` must go to one of four places: refine, escalate, explicitly discard-and-record, or hand off to the caller) | The kind of hand-written judgment in Example 2 ("if the probability's distance from 0.5 is below a threshold, fall back") does not need to be written by every project — the runtime decides uniformly, by the reading's majority bucket or the author's line; unlike the original projects, it is not allowed to quietly drop "uncertain" into a failure list and never touch it again | The public rewrite-study report's "(c) defects and correctness" section records: "when the original project hits an uncertain case, the common approach is to drop it straight into a failure list and never process it again (at least two projects in this rewrite did exactly that) — J++ does not allow this kind of implicit discard." Example 2's floating-point boundary problem is handled uniformly in J++ by `closed` (an explicit open/closed interval declaration, B153/20j-3) and a merge tolerance; `B7-01`'s rewrite record documents exactly this tolerance difference (the merge tolerance defaults to 0, and 1 of 8 tickets differs at a rounding boundary — predicted in advance by the pre-registration) |
| 4 Retry and failure | **Judgment-absence handling (B32/K-039, built)**: when the judge is unavailable (no credentials/profile/calibration record/incompatible model/consecutive failures), the exit is `Unsure(absent)`, routed by J-05's four destinations; a `budget` block carries `absent: {retry: n, backoff, then: escalate\|conservative\|fail}`, defaulting to `escalate`; k consecutive failures trip a circuit breaker; the ledger records the absence event (`research/地基/12-IR与类契约-v0.1.md`, line 194; `research/地基/18-设计总账-v1.md`, K-039: built, `rust/crates/jpp/tests/b32_absent_latency.rs`) | Every project no longer has to write its own version of Example 7's three approaches ("retry count, backoff multiplier, fall back to a safe default once exhausted") — `retry`/`backoff`/`then` are fields on the profile and the `budget` block, not code the author writes by hand each time | K-039 has unit-test coverage (`rust/crates/jpp/tests/b32_absent_latency.rs`) — the mechanism itself is built, not just a design document; but there has been no dedicated real-run comparison against this batch of open-source projects' "what to do when a call fails" scenarios (rewriting them one way, comparing failure-handling behavior another), so there is no number yet for the specific question of "once this mechanism is installed, can Example 7's three approaches be unified into a single statement" |
| 5 Caching | The ledger + `--replay` keyed replay (`00-目标与动机-v1.md` §3, "caching and incrementality") | No project needs to build its own SQLite table or its own JSON-based cache key (Example 4); the same input does not need another real call when replayed | Case 05 (通爻网络/Towow Network) replay spot-check: internal record notes "of the 2,309 tasks in the main run, one in every 150 was sampled, 15 in total... replayed via the ledger's `--replay`: return values matched the original report field-for-field, 0 new calls, 15/15 passed" — only 15 were spot-checked, not all 2,309; a search skeleton record notes "about $0.0001 per real call, zero calls on replay" (internal record) |
| 6 Cost and call counting | The ledger accounts automatically — the author does not need to write a counter or a `Usage` dataclass (Example 4) | The ledger inherently knows how much was spent and how many calls were made — no need to maintain a `cost_usd` property like `commitjev` does | Case 05: "cost was roughly 1/74 of a single read-through by the generation model" (basis: a `claude -p` subscription cost converted at list price, not an actual charge, as stated in the report) — these numbers all come out of the ledger automatically |
| 7 Concurrency limits | **A scheduling pass (K-200/H-011, decided but not built)**: by design, `judge` batches by layer and concurrency adapts to state size by reading the profile's `profile.concurrency` field (`research/地基/12-IR与类契约-v0.1.md`, line 743). Currently, the `Passes.schedule` field exists in `rust/crates/jpp/src/interp.rs` but is permanently off (`research/地基/18-设计总账-v1.md`, K-200: not done). A related but distinct mechanism, budget-exhaustion degrading calls instead of halting (build step 22-0, B93), handles "don't send more calls once the budget is spent," not "cap concurrency at N" | By design, the author should not need to write a semaphore/rate limiter — **but today, this has not been built**, and the program does not automatically limit concurrency | None — this is marked "decided but not built," and no "measured effect" is invented for it. The 846 passing tests from build step 22-0 verify the adjacent budget-exhaustion mechanism, and cannot be treated as a number for concurrency limiting |
| 8 Question assembly | Question-assembly rules built into effects like `judge`/`choice`/`test` (see the glossary entries "question form" and "effect" in `00-定位与方法论-v1.md`) | One judgment statement replaces the kind of hand-written f-string/template concatenation in Example 9 (RoboJEV's repeated template written once per axis; Example 5's `pulso-nps` and Example 6's `hush` are two other independently invented approaches) | The public rewrite-study report: 83 comparable projects, "judgment core only," median fold-normalized ratio of 2.5x — a good part of this number comes from question assembly and threshold checks being replaced by a single statement, but the report does not break out "question assembly" on its own, so it's not possible to say precisely how many lines this one category saves |
| 9 Material trimming | **A fission pass (K-199/K-220/L-035, decided but not built)**: by design, material over a window limit (`noul`: 500 words; `choice`: 1.8k at paragraph granularity; code: 1.5k) is cut along the window, with each object holding its own slot, then merged back by dispatching on the semantic operation (`research/地基/18-设计总账-v1.md`, K-199, L-035). Currently, the `Passes.fission` field exists in `rust/crates/jpp/src/interp.rs` (around lines 245-295) but is permanently off — **this has not been built today, and the `hush` project in Example 6 still has to write `.slice(0, 6000)` by hand** | By design, the author should not have to compute a truncation length by hand — **but today, this has not been built**, and material trimming today still has to be written by the author in J++ too | None. K-220 additionally requires that fission "must be marked as an approximation and validated by model experiments, and cannot be treated as an unconditionally equivalent rewrite" — that experiment hasn't been done either. This gap, found in this document, is a case where the design exists and the code does not |
| 10 Result feedback | The composition layer's `search`/multi-hop judgment skeleton (`rust/lib/compose/search.jpp`, build step 25c, B0302) | No need to hand-write a dictionary like `pulso-nps` does, keyed by index from the previous round's result, to narrow the candidate set; `search` accumulates "propose → ground → check feasibility → check goal" round by round, and routes undecided candidates by carry/refine/handoff instead of discarding them | Build step 25c's record: "about $0.0001 per real call, zero calls on replay"; "two of the composition layer's three skeletons (search, ground pending 25e, graph) have landed" — the search skeleton has landed and has real-run numbers, but there is no dedicated rewrite-comparison number for a scenario like `pulso-nps`'s two-round candidate narrowing specifically |
| 11 Combining multiple judgments | `cut`'s `stat` statistic (max/expect/confidence, B153/B165/B167, build step 20j-3) + author-declared combination rules (build step 20j-4, first paragraph: the author writes the combination rule, then declares a judgment line on the combined score) | No need to hand-write the glue code for weighted sums/voting/cascading like the original projects do; arithmetic over readings is split into two steps — "compute a statistic, then declare a line on the statistic" — both inside the language | The public rewrite-study report: "design decided (B153, B166, B167), corresponding to build step 15k (ranking) and the first paragraph of 20j-4 ... landed; blocked items rewritten with the new version mostly became fully equivalent" — but a real counterexample remains: `B6-11` needs cross-dimension comparison for its cross-axis ranking, which B153 does not currently support; the checker reports J-04, and it's listed as a new gap — not "fully resolved" |
| 12 Calibration and labeling | The calibration-threshold mechanism: author-declared lines (`declare`) are kept separate from certified lines (calibration records); `declare` requires no labeled dataset (B128) | The author can write their own judgment line without being forced to build a labeled dataset and a threshold-tuning script; a certified line is only needed when the language has to vouch for an error rate | Internal record: an offline comparison gives the real lower bound for a trial line (60–80 literal examples, 80–100 semantic examples), with a formal line at roughly 160 literal examples — this number shows that "whether to label" itself has a cost; J++ turns that cost from "every project has to build a labeled set once" into "only needed when a certified line is explicitly required" — it doesn't reduce the cost to zero |

### Counterexamples

- **Material trimming (9) was designed but never built.** The fission pass (K-199/K-220/L-035) is a compile-time mechanism spelled out on its own, outside "the eight things the runtime takes over," in `research/地基/12-IR与类契约-v0.1.md` — but the corresponding switch in `rust/crates/jpp/src/interp.rs` is permanently off today. The trimming that the `hush` project hand-writes in Example 6 — `.slice(0, 6000)`, `.slice(0, 40)` — still has to be decided by the author in J++ today. This category of busywork appears in 54.3% of projects (section 3.1), the fourth most common in this document's sample, and is the clearest counterexample in this section of "the design exists, the code does not."
- **Concurrency limits (7) is likewise designed but never built** (the scheduling pass, K-200/H-011) — the same kind of gap as material trimming, not "never even considered."
- **The judgment-absence mechanism for retry and failure (4) has already been built and has unit tests**, but there is no dedicated real-run comparison for this batch of open-source projects' "what to do when a call fails" scenarios — a mechanism being built and it having a measured effect on these projects are two different claims, and the first being true does not make the second true by default.
- **Combining multiple judgments (11), despite the design being decided, still has a real gap** (`B6-11`'s cross-dimension ranking) — it's not "the mechanism ships and every project instantly becomes equivalent."
- **These four categories (4/7/9/11) are each at a different stage and cannot be collapsed into one sentence**: 4's mechanism is built but its effect on this batch of projects is untested; 7 and 9's mechanisms haven't even been built; 11 is built but leaves a known gap. Neither "J++ has handled all of it" nor "J++ hasn't handled any of it" is accurate — each of the four categories needs to be looked at on its own terms, as in the table above.

## 6. Conclusions

**P1 (line-share median ≥ 50%) does not hold.** Measured with the more accurate method (v2, block-level attribution + context proximity), the median share of busywork lines in the judgment core is 11.1% — higher than the 6.1% from naive line-by-line keywords, but far short of 50%. The distribution has a long tail — 5 projects, including `B6-06` (`diluteoxygen/JevMood`), have a core that is over half busywork, but most (55 of 77) do not. This prediction is wrong by the letter of what was written down; the results of the other two predictions cannot cover for it.

**This 11.1% figure clearly does not line up with another existing dataset — both numbers are put on the table here, without taking either side.** The public rewrite-study report, using a completely different method (counting total lines before and after the rewrite, with no categorization), measures: for 83 comparable projects, "judgment core only," the median fold-normalized ratio is 2.5x — meaning the J++ version, on average, uses only about 40% of the original judgment core's lines, with about 60% cut away; counting in the host glue code both sides have to write, the ratio drops to 1.66x, about 40% cut away. Both of these numbers (60%, 40%) are far larger than the 11.1% this document measures with 12 regex-based categories. The gap on the table means at least one of two things is true: either this document's 12 regex categories missed most of the code J++ genuinely replaces (section 3.3 already confirmed both missed detections and scope-boundary issues), or J++ compresses more than just these 12 busywork categories — other repeated patterns this document never categorized on their own. This document has no way to separate the two possibilities, and that is a gap it has not solved and has to put on the table — more important than "is it 11.1% or 60%" itself, because it bears directly on whether the claim "busywork accounts for most of the development effort" has independent quantitative support.

**P2 (≥ 8 categories each present in ≥ 30% of projects) does not hold — 7/12.** The first-version number (8/12) counted "calibration and labeling" at 42.0%; that number was later found in a self-check to be a false positive — `from __future__ import annotations` (an extremely common Python idiom) matched the word `annotation`, unrelated to labeled data. After fixing this false positive, the real occurrence rate of "calibration and labeling" is 9.9%, dropping the category count from 8 to 7 — short of the line of 8 set in the task. The two highest-ranking categories here, "retry and failure" (79.0%) and "call loops and batching" (75.3%), also rank first and second in the ecosystem sample (P3) — this part of the conclusion does not depend on P2's overall score.

**P3 (ecosystem sample rankings roughly consistent) partly holds.** The categories ranked first and second (retry, call loops) match exactly, but the remaining rankings and rates diverge. Material trimming, concurrency limits, and cost/call counting are over 10 percentage points higher in the ecosystem sample; thresholds, result feedback, and handling uncertainty are over 10 points lower. Combining multiple judgments is higher by 9.1 points, about 2.8 times the hand-picked rate, while result feedback ranks eighth of 12 in the hand-picked sample and last in the ecosystem sample. The coarse keyword sampling can include more SDK adapter and test code, a possible contributor to the higher infrastructure-related rates; it does not establish the cause of every difference (see 3.2). The ecosystem calibration-and-labeling rate retains a known false positive, so that row remains incomparable. Agreement at the top does not justify carrying the other ranks or percentages across samples.

**The three most common busywork categories: retry and failure, call loops and batching, thresholds.** These three rank near the top in both the 84 projects and the ecosystem sample. The nine examples given in this document — Example 2 (deciding "not sure," spanning 3 projects and 3 structures: a symmetric interval, dual gates, an override check), Example 7 (retry and failure, spanning 3 projects and 3 approaches: a full retry infrastructure, per-error-type backoff, forfeit to a safe default once exhausted), Example 5 (two rounds of calls after a candidate-set change), Example 8 (the same `confidence` misunderstanding shared across 24 projects) — all speak directly to the question of "where is the repetition."

**How much busywork actually is there: this document's narrow scope and the rewrite report's broad scope give two different orders of magnitude, and which one to use depends on which question you're asking.** Counting only busywork the 12 keyword categories can recognize, it's just over a tenth of the judgment core; counting "how many lines were saved after switching to J++," the rewrite report says close to six-tenths. "Repetition" as a phenomenon holds up — nearly every project redesigns the same thing from scratch — but "accounts for most of the development effort" is not supported by this document's own 12-category classification, though it is supported by the rewrite report's overall compression ratio — except that the rewrite report measures "how many lines were saved," not "what share of lines is busywork" — the two are not two answers to the same question, and this document does not merge them into a single number; it's left to the reader to decide which one to trust, or to trust each partially. The rewrite report itself acknowledges it never measured the two more direct indicators of development effort — hours and cognitive load.

**J++'s mechanism coverage, category by category from the table, not summarized loosely as "mostly" or "not at all."** 10 of the 12 categories have a built mechanism (1, 2, 3, 4, 5, 6, 8, 10, 11, 12); of these, four — "call loops and batching" (B0303: 1022→883), "handling uncertainty," "caching" (case 05, 15/15 replays), "cost and call counting" (case 05, 1/74 of the cost) — have real-run or rewrite numbers behind them; three — "thresholds," "combining multiple judgments," "calibration and labeling" — have a built mechanism but only partial, indirect numbers (a trial-line lower bound, a build-step landing record for B153); "retry and failure" (judgment-absence handling, B32/K-039) has a built mechanism with unit tests, but no dedicated real-run comparison against this batch of projects; "question assembly" has a built mechanism whose effect is mixed into the overall line-ratio number and cannot be pulled out on its own. "Concurrency limits" and "material trimming" are two clear gaps — designed but not built — where the author still has to write the code today.

**Counterexamples, listed one by one.** 55 of the projects have a judgment core that is less than a fifth busywork — not every project is heavy on busywork; material trimming and concurrency limits are clearly not built yet — not every busywork category has been taken over by J++; judgment-absence handling and threshold-declaration lines both still lack a dedicated real-run comparison against this batch of projects — not every built mechanism has real-run numbers; the middle categories' rankings and magnitudes swing noticeably with the sampling method, and the ecosystem number for "calibration and labeling" still contains the known false positive — the ecosystem sample does not confirm the hand-picked projects' conclusions everywhere; the 11.1% and roughly-60% figures differ by more than fivefold, a genuine gap this material cannot reconcile on its own. These counterexamples define the boundary of what this document can conclude — they are not a sign that this document's argument has failed.
