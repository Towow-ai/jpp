# Roadmap / 路线图

Updated 2026-09-21. Milestones describe observable results, without speculative release dates.

| Milestone / 阶段 | Status / 状态 | Acceptance / 完成标准 |
|---|---|---|
| Reproducible Python reference / 可复现 Python 对照 | Delivered / 已交付 | Installable package, examples and CI; retained for experiments |
| Standalone source on Rust / 独立源码与 Rust 执行 | Delivered / 已交付 | `.jpp` → shared checking → execution; complete algorithms, native installation and replay. [PR #12](https://github.com/Towow-ai/jpp/pull/12) |
| Method contracts / 方法契约 | Delivered subset / 已交付可用子集 | Explicit type-position effect rows lower to `Type::Method`; old syntax remains compatible; parameter, returned-method and record use are covered. Remaining dynamic limits stay documented in the [core interface](rust/crates/jpp/INTERFACE.md) |
| Reusable source methods / 可复用源码方法 | Initial library delivered / 初版库已交付 | Relative imports load shared method declarations once; two programs reuse the method library. Broader search/selection/continuation library migration remains next |
| External capabilities and lifecycle / 外部能力与运行生命周期 | Fixed path delivered / 固定后端路径已交付 | `judge/gen/do/ask`, JSON file actions, pending → resume → replay share the existing core. [Runnable guide / 运行指南](rust/METHODS-AND-LIFECYCLE.md) |
| Application using standalone source / 用独立源码构造应用 | Next, after library integration / 组合库贯通后 | Move a bounded discovery task onto `.jpp`, show inputs, intermediate results and output, and compare against the existing application |
| Broader composition / 更多独立方法 | Open / 待验证 | Contributors build three substantially different methods without modifying the core |
| Live judgment and portability / 真实判断与后端替换 | Partial / 部分已有 | [Published JEV discovery recordings](docs/towow-demo.zh-CN.md) exist on the retained Python path; a Rust live quickstart and a measured second backend remain open |

The first usable version proceeds through source execution → method/result composition → external capabilities/lifecycle → libraries/basic optimization → integrated release. Preserve the existing design and make necessary local changes. Complete usable paths and required regressions at each step; a full redesign or clearing every known issue is not a prerequisite. Known wrong-result, repeated-action or lost-record defects remain with their implementation owners.

沿已有设计完成源码运行、方法与结果组合、外部能力与生命周期、程序库与基础优化，最后整版交付。当前包已经贯通方法契约、源码复用和固定后端恢复路径；接下来迁移更多已有库方法、接真实适配并完成整版使用路径。必要回归随实现做，不以全面重评或全部问题清零为前置。尚未迁移的发现应用和优化不冒称完成。
