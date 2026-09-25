#!/usr/bin/env python3
"""等价写法对：慢写法与快写法在同一固定观察下的调用次数比（阶段评估①建议4；20a D19，
`地基/评估/2026-09-24-阶段评估-1.md` §五·4、§一 D19 行）。

对子定义在 `tests/equiv_pairs/manifest.json`（三对，2026-09-24 现状）：
  sieve_batch  14:1   sieve-batch-old（逐题单发）/ sieve-batch-new（一次批发）
  iterate      24:12  iterate.jpp 内两条语义等价链路（step/step_drop）未跨调用位置复用
                       （地基/过程记录/2026-09-23-施工efg.md 第38行）；「12」＝单条链路
                       本该需要的调用数，取 value.by_count.measures[:rounds] 求和
  refund       14:10  地基/评估/2026-09-24-试写/refund 的链式 / 平铺一对

只用报告模式（Nature 任务书：本检查现在只报告、记基线，比值上升不使 CI 失败）。
步 13b（向量化穿过用户函数包装）把 sieve_batch 从 14.00 降到 1.00，基线随之重记。iterate 的 24:12
来自账本键含调用位置，归步 19（`21` §六·5，同题跨位置复用），不归 13b；refund 的第二层输入是第一层的
接受结果，属数据依赖，也不归 13b（`地基/过程记录/工程-步13b.md`）。基线只许降、不许升，指将来转失败
模式后的规则，由那时的改动落实，不由本脚本现在的退出码落实。

风格照 `_baseline.py` 的 `finish`（同一份 ROOT/baselines 目录、同样的 JSON 基线文件、同样的
JPP_CI_UPDATE_BASELINE 更新开关），但 `finish` 的 delta 用 `%d` 格式化整数违规计数，比值是
浮点数，所以这里另写 `report_ratio`，且不读 JPP_CI_MODE、不以非零退出码结束——这是与
`finish` 唯一的行为差异，专为「只用报告模式」这条任务要求而加。
"""
import json
import subprocess
import sys

from _baseline import BASE, ROOT

MANIFEST = ROOT / "tests" / "equiv_pairs" / "manifest.json"
CLI = ["cargo", "run", "-p", "jpp", "--offline", "-q", "--"]


def run(source: str, fixtures: str) -> dict:
    out = ROOT / "target" / "_equiv_pairs_tmp_report.json"
    proc = subprocess.run(
        CLI + ["run", source, "--fixtures", fixtures, "--output", str(out)],
        cwd=ROOT, capture_output=True, text=True,
    )
    if proc.returncode != 0 or not out.exists():
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"[equiv_pairs] jpp run 失败：{source}")
    data = json.loads(out.read_text(encoding="utf-8"))
    out.unlink()
    return data


def report_ratio(name: str, ratio: float, detail: list) -> None:
    """打印比值，与基线比较，记基线；永远报告模式（见模块说明）。"""
    import os

    path = BASE / f"{name}.json"
    base = json.loads(path.read_text()) if path.exists() else None
    if os.environ.get("JPP_CI_UPDATE_BASELINE") == "1" or base is None:
        BASE.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"ratio": ratio, "detail": detail}, ensure_ascii=False, indent=1) + "\n")
        base = {"ratio": ratio}
    delta = ratio - base["ratio"]
    print(f"[{name}] 比值 {ratio:.2f}（基线 {base['ratio']:.2f}，变化 {delta:+.2f}；报告模式，只记录不使 CI 失败）")
    for d in detail:
        print(f"  - {d}")


def two_files(pair: dict) -> None:
    slow = run(pair["slow"], pair["fixtures"])["cost"]["calls"]
    fast = run(pair["fast"], pair["fixtures"])["cost"]["calls"]
    ratio = slow / fast
    report_ratio(
        f"equiv_pairs_{pair['name']}", ratio,
        [f"{pair['slow']} {slow} 次 / {pair['fast']} {fast} 次（{pair['fixtures']}）", pair["note"]],
    )


def single_file_measures(pair: dict) -> None:
    d = run(pair["source"], pair["fixtures"])
    actual = d["cost"]["calls"]
    bc = d["value"]["by_count"]
    needed = sum(bc["measures"][: bc["rounds"]])
    ratio = actual / needed
    report_ratio(
        f"equiv_pairs_{pair['name']}", ratio,
        [
            f"{pair['source']} 实际 {actual} 次 / 单链路所需 {needed} 次"
            f"（by_count.measures={bc['measures']}, rounds={bc['rounds']}，取前 rounds 项求和）",
            pair["note"],
        ],
    )


KINDS = {"two_files": two_files, "single_file_measures": single_file_measures}


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    for pair in manifest["pairs"]:
        KINDS[pair["kind"]](pair)


if __name__ == "__main__":
    main()
