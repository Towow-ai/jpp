import argparse
import json
import sys
from math import comb

from jev_interface import judge_batch

RELATION_INSTRUCTIONS = "两条实体描述作为产品是什么关系？"
RELATION_CRITERIA = [
    "它们描述的是两种不同的产品。",
    "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
    "它们描述的是同一款产品。",
]
NAME_INSTRUCTIONS = "两条实体写的是同一个啤酒名吗？"
BREWERY_INSTRUCTIONS = "两条实体来自同一家酒厂吗？"
STYLE_INSTRUCTIONS = "两条实体描述的是同一种啤酒风格吗？"

RELATION_OUTCOMES = ["leave unlinked", "curator queue", "assert sameAs"]
TALLY_KEYS = ["unlinked", "curator", "sameAs"]

QUESTIONS = [
    {"type": "score", "instructions": RELATION_INSTRUCTIONS, "criteria": RELATION_CRITERIA},
    {"type": "noul", "instructions": NAME_INSTRUCTIONS},
    {"type": "noul", "instructions": BREWERY_INSTRUCTIONS},
    {"type": "noul", "instructions": STYLE_INSTRUCTIONS},
]

NOUL_KEYS = ["align-brewery", "align-name", "align-style"]
SCORE_KEYS = ["align-link"]
ALL_KEYS = sorted(NOUL_KEYS + SCORE_KEYS)

ALPHA = 0.1
CONF_C = 0.1
MIN_N = 22


def canon(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def load_materials():
    with open("materials.json", encoding="utf-8") as fh:
        return json.load(fh)


def load_labels():
    rows = []
    with open("labels.jsonl", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def build_pairs(catalog_a, catalog_b):
    pairs = []
    for a in catalog_a:
        for b in catalog_b:
            if abs(a["abv"] - b["abv"]) <= 1.5:
                pairs.append((a, b))
    return pairs


def binom_cdf(k, n, p):
    # P(Bin(n, p) <= k)
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 1.0 if k >= n else 0.0
    total = 0.0
    for i in range(0, k + 1):
        total += comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return total


def clopper_pearson_upper(k, n, c=CONF_C):
    # U(k, n): the p such that P(Bin(n, p) <= k) = c. k >= n or n == 0 -> 1.0.
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(60):
        mid = (lo + hi) / 2.0
        if binom_cdf(k, n, mid) > c:
            lo = mid
        else:
            hi = mid
    return hi


def best_prefix(ordered_samples, alpha=ALPHA, min_n=MIN_N):
    """ordered_samples: (p, correct) 已按"最有把握先来"的方向排好序（分组内 p 相同）。
    从最有把握的一端向外累加，在每个 p 分组边界处检验 Clopper-Pearson 单侧上界；
    在满足 n>=min_n 且 U(k,n)<=alpha 的边界里取 n 最大的一个（样本量最大，最抗新数据波动）。
    这是「零错误计数」思路的推广：优先把没有错误、样本量尽量大的那段划进已决区，
    错误一旦混进来也允许，只要上界仍压得住。
    返回 (t, n, k) 或 None（找不到满足条件的边界）。"""
    best = None
    n = 0
    k = 0
    i = 0
    total = len(ordered_samples)
    while i < total:
        p = ordered_samples[i][0]
        j = i
        while j < total and ordered_samples[j][0] == p:
            if not ordered_samples[j][1]:
                k += 1
            n += 1
            j += 1
        if n >= min_n:
            u = clopper_pearson_upper(k, n, CONF_C)
            if u <= alpha:
                if best is None or n > best[1]:
                    best = (p, n, k)
        i = j
    return best


def compute_line_noul(rows, delta):
    hi_samples = sorted(((r["p"], r["label"] is True) for r in rows), key=lambda x: -x[0])
    lo_samples = sorted(((r["p"], r["label"] is False) for r in rows), key=lambda x: x[0])
    hi_best = best_prefix(hi_samples)
    lo_best = best_prefix(lo_samples)
    if hi_best is None or lo_best is None:
        return None
    hi = hi_best[0] - delta
    lo = lo_best[0] + delta
    if lo > hi + 1e-9:
        return None
    return {"hi": hi, "lo": lo}


def compute_line_score(rows, delta):
    samples = sorted(((r["p"], r["pick"] == r["label"]) for r in rows), key=lambda x: -x[0])
    hi_best = best_prefix(samples)
    if hi_best is None:
        return None
    hi = hi_best[0] - delta
    return {"hi": hi, "lo": 0}


def compute_lines(materials, labels):
    by_key = {}
    for r in labels:
        by_key.setdefault(r["key"], []).append(r)
    deltas = materials["deltas"]
    lines = {}
    infeasible = []
    for key in ALL_KEYS:
        rows = by_key.get(key, [])
        delta = deltas[key]
        if key in SCORE_KEYS:
            line = compute_line_score(rows, delta)
        else:
            line = compute_line_noul(rows, delta)
        if line is None:
            infeasible.append(key)
            lines[key] = {"status": "待真值"}
        else:
            lines[key] = line
    return lines, infeasible


def route_score(probabilities, threshold):
    ranked = sorted(probabilities.items(), key=lambda kv: int(kv[0]))
    best_idx, best_p = None, -1.0
    for key, p in ranked:
        if p > best_p:
            best_p = p
            best_idx = int(key)
    if best_p >= threshold["hi"] + threshold["delta"]:
        return best_idx
    return None


def route_noul(p, threshold):
    if p >= threshold["hi"] + threshold["delta"]:
        return "是"
    if p <= threshold["lo"] - threshold["delta"]:
        return "否"
    return "未决"


def load_replay(path):
    table = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            key = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
            table[key] = rec["answers"]
    return table


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    materials = load_materials()
    labels = load_labels()
    lines, infeasible = compute_lines(materials, labels)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    if infeasible:
        first_key = sorted(infeasible)[0]
        print(json.dumps({"status": "待真值", "key": first_key}, ensure_ascii=False))
        return

    deltas = materials["deltas"]
    thresholds = {key: {"hi": lines[key]["hi"], "lo": lines[key]["lo"], "delta": deltas[key]}
                  for key in ALL_KEYS}

    catalog_a = materials["catalog_a"]
    catalog_b = materials["catalog_b"]
    pairs = build_pairs(catalog_a, catalog_b)

    if args.max_calls is not None:
        process_pairs = pairs[:args.max_calls]
        unobserved_pairs = pairs[args.max_calls:]
    else:
        process_pairs = pairs
        unobserved_pairs = []

    requests = [({"a": a, "b": b}, QUESTIONS, None) for a, b in process_pairs]

    if args.replay:
        replay_table = load_replay(args.replay)
        answers_list = []
        for material, questions, over in requests:
            key = (canon(material), canon(questions), canon(over))
            answers = replay_table.get(key)
            if answers is None:
                sys.stderr.write(
                    "replay 缺失读数：题=%r 材料=%r\n" % (questions, material)
                )
                sys.exit(1)
            answers_list.append(answers)
    else:
        answers_list = judge_batch(requests) if requests else []
        if args.ledger_out:
            with open(args.ledger_out, "a", encoding="utf-8") as fh:
                for (material, questions, over), answers in zip(requests, answers_list):
                    rec = {"material": material, "questions": questions, "over": over, "answers": answers}
                    fh.write(json.dumps(rec, sort_keys=True, ensure_ascii=False) + "\n")

    tally = {"sameAs": 0, "curator": 0, "unlinked": 0}
    routed = []

    for (a, b), answers in zip(process_pairs, answers_list):
        relation_answer, name_answer, brewery_answer, style_answer = answers

        tier = route_score(relation_answer["probabilities"], thresholds["align-link"])
        outcome_idx = 1 if tier is None else tier
        outcome = RELATION_OUTCOMES[outcome_idx]
        tally[TALLY_KEYS[outcome_idx]] += 1

        hints = {
            "name": route_noul(name_answer["noul"], thresholds["align-name"]),
            "brewery": route_noul(brewery_answer["noul"], thresholds["align-brewery"]),
            "style": route_noul(style_answer["noul"], thresholds["align-style"]),
        }

        routed.append({
            "pair": {"a": a["name"], "b": b["name"]},
            "outcome": outcome,
            "hints": hints,
        })

    result = {
        "candidates": len(pairs),
        "tally": tally,
        "routed": routed,
        "unobserved": [{"a": a["name"], "b": b["name"]} for a, b in unobserved_pairs],
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
