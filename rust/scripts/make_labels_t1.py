"""生成 T1 任务书用的标注：选线集 `baseline/t1/labels.jsonl`（交给程序）与留出集 `t1/labels-cert.jsonl`
（考官持有，程序看不见）。合成真值，构造检验用，不是模型评测；不计行。

第二版（步 31-1b，B96）。第一版（步 31-1）让拆分认证 `certify_ref.py` 得出的线与 T0 手写阈值给出同一出口，
T1 (d) 验收要求逐位对上参考实现；那份标注冻结为 `t1/labels-strict.jsonl`，只供 `t1-strict` 档的旧验收用。
B96 改为性质验收：线只要在留出集上满足「每侧 n ≥ 22、Clopper–Pearson 上界 ≤ α」即过 ①，方法不限；
于是标注必须保证「任何过 ① 的线在固定观察的读数上给出同一出口」（验收 ③），否则一个合法的保守线也会改出口。
第一版做不到：清楚区一直铺到 1（取更高的线也有 22 条），混杂点真假各半（n 大时吞进一个混杂点上界仍 ≤ α）。

第二版的构造（每个键、每一侧；h 是用线时的比较点 hi + δ，l 是 lo − δ）：
  读数带：固定观察里 T0 判「是」的最小读数 B_h，其余读数的最大值 A_h；「否」一侧对称（A_l 是判「否」的最大读数，
  B_l 是其余读数的最小值）。任何 h ∈ (A_h, B_h]、l ∈ [A_l, B_l) 都给出与 T0 相同的出口。
  墙：在 W_h = h0 − min(0.02, (h0 − A_h)/2) 放 WALL 条真值为否的行（h0 是 T0 的比较点，W_h 落在 (A_h, h0) 内）；
      下侧在 W_l = l0 + min(0.02, (B_l − l0)/2) 放 WALL 条真值为是的行。任何 h ≤ W_h 都把墙吞进已决集，错误率 > α，过不了。
  核心：W_h 与 B_h 之间 CORE 条真值为是；A_l 与 W_l 之间 CORE 条真值为否。
  尾：B_h 以上 TAIL 条真值为是、A_l 以下 TAIL 条真值为否，TAIL < 22，所以 h > B_h 或 l < A_l 的线已决不足 22 条，过不了。
  K 选一与打分只有上侧；W_h 以下另放 CORE 条 argmax 错误的行。
  行与读数、墙之间留 GAP 的间隔（取值四位小数），避免浮点往返把读数分到两边。
选线集与留出集同一构造、不同种子（行的位置随机，条数固定）。

生成后自检（`t1_cert.self_check`）：在留出集上枚举全部阈值等价类，每个通过检验的阈值都必须让每条固定读数
落在 T0 出口的一侧，且离区间端点至少 1e-4；不满足即报错退出。另核：`jpp calib-import`（J++ 侧实际用的认证）、
`certify_ref`（拆分法）、零错误计数与全集最大覆盖 CP 两种简单方法在选线集上给出的线都过留出检验。

用法：python3 make_labels_t1.py            生成两份标注并自检
      python3 make_labels_t1.py --check    只自检现有文件
"""
import json
import pathlib
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import certify_ref  # noqa: E402
import t1_cert  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent
PROBES = ROOT / "probes"
JPP = ROOT / "target" / "debug" / "jpp"
FIXTURES = {
    "winnow": PROBES / "winnow/fixture.json",
    "folio": PROBES / "folio/fixture.json",
    "entity-align": PROBES / "entity-align/fixture.json",
    "trial-refund": ROOT.parent / "评估/2026-09-24-试写/refund/fixture.json",
    "trial-interview": ROOT.parent / "评估/2026-09-24-试写/interview/fixture.json",
}
DELTA = {"test": 0.05, "select": 0.15, "measure": 0.15}
WALL, CORE, TAIL = 15, 60, 12
GAP = 0.005
SEEDS = {"labels.jsonl": "t1-labels-v2-select", "labels-cert.jsonl": "t1-labels-v2-cert"}


def p_of(o):
    a = o["answer"]
    return a["Noul"] if "Noul" in a else max(a.get("Choice") or a.get("Score"))


def t0_exit(o, cal):
    """T0 手写阈值的出口（任务书 T0「阈值规则」）：act / ignore / unsure。"""
    p, d = p_of(o), DELTA[o["op"]]
    if o["op"] == "test":
        return "act" if p >= cal["hi"] + d else ("ignore" if p <= cal["lo"] - d else "unsure")
    ms = o.get("mode_share", o["answer"].get("mode_share"))
    if o["op"] == "select" and ms is not None and ms != 1.0:
        return "unsure"
    return "act" if p >= cal["hi"] + d else "unsure"


def readings_of(fx):
    """{键: (op, T0 记录, [(p, T0 出口)])}；mode_share 不为 1 的 K 选一读数与线无关，不进自检。"""
    cal = {c["key"]: c for c in fx["calibrations"]}
    out = {}
    for o in fx["observations"]:
        k = o["calib"]
        ms = o.get("mode_share", o["answer"].get("mode_share"))
        if o["op"] == "select" and ms is not None and ms != 1.0:
            continue
        out.setdefault(k, (o["op"], cal[k], []))[2].append((round(p_of(o), 6), t0_exit(o, cal[k])))
    return out


def geometry(op, cal, rd):
    d = DELTA[op]
    h0 = cal["hi"] + d
    yes = [p for p, e in rd if e == "act"]
    not_yes = [p for p, e in rd if e != "act"]
    a_h = max(not_yes) if not_yes else d
    b_h = min(yes) if yes else 1.0
    g = {"w_h": h0 - min(0.02, (h0 - a_h) / 2), "a_h": a_h, "b_h": b_h}
    if op == "test":
        l0 = cal["lo"] - d
        no = [p for p, e in rd if e == "ignore"]
        not_no = [p for p, e in rd if e != "ignore"]
        a_l = max(no) if no else 0.0
        b_l = min(not_no) if not_no else 1.0
        g.update({"w_l": l0 + min(0.02, (b_l - l0) / 2), "a_l": a_l, "b_l": b_l})
        assert g["w_l"] < g["w_h"], (op, g)
    return g


def rows_for(key, op, g, rng):
    out = []

    def add(p, ok):
        p = round(min(max(p, 0.0), 1.0), 4)
        n = len(out)
        if op == "test":
            out.append({"key": key, "item": f"{key}-{n:04d}", "p": p, "label": ok, "source": "computed"})
        else:
            pick = rng.randrange(3)
            out.append({"key": key, "op": op, "item": f"{key}-{n:04d}", "p": p, "pick": pick,
                        "label": pick if ok else (pick + 1) % 3, "source": "computed"})

    def spread(lo, hi, n, ok):
        for _ in range(n):
            add(lo + (hi - lo) * rng.random(), ok)

    gap_h = min(GAP, (g["b_h"] - g["w_h"]) / 4)
    spread(g["w_h"] + gap_h, g["b_h"] - gap_h, CORE, True)        # 上侧核心：是
    if g["b_h"] < 1.0:
        spread(min(g["b_h"] + gap_h, 1.0), 1.0, TAIL, True)       # 上侧尾：是，不足 22 条
    for _ in range(WALL):
        add(g["w_h"], False)                                      # 上侧墙：否
    if op == "test":
        gap_l = min(GAP, (g["w_l"] - g["a_l"]) / 4)
        spread(g["a_l"] + gap_l, g["w_l"] - gap_l, CORE, False)  # 下侧核心：否
        if g["a_l"] > 0.0:
            spread(0.0, max(g["a_l"] - gap_l, 0.0), TAIL, False)  # 下侧尾：否，不足 22 条
        for _ in range(WALL):
            add(g["w_l"], True)                                   # 下侧墙：是
    else:
        spread(0.34, g["w_h"] - gap_h, CORE, False)              # K 元：墙下 argmax 错
    return out


def generate():
    for proj, path in FIXTURES.items():
        fx = json.loads(path.read_text(encoding="utf-8"))
        rd = readings_of(fx)
        for name, seed in SEEDS.items():
            rng = random.Random(f"{seed}-{proj}")
            rows = []
            for key in sorted(rd):
                op, cal, r = rd[key]
                rows += rows_for(key, op, geometry(op, cal, r), rng)
            dst = PROBES / proj / ("baseline/t1" if name == "labels.jsonl" else "t1") / name
            with open(dst, "w", encoding="utf-8") as fh:
                for x in rows:
                    fh.write(json.dumps(x, ensure_ascii=False) + "\n")
            print(proj, name, len(rows), "行")


# —— 简单方法（自检用：选线集上几种合理写法得出的线都要过留出检验）——

def zero_error(op, smp, d):
    """全集零错误计数：上侧取已决全对且 ≥ NEED 的最低比较点；下侧对称。"""
    ps = sorted({p for p, _ in smp})
    h = next((c for c in ps if all(t for p, t in smp if p >= c) and sum(p >= c for p, _ in smp) >= t1_cert.NEED), None)
    line = {"hi": h - d} if h is not None else {}
    if op == "test":
        l = next((c for c in reversed(ps) if not any(t for p, t in smp if p <= c)
                  and sum(p <= c for p, _ in smp) >= t1_cert.NEED), None)
        if l is not None:
            line["lo"] = l + d
    else:
        line["lo"] = 0.0
    return line


def max_cover_cp(op, smp, d):
    """全集最大覆盖：上侧取 U(k, n) ≤ α 且 n 最大的比较点；下侧对称。"""
    ps = sorted({p for p, _ in smp})
    ups = [c for c in ps if t1_cert.side_ok([s for s in smp if s[0] >= c], True)[0]]
    line = {"hi": min(ups) - d} if ups else {}
    if op == "test":
        dns = [c for c in ps if t1_cert.side_ok([s for s in smp if s[0] <= c], False)[0]]
        if dns:
            line["lo"] = max(dns) + d
    else:
        line["lo"] = 0.0
    return line


def calib_import_lines(labels):
    if not JPP.exists():
        return None
    with tempfile.TemporaryDirectory(dir=str(ROOT / "target")) as t:
        p = subprocess.run([str(JPP), "calib-import", str(labels), "--calib-out", t, "--profile", str(ROOT / "profiles" / "jev-1.13.0.json")], capture_output=True, text=True)  # 15d-2 起必带画像
        if p.returncode != 0:
            return {"_error": p.stderr[-300:]}
        out = {}
        for f in pathlib.Path(t).glob("*.json"):
            r = json.loads(f.read_text(encoding="utf-8"))
            if r.get("status") != "待真值":
                out[r["key"]] = {"hi": r["hi"], "lo": r["lo"]}
        return out


def check():
    bad = []
    for proj, path in FIXTURES.items():
        fx = json.loads(path.read_text(encoding="utf-8"))
        rd = {k: v[2] for k, v in readings_of(fx).items()}
        sel = t1_cert.load_rows(PROBES / proj / "baseline/t1/labels.jsonl")
        cert = t1_cert.load_rows(PROBES / proj / "t1/labels-cert.jsonl")
        deltas = json.loads((PROBES / proj / "baseline/t1/materials.json").read_text(encoding="utf-8"))["deltas"]
        for name, rows in (("labels.jsonl", sel), ("labels-cert.jsonl", cert)):
            bad += [f"{proj} {name} {x}" for x in t1_cert.self_check(rows, rd)]
        smp = t1_cert.samples(sel)
        methods = {
            "certify_ref（拆分）": {k: v for k, v in certify_ref.certify(sel).items() if "hi" in v},
            "零错误计数": {k: zero_error(op, s, deltas[k]) for k, (op, s) in smp.items()},
            "最大覆盖 CP": {k: max_cover_cp(op, s, deltas[k]) for k, (op, s) in smp.items()},
        }
        ci = calib_import_lines(PROBES / proj / "baseline/t1/labels.jsonl")
        if ci is not None:
            methods["jpp calib-import"] = ci
        for m, lines in methods.items():
            res = t1_cert.cert_check(lines, cert, deltas)
            fails = [f"{k}（{why}）" for k, (ok, why) in res.items() if not ok]
            print(f"[{proj}] {m}：{'全过' if not fails else '不过 ' + '；'.join(fails)}")
            bad += [f"{proj} {m} {x}" for x in fails]
    for b in bad:
        print("自检失败：", b)
    return not bad


if __name__ == "__main__":
    if "--check" not in sys.argv:
        generate()
    ok = check()
    print("自检", "通过" if ok else "失败")
    sys.exit(0 if ok else 1)
