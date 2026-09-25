#!/usr/bin/env python3
"""步 18a 金样自检：旧 v2 金样账本（取自标签 ledger-v2-archive）经 `jpp ledger-migrate` 迁移后，
与新录的 v3 金样账本比较——头行相同；条目除 `CalibUsed` 外逐条相同；`CalibUsed` 条目按键的集合相同
（v2 不记首次命中位置，迁移接在末尾，新录的在首次命中处）。同时是迁移函数的自检。

用法：python3 scripts/golden_v3_crosscheck.py <jpp 二进制> [<v2 金样所在的 git 引用，缺省 ledger-v2-archive>]
（rebase 到更新的 main 后，用 main 作引用：main 上的金样是 v2、本步重录的是 v3。）
"""
import json, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TAG = "ledger-v2-archive"


def entries(text):
    ls = [json.loads(l) for l in text.splitlines() if l.strip()]
    used = {}
    rest = []
    for x in ls[1:]:
        e = x["entry"]
        if "CalibUsed" in e:
            used[e["CalibUsed"]["key"]] = e["CalibUsed"]
        else:
            rest.append(e)
    return ls[0], rest, used


def main(jpp, ref=TAG):
    bad, n, 条目数 = [], 0, {}
    for new in sorted((ROOT / "tests" / "golden").glob("*/ledger.json")):
        rel = new.relative_to(ROOT.parent.parent)
        old = subprocess.run(["git", "show", f"{ref}:{rel}"], cwd=ROOT, capture_output=True, text=True)
        if old.returncode != 0:
            bad.append(f"{new.parent.name}：标签处没有这个金样")
            continue
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d) / "v2.jsonl", Path(d) / "v3.jsonl"
            a.write_text(old.stdout, encoding="utf-8")
            p = subprocess.run([jpp, "ledger-migrate", str(a), str(b)], capture_output=True, text=True)
            if p.returncode != 0:
                bad.append(f"{new.parent.name}：迁移失败 {p.stderr.strip()}")
                continue
            mh, mr, mu = entries(b.read_text(encoding="utf-8"))
        nh, nr, nu = entries(new.read_text(encoding="utf-8"))
        n += 1
        条目数[new.parent.name] = {"CalibUsed": len(nu), "其余": len(nr)}
        if mh != nh:
            bad.append(f"{new.parent.name}：头行不同")
        if mr != nr:
            bad.append(f"{new.parent.name}：CalibUsed 以外的条目不同")
        if mu != nu:
            bad.append(f"{new.parent.name}：CalibUsed 按键不同")
    print(json.dumps(条目数, ensure_ascii=False, indent=1))
    print(f"{n} 个金样；不符 {len(bad)} 处")
    for b in bad:
        print("  - " + b)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main(*sys.argv[1:3])
