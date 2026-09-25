#!/usr/bin/env python3
"""表达量比：J++ 程序对手写基线的行数比，主读数为折行归一行数比，原始行数、token、调用数并列（验收 1，A-6、B78、B80）。

依据：`地基/附注/2026-09-24-评估①裁定.md` §六·2（参考系与判定规则）、§六·3（计数口径与基线规程）；
`地基/附注/2026-09-24-仪表读数1裁定.md` §一（B78 口径更正）、§二（B80 单位并列与调用数比）；
参考带出处见 `地基/研究/07-语言倍率参考系-出处核实.md`、`地基/研究/08-表达力换算与DSL实测倍率.md` §七。

每个项目一份 `probes/<项目>/measure.toml`：头部写参考系，[jpp] 与 [baseline] 写文件、行分类与完成时间。
行分类用行号区间手工标注（可审计：改一行分类就是改一行 toml），脚本只做计数与核对。

计数（两边同一套规则）：
  代码行      非空、非注释行（`.jpp` 去掉 `//` 行；`.py` 去掉 `#` 行与文档字符串，同 cloc 口径）。
  数据行      一行的全部内容是字面输入材料、候选列表、标签集，或只做装载（toml 的 data 区间），不计。
  计入行      代码行 − 数据行。口径一的分子分母都用它。
  协调行      计入行中服务于八条缝之一的行（toml 的 coordination 区间），只报占比，不改计数。
  运行时职责  基线计入行中为承担运行时职责而写的行（批处理、阈值、三态分支、重放、记账、taint、重试；
              toml 的 runtime 区间），只报占比，供能力对照 B2（三态分流）取数，不从任何口径里扣（B78）。
  胶水        J++ 侧生产运行必需、使用者另写的代码（[[jpp.glue]]）：CLI 或 Session 调用、动作登记（do 的注册）、
              calib-import 调用，只进口径二。标 `shared = true` 的项（夹具脚本：两侧共用的测试设施，
              基线经不计行的 SDK 读的是同一份夹具）只报不计（B78）。
  折行归一    计入行逐行按 100 列折行（CJK 与全角字符按双宽，`max(1, ceil(宽/100))`）求和（B80）。

比值（基线侧在两个口径里都全计，B78；取多份基线实现的中位数）：
  折行归一比 = 基线折行归一行 / J++ 折行归一行      主读数，位置判定用它（B80：参考带的单位是按常规格式化的物理行）
  口径一     = 基线计入行 / J++ 计入行              原始行数比，并列报告
  口径二     = 基线计入行 / (J++ 计入行 + 非共用胶水行)
  token 比   = 与口径一同分子分母，只数计入行上的 token，作排版稳健性核对
  调用数比   = 固定观察下基线调用数 / J++ 调用数，逐项目报，不取中位数（方向不一致时中位数抹掉信息）
  排版差异   = 原始行数比与 token 比相差超过 1.5 倍（大比小）时置 `layout_flag`
参考带：折行归一与口径一对 9–20（专用语言档，只算专用语言文本）；口径二对 3.5–4.6（计入生成器或胶水代码）。

两档三口径（B79、B96）：T0 最小任务书；T1 完整职责任务书（判定档，(d) 为性质验收）；`t1-strict` 是步 31-1 的
旧 T1 基线（(d) 按 `calib-import` 逐位复刻），只报不判，J++ 侧与 T1 同一批程序。
「全部实现」与「只计验收全过」两个中位并列（B96）：后者两侧只取验收全过的实现（冻结不跑的实现按 measure.toml 的
`frozen_ok`，即冻结前的记录），某项目任一侧没有通过的实现即不进后者；T1 的位置判定用后者。
「J++ 侧无通过实现的项目」单列。

位置（§六·2 判定规则，B80 改用折行归一中位数）：< 9 低于该档，9–20 达到该档，> 20 高于该档；
口径二中位数低于其带下沿、而口径一中位数不低于其带下沿时，另标「胶水代码把倍率吃掉了」（只有这时低位才归因于胶水）。
第一轮弱受控，只报位置不判通过。

用法：
  measure_expr.py                      全部项目，打印简表
  measure_expr.py --json out.json      同时写机读结果（dashboard.py 用）
  measure_expr.py --check <项目>       以固定观察跑该项目全部基线，与 expected.json 比较（基线验收）
  measure_expr.py --regen-expected <项目>|all   跑 J++ 固定观察，按投影写 expected.json
  measure_expr.py --verify-jpp         J++ 固定观察输出投影后是否仍等于 expected.json
"""
from __future__ import annotations

import argparse
import ast
import io
import json
import math
import os
import re
import statistics
import subprocess
import sys
import tokenize

from _baseline import ROOT

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

PROBES = ROOT / "probes"
SDK = PROBES / "_baseline"
JPP = ROOT / "target" / "debug" / "jpp"
TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|\w+|==|!=|<=|>=|=>|->|&&|\|\||\*\*|//|[^\s\w]')

sys.path.insert(0, str(SDK))
from accept import PROJECTIONS  # noqa: E402


def spans(spec) -> set:
    """"6,33,40-48" → {6, 33, 40, …, 48}；空串或 None → 空集。"""
    out = set()
    for part in str(spec or "").replace(" ", "").split(","):
        if not part:
            continue
        a, _, b = part.partition("-")
        out.update(range(int(a), int(b or a) + 1))
    return out


def code_lines(path) -> dict:
    """{行号: 文本}：非空、非注释行。"""
    text = path.read_text(encoding="utf-8")
    raw = text.splitlines()
    if path.suffix == ".py":
        doc = set()
        tree = ast.parse(text)
        for node in ast.walk(tree):
            body = getattr(node, "body", None)
            if isinstance(node, (ast.Module, ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) and body:
                first = body[0]
                if isinstance(first, ast.Expr) and isinstance(getattr(first, "value", None), ast.Constant) \
                        and isinstance(first.value.value, str):
                    doc.update(range(first.lineno, first.end_lineno + 1))
        live = set()
        skip = {tokenize.COMMENT, tokenize.NL, tokenize.NEWLINE, tokenize.INDENT, tokenize.DEDENT,
                tokenize.ENCODING, tokenize.ENDMARKER}
        for tok in tokenize.tokenize(io.BytesIO(text.encode("utf-8")).readline):
            if tok.type not in skip:
                live.update(range(tok.start[0], tok.end[0] + 1))
        return {i: raw[i - 1] for i in sorted(live - doc)}
    return {i + 1: l for i, l in enumerate(raw) if l.strip() and not l.strip().startswith("//")}


def width(line: str) -> int:
    """显示宽度：CJK 与全角字符（码点 > U+2E80）按 2 列，其余按 1 列；行尾空白不计。
    与 `地基/评估/2026-09-24-仪表读数1诊断-取数/wrap.py` 同一函数。"""
    return sum(2 if ord(ch) > 0x2E80 else 1 for ch in line.rstrip())


def wrapped(line: str, cols: int = 100) -> int:
    return max(1, math.ceil(width(line) / cols))


def tokens(line: str, suffix: str) -> int:
    n = 0
    for t in TOKEN.findall(line):
        if (suffix == ".py" and t.startswith("#")) or (suffix != ".py" and t == "//"):
            break
        n += 1
    return n


def count_file(path, data="", coordination="", runtime="") -> dict:
    code = code_lines(path)
    d, c, r = spans(data), spans(coordination), spans(runtime)
    stray = sorted((d | c | r) - set(code))
    counted = {i: t for i, t in code.items() if i not in d}
    return {"file": str(path.relative_to(ROOT.parent)) if ROOT.parent in path.parents else str(path),
            "code": len(code), "data": len(d & set(code)), "counted": len(counted),
            "coordination": len(c & set(counted)), "runtime": len(r & set(counted)),
            "tokens": sum(tokens(t, path.suffix) for t in counted.values()),
            "wrap100": sum(wrapped(t) for t in counted.values()),
            "stray_spec_lines": stray}


def load(project: str):
    path = PROBES / project / "measure.toml"
    with open(path, "rb") as fh:
        return path.parent, tomllib.load(fh)


def projects() -> list:
    return sorted(p.parent.name for p in PROBES.glob("*/measure.toml"))


TIER_BAND = {"t0": "t0_band", "t1": "caliber1_band", "t1-strict": "caliber1_band"}   # T0 对 LMQL 带，T1 对 9–20 带（B79）
TIERS = ("t1", "t1-strict", "t0")


def jpp_tier(tier: str) -> str:
    """J++ 侧实现所在的档：t1-strict 与 t1 用同一批 J++ 程序（只有基线侧的 (d) 口径不同）。"""
    return "t1" if tier == "t1-strict" else tier


def jpp_impls(m: dict, tier: str) -> list:
    """该档的 J++ 实现：[jpp] 是设计者 T0 版；[[jpp.impl]] 列其余（设计者 T1 版、非设计者版），各带 tier。"""
    j = m["jpp"]
    out = []
    if tier == "t0":
        out.append({"source": j["source"], "run_dir": j.get("run_dir", "."), "data": j.get("data"), "input": j.get("input"),
                    "coordination": j.get("coordination"), "author": "设计者", "projection": m["project"]["projection"],
                    "minutes": j.get("minutes")})
    out += [x for x in j.get("impl", []) if x.get("tier", "t0") == jpp_tier(tier)]
    return out


def _ok_map(checks: dict | None) -> dict:
    """check_tier 的结果 → {("jpp"|"base", 源文件相对项目目录): 是否全过}。"""
    out = {}
    for x in (checks or {}).get("jpp", []):
        out[("jpp", x.get("source"))] = bool(x.get("ok"))
    for x in (checks or {}).get("baselines", []):
        out[("base", x.get("file"))] = bool(x.get("ok"))
    return out


def measure(project: str, calls: dict | None = None, tier: str = "t0", checks: dict | None = None) -> dict:
    """calls：{"jpp": n, "baseline": {文件: n}}（固定观察下的调用数）；缺省时不报调用数比。
    tier：t0、t1 或 t1-strict。两侧都取该档全部实现的中位数（A-6 多实现）；给了 checks（check_tier 的结果）时
    另算「只计验收全过」（B96）。"""
    base, m = load(project)
    j = m["jpp"]
    okm = _ok_map(checks)
    jl = []
    for x in jpp_impls(m, tier):
        f = (base / x["source"]).resolve()
        if f.exists():
            c = count_file(f, x.get("data"), x.get("coordination"))
            ok = bool(x.get("frozen_ok")) if x.get("redispatch") else okm.get(("jpp", x["source"]))
            c.update({"author": x.get("author", ""), "minutes": x.get("minutes"),
                      "redispatch": x.get("redispatch"), "ok": ok})
            jl.append(c)
    glue = []
    extra = m.get("t1", {}).get("glue", []) if tier in ("t1", "t1-strict") else []
    for g in j.get("glue", []) + extra:
        shared = bool(g.get("shared", False))
        if "file" in g:
            gc = count_file((base / g["file"]).resolve(), g.get("data"))
            glue.append({"what": g["what"], "file": gc["file"], "lines": gc["counted"], "tokens": gc["tokens"],
                         "shared": shared})
        else:
            glue.append({"what": g["what"], "lines": int(g["lines"]), "tokens": int(g.get("tokens", 0)),
                         "shared": shared})
    glue_lines = sum(g["lines"] for g in glue if not g["shared"])
    shared_lines = sum(g["lines"] for g in glue if g["shared"])
    impls = []
    for b in m["baseline"].get("impl", []):
        if b.get("tier", "t0") != tier:
            continue
        f = (base / m["baseline"]["dir"] / b["file"]).resolve()
        if not f.exists():
            continue
        bc = count_file(f, b.get("data"), b.get("coordination"), b.get("runtime"))
        bc.update({"author": b.get("author", ""), "minutes": b.get("minutes"), "passed": b.get("passed"),
                   "ok": okm.get(("base", b["file"]))})
        impls.append(bc)
    out = {"project": project, "tier": tier, "original": m["project"]["original"], "shape": m["project"]["shape"],
           "control": m["reference"]["control"], "jpp_impls": jl, "glue": glue, "glue_lines": glue_lines,
           "shared_glue_lines": shared_lines,
           "jpp_minutes": j.get("minutes"), "jpp_minutes_note": j.get("minutes_note", ""),
           "baselines": impls}
    if not jl:
        out["status"] = "无数：该档还没有 J++ 实现"
        return out
    jmed = lambda k: statistics.median(x[k] for x in jl)  # noqa: E731
    jc = {"counted": jmed("counted"), "tokens": jmed("tokens"), "wrap100": jmed("wrap100"),
          "coordination": jmed("coordination")}
    out["jpp"] = jc
    if not impls:
        out["status"] = "无数：还没有通过验收的基线"
        return out
    med = lambda k: statistics.median(x[k] for x in impls)  # noqa: E731
    b_lines, b_tok, b_wrap = med("counted"), med("tokens"), med("wrap100")
    c1 = round(b_lines / jc["counted"], 2)
    tr = round(b_tok / jc["tokens"], 2)
    out.update({
        "baseline_counted_median": b_lines,
        "baseline_wrap100_median": b_wrap,
        "wrap100": round(b_wrap / jc["wrap100"], 2),
        "caliber1": c1,
        "caliber2": round(b_lines / (jc["counted"] + glue_lines), 2),
        "token_ratio": tr,
        "layout_flag": max(c1, tr) / min(c1, tr) > 1.5,
        "runtime_share_baseline": round(med("runtime") / b_lines, 2),
        "coordination_share": {"jpp": round(jc["coordination"] / jc["counted"], 2),
                               "baseline": round(med("coordination") / b_lines, 2)},
        "status": "有数",
    })
    out["calls"] = None
    if calls and calls.get("jpp") and calls.get("baseline"):
        bc = [calls["baseline"][x] for x in calls["baseline"] if calls["baseline"][x] is not None]
        if bc:
            out["calls"] = {"jpp": calls["jpp"], "baseline": calls["baseline"],
                            "ratio": round(statistics.median(bc) / calls["jpp"], 2)}
    # B96：只计验收全过的实现（两侧各自过滤；任一侧为空则该项目不进这一口径）
    if checks is not None:
        jp = [x for x in jl if x["ok"]]
        bp = [x for x in impls if x["ok"]]
        out["jpp_pass_count"], out["baseline_pass_count"] = len(jp), len(bp)
        if not jp:
            out["passed_only"] = {"status": "J++ 侧无通过实现"}
        elif not bp:
            out["passed_only"] = {"status": "基线侧无通过实现"}
        else:
            pm = lambda xs, k: statistics.median(x[k] for x in xs)  # noqa: E731
            out["passed_only"] = {"status": "有数", "wrap100": round(pm(bp, "wrap100") / pm(jp, "wrap100"), 2),
                                  "caliber1": round(pm(bp, "counted") / pm(jp, "counted"), 2),
                                  "jpp_counted": pm(jp, "counted"), "baseline_counted": pm(bp, "counted")}
    mins = [x["minutes"] for x in impls if isinstance(x["minutes"], (int, float))]
    jm = [x["minutes"] for x in jl if isinstance(x["minutes"], (int, float))]
    out["time_ratio"] = round(statistics.median(mins) / statistics.median(jm), 2) if mins and jm else None
    return out


def position(value, band) -> str:
    lo, hi = band
    return "低于该档" if value < lo else ("达到该档" if value <= hi else "高于该档")


def summarize(rows: list, tier: str = "t0") -> dict:
    ok = [r for r in rows if r.get("status") == "有数"]
    if not ok:
        return {"status": "无数：没有任一项目的基线通过验收", "tier": tier}
    with open(PROBES / ok[0]["project"] / "measure.toml", "rb") as fh:
        ref = dict(tomllib.load(fh)["reference"])
    band = ref.get(TIER_BAND[tier], ref["caliber1_band"])
    w = statistics.median(r["wrap100"] for r in ok)
    c1 = statistics.median(r["caliber1"] for r in ok)
    c2 = statistics.median(r["caliber2"] for r in ok)
    tm = statistics.median(r["token_ratio"] for r in ok)
    s = {"status": "有数", "tier": tier, "projects": len(ok), "frame": ref["frame"], "control": ref["control"],
         "primary": "wrap100",
         "wrap100_median": round(w, 2), "wrap100_band": band,
         "position": position(w, band),
         "caliber1_median": round(c1, 2), "caliber1_band": band,
         "position_caliber1": position(c1, band),
         "caliber2_median": round(c2, 2), "caliber2_band": ref["caliber2_band"],
         "position_caliber2": position(c2, ref["caliber2_band"]),
         "token_ratio_median": round(tm, 2),
         "calls_ratio": {r["project"]: (r["calls"] or {}).get("ratio") for r in ok},
         "layout_flag": [r["project"] for r in ok if r["layout_flag"]],
         # B80 推翻条件 (2)：折行归一后与 token 比仍相差超过 2 倍的项目，差异不在排版而在语言摩擦
         "overturn_b80_2": [r["project"] for r in ok
                            if max(r["wrap100"], r["token_ratio"]) / min(r["wrap100"], r["token_ratio"]) > 2],
         "coordination_share_median": {
             "jpp": round(statistics.median(r["coordination_share"]["jpp"] for r in ok), 2),
             "baseline": round(statistics.median(r["coordination_share"]["baseline"] for r in ok), 2)},
         "judgement": "只报位置，不判通过（第一轮弱受控；通过判定要等多实现中位数，步 31-1）"}
    if c2 < ref["caliber2_band"][0] and c1 >= band[0]:
        s["glue_flag"] = "胶水代码把倍率吃掉了：J++ 侧胶水行 " + "、".join(
            f"{r['project']} {r['glue_lines']}" for r in ok)
    t = [r["time_ratio"] for r in ok if r["time_ratio"] is not None]
    s["time_ratio_median"] = round(statistics.median(t), 2) if t else None
    s["time_band"] = ref["time_band"]
    bm = sorted(b["minutes"] for r in ok for b in r["baselines"] if isinstance(b["minutes"], (int, float)))
    s["time_note"] = (f"{len(t)} 个项目有两侧完成时间；参考带 {ref['time_band'][0]}–{ref['time_band'][1]}×，不设通过线"
                      if t else "无数：J++ 侧没有逐程序的完成时间（探针首轮未计时；试写两程序合计约 6 分钟、未拆分）"
                      + (f"；基线侧为代理墙钟 {bm[0]}–{bm[-1]} 分钟，与 J++ 侧作者不同，不计比值" if bm else ""))
    if t:
        s["time_position"] = position(s["time_ratio_median"], ref["time_band"])
    if abs(c1 / c2) > 4:
        s["overturn"] = "口径一与口径二差超过 4 倍：按 §六 推翻条件改以口径二为主判定"
    # B96：「只计验收全过」中位与「J++ 侧无通过实现的项目」
    po = [r for r in ok if (r.get("passed_only") or {}).get("status") == "有数"]
    s["no_pass_jpp"] = [r["project"] for r in ok if (r.get("passed_only") or {}).get("status") == "J++ 侧无通过实现"]
    s["no_pass_baseline"] = [r["project"] for r in ok if (r.get("passed_only") or {}).get("status") == "基线侧无通过实现"]
    if po:
        s["passed_only_wrap100_median"] = round(statistics.median(r["passed_only"]["wrap100"] for r in po), 2)
        s["passed_only_caliber1_median"] = round(statistics.median(r["passed_only"]["caliber1"] for r in po), 2)
        s["passed_only_projects"] = len(po)
        s["passed_only_position"] = position(s["passed_only_wrap100_median"], band)
    if tier == "t1":
        s["judgement"] = "位置判定用「只计验收全过」中位（B96）；全部实现中位并列"
    elif tier == "t1-strict":
        s["judgement"] = "只报不判（B96：旧 (d) 口径逐位复刻 calib-import，基线计入行含被指定的实现细节）"
    return s


# —— 验收 ——

def run_jpp(project: str) -> dict:
    return run_jpp_report(project)[0]


def run_jpp_report(project: str):
    """以固定观察跑 J++ 程序，返回（投影后的返回值，调用数 cost.calls）。
    调用数取当前二进制这次运行的报告，不读仓库里可能过时的 fixed-*.json（B80 的取数，见过程记录 31-0b）。"""
    base, m = load(project)
    j = m["jpp"]
    out = ROOT / "target" / f"_measure_{project}.json"
    cwd = (base / j["run_dir"]).resolve()
    src = os.path.relpath((base / j["source"]).resolve(), cwd)
    fx = os.path.relpath((base / j["fixture"]).resolve(), cwd)
    args = [str(JPP), "run", src, "--fixtures", fx, "--output", str(out)]
    if j.get("input"):   # 步 14b-0：材料经 --input 交给程序（路径相对项目目录）
        args += ["--input", os.path.relpath((base / j["input"]).resolve(), cwd)]
    p = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if p.returncode != 0 or not out.exists():
        raise SystemExit(f"[measure_expr] jpp run 失败（{project}）：{p.stderr[-800:]}")
    d = json.loads(out.read_text(encoding="utf-8"))
    out.unlink()
    return PROJECTIONS[m["project"]["projection"]](d["value"]), d.get("cost", {}).get("calls")


def first_diff(a, b, path="$"):
    if type(a) is not type(b):
        return f"{path}: 期望 {json.dumps(a, ensure_ascii=False)[:120]}，实得 {json.dumps(b, ensure_ascii=False)[:120]}"
    if isinstance(a, dict):
        # 只比任务书要求的键（expected 里有的）；基线多输出的键（如折叠后的全文）不影响验收
        for k in sorted(a):
            if k not in b:
                return f"{path}.{k}: 缺这个键"
            d = first_diff(a[k], b[k], f"{path}.{k}")
            if d:
                return d
        return None
    if isinstance(a, list):
        if len(a) != len(b):
            return f"{path}: 长度期望 {len(a)}，实得 {len(b)}"
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_diff(x, y, f"{path}[{i}]")
            if d:
                return d
        return None
    return None if a == b else f"{path}: 期望 {a!r}，实得 {b!r}"


def check(project: str) -> bool:
    rs = check_detail(project)
    return bool(rs) and all(r["ok"] for r in rs)


def check_detail(project: str, quiet: bool = False) -> list:
    """逐份基线以固定观察运行、与 expected.json 比较；每份返回 {file, ok, calls}。
    calls 是 jev_interface 在进程退出时写到 stderr 的累计调用数（JEV_REPORT_CALLS=1）。"""
    say = (lambda *a: None) if quiet else print
    base, m = load(project)
    bdir = base / m["baseline"]["dir"]
    expected = json.loads((bdir / "expected.json").read_text(encoding="utf-8"))
    env = dict(os.environ, JEV_BACKEND="fixture", JEV_FIXTURE=str((base / m["jpp"]["fixture"]).resolve()),
               PYTHONPATH=str(SDK), JEV_REPORT_CALLS="1")
    files = [b["file"] for b in m["baseline"].get("impl", []) if b.get("tier", "t0") == "t0"] \
        or sorted(p.name for p in bdir.glob("*_[a-z].py"))
    if not files:
        say(f"[{project}] {bdir.relative_to(ROOT)} 下还没有基线实现（*_a.py）")
        return []
    out = []
    for f in files:
        if not (bdir / f).exists():
            say(f"[{project}] {f}：文件不存在")
            out.append({"file": f, "ok": False, "calls": None})
            continue
        p = subprocess.run([sys.executable, f], cwd=bdir, env=env, capture_output=True, text=True)
        n = re.findall(r"^JEV_CALLS=(\d+)$", p.stderr, re.M)
        n = int(n[-1]) if n else None
        try:
            got = json.loads(p.stdout)
        except json.JSONDecodeError:
            say(f"[{project}] {f}：不通过（stdout 不是 JSON；退出码 {p.returncode}）\n{p.stderr[-1500:]}")
            out.append({"file": f, "ok": False, "calls": n})
            continue
        d = first_diff(expected, got)
        say(f"[{project}] {f}：{'通过' if d is None else '不通过 — ' + d}（调用 {n} 次）")
        out.append({"file": f, "ok": d is None, "calls": n})
    return out


def run_jpp_impl(project: str, impl: dict, fixture=None, calib=None):
    """跑一份 J++ 实现（固定观察），返回（投影后的返回值，调用数）。impl 无 projection 时返回值原样比较。"""
    base, m = load(project)
    src = (base / impl["source"]).resolve()
    cwd = (base / impl.get("run_dir", os.path.relpath(src.parent, base))).resolve()
    fx = (base / (fixture or m["jpp"]["fixture"])).resolve()
    out = ROOT / "target" / f"_measure_{project}_{src.stem}.json"
    args = [str(JPP), "run", os.path.relpath(src, cwd), "--fixtures", str(fx), "--output", str(out)]
    if calib:
        args += ["--calib", str(calib)]
    if impl.get("input"):   # 步 14b-0：材料经 --input 交给程序（路径相对项目目录）
        args += ["--input", str((base / impl["input"]).resolve())]
    p = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if p.returncode != 0 or not out.exists():
        return None, None, p.stderr[-800:]
    d = json.loads(out.read_text(encoding="utf-8"))
    out.unlink()
    pj = impl.get("projection")
    return (PROJECTIONS[pj](d["value"]) if pj else d["value"]), d.get("cost", {}).get("calls"), ""


def check_tier(project: str, tier: str, quiet: bool = False, jpp_results: dict | None = None) -> dict:
    """该档全部实现的验收。t0：手写基线（check_detail）与 J++ 非设计者 T0 版；t1：t1_check 逐项。
    返回 {"baselines": [...], "jpp": [...]}，每项带 ok 与 calls。"""
    say = (lambda *a: None) if quiet else print
    base, m = load(project)
    skipped = []
    if tier == "t0":
        bl = check_detail(project, quiet=quiet)
        jr = []
        exp = json.loads((base / m["baseline"]["dir"] / "expected.json").read_text(encoding="utf-8"))
        for x in jpp_impls(m, "t0"):
            if x.get("redispatch"):
                # 步 25-0：待重派的非设计者版不跑验收（源文件按旧形状写成，新运行时下跑不起来）；
                # 行数照旧计，读数冻结在迁移前
                say(f"[{project}] J++ {x['source']}：跳过（{x['redispatch']}）")
                skipped.append({"file": x["source"], "reason": x["redispatch"]})
                continue
            if not (base / x["source"]).exists():
                say(f"[{project}] J++ {x['source']}：文件不存在")
                continue
            v, n, err = run_jpp_impl(project, x)
            d = first_diff(exp, v) if v is not None else "运行失败：" + err
            say(f"[{project}] J++ {x['source']}：{'通过' if d is None else '不通过 — ' + d}（调用 {n} 次）")
            jr.append({"file": x["source"], "source": x["source"], "ok": d is None, "calls": n})
        return {"baselines": bl, "jpp": jr, "skipped": skipped}
    import t1_check
    mode = "strict" if tier == "t1-strict" else "property"
    bl = []
    for b in m["baseline"].get("impl", []):
        if b.get("tier") == tier and (base / m["baseline"]["dir"] / b["file"]).exists():
            r = t1_check.check_baseline(project, b["file"], quiet=quiet, mode=mode)
            r["file"] = b["file"]
            bl.append(r)
    if jpp_results is not None:   # t1-strict 与 t1 同一批 J++ 程序，不重跑
        return {"baselines": bl, "jpp": jpp_results["jpp"], "skipped": jpp_results.get("skipped", [])}
    jr = []
    for x in jpp_impls(m, "t1"):
        if x.get("redispatch"):
            say(f"[{project}] J++ {x['source']}：跳过（{x['redispatch']}）")
            skipped.append({"file": x["source"], "reason": x["redispatch"]})
        elif (base / x["source"]).exists():
            r = t1_check.check_jpp(project, x, quiet=quiet)
            r["source"] = x["source"]
            jr.append(r)
    return {"baselines": bl, "jpp": jr, "skipped": skipped}


def observe_calls(project: str, detail: list | None = None) -> dict:
    """固定观察下两侧的调用数：J++ 取本次运行报告的 cost.calls，基线取 check_detail 的计数。"""
    detail = check_detail(project, quiet=True) if detail is None else detail
    try:
        jc = run_jpp_report(project)[1]
    except SystemExit:
        jc = None
    return {"jpp": jc, "baseline": {r["file"]: r["calls"] for r in detail}}


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--json")
    ap.add_argument("--check")
    ap.add_argument("--regen-expected")
    ap.add_argument("--verify-jpp", action="store_true")
    ap.add_argument("--tier", default="t0", choices=["t0", "t1", "t1-strict"])
    a = ap.parse_args()
    if a.check:
        r = check_tier(a.check, a.tier)
        allr = r["baselines"] + r["jpp"]
        sys.exit(0 if allr and all(x["ok"] for x in allr) else 1)
    if a.regen_expected:
        for p in projects() if a.regen_expected == "all" else [a.regen_expected]:
            base, m = load(p)
            path = base / m["baseline"]["dir"] / "expected.json"
            path.write_text(json.dumps(run_jpp(p), ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
            print(f"[{p}] 写 {path.relative_to(ROOT)}")
        return
    if a.verify_jpp:
        bad = 0
        for p in projects():
            base, m = load(p)
            exp = json.loads((base / m["baseline"]["dir"] / "expected.json").read_text(encoding="utf-8"))
            d = first_diff(exp, run_jpp(p))
            print(f"[{p}] J++ 固定观察 {'与 expected.json 一致' if d is None else '偏离 — ' + d}")
            bad += d is not None
        sys.exit(1 if bad else 0)
    doc = measure_all(quiet=True)
    for tier in TIERS:
        rows, summary = doc[tier]["projects"], doc[tier]["summary"]
        print(f"== {tier.upper()} ==")
        print("项目 | J++ 计入(中位) | 胶水 | 基线计入(中位) | 折行归一 | 原始行数 | 口径二 | token 比 | 调用数比 | 排版差异")
        for r in rows:
            if r.get("status") != "有数":
                print(f"{r['project']} | {r['status']}")
                continue
            c = r["calls"]
            print(f"{r['project']} | {r['jpp']['counted']} | {r['glue_lines']} | {r['baseline_counted_median']} | "
                  f"{r['wrap100']} | {r['caliber1']} | {r['caliber2']} | {r['token_ratio']} | "
                  f"{c['ratio'] if c else '无数'} | {'是' if r['layout_flag'] else '否'}")
        print(json.dumps(summary, ensure_ascii=False, indent=1))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump(doc, fh, ensure_ascii=False, indent=1)


def calls_from(res: dict) -> dict:
    j = [x["calls"] for x in res["jpp"] if x.get("calls") is not None]
    return {"jpp": statistics.median(j) if j else None,
            "baseline": {x["file"]: x.get("calls") for x in res["baselines"]}}


def measure_all(quiet: bool = True) -> dict:
    """两档全部项目：先逐项验收（取调用数与是否通过），再计数。返回 {t0: {...}, t1: {...}}。"""
    doc = {}
    for tier in ("t0", "t1", "t1-strict"):
        rows, checks = [], {}
        for p in projects():
            reuse = doc["t1"]["checks"].get(p) if tier == "t1-strict" else None
            res = check_tier(p, tier, quiet=quiet, jpp_results=reuse) if JPP.exists() else {"baselines": [], "jpp": []}
            checks[p] = res
            rows.append(measure(p, calls_from(res), tier, res if JPP.exists() else None))
        doc[tier] = {"projects": rows, "summary": summarize(rows, tier), "checks": checks}
    return doc

if __name__ == "__main__":
    main()
