# 公开快照说明 / About this public snapshot

`rust/` 是研究树 `地基/rust-jpp` 的公开快照。本次同步取研究区提交 `85e28bfc`（2026-09-25）；上一次同步到研究区提交 `9716e61b`（2026-09-24）。目录一一对应：研究树里的 `X` 就是这里的 `rust/X`。同步用 `tools/sync-rust-from-research.sh`，它只取已提交的内容。

**本次同步里的一次重命名（研究树步 14a，B74）**：`crates/jpp-core`（AST、检查器、解释器、效应、账本、保形）与 `crates/jpp-cli`（`jpp` 二进制）合并改名为单个 crate `crates/jpp`（lib 目标 + bin 目标）。原 `crates/jpp-core/tests/fixtures/calib_legacy/` 与 `crates/jpp-core/tests/known_defects.rs` 这两个只在公开侧的文件随之手工搬到 `crates/jpp/tests/`（见下节），`tools/sync-rust-from-research.sh` 的 KEEP 列表与两处路径改写已同步更新；README、ROADMAP、CI 工作流与文档里 `-p jpp-cli` 的调用改为 `-p jpp`，`crates/jpp-core/INTERFACE.md` 等链接改指向 `crates/jpp/INTERFACE.md`。历史文档（`rust/前端需求-来自核实.md` 等标了具体研究区提交号的稽核记录）保留旧路径原样，不追溯改写。

`rust/` is a public snapshot of the research tree `地基/rust-jpp`. This sync takes research commit `85e28bfc` (2026-09-25); the previous sync reached research commit `9716e61b` (2026-09-24). Paths map one to one: `X` in the research tree is `rust/X` here. `tools/sync-rust-from-research.sh` performs the sync from committed content only.

**A rename landed in this sync (research-tree step 14a, ruling B74)**: `crates/jpp-core` (AST, checker, interpreter, effects, ledger, conformal) and `crates/jpp-cli` (the `jpp` binary) merged into a single crate `crates/jpp` (a lib target plus a bin target). The two public-only files that used to live under `crates/jpp-core/tests/` — `fixtures/calib_legacy/` and `known_defects.rs` — were moved by hand to `crates/jpp/tests/` as part of this sync (see below); `tools/sync-rust-from-research.sh`'s KEEP list and its two path rewrites were updated to match. README, ROADMAP, the CI workflow and other docs now call `-p jpp` instead of `-p jpp-cli`, and links to `crates/jpp-core/INTERFACE.md` now point at `crates/jpp/INTERFACE.md`. Historical documents that cite a specific research-tree commit (e.g. `rust/前端需求-来自核实.md`) keep their original paths as written, unchanged retroactively.

## 没有带过来的 / Not included

- 研究区的过程记录、黑板、附注在 `rust-jpp` 目录之外，任何一次同步都不带。
- `COORDINATION.md`：代理之间的分工与协作登记。
- `probes/scope/语义R-带材料.jsonl`：含人工抽检的逐条标注。`probes/scope/` 里读它的脚本（`find_items.py`、`class_check.py`、`rule_gradient.py`）在这里跑不起来；由它导出的校准记录只含汇总数（抽检条数与一致率）。

- Process logs, the blackboard and the research notes live outside `rust-jpp` and are never synced.
- `COORDINATION.md`: agent ownership and coordination entries.
- `probes/scope/语义R-带材料.jsonl`: contains per-item human spot-check labels. The scripts in `probes/scope/` that read it (`find_items.py`, `class_check.py`, `rule_gradient.py`) cannot run here; calibration records derived from it carry only aggregates (spot-check count and agreement rate).

## 只在公开侧的文件与改写 / Public-only files and rewrites

- `.gitignore`、`crates/jpp/tests/fixtures/calib_legacy/`（旧格式校准记录夹具）、`crates/jpp/tests/known_defects.rs`（PR #20 评审要求的溢出源码区间断言）、`scripts/ci_public.sh` 与 `scripts/doc_snippets.py`（GitHub CI 用）、本文件。
- 测试里指向研究区 `foundation/profile/profiles/` 的路径改为本仓的 `src/foundation/profile/profiles/`；`calib_load.rs` 改读上面的旧格式夹具。
- 读研究区实验原始数据的两个测试（`e_alloc.rs`、`jev_client.rs` 的置换一致率）在这里读不到数据，打印「跳过」后返回。
- `probes/scope/rule_gradient.py` 的仓库根目录与 `probes/scope/result.json` 里记录的 `--calib` 路径，原是研究区的本机绝对路径，改成相对路径；这两个文件不被测试或金样读取。

- `.gitignore`, `crates/jpp/tests/fixtures/calib_legacy/` (legacy-format calibration fixtures), `crates/jpp/tests/known_defects.rs` (the overflow source-span assertion requested in the PR #20 review), `scripts/ci_public.sh` and `scripts/doc_snippets.py` (used by GitHub CI), and this file.
- Test paths into the research `foundation/profile/profiles/` point at this repo's `src/foundation/profile/profiles/`; `calib_load.rs` reads the legacy fixtures above.
- Two tests that read raw research experiment data (`e_alloc.rs` and the permutation-agreement test in `jev_client.rs`) find no data here, print a skip notice and return.
- The repository root in `probes/scope/rule_gradient.py` and the `--calib` paths recorded in `probes/scope/result.json` were absolute paths on a research machine; they are rewritten as relative paths. Neither file is read by the tests or goldens.

## 注释里的研究区路径 / Research paths in comments

代码注释、测试说明和部分夹具的 `description` 引用研究区文档：`附注/`、`过程记录/`、`评估/`、`研究/`、`实测/`、`进展/` 下的文件，以及带编号的依据与方案文本（`12`、`13`、`20`、`21`）。这些引用保留原样，便于和研究区对账。`12`、`13` 在 `research/地基/` 有 2026-09-21 的副本；其余文档没有公开。每一步做了什么、依据是什么、验证到哪一步，公开说明写在 `docs/progress.md` 与 `docs/updates/`。

Code comments, test notes and some fixture `description` fields cite research documents under `附注/`, `过程记录/`, `评估/`, `研究/`, `实测/` and `进展/`, and the numbered authority and plan texts (`12`, `13`, `20`, `21`). The citations are kept verbatim so they can be matched against the research tree. `12` and `13` have a 2026-09-21 copy under `research/地基/`; the other documents are not published. The public account of each step (what changed, on what basis, how far it was verified) is in `docs/progress.md` and `docs/updates/`.
