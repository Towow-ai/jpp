#!/usr/bin/env python3
"""内核无固定路径与环境读取（20 §11.1、D17.4；21 §九·1：步 14 起失败模式）。

内核（现行 jpp-runtime、jpp-syntax）不许出现 home_dir、"~/"、std::env、env::var。
凭据读取属于后端：live 客户端读 HOME 在 jpp-core 时计为违规；步 14a 起 `JevClient` 在 `jpp::backends::jev`
（B74，内核之外，A6），不再计（步 14a 前扫 jpp-core、jpp-syntax）。

同一脚本另核一条（21 步 12d）：`jpp-syntax` 的文法层（词法、语法、表层 AST、装载、诊断）不引用
`jpp_ir`——文法层不承担语义识别，只有 `lower` 模块读 IR 与名字表。
"""
import re
from _baseline import ROOT, finish, rust_files

pat = re.compile(r'home_dir|"~/|std::env|env::var')
hits = []
for f in rust_files("jpp-runtime", "jpp-syntax"):
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        if pat.search(line.split("//")[0]):
            hits.append(f"{f.relative_to(ROOT)}:{i}")
grammar = ROOT / "crates" / "jpp-syntax" / "src"
for f in sorted(grammar.glob("*.rs")):
    if f.name == "lib.rs":
        continue
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        if "jpp_ir" in line.split("//")[0]:
            hits.append(f"{f.relative_to(ROOT)}:{i}（文法层引用 jpp_ir）")
finish("grep_paths", len(hits), hits)
