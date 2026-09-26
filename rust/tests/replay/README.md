# 重放一致性（21 §六·1）

重放门禁与金样在同一个测试里跑：`crates/jpp/tests/golden.rs` 的 `examples_match_golden_and_replay` 对 `tests/golden/manifest.json` 的每个用例先记录一次（账本写入临时目录），再用该账本重放一次，断言新增调用 0、提问 0、返回值与首跑相同，重放报告与 `tests/golden/<用例>/replay-report.json` 逐字节相同。登记的重放例外见 `tests/golden/UNCOVERED.md`。

步 19 起加「复用条目不进观察样本数」的断言。

## `r1/`：旧渲染版本的账本（步 15i，B155）

`bank-which_named.ledger.json` 与 `bank-which_named.replay-report.json` 取自改动前（`0e076e6f`）的同名金样，渲染版本 `r1`。`crates/jpp/tests/b155_wire_and_batch.rs` 的 `d_r1金样账本经cli重放` 用新二进制只凭它重放：按账本头的渲染版本算判断键，值、出口、未决清单与当时的重放报告相同，多报一条 `W-header … render_version 旧 r1 新 r2`；`m_导入r1账本报渲染版本` 用它核 `calib-import --from-ledger` 的 `W-render-version`。这两份文件不随金样重录更新。
