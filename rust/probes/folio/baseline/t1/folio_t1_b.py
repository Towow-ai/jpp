"""folio T1 · 基线 b：法律本体逐层下探分类，完整职责版本（账本回放 / 预算停机 / 合批 / 认证线）。

(d) 认证线的算法（见 find_boundary 与 compute_lines）：
把每个键的标注行按 item 名排序后奇偶分两半（fitA / fitB，确定性、非随机）。
两个方向各做一次「留出验证」：在一半上用「从极端往里收，遇到第一个错误就停」的办法找一条候选线
（这条线在该半的已决集合里错误数为 0），再拿另一半（完全没参与找线）验证这条候选线的
Clopper–Pearson 错误率上界；只有 n ≥ 22 且 U(k, n) ≤ α 才算候选通过。两个方向都算一遍，
取两个通过的候选里更严格的一个（hi 取大者、lo 取小者），作为最终线——用「没参与找线的那一半」
做验证，是为了不让「在同一份数据上找线又在同一份数据上验证」的选择偏差把置信区间吹成假的；
取更严格的一侧，是为了在两次五五分的运气之间对冲，多留一点安全边际，抵消换成考官另一批
标注后自然的抽样波动。两个方向都没通过（或该键找不到候选）就判该键「待真值」。
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from math import comb

from jev_interface import judge

ALPHA = 0.1       # 线要满足的错误率上界
CONF_C = 0.1       # Clopper–Pearson 置信参数
MIN_N = 22         # 已决行数下限


def clopper_pearson_upper(k: int, n: int, c: float) -> float:
    """使 P(Bin(n, U) <= k) = c 的错误率 U（单侧上界）。k >= n 或 n == 0 时为 1。"""
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2
        s = sum(comb(n, i) * mid ** i * (1 - mid) ** (n - i) for i in range(k + 1))
        if s > c:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def _group_by_p(samples):
    groups = defaultdict(list)
    for p, correct in samples:
        groups[p].append(correct)
    return groups


def search_hi_boundary(samples):
    """从最大 p 往下收，取到第一个含错误的分组为止；返回 (边界 T, 已决行数)。"""
    groups = _group_by_p(samples)
    boundary, n = None, 0
    for p in sorted(groups, reverse=True):
        vals = groups[p]
        if all(vals):
            boundary, n = p, n + len(vals)
        else:
            break
    return boundary, n


def search_lo_boundary(samples):
    """从最小 p 往上收，取到第一个含错误的分组为止；返回 (边界 T, 已决行数)。"""
    groups = _group_by_p(samples)
    boundary, n = None, 0
    for p in sorted(groups):
        vals = groups[p]
        if all(vals):
            boundary, n = p, n + len(vals)
        else:
            break
    return boundary, n


def _validate(fold_samples, side: str, boundary: float, c: float):
    if side == "hi":
        decided = [correct for p, correct in fold_samples if p >= boundary]
    else:
        decided = [correct for p, correct in fold_samples if p <= boundary]
    n = len(decided)
    k = sum(1 for correct in decided if not correct)
    return n, k, clopper_pearson_upper(k, n, c)


def find_boundary(rows, extract_correct, side: str):
    """留出验证法找 T（decided 用的原始 p 边界，未减/加 delta）。找不到返回 None。"""
    rows_sorted = sorted(rows, key=lambda r: r["item"])
    fold_a, fold_b = rows_sorted[0::2], rows_sorted[1::2]

    def to_samples(fold):
        return [(r["p"], extract_correct(r)) for r in fold]

    candidates = []
    for search_fold, check_fold in ((fold_a, fold_b), (fold_b, fold_a)):
        search_samples = to_samples(search_fold)
        if side == "hi":
            boundary, _ = search_hi_boundary(search_samples)
        else:
            boundary, _ = search_lo_boundary(search_samples)
        if boundary is None:
            continue
        n, k, u = _validate(to_samples(check_fold), side, boundary, CONF_C)
        if n >= MIN_N and u <= ALPHA:
            candidates.append(boundary)
    if not candidates:
        return None
    return max(candidates) if side == "hi" else min(candidates)


def load_labels(path: str):
    by_key = defaultdict(list)
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            by_key[row["key"]].append(row)
    return by_key


def compute_lines(by_key, deltas: dict) -> dict:
    lines = {}
    for key, rows in by_key.items():
        delta = deltas.get(key)
        if delta is None:
            lines[key] = {"status": "待真值"}
            continue
        op = rows[0].get("op")
        if op in ("select", "measure"):
            t_hi = find_boundary(rows, lambda r: r["pick"] == r["label"], "hi")
            if t_hi is None:
                lines[key] = {"status": "待真值"}
                continue
            lines[key] = {"hi": t_hi - delta, "lo": 0}
        else:
            t_hi = find_boundary(rows, lambda r: r["label"] is True, "hi")
            t_lo = find_boundary(rows, lambda r: r["label"] is False, "lo")
            if t_hi is None or t_lo is None:
                lines[key] = {"status": "待真值"}
                continue
            lines[key] = {"hi": t_hi - delta, "lo": t_lo + delta}
    return lines


def _canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def make_asker(ledger_out: str | None, replay: str | None):
    replay_index = None
    if replay:
        replay_index = {}
        with open(replay, encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                entry = json.loads(line)
                key = (_canon(entry["material"]), _canon(entry["questions"]),
                       _canon(entry.get("over")))
                replay_index[key] = entry["answers"]

    ledger_fh = open(ledger_out, "w", encoding="utf-8") if ledger_out else None

    def ask(material, questions, over=None):
        if replay_index is not None:
            key = (_canon(material), _canon(questions), _canon(over))
            if key not in replay_index:
                sys.stderr.write(
                    f"replay 缺失：题={questions!r} 材料={_canon(material)[:80]}…\n")
                sys.exit(1)
            return replay_index[key]
        answers = judge(material, questions, over)
        if ledger_fh is not None:
            entry = {"material": material, "questions": questions,
                     "over": over, "answers": answers}
            ledger_fh.write(json.dumps(entry, sort_keys=True, ensure_ascii=False) + "\n")
            ledger_fh.flush()
        return answers

    return ask


def argmax_choice(probs: dict, children: list):
    best_i, best_p = None, -1.0
    for i in range(len(children)):
        p = probs.get(f"c{i}", 0.0)
        if p > best_p:
            best_i, best_p = i, p
    return best_i, best_p


def strategy_a(document, tree, root, ask, hi, delta, max_calls, budget_state):
    path = []
    current = root
    stopped = None
    unobserved = []
    steps_taken = 0
    while True:
        children = tree.get(current)
        if not children:
            stopped = "leaf"
            break
        if steps_taken >= 6:
            stopped = "depth"
            break
        if max_calls is not None and budget_state["calls"] >= max_calls:
            stopped = "budget"
            unobserved = list(children)
            break
        q = {"type": "choice", "instructions": "这份法律文书最应归入下列哪一类？"}
        answers = ask(document, [q], children)
        budget_state["calls"] += 1
        steps_taken += 1
        ans = answers[0]
        probs = ans.get("probabilities", {})
        mode_share = ans.get("mode_share")
        idx, maxp = argmax_choice(probs, children)
        selected = None
        if mode_share == 1.0 and idx is not None and maxp >= hi + delta:
            selected = children[idx]
        if selected is not None:
            path.append(selected)
            current = selected
            continue
        stopped = "unsure"
        break
    return {"leaf": current, "path": path, "stopped": stopped, "unobserved": unobserved}


def strategy_b(document, tree, root, ask, hi, lo, delta, max_calls, budget_state):
    frontier = [root]
    leaves = []
    dead_ends = []
    layers = 0
    undecided = 0
    unobserved = []
    for _ in range(6):
        internal = [c for c in frontier if tree.get(c)]
        leaf_part = [c for c in frontier if not tree.get(c)]
        if not internal:
            leaves.extend(leaf_part)
            break
        q_items = []
        for cat in internal:
            for child in tree[cat]:
                q_items.append((cat, child))
        if max_calls is not None and budget_state["calls"] >= max_calls:
            unobserved = [child for (_, child) in q_items]
            break
        questions = [
            {"type": "noul", "instructions": f"这段话的内容是否与「{child}」这个话题相关？"}
            for (_, child) in q_items
        ]
        answers = ask(document, questions, None)
        budget_state["calls"] += 1
        next_frontier = []
        any_yes = {cat: False for cat in internal}
        for (cat, child), ans in zip(q_items, answers):
            p = ans["noul"]
            if p >= hi + delta:
                next_frontier.append(child)
                any_yes[cat] = True
            elif p <= lo - delta:
                pass
            else:
                undecided += 1
        for cat in internal:
            if not any_yes[cat]:
                dead_ends.append(cat)
        leaves.extend(leaf_part)
        layers += 1
        frontier = next_frontier
    return {"leaves": leaves, "dead_ends": dead_ends, "layers": layers,
            "undecided": undecided, "unobserved": unobserved}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    by_key = load_labels("labels.jsonl")
    lines = compute_lines(by_key, materials["deltas"])

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    pending = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if pending:
        print(json.dumps({"status": "待真值", "key": pending[0]}, ensure_ascii=False))
        return

    ask = make_asker(args.ledger_out, args.replay)
    budget_state = {"calls": 0}

    fl = lines["folio-level"]
    ft = lines["form-topic"]
    delta_fl = materials["deltas"]["folio-level"]
    delta_ft = materials["deltas"]["form-topic"]

    single_path = strategy_a(
        materials["document"], materials["tree"], materials["root"], ask,
        fl["hi"], delta_fl, args.max_calls, budget_state,
    )
    multi_path = strategy_b(
        materials["document"], materials["tree"], materials["root"], ask,
        ft["hi"], ft["lo"], delta_ft, args.max_calls, budget_state,
    )

    print(json.dumps({"single_path": single_path, "multi_path": multi_path},
                      ensure_ascii=False))


if __name__ == "__main__":
    main()
