"""Generate the fixed-observation fixture for winnow.jpp from baseline/materials.json (the file `jpp run --input` gives the program; step 14b-0).

Readings are synthetic (construction check, not model evaluation). Calibration
records are hand-written to exercise routing; the live run uses real lines instead.
"""
import json

inp = json.load(open("baseline/materials.json", encoding="utf-8"))["tool_outputs"]
REL = "这段话的内容是否与「{concept}」这个话题相关？".replace("{concept}", inp["task"])
ERR = "这段工具输出里是否含有报错、失败或异常信息？"
# synthetic readings: tool-1 has an error; tool-2 does not
err_p = {"tool-1:cargo test": 0.97, "tool-2:grep -rn password src/": 0.04}
rel_p = {
    "tool-1:cargo test": [0.05, 0.10, 0.98, 0.60],
    # login.rs hit, metrics, migration, i18n (band -> unsure), form struct
    "tool-2:grep -rn password src/": [0.96, 0.08, 0.30, 0.57, 0.90],
}
obs = []
for r in inp["results"]:
    obs.append({"on": ["\n".join(r["chunks"])], "op": "test", "text": ERR,
                "calib": "winnow-error", "answer": {"Noul": err_p[r["id"]]}})
    for c, p in zip(r["chunks"], rel_p[r["id"]]):
        obs.append({"on": [c], "op": "test", "text": REL, "calib": "form-topic",
                    "answer": {"Noul": p}})
# batched variant: one state per tool result holding task + all blocks
USE = "状态里编号为 {id} 的输出块，对完成状态里的 task 是否有用？"
for r in inp["results"]:
    st = {"task": inp["task"], "blocks": [{"id": "b%d" % i, "text": c} for i, c in enumerate(r["chunks"])]}
    obs.append({"on": [st], "op": "test", "text": ERR, "calib": "winnow-error",
                "answer": {"Noul": err_p[r["id"]]}})
    for i, p in enumerate(rel_p[r["id"]]):
        obs.append({"on": [st], "op": "test", "text": USE.replace("{id}", "b%d" % i),
                    "calib": "winnow-block", "answer": {"Noul": p}})
fx = {"description": "Synthetic fixed observations for probes/winnow (construction check only).",
      "calibrations": [
          {"key": "winnow-error", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗"},
          {"key": "form-topic", "hi": 0.62, "lo": 0.54, "n": 1, "status": "上岗"},
          {"key": "winnow-block", "hi": 0.62, "lo": 0.54, "n": 1, "status": "上岗"}],
      "observations": obs}
json.dump(fx, open("fixture.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(len(obs), "observations")
