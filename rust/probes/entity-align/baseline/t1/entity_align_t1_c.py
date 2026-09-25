"""entity-align T1 手写基线（作者 c）。

实现 probes/entity-align/baseline/任务书.md 描述的实体对齐路由（两份啤酒目录按候选对问
四道题、按关系题的档位路由、三道字段题结果作为 hints），加上通用任务书 T1 部分要求的四项
职责：账本与重放、预算停机、合批与同层一起发、从带真值的标注算认证线。

(d) 认证线的算法：把 labels.jsonl 按 key 分组，每一侧（是非题的「是」/「否」两侧，
打分只有「是」一侧）把样本按判到该侧的优先顺序排序（「是」侧按 p 降序、「否」侧按 p 升序），
按相同 p 值分组（判断规则是 p 与线的比较，同一个 p 只能整体被划进或划出，不能把一组并列的
样本劈成两半），得到一串「累计前缀」（前 1 组、前 2 组、……、全部组）。从「全部组」（最宽松、
覆盖最多标注）往回收缩，取第一个同时满足 n(已决行数) >= 22 与 Clopper-Pearson 单侧上界
U(k, n) <= α(=0.1) 的前缀；该前缀最外沿的原始 p 值，减去（「是」侧/打分）或加上（「否」侧）
δ，就是这一侧的线。「是」侧的线是 hi，「否」侧的线是 lo；打分只算 hi，lo 固定写 0。
两侧都算不出合格前缀（或者是非题出现 lo > hi）时，该键判「待真值」。

这不是先手写一条线再去核验，而是直接在全部标注里搜「统计上站得住、同时覆盖最多标注」的
那条边界，零错误只是它的一个特例（错误数=0时 n=22 即满足上界，这是任务书给出的事实，本算
法把它当作搜索能收敛到的下界，而不是唯一允许的答案）。
"""
from __future__ import annotations

import argparse
import json
import math
import sys

from jev_interface import judge_batch

ALPHA = 0.1  # 线要满足的上界
CP_C = 0.1  # Clopper-Pearson 置信参数 c
MIN_N = 22  # 每侧最少已决行数
EPS = 1e-9

RELATION_Q = {
    "type": "score",
    "instructions": "两条实体描述作为产品是什么关系？",
    "criteria": [
        "它们描述的是两种不同的产品。",
        "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
        "它们描述的是同一款产品。",
    ],
}
NAME_Q = {"type": "noul", "instructions": "两条实体写的是同一个啤酒名吗？"}
BREWERY_Q = {"type": "noul", "instructions": "两条实体来自同一家酒厂吗？"}
STYLE_Q = {"type": "noul", "instructions": "两条实体描述的是同一种啤酒风格吗？"}
QUESTIONS = [RELATION_Q, NAME_Q, BREWERY_Q, STYLE_Q]

OUTCOME_BY_DEGREE = ["leave unlinked", "curator queue", "assert sameAs"]
TALLY_KEY = {"leave unlinked": "unlinked", "curator queue": "curator", "assert sameAs": "sameAs"}

KEY_KIND = {
    "align-link": "score",
    "align-name": "noul",
    "align-brewery": "noul",
    "align-style": "noul",
}


def _canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def _cp_upper(k: int, n: int) -> float:
    """U(k, n)：使 P(Bin(n, U) <= k) = CP_C 的 U（k >= n 或 n = 0 时为 1）。二分求解。"""
    if n == 0 or k >= n:
        return 1.0
    combs = [math.comb(n, i) for i in range(k + 1)]

    def cdf_le_k(p: float) -> float:
        if p <= 0.0:
            return 1.0
        if p >= 1.0:
            return 0.0
        return sum(c * (p ** i) * ((1.0 - p) ** (n - i)) for i, c in enumerate(combs))

    lo, hi = 0.0, 1.0
    for _ in range(60):
        mid = (lo + hi) / 2.0
        if cdf_le_k(mid) > CP_C:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2.0


def _scan(ordered) -> tuple | None:
    """ordered：按纳入优先级排好序的 (value, ok) 列表。相同 value 的项分到同一组，一起进出。
    从最宽松（全部组）往回收缩，找第一个 n(累计计数) >= MIN_N 且 U(k, n) <= ALPHA 的前缀。
    返回 (n, 该前缀最外沿的 value)，找不到时返回 None。"""
    if not ordered:
        return None
    groups = []
    for v, ok in ordered:
        if groups and groups[-1][0] == v:
            val, cnt, errs = groups[-1]
            groups[-1] = (val, cnt + 1, errs + (0 if ok else 1))
        else:
            groups.append((v, 1, 0 if ok else 1))
    cum_n = 0
    cum_k = 0
    prefixes = []
    for val, cnt, errs in groups:
        cum_n += cnt
        cum_k += errs
        prefixes.append((cum_n, cum_k, val))
    for cum_n, cum_k, val in reversed(prefixes):
        if cum_n < MIN_N:
            break
        if cum_k >= cum_n:
            continue
        if _cp_upper(cum_k, cum_n) <= ALPHA:
            return cum_n, val
    return None


def _load_labels(path: str) -> dict:
    by_key: dict = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            by_key.setdefault(row["key"], []).append(row)
    return by_key


def compute_lines(labels_path: str, deltas: dict) -> dict:
    by_key = _load_labels(labels_path)
    lines: dict = {}
    for key, kind in KEY_KIND.items():
        rows = by_key.get(key, [])
        delta = deltas[key]
        if kind == "score":
            samples = [(r["p"], r["pick"] == r["label"]) for r in rows]
            desc = sorted(samples, key=lambda x: x[0], reverse=True)
            res = _scan(desc)
            if res is None:
                lines[key] = {"status": "待真值"}
                continue
            _n, boundary_p = res
            lines[key] = {"hi": boundary_p - delta, "lo": 0.0}
        else:
            samples = [(r["p"], bool(r["label"])) for r in rows]
            desc = sorted(samples, key=lambda x: x[0], reverse=True)
            asc = sorted(samples, key=lambda x: x[0])
            res_hi = _scan(desc)
            res_lo = _scan([(p, not label) for p, label in asc])
            if res_hi is None or res_lo is None:
                lines[key] = {"status": "待真值"}
                continue
            _n_hi, boundary_hi = res_hi
            _n_lo, boundary_lo = res_lo
            hi = boundary_hi - delta
            lo = boundary_lo + delta
            if lo > hi + EPS:
                lines[key] = {"status": "待真值"}
                continue
            lines[key] = {"hi": hi, "lo": lo}
    return lines


def noul_verdict(p: float, line: dict, delta: float) -> str:
    if p >= line["hi"] + delta:
        return "是"
    if p <= line["lo"] - delta:
        return "否"
    return "未决"


def score_degree(probs: dict, line: dict, delta: float, n_degrees: int):
    best_idx, best_p = 0, -1.0
    for i in range(n_degrees):
        p = probs.get(str(i), 0.0)
        if p > best_p:
            best_p, best_idx = p, i
    if best_p >= line["hi"] + delta:
        return best_idx
    return None


def build_candidates(catalog_a: list, catalog_b: list) -> list:
    pairs = []
    for a in catalog_a:
        for b in catalog_b:
            if abs(a["abv"] - b["abv"]) <= 1.5:
                pairs.append((a, b))
    return pairs


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger-out")
    ap.add_argument("--replay")
    ap.add_argument("--max-calls", type=int)
    ap.add_argument("--print-lines", action="store_true")
    args = ap.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    deltas = materials["deltas"]

    lines = compute_lines("labels.jsonl", deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    pending = sorted(k for k, v in lines.items() if "status" in v)
    if pending:
        print(json.dumps({"status": "待真值", "key": pending[0]}, ensure_ascii=False))
        return

    candidates = build_candidates(materials["catalog_a"], materials["catalog_b"])
    if args.max_calls is not None:
        observed = candidates[: args.max_calls]
        unobserved_pairs = candidates[args.max_calls:]
    else:
        observed = candidates
        unobserved_pairs = []

    requests = [({"a": a, "b": b}, QUESTIONS, None) for a, b in observed]

    if args.replay:
        index = {}
        with open(args.replay, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                row = json.loads(line)
                key = (_canon(row["material"]), _canon(row["questions"]), _canon(row.get("over")))
                index[key] = row["answers"]
        answers_list = []
        for material, questions, over in requests:
            key = (_canon(material), _canon(questions), _canon(over))
            if key not in index:
                a_name = material["a"]["name"]
                b_name = material["b"]["name"]
                qs = "、".join(q["instructions"] for q in questions)
                sys.stderr.write(
                    f"账本里找不到这份请求的答案：材料 a={a_name!r} b={b_name!r}，题：{qs}\n"
                )
                sys.exit(1)
            answers_list.append(index[key])
    else:
        answers_list = judge_batch(requests) if requests else []
        if args.ledger_out:
            with open(args.ledger_out, "a", encoding="utf-8") as fh:
                for (material, questions, over), answers in zip(requests, answers_list):
                    row = {"material": material, "questions": questions, "over": over, "answers": answers}
                    fh.write(json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n")

    tally = {"sameAs": 0, "curator": 0, "unlinked": 0}
    routed = []
    for (a, b), answers in zip(observed, answers_list):
        relation_ans, name_ans, brewery_ans, style_ans = answers
        degree = score_degree(relation_ans["probabilities"], lines["align-link"], deltas["align-link"], 3)
        outcome = "curator queue" if degree is None else OUTCOME_BY_DEGREE[degree]
        tally[TALLY_KEY[outcome]] += 1
        hints = {
            "name": noul_verdict(name_ans["noul"], lines["align-name"], deltas["align-name"]),
            "brewery": noul_verdict(brewery_ans["noul"], lines["align-brewery"], deltas["align-brewery"]),
            "style": noul_verdict(style_ans["noul"], lines["align-style"], deltas["align-style"]),
        }
        routed.append({"pair": {"a": a["name"], "b": b["name"]}, "outcome": outcome, "hints": hints})

    unobserved = [{"a": a["name"], "b": b["name"]} for a, b in unobserved_pairs]

    result = {
        "candidates": len(candidates),
        "tally": tally,
        "routed": routed,
        "unobserved": unobserved,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
