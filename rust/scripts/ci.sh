#!/usr/bin/env bash
# J++ 持续集成入口（21 §九·1）。本地与公开仓库的 workflow 调同一个脚本。
# JPP_CI_MODE=report（默认，步 0 起）：每项只报告、记基线，不失败；
# JPP_CI_MODE=fail：超过基线或命令失败即非零退出。各脚本转失败模式的步见 21 §九·1。
set -u
cd "$(dirname "$0")/.."
MODE="${JPP_CI_MODE:-report}"
status=0
step() {
  local name="$1"; shift
  echo "== $name"
  if "$@"; then echo "   通过"; else echo "   未通过"; [ "$MODE" = fail ] && status=1; fi
}
# 已转失败模式的项用 hard：不论 JPP_CI_MODE，超过基线即失败（21 §九·1）。fmt 自步 0b 起（基线 0），clippy 自步 14a 起。
hard() {
  local name="$1"; shift
  echo "== $name"
  if JPP_CI_MODE=fail "$@"; then echo "   通过"; else echo "   未通过（失败模式）"; status=1; fi
}
hard "cargo fmt --check（失败模式，步 0b 起）" python3 scripts/fmt_clippy.py fmt
hard "cargo clippy（失败模式，步 14a 起：告警数不超过基线；清零另开一步）" python3 scripts/fmt_clippy.py clippy
step "cargo test（含金样与重放）" cargo test --workspace --offline -q
hard "grep_effect_names（失败模式，步 15a 起：效应名只在注册表，20 A2）" python3 scripts/grep_effect_names.py
hard "grep_privileges（失败模式，步 25-2 起：内核构造只经能力令牌碰内核能力，B57）" python3 scripts/grep_privileges.py
for s in deps lines grep_constants grep_paths grep_fill grep_rules_checker grep_rt_codes trace kill_switch; do
  step "$s" python3 "scripts/$s.py"
done
step "gen_profiles --check（发行画像与 foundation 来源一致，B73）" python3 scripts/gen_profiles.py --check
step "equiv_pairs（等价写法对调用比，只报告，见脚本头注）" python3 scripts/equiv_pairs.py
step "cold_run（空校准库跑示例与探针，运行期 J-05 计数，报告模式；B96、B81 (c)，步 31-1b）" python3 scripts/cold_run.py
exit $status
