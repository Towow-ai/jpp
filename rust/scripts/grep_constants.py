#!/usr/bin/env python3
"""内核不写死判断器属性的数值（20 §11.1、§3.9；21 §九·1：步 15d 起失败模式）。

L0–L5 内代表判断器属性的数值字面量：小数、科学计数法，或 concurrency|window|delta|k_limit|latency
后接数字。允许名单：jpp-value/src/stat.rs、jpp-runtime/src/strength.rs（步 14a 前在 jpp-calib，更早在 jpp-core）与测试。

扫描范围：`crates/` 下除宿主 crate `jpp` 以外的全部 crate，加 `jpp/src/backends/`（B74：`20` §11.1 数值
字面量 grep 覆盖 L0–L5 与 `jpp/src/backends/`；步 14a 前是「除 jpp-cli 以外的全部 crate」，2026-09-24 起；
此前只扫 jpp-core、jpp-frontend，拆 crate 时常数随文件移出扫描面，计数下降却没有删常数——步 9 实测 99→95）。
新增 crate 自动纳入。

计数单位是**字面量出现次数**（每个匹配计一次），不是「含字面量的行数」：rustfmt 把一行结构体
字面量拆成多行不应让计数变化（步 4c 实测按行计数因此 +8，而没有新增任何常数）。
"""
import re
from _baseline import ROOT, finish, rust_files

ALLOW = ("jpp-value/src/stat.rs", "jpp-runtime/src/strength.rs")
num = re.compile(r"(?<![\w.])(\d+\.\d+|\d+(\.\d+)?e-?\d+)(?![\w])")
named = re.compile(r"(concurrency|window|delta|k_limit|latency)\w*\s*[:=]\s*\d")
hits = []
SCAN = sorted(p.name for p in (ROOT / "crates").iterdir() if p.is_dir() and p.name != "jpp")
BACKENDS = sorted((ROOT / "crates" / "jpp" / "src" / "backends").rglob("*.rs"))
for f in list(rust_files(*SCAN)) + BACKENDS:
    rel = f.relative_to(ROOT).as_posix()
    if rel.endswith(ALLOW):
        continue
    in_test = False
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        if "#[cfg(test)]" in line:
            in_test = True
        if in_test:
            continue
        s = re.sub(r'"(\\.|[^"\\])*"', '""', line.split("//")[0])
        for m in list(num.finditer(s)) + list(named.finditer(s)):
            hits.append(f"{rel}:{i}: {m.group(0)}")
finish("grep_constants", len(hits), hits)
