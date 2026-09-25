#!/usr/bin/env python3
"""依赖表核对（20 §2.2 第 1 条；21 §九·1：步 6 起对已存在的 crate 改为失败模式）。

读 cargo metadata，逐个工作区 crate 比对允许的依赖集合；多一条即违规。
不在表里的现存 crate 只报告其依赖，不计违规（步 14a 删 jpp-core 后已没有这种 crate）。

步 14a（B74）起另核 `jpp` crate 的模块级约束（`评估①裁定` §十第 12(c) 条、`20` §2.3 L6）：
`session/` 不引用 std::fs、std::env；`store/` 不引用运行时与 `crate::backends`；
`backends/` 不引用 `crate::store`、`crate::session`；`cli/` 不限。按源码 grep，注释行不计。
"""
import json, re, subprocess
from _baseline import ROOT, finish

# 20 §2.2 第 1 条（终审后：jpp-stat 并入 jpp-value，jpp-lower 并入 jpp-syntax；
# B74：jpp-store、jpp-backend-jev 成为 jpp 的模块，jpp-cli 并入 jpp 的 bin 目标，jpp 依赖全部）。
ALLOWED = {
    "jpp-ir": set(),
    "jpp-syntax": {"jpp-ir"},
    "jpp-value": {"jpp-ir"},
    "jpp-effects": {"jpp-ir", "jpp-value"},
    "jpp-ledger": {"jpp-ir", "jpp-value", "jpp-effects"},
    "jpp-calib": {"jpp-ir", "jpp-value", "jpp-effects"},
    "jpp-check": {"jpp-ir", "jpp-effects"},
    "jpp-plan": {"jpp-ir", "jpp-effects"},
    "jpp-runtime": {"jpp-ir", "jpp-value", "jpp-effects", "jpp-ledger"},
    "jpp-lib": {"jpp-ir", "jpp-value", "jpp-effects", "jpp-check"},
}

# `jpp` 的模块级 grep 表（B74；21 §四·6 落步表）：模块目录 → 不许出现的引用
MODULE_RULES = {
    "session": [r"\bstd::fs\b", r"\bstd::env\b", r"(?<![\w:])fs::", r"(?<![\w:])env::"],
    "store": [r"\bjpp_runtime\b", r"\bcrate::backends\b"],
    "backends": [r"\bcrate::store\b", r"\bcrate::session\b"],
}

meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"],
                                 cwd=ROOT, capture_output=True, text=True, check=True).stdout)
members = {p["name"]: p for p in meta["packages"]}
violations, notes = [], []
for name, p in sorted(members.items()):
    deps = {d["name"] for d in p["dependencies"] if d["name"].startswith("jpp") and d.get("kind") in (None, "normal")}
    if name == "jpp":
        continue  # 外观依赖全部
    if name not in ALLOWED:
        notes.append(f"{name}（现存 crate，不在目标表）→ {sorted(deps)}")
        continue
    for d in sorted(deps - ALLOWED[name]):
        violations.append(f"{name} → {d} 不在 20 §2.2 依赖表")
jpp_src = ROOT / "crates" / "jpp" / "src"
for module, pats in MODULE_RULES.items():
    d = jpp_src / module
    if not d.exists():
        print(f"  · jpp::{module} 尚未建出（不计）")
        continue
    for f in sorted(d.rglob("*.rs")):
        for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
            code = line.split("//", 1)[0]
            for pat in pats:
                if re.search(pat, code):
                    violations.append(f"jpp::{module} {f.relative_to(ROOT)}:{i} 引用了 {pat}（B74 模块约束）")
for n in notes:
    print(f"  · {n}")
finish("deps", len(violations), violations)
