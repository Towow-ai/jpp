#!/usr/bin/env python3
"""账本 v2 → v3 迁移的证明（步 18a，B124 Q3）。

两段：
  before  用 v2 二进制（`ledger-v2-archive` 标签处构建）对每份 v2 账本只凭账本重放，存报告；
  after   仓库里的账本已用 `jpp ledger-migrate` 改写为 v3 后，用新二进制重放同一批账本，
          报告与 before 逐字节比较；另做结构核对：v2 原文（取自 git 标签 ledger-v2-archive）与 v3 文件
          的判断、问人、缺席条目逐条相同，效应输出内容相同，头行 calib_used 与 CalibUsed 条目按键相同。

用法：
  python3 scripts/ledger_v2_migration_check.py before <v2 二进制>
  python3 scripts/ledger_v2_migration_check.py after  <v3 二进制>
结果写在 地基/实测/2026-09-25-账本v3迁移/（before/、after/、summary.json）。
"""
import json, os, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]          # rust-jpp
BASE = ROOT.parent                                    # 地基
OUT = BASE / "实测" / "2026-09-25-账本v3迁移"
TAG = "ledger-v2-archive"

# 账本 → (源程序, 是否带 --input 文件)。相对 地基/。确定不了源程序的写 None，只做结构核对。
TABLE = {
    "评估/2026-09-24-试写/interview/fixed-ledger.jsonl": ("评估/2026-09-24-试写/interview/interview.jpp", None),
    "评估/2026-09-24-试写/interview/live-ledger.jsonl": ("评估/2026-09-24-试写/interview/interview.jpp", None),
    "评估/2026-09-24-试写/refund/fixed-ledger.jsonl": ("评估/2026-09-24-试写/refund/refund.jpp", None),
    "评估/2026-09-24-试写/refund/live-ledger.jsonl": ("评估/2026-09-24-试写/refund/refund.jpp", None),
    "评估/2026-09-24-试写/refund/live-ledger-flat.jsonl": ("评估/2026-09-24-试写/refund/refund-flat.jpp", None),
    "评估/2026-09-25-试写/triage/fixed-ledger.jsonl": ("评估/2026-09-25-试写/triage/triage.jpp", "materials.json"),
    "评估/2026-09-25-试写/triage/live-ledger.jsonl": ("评估/2026-09-25-试写/triage/triage.jpp", "materials.json"),
    "评估/2026-09-25-试写/triage/resume-ledger.jsonl": ("评估/2026-09-25-试写/triage/triage.jpp", "materials.json"),
    "评估/2026-09-25-试写/aspects/fixed-ledger.jsonl": ("评估/2026-09-25-试写/aspects/aspects.jpp", "materials.json"),
    "评估/2026-09-25-试写/aspects/live-ledger.jsonl": ("评估/2026-09-25-试写/aspects/aspects.jpp", "materials.json"),
    "评估/27a-准备/F1-JSON字段/live-ledger.jsonl": ("评估/27a-准备/F1-JSON字段/read.jpp", "materials.json"),
    "评估/27a-准备/F2-回答了问题/live-ledger.jsonl": ("评估/27a-准备/F2-回答了问题/read.jpp", "materials.json"),
    "评估/27a-准备/F3-提到哪一个/live-ledger.jsonl": ("评估/27a-准备/F3-提到哪一个/read.jpp", "materials.json"),
    "评估/27a-准备/F4-同一个/live-ledger.jsonl": ("评估/27a-准备/F4-同一个/read.jpp", "materials.json"),
    "评估/27a-准备/F5-还剩多少/live-ledger.jsonl": ("评估/27a-准备/F5-还剩多少/read.jpp", "materials.json"),
    "评估/27a-置换重跑/F3-提到哪一个/live-ledger.jsonl": ("评估/27a-置换重跑/F3-提到哪一个/read.jpp", "materials.json"),
    "评估/27a-置换重跑/F4-同一个/live-ledger.jsonl": ("评估/27a-置换重跑/F4-同一个/read.jpp", "materials.json"),
    "评估/2026-09-24-V6试用线/live-refund.jsonl": ("评估/2026-09-24-试写/refund/refund.jpp", None),
    "评估/2026-09-24-V6试用线/live-refund-do.jsonl": ("评估/2026-09-24-V6试用线/refund-do.jpp", None),
    "评估/2026-09-24-V6试用线/live-readings.jsonl": ("评估/2026-09-24-V6试用线/readings.jpp", None),
    "评估/2026-09-24-V7固定序/live-refund.jsonl": ("评估/2026-09-24-试写/refund/refund.jpp", None),
    "评估/2026-09-24-V7固定序/live-refund-do.jsonl": ("评估/2026-09-24-V7固定序/refund-do.jpp", None),
    "评估/2026-09-25-31-0c深度曲线/resume-ledger.jsonl": ("评估/2026-09-24-试写/refund/refund.jpp", None),
    "rust-jpp/probes/winnow/fixed-winnow.ledger.jsonl": ("rust-jpp/probes/winnow/winnow.jpp", "baseline/materials.json"),
    "rust-jpp/probes/winnow/live-winnow.ledger.jsonl": ("rust-jpp/probes/winnow/winnow.jpp", "baseline/materials.json"),
    "rust-jpp/probes/winnow/live-winnow-lined.ledger.jsonl": ("rust-jpp/probes/winnow/winnow.jpp", "baseline/materials.json"),
    "rust-jpp/probes/winnow/fixed-winnow-batched.ledger.jsonl": ("rust-jpp/probes/winnow/winnow-batched.jpp", "baseline/materials.json"),
    "rust-jpp/probes/winnow/live-winnow-batched.ledger.jsonl": ("rust-jpp/probes/winnow/winnow-batched.jpp", "baseline/materials.json"),
    "rust-jpp/probes/winnow/truth/live-read.ledger.jsonl": ("rust-jpp/probes/winnow/truth/read-error.jpp", None),
    "rust-jpp/probes/winnow/truth/live-read2.ledger.jsonl": ("rust-jpp/probes/winnow/truth/read-error.jpp", None),
    "rust-jpp/probes/folio/fixed-folio.ledger.jsonl": ("rust-jpp/probes/folio/folio.jpp", "baseline/materials.json"),
    "rust-jpp/probes/folio/live-folio.ledger.jsonl": ("rust-jpp/probes/folio/folio.jpp", "baseline/materials.json"),
    "rust-jpp/probes/entity-align/fixed-align.ledger.jsonl": ("rust-jpp/probes/entity-align/align.jpp", "baseline/materials.json"),
    "rust-jpp/probes/entity-align/live-align.ledger.jsonl": ("rust-jpp/probes/entity-align/align.jpp", "baseline/materials.json"),
    "rust-jpp/probes/entity-align/live-align-20e.ledger.jsonl": ("rust-jpp/probes/entity-align/align.jpp", "baseline/materials.json"),
}
for mode in ("C1", "C2", "C3", "S1", "S2", "S3"):
    for prog in ("sieve", "topic-relevance"):
        TABLE[f"实测/2026-09-25-V2-层内并发/{prog}.{mode}.ledger.jsonl"] = (f"rust-jpp/examples/{prog}.jpp", None)


# 不迁移的：伪账本（头行带 `note`、条目不成链，从来不是 `Ledger::decode` 能读的账本，只供
# `calib-import --from-ledger --list-out` 按行读 Judge 条目；该读法不看版本号）。
NOT_MIGRATED = {"评估/2026-09-24-S4R迁移/frame-ledger.jsonl": "伪账本，只供 --list-out 按行读，不迁移"}


def slug(p):
    return p.replace("/", "__")


def replay(binary, ledger, prog, inp, dest):
    led = BASE / ledger
    src = BASE / prog
    cwd = src.parent
    args = [binary, "run", src.name, "--replay", str(led), "--output", str(dest.with_suffix(".report.json"))]
    if inp and (cwd / inp).exists():
        args += ["--input", inp]
    p = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    dest.with_suffix(".stderr.txt").write_text(p.stderr.replace(str(BASE), "<地基>"), encoding="utf-8")
    return p.returncode


def before(binary):
    d = OUT / "before"
    d.mkdir(parents=True, exist_ok=True)
    res = {}
    for ledger, (prog, inp) in sorted(TABLE.items()):
        if prog is None:
            res[ledger] = "无源程序：只做结构核对"
            continue
        rc = replay(binary, ledger, prog, inp, d / slug(ledger))
        res[ledger] = "重放成功" if rc == 0 and (d / slug(ledger)).with_suffix(".report.json").exists() else f"重放失败（退出码 {rc}）"
    (d / "status.json").write_text(json.dumps(res, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(res, ensure_ascii=False, indent=1))


def v2_text(ledger):
    return subprocess.run(["git", "show", f"{TAG}:地基/{ledger}"], cwd=BASE, capture_output=True, text=True, check=True).stdout


def lines(text):
    return [json.loads(l) for l in text.splitlines() if l.strip()]


def structural(ledger):
    """v2 原文与 v3 文件逐条核对；返回不符项列表（空即相符）。"""
    old = lines(v2_text(ledger))
    new = lines((BASE / ledger).read_text(encoding="utf-8"))
    bad = []
    if new[0].get("version") != 3:
        bad.append("v3 头行版本不是 3")
    oh, nh = old[0].get("header"), new[0].get("header")
    if oh is not None:
        oh = json.loads(json.dumps(oh))
        oh["compared"].pop("calib_used_hash", None)
    if oh != nh:
        bad.append("头（去掉 calib_used_hash 后）不同")
    oe = [x["entry"] for x in old[1:]]
    ne = [x["entry"] for x in new[1:]]
    used = {}
    rest = []
    for e in ne:
        if "CalibUsed" in e:
            used[e["CalibUsed"]["key"]] = {"hash": e["CalibUsed"]["hash"], "record": e["CalibUsed"]["record"]}
        else:
            rest.append(e)
    if used != old[0].get("calib_used", {}):
        bad.append("CalibUsed 条目与 v2 头行 calib_used 不同")
    if len(rest) != len(oe):
        bad.append(f"条目数不同：v2 {len(oe)}，v3（除 CalibUsed）{len(rest)}")
    for i, (a, b) in enumerate(zip(oe, rest)):
        if "Effect" in a:
            ea, eb = a["Effect"], b.get("Effect", {})
            out = ea["output"]
            if isinstance(out, dict) and "__mat" in out:
                want_out = out["__mat"]
                want_meta = {"addr": out.get("addr", ""), "origin": out.get("origin", []), "taint": out.get("taint")}
            else:
                want_out, want_meta = out, None
            got_meta = eb.get("output_mat")
            if got_meta is not None:
                got_meta = {k: got_meta.get(k) for k in ("addr", "origin", "taint")}
                if eb.get("output_mat", {}).get("sources"):
                    bad.append(f"第 {i + 1} 条效应迁移后带了 sources（v2 没有来源）")
            if eb.get("output") != want_out or got_meta != want_meta:
                bad.append(f"第 {i + 1} 条效应输出不同")
            if {k: v for k, v in ea.items() if k not in ("output", "output_mat")} != {k: v for k, v in eb.items() if k not in ("output", "output_mat")}:
                bad.append(f"第 {i + 1} 条效应其余字段不同")
        elif a != b:
            bad.append(f"第 {i + 1} 条不同")
    return bad


def after(binary):
    d = OUT / "after"
    d.mkdir(parents=True, exist_ok=True)
    st = json.loads((OUT / "before" / "status.json").read_text(encoding="utf-8"))
    summary = {k: {"结构核对": v, "重放": "不适用"} for k, v in NOT_MIGRATED.items()}
    for ledger, (prog, inp) in sorted(TABLE.items()):
        row = {"结构核对": structural(ledger) or "相符"}
        if st.get(ledger) == "重放成功":
            rc = replay(binary, ledger, prog, inp, d / slug(ledger))
            a = (OUT / "before" / slug(ledger)).with_suffix(".report.json")
            b = (d / slug(ledger)).with_suffix(".report.json")
            if rc != 0 or not b.exists():
                row["重放"] = f"v3 重放失败（退出码 {rc}）"
            else:
                if a.read_bytes() == b.read_bytes():
                    row["重放"] = "报告逐字节相同"
                else:
                    ra, rb = json.loads(a.read_text(encoding="utf-8")), json.loads(b.read_text(encoding="utf-8"))
                    旧告警 = [w for w in ra["trace"]["warnings"] if w.startswith("W-header") and "calib_used_hash 旧 （无）" in w]
                    ra["trace"]["warnings"] = [w for w in ra["trace"]["warnings"] if w not in 旧告警]
                    row["重放"] = ("报告相同，只少一条 v2 误报（步 7b 前录的账本头没有 calib_used_hash，v2 重放报「旧 （无）」；"
                                 "v3 该字段移出比对集合、改逐键比，不再误报）") if 旧告警 and ra == rb else "报告不同"
        else:
            row["重放"] = st.get(ledger, "未跑")
        summary[ledger] = row
    (OUT / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    ok = all(k in NOT_MIGRATED or (r["结构核对"] == "相符" and (r["重放"] == "报告逐字节相同" or r["重放"].startswith("报告相同，只少一条") or r["重放"].startswith("重放失败")))
             for k, r in summary.items())
    print(json.dumps(summary, ensure_ascii=False, indent=1))
    print("通过" if ok else "未通过")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    {"before": before, "after": after}[sys.argv[1]](sys.argv[2])
