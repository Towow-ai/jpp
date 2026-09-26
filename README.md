# J++

![J++ — compose questions and methods](assets/social-card.svg)

**Compose questions. Compose methods. Compose the compositions.**

[简体中文](README.zh-CN.md) · [Why J++](docs/why-jpp.md) · [Ecosystem reassessment](docs/updates/2026-09-23-ecosystem-reassessment.md) · [Progress](docs/progress.md) · [Language design & grammar](docs/design.md) · [Contributing](CONTRIBUTING.md)

J++ is an experimental programming-language project exploring semantic judgment as a programmable operation. Questions are values. Methods are values. A composed method can become a building block in another method.

**Write standalone `.jpp` source and run it with the native Rust implementation.** It parses source, checks language rules and executes methods through one shared kernel. The earlier Python 3.12 embedded implementation remains available as a behavior reference and experiment tool. [Rust package and examples](rust/README.md) · [Language implementation decision](docs/adr/0001-rust-kernel.md).

## Demos / 演示

All demos live on one page: **[towow-ai.github.io/jpp/demos/](https://towow-ai.github.io/jpp/demos/)**. Cards 01–04 replay real runs; every number comes from that run's report and ledger. Card 05 is the exception (see below). None of the cases is fully settled, and the pages say so.

所有演示都在这一页：**[towow-ai.github.io/jpp/demos/](https://towow-ai.github.io/jpp/demos/)**。01–04 回放真机运行，数字来自该次运行的报告与账本；05 例外（见下）。没有一个案子完全定案，页面照实写出。

- **01 Who is lying / 谁在说谎**: eight testimonies compared pair by pair. Cases A and B circle two people that include the culprit; in case C the program names Zhou Lin while the case design says Han Mei; some pairs stay uncertain in all three. / 八份证词两两对质。A、B 两案圈出的两人含真凶，C 案程序认定周琳而出题设定是韩梅，三案都仍有拿不准的证词对。
- **02 Hangzhou dinner / 杭州聚餐**: five people, 576 real OpenStreetMap restaurants; the program filters venues, seats guests and splits groups, about 270–305 judgments and $0.006–0.007 per group. After the relations are filled in, one pair turns red in groups A and C while group B stays uncertain. / 五个人、576 家 OpenStreetMap 真实餐厅，程序筛店、排座、分组，每组约 270–305 次判断、$0.006–0.007。关系补完信息后，甲组、丙组各有一对变红，乙组仍拿不准。
- **03 Shortest reading list / 最短书单**: 30 Wikipedia articles reduced to a list of 6, with 5 concepts still unexplained by any of them. / 30 篇维基百科条目收成 6 篇书单，仍有 5 个概念没有一篇讲清。
- **04 Flat-share matching / 合租分配**: 12 people matched; 8 tie-break orderings give 4 different stable matchings. / 12 人配对，同分时换 8 种排序，得到 4 种稳定匹配。
- **05 Towow network / 通爻网络**: one sentence grows into multi-party plans on a 3D screen, a results page and a phone page. This is demo data, not a real-backend run; every page is marked "假结果 · mock data", and real results will replace it. / 一句话长出多方方案，含 3D 大屏、结果页和手机页。这是演示数据，不是真机运行结果，每页都标了「假结果 · mock data」，真机结果出来后会替换。

## Why we are doing this

We are drawn to a familiar power of algorithms: a few simple operations, organized well, can accomplish something surprisingly complex. JEV led us to ask what happens when semantic judgment joins those operations, alongside exact computation, search and feedback.

Our intuition is that questions and solving methods should be reusable values. A program should be able to construct its next question, accept a method as an argument, and return a method that another program can use. The resulting composition should remain a building block.

We do not yet know every application this will enable. We want others to construct methods we did not anticipate. Working components and executable examples let experience shape the language. [Read the project origin and design motivation](docs/why-jpp.md).

Our first application question comes from Towow: can a fuzzy intent meet different participants' local contexts to produce new cooperation possibilities, with ongoing results and candidate combinations participating in further discovery? [Read the research proposal (中文)](docs/first-problem-towow.zh-CN.md).

The first application is the **Towow discovery lab**. Explore [216 participants and 20 intents](https://towow-ai.github.io/jpp/demos/towow/population/), inspect profiles and compare a semantic-plus-lexical method against BM25, or [disable referral/composition in the ten-person experiment](https://towow-ai.github.io/jpp/demos/towow/lab/). Both pages execute the current J++ Python sources in the browser using published real JEV response recordings. [Animated explanation](https://towow-ai.github.io/jpp/) · [Measurements and reproduction (中文)](docs/towow-demo.zh-CN.md).

The [325-profile real-source comparison](https://towow-ai.github.io/jpp/demos/towow/real/) evaluates seven retrieval/judgment compositions against 963 historical proxy relation labels. Inspect individual candidates, regressions, exact response recordings and offline reproduction. [Results and evaluation scope (中文)](docs/towow-real-relations-results.zh-CN.md).

[Discovery roadmap (中文)](docs/towow-discovery-roadmap.zh-CN.md) records candidate-pool bottlenecks, reusable Towow research assets and the next bounded experiment. Its offline diagnostic script requires no model calls.

The [composable discovery application](https://towow-ai.github.io/jpp/demos/towow/teams/) runs different task plans through the same J++ composition and feeds a two-member proposal back in to nominate a third member. Its API accepts replaceable questions, routing and combination functions. Three synthetic live examples, exact replay and the unsuccessful fixed-slot exploration control are documented in the [iteration report](docs/towow-discovery-iteration.zh-CN.md); this is a bounded application component, not a delivered distributed discovery network.

## Run standalone J++

```sh
git clone https://github.com/towow-ai/jpp.git
cd jpp/rust
cargo build --locked --workspace
cargo run -p jpp -- run examples/composition.jpp
cargo run -p jpp -- run examples/adaptive.jpp --fixtures examples/fixtures/adaptive.json
cargo run -p jpp -- run examples/partial.jpp --fixtures examples/fixtures/partial.json
```

The three programs compose methods, locate 731 among 1,000 candidates in ten
questions, and improve a usable cost-9 candidate combination to cost 2 by changing
the continuation strategy. The source contains the algorithms; the CLI supplies
fixed observations and a local recording action. No model API is called.
[Grammar](rust/FRONTEND.md) · [Source and direct-core equivalence](rust/COMPARISON.md).

Building needs Rust; the installed native executable runs without Python or Cargo.
Use the explicit executable path if the retained Python `jpp` command is also installed.

The native delivery passed 41 Rust tests and GitHub's Rust/Python checks.
[Next steps](ROADMAP.md): reusable source-library methods, consistent composition
rules and an application using standalone source.

## Run the retained Python reference


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

Windows: activate with `.venv\Scripts\activate` instead.

The `methods` command runs complete dynamically constructed methods and saves
their plan, generated structures and execution results. You can also install the
built wheel without a source checkout. [Write your own methods and inspect a run](docs/developer-guide.md).

The demo runs offline with no API key or API charges. It identifies a target among 1,000 candidates using at most 10 adaptive binary questions, then constructs an expression using counterexamples and checks all nine declared inputs. The answers are synthetic and the generator is finite enumeration: this demonstrates the execution and composition mechanisms, not real-model accuracy or a new search algorithm.

## A method stays a method

```python
from jev_compose import component, execute
from jev_compose.fixtures import runtime

@component("length", str, int)
def length(text):
    return len(text)

@component("double", int, int)
def double(n):
    return n * 2

method = length.then(double)
assert execute(method, "hello", runtime()).value == 10
```

The same interface supports methods that ask questions, choose subsequent methods, or iterate over feedback. `inquire(...)` and `feedback(...)` are themselves composition constructors; their strategies can be replaced without changing the runtime.

## What is in the retained Python implementation

| Layer | Current implementation |
|---|---|
| Questions | Test, selection, and measurement; save and restore question values |
| Composition | Sequential composition, branches, products, dynamic method selection, bounded iteration |
| Algorithms | Adaptive inquiry and candidate/check/counterexample feedback |
| Uncertainty | Explicit unresolved observations and conditional count bounds |
| Runtime | Materials, judgment effects, generation/action hooks, budgets, records and replay |
| Distribution | Installable Python package, offline CLI demonstration and tests |

`src/foundation` contains the runtime; `src/jev_compose` contains the composition layer; `src/jpp` provides the distribution entry point. Existing import names remain available while the language design develops.

This is an early alpha. APIs may change. Real-model quality requires separate evaluation; the default demo never contacts JEV. See [current scope and backend notes](docs/status.md).

## Help shape the language

Standalone syntax and Rust execution are delivered. Current work is to resolve correctness findings from review, make source methods reusable, clarify composition rules and extend measured backend evaluation. The Towow examples include published live-backend recordings on the retained Python path. [Dated progress report](docs/progress.md) · [Roadmap](ROADMAP.md).

The most useful contribution is a new method built from existing components, together with an example that runs. Tell us where composition becomes awkward, what you had to duplicate, and which primitive would eliminate that duplication. [Start here](CONTRIBUTING.md).

MIT licensed. J++ is an independent project, not an official JEV/TypeSafe product, and is unrelated to Microsoft's historical Visual J++.
