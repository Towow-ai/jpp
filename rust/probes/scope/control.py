"""对照：同一条线去掉指纹再跑，核对「范围外只加告警与降级、不改路由」。"""
import json
import pathlib
import subprocess

R = pathlib.Path(__file__).resolve().parents[2]
HERE = pathlib.Path(__file__).parent
JPP = R / "target" / "debug" / "jpp"
nofp = HERE / "calib-nofp"
nofp.mkdir(exist_ok=True)
for f in (HERE / "calib").glob("*.json"):
    d = json.load(open(f))
    d["scope"].pop("fingerprint", None)
    json.dump(d, open(nofp / f.name, "w"), ensure_ascii=False, indent=1)
res = {}
for name, src in [("folio", R / "probes/folio/folio.jpp"), ("winnow", R / "probes/winnow/winnow.jpp"),
                  ("topic-relevance", R / "examples/topic-relevance.jpp")]:
    rep = HERE / f"report-{name}-nofp.json"
    subprocess.run([str(JPP), "run", str(src), "--fixtures", str(HERE / f"fixture-{name}.json"), "--calib", str(nofp),
                    "--output", str(rep), "--ledger-out", str(HERE / f"ledger-{name}-nofp.jsonl")],
                   capture_output=True, text=True,
                   cwd=src.parent if name != "topic-relevance" else R)
    a = json.load(open(HERE / f"report-{name}.json"))
    b = json.load(open(rep))
    res[name] = {"value_same": a["value"] == b["value"],
                 "scope_warnings_nofp": sum(w.startswith("W-calib-scope") for w in b["trace"]["warnings"])}
print(json.dumps(res, ensure_ascii=False))
