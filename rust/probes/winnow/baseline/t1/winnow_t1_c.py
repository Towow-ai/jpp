"""winnow T1（作者 c）：工具输出逐块筛选，带账本/回放、预算停机、合批、认证线四项职责。

认证线算法（(d)，写在这里因为它是「怎么算」而不是「代码在哪」）：
对每个键、每一侧，取标注里对面类别里最靠近决策边界的极值作为线——
「是」侧取全部假例里最大的 p 当 hi，「否」侧取全部真例里最小的 p 当 lo。
这样 hi、lo 本身就是「零错误」的边界：判「是」的门槛是 hi+delta，严格大于我们见过的任何假例的
p，所以在本键全部标注上判「是」的一侧错误数恒为 0（「否」侧同理，恒为 0）。deltas 里现成的 δ
当了安全边际，不用另外再留出校验集或切分。再检查已决行数 ≥ 22（配合 0 错误，
Clopper–Pearson 上界 U(0, 22) ≈ 0.0994 ≤ α=0.1，任务书已给出这个数学事实，
不必在本程序里再实现不完全贝塔函数）以及 lo ≤ hi；两者但凡有一个不满足，或某一类
（真/假）在该键里压根没出现过，这个键就记「待真值」。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys

from jev_interface import judge_batch

ERROR_KEY = "winnow-error"
TOPIC_KEY = "form-topic"

ERROR_Q_TEXT = "这段工具输出里是否含有报错、失败或异常信息？"


def canon(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def load_materials():
    with open("materials.json", encoding="utf-8") as fh:
        return json.load(fh)


def load_labels():
    rows_by_key = {}
    with open("labels.jsonl", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            rows_by_key.setdefault(rec["key"], []).append(rec)
    return rows_by_key


def compute_lines(rows_by_key, deltas):
    lines = {}
    for key, delta in deltas.items():
        rows = rows_by_key.get(key, [])
        trues = [r["p"] for r in rows if r["label"] is True]
        falses = [r["p"] for r in rows if r["label"] is False]
        if not trues or not falses:
            lines[key] = {"status": "待真值"}
            continue
        hi = max(falses)
        lo = min(trues)
        cutoff_yes = hi + delta
        cutoff_no = lo - delta
        n_yes = sum(1 for r in rows if r["p"] >= cutoff_yes)
        err_yes = sum(1 for r in rows if r["p"] >= cutoff_yes and r["label"] is False)
        n_no = sum(1 for r in rows if r["p"] <= cutoff_no)
        err_no = sum(1 for r in rows if r["p"] <= cutoff_no and r["label"] is True)
        if lo <= hi and n_yes >= 22 and err_yes == 0 and n_no >= 22 and err_no == 0:
            lines[key] = {"hi": hi, "lo": lo}
        else:
            lines[key] = {"status": "待真值"}
    return lines


def first_pending_key(lines):
    pending = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    return pending[0] if pending else None


def classify(p, hi, lo, delta):
    if p >= hi + delta:
        return "是"
    if p <= lo - delta:
        return "否"
    return "未决"


def build_requests(item, task):
    chunks = item["chunks"]
    material_err = "\n".join(chunks)
    q_err = {"type": "noul", "instructions": ERROR_Q_TEXT}
    requests = [(material_err, [q_err], None)]
    for chunk in chunks:
        q_rel = {
            "type": "noul",
            "instructions": f"这段话的内容是否与「{task}」这个话题相关？",
        }
        requests.append((chunk, [q_rel], None))
    return requests


def load_replay_index(path):
    index = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            key = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
            index[key] = rec["answers"]
    return index


def resolve_batch(requests, replay_index, ledger_fh):
    if not requests:
        return []
    if replay_index is not None:
        answers = []
        for material, questions, over in requests:
            key = (canon(material), canon(questions), canon(over))
            if key not in replay_index:
                sys.stderr.write(
                    f"replay 缺失读数：题={questions!r} 材料={canon(material)[:120]}\n"
                )
                sys.exit(1)
            answers.append(replay_index[key])
    else:
        answers = judge_batch(requests)
    if ledger_fh is not None:
        for (material, questions, over), ans in zip(requests, answers):
            rec = {"material": material, "questions": questions, "over": over, "answers": ans}
            ledger_fh.write(json.dumps(rec, sort_keys=True, ensure_ascii=False) + "\n")
            ledger_fh.flush()
    return answers


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    args = parser.parse_args()

    materials = load_materials()
    deltas = materials["deltas"]
    rows_by_key = load_labels()
    lines = compute_lines(rows_by_key, deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    pending = first_pending_key(lines)
    if pending is not None:
        print(json.dumps({"status": "待真值", "key": pending}, ensure_ascii=False))
        return

    error_line = lines[ERROR_KEY]
    topic_line = lines[TOPIC_KEY]
    error_delta = deltas[ERROR_KEY]
    topic_delta = deltas[TOPIC_KEY]

    task = materials["tool_outputs"]["task"]
    results_in = materials["tool_outputs"]["results"]

    replay_index = load_replay_index(args.replay) if args.replay else None
    ledger_fh = open(args.ledger_out, "w", encoding="utf-8") if args.ledger_out else None

    used = 0
    per_output = []
    for item in results_in:
        chunks = item["chunks"]
        requests_full = build_requests(item, task)
        n_full = len(requests_full)
        if args.max_calls is None:
            n_send = n_full
        else:
            remaining = max(0, args.max_calls - used)
            n_send = min(n_full, remaining)
        sent = requests_full[:n_send]
        answers = resolve_batch(sent, replay_index, ledger_fh)
        used += n_send
        full_answers = answers + [None] * (n_full - n_send)
        per_output.append((item, chunks, full_answers))

    if ledger_fh is not None:
        ledger_fh.close()

    out_results = []
    for item, chunks, full_answers in per_output:
        error_ans = full_answers[0]
        if error_ans is None:
            reason = "error_unobserved"
            pruned, uncertain, archive = [], [], []
            text = list(chunks)
        else:
            err_p = error_ans[0]["noul"]
            outcome = classify(err_p, error_line["hi"], error_line["lo"], error_delta)
            if outcome == "是":
                reason = "error_present"
                pruned, uncertain, archive = [], [], []
                text = list(chunks)
            elif outcome == "未决":
                reason = "error_unsure"
                pruned, uncertain, archive = [], [], []
                text = list(chunks)
            else:
                candidates, unsure = [], []
                for j in range(len(chunks)):
                    ans_j = full_answers[1 + j]
                    if ans_j is None:
                        continue
                    rp = ans_j[0]["noul"]
                    c = classify(rp, topic_line["hi"], topic_line["lo"], topic_delta)
                    if c == "否":
                        candidates.append(j)
                    elif c == "未决":
                        unsure.append(j)
                candidate_chars = sum(len(chunks[j]) for j in candidates)
                total_chars = sum(len(c) for c in chunks)
                if candidate_chars * 100 < total_chars * 20:
                    reason = "below_min_prune_ratio"
                    pruned, uncertain, archive = [], [], []
                    text = list(chunks)
                else:
                    reason = "pruned"
                    pruned = sorted(candidates)
                    uncertain = sorted(unsure)
                    pruned_set = set(pruned)
                    text, archive = [], []
                    for j, c in enumerate(chunks):
                        if j in pruned_set:
                            digest = hashlib.sha256(c.encode("utf-8")).hexdigest()[:16]
                            text.append(f"[winnow 已折叠 {len(c)} 字 · 展开键 {digest}]")
                            archive.append({"key": digest, "index": j, "text": c})
                        else:
                            text.append(c)

        unobserved = sorted(j for j in range(len(chunks)) if full_answers[1 + j] is None)
        out_results.append({
            "id": item["id"],
            "reason": reason,
            "pruned": pruned,
            "uncertain": uncertain,
            "text": text,
            "archive": archive,
            "unobserved": unobserved,
        })

    print(json.dumps({"task": task, "results": out_results}, ensure_ascii=False))


if __name__ == "__main__":
    main()
