import argparse
import json
import math
import sys

from jev_interface import judge_batch

QUESTION_TEXT = "面试官 b 的专长是否覆盖候选人 a 的技术方向，足以对其进行技术面试？"
LINE_KEY = "iv-cover"
ALPHA = 0.1     # 认证线要求的错误率上界
CONF = 0.1      # Clopper-Pearson 置信参数 c
MIN_N = 22      # 每侧最少已决行数


def _canon(x):
    return json.dumps(x, sort_keys=True, ensure_ascii=False)


def _binom_cdf(k, n, p):
    """P(Bin(n, p) <= k)，用于 Clopper-Pearson 上界求根。"""
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 0.0 if k < n else 1.0
    total = 0.0
    for i in range(0, k + 1):
        total += math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return total


def _cp_upper(k, n, c=CONF):
    """Clopper-Pearson 单侧错误率上界 U(k, n)：使 P(Bin(n, U) <= k) = c 的 U。
    k >= n 或 n = 0 时按任务书规定为 1。用二分法求根（P(Bin(n,U)<=k) 关于 U 单调递减）。"""
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2.0
        if _binom_cdf(k, n, mid) > c:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2.0


def _find_cutoff(samples, direction):
    """在 (p, is_error_if_decided) 样本上找一条切分线 cutoff。
    direction='ge'：已决区为 p >= cutoff（用于「是」侧，hi = cutoff - delta）；
    direction='le'：已决区为 p <= cutoff（用于「否」侧，lo = cutoff + delta）。
    候选切点取样本里所有不同 p 值之间的中点（外加两端），逐个检验是否满足
    n >= MIN_N 且 Clopper-Pearson 上界 U(k, n) <= ALPHA；在满足的候选里，先取
    已决行数最多的（覆盖面最大），再取离最近样本点最远的（离决策边界最稳，
    对新一批同分布标注的抖动最不敏感）。找不到满足条件的切点时返回 None。"""
    ps = sorted(set(p for p, _ in samples))
    if not ps:
        return None
    candidates = [ps[0] - 1e-6]
    for i in range(len(ps) - 1):
        candidates.append((ps[i] + ps[i + 1]) / 2.0)
    candidates.append(ps[-1] + 1e-6)

    best = None
    best_key = (-1, -1.0)
    for c in candidates:
        if direction == "ge":
            decided = [(p, err) for p, err in samples if p >= c]
        else:
            decided = [(p, err) for p, err in samples if p <= c]
        n = len(decided)
        if n < MIN_N:
            continue
        k = sum(1 for _, err in decided if err)
        if _cp_upper(k, n) > ALPHA:
            continue
        left = [v for v in ps if v < c]
        right = [v for v in ps if v > c]
        m_left = (c - max(left)) if left else float("inf")
        m_right = (min(right) - c) if right else float("inf")
        margin = min(m_left, m_right)
        key = (n, margin)
        if key > best_key:
            best_key = key
            best = c
    return best


def _compute_line(rows, delta):
    """rows：某个键、是非题（无 op 或 op == "test"）的 (p, label) 标注行。
    返回 {"hi":.., "lo":..} 或 {"status": "待真值"}。"""
    yes_samples = [(p, label is False) for p, label in rows]   # 是侧：真值为否即错
    no_samples = [(p, label is True) for p, label in rows]     # 否侧：真值为是即错
    c_hi = _find_cutoff(yes_samples, "ge")
    c_lo = _find_cutoff(no_samples, "le")
    if c_hi is None or c_lo is None:
        return {"status": "待真值"}
    hi = round(c_hi - delta, 6)
    lo = round(c_lo + delta, 6)
    if lo > hi:
        return {"status": "待真值"}
    return {"hi": hi, "lo": lo}


def build_lines(labels_path, deltas):
    groups = {}
    with open(labels_path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            if rec.get("op") not in (None, "test"):
                continue  # 本项目只有是非题（noul），其余题型不出现，不处理
            groups.setdefault(rec["key"], []).append((rec["p"], rec["label"]))
    lines = {}
    for key, rows in groups.items():
        delta = deltas.get(key, 0.0)
        lines[key] = _compute_line(rows, delta)
    return lines


def build_pairs(candidates, interviewers):
    pairs = []
    for cand in candidates:
        for iv in interviewers:
            if cand["level"] in iv["levels"]:
                pairs.append((cand, iv))
    return pairs


def make_request(cand, iv):
    material = {"a": cand, "b": iv}
    questions = [{"type": "noul", "instructions": QUESTION_TEXT}]
    return material, questions, None


def load_ledger_index(path):
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


def get_answers(requests, replay_path, ledger_path):
    if not requests:
        return []
    if replay_path:
        index = load_ledger_index(replay_path)
        results = []
        for material, questions, over in requests:
            key = (_canon(material), _canon(questions), _canon(over))
            if key not in index:
                sys.stderr.write(
                    "账本缺少读数：题=%s 材料=%s\n" % (_canon(questions), _canon(material))
                )
                sys.exit(1)
            results.append(index[key])
        return results

    results = judge_batch(requests)
    if ledger_path:
        with open(ledger_path, "a", encoding="utf-8") as fh:
            for (material, questions, over), answers in zip(requests, results):
                rec = {"material": material, "questions": questions,
                       "over": over, "answers": answers}
                fh.write(_canon(rec) + "\n")
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger-out")
    ap.add_argument("--replay")
    ap.add_argument("--max-calls", type=int, default=None)
    ap.add_argument("--print-lines", action="store_true")
    args = ap.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    candidates = materials["candidates"]
    interviewers = materials["interviewers"]
    deltas = materials["deltas"]

    lines = build_lines("labels.jsonl", deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    pending = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if pending:
        print(json.dumps({"status": "待真值", "key": pending[0]}, ensure_ascii=False))
        return

    hi = lines[LINE_KEY]["hi"]
    lo = lines[LINE_KEY]["lo"]
    delta = deltas[LINE_KEY]

    pairs = build_pairs(candidates, interviewers)
    pairs_asked = len(pairs)

    all_requests = [make_request(cand, iv) for cand, iv in pairs]

    if args.max_calls is None:
        n_send = len(all_requests)
    else:
        n_send = max(0, min(args.max_calls, len(all_requests)))

    sent_pairs = pairs[:n_send]
    unobserved_pairs = pairs[n_send:]
    sent_requests = all_requests[:n_send]

    answers = get_answers(sent_requests, args.replay, args.ledger_out)

    review = []
    rejected = []
    decided_yes_pairs = []
    for (cand, iv), ans in zip(sent_pairs, answers):
        p = ans[0]["noul"]
        if p >= hi + delta:
            decided_yes_pairs.append((cand, iv))
        elif p <= lo - delta:
            rejected.append({"candidate": cand["name"], "interviewer": iv["name"]})
        else:
            review.append({"candidate": cand["name"], "interviewer": iv["name"]})

    plan = []
    assigned = set()
    iv_counts = {}
    for cand, iv in decided_yes_pairs:
        if cand["name"] in assigned:
            continue
        cnt = iv_counts.get(iv["name"], 0)
        if cnt >= 2:
            continue
        plan.append({"candidate": cand["name"], "interviewer": iv["name"]})
        assigned.add(cand["name"])
        iv_counts[iv["name"]] = cnt + 1

    unplaced = [c["name"] for c in candidates if c["name"] not in assigned]
    unobserved = [{"candidate": c["name"], "interviewer": iv["name"]} for c, iv in unobserved_pairs]

    output = {
        "pairs_asked": pairs_asked,
        "plan": plan,
        "unplaced": unplaced,
        "review": review,
        "rejected": rejected,
        "unobserved": unobserved,
    }
    print(json.dumps(output, ensure_ascii=False))


if __name__ == "__main__":
    main()
