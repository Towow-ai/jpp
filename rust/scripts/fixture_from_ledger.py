#!/usr/bin/env python3
"""真机账本 → 固定观察夹具（步 31-1 工具项；`附注/2026-09-24-仪表读数1裁定.md` §六·2、诊断 §三·7）。

有了它，探针不必再手写夹具脚本（`make_fixture.py`：材料副本 + 合成读数 + 校准字面量），夹具直接来自一次真机运行。

为什么要跑程序：账本按 B17 (3) 只存键（状态哈希、题哈希），不存材料原文，从账本本身还原不出夹具的 `on`/`text`。
做法是让 `jpp run` 在固定观察模式下反复跑同一个程序：每次缺一条观察，`jpp` 报「固定观察未命中」并给出
内核算出的状态 JSON、题 JSON 与状态短哈希；本工具按「状态哈希前缀 + 题面」在真机账本里找到那次调用的读数，
补进夹具，再跑，直到程序跑完。因为补进去的都是真机读数，固定观察下的控制流与真机运行相同。

题面来自真机运行报告（`--output` 写的 JSON）里 trace 事件的 note；trace 与账本按键一一对应。

用法：
    fixture_from_ledger.py <程序.jpp> --ledger <真机账本.jsonl> --report <真机报告.json> -o <夹具.json>
        [--calib <校准目录>]      用这个目录里的线跑；不给时用账本记下的（v3：CalibUsed 条目；v2：头行 calib_used）、真机运行实际用过的线
                                  （写到临时目录，不进夹具），这样固定观察的控制流与真机运行相同
        [--run-dir <目录>]        程序的运行目录（默认程序所在目录）
        [--max-rounds 500]

夹具的 `calibrations` 留空：线只从校准目录或 calib-import 来（B79 (d)、J-03）。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JPP = ROOT / "target" / "debug" / "jpp"


def load_live(ledger: Path, report: Path) -> list:
    rows = []
    for x in ledger.read_text(encoding="utf-8").splitlines()[1:]:
        j = json.loads(x).get("entry", {}).get("Judge")
        if j:
            rows.append(j)
    notes = {}
    rep = json.loads(report.read_text(encoding="utf-8"))
    for e in rep.get("trace", {}).get("events", []):
        if e.get("kind") == "judge":
            notes[e["key"]] = e.get("note", "")
    out = []
    for j in rows:
        n = notes.get(j["key"], "")
        text = n[1:-1] if n.startswith("「") and n.endswith("」") else n
        row = {"state": j["jkey"]["state"], "text": text, "answer": j["answer"], "key": j["key"]}
        # 判断器随答案报的自报置信度（B154，步 20j-3）：账本有才写，夹具缺省按 p_max
        if j.get("confidence") is not None:
            row["confidence"] = j["confidence"]
        out.append(row)
    return out


def parse_json_prefix(s: str):
    """错误信息里的 JSON 可能含嵌套花括号，正则非贪婪会截短；从起点用解码器读一个完整值。"""
    return json.JSONDecoder().raw_decode(s)[0]


def calib_used(ledger):
    """账本实际命中的校准记录：v3 起是 `CalibUsed` 条目（按键取最后一条，步 18a）；v2 在头行 `calib_used`。"""
    lines = ledger.read_text(encoding="utf-8").splitlines()
    head = json.loads(lines[0])
    if head.get("version") == 2:
        return head.get("calib_used") or {}
    used = {}
    for l in lines[1:]:
        e = json.loads(l).get("entry", {}).get("CalibUsed")
        # B142（步 20j-1）：`declared:` 键是作者声明线，不是校准记录，不写成记录文件
        if e and not e["key"].startswith("declared:"):
            used[e["key"]] = {"hash": e["hash"], "record": e["record"]}
    return used


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("program")
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--report", required=True)
    ap.add_argument("-o", "--out", required=True)
    ap.add_argument("--calib")
    ap.add_argument("--run-dir")
    ap.add_argument("--input", help="程序的宿主入口材料（jpp run --input，步 14b-0）")
    ap.add_argument("--max-rounds", type=int, default=500)
    a = ap.parse_args()
    prog = Path(a.program).resolve()
    cwd = Path(a.run_dir).resolve() if a.run_dir else prog.parent
    live = load_live(Path(a.ledger).resolve(), Path(a.report).resolve())
    used = set()
    fx = {"description": f"由 scripts/fixture_from_ledger.py 从真机账本 {os.path.basename(a.ledger)} 导出；"
                         "读数全部来自真机，校准记录不写进夹具。", "calibrations": [], "observations": []}
    out = Path(a.out).resolve()
    with tempfile.TemporaryDirectory() as td:
        rep = Path(td) / "r.json"
        if not a.calib:
            cd = Path(td) / "calib"
            cd.mkdir()
            for k, v in calib_used(Path(a.ledger)).items():
                (cd / (k.replace("\x1f", "_") + ".json")).write_text(json.dumps(v["record"], ensure_ascii=False),
                                                                       encoding="utf-8")
            a.calib = str(cd)
        for _ in range(a.max_rounds):
            out.write_text(json.dumps(fx, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
            args = [str(JPP), "run", os.path.relpath(prog, cwd), "--fixtures", str(out), "--output", str(rep)]
            if a.calib:
                args += ["--calib", str(Path(a.calib).resolve())]
            if a.input:
                args += ["--input", str(Path(a.input).resolve())]
            p = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
            i = p.stderr.find("固定观察未命中")
            if i < 0:
                if p.returncode != 0:
                    sys.exit(f"程序运行失败（不是缺观察）：{p.stderr[-800:]}")
                print(f"完成：{len(fx['observations'])} 条观察，写 {out}")
                return
            err = p.stderr[i:]
            m = re.search(r"题「(.*?)」× 状态 ([0-9a-f]+)", err)
            text, short = m.group(1), m.group(2)
            st = parse_json_prefix(err[err.index("内核这次算出来的状态 = ") + len("内核这次算出来的状态 = "):])
            q = parse_json_prefix(err[err.index("；题 = ") + len("；题 = "):])
            cands = [k for k, r in enumerate(live) if r["state"].startswith(short) and r["text"] == text and k not in used]
            if not cands:
                sys.exit(f"真机账本里没有这次调用：题「{text}」× 状态 {short}。控制流与真机运行不同"
                         f"（常见原因：没给真机运行时的 --calib），或账本与报告不是同一次运行。")
            k = cands[0]
            used.add(k)
            obs = {kk: st[kk] for kk in ("on", "ctx", "ref", "over") if kk in st}
            obs.update({kk: q[kk] for kk in ("op", "text", "calib", "scale", "evidence", "labels") if kk in q and q[kk] not in (None, [])})
            obs["answer"] = live[k]["answer"]
            if "confidence" in live[k]:
                obs["confidence"] = live[k]["confidence"]
            fx["observations"].append(obs)
        sys.exit(f"超过 {a.max_rounds} 轮仍未跑完")


if __name__ == "__main__":
    main()
