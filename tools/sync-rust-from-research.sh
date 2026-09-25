#!/usr/bin/env bash
# 把研究树 `地基/rust-jpp` 某个提交的状态同步到本仓库的 `rust/`。
# Sync the research tree `地基/rust-jpp` at a given commit into this repo's `rust/`.
#
# 用法 / usage:
#   tools/sync-rust-from-research.sh [研究仓库路径] [提交]
#   默认 ~/个人项目/jev 与其 HEAD；只取已提交内容（git archive），不带工作区未提交的改动。
#
# 目录一一对应：研究树 X ↔ 本仓 rust/X。研究树删掉的文件这里也删（rsync --delete），
# 下面三类例外：
#   1. 只在公开侧的文件（KEEP）：保留，不被删除。
#   2. 不公开的文件（SKIP）：不复制。过程记录、黑板、附注不在 rust-jpp 目录里，本来就不会带上；
#      这里列的是 rust-jpp 里面的协作记录与 Nature 人工抽检的逐条标注。
#   3. 可移植改写（PORT）：测试里指向研究工作区 `foundation/` 的路径改到本仓的 `src/foundation/`，
#      读未公开运行目录的测试改读仓库内夹具。改写找不到原文时报错退出，提醒人工核对。
# 跑完后在 rust/ 下执行 `cargo test --locked --workspace`，并用 `git status` 看清改动再提交。
set -euo pipefail

REPO="${1:-$HOME/个人项目/jev}"
REV="${2:-HEAD}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HERE/rust"

# 2026-09-25（研究树步 14a）：jpp-core 与 jpp-cli 合并改名为 jpp（lib + bin）。
# 只在公开侧的两个文件随之从 crates/jpp-core/tests/ 手动搬到 crates/jpp/tests/（一次性，
# 已在当次同步提交里做完）；这里的 KEEP 路径改指向新位置。
KEEP=(
  /.gitignore
  /target/
  /crates/jpp/tests/fixtures/
  /crates/jpp/tests/known_defects.rs
  /scripts/ci_public.sh
  /scripts/doc_snippets.py
  /PUBLIC-SNAPSHOT.md
)
SKIP=(
  /COORDINATION.md
  /probes/scope/语义R-带材料.jsonl
)

commit="$(git -C "$REPO" rev-parse --verify "$REV^{commit}")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git -C "$REPO" archive "$commit" 地基/rust-jpp | tar -x -C "$tmp"
src="$tmp/地基/rust-jpp"

# KEEP 用 protect（P）：不删、但研究树若有同名文件仍会覆盖；SKIP 用 exclude：不复制。
args=(-a --delete)
for p in "${KEEP[@]}"; do args+=(--filter="P $p"); done
for p in "${SKIP[@]}"; do args+=(--exclude="$p"); done
rsync "${args[@]}" "$src/" "$DEST/"

python3 - "$DEST" <<'PY'
import pathlib, sys

root = pathlib.Path(sys.argv[1])

def rewrite(rel, old, new, count=None, optional=False):
    p = root / rel
    s = p.read_text(encoding="utf-8")
    n = s.count(old)
    if optional and n == 0:
        return
    if n == 0 or (count is not None and n != count):
        sys.exit(f"PORT 改写找不到原文或次数不符（{rel}：{n} 处）：{old[:60]!r}")
    p.write_text(s.replace(old, new), encoding="utf-8")
    print(f"PORT {rel}：{n} 处")

prof_old = '"../../../foundation/profile/profiles/'
prof_new = '"../../../src/foundation/profile/profiles/'
# 2026-09-25：扫描面从 crates/*/tests/*.rs 放宽到每个 crate 的 src/ 与 tests/ 全树（rglob），
# 因为这条路径也会出现在 src 内嵌单元测试里（crates/jpp-effects/src/profile.rs 曾漏改，报
# 「读不到档案」，cargo test 才发现——见 docs/progress.md 2026-09-25 条目）。
for sub in ("src", "tests"):
    for f in sorted((root / "crates").glob(f"*/{sub}/**/*.rs")):
        if prof_old in f.read_text(encoding="utf-8"):
            rewrite(f.relative_to(root), prof_old, prof_new)
rewrite("crates/jpp/tests/wiring.rs",
        '"../foundation/profile/profiles/', '"../src/foundation/profile/profiles/')
rewrite("crates/jpp/tests/calib_load.rs",
        '''fn 真records() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../foundation/runs/jv/e-cal/calib")
}

/// **内核第一次读得到那三条记录。**
#[test]
fn 读得进e_cal那三条真记录() {
    let store = CalibStore::load(&真records()).expect("三条真记录该读得进来");''',
        '''fn 记录夹具() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calib_legacy")
}

/// 旧记录格式的可携带回归：保留字段形状，不依赖未发布的研究运行目录。
#[test]
fn 读得进e_cal旧格式记录夹具() {
    let store = CalibStore::load(&记录夹具()).expect("仓库内三条旧格式夹具该读得进来");''', count=1)

# 探针脚本与运行记录里的本机绝对路径改成相对路径（不被测试或金样读取；研究树改了之后这两条自动跳过）。
rewrite("probes/scope/rule_gradient.py",
        'ROOT = pathlib.Path("/Users/nature/个人项目/jev")\nRJ = ROOT / "地基/rust-jpp"\nCAL = ROOT / "实测/校准题式-2026-09-23"',
        'RJ = pathlib.Path(__file__).resolve().parents[2]  # rust-jpp\nCAL = RJ.parents[1] / "实测/校准题式-2026-09-23"',
        optional=True)
rewrite("probes/scope/rule_gradient.py", "（路径写死为本仓库）", "（路径相对本文件；实测目录在公开仓库里没有）", optional=True)
rewrite("probes/scope/result.json",
        "/Users/nature/个人项目/jev/.claude/worktrees/agent-a9b7f475a79e897cb/地基/rust-jpp/", "", optional=True)
import re as _re
for f in root.rglob("*"):
    if f.is_file() and "target" not in f.parts and f.suffix in {".py", ".json", ".jsonl", ".toml", ".rs", ".md", ".sh"}:
        if _re.search(r"/Users/[A-Za-z]", f.read_text(encoding="utf-8", errors="ignore")):
            print(f"注意：{f.relative_to(root)} 含本机绝对路径，核对后决定是否改写")

# 兜底检查：人工抽检行（source=human 且带 spot_check）不应出现在任何 jsonl 里。
import json
bad = []
for f in root.rglob("*.jsonl"):
    if "target" in f.parts:
        continue
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if isinstance(r, dict) and r.get("source") == "human" and "spot_check" in r:
            bad.append(f"{f.relative_to(root)}:{i}")
if bad:
    sys.exit("发现人工抽检逐条标注，先加进 SKIP：" + ", ".join(bad[:10]))
PY

echo "已同步研究树 $commit → rust/"
echo "Synced research-tree commit $commit into rust/"
