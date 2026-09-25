import argparse
import json
import math
import sys

from jev_interface import judge

CHOICE_Q = "这份法律文书最应归入下列哪一类？"


def canon(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def binom_cdf_le(k, n, p):
    if k >= n:
        return 1.0
    if k < 0:
        return 0.0
    total = 0.0
    for i in range(0, k + 1):
        total += math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return total


def cp_upper(k, n, alpha):
    # Clopper-Pearson 单侧上界：解 U 使 P(Bin(n,U) <= k) = alpha，用二分法求根
    # （P(Bin(n,U)<=k) 关于 U 单调不增，从 1 降到 0）。
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2
        if binom_cdf_le(k, n, mid) > alpha:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


ALPHA = 0.1
MIN_N = 22


def best_cut(samples, direction):
    # samples: [(p, is_error_if_decided), ...]。
    # 方法（(d) 用的方法，见任务书"线怎么用"一节）：
    # 对每一对相邻的不同 p 值取中点作候选线；候选线把样本分成"已决"（按 direction 取
    # >= 候选线 或 <= 候选线）与"未决"两半；对每条候选线算已决行数 n 与已决里的错误数 k，
    # 用 Clopper-Pearson 单侧上界 U(k,n) 与 alpha=0.1 比较，只保留 n>=22 且 U(k,n)<=alpha
    # 的候选。这些候选里选"相邻两个真值 p 之间空档最宽"的那个中点——空档越宽，这条线在
    # 同分布的新标注上被样本落在线两侧翻转判断的风险越小（比单纯挑 n 最大更抗新数据的
    # 抽样波动；本项目里恰好两者重合）。没有任何候选满足条件时返回 None（该键"待真值"）。
    pts = sorted(set(p for p, _ in samples))
    best = None
    for j in range(len(pts) - 1):
        margin = pts[j + 1] - pts[j]
        cand = (pts[j] + pts[j + 1]) / 2
        if direction == "upper":
            decided = [e for (p, e) in samples if p >= cand]
        else:
            decided = [e for (p, e) in samples if p <= cand]
        n = len(decided)
        k = sum(1 for e in decided if e)
        if n >= MIN_N and cp_upper(k, n, ALPHA) <= ALPHA + 1e-9:
            if best is None or margin > best[0] or (margin == best[0] and n > best[1]):
                best = (margin, n, cand)
    return best[2] if best else None


def compute_lines(materials, labels_path):
    deltas = materials["deltas"]
    groups = {}
    with open(labels_path, encoding="utf-8") as fh:
        for raw in fh:
            raw = raw.strip()
            if not raw:
                continue
            rec = json.loads(raw)
            groups.setdefault(rec["key"], []).append(rec)

    lines = {}
    for key in sorted(groups):
        recs = groups[key]
        delta = deltas[key]
        is_choice = any(r.get("op") == "select" for r in recs)
        if is_choice:
            samples = [(r["p"], r["pick"] != r["label"]) for r in recs]
            cut = best_cut(samples, "upper")
            if cut is None:
                lines[key] = {"status": "待真值"}
            else:
                lines[key] = {"hi": cut - delta, "lo": 0.0}
        else:
            hi_samples = [(r["p"], r["label"] is False) for r in recs]
            lo_samples = [(r["p"], r["label"] is True) for r in recs]
            hi_cut = best_cut(hi_samples, "upper")
            lo_cut = best_cut(lo_samples, "lower")
            if hi_cut is None or lo_cut is None:
                lines[key] = {"status": "待真值"}
            else:
                hi = hi_cut - delta
                lo = lo_cut + delta
                if lo > hi + 1e-9:
                    lines[key] = {"status": "待真值"}
                else:
                    lines[key] = {"hi": hi, "lo": lo}
    return lines


def decide_choice(ans, n_over, hi, delta):
    probs = ans.get("probabilities", {})
    vals = [probs.get(f"c{i}", 0.0) for i in range(n_over)]
    best_i, best_v = 0, vals[0] if vals else 0.0
    for i in range(1, n_over):
        if vals[i] > best_v:
            best_v, best_i = vals[i], i
    mode_share = ans.get("mode_share")
    if mode_share is not None and mode_share == 1.0 and best_v >= hi + delta:
        return best_i
    return None


def decide_noul(ans, hi, lo, delta):
    p = ans["noul"]
    if p >= hi + delta:
        return True
    if p <= lo - delta:
        return False
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger-out")
    ap.add_argument("--replay")
    ap.add_argument("--max-calls", type=int, default=None)
    ap.add_argument("--print-lines", action="store_true")
    args = ap.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    lines = compute_lines(materials, "labels.jsonl")

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    bad_keys = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if bad_keys:
        print(json.dumps({"status": "待真值", "key": bad_keys[0]}, ensure_ascii=False))
        return

    root = materials["root"]
    tree = materials["tree"]
    document = materials["document"]
    hi_fl, lo_fl = lines["folio-level"]["hi"], lines["folio-level"]["lo"]
    delta_fl = materials["deltas"]["folio-level"]
    hi_ft, lo_ft = lines["form-topic"]["hi"], lines["form-topic"]["lo"]
    delta_ft = materials["deltas"]["form-topic"]

    replay_index = None
    if args.replay:
        replay_index = {}
        with open(args.replay, encoding="utf-8") as fh:
            for raw in fh:
                raw = raw.strip()
                if not raw:
                    continue
                rec = json.loads(raw)
                rkey = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
                replay_index[rkey] = rec["answers"]

    ledger_fh = open(args.ledger_out, "a", encoding="utf-8") if args.ledger_out else None
    issued = {"n": 0}

    def budget_ok():
        return args.max_calls is None or issued["n"] < args.max_calls

    def dispatch(material, questions, over=None):
        if replay_index is not None:
            rkey = (canon(material), canon(questions), canon(over))
            if rkey not in replay_index:
                sys.stderr.write(
                    f"replay 缺失：questions={questions!r} material={str(material)[:80]!r}\n"
                )
                sys.exit(1)
            answers = replay_index[rkey]
        else:
            answers = judge(material, questions, over=over)
        issued["n"] += 1
        if ledger_fh is not None:
            rec = {"material": material, "questions": questions, "over": over, "answers": answers}
            ledger_fh.write(json.dumps(rec, sort_keys=True, ensure_ascii=False) + "\n")
            ledger_fh.flush()
        return answers

    # 策略 A：单路 K 选一
    category = root
    path = []
    stopped = None
    unobserved_a = []
    for _ in range(6):
        children = tree.get(category)
        if not children:
            stopped = "leaf"
            break
        if not budget_ok():
            stopped = "budget"
            unobserved_a = list(children)
            break
        q = {"type": "choice", "instructions": CHOICE_Q}
        answers = dispatch(document, [q], over=list(children))
        idx = decide_choice(answers[0], len(children), hi_fl, delta_fl)
        if idx is None:
            stopped = "unsure"
            break
        category = children[idx]
        path.append(category)
    else:
        stopped = "depth"

    single_path = {"leaf": category, "path": path, "stopped": stopped, "unobserved": unobserved_a}

    # 策略 B：多路逐子类是非题
    front = [root]
    leaves = []
    dead_ends = []
    layers = 0
    undecided = 0
    unobserved_b = []
    for _ in range(6):
        internal = [c for c in front if tree.get(c)]
        reached = [c for c in front if not tree.get(c)]
        if not internal:
            leaves.extend(reached)
            break

        requests = []
        for cat in internal:
            for child in tree[cat]:
                requests.append((cat, child))

        if not budget_ok():
            unobserved_b = [child for (_, child) in requests]
            break

        questions = [
            {"type": "noul", "instructions": f"这段话的内容是否与「{child}」这个话题相关？"}
            for (_, child) in requests
        ]
        answers = dispatch(document, questions)

        next_front = []
        cat_any_yes = {cat: False for cat in internal}
        idx = 0
        for cat in internal:
            for child in tree[cat]:
                decision = decide_noul(answers[idx], hi_ft, lo_ft, delta_ft)
                idx += 1
                if decision is True:
                    next_front.append(child)
                    cat_any_yes[cat] = True
                elif decision is None:
                    undecided += 1
        for cat in internal:
            if not cat_any_yes[cat]:
                dead_ends.append(cat)

        leaves.extend(reached)
        layers += 1
        front = next_front

    multi_path = {
        "leaves": leaves,
        "dead_ends": dead_ends,
        "layers": layers,
        "undecided": undecided,
        "unobserved": unobserved_b,
    }

    if ledger_fh is not None:
        ledger_fh.close()

    print(json.dumps({"single_path": single_path, "multi_path": multi_path}, ensure_ascii=False))


if __name__ == "__main__":
    main()
