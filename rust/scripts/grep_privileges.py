#!/usr/bin/env python3
"""内核构造只经能力令牌碰内核能力（20 v2 §五 S3、附录 B57；21 步 25-2 起失败模式，ci.sh 以 hard 调用，基线 0）。

同一 crate 里 Rust 的可见性挡不住构造文件直接读 `Interp` 的私有字段，所以这里核三件事：

1. `crates/jpp-runtime/src/constructs/*.rs` 不直接调用能力背后的 API（读答案、签发出口与读数、
   责任表置位、选择边、报告 exits 行、循环已见键、记账位置、账本与 trace 事件、缺席账、合成出口的分量），
   也不自己造令牌（能力类按 B139 冻结为十二类令牌加刷新点）；不直接造题值或写题值字段（B138：经
   `IssueQuestion`）；`self.flush(` 只许出现在注册表里 `refresh` 非空的构造所在文件（B138 (2)、B139 的刷新点）；
2. `crates/jpp-runtime/src/caps.rs` 的注册表里每条 `ConstructSpec` 前后 5 行内有 `依据：`；
3. 按文件核对声明与使用：文件里用到的能力（`caps.<能力>()`）必须有该文件里的某个构造声明；
   每个构造声明的能力必须在它的实现所在文件里用到（文件级，粗粒度）。内部构造（`run: None`，
   步 25-2b 的 `compose`、`element`）的实现文件是 `constructs/<名字>.rs`。
"""
import re
from _baseline import ROOT, finish

RT = ROOT / "crates" / "jpp-runtime" / "src"
CONSTRUCTS = RT / "constructs"
CAPS = RT / "caps.rs"

FORBIDDEN = [
    r"self\.answers\b",
    r"self\.answer_of\(",
    r"self\.new_exit\(",
    r"self\.new_reading_id\(",
    r"self\.fill_answer\(",
    r"\.consumed\.set\(",
    r"\.consumed_by\.borrow_mut\(",
    r"self\.ledger\b",
    r"self\.mark\(\)",
    r"self\.since\(",
    r"self\.loops\b",
    r"\.absent_marks\b",
    r"self\.exit_grades\b",
    r"self\.trace\.push\(",
    r"self\.exit_rows\b",
    r"Sources::select\(",
    r"sources_only\(",
    r"\.parts\.borrow_mut\(",
    r"Question::new\(",
    r"Question::with_evidence\(",
    r"Form::new\(",
    r"\.fill\(&",
    r"Value::Question\(Rc::new",
    r"Value::Form\(Rc::new",
    r"\.(taint|presupposition|request|permute|over_kind)\s*=[^=]",
    r"\bCap\s*\{",
    r"PhantomData",
]
CAP_CALL = {
    "read_answer": "ReadAnswer",
    "issue_reading": "IssueReading",
    "issue_unsure": "IssueUnsure",
    "issue_composite": "IssueComposite",
    "issue_question": "IssueQuestion",
    "duty": "Duty",
    "source_select": "SourceSelect",
    "exit_row": "ExitRow",
    "loop_context": "LoopContext",
    "key_collect": "KeyCollect",
    "ledger_read": "LedgerRead",
    "ledger_write": "LedgerWrite",
}
ALIAS = {
    "P_READ": "ReadAnswer", "P_READING": "IssueReading", "P_UNSURE": "IssueUnsure",
    "P_COMPOSITE": "IssueComposite", "P_QUESTION": "IssueQuestion", "P_DUTY": "Duty",
    "P_SELECT": "SourceSelect", "P_ROW": "ExitRow", "P_LOOP": "LoopContext", "P_KEYS": "KeyCollect",
    "P_LREAD": "LedgerRead", "P_LWRITE": "LedgerWrite",
}

hits = []
code_of = {}
for f in sorted(CONSTRUCTS.glob("*.rs")):
    rel = f.relative_to(ROOT).as_posix()
    lines = f.read_text(encoding="utf-8").splitlines()
    code = [ln.split("//")[0] for ln in lines]
    code_of[f.name] = "\n".join(code)
    for i, s in enumerate(code, 1):
        for p in FORBIDDEN:
            if re.search(p, s):
                hits.append(f"{rel}:{i}: 直接调用 {p}（应经能力令牌，B57）")

# 注册表：逐条取 name、privileges、run 调到的实现函数，并核 `依据：`
caps_lines = CAPS.read_text(encoding="utf-8").splitlines()
entries = []
for i, ln in enumerate(caps_lines):
    if re.match(r"\s*ConstructSpec \{\s*$", ln):
        window = caps_lines[max(0, i - 5): i + 6]
        body = []
        for ln2 in caps_lines[i:]:
            body.append(ln2)
            if re.match(r"\s*\},\s*$", ln2):
                break
        text = "\n".join(body)
        name = re.search(r'name:\s*"([^"]+)"', text).group(1)
        privs = re.search(r"privileges:\s*&\[([^\]]*)\]", text).group(1)
        privs = {ALIAS.get(p.strip(), p.strip()) for p in privs.split(",") if p.strip()}
        refresh = re.search(r"refresh:\s*&\[([^\]]*)\]", text).group(1).strip() != ""
        m_run = re.search(r"it\.(b_\w+)\(", text)
        run = m_run.group(1) if m_run else None  # None = 内部构造（run: None）
        if not any("依据：" in w for w in window):
            hits.append(f"crates/jpp-runtime/src/caps.rs:{i + 1}: 构造 {name} 的 ConstructSpec 前后 5 行内没有 `依据：`")
        entries.append((name, privs, run, refresh))

by_file = {}
for name, privs, run, refresh in entries:
    if run is None:
        owner = [f"{name}.rs"] if f"{name}.rs" in code_of else []
    else:
        owner = [fn for fn, code in code_of.items() if re.search(rf"fn {run}\(", code)]
    if len(owner) != 1:
        hits.append(f"caps.rs: 构造 {name} 的实现 {run} 在 constructs/ 里找到 {len(owner)} 处")
        continue
    by_file.setdefault(owner[0], []).append((name, privs, refresh))

for fn, cons in sorted(by_file.items()):
    used = {CAP_CALL[m] for m in re.findall(r"caps\s*\.\s*(%s)\(\)" % "|".join(CAP_CALL), code_of[fn])}
    declared = set().union(*(p for _, p, _ in cons))
    if not any(r for _, _, r in cons):
        for i, ln in enumerate(code_of[fn].splitlines(), 1):
            if "self.flush(" in ln:
                hits.append(f"crates/jpp-runtime/src/constructs/{fn}:{i}: self.flush( 只许出现在刷新点构造的文件里（B138 (2)）")
    for u in sorted(used - declared):
        hits.append(f"crates/jpp-runtime/src/constructs/{fn}: 用了能力 {u}，但本文件的构造都没有声明")
    for name, privs, _ in cons:
        for p in sorted(privs - used):
            hits.append(f"caps.rs: 构造 {name} 声明了 {p}，它所在的 constructs/{fn} 没有用到")

print(f"  · 注册表 {len(entries)} 条，分布在 {len(by_file)} 个构造文件")
finish("grep_privileges", len(hits), hits)
