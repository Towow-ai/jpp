#!/usr/bin/env python3
"""把文档里的代码片段和全部示例实跑一遍（公开仓库 CI 用，只在公开侧，见 PUBLIC-SNAPSHOT.md）。

范围：
  - DOCS 里的 ```sh 片段：每段当一个 bash 脚本（set -e）在 rust/ 下跑，同一份文档的各段依次跑，
    前一段写出的文件后一段能读到；整份文档跑完删掉它新建的未跟踪文件。
    ```jpp 片段写进 examples/ 下的临时文件，先 check 再 run。
    ```json 片段先解析；带 observations 的当夹具喂给 examples/composition.jpp，核对夹具加载器收得下。
  - tests/golden/manifest.json 登记的每个示例：check 源文件，再按清单的 fixtures / calib / files /
    resume_from 跑一次；登记为 expect=error 的必须失败。examples/ 下没登记的 .jpp 只 check 并列出来。
    逐字节比对金样与重放在 `cargo test -p jpp --test golden` 里做，这里不重复。

跳过（逐行打印原因）：需要真机的行（`--features live` 构建、`--backend live` 调用），以及读这些行产物的行
（`--replay` / `--resume` / `--calib` 指向被跳过行的 `--ledger-out` / `--output` / `--calib-out`）。
`./target/release/jpp` 换成本次构建的 `target/debug/jpp`；片段用到 `labels.jsonl` 而文件不存在时，
取同一片段注释里给出的样例行写一份。同一条 cargo build / test / install 命令成功过一次就不再重跑。

模式与 scripts/ci.sh 相同：JPP_CI_MODE=report（默认）只报告、退出 0；fail 时任一项未通过即退出 1。
在 GitHub Actions 里另把结果表写进 $GITHUB_STEP_SUMMARY。
"""
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ["GUIDE.md", "README.md", "METHODS-AND-LIFECYCLE.md", "METHODS-AND-LIFECYCLE.zh-CN.md"]
JPP = ROOT / "target" / "debug" / "jpp"
MODE = os.environ.get("JPP_CI_MODE", "report")
DEDUP = re.compile(r"^cargo (build|test|install)\b")

results = []  # (状态, 位置, 说明)；状态 ∈ 通过 / 未通过 / 跳过


def record(status, where, note=""):
    results.append((status, where, note))
    print(f"  [{status}] {where}" + (f"：{note}" if note else ""), flush=True)


def tail(text, n=15):
    return "\n".join(text.strip().splitlines()[-n:])


def blocks(text):
    """返回 [(语言, 起始行号, 行列表)]；只认行首的 ``` 围栏。"""
    out, lang, start, buf = [], None, 0, []
    for i, line in enumerate(text.splitlines(), 1):
        if line.startswith("```"):
            if lang is None:
                lang, start, buf = line[3:].strip() or "-", i, []
            else:
                out.append((lang, start, buf))
                lang = None
        elif lang is not None:
            buf.append(line)
    return out


def logical(lines):
    """把行尾反斜杠续行并成一条。"""
    out, cur = [], ""
    for line in lines:
        if line.rstrip().endswith("\\"):
            cur += line.rstrip()[:-1] + " "
        else:
            out.append(cur + line)
            cur = ""
    if cur:
        out.append(cur)
    return out


def sample_row(lines):
    """片段注释里给出的 JSON 样例行（可能跨两行注释）。"""
    text = " ".join(l.strip().lstrip("#").strip() for l in lines if l.strip().startswith("#"))
    start = text.find("{")
    while start != -1:
        depth = 0
        for j in range(start, len(text)):
            depth += {"{": 1, "}": -1}.get(text[j], 0)
            if depth == 0:
                try:
                    return json.loads(text[start:j + 1])
                except ValueError:
                    break
        start = text.find("{", start + 1)
    return None


def flag_values(tokens, flags):
    return [tokens[i + 1] for i, t in enumerate(tokens[:-1]) if t in flags]


def untracked():
    r = subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "-z", "."],
                       cwd=ROOT, capture_output=True, text=True)
    return set(filter(None, r.stdout.split("\0"))) if r.returncode == 0 else None


def cleanup(before):
    after = untracked()
    if before is None or after is None:
        return
    for rel in sorted(after - before, reverse=True):
        p = ROOT / rel
        p.unlink(missing_ok=True)
        for parent in p.parents:
            if parent == ROOT or any(parent.iterdir()):
                break
            parent.rmdir()


def run_sh(where, lines, state):
    script, runnable, dedup = ["set -e"], 0, []
    for line in logical(lines):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        cmd = s.replace("./target/release/jpp", "target/debug/jpp")
        try:
            tokens = shlex.split(cmd, comments=True)
        except ValueError:
            tokens = cmd.split()
        why = None
        if "--features" in tokens and "live" in flag_values(tokens, {"--features"}):
            why = "真机构建（--features live）"
        elif "live" in flag_values(tokens, {"--backend"}):
            why = "真机调用（--backend live，需要 ~/.typesafe-key 与网络）"
        else:
            used = set(flag_values(tokens, {"--replay", "--resume", "--calib"})) & state["skipped_out"]
            if used:
                why = f"读被跳过行的产物 {', '.join(sorted(used))}"
        if why:
            state["skipped_out"] |= set(flag_values(tokens, {"--ledger-out", "--output", "--calib-out"}))
            record("跳过", where, f"{why}：{s}")
            continue
        if DEDUP.match(cmd) and cmd in state["done"]:
            script.append(f"echo {shlex.quote('[已跑过] ' + cmd)}")
            continue
        if "labels.jsonl" in tokens and not (ROOT / "labels.jsonl").exists():
            row = sample_row(lines)
            if row is None:
                record("未通过", where, "片段用到 labels.jsonl，注释里找不到样例行")
                return
            (ROOT / "labels.jsonl").write_text(json.dumps(row, ensure_ascii=False) + "\n", encoding="utf-8")
            print(f"    labels.jsonl 取自片段注释里的样例行：{json.dumps(row, ensure_ascii=False)}")
        script.append(cmd)
        runnable += 1
        if DEDUP.match(cmd):
            dedup.append(cmd)
    if runnable == 0:
        return
    with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False, encoding="utf-8") as fh:
        fh.write("\n".join(script) + "\n")
    r = subprocess.run(["bash", "-x", fh.name], cwd=ROOT, capture_output=True, text=True, timeout=1800)
    os.unlink(fh.name)
    if r.returncode == 0:
        state["done"].update(dedup)
        record("通过", where, f"sh，{runnable} 条命令")
    else:
        record("未通过", where, f"sh 退出 {r.returncode}\n{tail(r.stdout + r.stderr)}")


def run_jpp_source(where, path, expect_error=False):
    c = subprocess.run([str(JPP), "check", str(path)], cwd=ROOT, capture_output=True, text=True)
    if expect_error:
        return c.returncode != 0, c.stderr
    return c.returncode == 0, c.stdout + c.stderr


def run_jpp_block(where, lines, doc):
    tmp = ROOT / "examples" / f"_doc_snippet_{pathlib.Path(doc).stem}_{where.rsplit(':', 1)[1]}.jpp"
    tmp.write_text("\n".join(lines) + "\n", encoding="utf-8")
    try:
        c = subprocess.run([str(JPP), "check", str(tmp)], cwd=ROOT, capture_output=True, text=True)
        r = subprocess.run([str(JPP), "run", str(tmp)], cwd=ROOT, capture_output=True, text=True)
    finally:
        tmp.unlink()
    if c.returncode == 0 and r.returncode == 0:
        record("通过", where, "jpp，check 与 run")
    else:
        record("未通过", where, f"jpp check 退出 {c.returncode}、run 退出 {r.returncode}\n{tail(c.stderr + r.stderr)}")


def run_json_block(where, lines):
    try:
        doc = json.loads("\n".join(lines))
    except ValueError as e:
        record("未通过", where, f"json 解析失败：{e}")
        return
    if not (isinstance(doc, dict) and "observations" in doc):
        record("通过", where, "json 可解析")
        return
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False, encoding="utf-8") as fh:
        json.dump(doc, fh, ensure_ascii=False)
    r = subprocess.run([str(JPP), "run", "examples/composition.jpp", "--fixtures", fh.name],
                       cwd=ROOT, capture_output=True, text=True)
    os.unlink(fh.name)
    if r.returncode == 0:
        record("通过", where, "json 夹具被 jpp run --fixtures 收下")
    else:
        record("未通过", where, f"json 夹具加载失败\n{tail(r.stderr)}")


def docs():
    print("== 文档片段", flush=True)
    state = {"skipped_out": set(), "done": set()}
    for doc in DOCS:
        path = ROOT / doc
        if not path.exists():
            continue
        before = untracked()
        for lang, start, lines in blocks(path.read_text(encoding="utf-8")):
            where = f"{doc}:{start}"
            if lang in ("sh", "bash", "shell"):
                run_sh(where, lines, state)
            elif lang == "jpp":
                run_jpp_block(where, lines, doc)
            elif lang == "json":
                run_json_block(where, lines)
        cleanup(before)
        state["skipped_out"].clear()


def examples():
    print("== 示例（tests/golden/manifest.json）", flush=True)
    manifest = json.loads((ROOT / "tests/golden/manifest.json").read_text(encoding="utf-8"))
    base = pathlib.Path(tempfile.mkdtemp(prefix="jpp-examples-"))
    seen = set()
    try:
        for c in manifest["cases"]:
            name, src = c["name"], c["source"]
            expect_error = c.get("expect") == "error"
            seen.add(src)
            if not expect_error:
                ok, out = run_jpp_source(name, ROOT / src)
                if not ok:
                    record("未通过", f"{name}（{src}）", f"check 失败\n{tail(out)}")
                    continue
            tmp = base / name
            tmp.mkdir(parents=True)
            for fname, body in c.get("files", {}).items():
                (tmp / fname).write_text(body, encoding="utf-8")
            args = [str(JPP), "run", str(ROOT / src)]
            if c.get("fixtures"):
                args += ["--fixtures", str(ROOT / c["fixtures"])]
            if c.get("calib") and not expect_error:
                args += ["--calib", str(ROOT / c["calib"])]
            if c.get("resume_from"):
                args += ["--resume", str(base / c["resume_from"] / "ledger.json")]
            if not expect_error:
                args += ["--ledger-out", "ledger.json", "--output", "report.json"]
            r = subprocess.run(args, cwd=tmp, capture_output=True, text=True)
            if expect_error:
                if r.returncode != 0:
                    record("通过", f"{name}（{src}）", "按登记报错")
                else:
                    record("未通过", f"{name}（{src}）", "登记为预期报错，却运行成功")
            elif r.returncode == 0:
                status = json.loads((tmp / "report.json").read_text(encoding="utf-8")).get("status")
                record("通过", f"{name}（{src}）", f"check 与 run，status={status}")
            else:
                record("未通过", f"{name}（{src}）", f"run 退出 {r.returncode}\n{tail(r.stderr)}")
        for path in sorted((ROOT / "examples").rglob("*.jpp")):
            rel = path.relative_to(ROOT).as_posix()
            if rel in seen:
                continue
            ok, out = run_jpp_source(rel, path, expect_error=rel.startswith("examples/errors/"))
            record("通过" if ok else "未通过", rel, "未登记进金样清单，只 check" + ("" if ok else f"\n{tail(out)}"))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def summary():
    counts = {s: sum(1 for r in results if r[0] == s) for s in ("通过", "未通过", "跳过")}
    line = f"合计：通过 {counts['通过']}，未通过 {counts['未通过']}，跳过 {counts['跳过']}（模式 {MODE}）"
    print(line)
    out = os.environ.get("GITHUB_STEP_SUMMARY")
    if out:
        with open(out, "a", encoding="utf-8") as fh:
            fh.write("### 文档片段与示例 / Doc snippets and examples\n\n" + line + "\n\n")
            fh.write("| 结果 | 位置 | 说明 |\n|---|---|---|\n")
            for s, where, note in results:
                if s != "通过":
                    first = note.splitlines()[0] if note else ""
                    fh.write(f"| {s} | `{where}` | {first.replace('|', '/')} |\n")
            fh.write("\n")
    return counts["未通过"]


def main():
    b = subprocess.run(["cargo", "build", "--locked", "-q", "-p", "jpp"], cwd=ROOT)
    if b.returncode != 0:
        sys.exit("cargo build -p jpp 失败")
    docs()
    examples()
    failed = summary()
    sys.exit(1 if MODE == "fail" and failed else 0)


if __name__ == "__main__":
    main()
