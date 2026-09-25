#!/usr/bin/env python3
"""验收仪表（步 31-0 最小版，31-0b 改口径）：七项一次跑完，结果写 `地基/评估/仪表/<日期>[-<序号>].json` 与同名简表 `.md`。

依据：`地基/附注/2026-09-24-评估①裁定.md` §六·5（七项定义、数据来源、第一版允许）、§六·4（能力对照）、
§四（B77 重放一致）；`地基/评估/2026-09-24-阶段评估-1.md` §五 建议 10 与附录 A。
Nature 2026-09-24：「有了这个测量以后我们就可以根据结果不断地反馈，不断地修正……按真实的倍率或者真实的水平比较。」
所以每项都写「在参考系里的位置」或「对上一次读数的变化」，暂时取不到数的项写「无数」与原因，不留空。

每个里程碑与每个 S 步后运行（`17` 阶段评估条，主会话落）：

    cd 地基/rust-jpp && python3 scripts/dashboard.py        # 先自行 cargo build -p jpp，保证量的是当前源码

只读仓库、只跑固定观察与本地测试，不发真机调用、不花钱。每次运行写一份新读数：当天第一份为 `<日期>`，
之后为 `<日期>-2`、`<日期>-3`…（不覆盖已有读数；`--overwrite` 改写当天最新一份，`--out <路径前缀>` 写到别处、
供确定性核对）。上一次读数取正要写的那份之外最新的一份，逐项给出变化。

31-0b（`地基/附注/2026-09-24-仪表读数1裁定.md` B78、B80）：表达量比主读数为折行归一行数比，
原始行数比、token 比、调用数比并列；删「字面口径一」；夹具脚本不计入口径二。

31-1b（`地基/评估/2026-09-24-仪表读数4诊断与裁定.md` B96）：表达量比分 T1（(d) 性质验收，判定档）、
T1-strict（旧 (d) 逐位口径，只报不判）、T0 三档；T1 并列「全部实现」与「只计验收全过」两个中位，位置判定用后者；
单列「J++ 侧无通过实现的项目」。验收未过的基线不再让整项变「无数」，只从「只计验收全过」里去掉并照实列出。

31-0c（B109、B103，`地基/过程记录/工程-步31-0c.md`）：深度项在固定观察跳数分布之外加真机逐跳曲线（`live`）：
登记表 `地基/评估/2026-09-25-31-0c深度曲线/runs.json` 里的账本逐个只凭账本重放，出口按站点内次序对到判断条目
（对不上即报无数），每跳报 n、未决率（budget/unobserved 单列）、主真值与补充真值各自的覆盖与一致率。
"""
from __future__ import annotations

import argparse
import datetime
import glob
import json
import os
import pathlib
import re
import subprocess
import sys

from _baseline import ROOT

sys.path.insert(0, str(ROOT / "scripts"))
import measure_expr  # noqa: E402
import replay_scan  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

OUT = ROOT.parent / "评估" / "仪表"
JPP = ROOT / "target" / "debug" / "jpp"
TRIAL = ROOT.parent / "评估" / "2026-09-24-试写"


def sh(cmd, **kw):
    return subprocess.run(cmd, cwd=kw.pop("cwd", ROOT), capture_output=True, text=True, **kw)


def cargo_test(pkg: str, test: str, filt: str = "") -> dict:
    cmd = ["cargo", "test", "-p", pkg, "--test", test, "--offline", "-q"] + (["--", filt] if filt else [])
    p = sh(cmd)
    m = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", p.stdout)
    passed = sum(int(x[1]) for x in m)
    failed = sum(int(x[2]) for x in m)
    return {"cmd": " ".join(cmd), "ok": p.returncode == 0 and failed == 0 and passed > 0,
            "passed": passed, "failed": failed, "tail": (p.stdout + p.stderr).strip()[-300:] if p.returncode else ""}


# —— 1 表达量比 ——

def _tier_rows(t: dict) -> dict:
    per = {}
    for r in t["projects"]:
        p = r["project"]
        chk = t["checks"][p]
        fails = [f"{x['file']}：" + "；".join(f"{k} {v}" for k, v in x.get("items", {}).items() if not v.startswith("通过"))
                 if x.get("items") else x["file"] for x in chk["baselines"] + chk["jpp"] if not x["ok"]]
        per[p] = {k: r.get(k) for k in ("wrap100", "caliber1", "caliber2", "token_ratio", "calls", "layout_flag",
                                        "glue_lines", "shared_glue_lines", "coordination_share",
                                        "runtime_share_baseline", "time_ratio", "status")} | {
            "jpp_counted": (r.get("jpp") or {}).get("counted"),
            "jpp_impls": [(x["file"].split("/")[-1] + ("（冻结）" if x.get("redispatch") else "")
                           + ("" if x.get("ok") is None else ("" if x["ok"] else "✗")), x["counted"])
                          for x in r.get("jpp_impls", [])],
            "passed_only": r.get("passed_only"),
            # 步 25-0：标了 redispatch 的实现不跑验收，行数冻结在迁移前的源文件上
            "skipped": [f"{x['file'].split('/')[-1]}：{x['reason']}" for x in chk.get("skipped", [])],
            "baseline_counted": r.get("baseline_counted_median"),
            "baselines": [(b["file"].split("/")[-1] + ("" if b.get("ok") is None else ("" if b["ok"] else "✗")),
                           b["counted"]) for b in r["baselines"]],
            "baseline_minutes": [b["minutes"] for b in r["baselines"]],
            "baseline_all_pass": all(x["ok"] for x in chk["baselines"]) and bool(chk["baselines"]),
            "acceptance_failures": fails}
    return per


def item_expr() -> dict:
    """三档（B79、B96）：T1 是验收 1 的判定档（Nature 2026-09-24 确认），(d) 为性质验收；T1-strict 是旧 (d) 口径，
    只报不判；T0 并列。两侧取该档实现的中位数；T1 位置判定用「只计验收全过」（B96）。"""
    doc = measure_expr.measure_all(quiet=True)
    out = {"tiers": {}}
    for tier in ("t1", "t1-strict", "t0"):
        t = doc[tier]
        out["tiers"][tier] = {"summary": t["summary"], "projects": _tier_rows(t)}
    s1 = doc["t1"]["summary"]
    base_fail = sorted({p for tier in ("t0", "t1", "t1-strict") for p, r in out["tiers"][tier]["projects"].items()
                        if r["status"] == "有数" and not r["baseline_all_pass"]})
    if s1.get("status") != "有数":
        return {"status": s1["status"], **out}
    if "passed_only_wrap100_median" not in s1:
        return {"status": "无数：T1 没有任一项目两侧都有验收全过的实现", **out}
    return {"status": "有数", "value": s1["passed_only_wrap100_median"], "position": s1["passed_only_position"],
            "value_all": s1["wrap100_median"], "no_pass_jpp": s1.get("no_pass_jpp", []),
            "baseline_failures": base_fail, "summary": s1,
            "reference": "T1 折行归一行数比对 9–20×（判定档，Nature 2026-09-24 确认按 T1 判；B96 位置用只计验收全过）；"
                         "T1-strict 只报不判；T0 对 LMQL 2.7–4.3× 并列",
            "source": "scripts/measure_expr.py（measure_all）；scripts/t1_check.py；scripts/t1_cert.py；probes/*/measure.toml",
            **out}


# —— 2 真机新题已决出口率 ——

V6 = ROOT.parent / "评估" / "2026-09-24-V6试用线"
V7 = ROOT.parent / "评估" / "2026-09-24-V7固定序"
# 等级（报告 `exits[].grade`，`20` §3.4 LineGrade 名）→ 仪表的分档
GRADE_BIN = {"Certified": "正式", "Form": "题式", "Trial": "试用", "Class": "借线",
             "Provisional": "其他", "Fixture": "其他", "Cold": "无线"}


def live_runs():
    """真机账本与它的源程序：试写三次（`live-ledger*.jsonl` ↔ 同名目录的 `<目录>.jpp` / `<目录>-flat.jpp`），
    加 V6、V7 目录 `runs.json` 登记的运行（V7 由步 31-1b 登记）。"""
    runs = []
    for f in sorted(glob.glob(str(TRIAL / "*" / "live-ledger*.jsonl"))):
        d = pathlib.Path(f).parent
        src = d / (d.name + ("-flat" if "flat" in pathlib.Path(f).name else "") + ".jpp")
        runs.append((pathlib.Path(f), src))
    for d in (V6, V7):
        reg = d / "runs.json"
        if reg.exists():
            for r in json.loads(reg.read_text(encoding="utf-8")):
                runs.append((d / r["ledger"], (d / r["source"]).resolve()))
    return runs


def ledger_has_lines(led: pathlib.Path) -> bool:
    """账本记有命中的校准记录：这次真机运行带着线跑（步 31-1b 的分母口径）。账本 v3 起命中记录是
    `CalibUsed` 条目（步 18a）；v2 在头行 `calib_used`。"""
    try:
        lines = led.read_text(encoding="utf-8").splitlines()
        head = json.loads(lines[0])
    except (OSError, ValueError, IndexError):
        return False
    if head.get("version") == 2:
        return bool(head.get("calib_used"))
    return any('"CalibUsed"' in l for l in lines[1:])


def item_live_decided() -> dict:
    """只凭账本重放每个真机账本（0 调用，校准记录由账本的 `CalibUsed` 条目补回），从重放报告的 `exits`
    （步 20f 逐出口记线等级）数已决出口（act / ignore / pick / at），按等级分。出口不进账本（`20` §3.7(1)），
    所以这里是重放重算，不是读账本字段。"""
    per, exits, decided, ex_l, dec_l = {}, 0, 0, 0, 0
    by_grade = {k: 0 for k in ("正式", "题式", "试用", "借线", "其他")}
    for led, src in live_runs():
        out = ROOT / "target" / "_dashboard_live.json"
        p = sh([str(JPP), "run", src.name, "--replay", str(led), "--output", str(out)], cwd=src.parent)
        name = os.path.relpath(led, ROOT.parent)
        if p.returncode != 0 or not out.exists():
            per[name] = {"error": (p.stderr or p.stdout).strip()[-300:]}
            continue
        rep = json.loads(out.read_text(encoding="utf-8"))
        out.unlink()
        ex = rep.get("exits", [])
        d = [e for e in ex if not e["exit"].startswith("unsure")]
        g = {}
        for e in d:
            b = GRADE_BIN[e["grade"]]
            g[b] = g.get(b, 0) + 1
            by_grade[b] += 1
        lined = ledger_has_lines(led)
        per[name] = {"source": os.path.relpath(src, ROOT.parent), "exits": len(ex), "decided": len(d),
                     "decided_by_grade": g, "replay_calls": rep["cost"]["calls"],
                     "releases": sum(1 for e in d if e.get("releases")), "with_lines": lined}
        exits += len(ex)
        decided += len(d)
        if lined:
            ex_l += len(ex)
            dec_l += len(d)
    if not per:
        return {"status": "无数：没有真机账本", "value": 0.0}
    bad = [k for k, v in per.items() if "error" in v or v["replay_calls"]]
    if bad:
        return {"status": "无数：重放失败或重放发了新调用（" + "、".join(bad) + "）", "runs": per}
    if decided == 0:
        return {"status": "无数：真机出口全部未决（无可用的线）", "value": 0.0, "decided": 0, "exits": exits,
                "by_grade": by_grade, "runs": per}
    # 步 31-1b 口径：主读数只数带线运行（账本记有命中的校准记录）的出口；冷线运行（试写三次）没有线，
    # 量的不是线库的覆盖，只作并列的全部运行读数
    return {"status": "有数（真机账本重放）", "value": round(dec_l / ex_l, 3) if ex_l else 0.0,
            "decided": dec_l, "exits": ex_l, "value_all": round(decided / exits, 3), "decided_all": decided,
            "exits_all": exits, "by_grade": by_grade, "runs": per,
            "note": "主读数的分母只含带线运行（账本记有命中的校准记录）的全部 cut 出口；全部运行（含冷线试写）并列为 "
                    "value_all；by_grade 只数已决出口（全部运行）。试用线出口可路由、不放行不可逆 do（B72）",
            "source": "真机账本 --replay 的报告 exits（步 20f）"}


# —— 3 深度曲线 ——

def item_depth() -> dict:
    hops, parents = [], 0
    probes = [("examples/iterate.jpp", "examples/fixtures/iterate.json", ROOT),
              ("refund.jpp", "fixture.json", TRIAL / "refund")]
    for src, fx, cwd in probes:
        led = ROOT / "target" / "_dashboard_depth.jsonl"
        sh([str(JPP), "run", src, "--fixtures", fx, "--ledger-out", str(led), "--output", str(led) + ".json"], cwd=cwd)
        if led.exists():
            for x in led.read_text(encoding="utf-8").splitlines()[1:]:
                j = json.loads(x).get("entry", {}).get("Judge")
                if j:
                    hops.append(j.get("hop", 0))
                    parents += bool(j.get("parents"))
            led.unlink()
            os.unlink(str(led) + ".json")
    if hops and max(hops) == 0 and parents == 0:
        return {"status": "无数：17a 未落，账本 parents/hop 恒空", "judge_entries": len(hops),
                "note": "iterate（3 层）与试写 refund（链式 2 层）的固定观察账本里 hop 全为 0、parents 全空",
                "reference": "与 00-目标与动机 §四 误差累积（每单元九成准、十跳全对约 35%）对照，17a 后取数"}
    dist = {h: hops.count(h) for h in sorted(set(hops))}
    out = {"status": "有数（固定观察）", "hop_distribution": dist,
           "note": "hop_distribution 是固定观察的跳数分布，按值级来源计（B84，步 17c）：经普通值、下标、content() 传递的"
                   "依赖都算，控制流不算。live 是真机账本的逐跳曲线（B103，步 31-0c）"}
    live = depth_live()
    out["live"] = live
    if any(r.get("hops") for r in live.values()):
        out["status"] = "有数（真机曲线 + 固定观察跳数分布）"
    return out


# 步 31-0c（B109、B103）：真机深度曲线。登记表与 V6/V7 的 runs.json 同格式，另带真值来源
DEPTH_RUNS = ROOT.parent / "评估" / "2026-09-25-31-0c深度曲线" / "runs.json"
HEX24 = re.compile(r"^[0-9a-f]{24}$")


def _exit_cause(ex: str) -> list[str]:
    m = re.match(r"unsure\((.*)\)$", ex)
    return m.group(1).split("|") if m else []


def _state_of_items(led: pathlib.Path, keys: set[str], materials: pathlib.Path) -> dict:
    """材料文本 → 状态哈希：用 `jpp calib-import --from-ledger --list-out --materials`（B88，与 `state(mat(文本))`
    同算法），不在这里重写哈希。只对得上账本里出现过的状态。"""
    text_to_state = {}
    tmp = ROOT / "target" / "_dashboard_list.jsonl"
    for k in sorted(keys):
        p = sh([str(JPP), "calib-import", "--from-ledger", str(led), "--key", k, "--list-out", str(tmp),
                "--materials", str(materials)])
        if p.returncode != 0 or not tmp.exists():
            continue
        for line in tmp.read_text(encoding="utf-8").splitlines()[1:]:
            r = json.loads(line)
            if "material" in r:
                text_to_state[r["material"]] = r["item"]
        tmp.unlink()
    return text_to_state


def _truth(files: list, base: pathlib.Path, items: dict, text_to_state: dict) -> dict:
    """标签文件 → {(键, 状态哈希): 真值}。标签 `item` 是状态哈希时直接用；否则经 items（标签 item → 材料文本）
    与材料表对回（B88）。`ambiguous` 不作真值。"""
    t = {}
    for f in files:
        for line in (base / f).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if r.get("label") not in (True, False) or "key" not in r:
                continue
            it = r["item"]
            st = it if HEX24.match(it) else text_to_state.get(items.get(it, r.get("text")))
            if st:
                t[(r["key"], st)] = r["label"]
    return t


def _join(entries: list, exits: list):
    """出口 → 账本判断条目，按站点内先后对接（`过程记录/工程-步31-0c.md` §1.3、§2.2）。返回 (对, 不符项)。
    核对：每站点数目相等、键相同；是非题同键里 act 的读数都高于 ignore，band 未决的读数落在两者之间。
    这是相对核对，不重算线（不重写 B104 的 δ 取法）；预注册写的是按线的带判，偏差见过程记录 §2.2。"""
    by_site_e, by_site_x = {}, {}
    for j in entries:
        by_site_e.setdefault(j["jkey"]["site"], []).append(j)
    for x in exits:
        by_site_x.setdefault(x["site"], []).append(x)
    bad = [s for s in set(by_site_e) | set(by_site_x) if len(by_site_e.get(s, [])) != len(by_site_x.get(s, []))]
    pairs = [(j, x) for s in by_site_e for j, x in zip(by_site_e[s], by_site_x.get(s, []))]
    bad += [j["key"] for j, x in pairs if x.get("key") not in (None, j["calib_ref"]["declared"])]
    for k in {j["calib_ref"]["declared"] for j, _ in pairs}:
        ps = {"act": [], "ignore": [], "band": []}
        for j, x in pairs:
            p_ = j["answer"].get("Noul")
            if j["calib_ref"]["declared"] != k or p_ is None:
                continue
            c = "band" if "band" in _exit_cause(x["exit"]) else x["exit"]
            if c in ps:
                ps[c].append(p_)
        a, i, b = ps["act"], ps["ignore"], ps["band"]
        if (a and i and min(a) <= max(i)) or (b and a and min(a) <= max(b)) or (b and i and max(i) >= min(b)):
            bad.append(f"读数顺序:{k}")
    return pairs, bad


def depth_live() -> dict:
    """真机账本的逐跳曲线（B103 口径，`过程记录/工程-步31-0c.md` §1.3）：跳取账本 `hop` 原样；出口取只凭账本重放的
    报告 `exits`，按同一站点内的先后次序对到判断条目上（出口还没有 item，B120 (b) 之后改按 item 对），对接失败即
    报无数；未决率不含 budget/unobserved（单列）；一致率 = 带真值的已决出口里与真值一致的比例，没有即「无数」。"""
    if not DEPTH_RUNS.exists():
        return {}
    base = DEPTH_RUNS.parent
    res = {}
    for reg in json.loads(DEPTH_RUNS.read_text(encoding="utf-8")):
        led = (base / reg["ledger"]).resolve()
        src = (base / reg["source"]).resolve()
        name = os.path.relpath(led, ROOT.parent)
        out = ROOT / "target" / "_dashboard_depth_live.json"
        p = sh([str(JPP), "run", src.name, "--replay", str(led), "--output", str(out)], cwd=src.parent)
        if p.returncode != 0 or not out.exists():
            res[name] = {"status": "无数：重放失败", "error": (p.stderr or p.stdout).strip()[-300:]}
            continue
        rep = json.loads(out.read_text(encoding="utf-8"))
        out.unlink()
        if rep["cost"]["calls"]:
            res[name] = {"status": "无数：重放发了新调用"}
            continue
        entries = []
        for line in led.read_text(encoding="utf-8").splitlines()[1:]:
            j = json.loads(line).get("entry", {}).get("Judge")
            if j and j.get("jkey"):
                entries.append(j)
        pairs, bad = _join(entries, rep.get("exits", []))
        if bad:
            res[name] = {"status": "无数：出口对不上账本条目", "mismatch": [str(b) for b in bad][:10]}
            continue
        keys = {j["calib_ref"]["declared"] for j, _ in pairs}
        items = json.loads((base / reg["items"]).read_text(encoding="utf-8")) if reg.get("items") else {}
        t2s = _state_of_items(led, keys, base / reg["materials"]) if reg.get("materials") else {}
        truth = {"primary": _truth(reg.get("labels", []), base, items, t2s),
                 "extra": _truth(reg.get("labels_extra", []), base, items, t2s)}
        hops = {}
        for j, x in pairs:
            h = hops.setdefault(j["hop"], {"n": 0, "unsure": 0, "budget": 0, "unobserved": 0, "decided": 0,
                                           "keys": set(), "primary": [0, 0, 0], "extra": [0, 0, 0]})
            h["n"] += 1
            h["keys"].add(j["calib_ref"]["declared"])
            cause = _exit_cause(x["exit"])
            if "budget" in cause:
                h["budget"] += 1
            elif "unobserved" in cause:
                h["unobserved"] += 1
            elif cause:
                h["unsure"] += 1
            else:
                h["decided"] += 1
            k = (j["calib_ref"]["declared"], j["jkey"]["state"])
            for which in ("primary", "extra"):
                tv = truth[which].get(k)
                if tv is None:
                    continue
                h[which][0] += 1                                   # 有真值的条目
                if not cause and x["exit"] in ("act", "ignore"):
                    h[which][1] += 1                               # 有真值的已决出口
                    h[which][2] += (x["exit"] == "act") == tv      # 与真值一致
        rows = {}
        for hp in sorted(hops):
            h = hops[hp]
            row = {"n": h["n"], "keys": sorted(h["keys"]), "unsure": h["unsure"],
                   "unsure_rate": round(h["unsure"] / h["n"], 3), "budget": h["budget"],
                   "unobserved": h["unobserved"], "decided": h["decided"]}
            for which in ("primary", "extra"):
                cov, dec, agr = h[which]
                row[which] = {"truth_coverage": f"{cov}/{h['n']}", "agree": f"{agr}/{dec}",
                              "agree_rate": round(agr / dec, 3) if dec else "无数"}
            rows[hp] = row
        res[name] = {"status": "有数", "source": os.path.relpath(src, ROOT.parent), "note": reg.get("note", ""),
                     "labels": reg.get("labels", []), "labels_extra": reg.get("labels_extra", []), "hops": rows}
    return res


# —— 4 换画像通过数 ——

def item_profile_swap() -> dict:
    """验收 3 第一级（步 15g 起）：跑 crates/jpp/tests/profile_swap 的 (B) 节，只数通过且未忽略的 h1–h8。
    一条假设的全部测试（h<N>_*）都通过、都不带 ignore 才计 1；不从源码里数 H 字样（那样读者未建也会计数）。"""
    cmd = ["cargo", "test", "-p", "jpp", "--test", "profile_swap", "--offline", "--", "h", "--test-threads=2"]
    p = sh(cmd)
    rows = re.findall(r"^test (h([1-8])_\S*) \.\.\. (ok|FAILED|ignored)", p.stdout, re.M)
    by_h: dict = {}
    for name, h, st in rows:
        by_h.setdefault("H" + h, []).append((name, st))
    passed = sorted(h for h, ts in by_h.items() if ts and all(st == "ok" for _, st in ts))
    ignored = {h: [n for n, st in ts if st == "ignored"] for h, ts in by_h.items() if any(st == "ignored" for _, st in ts)}
    failed = sorted(h for h, ts in by_h.items() if any(st == "FAILED" for _, st in ts))
    ok = p.returncode == 0 and len(by_h) == 8
    n = len(passed) if ok else 0
    return {"status": "有数" if ok else "无数", "value": f"{n}/8", "covered": passed, "ignored": ignored, "failed": failed,
            "second_level": "替身判断器 stub 已接（步 15g，程序不改在第二判断器上跑）；真机第二判断器无",
            "test": {"cmd": " ".join(cmd), "ok": ok, "tail": (p.stdout + p.stderr).strip()[-300:] if not ok else ""},
            "note": "证据 crates/jpp/tests/profile_swap（步 15g）；ignore 的测试写明降级落在哪一步"}


# —— 5 等价写法调用比 ——

def item_equiv() -> dict:
    os.environ.pop("JPP_CI_UPDATE_BASELINE", None)
    import equiv_pairs
    got = {}
    equiv_pairs.report_ratio = lambda name, ratio, detail: got.__setitem__(name.replace("equiv_pairs_", ""), round(ratio, 2))
    equiv_pairs.CLI = [str(JPP)]
    equiv_pairs.main()
    return {"status": "有数", "value": got, "reference": "13b 后失败线 1.5；第一版允许 14 / 2 / 1.4",
            "source": "scripts/equiv_pairs.py（tests/equiv_pairs/manifest.json）"}


# —— 6 能力对照 ——

def item_capabilities(equiv: dict, replay: dict, swap: dict) -> dict:
    with open(ROOT / "probes" / "measure.toml", "rb") as fh:
        caps = tomllib.load(fh)["capabilities"]
    golden = cargo_test("jpp", "golden")
    failopen = cargo_test("jpp", "failopen")
    judged = {
        "A3": (golden["ok"] and replay.get("diff_groups") == 0,
               f"golden {'通过' if golden['ok'] else '失败'}；只凭账本重放差异 {replay.get('diff_groups')} 组"),
        "A4": (failopen["ok"], f"failopen {failopen['passed']} 通过 / {failopen['failed']} 失败"),
        # 步 13b（1f51319e）后慢写法也被合并成一次批发，比值 14 → 1；判据随之改为「≤ 13b 失败线 1.5」
        # （31-0b 过程记录 §三：旧判据「> 1」在 13b 后把合并成功判成未通过）
        "A5": (isinstance(equiv.get("value"), dict) and 0 < equiv["value"].get("sieve_batch", 0) <= 1.5,
               f"sieve_batch 慢写法 / 快写法 = {equiv.get('value', {}).get('sieve_batch')}（≤ 1.5：两种写法都合并成一次批发）"),
        "A7": (swap.get("value") == "8/8", f"换画像 {swap.get('value')}（8/8 才算通过）"),
        "B5": (golden["ok"], "金样 error-question-field-typo 报 W-diag-mention-scope 与 E-field"),
    }
    rows, passed = {}, 0
    for k, c in caps.items():
        if c["status"] == "measured" and k in judged:
            ok, ev = judged[k]
            passed += ok
            rows[k] = {"name": c["name"], "result": "通过" if ok else "未通过", "evidence": ev}
        else:
            rows[k] = {"name": c["name"], "result": "未测", "why": c.get("why", "")}
    return {"status": "有数", "value": f"{passed}/12", "items": rows,
            "tests": {"golden": golden, "failopen": failopen},
            "note": "只判 J++ 一侧能否做到；基线侧的实现方式与代价在 probes/measure.toml，步 31 逐项对基线测"}


# —— 7 重放一致 ——

def item_replay(scan: dict) -> dict:
    return {"status": "有数", "value": {"groups": scan["groups"], "w_header_groups": scan["w_header_groups"],
                                        "diff_groups": scan["diff_groups"], "first_run_failures": len(scan["errors"])},
            "reference": "目标 0；第一版允许 11（修复记录 §六，组的构成不同，见 note）",
            "note": "scripts/replay_scan.py 复建修复记录 §六 的扫查；本仓库组的构成：金样 24（去掉预期报错与续跑）、"
                    "探针 4、winnow 与 folio 各三种 scope 校准目录 6、topic-relevance 三种 scope 校准目录 3、续接后账本 1（B83，步 7c），合 35 组；"
                    "比原记录多出 winnow-batched 与 winnow×scope 三组，所以 W-header 组数不与 11 直接相减",
            "w_header": [r["name"] for r in scan["rows"] if r.get("w_header")]}


def reading_key(stem: str):
    """`2026-09-24` → (日期, 1)；`2026-09-24-2` → (日期, 2)。序号按整数比较。"""
    m = re.fullmatch(r"(\d{4}-\d{2}-\d{2})(?:-(\d+))?", stem)
    return (m.group(1), int(m.group(2) or 1)) if m else None


def readings():
    return sorted((reading_key(f.stem), f) for f in OUT.glob("*.json") if reading_key(f.stem))


def target_stem(today: str, overwrite: bool) -> str:
    seqs = [k[1] for k, _ in readings() if k[0] == today]
    if overwrite and seqs:
        n = max(seqs)
    else:
        n = max(seqs) + 1 if seqs else 1
    return today if n == 1 else f"{today}-{n}"


def previous(exclude_stem: str | None = None):
    files = [f for _, f in readings() if f.stem != exclude_stem]
    return json.loads(files[-1].read_text(encoding="utf-8")) if files else None


def headline(k: str, v: dict) -> str:
    if k == "expr" and v.get("status") == "有数" and "tiers" in v:
        def one(s, band):
            calls = " / ".join(f"{p} {r}" for p, r in s["calls_ratio"].items())
            return (f"折行归一中位 {s['wrap100_median']}×（{s['position']}，参考带 {band}×）；原始行数 {s['caliber1_median']}×、"
                    f"token {s['token_ratio_median']}×、口径二 {s['caliber2_median']}×；调用数比 {calls}")
        t1, t0 = v["tiers"]["t1"]["summary"], v["tiers"]["t0"]["summary"]
        if "passed_only_wrap100_median" not in t1:   # 31-1b 之前的读数格式
            return (f"**T1** {one(t1, '9–20')}；**T0** {one(t0, '2.7–4.3')}；{t1['projects']} 个项目，两侧多实现中位")
        st = v["tiers"].get("t1-strict", {}).get("summary", {})
        npj = t1.get("no_pass_jpp", [])
        return (f"**T1**（(d) 性质验收）只计验收全过 {t1['passed_only_wrap100_median']}×（{t1['passed_only_position']}，"
                f"参考带 9–20×，{t1['passed_only_projects']} 个项目）、全部实现 {t1['wrap100_median']}×；"
                f"J++ 侧无通过实现的项目 {len(npj)}/{t1['projects']}（{'、'.join(npj) or '无'}）；"
                f"**T1-strict**（旧 (d)，只报不判）全部实现 {st.get('wrap100_median', '—')}×；"
                f"**T0** {one(t0, '2.7–4.3')}")
    if k == "expr" and v.get("status") == "有数":
        s = v["summary"]
        if "wrap100_median" not in s:  # 31-0b 之前的读数格式
            return (f"口径一中位：行数 {s['caliber1_median']}×、token {s['token_ratio_median']}×（{s['position']}，参考带 9–20×；"
                    f"行数对排版敏感，两数并看）；口径二 {s['caliber2_median']}×（参考带 3.5–4.6×）；{s['projects']} 个项目，弱受控")
        calls = " / ".join(f"{p} {r}" for p, r in s["calls_ratio"].items())
        return (f"折行归一中位 {s['wrap100_median']}×（{s['position']}，参考带 9–20×）；并列：原始行数 {s['caliber1_median']}×、"
                f"token {s['token_ratio_median']}×、口径二 {s['caliber2_median']}×（参考带 3.5–4.6×）；"
                f"调用数比 {calls}；{s['projects']} 个项目，T0，弱受控")
    if k == "live_decided":
        if "value_all" in v:
            return (f"带线运行 {v['value']}（{v['decided']}/{v['exits']}）；全部运行 {v['value_all']}"
                    f"（{v['decided_all']}/{v['exits_all']}）")
        return f"{v.get('value', '—')}（{v.get('decided', '—')}/{v.get('exits', '—')}）"
    if k == "depth":
        d = v.get("hop_distribution")
        s = ("固定观察 hop " + "、".join(f"{h}:{n}" for h, n in d.items())) if d else "—"
        for name, r in (v.get("live") or {}).items():
            if not r.get("hops"):
                s += f"；真机 {pathlib.Path(name).name}：{r.get('status')}"
                continue
            s += f"；真机 {pathlib.Path(name).name}：" + "、".join(
                f"hop {h} n{x['n']} 未决 {x['unsure']}/{x['n']} 一致 {x['primary']['agree']}"
                f"（补充 {x['extra']['agree']}）" for h, x in r["hops"].items())
        return s
    if k == "equiv":
        return " / ".join(f"{a} {b}" for a, b in v["value"].items())
    if k == "replay":
        x = v["value"]
        return f"W-header {x['w_header_groups']}/{x['groups']} 组；差异 {x['diff_groups']} 组"
    return str(v.get("value", "—"))


NAMES = {"expr": "表达量比", "live_decided": "真机新题已决出口率", "depth": "深度曲线", "profile_swap": "换画像通过数",
         "equiv": "等价写法调用比", "capabilities": "能力对照", "replay": "重放一致"}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--overwrite", action="store_true", help="改写当天最新一份读数，不新开序号")
    ap.add_argument("--out", help="写到这个路径前缀（不加扩展名），不进 评估/仪表/；供确定性核对")
    a = ap.parse_args()
    # 先按当前源码重建：S 步之后仪表必须量新代码，不量旧二进制
    b = sh(["cargo", "build", "-p", "jpp", "--offline", "-q"])
    if b.returncode != 0 or not JPP.exists():
        sys.exit("cargo build -p jpp 失败：" + b.stderr[-800:])
    today = datetime.date.today().isoformat()
    stem = target_stem(today, a.overwrite)
    prev = previous(exclude_stem=stem if not a.out else None)
    scan = replay_scan.scan()
    equiv = item_equiv()
    swap = item_profile_swap()
    items = {"expr": item_expr(), "live_decided": item_live_decided(), "depth": item_depth(), "profile_swap": swap,
             "equiv": equiv, "capabilities": item_capabilities(equiv, scan, swap), "replay": item_replay(scan)}
    head = sh(["git", "rev-parse", "--short", "HEAD"]).stdout.strip()
    doc = {"date": today, "reading": stem, "time": datetime.datetime.now().astimezone().isoformat(timespec="minutes"), "commit": head,
           "spec": "地基/附注/2026-09-24-评估①裁定.md §六·5", "items": items,
           "has_number": sum(1 for v in items.values() if v["status"].startswith("有数")),
           "previous": prev and {"date": prev["date"], "commit": prev["commit"],
                                 "headlines": {k: headline(k, v) for k, v in prev["items"].items()}}}
    base = pathlib.Path(a.out) if a.out else OUT / stem
    base.parent.mkdir(parents=True, exist_ok=True)
    base.with_suffix(".json").write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    lines = [f"# 验收仪表 {stem}", "",
             f"提交 `{head}`；口径 `{doc['spec']}`；由 `地基/rust-jpp/scripts/dashboard.py` 生成，全文见同名 `.json`。"
             f"七项中 {doc['has_number']} 项有数。", "",
             "| 项 | 状态 | 读数 | 上一次 |", "|---|---|---|---|"]
    for k, v in items.items():
        pv = doc["previous"]["headlines"].get(k, "—") if doc["previous"] else "（首次）"
        lines.append(f"| {NAMES[k]} | {v['status']} | {headline(k, v)} | {pv} |")
    e = items["expr"]
    for tier, title in (("t1", "T1 完整职责任务书（判定档，对 9–20×；(d) 性质验收，B96）"),
                        ("t1-strict", "T1-strict 旧 (d) 逐位口径（只报不判，B96）"),
                        ("t0", "T0 最小任务书（并列，对 LMQL 2.7–4.3×）")):
        tt = e.get("tiers", {}).get(tier)
        if not tt:
            continue
        lines += ["", f"**表达量比逐项目 · {title}**（两侧取该档全部实现的中位数；行数 = 非注释非空、去数据行；"
                  "折行归一 = 100 列、CJK 双宽；口径二 J++ 侧只加生产必需胶水，夹具脚本不计，B78）", "",
                  "| 项目 | J++ 实现（行；✗ 验收未过） | 基线实现（行） | 折行归一 | 只计验收全过 | 原始行数 | token | 调用数比 | 口径二 | 排版差异 |",
                  "|---|---|---|---|---|---|---|---|---|---|"]
        for p, r in tt["projects"].items():
            c = r["calls"]
            po = r.get("passed_only") or {}
            pov = po.get("wrap100") if po.get("status") == "有数" else (po.get("status") or "—")
            lines.append(f"| {p} | {'、'.join(f'{a} {b}' for a, b in r['jpp_impls'])} | "
                         f"{'、'.join(f'{a} {b}' for a, b in r['baselines'])} | {r['wrap100']} | {pov} | {r['caliber1']} | "
                         f"{r['token_ratio']} | {c['ratio'] if c else '无数'} | {r['caliber2']} | "
                         f"{'是' if r['layout_flag'] else '否'} |")
        s = tt["summary"]
        notes = []
        frozen = [x for r in tt["projects"].values() for x in r.get("skipped", [])]
        if frozen:
            notes.append(f"冻结 {len(frozen)} 份（不跑验收，行数取迁移前的源文件）：" + "；".join(frozen) + "。")
        if s.get("status") == "有数":
            notes.append(f"中位：折行归一 {s['wrap100_median']}×（{s['position']}），原始行数 {s['caliber1_median']}×，"
                         f"token {s['token_ratio_median']}×，口径二 {s['caliber2_median']}×。")
            if "passed_only_wrap100_median" in s:
                notes.append(f"只计验收全过（B96；冻结实现按冻结前记录）：折行归一中位 {s['passed_only_wrap100_median']}×"
                             f"（{s['passed_only_position']}，{s['passed_only_projects']} 个项目）、原始行数 "
                             f"{s['passed_only_caliber1_median']}×。")
            notes.append(f"J++ 侧无通过实现的项目：{len(s.get('no_pass_jpp', []))}/{s['projects']}"
                         f"（{'、'.join(s.get('no_pass_jpp', [])) or '无'}）；基线侧无通过实现的项目："
                         f"{'、'.join(s.get('no_pass_baseline', [])) or '无'}。{s.get('judgement', '')}。")
            if s.get("overturn_b80_2"):
                notes.append("B80 推翻条件 (2)（折行归一与 token 相差 > 2×）：" + "、".join(s["overturn_b80_2"]) + "。")
        fails = [f"{p}：{f}" for p, r in tt["projects"].items() for f in r["acceptance_failures"]]
        if fails:
            notes.append("验收未过的实现（照实报，不从计数里剔除）：" + "；".join(fails) + "。")
        lines += [""] + notes
    dl = items["depth"].get("live") or {}
    if dl:
        lines += ["", "**深度曲线 · 真机逐跳**（B103，步 31-0c；跳取账本 `hop` 原样；未决率不含 budget/unobserved；"
                  "一致率 = 带真值的已决出口中与真值一致的比例；主真值为登记表 `labels`，补充真值 `labels_extra` 单列、不并入）", "",
                  "| 账本 | hop | 键 | n | 未决 | 未决率 | budget | unobserved | 已决 | 主真值覆盖 | 主一致 | 主一致率 | 补充真值覆盖 | 补充一致 | 补充一致率 |",
                  "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
        for name, r in dl.items():
            if not r.get("hops"):
                lines.append(f"| {name} | {r.get('status')} |" + " |" * 13)
                continue
            for h, x in r["hops"].items():
                lines.append(f"| {name} | {h} | {'、'.join(x['keys'])} | {x['n']} | {x['unsure']} | {x['unsure_rate']} | "
                             f"{x['budget']} | {x['unobserved']} | {x['decided']} | {x['primary']['truth_coverage']} | "
                             f"{x['primary']['agree']} | {x['primary']['agree_rate']} | {x['extra']['truth_coverage']} | "
                             f"{x['extra']['agree']} | {x['extra']['agree_rate']} |")
        lines += ["", "登记表 `地基/评估/2026-09-25-31-0c深度曲线/runs.json`：" + "；".join(
            f"{pathlib.Path(n).name}：{r.get('note', '')}" for n, r in dl.items())]
    c = items["capabilities"]["items"]
    lines += ["", "**能力对照**：" + "；".join(f"{k} {v['name']} {v['result']}" for k, v in c.items())]
    base.with_suffix(".md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
