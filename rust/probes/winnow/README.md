# 阶段 5 探针 / Stage-5 reproduction probes

评估与缺口见 / Evaluation and gaps: `地基/过程记录/阶段5-探针-首轮.md`.

| 目录 | 复现对象 | 固定观察 | 真机 |
|---|---|---|---|
| `winnow/` | GhalebDweikat/winnow（工具输出逐块筛选） | `jpp run winnow.jpp --input baseline/materials.json --fixtures fixture.json` | `--backend live --calib <form-topic 线目录>`；报错题的线在 `truth/calib/` |
| `folio/` | jev-folio-recursive-classifier（本体逐层下探） | `jpp run folio.jpp --input baseline/materials.json --fixtures fixture.json` | `--backend live --calib <form-topic 线目录>` |
| `entity-align/` | TypeSafe cookbook entity alignment（候选对匹配） | `jpp run align.jpp --input baseline/materials.json --fixtures fixture.json` | `--backend live` |

在各自目录下运行。材料经 `--input baseline/materials.json` 交给程序（名字 `input`，叶子不可信；步 14b-0，此前各程序把材料写成字面量或用 `read_json` 读），基线读同一份文件；真机运行同样要加 `--input`。`make_fixture.py` 读同一份材料生成夹具，读数为合成值，只检验构造。目录里的 `live-*.ledger.jsonl` 与 `fixed-*.ledger.jsonl` 是步 14b-0 之前的程序记下的，判断键含调用位置，重放要用那时的提交（`1e5d7aeb`）。
Run from each directory. Materials reach the program through `--input baseline/materials.json` (bound as `input`, untrusted leaves; step 14b-0), the same file the baselines read. Fixture readings are synthetic (construction checks only). The ledgers kept here were recorded by the programs before step 14b-0; replay them at that commit.
