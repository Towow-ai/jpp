# J++

![J++：组合问题与方法](assets/social-card.svg)

**组合问题，组合方法，再组合这些组合。**

[English](README.md) · [为什么做 J++](docs/why-jpp.zh-CN.md) · [生态复盘](docs/updates/2026-09-23-ecosystem-reassessment.md) · [当前进度](docs/progress.md) · [语言设计与文法](docs/design.md) · [参与贡献](CONTRIBUTING.md)

J++ 是一门正在开发的实验性编程语言。我们想让语义判断成为可以编程的基本操作：问题可以保存、传递和组合；求解方法也可以保存、传递和组合；复杂方法封装以后，仍然可以成为下一个方法的基本单元。

**现在可以直接写 `.jpp` 源码，用原生 Rust 程序检查并运行。** 函数、问题和组合方法交给同一个内核执行。此前的 Python 3.12 嵌入式实现保留为行为对照和实验工具。[Rust 安装与完整例子](rust/README.md) · [语言实现路线](docs/adr/0001-rust-kernel.md)。

## 我们最初感受到的直觉

我们一直被算法的一种力量吸引：少量基本操作，经过合适的组织，可以处理令人惊讶的复杂问题。JEV 让我们想到，如果语义判断也能加入这些基本操作，与精确计算、搜索、生成和反馈结合，会出现什么？

我们希望问题本身可以成为输入，方法本身可以成为参数。程序能选择下一道问题，也能构造下一段方法；一组组合完成之后，仍然可以参与更大的组合。我们还不知道它最后会带来哪些应用，但希望别人能用这些基本单元做出设计者没有想到的东西。

这就是我们开始做 J++、并选择早期开源的原因。[阅读完整的项目起源与设计动机](docs/why-jpp.zh-CN.md)。

我们的第一个应用构想来自通爻：一个模糊意图能否在不同主体的上下文中产生合作可能，而这些进行中的计算和候选组合又继续参与发现？[阅读问题与构想](docs/first-problem-towow.zh-CN.md)。

第一个应用是**通爻发现实验台**：[216 人 × 20 种意图](https://towow-ai.github.io/jpp/demos/towow/population/)可以切换意图、查看具体人物和词面检索对照；[十人组合实验](https://towow-ai.github.io/jpp/demos/towow/lab/)可以关闭转介或组合，检查哪些结果消失。两个页面都可以在浏览器执行当前 J++ 源码，模型层使用已公开的真实 JEV 回答录制。原有[图谱动画与介绍](https://towow-ai.github.io/jpp/)继续保留。[案例、实测速度与复现方法](docs/towow-demo.zh-CN.md)。

新增[325 人真实来源关系对照](https://towow-ai.github.io/jpp/demos/towow/real/)：在同一批职业资料上比较七种检索与判断组合，核对 963 条旧关系标签，逐人查看找到与漏掉的候选。公开真实调用录制、全部结果和离线复现程序。[结果与历史口径](docs/towow-real-relations-results.zh-CN.md)。

[网络发现的下一步](docs/towow-discovery-roadmap.zh-CN.md)：定位候选入口与排序的漏失，整理旧通爻研究中可复用的语料、转介、多人组合和动态网络接口，提供零 API 费用的诊断脚本。

[可续接的发现应用](https://towow-ai.github.io/jpp/demos/towow/teams/)用同一套 J++ 组合运行不同任务，并把两人提议重新输入，继续发现第三位成员。接口支持替换问题、路由与组合函数。[本轮实现与复现](docs/towow-discovery-iteration.zh-CN.md)保留三个合成样本真实运行、精确回放，以及没有改善结果的固定名额探索对照。当前交付为有界应用组件，尚未实现分布式发现网络。

## 直接运行 J++ 源码

```sh
git clone https://github.com/towow-ai/jpp.git
cd jpp/rust
cargo build --locked --workspace
cargo run -p jpp -- run examples/composition.jpp
cargo run -p jpp -- run examples/adaptive.jpp --fixtures examples/fixtures/adaptive.json
cargo run -p jpp -- run examples/partial.jpp --fixtures examples/fixtures/partial.json
```

三个程序分别展示：组合方法再参与组合；自己选择十道问题，在 1,000 个候选里找到
731；先取得成本 9 的可用组合，再换一种策略继续问，得到成本 2 的方案，旧检查不
重复做。算法写在 `.jpp` 源码中，运行器只提供固定观察和记录动作，本次不调用模型。
[源码文法](rust/FRONTEND.md) · [与直接构造内核程序的等价对照](rust/COMPARISON.md)。

构建时需要 Rust；安装后的原生程序无须 Python 或 Cargo。若同时安装了旧 Python
版本，两者都叫 `jpp`，请使用原生程序的完整路径区分。

## 运行保留的 Python 对照


```sh
git clone https://github.com/towow-ai/jpp.git
cd jpp
python3.12 -m venv .venv
source .venv/bin/activate
python -m pip install -e '.[dev]'
jpp demo
jpp methods --output method-report.json
python -m pytest -q
```

Windows 使用 `.venv\Scripts\activate` 激活环境。

`methods` 运行完整的动态方法构造、嵌套和内部步骤替换，将共同计划、实际生成结构与结果写到指定文件。也可以直接安装 wheel，无须研究源码。[开发者使用与调试指南](docs/developer-guide.md)。

演示完全离线，无须 API Key，不产生模型费用。它做两件事：在 1,000 个候选里，通过最多 10 次二分提问定位目标；通过“生成候选—运行检查—吸收反例”构造一个表达式，并检查全部九个指定输入。模型回答采用合成数据，生成器采用有限枚举，展示的是组合机制如何运行，不能据此推断真实模型准确率。

## 我们现在正在做什么

公开 Python alpha 的安装入口、组合库和通爻案例继续可用。独立 J++ 源码到 Rust 执行已贯通：方法定义与组合、自适应选问，以及未决候选接精确组合并继续求解。首包使用固定观察验证执行机制；真实模型表现和更大规模应用需要各自的实验。

原生源码整包的 41 项 Rust 测试通过，GitHub 上 Rust、Python 3.12 与 3.13 的检查通过。[进度页](docs/progress.md)记录具体交付；[路线图](ROADMAP.md)明确下一步：源码方法标准库、组合规则统一，以及使用独立源码的应用。

## 方法怎样继续组合

直接调用一次判断接口，可以得到一个答案。我们想进一步做到：开发者写出一种求解方法以后，别人能把这整个方法作为一个参数，嵌入另一种方法，继续构造更复杂的程序。

因此，我们同时研究问题本身如何表达、方法之间如何连接、不确定性如何保留，以及精确算法如何与模型判断配合。语言的价值，要由别人使用它构造出的程序体现。

保留的 Python 实现已有顺序组合、分支、动态选择方法、迭代、问题序列化，以及自适应提问和反例反馈两种算法构造器。其入口是 `foundation.jv` 和 `jev_compose`，`jpp demo` 是旧版体验入口。原生源码入口位于 `rust/`；两者能力不自动视为完全相同。

我们最希望收到这样的贡献：用已有基本单元构造一种新方法，附上能运行的例子，并告诉我们哪些地方还需要重复劳动。[贡献指南](CONTRIBUTING.md)列出了具体入口。

这是早期 alpha 版本，接口仍在发展。项目采用 MIT 许可证；它不是 JEV/TypeSafe 官方产品，也与历史上的 Microsoft Visual J++ 无关。
