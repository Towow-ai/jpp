"""生成 `tests/py_exits.json`：Python `runtime.py::_decide` 的 `test` 臂在一条 p 网格上的出口（跨内核对照的金标）。

用法（在 `地基/` 下，需 Python ≥ 3.10）：python3.12 rust-jpp/crates/jpp/tests/oracle/gen_py_exits.py
步 15d-2 起两个内核的边界比较都带往返容差 `BOUNDARY_EPS`（1e-12）；此前金标是手工当场跑出的，没有留脚本，
本脚本按同一组参数（hi 0.66、lo 0.56、δ 0.04，p 0.00–1.00 步长 0.01）重跑。依据：21 步 15d-2。
"""
import json
import pathlib
import sys

地基 = pathlib.Path(__file__).resolve().parents[5]
sys.path.insert(0, str(地基))
from foundation.jv import runtime as rt  # noqa: E402

HI, LO, DELTA = 0.66, 0.56, 0.04


class _Q:
    op = "test"


out = {}
for i in range(101):
    p = round(i / 100, 2)
    e = rt.Runtime._decide(None, {"p": p}, _Q(), None, HI, LO, DELTA, {})
    out[f"{p:.2f}"] = e.cause if e.kind == "unsure" else e.kind
dst = pathlib.Path(__file__).resolve().parents[1] / "py_exits.json"
dst.write_text(json.dumps({"hi": HI, "lo": LO, "delta": DELTA, "exits": out}, ensure_ascii=False))
print(dst, sum(1 for v in out.values() if v == "act"), "act")
