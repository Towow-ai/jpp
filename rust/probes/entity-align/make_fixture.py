"""Fixed-observation fixture for align.jpp (synthetic readings; construction check only)."""
import json
# the two catalogs come from the file `jpp run --input` gives the program (step 14b-0)
M = json.load(open("baseline/materials.json", encoding="utf-8"))
A, B = M["catalog_a"], M["catalog_b"]
SCALE = ["它们描述的是两种不同的产品。",
         "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
         "它们描述的是同一款产品。"]
Q = [("measure", "两条实体描述作为产品是什么关系？", "align-link"),
     ("test", "两条实体写的是同一个啤酒名吗？", "align-name"),
     ("test", "两条实体来自同一家酒厂吗？", "align-brewery"),
     ("test", "两条实体描述的是同一种啤酒风格吗？", "align-style")]
# synthetic readings keyed by (i, j); default = clearly different products
special = {(0, 0): ([0.02, 0.08, 0.90], 0.85, 0.95, 0.80),
           (1, 1): ([0.20, 0.72, 0.08], 0.10, 0.97, 0.55),
           (2, 2): ([0.03, 0.12, 0.85], 0.80, 0.93, 0.60)}
obs = []
n = 0
for i, a in enumerate(A):
    for j, b in enumerate(B):
        if abs(a["abv"] - b["abv"]) > 1.5:
            continue
        n += 1
        score, pn, pb, ps = special.get((i, j), ([0.93, 0.05, 0.02], 0.02, 0.10, 0.30))
        on = [{"a": a, "b": b}]
        for (op, text, calib), ans in zip(Q, [{"Score": score}, {"Noul": pn}, {"Noul": pb}, {"Noul": ps}]):
            o = {"on": on, "op": op, "text": text, "calib": calib, "answer": ans}
            if op == "measure":
                o["scale"] = SCALE
            obs.append(o)
cal = [{"key": k, "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗"} for k in
       ["align-link", "align-name", "align-brewery", "align-style"]]
json.dump({"description": "Synthetic fixed observations for probes/entity-align (construction check only).",
           "calibrations": cal, "observations": obs},
          open("fixture.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(n, "pairs", len(obs), "observations")
