#!/usr/bin/env python3
"""运行期错误必须带编号（21 步 9a；评估①建议 6）。

计 `crates/jpp-runtime/src/`（步 14a 前 `crates/jpp-core/src/interp/`）里不带规则号的运行期错误站点：`err(None` 与 `RtError::new(None`
（含参数换行写的形式）。步 9a 填完编号后为 0，只许保持；编号与类型说明见 `crates/jpp/src/cli/diag_json.rs`。
"""
import re
from _baseline import ROOT, finish

pat = re.compile(r"\b(?:err|RtError::new)\(\s*None\b")
hits = []
for f in sorted((ROOT / "crates" / "jpp-runtime" / "src").rglob("*.rs")):
    text = f.read_text(encoding="utf-8")
    for m in pat.finditer(text):
        line = text.count("\n", 0, m.start()) + 1
        hits.append(f"{f.relative_to(ROOT)}:{line}")
finish("grep_rt_codes", len(hits), hits)
