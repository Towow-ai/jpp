"""验收投影：把 J++ 运行报告的返回值投影成任务书规定的输出形状（A-6 §3「必须通过同一验收测试」）。

J++ 的返回值里有只属于 J++ 的产物（出口编号、材料哈希、pending 责任对象、spent 花费、未决原因字符串）；
任务书没有要求基线产出这些，比较前先去掉。投影只作用在 J++ 一侧：基线直接按任务书的形状输出，
与 `expected.json`（由本投影从 J++ 固定观察运行生成）逐项相等才算通过。

每个投影的字段取舍写在函数注释里；改投影要同时重生 expected.json 并重跑全部基线验收。
T1（步 31-1）：返回值里有 `unobserved`（预算停机没问到的项）时原样带过，T0 程序没有这个键则不带。
"""


def _u(src, dst):
    if isinstance(src, dict) and "unobserved" in src:
        dst["unobserved"] = src["unobserved"]
    return dst


def winnow(v):
    # 折叠存根的文字里含 J++ 的内容哈希，基线无法复现，所以只比被折叠 / 未决的块编号与原因
    return {"task": v["task"],
            "results": [_u(r, {"id": r["id"], "reason": r["reason"], "pruned": r["pruned"],
                               # 步 25-0（B81 (c)）起 J++ 直接返回未决元素；只比它们的原始编号
                               "uncertain": [u["index"] if isinstance(u, dict) else u
                                             for u in r["uncertain"]]}) for r in v["results"]]}


def folio(v):
    s, m = v["single_path"], v["multi_path"]
    # 未决对象本身是 J++ 的出口记录；任务书只要求报未决的道数
    return {"single_path": _u(s, {"leaf": s["leaf"], "path": s["path"], "stopped": s["stopped"]}),
            "multi_path": _u(m, {"leaves": m["leaves"], "dead_ends": m["dead_ends"], "layers": m["layers"],
                                 "undecided": len(m["undecided"])})}


def entity_align(v):
    # duty（未决责任对象）去掉；其余逐对比较
    return _u(v, {"candidates": v["candidates"], "tally": v["tally"],
                  "routed": [{"pair": r["pair"], "outcome": r["outcome"], "hints": r["hints"]} for r in v["routed"]]})


def refund(v):
    # 编号一律是原始对话编号；未决只比编号，不比原因字符串
    return _u(v, {"urgent": [e["index"] for e in v["urgent"]],
                  "refund_calm": [e["index"] for e in v["refund_calm"]],
                  "no_refund": v["no_refund"],
                  "review": [e["index"] for e in v["review"]],
                  "urgent_count": v["urgent_count"]})


def interview(v):
    return _u(v, {"pairs_asked": v["pairs_asked"], "plan": v["plan"], "unplaced": v["unplaced"],
                  "review": [{"candidate": r["candidate"], "interviewer": r["interviewer"]} for r in v["review"]],
                  "rejected": v["rejected"]})


PROJECTIONS = {"winnow": winnow, "folio": folio, "entity-align": entity_align,
               "refund": refund, "interview": interview}
