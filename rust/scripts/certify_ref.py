"""认证线参考实现（T1 任务书 (d) 的验收用，不计行；放在 scripts/ 而不在 probes/_baseline/，基线程序不得导入它）。

用法：
    python3 certify_ref.py labels.jsonl            打印 {key: {"hi", "lo", "delta"} 或 {"status": "待真值", ...}}

算法按 `jpp calib-import` 的拆分样本认证逐行移植（`crates/jpp-calib/src/calib/commission.rs`
的 `commission_two_sided_split_graded`、`commission_upper_split_graded`（步 20a-2b 起为 crate 内部，测试经
`commission_legacy_seed_split_test_only` 调）、`选两侧线对`、`splitmix64`，
`crates/jpp-value/src/stat.rs` 的 `binomial_upper`、`n_needed_zero_error`），
使手写基线算出的线与 J++ 侧经 `calib-import` 得到的线逐位相同。任务书 T1 (d) 用文字写了同一算法。

默认参数与 `jpp calib-import` 相同：alpha 0.1、conf_delta 0.1、seed 20260923；
δ 取不加载画像时的代码兜底：是非题 0.05，K 选一与打分 0.15。
"""
from __future__ import annotations

import json
import math
import sys

ALPHA = 0.1
CONF_DELTA = 0.1
SEED = 20260923
DELTA = {"test": 0.05, "select": 0.15, "measure": 0.15}
M64 = (1 << 64) - 1


def splitmix64(x: int) -> int:
    z = (x + 0x9E3779B97F4A7C15) & M64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
    return z ^ (z >> 31)


def binomial_upper(k: int, n: int, conf_delta: float) -> float:
    """Clopper–Pearson 单侧上界，二分 200 次（与 Rust 同一浮点运算顺序）。"""
    if n == 0 or k >= n:
        return 1.0
    log_choose = 0.0
    for i in range(1, k + 1):
        log_choose += math.log((n - i + 1) / i)
    lo, hi = k / n, 1.0
    for _ in range(200):
        mid = (lo + hi) / 2.0
        term = math.exp(log_choose + k * math.log(mid) + (n - k) * math.log1p(-mid))
        cdf = term
        for i in range(k, 0, -1):
            term *= i / (n - i + 1) * (1.0 - mid) / mid
            cdf += term
        if cdf > conf_delta:
            lo = mid
        else:
            hi = mid
    return (lo + hi) / 2.0


def n_needed(alpha: float, conf_delta: float) -> int:
    return math.ceil(math.log(conf_delta) / math.log(1.0 - alpha))


def split(samples, seed):
    canon = sorted(samples, key=lambda x: (x[0], x[1]))
    sel, cert = [], []
    for i, x in enumerate(canon):
        (sel if splitmix64(seed ^ i) & 1 == 0 else cert).append(x)
    return sel, cert


def candidates(ps):
    ps = sorted(set(ps))
    c = set(ps)
    for a, b in zip(ps, ps[1:]):
        c.add((a + b) / 2.0)
    return sorted(c)


def two_sided(samples, delta, alpha=ALPHA, conf_delta=CONF_DELTA, seed=SEED):
    """samples: [(p, 真值是否为真)]。返回 {"hi", "lo"} 或 {"status": "待真值", "why"}。"""
    need = n_needed(alpha, conf_delta)
    sel, cert = split(samples, seed)
    pos, neg = sum(1 for x in sel if x[1]), sum(1 for x in sel if not x[1])
    if min(pos, neg) < need:
        return {"status": "待真值", "why": f"选线半样本不足（正例 {pos}、负例 {neg}）"}
    cands = candidates([x[0] for x in sel])
    up, down = [], []
    for h in cands:
        acc = [x for x in sel if x[0] >= h]
        if acc and h >= delta:
            k = sum(1 for x in acc if not x[1])
            if binomial_upper(k, len(acc), conf_delta) <= alpha:
                up.append((h, len(acc)))
    for l in cands:
        acc = [x for x in sel if x[0] <= l]
        if acc and l <= 1.0 - delta:
            k = sum(1 for x in acc if x[1])
            if binomial_upper(k, len(acc), conf_delta) <= alpha:
                down.append((l, len(acc)))
    best = None
    for a in up:
        for d in down:
            if d[0] + delta > a[0] - delta:
                continue
            if best is None or a[1] + d[1] > best[2]:
                best = (a[0], d[0], a[1] + d[1])
    if best is None:
        return {"status": "待真值", "why": "选线半上没有满足 α 的线对"}
    h, l = best[0], best[1]
    u = [x for x in cert if x[0] >= h]
    dn = [x for x in cert if x[0] <= l]
    if len(u) < need or len(dn) < need:
        return {"status": "待真值", "why": f"认证半样本不足（上侧 {len(u)}、下侧 {len(dn)}）"}
    ka = sum(1 for x in u if not x[1])
    kd = sum(1 for x in dn if x[1])
    if binomial_upper(ka, len(u), conf_delta) > alpha or binomial_upper(kd, len(dn), conf_delta) > alpha:
        return {"status": "待真值", "why": "认证半上界超过 α"}
    return {"hi": min(max(h - delta, 0.0), 1.0), "lo": min(max(l + delta, 0.0), 1.0)}


def upper(samples, delta, alpha=ALPHA, conf_delta=CONF_DELTA, seed=SEED):
    """K 选一 / 打分：samples [(p_max, argmax 是否等于真值)]，只认上侧。"""
    need = n_needed(alpha, conf_delta)
    sel, cert = split(samples, seed)
    best = None
    for h in candidates([x[0] for x in sel]):
        if h < delta:
            continue
        acc = [x for x in sel if x[0] >= h]
        if len(acc) < need:
            continue
        k = sum(1 for x in acc if not x[1])
        if binomial_upper(k, len(acc), conf_delta) <= alpha:
            if best is None or len(acc) > best[1]:
                best = (h, len(acc))
    if best is None:
        return {"status": "待真值", "why": "选线半上没有满足 α 的单侧线"}
    h = best[0]
    u = [x for x in cert if x[0] >= h]
    if len(u) < need:
        return {"status": "待真值", "why": f"认证半样本不足（已决 {len(u)}）"}
    if binomial_upper(sum(1 for x in u if not x[1]), len(u), conf_delta) > alpha:
        return {"status": "待真值", "why": "认证半上界超过 α"}
    return {"hi": min(max(h - delta, 0.0), 1.0), "lo": 0.0}


def certify(rows):
    """rows：labels.jsonl 的行。按键分组；是非题用 label（true/false），K 选一与打分用 pick == label。"""
    by = {}
    for r in rows:
        op = r.get("op", "test")
        if op == "test":
            if not isinstance(r["label"], bool):
                continue
            s = (r["p"], r["label"])
        else:
            if isinstance(r["label"], bool) or not isinstance(r["label"], int):
                continue
            s = (r["p"], r["pick"] == r["label"])
        by.setdefault((r["key"], op), []).append(s)
    out = {}
    for (key, op), samples in sorted(by.items()):
        d = DELTA[op]
        res = two_sided(samples, d) if op == "test" else upper(samples, d)
        res["delta"] = d
        res["op"] = op
        out[key] = res
    return out


def main():
    rows = [json.loads(x) for x in open(sys.argv[1], encoding="utf-8") if x.strip()]
    print(json.dumps(certify(rows), ensure_ascii=False, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
