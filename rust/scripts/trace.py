#!/usr/bin/env python3
"""代码与依据条文的双向追溯（20 §11.3、D15；21 §九·1：始终只许孤儿数减少）。

反向孤儿：代码里的检查点（构造诊断或运行期错误、带 W-/E-/J- 编号的报文）前后 N 行内没有
`依据：` 标记。运行期编号 `E-rt-*`（步 9a）不算检查点：它们不对应依据条文，说明集中在 RT_CODES 表。正向孤儿：依据文本 12 里出现的稳定编号（J-NN、Bn）在代码中没有任何引用。
标记必须含稳定编号作键，节号只作附带说明。生成 docs/追溯索引.md。
"""
import re
from _baseline import ROOT, finish, rust_files

N = 5
SPEC = ROOT.parent / "12-IR与类契约-v0.1.md"
check_pat = re.compile(r'Diagnostic\s*\{|RtError::new|err\(Some\("|"[WE]-[A-Za-z0-9-]+|"J-\d\d')
rt_pat = re.compile(r'(?:err|RtError::new)\(\s*Some\("E-rt-[a-z]+"\)|"E-rt-[a-z]+"')
id_pat = re.compile(r"\b(J-\d\d|B\d{1,2})\b")

reverse, refs = [], {}
for f in rust_files():
    lines = f.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines):
        for m in id_pat.finditer(line):
            refs.setdefault(m.group(1), set()).add(f"{f.relative_to(ROOT)}:{i + 1}")
        code = line.split("//")[0]
        # 运行期编号 E-rt-*（步 9a）不对应依据文本的条目，类型说明的唯一来源是 jpp 的 cli/diag_json.rs 中 RT_CODES 表，
        # 不要求逐站点写 `依据：`；只带 E-rt 编号的行不算检查点
        if check_pat.search(rt_pat.sub("", code)):
            window = lines[max(0, i - N): i + N + 1]
            if not any("依据：" in w for w in window):
                reverse.append(f"{f.relative_to(ROOT)}:{i + 1}")

spec_ids = sorted(set(id_pat.findall(SPEC.read_text(encoding="utf-8")))) if SPEC.exists() else []
forward = [i for i in spec_ids if i not in refs]

out = ROOT / "docs" / "追溯索引.md"
out.parent.mkdir(exist_ok=True)
rows = [f"| {i} | {', '.join(sorted(refs.get(i, []))[:6]) or '（无引用：正向孤儿）'} |" for i in spec_ids]
out.write_text("# 追溯索引（由 scripts/trace.py 生成，勿手改）\n\n"
               f"依据文本 12 中的稳定编号 {len(spec_ids)} 个；正向孤儿 {len(forward)}；"
               f"代码检查点缺 `依据：` 标记的反向孤儿 {len(reverse)}。\n\n| 编号 | 代码引用（前 6 处） |\n|---|---|\n"
               + "\n".join(rows) + "\n", encoding="utf-8")
detail = [f"反向孤儿 {r}" for r in reverse] + [f"正向孤儿 {x}" for x in forward]
print(f"  · 反向孤儿 {len(reverse)}，正向孤儿 {len(forward)}；索引写入 docs/追溯索引.md")
finish("trace", len(reverse) + len(forward), detail)
