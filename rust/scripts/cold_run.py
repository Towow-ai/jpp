#!/usr/bin/env python3
"""冷线跑：空校准库下跑全部示例与探针程序，查运行期 J-05（步 31-1b；B96 配套、B81 (c) 收窄）。

依据：`地基/评估/2026-09-24-仪表读数4诊断与裁定.md` §五·4 第 4 项、§六「收窄」第 2 条。
固定观察下线都在、未决为零，漏写的转交（例如读了 `tally` 的聚合值、没交出它的 `pending`）看不出来；
冷线（没有任何校准记录）下全部出口都是 `Unsure(cold)`，每一处没有去向的未决都会在程序结束时报运行期 J-05。
这是「未决是否都有去向」的免费代理，不花钱、不发真机调用。

跑什么：
  1. `tests/golden/manifest.json` 里有夹具、不预期报错、不是续接的用例；
  2. 没进金样、有同名夹具的 `examples/*.jpp`；
  3. 探针：各项目 `measure.toml` 的 J++ 实现（设计者 T0 / T1、没冻结的非设计者版），材料经 `--input`。
     标了 `redispatch` 的冻结实现不跑（源文件按旧形状写成，新运行时下跑不起来）。
怎么冷：夹具去掉 `calibrations`，不给 `--calib`。
判定：退出码非零且报文含 `J-05` 记为命中；其余非零退出记为「其他失败」并列出首行，不算命中。

用法：
  cold_run.py            打印逐程序结果与汇总（报告模式，恒退出 0）
  cold_run.py --fail     有命中即退出 1（随步 9b 转失败模式）
  cold_run.py --json f   另写机读结果
"""
from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

from _baseline import ROOT

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

JPP = ROOT / "target" / "debug" / "jpp"
PROBES = ROOT / "probes"


def groups() -> list:
    out = []
    m = json.loads((ROOT / "tests/golden/manifest.json").read_text(encoding="utf-8"))
    covered = set()
    for c in m["cases"]:
        covered.add(c["source"])
        if c.get("expect") == "error" or c.get("resume_from") or not c.get("fixtures"):
            continue
        out.append({"name": "golden/" + c["name"], "cwd": None, "source": ROOT / c["source"],
                    "fixtures": ROOT / c["fixtures"], "files": c.get("files", {})})
    for src in sorted((ROOT / "examples").glob("*.jpp")):
        rel = str(src.relative_to(ROOT))
        fx = ROOT / "examples/fixtures" / (src.stem + ".json")
        if rel not in covered and fx.exists():
            out.append({"name": rel, "cwd": None, "source": src, "fixtures": fx, "files": {}})
    for toml in sorted(PROBES.glob("*/measure.toml")):
        base = toml.parent
        with open(toml, "rb") as fh:
            mt = tomllib.load(fh)
        j = mt["jpp"]
        impls = [{"source": j["source"], "run_dir": j.get("run_dir", "."), "input": j.get("input"),
                  "fixture": j["fixture"], "tier": "t0"}]
        for x in j.get("impl", []):
            fx = mt.get("t1", {}).get("fixture") if x.get("tier") == "t1" else j["fixture"]
            impls.append({**x, "fixture": fx})
        for x in impls:
            src = (base / x["source"]).resolve()
            name = "probes/" + base.name + "/" + x["source"]
            if x.get("redispatch"):
                out.append({"name": name, "skip": x["redispatch"]})
                continue
            if not src.exists():
                continue
            cwd = (base / x.get("run_dir", str(pathlib.Path(x["source"]).parent))).resolve()
            out.append({"name": name, "cwd": cwd, "source": src, "fixtures": (base / x["fixture"]).resolve(),
                        "files": {}, "input": (base / x["input"]).resolve() if x.get("input") else None})
    return out


def run_one(g: dict) -> dict:
    if g.get("skip"):
        return {"name": g["name"], "result": "跳过", "why": g["skip"]}
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="jpp_cold_", dir=str(ROOT / "target")))
    try:
        for k, v in g["files"].items():
            (tmp / k).write_text(v, encoding="utf-8")
        fx = json.loads(pathlib.Path(g["fixtures"]).read_text(encoding="utf-8"))
        fx["calibrations"] = []
        cold = tmp / "cold-fixture.json"
        cold.write_text(json.dumps(fx, ensure_ascii=False), encoding="utf-8")
        rep = tmp / "report.json"
        args = [str(JPP), "run", str(g["source"]), "--fixtures", str(cold), "--output", str(rep)]
        if g.get("input"):
            args += ["--input", str(g["input"])]
        p = subprocess.run(args, cwd=g["cwd"] or tmp, capture_output=True, text=True)
        err = "\n".join(x for x in p.stderr.splitlines() if not x.startswith("档案："))
        if p.returncode == 0:
            r = json.loads(rep.read_text(encoding="utf-8")) if rep.exists() else {}
            return {"name": g["name"], "result": "通过", "status": r.get("status"),
                    "pending": len(r.get("pending") or [])}
        if "J-05" in p.stderr:
            first = next((x for x in err.splitlines() if "J-05" in x), err.splitlines()[0] if err else "")
            return {"name": g["name"], "result": "J-05", "why": first.split(": ", 1)[-1][:200]}
        return {"name": g["name"], "result": "其他失败", "why": (err.splitlines() or [""])[0][:200]}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--fail", action="store_true")
    ap.add_argument("--json")
    a = ap.parse_args()
    if not JPP.exists():
        sys.exit("先 cargo build -p jpp")
    rows = [run_one(g) for g in groups()]
    for r in rows:
        print(f"[cold_run] {r['name']}：{r['result']}" + (f"（{r['why']}）" if r.get("why") else ""))
    n = {k: sum(1 for r in rows if r["result"] == k) for k in ("通过", "J-05", "其他失败", "跳过")}
    print(f"[cold_run] {len(rows)} 个程序：通过 {n['通过']}，运行期 J-05 {n['J-05']}，其他失败 {n['其他失败']}，跳过 {n['跳过']}")
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps({"counts": n, "rows": rows}, ensure_ascii=False, indent=1) + "\n",
                                        encoding="utf-8")
    sys.exit(1 if a.fail and n["J-05"] else 0)


if __name__ == "__main__":
    main()
