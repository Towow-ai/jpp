"""把 Python 内核的 `allocate` / `unsure_bound` 跑出来，存成 Rust 侧的对照基准。

Rust 这两个构件是**有对照的移植**，不是重新设计：Python 侧（`foundation/jv/runtime.py:1236` 的
`allocate`、`:1246` 的 `unsure_bound`）就是 oracle。这个脚本走真实的 `jv.judge` 路径拿读数，
再调这两个函数，把输入与输出一起写进 `oracle.json`；Rust 的 `tests/allocate.rs` 断言自己给出同一批
下标与同一组界。

跑法（要 3.10+，本机用 3.12）：
    cd ~/个人项目/jev/地基 && python3.12 rust-jpp/crates/jpp-core/tests/oracle/generate.py

改了 Python 侧语义就重跑它，Rust 测试会跟着红。不要手改 `oracle.json` 里的数字。
"""

from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__))))))))

import foundation.jv as jv  # noqa: E402


class Rule:
    """按状态的 `on` 文本给固定的 p：不是模型，缺记录即错（与 Rust 的 FixedClient 同纪律）。"""

    def __init__(self, table: dict, phys: str = "noul", scale=()):
        self.table, self.phys, self.scale = table, phys, scale

    def __call__(self, text, qid, q):
        st = json.loads(text) if text.startswith("{") else {}
        on = str(st.get("on", ""))
        p = self.table[on]
        if q["type"] == "noul":
            return {"type": "noul", "noul": p}
        if q["type"] == "score":
            # 第 0 档拿 p，其余平分。**选中的那一档取概率最大的那个**——模型报一个不是最大档的档位
            # 是自相矛盾的数据，而 Rust 侧的 Answer::Score 只存概率向量、选档由 argmax 定，
            # 所以这里让两边用同一个口径，否则比的是数据不一致而不是实现不一致。
            rest = (1.0 - p) / max(len(self.scale) - 1, 1)
            probs = [p if i == 0 else rest for i in range(len(self.scale))]
            return {"type": "score", "score": probs.index(max(probs)),
                    "probabilities": {str(i): v for i, v in enumerate(probs)}}
        raise AssertionError(q["type"])


PROFILE = {}


def collect(name, ps, k, *, status="上岗", hi=0.65, lo=0.35, unsure_rate=0.2, delta=None, phys="noul", scale=()):
    """跑一次，返回一条对照基准。`ps` 是每个状态的 p，按下标顺序。"""
    table = {f"s{i}": p for i, p in enumerate(ps)}
    rt = jv.Runtime(client=jv.FakeClient(rule=Rule(table, phys, scale)))
    with rt:
        kw = dict(hi=hi, lo=lo, n=100, status=status, set_id="conf")
        if unsure_rate is not None:
            kw["unsure_rate"] = unsure_rate
        if delta is not None:
            kw["delta"] = delta
        rt.calib.put("k", **kw)

        @jv.program(budget=jv.Budget(calls=50, cost=1.0), check_static=False)
        def prog():
            q = jv.measure("档", scale=scale, calib=jv.calib("k")) if phys == "score" else jv.test("行吗", calib=jv.calib("k"))
            rs = jv.judge([jv.state(on=jv.lit(f"s{i}")) for i in range(len(ps))], q)
            picked = jv.allocate(rs, k)
            bound = jv.unsure_bound(rs)
            jv.consume(jv.cut(rs), unsure=jv.drop)
            return picked, bound

        picked, bound = prog()
        # 档案字段（§1「凡是数字都是档案字段」）：冷键的保守线与每种题式的 δ 都从档案来，不是代码常数
        PROFILE.setdefault("safety_lines", list(rt.safety_lines()))
        PROFILE.setdefault("delta", {k2: rt.delta_for(k2) for k2 in ("noul", "choice", "score")})
    return {"name": name, "ps": ps, "k": k, "status": status, "hi": hi, "lo": lo,
            "unsure_rate": unsure_rate, "delta": delta, "phys": phys, "scale": list(scale),
            "picked": list(picked), "bound": bound}


FUZZY = [0.93, 0.05, 0.52, 0.91, 0.58, 0.04, 0.47, 0.95, 0.43, 0.06, 0.90, 0.03]

cases = [
    # 1. strength.py 的十二段：最不确定的四段正是四段 fuzzy（带内并列，按下标升序）
    collect("noul_twelve", FUZZY, 4),
    # 2. 全部落在带内：分数全是 0.0，结果就是纯下标顺序——比较器写错这条会露馅
    collect("noul_all_in_band", [0.50, 0.48, 0.52, 0.45, 0.55], 3),
    # 3. 并列：两两同分，按下标升序定序
    collect("noul_ties", [0.90, 0.10, 0.90, 0.10], 2),
    # 4. 全部落在带外：按离带的距离排
    collect("noul_all_out", [0.99, 0.72, 0.01, 0.28, 0.85], 3),
    # 5. k 大于读数个数 / k 为 0 / 空输入
    collect("k_gt_len", [0.9, 0.1], 5),
    collect("k_zero", [0.9, 0.1, 0.5], 0),
    collect("empty", [], 3),
    # 6. 校准记录没有 unsure_rate：无记录的题按 1 计，进 n_unknown
    collect("no_unsure_rate", [0.9, 0.5, 0.1], 2, unsure_rate=None),
    # 7. Σu 超过 n：联合界要被 n 夹住
    collect("bound_clamped", [0.9, 0.5, 0.1], 1, unsure_rate=0.8),
    # 8. 记录自带 delta，覆盖档案默认
    collect("delta_override", [0.80, 0.72, 0.50, 0.20], 2, delta=0.2),
    # 9. 分档题：p 是选中那档的概率，δ 默认 0.15
    collect("score_levels", [0.95, 0.60, 0.40, 0.05], 2, phys="score", scale=("低", "中", "高")),
    # 10. 分档题的 δ 能不能分辨出来：δ=0.1141 时 0.78 还在带内，δ=0.05 就出带了，选出来的下标不同
    collect("score_delta_matters", [0.78, 0.50], 1, phys="score", scale=("低", "中", "高")),
    # 11. 冷记录（非上岗）：线不从记录来，从档案的 safety_lines 来——记录写 0.9/0.1 也不该被用上
    collect("cold_record", [0.93, 0.52, 0.05], 2, status="冷"),
    collect("cold_lines_ignored", [0.93, 0.52, 0.05], 2, status="冷", hi=0.9, lo=0.1),
]

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "oracle.json")
with open(out, "w", encoding="utf-8") as f:
    json.dump({"source": "foundation/jv/runtime.py allocate:1236 unsure_bound:1246",
               "python": sys.version.split()[0], "profile": PROFILE, "cases": cases}, f, ensure_ascii=False, indent=2)
print(f"写了 {len(cases)} 条到 {out}")
for c in cases:
    print(f"  {c['name']:20s} picked={c['picked']} bound={c['bound']}")

# ---------------------------------------------------------------- 档案（Profile）对照基准
#
# Rust 侧 `CalibStore::new()` 一直恒给 `Profile::default()`（代码兜底值），而 Python 从
# `foundation/profile/profiles/*.json` 加载。两边的线与 δ 因此不同 —— 同一个程序会切出
# **不同的出口**，且不报错。这一段把 Python 的实际取值与账本头用的 profile_hash 记下来。

from foundation.core.canon import H  # noqa: E402
from foundation.jv.runtime import load_profile  # noqa: E402

_prof = load_profile()
_rt_for_profile = jv.Runtime(client=jv.FakeClient(rule=Rule({}, "noul")), profile=_prof)
with _rt_for_profile:
    _profile_out = {
        "name": "jev-1.13.0",
        # 账本头里的那个哈希：sha256(canon([profile]))[:16]
        "profile_hash": H(_prof),
        "safety_lines": list(_rt_for_profile.safety_lines()),
        "delta": {k: _rt_for_profile.delta_for(k) for k in ("noul", "choice", "score")},
        # δ 是从哪几个字段读出来的（Rust 要按同样的路径取，不是抄结果）
        "delta_paths": {
            "noul": ["delta", "noul", "immediate", "p99"],
            "choice": ["delta", "choice_prob_chosen", "immediate", "p99"],
            "score": ["delta", "score", "immediate", "p99"],
        },
        "safety_path": ["lines", "safety_default"],
    }

with open(out, encoding="utf-8") as f:
    _all = json.load(f)
_all["profile_oracle"] = _profile_out
with open(out, "w", encoding="utf-8") as f:
    json.dump(_all, f, ensure_ascii=False, indent=2)
print("档案对照基准：", json.dumps(_profile_out, ensure_ascii=False))
