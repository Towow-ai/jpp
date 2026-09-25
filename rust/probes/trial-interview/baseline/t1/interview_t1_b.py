"""interview T1：候选人配对面试官，含账本重放、预算停机、合批与认证线四项职责。

线（hi/lo）的算法（(d) 用的方法，写在这里而不是散在各处）：
按题的键（本项目只有 iv-cover，是非题）把 labels.jsonl 分组成 (p, label) 样本。
对「是」侧：取所有 label=false 样本里最大的 p（记 mp），只在 p > mp 的 label=true 样本里选
最小的 p（记 mt），把切点定在 mp 与 mt 的中点——这是「零错误」且已决样本数最大的切点
（含零错误的整段高分真样本），margin 也最大，最抗新样本的小幅波动。hi = 切点 - delta。
对「否」侧对称：在 label=true 样本里取最小 p（记 mt2），只在 p < mt2 的 label=false 样本里
选最大的 p（记 mf2），切点取两者中点，lo = 切点 + delta。
算出切点后核验已决行数 n>=22 且 Clopper-Pearson 单侧上界 U(k,n)<=alpha（这里 k 应为 0）；
不满足、或某一侧完全找不到候选（比如全是一种标签且中间样本不够），该键判「待真值」。
"""
from __future__ import annotations

import argparse
import json
import math
import os
import sys
from collections import Counter

from jev_interface import judge_batch

QUESTION_TEXT = "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？"
ALPHA = 0.1
CONF_C = 0.1
MIN_DECIDED = 22


def _canon(x) -> str:
    return json.dumps(x, sort_keys=True, ensure_ascii=False)


def _binom_cdf_le(k: int, n: int, p: float) -> float:
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 1.0 if k >= n else 0.0
    total = 0.0
    for i in range(0, k + 1):
        total += math.comb(n, i) * (p ** i) * ((1.0 - p) ** (n - i))
    return total


def _clopper_pearson_upper(k: int, n: int, c: float) -> float:
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2.0
        if _binom_cdf_le(k, n, mid) > c:
            lo = mid
        else:
            hi = mid
    return hi


def _load_bool_samples(labels_path: str) -> dict:
    by_key: dict = {}
    with open(labels_path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            op = row.get("op")
            if op not in (None, "test"):
                continue
            by_key.setdefault(row["key"], []).append((float(row["p"]), bool(row["label"])))
    return by_key


def _find_hi_cutoff(samples):
    trues = [p for p, lab in samples if lab]
    falses = [p for p, lab in samples if not lab]
    max_false = max(falses) if falses else None
    if max_false is None:
        candidates = trues
    else:
        candidates = [p for p in trues if p > max_false]
    if not candidates:
        return None
    min_true_above = min(candidates)
    cutoff = min_true_above if max_false is None else (max_false + min_true_above) / 2.0
    n = sum(1 for p, _ in samples if p >= cutoff)
    k = sum(1 for p, lab in samples if p >= cutoff and not lab)
    if n < MIN_DECIDED:
        return None
    if _clopper_pearson_upper(k, n, CONF_C) > ALPHA:
        return None
    return cutoff


def _find_lo_cutoff(samples):
    trues = [p for p, lab in samples if lab]
    falses = [p for p, lab in samples if not lab]
    min_true = min(trues) if trues else None
    if min_true is None:
        candidates = falses
    else:
        candidates = [p for p in falses if p < min_true]
    if not candidates:
        return None
    max_false_below = max(candidates)
    cutoff = max_false_below if min_true is None else (max_false_below + min_true) / 2.0
    n = sum(1 for p, _ in samples if p <= cutoff)
    k = sum(1 for p, lab in samples if p <= cutoff and lab)
    if n < MIN_DECIDED:
        return None
    if _clopper_pearson_upper(k, n, CONF_C) > ALPHA:
        return None
    return cutoff


def derive_lines(needed_keys, labels_path: str, deltas: dict) -> dict:
    by_key = _load_bool_samples(labels_path)
    lines = {}
    for key in needed_keys:
        samples = by_key.get(key, [])
        delta = deltas[key]
        hi_cut = _find_hi_cutoff(samples)
        lo_cut = _find_lo_cutoff(samples)
        if hi_cut is None or lo_cut is None:
            lines[key] = {"status": "待真值"}
            continue
        hi = hi_cut - delta
        lo = lo_cut + delta
        if lo > hi:
            lines[key] = {"status": "待真值"}
            continue
        lines[key] = {"hi": hi, "lo": lo}
    return lines


def build_pairs(materials: dict):
    pairs = []
    for c in materials["candidates"]:
        for iv in materials["interviewers"]:
            if c["level"] in iv["levels"]:
                pairs.append((c, iv))
    return pairs


def make_request(c: dict, iv: dict):
    material = {"a": c, "b": iv}
    questions = [{"type": "noul", "instructions": QUESTION_TEXT}]
    return material, questions, None


def load_ledger_index(path: str) -> dict:
    index = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            key = (_canon(rec["material"]), _canon(rec["questions"]), _canon(rec.get("over")))
            index[key] = rec["answers"]
    return index


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger-out")
    ap.add_argument("--replay")
    ap.add_argument("--max-calls", type=int, default=None)
    ap.add_argument("--print-lines", action="store_true")
    args = ap.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    deltas = materials["deltas"]
    needed_keys = sorted(deltas.keys())

    lines = derive_lines(needed_keys, "labels.jsonl", deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return 0

    bad_keys = sorted(k for k in needed_keys if "status" in lines[k])
    if bad_keys:
        print(json.dumps({"status": "待真值", "key": bad_keys[0]}, ensure_ascii=False))
        return 0

    hi = lines["iv-cover"]["hi"]
    lo = lines["iv-cover"]["lo"]
    delta = deltas["iv-cover"]

    pairs = build_pairs(materials)
    limit = args.max_calls if args.max_calls is not None else len(pairs)
    asked_pairs = pairs[:limit]
    unobserved_pairs = pairs[limit:]

    requests = [make_request(c, iv) for c, iv in asked_pairs]

    if args.replay:
        index = load_ledger_index(args.replay)
        answers_list = []
        for (c, iv), (material, questions, over) in zip(asked_pairs, requests):
            key = (_canon(material), _canon(questions), _canon(over))
            if key not in index:
                msg = (
                    f"replay 缺失读数：题={QUESTION_TEXT!r} "
                    f"候选人={c['name']!r} 面试官={iv['name']!r}\n"
                )
                sys.stderr.write(msg)
                return 1
            answers_list.append(index[key])
    else:
        results = judge_batch(requests) if requests else []
        answers_list = list(results)

    ledger_fh = None
    if args.ledger_out:
        ledger_fh = open(args.ledger_out, "w", encoding="utf-8")

    try:
        if ledger_fh is not None:
            for (material, questions, over), answers in zip(requests, answers_list):
                rec = {
                    "material": material,
                    "questions": questions,
                    "over": over,
                    "answers": answers,
                }
                ledger_fh.write(_canon(rec) + "\n")
    finally:
        if ledger_fh is not None:
            ledger_fh.close()

    plan = []
    review = []
    rejected = []
    verdict_by_index = {}
    for i, ((c, iv), answers) in enumerate(zip(asked_pairs, answers_list)):
        p = answers[0]["noul"]
        if p >= hi + delta:
            verdict = "是"
        elif p <= lo - delta:
            verdict = "否"
        else:
            verdict = "未决"
        verdict_by_index[i] = verdict
        if verdict == "否":
            rejected.append({"candidate": c["name"], "interviewer": iv["name"]})
        elif verdict == "未决":
            review.append({"candidate": c["name"], "interviewer": iv["name"]})

    assigned_candidates = set()
    interviewer_count = Counter()
    for i, (c, iv) in enumerate(asked_pairs):
        if verdict_by_index[i] != "是":
            continue
        if c["name"] in assigned_candidates:
            continue
        if interviewer_count[iv["name"]] >= 2:
            continue
        plan.append({"candidate": c["name"], "interviewer": iv["name"]})
        assigned_candidates.add(c["name"])
        interviewer_count[iv["name"]] += 1

    unplaced = [
        c["name"] for c in materials["candidates"] if c["name"] not in assigned_candidates
    ]
    unobserved = [{"candidate": c["name"], "interviewer": iv["name"]} for c, iv in unobserved_pairs]

    result = {
        "pairs_asked": len(pairs),
        "plan": plan,
        "unplaced": unplaced,
        "review": review,
        "rejected": rejected,
        "unobserved": unobserved,
    }
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
