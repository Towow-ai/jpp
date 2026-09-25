#!/usr/bin/env python3
"""cargo fmt --check 与 cargo clippy -D warnings 的计数（21 §九·1 前两项）。

报告模式下只计「需重排的文件数」与「clippy 告警数」并记基线；失败模式下任一增加即失败。
fmt 自步 0b、clippy 自步 14a 起在 ci.sh 里按失败模式跑（计数不超过基线；清零另开一步）。
clippy 带 --keep-going（步 14a）：有 error 级 lint 时 cargo 会中途停，没编到的目标不报，计数随任务顺序抖动；
带上它每个目标都编到，计数确定。
"""
import re, subprocess, sys
from _baseline import ROOT, finish

which = sys.argv[1]
if which == "fmt":
    r = subprocess.run(["cargo", "fmt", "--all", "--", "--check", "-l"], cwd=ROOT, capture_output=True, text=True)
    files = sorted({l.replace(str(ROOT) + "/", "") for l in r.stdout.splitlines() if l.strip()})
    finish("fmt", len(files), files)
else:
    r = subprocess.run(["cargo", "clippy", "--workspace", "--all-targets", "--keep-going", "--offline",
                        "--message-format=short"],
                       cwd=ROOT, capture_output=True, text=True)
    warns = sorted({l.replace(str(ROOT) + "/", "") for l in r.stderr.splitlines()
                    if re.search(r": (warning|error): ", l)})
    finish("clippy", len(warns), warns)
