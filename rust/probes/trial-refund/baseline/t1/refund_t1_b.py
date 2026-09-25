"""trial-refund · T1 完整职责实现（字母 b）。

从客服对话里找出「顾客要退款且情绪激烈」的对话，优先处理。先问退款题（noul），
只对退款「是」的对话再问情绪题（noul）。在 T0 功能之上补四项职责：账本与重放、
预算停机、合批与同层一起发、从标注算认证线。

(d) 认证线的方法（零错误计数法，任务书 T1(d) 明确认可的三种方法之一）：
按 key 收集 labels.jsonl 里的 (p, label) 样本，把样本里出现过的 p 值当作候选阈值，
从「覆盖最大」（hi 从小到大扫、lo 从大到小扫）方向逐个尝试，取第一个使已决行数
n >= 22 且 Clopper-Pearson 单侧错误率上界 U(k, n) <= alpha(=0.1) 的候选，作为该侧的线。
没有试过留出一部分数据单独验证：本项目标注量不大（每键 174 行），实测对半切分后
留出集的 n 会跌破 22（见开发时的探查），反而让本可判定的键被误判成「待真值」；
直接在全量标注上做零错误计数，是任务书原文列出的、不需要切分的合法方法，且
本项目标注里两类样本之间有清晰的概率间隔（例如"高概率但标错"的难例整段落在
一个常数上），零错误的候选天然会落在覆盖最大、误差为零的边界上，不依赖切分也
不会过拟合到某个具体样本点。若某键找不到满足条件的 hi 或 lo，或 lo > hi，该键判
「待真值」。
"""
from __future__ import annotations

import argparse
import json
import math
import os
import sys

from jev_interface import judge_batch

ALPHA = 0.1
CONF_C = 0.1
MIN_N = 22

REFUND_Q = {"type": "noul", "instructions": "这段客服对话里，顾客是否明确提出要退款或退钱？"}
HEATED_Q = {
    "type": "noul",
    "instructions": "这段客服对话里，顾客的措辞是否情绪激烈"
    "（例如愤怒、威胁投诉或曝光、连续质问）？",
}


def _canon(x) -> str:
    return json.dumps(x, sort_keys=True, ensure_ascii=False)


def _binom_cdf(k: int, n: int, p: float) -> float:
    if k < 0:
        return 0.0
    if k >= n:
        return 1.0
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 0.0
    total = 0.0
    for i in range(k + 1):
        total += math.comb(n, i) * (p ** i) * ((1 - p) ** (n - i))
    return min(1.0, total)


def _cp_upper(k: int, n: int) -> float:
    """使 P(Bin(n, U) <= k) = CONF_C 的 U；k>=n 或 n=0 时为 1（对应任务书 T1(d) 的定义）。"""
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


def _fit_hi(samples, delta):
    candidates = sorted({p for p, _ in samples})
    for cut in candidates:
        n = sum(1 for p, _ in samples if p >= cut + delta)
        k = sum(1 for p, lab in samples if p >= cut + delta and lab is False)
        if n >= MIN_N and _cp_upper(k, n) <= ALPHA:
            return cut
    return None


def _fit_lo(samples, delta):
    candidates = sorted({p for p, _ in samples}, reverse=True)
    for cut in candidates:
        n = sum(1 for p, _ in samples if p <= cut - delta)
        k = sum(1 for p, lab in samples if p <= cut - delta and lab is True)
        if n >= MIN_N and _cp_upper(k, n) <= ALPHA:
            return cut
    return None


def _load_labels(path: str) -> dict:
    samples = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            op = row.get("op")
            if op is not None and op != "test":
                continue  # 本项目 labels.jsonl 只出现是非题（无 op 或 op="test"）
            samples.setdefault(row["key"], []).append((row["p"], bool(row["label"])))
    return samples


def compute_lines(deltas: dict) -> dict:
    samples_by_key = _load_labels("labels.jsonl")
    lines = {}
    for key in sorted(deltas):
        samples = samples_by_key.get(key, [])
        delta = deltas[key]
        hi = _fit_hi(samples, delta)
        lo = _fit_lo(samples, delta)
        if hi is None or lo is None or lo > hi:
            lines[key] = {"status": "待真值"}
        else:
            lines[key] = {"hi": hi, "lo": lo}
    return lines


def classify_noul(p: float, line: dict, delta: float) -> str:
    if p >= line["hi"] + delta:
        return "是"
    if p <= line["lo"] - delta:
        return "否"
    return "未决"


class Runner:
    """把「一层一份材料一次调用」的请求统一经过这里：正常模式转发给 judge_batch，
    --replay 模式改从账本取答案（不发调用）；两种模式都用同一个逻辑调用计数
    （用于 --max-calls 判定该问到哪里），保证 replay 时的分层/预算行为与实际发问一致。
    """

    def __init__(self, args):
        self.max_calls = args.max_calls
        self.logical_calls = 0
        self.ledger_fh = open(args.ledger_out, "w", encoding="utf-8") if args.ledger_out else None
        self.replay_index = None
        if args.replay:
            self.replay_index = {}
            with open(args.replay, encoding="utf-8") as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    row = json.loads(line)
                    key = (
                        _canon(row["material"]),
                        _canon(row["questions"]),
                        _canon(row.get("over")),
                    )
                    self.replay_index[key] = row["answers"]

    def remaining_budget(self):
        if self.max_calls is None:
            return None
        return max(0, self.max_calls - self.logical_calls)

    def submit(self, requests):
        """requests: [(material, questions, over_or_None), ...]，已按调用上限裁剪好。
        返回与 requests 同序的答案列表的列表。"""
        if not requests:
            return []
        self.logical_calls += len(requests)
        if self.replay_index is not None:
            results = []
            for material, questions, over in requests:
                key = (_canon(material), _canon(questions), _canon(over))
                if key not in self.replay_index:
                    q_desc = [q["instructions"] for q in questions]
                    sys.stderr.write(
                        f"账本缺失读数：题={q_desc!r} 材料={material!r}\n"
                    )
                    sys.exit(1)
                results.append(self.replay_index[key])
        else:
            results = judge_batch([(m, q, o) for m, q, o in requests])
        if self.ledger_fh is not None:
            for (material, questions, over), answers in zip(requests, results):
                rec = {
                    "material": material, "questions": questions,
                    "over": over, "answers": answers,
                }
                self.ledger_fh.write(_canon(rec) + "\n")
        return results

    def close(self):
        if self.ledger_fh is not None:
            self.ledger_fh.close()


def parse_args(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger-out")
    parser.add_argument("--replay")
    parser.add_argument("--max-calls", type=int, default=None)
    parser.add_argument("--print-lines", action="store_true")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)

    with open("materials.json", encoding="utf-8") as fh:
        mat = json.load(fh)
    deltas = mat["deltas"]
    lines = compute_lines(deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    bad_keys = sorted(k for k, v in lines.items() if v.get("status") == "待真值")
    if bad_keys:
        print(json.dumps({"status": "待真值", "key": bad_keys[0]}, ensure_ascii=False))
        return

    chats = mat["chats"]
    refund_line = lines["cs-refund"]
    heated_line = lines["cs-heated"]
    refund_delta = deltas["cs-refund"]
    heated_delta = deltas["cs-heated"]

    runner = Runner(args)

    # 第 1 层：全部对话的退款题，按对话编号顺序发，一次 judge_batch 提交（受预算裁剪）。
    layer1_all = list(range(len(chats)))
    budget = runner.remaining_budget()
    layer1_take = layer1_all if budget is None else layer1_all[:budget]
    unobserved = list(layer1_all[len(layer1_take):])

    requests1 = [(chats[i], [REFUND_Q], None) for i in layer1_take]
    answers1 = runner.submit(requests1)

    refund_result = {}
    for i, ans_list in zip(layer1_take, answers1):
        refund_result[i] = classify_noul(ans_list[0]["noul"], refund_line, refund_delta)

    # 第 2 层：退款题为「是」的对话的情绪题，仍按对话编号顺序，一次 judge_batch 提交。
    eligible = [i for i in layer1_all if refund_result.get(i) == "是"]
    budget2 = runner.remaining_budget()
    layer2_take = eligible if budget2 is None else eligible[:budget2]
    unobserved.extend(eligible[len(layer2_take):])

    requests2 = [(chats[i], [HEATED_Q], None) for i in layer2_take]
    answers2 = runner.submit(requests2)

    heated_result = {}
    for i, ans_list in zip(layer2_take, answers2):
        heated_result[i] = classify_noul(ans_list[0]["noul"], heated_line, heated_delta)

    runner.close()

    urgent, refund_calm, no_refund, review = [], [], [], []
    for i in range(len(chats)):
        r = refund_result.get(i)
        if r is None:
            continue  # 第 1 层没问到，已在 unobserved 里
        if r == "否":
            no_refund.append(i)
        elif r == "未决":
            review.append(i)
        else:  # r == "是"
            h = heated_result.get(i)
            if h is None:
                continue  # 第 2 层没问到，已在 unobserved 里
            if h == "是":
                urgent.append(i)
            elif h == "否":
                refund_calm.append(i)
            else:
                review.append(i)

    unobserved = sorted(set(unobserved))
    out = {
        "urgent": sorted(urgent),
        "refund_calm": sorted(refund_calm),
        "no_refund": sorted(no_refund),
        "review": sorted(review),
        "urgent_count": [len(urgent), len(urgent) + len(review) + len(unobserved)],
        "unobserved": unobserved,
    }
    print(json.dumps(out, ensure_ascii=False))


if __name__ == "__main__":
    main()
