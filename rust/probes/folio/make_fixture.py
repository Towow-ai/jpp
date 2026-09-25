"""Fixed-observation fixture for folio.jpp (synthetic readings; construction check only)."""
import json
# document and ontology come from the file `jpp run --input` gives the program (step 14b-0)
M = json.load(open("baseline/materials.json", encoding="utf-8"))
doc, tree = M["document"], M["tree"]
SEL = "这份法律文书最应归入下列哪一类？"
TOPIC = "这段话的内容是否与「{concept}」这个话题相关？"
obs = []
# strategy A: select path root -> 合同与协议 -> 租赁合同 -> 住宅租赁合同, permutations tested (K=2, agree)
for node, probs in [("法律文书", [0.93, 0.03, 0.02, 0.02]), ("合同与协议", [0.90, 0.07, 0.03]),
                    ("租赁合同", [0.88, 0.10, 0.02])]:
    obs.append({"on": [doc], "over": tree[node], "op": "select", "text": SEL, "calib": "folio-level",
                "answer": {"Choice": probs}, "perms": 2, "mode_share": 1.0})
# strategy B: per-child test readings
p = {"合同与协议": 0.95, "诉讼文书": 0.05, "公司治理文件": 0.57, "知识产权文件": 0.04,
     "租赁合同": 0.97, "买卖合同": 0.30, "劳动合同": 0.08,
     "住宅租赁合同": 0.96, "商铺租赁合同": 0.58, "设备租赁合同": 0.03}
for c, v in p.items():
    obs.append({"on": [doc], "op": "test", "text": TOPIC.replace("{concept}", c), "calib": "form-topic",
                "answer": {"Noul": v}})
json.dump({"description": "Synthetic fixed observations for probes/folio (construction check only).",
           "calibrations": [{"key": "folio-level", "hi": 0.75, "lo": 0.25, "n": 1, "status": "上岗"},
                            {"key": "form-topic", "hi": 0.62, "lo": 0.54, "n": 1, "status": "上岗"}],
           "observations": obs}, open("fixture.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(len(obs))
