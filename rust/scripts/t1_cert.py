"""T1 (d) 的考官侧：留出集上的性质检验与标注自检（步 31-1b，B96；不计行，基线程序不得导入）。

依据：`地基/评估/2026-09-24-仪表读数4诊断与裁定.md` §一·2 第 2 条、B96。

性质（任务书 T1 (d) 的文字，本模块是它的可执行形式）：程序为每个键给出 `{hi, lo}`（K 选一与打分只有 hi，lo 视为 0）；
在程序看不见的留出集 `labels-cert.jsonl` 上，
  上侧：已决行 = `p >= hi + δ`，错误 = 其中真值为否（K 元：argmax 不等于真值）；
  下侧（只有是非题）：已决行 = `p <= lo - δ`，错误 = 其中真值为是；
每一侧 n >= NEED（22）且 Clopper–Pearson 单侧上界 U(k, n) <= α（α = 0.1，置信参数 0.1）；是非题另要求 lo <= hi。
比较留 1e-9 的容差（程序以 hi = h - δ 输出时浮点往返可能差一个末位）。

自检（`make_labels_t1.py` 生成标注后调用）：在留出集上枚举全部阈值等价类，确认每个通过检验的阈值
在固定观察的每条读数上给出同一出口、并与 T0 手写阈值的出口相同——于是「任何过 ① 的线」都让功能期望不变（验收 ③）。
"""
from __future__ import annotations

import json

from certify_ref import binomial_upper, n_needed

ALPHA = 0.1
CONF = 0.1
NEED = n_needed(ALPHA, CONF)   # 22
EPS = 1e-9
MARGIN = 1e-4                  # 自检：读数离通过区间端点至少这么远，防浮点往返翻转


def load_rows(path) -> list:
    return [json.loads(x) for x in open(path, encoding="utf-8") if x.strip()]


def samples(rows) -> dict:
    """{键: (op, [(p, 是否正确)])}。是非题「正确」即真值为是；K 元为 pick == label。"""
    by = {}
    for r in rows:
        op = r.get("op", "test")
        ok = r["label"] if op == "test" else r["pick"] == r["label"]
        by.setdefault(r["key"], (op, []))[1].append((r["p"], bool(ok)))
    return by


def side_ok(decided: list, want: bool) -> tuple:
    """decided：[(p, 真值位)]；want：该侧的正确真值位（上侧 True、下侧 False）。返回 (通过, n, k, U)。"""
    n = len(decided)
    k = sum(1 for _, t in decided if t != want)
    u = binomial_upper(k, n, CONF) if n else 1.0
    return n >= NEED and u <= ALPHA, n, k, u


def check_key(op: str, smp: list, delta: float, line: dict) -> tuple:
    """一个键的性质检验。line：{hi, lo}。返回 (通过, 说明)。"""
    if not isinstance(line, dict) or not isinstance(line.get("hi"), (int, float)):
        return False, "没有线"
    hi = float(line["hi"])
    ok_u, n_u, k_u, u_u = side_ok([s for s in smp if s[0] >= hi + delta - EPS], True)
    msg = f"上侧 n={n_u} k={k_u} U={u_u:.3f}"
    if op != "test":
        return ok_u, msg
    if not isinstance(line.get("lo"), (int, float)):
        return False, msg + "；没有 lo"
    lo = float(line["lo"])
    ok_d, n_d, k_d, u_d = side_ok([s for s in smp if s[0] <= lo - delta + EPS], False)
    msg += f"；下侧 n={n_d} k={k_d} U={u_d:.3f}"
    if lo > hi + EPS:
        return False, msg + f"；lo {lo} > hi {hi}"
    return ok_u and ok_d, msg


def cert_check(lines: dict, cert_rows: list, deltas: dict) -> dict:
    """lines：{键: {hi, lo}}（程序 --print-lines 的输出或 calib-import 的线）。返回 {键: (通过, 说明)}。"""
    out = {}
    for key, (op, smp) in sorted(samples(cert_rows).items()):
        out[key] = check_key(op, smp, deltas[key], lines.get(key))
    return out


# —— 自检 ——

def passing_intervals(op: str, smp: list, side: str) -> list:
    """留出集上通过检验的阈值等价类。上侧：阈值 h 取 (a, b] 时已决集相同；下侧：l 取 [a, b)。
    返回 [(a, b)]（开闭按侧），用于判断读数是否可能被不同的通过阈值分到两边。"""
    ps = sorted({p for p, _ in smp})
    out = []
    if side == "up":
        prev = -1.0
        for c in ps:
            ok, *_ = side_ok([s for s in smp if s[0] >= c], True)
            if ok:
                out.append((prev, c))
            prev = c
    else:
        for i, c in enumerate(ps):
            nxt = ps[i + 1] if i + 1 < len(ps) else 2.0
            ok, *_ = side_ok([s for s in smp if s[0] <= c], False)
            if ok:
                out.append((c, nxt))
    return out


def self_check(cert_rows: list, readings: dict) -> list:
    """readings：{键: [(p, T0 出口)]}，出口取 "act"、"ignore"、"unsure"（K 元只分 act / 其他）。
    返回问题列表（空 = 自检通过）。这里的阈值 h 是用线时的比较点（hi + δ），与 δ 无关。"""
    bad = []
    for key, (op, smp) in sorted(samples(cert_rows).items()):
        sides = [("up", "act")] + ([("down", "ignore")] if op == "test" else [])
        for side, exit_name in sides:
            iv = passing_intervals(op, smp, side)
            if not iv:
                bad.append(f"{key} {side}：留出集上没有任何通过的阈值")
                continue
            for p, ex in readings.get(key, []):
                want = ex == exit_name
                for a, b in iv:
                    if side == "up":   # h ∈ (a, b]：p >= h 对全部 h 成立当 p >= b，都不成立当 p <= a
                        got = True if p >= b + MARGIN else (False if p <= a - MARGIN else None)
                    else:              # l ∈ [a, b)：p <= l 对全部 l 成立当 p <= a，都不成立当 p >= b
                        got = True if p <= a - MARGIN else (False if p >= b + MARGIN else None)
                    if got is None or got != want:
                        bad.append(f"{key} {side}：读数 {p}（T0 {ex}）在通过区间 ({a}, {b}) 上"
                                   + ("可两分" if got is None else "出口与 T0 不同"))
                        break
    return bad


def band(iv: list) -> tuple:
    return (min(a for a, _ in iv), max(b for _, b in iv)) if iv else None


if __name__ == "__main__":
    import sys
    rows = load_rows(sys.argv[1])
    for key, (op, smp) in sorted(samples(rows).items()):
        print(key, op, "up", band(passing_intervals(op, smp, "up")),
              "down", band(passing_intervals(op, smp, "down")) if op == "test" else "—")
