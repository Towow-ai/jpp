"""trial-refund · T1（完整职责版）。

先判断每段客服对话是否要退款（第 1 层），只对要退款的对话再判断情绪是否激烈（第 2 层）。
在此之上加四项职责：账本与重放、预算停机、合批与同层一起发、从标注算认证线（见下方 compute_lines）。
"""
import argparse
import json
import math
import os
import sys

from jev_interface import judge_batch

REFUND_Q = {"type": "noul", "instructions": "这段客服对话里，顾客是否明确提出要退款或退钱？"}
HEATED_Q = {"type": "noul", "instructions": "这段客服对话里，顾客的措辞是否情绪激烈（例如愤怒、威胁投诉或曝光、连续质问）？"}

ALPHA = 0.1
CONF_C = 0.1
MIN_N = 22
_MISSING = object()


def canon(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


# ---------- (d) 认证线：从 labels.jsonl 算 hi/lo ----------
#
# 方法（是非题，本项目 labels.jsonl 只有这种题型，没有 op 字段，等同 op="test"）：
# 把某个键的标注按 p 排序（「是」侧降序、「否」侧升序），从最靠外的一端开始，逐个纳入。
# 对每个可能的纳入前缀（长度 n>=22），统计其中的错误数 k（「是」侧错误=真值为否，「否」侧错误=真值为是），
# 用 Clopper–Pearson 上界 U(k,n)（对 U 做二分求根，使二项分布 CDF 在 U 处等于置信参数 c）判断
# U(k,n) <= alpha 是否成立；在所有满足条件的前缀里取 n 最大的一个（数据越多、边界越稳）。
# 该前缀最靠里一侧的 p 值定为边界：「是」侧 hi = 边界 p - delta，「否」侧 lo = 边界 p + delta，
# 使得按「p >= hi+delta 判是 / p <= lo-delta 判否」的规则重建出恰好这个前缀集合。
# 用 Clopper–Pearson 上界而不是简单的样本错误率，是因为它本身就把「这批标注只是一次抽样」的
# 不确定性算进去了，所以不再需要额外切一份验证集——只要标注是同分布抽样，这个上界对新一批
# 标注的真实错误率仍然成立。若某一侧找不到满足条件的前缀，该键判「待真值」。
def _binom_cdf(k, n, p):
    if p <= 0.0:
        return 1.0
    if p >= 1.0:
        return 1.0 if k >= n else 0.0
    total = 0.0
    for i in range(0, k + 1):
        total += math.comb(n, i) * (p ** i) * ((1.0 - p) ** (n - i))
    return total


def _cp_upper(k, n, c=CONF_C):
    if n == 0 or k >= n:
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(100):
        mid = (lo + hi) / 2.0
        if _binom_cdf(k, n, mid) > c:
            lo = mid
        else:
            hi = mid
    return hi


def _best_prefix(pairs, alpha=ALPHA, min_n=MIN_N):
    """pairs: 按纳入顺序排好的 [(p, is_error), ...]。返回 (n, k, 边界p) 里 n 最大且满足
    U(k,n)<=alpha 的一个；找不到则 None。
    只在「下一行的 p 与当前不同」处取边界（同一 p 值的所有行必须整批纳入或整批不纳入），
    否则判断时按 p>=hi+delta 会把同一读数的行切成两半，边界就对不上取样时的前缀。"""
    best = None
    k = 0
    n_total = len(pairs)
    for i, (p, is_err) in enumerate(pairs):
        if is_err:
            k += 1
        n = i + 1
        if n < n_total and pairs[n][0] == p:
            continue  # 还在同一批相同 p 值里，不能在这里切
        if n >= min_n:
            u = _cp_upper(k, n)
            if u <= alpha + 1e-9:
                if best is None or n > best[0]:
                    best = (n, k, p)
    return best


def compute_lines(deltas):
    labels_by_key = {}
    with open("labels.jsonl", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            labels_by_key.setdefault(rec["key"], []).append(rec)

    lines = {}
    for key, recs in labels_by_key.items():
        samples = [(r["p"], bool(r["label"])) for r in recs if r.get("op") in (None, "test")]
        hi_pairs = sorted(samples, key=lambda x: x[0], reverse=True)
        hi_pairs = [(p, not label) for p, label in hi_pairs]
        lo_pairs = sorted(samples, key=lambda x: x[0])
        lo_pairs = [(p, label) for p, label in lo_pairs]

        hi_res = _best_prefix(hi_pairs)
        lo_res = _best_prefix(lo_pairs)
        if hi_res is None or lo_res is None:
            lines[key] = {"status": "待真值"}
            continue

        delta = deltas[key]
        hi = hi_res[2] - delta
        lo = lo_res[2] + delta
        if lo > hi + 1e-9:
            lines[key] = {"status": "待真值"}
            continue
        lines[key] = {"hi": hi, "lo": lo}
    return lines


# ---------- 判断：阈值规则（是非题） ----------
def decide(p, hi, lo, delta):
    if p >= hi + delta:
        return True
    if p <= lo - delta:
        return False
    return None


# ---------- (a) 账本与重放 ----------
class Judger:
    def __init__(self, ledger_out, replay_path):
        self.ledger_out = ledger_out
        self.replay_index = None
        if replay_path:
            self.replay_index = {}
            with open(replay_path, encoding="utf-8") as fh:
                for line in fh:
                    line = line.strip()
                    if not line:
                        continue
                    rec = json.loads(line)
                    key = (canon(rec["material"]), canon(rec["questions"]), canon(rec.get("over")))
                    self.replay_index[key] = rec["answers"]

    def _record(self, material, questions, over, answers):
        if not self.ledger_out:
            return
        line = json.dumps(
            {"material": material, "questions": questions, "over": over, "answers": answers},
            sort_keys=True, ensure_ascii=False,
        )
        with open(self.ledger_out, "a", encoding="utf-8") as fh:
            fh.write(line + "\n")

    def _replay_one(self, material, questions, over):
        key = (canon(material), canon(questions), canon(over))
        if key not in self.replay_index:
            qtext = "、".join(q.get("instructions", "?") for q in questions)
            sys.stderr.write(
                "replay 找不到读数：题=%r 材料=%s\n" % (qtext, canon(material)[:160])
            )
            sys.exit(1)
        return self.replay_index[key]

    def ask_batch(self, requests):
        """requests: [(material, questions, over), ...]。返回与 requests 同序的答案列表。"""
        if not requests:
            return []
        if self.replay_index is not None:
            return [self._replay_one(m, q, o) for (m, q, o) in requests]
        results = judge_batch(requests)
        for (m, q, o), ans in zip(requests, results):
            self._record(m, q, o, ans)
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
    deltas = materials["deltas"]

    lines = compute_lines(deltas)

    if args.print_lines:
        print(json.dumps(lines, ensure_ascii=False))
        return

    pending = [k for k in sorted(lines) if lines[k].get("status") == "待真值"]
    if pending:
        print(json.dumps({"status": "待真值", "key": pending[0]}, ensure_ascii=False))
        return

    chats = materials["chats"]
    hi_refund, lo_refund = lines["cs-refund"]["hi"], lines["cs-refund"]["lo"]
    hi_heated, lo_heated = lines["cs-heated"]["hi"], lines["cs-heated"]["lo"]
    d_refund = deltas["cs-refund"]
    d_heated = deltas["cs-heated"]

    judger = Judger(args.ledger_out, args.replay)

    max_calls = args.max_calls
    used = 0

    def remaining():
        if max_calls is None:
            return None
        return max(0, max_calls - used)

    unobserved = set()

    # 第 1 层：全部对话，按编号顺序，一起发。
    order = list(range(len(chats)))
    r = remaining()
    call1 = order if r is None else order[:r]
    for i in order:
        if i not in call1:
            unobserved.add(i)

    refund_decision = {}
    if call1:
        requests = [(chats[i], [REFUND_Q], None) for i in call1]
        results = judger.ask_batch(requests)
        used += len(call1)
        for i, ans in zip(call1, results):
            refund_decision[i] = decide(ans[0]["noul"], hi_refund, lo_refund, d_refund)

    # 第 2 层：退款题为「是」的对话，按编号顺序，一起发。
    eligible = [i for i in order if refund_decision.get(i, _MISSING) is True]
    r = remaining()
    call2 = eligible if r is None else eligible[:r]
    for i in eligible:
        if i not in call2:
            unobserved.add(i)

    heated_decision = {}
    if call2:
        requests = [(chats[i], [HEATED_Q], None) for i in call2]
        results = judger.ask_batch(requests)
        used += len(call2)
        for i, ans in zip(call2, results):
            heated_decision[i] = decide(ans[0]["noul"], hi_heated, lo_heated, d_heated)

    urgent, refund_calm, no_refund, review = [], [], [], []
    for i in order:
        rd = refund_decision.get(i, _MISSING)
        if rd is _MISSING:
            continue  # 第 1 层没问到，不计入任何列表
        if rd is None:
            review.append(i)
        elif rd is False:
            no_refund.append(i)
        else:
            hd = heated_decision.get(i, _MISSING)
            if hd is _MISSING:
                continue  # 第 2 层没问到，不计入任何列表
            if hd is None:
                review.append(i)
            elif hd is True:
                urgent.append(i)
            else:
                refund_calm.append(i)

    unobserved_list = sorted(unobserved)
    lower = len(urgent)
    upper = lower + len(review) + len(unobserved_list)

    result = {
        "unobserved": unobserved_list,
        "urgent": urgent,
        "refund_calm": refund_calm,
        "no_refund": no_refund,
        "review": review,
        "urgent_count": [lower, upper],
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
