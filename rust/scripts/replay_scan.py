#!/usr/bin/env python3
"""只凭账本重放扫查：仪表「重放一致」项（B77，`地基/附注/2026-09-24-评估①裁定.md` §四、§六·5）。

复建 `地基/过程记录/工程-修复-folio重放.md` §六 的扫查（原脚本在会话暂存区，未入库）。每组先首跑
（夹具加校准）写账本，再**只凭账本**重放（不给夹具、不给校准），比较：
  status、value、returned_unsure、pending 的未决原因、告警编号集合（W-header 另计）、重放新增调用数（须为 0）。

组的构成（与原扫查同法，组数按本仓库现状实数报告，不强凑 34）：
  1. `tests/golden/manifest.json` 里去掉 expect=error 与 resume_from 的用例；
  2. 没进金样的 `examples/*.jpp`（有同名夹具才跑）；
  3. 三个探针的夹具与 `probes/scope` 的校准目录组合；
  4. 续接后账本的只凭账本重放（B83，步 7c）：`lifecycle` 首跑 → `lifecycle-resume` 续接写新账本 → 只凭新账本重放，
     与续接趟的报告比。

输出：W-header 组数、差异组数、各组明细；`--json` 写机读结果。只报告，不失败。
"""
import argparse
import json
import pathlib
import re
import shutil
import subprocess
import tempfile

from _baseline import ROOT

JPP = ROOT / "target" / "debug" / "jpp"
P = ROOT / "probes"
SC = P / "scope"


def groups() -> list:
    out = []
    m = json.loads((ROOT / "tests/golden/manifest.json").read_text(encoding="utf-8"))
    covered = set()
    for c in m["cases"]:
        covered.add(c["source"])
        if c.get("expect") == "error" or c.get("resume_from") or not c.get("fixtures"):
            continue
        out.append({"name": c["name"], "cwd": None, "source": ROOT / c["source"], "fixtures": ROOT / c["fixtures"],
                    "calib": ROOT / c["calib"] if c.get("calib") else None, "files": c.get("files", {})})
    for src in sorted((ROOT / "examples").glob("*.jpp")):
        rel = str(src.relative_to(ROOT))
        fx = ROOT / "examples/fixtures" / (src.stem + ".json")
        if rel not in covered and fx.exists():
            out.append({"name": "examples/" + src.name, "cwd": None, "source": src, "fixtures": fx, "calib": None, "files": {}})
    probe = [
        ("winnow", "winnow.jpp", P / "winnow/fixture.json", None),
        ("winnow-batched", "winnow-batched.jpp", P / "winnow/fixture.json", None),
        ("folio", "folio.jpp", P / "folio/fixture.json", None),
        ("entity-align", "align.jpp", P / "entity-align/fixture.json", None),
    ]
    for cal in ("calib", "calib-class", "calib-nofp"):
        probe.append((f"winnow@scope-{cal}", "winnow.jpp", SC / "fixture-winnow.json", SC / cal))
        probe.append((f"folio@scope-{cal}", "folio.jpp", SC / "fixture-folio.json", SC / cal))
    for name, src, fx, cal in probe:
        d = P / name.split("@")[0].replace("winnow-batched", "winnow")
        # 步 14b-0：设计者版探针的材料经 --input 交给程序
        out.append({"name": "probes/" + name, "cwd": d, "source": d / src, "fixtures": fx, "calib": cal, "files": {},
                    "input": d / "baseline/materials.json"})
    byname = {c["name"]: c for c in m["cases"]}
    for c in m["cases"]:
        if c.get("resume_from") and c["resume_from"] in byname:
            f = byname[c["resume_from"]]
            out.append({"name": c["name"] + "（续接后账本）", "cwd": None, "source": ROOT / f["source"],
                        "fixtures": ROOT / f["fixtures"], "calib": ROOT / f["calib"] if f.get("calib") else None,
                        "files": f.get("files", {}),
                        "resume": {"source": ROOT / c["source"], "fixtures": ROOT / c["fixtures"],
                                   "calib": ROOT / c["calib"] if c.get("calib") else None}})
    for cal in ("calib", "calib-class", "calib-nofp"):
        out.append({"name": f"examples/topic-relevance@scope-{cal}", "cwd": None,
                    "source": ROOT / "examples/topic-relevance.jpp", "fixtures": SC / "fixture-topic-relevance.json",
                    "calib": SC / cal, "files": {}})
    return out


def run(args, cwd):
    return subprocess.run([str(JPP), "run", *map(str, args)], cwd=cwd, capture_output=True, text=True)


def codes(report: dict) -> list:
    return sorted({re.match(r"[A-Z]-[\w-]+", w).group(0) if re.match(r"[A-Z]-[\w-]+", w) else w[:20]
                   for w in report.get("trace", {}).get("warnings", [])})


def causes(report: dict) -> list:
    return sorted(json.dumps(p.get("cause", p.get("exit")), ensure_ascii=False, sort_keys=True)
                  for p in report.get("pending", []) or [])


def scan_one(g: dict) -> dict:
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="jpp_replay_"))
    try:
        cwd = g["cwd"] or tmp
        for k, v in g["files"].items():
            (tmp / k).write_text(v, encoding="utf-8")
        led, r1, r2 = tmp / "ledger.jsonl", tmp / "first.json", tmp / "replay.json"
        inp = ["--input", g["input"]] if g.get("input") else []
        a = [g["source"], "--fixtures", g["fixtures"], "--ledger-out", led, "--output", r1, *inp]
        if g["calib"]:
            a += ["--calib", g["calib"]]
        p1 = run(a, cwd)
        if not r1.exists():
            return {"name": g["name"], "error": "首跑失败：" + p1.stderr.strip()[-300:]}
        src = g["source"]
        if g.get("resume"):
            # 续接趟：比的是续接后的账本与续接趟的报告（B83）
            rs = g["resume"]
            led2, r1b = tmp / "ledger2.jsonl", tmp / "resumed.json"
            b = [rs["source"], "--fixtures", rs["fixtures"], "--resume", led, "--ledger-out", led2, "--output", r1b, *inp]
            if rs["calib"]:
                b += ["--calib", rs["calib"]]
            p1b = run(b, cwd)
            if not r1b.exists():
                return {"name": g["name"], "error": "续接失败：" + p1b.stderr.strip()[-300:]}
            led, r1, src = led2, r1b, rs["source"]
        p2 = run([src, "--replay", led, "--output", r2, *inp], cwd)
        if not r2.exists():
            return {"name": g["name"], "error": "重放失败：" + p2.stderr.strip()[-300:], "diff": ["重放失败"]}
        f, s = json.loads(r1.read_text(encoding="utf-8")), json.loads(r2.read_text(encoding="utf-8"))
        diff = []
        for k in ("status", "value", "returned_unsure"):
            if f.get(k) != s.get(k):
                diff.append(k)
        if causes(f) != causes(s):
            diff.append("pending.cause")
        cf, cs = [c for c in codes(f) if c != "W-header"], [c for c in codes(s) if c != "W-header"]
        if cf != cs:
            diff.append(f"告警编号 {cf} → {cs}")
        new_calls = s.get("cost", {}).get("calls", 0)
        if new_calls:
            diff.append(f"重放新增调用 {new_calls}")
        wh = [w for w in s.get("trace", {}).get("warnings", []) if w.startswith("W-header")]
        return {"name": g["name"], "w_header": len(wh), "w_header_text": [w[:120] for w in wh[:2]], "diff": diff}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def scan() -> dict:
    rows = [scan_one(g) for g in groups()]
    ok = [r for r in rows if "error" not in r or r.get("diff")]
    return {"groups": len(rows),
            "w_header_groups": sum(1 for r in rows if r.get("w_header")),
            "diff_groups": sum(1 for r in rows if r.get("diff")),
            "errors": [r for r in rows if "error" in r],
            "rows": rows, "scanned": len(ok)}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--json")
    a = ap.parse_args()
    s = scan()
    print(f"[replay_scan] {s['groups']} 组；只凭账本重放报 W-header {s['w_header_groups']} 组；差异 {s['diff_groups']} 组；首跑失败 {len(s['errors'])} 组")
    for r in s["rows"]:
        tag = "失败 " + r["error"] if "error" in r else ("W-header" if r["w_header"] else "")
        if r.get("diff") or tag:
            print(f"  - {r['name']}: {tag} {'差异 ' + '；'.join(r['diff']) if r.get('diff') else ''}")
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps(s, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
