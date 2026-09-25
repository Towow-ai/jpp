# 重放一致性（21 §六·1）

重放门禁与金样在同一个测试里跑：`crates/jpp/tests/golden.rs` 的 `examples_match_golden_and_replay` 对 `tests/golden/manifest.json` 的每个用例先记录一次（账本写入临时目录），再用该账本重放一次，断言新增调用 0、提问 0、返回值与首跑相同，重放报告与 `tests/golden/<用例>/replay-report.json` 逐字节相同。登记的重放例外见 `tests/golden/UNCOVERED.md`。

步 19 起加「复用条目不进观察样本数」的断言。
