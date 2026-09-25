"""winnow T1（完整职责）· 作者 b。

在 T0 功能（逐块判断工具输出与任务是否相关，折叠不相关块）之上加四项职责：
账本与重放、预算停机、合批与同层一起发、认证线（从 labels.jsonl 算阈值，不手写）。

认证线算法（(d)）说明见 compute_line_for_key 的注释；只为本项目 labels.jsonl
里出现的题型（是非题 noul，没有 op 字段）写代码。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "_baseline"))
from jev_interface import judge_batch  # noqa: E402

ALPHA = 0.1  # 线要满足：Clopper-Pearson 单侧上界 U(k,n) <= ALPHA
CONF_C = 0.1  # U(k,n) 定义里的置信参数 c
MIN_N = 22

ERROR_Q = {"type": "noul", "instructions": "这段工具输出里是否含有报错、失败或异常信息？"}


def rel_q(task: str) -> dict:
    return {"type": "noul", "instructions": f"这段话的内容是否与「{task}」这个话题相关？"}


def canon(x) -> str:
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


# ---------- (d) 认证线 ----------

def _binom_cdf(k: int, n: int, p: float) -> float:
    if p <= 0:
        return 1.0
    if p >= 1:
        return 1.0 if k >= n else 0.0
    return sum(math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i)) for i in range(k + 1))


def clopper_pearson_upper(k: int, n: int) -> float:
    """U(k, n)：使 P(Bin(n, U) <= k) = CONF_C 的错误率上界；k>=n 或 n=0 时为 1。"""
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2
        if _binom_cdf(k, n, mid) > CONF_C:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2


def _valid(k: int, n: int) -> bool:
    return n >= MIN_N and clopper_pearson_upper(k, n) <= ALPHA + 1e-9


def _find_hi_cut(samples):
    """samples: [(p, correct)]，correct=True 表示选中（p>=T）判「是」是对的。
    在候选 T（观测到的 p 值，降序）里找最小的合法 T（覆盖最多样本、n 最大）。"""
    best = None
    for t in sorted({p for p, _ in samples}, reverse=True):
        sel = [c for p, c in samples if p >= t]
        n = len(sel)
        k = sum(1 for c in sel if not c)
        if _valid(k, n):
            best = t
    return best


def _find_lo_cut(samples):
    """samples: [(p, correct)]，correct=True 表示选中（p<=T）判「否」是对的。
    在候选 T（升序）里找最大的合法 T（覆盖最多样本）。"""
    best = None
    for t in sorted({p for p, _ in samples}):
        sel = [c for p, c in samples if p <= t]
        n = len(sel)
        k = sum(1 for c in sel if not c)
        if _valid(k, n):
            best = t
    return best


def compute_line_for_key(rows: list, delta: float) -> dict:
    """从一个键的标注算一条线 {"hi":..,"lo":..}，算不出时 {"status":"待真值"}。

    方法：先用固定序切一半做校准、一半做留出检验（不依赖随机数，可重复）：
    按 item 排序后奇偶分组；在校准集上用零错误/最少错误贪心找「覆盖最多、仍满足
    n>=22 且 U(k,n)<=ALPHA 的门槛」，再拿这门槛去留出集上独立核验同一条性质——
    这一步没在校准集里出现过的数据仍然满足，才说明门槛不是只拟合了校准集的噪声。
    两侧都通过留出检验后，用全部标注按同一算法重新拟合门槛（信息量更大、更稳），
    换算成 hi/lo 输出；否则该键「待真值」。
    """
    if len(rows) < MIN_N * 2:
        return {"status": "待真值"}
    items = sorted(rows, key=lambda r: r["item"])
    cal = items[0::2]
    val = items[1::2]

    cal_hi = [(r["p"], r["label"]) for r in cal]
    t_cal_hi = _find_hi_cut(cal_hi)
    hi_ok = False
    if t_cal_hi is not None:
        sel = [r for r in val if r["p"] >= t_cal_hi]
        n = len(sel)
        k = sum(1 for r in sel if not r["label"])
        hi_ok = _valid(k, n)

    cal_lo = [(r["p"], not r["label"]) for r in cal]
    t_cal_lo = _find_lo_cut(cal_lo)
    lo_ok = False
    if t_cal_lo is not None:
        sel = [r for r in val if r["p"] <= t_cal_lo]
        n = len(sel)
        k = sum(1 for r in sel if r["label"])
        lo_ok = _valid(k, n)

    if not (hi_ok and lo_ok):
        return {"status": "待真值"}

    full_hi = [(r["p"], r["label"]) for r in items]
    t_full_hi = _find_hi_cut(full_hi)
    full_lo = [(r["p"], not r["label"]) for r in items]
    t_full_lo = _find_lo_cut(full_lo)
    if t_full_hi is None or t_full_lo is None:
        return {"status": "待真值"}

    hi = t_full_hi - delta
    lo = t_full_lo + delta
    if lo > hi + 1e-9:
        return {"status": "待真值"}
    return {"hi": hi, "lo": lo}


LINE_KEYS = ["winnow-error", "form-topic", "winnow-block"]


def compute_all_lines(label_rows: list, deltas: dict) -> dict:
    by_key = {}
    for r in label_rows:
        by_key.setdefault(r["key"], []).append(r)
    out = {}
    for key in LINE_KEYS:
        out[key] = compute_line_for_key(by_key.get(key, []), deltas.get(key, 0.0))
    return out


# ---------- (a)(b)(c) 账本、重放、预算、合批 ----------

class Budget:
    def __init__(self, max_calls):
        self.remaining = max_calls

    def take(self, want: int) -> int:
        if self.remaining is None:
            return want
        n = min(want, self.remaining)
        self.remaining -= n
        return n


def load_replay_index(path: str) -> dict:
    idx = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            key = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
            idx[key] = rec["answers"]
    return idx


def write_ledger_line(fh, material, questions, over, answers):
    rec = {"material": material, "questions": questions, "over": over, "answers": answers}
    fh.write(json.dumps(rec, sort_keys=True, ensure_ascii=False) + "\n")
    fh.flush()


def fetch_answers(to_send, replay_idx, ledger_fh):
    """to_send: [(material, questions, over)]，返回同序的答案列表的列表。"""
    if replay_idx is not None:
        out = []
        for material, questions, over in to_send:
            key = (canon(material), canon(questions), canon(over))
            if key not in replay_idx:
                sys.stderr.write(
                    f"replay 缺失读数：题={questions!r} 材料={canon(material)[:200]}\n"
                )
                sys.exit(1)
            out.append(replay_idx[key])
    else:
        out = judge_batch(list(to_send))
    if ledger_fh is not None:
        for (material, questions, over), answers in zip(to_send, out):
            write_ledger_line(ledger_fh, material, questions, over, answers)
    return out


def keep_all(result: dict, reason: str, unobserved: list) -> dict:
    return {
        "id": result["id"],
        "reason": reason,
        "pruned": [],
        "uncertain": [],
        "text": list(result["chunks"]),
        "archive": [],
        "unobserved": unobserved,
    }


def process_layer(result, task, lines, deltas, budget, replay_idx, ledger_fh):
    chunks = result["chunks"]
    joined = "\n".join(chunks)
    requests = [(joined, [ERROR_Q], None)]
    q_rel = rel_q(task)
    for c in chunks:
        requests.append((c, [q_rel], None))

    send_n = budget.take(len(requests))
    to_send = requests[:send_n]
    answers = fetch_answers(to_send, replay_idx, ledger_fh) if to_send else []

    if send_n == 0:
        return {
            "id": result["id"],
            "reason": "error_unobserved",
            "pruned": [],
            "uncertain": [],
            "text": list(chunks),
            "archive": [],
            "unobserved": list(range(len(chunks))),
        }

    observed_chunks = send_n - 1
    unobserved = list(range(observed_chunks, len(chunks)))

    p_err = answers[0][0]["noul"]
    hi_e, lo_e = lines["winnow-error"]["hi"], lines["winnow-error"]["lo"]
    delta_e = deltas["winnow-error"]
    if p_err >= hi_e + delta_e:
        return keep_all(result, "error_present", unobserved)
    if p_err <= lo_e - delta_e:
        err_no = True
    else:
        return keep_all(result, "error_unsure", unobserved)

    hi_t, lo_t = lines["form-topic"]["hi"], lines["form-topic"]["lo"]
    delta_t = deltas["form-topic"]
    candidate, uncertain = [], []
    for i in range(observed_chunks):
        p = answers[i + 1][0]["noul"]
        if p >= hi_t + delta_t:
            continue
        elif p <= lo_t - delta_t:
            candidate.append(i)
        else:
            uncertain.append(i)

    total_chars = sum(len(c) for c in chunks)
    candidate_chars = sum(len(chunks[i]) for i in candidate)
    if candidate_chars * 100 < total_chars * 20:
        return keep_all(result, "below_min_prune_ratio", unobserved)

    text, archive = [], []
    pruned_set = set(candidate)
    for i, c in enumerate(chunks):
        if i in pruned_set:
            h = hashlib.sha256(c.encode("utf-8")).hexdigest()
            text.append(f"[winnow 已折叠 {len(c)} 字 · 展开键 {h}]")
            archive.append({"key": h, "index": i, "text": c})
        else:
            text.append(c)

    return {
        "id": result["id"],
        "reason": "pruned",
        "pruned": sorted(candidate),
        "uncertain": sorted(uncertain),
        "text": text,
        "archive": archive,
        "unobserved": unobserved,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger-out")
    ap.add_argument("--replay")
    ap.add_argument("--max-calls", type=int, default=None)
    ap.add_argument("--print-lines", action="store_true")
    args = ap.parse_args()

    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)
    with open("labels.jsonl", encoding="utf-8") as fh:
        label_rows = [json.loads(line) for line in fh if line.strip()]

    deltas = materials["deltas"]
    lines = compute_all_lines(label_rows, deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    bad_keys = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if bad_keys:
        print(json.dumps({"status": "待真值", "key": bad_keys[0]}, ensure_ascii=False))
        return

    replay_idx = load_replay_index(args.replay) if args.replay else None
    ledger_fh = open(args.ledger_out, "w", encoding="utf-8") if args.ledger_out else None

    budget = Budget(args.max_calls)
    task = materials["tool_outputs"]["task"]
    results_out = []
    try:
        for result in materials["tool_outputs"]["results"]:
            results_out.append(
                process_layer(result, task, lines, deltas, budget, replay_idx, ledger_fh)
            )
    finally:
        if ledger_fh is not None:
            ledger_fh.close()

    print(json.dumps({"task": task, "results": results_out}, ensure_ascii=False))


if __name__ == "__main__":
    main()
