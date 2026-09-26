#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""GUIDE.md「搭配」一章的示例门禁。

逐个用 jpp 二进制对 examples/guide/ 下的每段示例跑 `check`（必要时带 --release-on-declared）、
能跑的再跑 `run --fixtures ...`（必要时带 --calib / --resume / --profile / --ledger-out），
比对退出码与（错误示例）报文里应出现的诊断码，最后报告通过数。

用法：
    python3 scripts/guide_check.py [--jpp <jpp 二进制路径>]

默认二进制路径是本文件同仓库编出的 target/debug/jpp（先 `cargo build -p jpp --bin jpp --offline`）。
不复用比赛期间其他会话编的临时二进制路径：那些路径是各自 worktree 的临时产物，换一台机器、换一次
checkout 就失效；本脚本永远认仓库自己的 target/。

约定（写在这里而不是散在各处，省得下一个人猜）：
- 每个 case 对应 examples/guide/ 下一个 .jpp 源文件，可以有多个 case 共用同一份源码
  （不同夹具 / 不同参数，例如作者声明线接受与不接受两种）。
- mode "run"：check 通过后再 run，两步都要过。
- mode "check_only"：只 check，不 run（原因见 GUIDE 正文，通常是需要联网下载模型、
  或需要在没有本仓库种子账本的机器上真跑执行器）。
- expect "ok"：check（以及 run，若有）都应无错误退出。
- expect "error"：check 或 run 应以非零退出，且输出里包含 expect_code 给的诊断码；
  用于报错表：报文以这里跑出来的原文为准，不是转述。
- 需要 --ledger-out 的 case（程序里出现非字面动作名的 do，CLI 一律按可能不可逆处理，
  即使该动作实际可逆，例如 lib/compose/graph.jpp 内部用 join 拼出的 "graph:xxx"）：
  ledger 写到临时目录，不进仓库。
"""
import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent  # 地基/rust-jpp
GUIDE_DIR = ROOT / "examples" / "guide"
FIX = GUIDE_DIR / "fixtures"


def jp(rel: str) -> str:
    return str(ROOT / rel)


CASES = [
    # ---- 元素 ----
    dict(name="elem-judge", source="examples/guide/elem-judge.jpp",
         fixtures="examples/guide/fixtures/elem-judge.json", mode="run", expect="ok"),
    dict(name="elem-declare-stat", source="examples/guide/elem-declare-stat.jpp",
         fixtures="examples/guide/fixtures/elem-declare-stat.json", mode="run", expect="ok"),
    dict(name="elem-declare-accept (未接受)", source="examples/guide/elem-declare-accept.jpp",
         fixtures="examples/guide/fixtures/elem-declare-accept.json",
         calib="tests/golden/_calib/declare-refund", ledger_out=True, mode="run", expect="ok"),
    dict(name="elem-declare-accept (已接受)", source="examples/guide/elem-declare-accept.jpp",
         fixtures="examples/guide/fixtures/elem-declare-accept-granted.json",
         calib="tests/golden/_calib/declare-refund", args=["--release-on-declared"],
         ledger_out=True, mode="run", expect="ok"),
    dict(name="elem-gen", source="examples/guide/elem-gen.jpp",
         fixtures="examples/guide/fixtures/elem-gen.json", mode="run", expect="ok"),
    dict(name="elem-exec", source="examples/guide/elem-exec.jpp", mode="run", expect="ok"),
    dict(name="elem-graph-algo", source="examples/guide/elem-graph-algo.jpp", mode="run", expect="ok"),
    dict(name="elem-retrieval-bm25", source="examples/guide/elem-retrieval-bm25.jpp", mode="run", expect="ok"),
    dict(name="elem-retrieval-embed", source="examples/guide/elem-retrieval-embed.jpp",
         mode="check_only", expect="ok",
         note="需要本机已缓存 sentence-transformers 模型，比赛现场机器不保证联网，只做静态检查"),
    dict(name="elem-text", source="examples/guide/elem-text.jpp", mode="run", expect="ok"),
    # ---- 组合 ----
    dict(name="comp-search", source="examples/guide/comp-search.jpp",
         fixtures="examples/guide/fixtures/comp-search.json", mode="run", expect="ok"),
    dict(name="comp-ground-verify", source="examples/guide/comp-ground-verify.jpp",
         fixtures="examples/guide/fixtures/comp-ground-verify.json",
         resume="tests/golden/search-ground/seed.ledger.jsonl",
         ledger_out=True, mode="run", expect="ok",
         note="从已在有 sandbox-exec 的机器上真跑一次录下的种子账本续跑（tests/golden/search-ground）；"
              "种子里已经带着两轮全部的 gen/exec_py/transform 记录，--resume 在本机（有 sandbox-exec）"
              "不重新执行任何一次；没有沙箱的机器上是否能跑通未验证，不下结论"),
    dict(name="comp-graph-interval", source="examples/guide/comp-graph-interval.jpp",
         fixtures="examples/guide/fixtures/comp-graph-interval.json", ledger_out=True,
         mode="run", expect="ok"),
    dict(name="comp-compose-cert", source="examples/guide/comp-compose-cert.jpp",
         fixtures="examples/guide/fixtures/comp-compose-cert.json", mode="run", expect="ok"),
    dict(name="comp-element", source="examples/guide/comp-element.jpp",
         fixtures="examples/guide/fixtures/comp-element.json", mode="run", expect="ok"),
    # ---- 嵌套 ----
    dict(name="nest-graph-then-graph", source="examples/guide/nest-graph-then-graph.jpp",
         fixtures="examples/guide/fixtures/nest-graph-then-graph.json", ledger_out=True,
         mode="run", expect="ok"),
    dict(name="nest-map-search", source="examples/guide/nest-map-search.jpp",
         fixtures="examples/guide/fixtures/nest-map-search.json", mode="run", expect="ok"),
    # ---- 报错（报文以这里跑出来的原文为准）----
    dict(name="err-j05", source="examples/guide/err-j05.jpp",
         fixtures="examples/guide/fixtures/err-j05.json", mode="run",
         expect="error", expect_code="J-05", where="run"),
    dict(name="err-j08", source="examples/guide/err-j08.jpp", mode="check_only",
         expect="error", expect_code="J-08", where="check"),
    dict(name="err-stat-unavailable", source="examples/guide/err-stat-unavailable.jpp",
         fixtures="examples/guide/fixtures/err-stat-unavailable.json",
         profile="profiles/jev-1.13.0.json", mode="run",
         expect="error", expect_code="E-stat-unavailable", where="run"),
]


def run(jpp: str, args, cwd: Path):
    return subprocess.run([jpp] + args, cwd=str(cwd), capture_output=True, text=True)


def do_check(jpp: str, case: dict, tmp: Path):
    args = ["check", case["source"]]
    args += case.get("args", [])
    r = run(jpp, args, ROOT)
    return r


def do_run(jpp: str, case: dict, tmp: Path):
    # 用绝对路径 + 换 cwd 到临时目录跑：declare-accept 的 act 分支会真的执行 write_json，
    # 相对路径按进程 cwd 落盘（`do` 不知道自己在被测试）；cwd 留在仓库里会把 refund-issued.json
    # 这类副作用文件写进 rust-jpp 根目录，污染工作树。import 语句相对源文件解析，不受 cwd 影响，
    # 所以换 cwd 不影响 import。
    args = ["run", jp(case["source"])]
    if "fixtures" in case:
        args += ["--fixtures", jp(case["fixtures"])]
    if "calib" in case:
        args += ["--calib", jp(case["calib"])]
    if "resume" in case:
        args += ["--resume", jp(case["resume"])]
    if "profile" in case:
        args += ["--profile", jp(case["profile"])]
    if case.get("ledger_out"):
        ledger = tmp / (case["name"].replace(" ", "_").replace("(", "").replace(")", "") + ".ledger.json")
        args += ["--ledger-out", str(ledger)]
    args += case.get("args", [])
    r = run(jpp, args, tmp)
    return r


GUIDE_MD = ROOT / "GUIDE.md"
GUIDE_HEADING = "## 搭配：元素 → 组合 → 嵌套"
FILE_REF_RE = re.compile(r"examples/guide/([A-Za-z0-9_.\-]+\.jpp)")


def strip_leading_comment(text: str) -> str:
    """去掉文件开头连续的 `//` 注释行（GUIDE 里嵌入代码块时不重复这些说明性注释）。"""
    lines = text.split("\n")
    i = 0
    while i < len(lines) and lines[i].startswith("//"):
        i += 1
    return "\n".join(lines[i:])


def extract_jpp_blocks(md_text: str):
    """从「搭配」一章起，抽出每个 ```jpp 代码块，及其后一行里 examples/guide/*.jpp 的引用（若有），
    以及它是否在前一行标了 `<!-- 片段 -->`（片段允许没有对应文件，因为本来就不是完整程序）。
    返回 [(code, ref_or_None, is_fragment)]。只认 fence 单独占一行、恰为 ```jpp 或 ``` 的写法。"""
    start = md_text.index(GUIDE_HEADING)
    lines = md_text[start:].split("\n")
    blocks = []
    i = 0
    while i < len(lines):
        if lines[i].strip() == "```jpp":
            is_fragment = i > 0 and "<!-- 片段 -->" in lines[i - 1]
            j = i + 1
            code_lines = []
            while j < len(lines) and lines[j].strip() != "```":
                code_lines.append(lines[j])
                j += 1
            code = "\n".join(code_lines)
            ref = None
            # 引用通常紧跟在代码块之后的 1~2 行文字里
            for k in range(j + 1, min(j + 3, len(lines))):
                m = FILE_REF_RE.search(lines[k])
                if m:
                    ref = m.group(1)
                    break
                if lines[k].strip() and not lines[k].startswith("（") and not lines[k].startswith("("):
                    break
            blocks.append((code, ref, is_fragment))
            i = j + 1
        else:
            i += 1
    return blocks


def check_guide_consistency():
    """GUIDE.md「搭配」一章里每个 ```jpp 代码块都要么指向 examples/guide/*.jpp 里逐字相同的一份
    （文件开头的说明性 `//` 注释除外），要么在代码块前一行标 `<!-- 片段 -->` 明说这不是完整程序、
    不打算独立运行。没有引用又没标片段的块算问题——这正是要防的情况：以后加一段代码忘了配文件、
    或者忘了标片段，「每段代码都能跑」就变成自我宣称，脚本应该抓住这个而不是放过。"""
    md_text = GUIDE_MD.read_text(encoding="utf-8")
    blocks = extract_jpp_blocks(md_text)
    problems = []
    checked = 0
    for code, ref, is_fragment in blocks:
        if ref is None:
            if is_fragment:
                continue
            problems.append("有一个 ```jpp 代码块既没有指向 examples/guide/ 下的文件，也没有标 <!-- 片段 -->：\n" + code[:120])
            continue
        path = GUIDE_DIR / ref
        if not path.exists():
            problems.append(f"{ref}：GUIDE 引用了这个文件，但它不存在")
            continue
        checked += 1
        file_code = strip_leading_comment(path.read_text(encoding="utf-8")).strip("\n")
        guide_code = code.strip("\n")
        if file_code != guide_code:
            problems.append(f"{ref}：GUIDE 里的代码块与文件不逐字相同")
    return checked, problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--jpp", default=str(ROOT / "target" / "debug" / "jpp"))
    ns = ap.parse_args()
    jpp = ns.jpp
    if not Path(jpp).exists():
        print(f"找不到 jpp 二进制：{jpp}")
        print("先编译：cd 地基/rust-jpp && cargo build -p jpp --bin jpp --offline")
        sys.exit(2)

    tmp = Path(tempfile.mkdtemp(prefix="guide_check_"))
    passed, failed = [], []
    for case in CASES:
        name = case["name"]
        expect = case.get("expect", "ok")
        where = case.get("where", "run" if case["mode"] == "run" else "check")
        expect_code = case.get("expect_code")

        r_check = do_check(jpp, case, tmp)
        ok = True
        detail = []

        if expect == "error" and where == "check":
            if r_check.returncode == 0 or (expect_code and expect_code not in r_check.stderr):
                ok = False
                detail.append(f"check 应报 {expect_code} 但没有：exit={r_check.returncode}\n{r_check.stderr}")
        else:
            if r_check.returncode != 0:
                ok = False
                detail.append(f"check 失败：exit={r_check.returncode}\n{r_check.stderr}")

        if ok and case["mode"] == "run":
            r_run = do_run(jpp, case, tmp)
            if expect == "error" and where == "run":
                if r_run.returncode == 0 or (expect_code and expect_code not in r_run.stderr and expect_code not in r_run.stdout):
                    ok = False
                    detail.append(f"run 应报 {expect_code} 但没有：exit={r_run.returncode}\n{r_run.stderr}\n{r_run.stdout}")
            else:
                if r_run.returncode != 0:
                    ok = False
                    detail.append(f"run 失败：exit={r_run.returncode}\n{r_run.stderr}")

        if ok:
            passed.append(name)
        else:
            failed.append((name, "\n".join(detail)))

    shutil.rmtree(tmp, ignore_errors=True)

    checked, guide_problems = check_guide_consistency()

    print(f"jpp 二进制：{jpp}")
    print(f"通过 {len(passed)}/{len(CASES)}")
    for n in passed:
        print(f"  OK   {n}")
    for n, d in failed:
        print(f"  FAIL {n}")
        print("       " + d.replace("\n", "\n       "))
    print(f"GUIDE.md 代码块与文件逐字核对：{checked - len(guide_problems)}/{checked}")
    for p in guide_problems:
        print(f"  MISMATCH {p}")
    sys.exit(0 if not failed and not guide_problems else 1)


if __name__ == "__main__":
    main()
