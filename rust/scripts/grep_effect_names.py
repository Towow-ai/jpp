#!/usr/bin/env python3
"""效应名只许出现在效应注册处（20 §11.1；21 §九·1：步 15a 起失败模式，ci.sh 以 hard 调用，基线 0）。

目标：`EffectId::(Judge|Gen|Do|Ask|Transform)` 只许在 jpp-effects/src/kinds/、builtin_ports/。
现行代码没有 EffectId；本脚本同时报告现行代码按效应名分支的点（字符串 "judge"/"gen"/"do"/"ask"
作 match 臂或比较），作为步 15a 的迁移清单，计入违规数。
"""
import re
from _baseline import ROOT, finish, rust_files

hits = []
pat_target = re.compile(r"EffectId::(Judge|Gen|Do|Ask|Transform)")
pat_now = re.compile(r'"(judge|gen|do|ask|transform)"\s*(=>|\|)|==\s*"(judge|gen|do|ask|transform)"')
for f in rust_files():
    rel = f.relative_to(ROOT).as_posix()
    allowed = "jpp-effects/src/kinds/" in rel or "jpp-effects/src/builtin_ports/" in rel
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        s = line.split("//")[0]
        if not allowed and (pat_target.search(s) or pat_now.search(s)):
            hits.append(f"{rel}:{i}")
finish("grep_effect_names", len(hits), hits)
