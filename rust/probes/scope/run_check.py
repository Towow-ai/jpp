"""步 20d-1 探针复跑：去掉手写 form-topic 夹具线，让查找落到带指纹的题式级认证线（固定观察，0 费用）。"""
import json
import pathlib
import subprocess

R = pathlib.Path(__file__).resolve().parents[2]  # rust-jpp
HERE = pathlib.Path(__file__).parent
JPP = R / "target" / "debug" / "jpp"
CALIB = HERE / "calib"
cases = [
    ("folio", R / "probes/folio/folio.jpp", R / "probes/folio/fixture.json"),
    ("winnow", R / "probes/winnow/winnow.jpp", R / "probes/winnow/fixture.json"),
    ("topic-relevance", R / "examples/topic-relevance.jpp", R / "examples/fixtures/topic-relevance.json"),
]
out = {}
for name, src, fx in cases:
    d = json.load(open(fx))
    d["calibrations"] = [c for c in d.get("calibrations", []) if c["key"] != "form-topic"]
    fx2 = HERE / f"fixture-{name}.json"
    json.dump(d, open(fx2, "w"), ensure_ascii=False, indent=1)
    rep = HERE / f"report-{name}.json"
    p = subprocess.run([str(JPP), "run", str(src), "--fixtures", str(fx2), "--calib", str(CALIB), "--output", str(rep),
                        "--ledger-out", str(HERE / f"ledger-{name}.jsonl")],
                       capture_output=True, text=True, cwd=src.parent if name != "topic-relevance" else R)
    r = json.load(open(rep)) if rep.exists() else {}
    w = r.get("trace", {}).get("warnings", [])
    scope = [x for x in w if x.startswith("W-calib-scope")]
    form = [x for x in w if x.startswith("W-form-line")]
    out[name] = {"exit": p.returncode, "status": r.get("status"), "form_line_exits": len(form),
                 "calib_scope": len(scope), "scope_examples": scope[:2], "stderr": p.stderr.strip()[-300:]}
json.dump(out, open(HERE / "result.json", "w"), ensure_ascii=False, indent=1)
print(json.dumps(out, ensure_ascii=False, indent=1))
