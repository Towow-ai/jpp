#!/usr/bin/env python3
"""生成发行附带的能力画像（B73；21 步 15d-0）。

来源：`地基/foundation/profile/profiles/<model>.json`（run.py 实测结果）。
产物：`地基/rust-jpp/profiles/<model>.json` = 来源内容原样 + 宿主策略节 `transport`（非实测）+ 顶层 `provenance` 一节，
写明来源路径、来源的 profile_hash、实测编号（来源的 `sources` 字段）与生成脚本。

`provenance` 不在 behavior_hash 的行为字段表里，所以行为摘要与来源相同；整份的
profile_hash 因多了这一节而与来源不同（账本头记的是发行文件的哈希）。

用法：
  python3 scripts/gen_profiles.py          # 重新生成
  python3 scripts/gen_profiles.py --check  # 只核对产物与来源一致，不一致非零退出
"""
import hashlib, json, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent          # 地基/rust-jpp
SRC_DIR = ROOT.parent / "foundation" / "profile" / "profiles"  # 地基/foundation/profile/profiles
OUT_DIR = ROOT / "profiles"
MODELS = ["jev-1.13.0"]


def canon(obj):
    return json.dumps(obj, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def H(*parts):
    """与 foundation/core/canon.py 的 H 同值：sha256(canon(list(parts)))[:16]。"""
    return hashlib.sha256(canon(list(parts)).encode("utf-8")).hexdigest()[:16]


def build(model):
    src = json.loads((SRC_DIR / f"{model}.json").read_text(encoding="utf-8"))
    out = dict(src)
    # 宿主传输策略（真机传输超时，地基/过程记录/工程-传输超时.md）：不是实测值，只在发行画像里
    out["transport"] = {
        "timeout_s": 30,
        "status": "宿主策略值，非实测",
        "reason": "E8 实测单次调用时延 p50 0.868 s、p95 1.026 s、最大 1.135 s（状态约 300 token、题数 1–200）；大状态未测。"
                  "超时只截断挂起、不截断慢响应，取实测最大值约 26 倍。推翻：真机测得大状态正常响应 p99.9 超过 10 s，按 p99.9 × 3 重定。",
    }
    out["provenance"] = {
        "generated_from": f"地基/foundation/profile/profiles/{model}.json",
        "source_profile_hash": H(src),
        "measurements": list(src.get("sources", [])),
        "measured_at": src.get("date"),
        "generator": "地基/rust-jpp/scripts/gen_profiles.py",
        "note": "发行附带画像（B73）。实测数值与来源逐字段相同；实测编号即 measurements，各项实验见 地基/foundation/experiments/。"
                "另加宿主策略节 transport（timeout_s = 30：宿主策略值，非实测；E8 最大时延 1.135 s；大状态未测）。",
    }
    return json.dumps(out, ensure_ascii=False, indent=2) + "\n"


def main():
    check = "--check" in sys.argv[1:]
    bad = 0
    for m in MODELS:
        text = build(m)
        path = OUT_DIR / f"{m}.json"
        h = H(json.loads(text))
        if check:
            if not path.exists() or path.read_text(encoding="utf-8") != text:
                print(f"[gen_profiles] {path.relative_to(ROOT)} 与来源不一致；运行 python3 scripts/gen_profiles.py 重新生成")
                bad += 1
            else:
                print(f"[gen_profiles] {path.relative_to(ROOT)} 一致（profile_hash {h}）")
        else:
            OUT_DIR.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
            print(f"[gen_profiles] 写了 {path.relative_to(ROOT)}（profile_hash {h}）")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
