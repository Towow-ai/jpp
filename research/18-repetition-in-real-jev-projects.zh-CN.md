# 18 别人怎么用 JEV：重复的部分

2026-09-27。依据 `research/地基/00-Nature意图汇编.md` 第 22 条：「分析现在所有的人，他们都是怎么用 JEV 的，然后发现里面重复的部分地方……我不能只是讲抽象地说『你要写要判断什么』，就是没有感受的。我们要讲一些例子……原来现在这么多的项目里面，他们都是这么做的，他们重复的地方是什么地方。那为什么我们提出的这个机制……能够让它不做那个重复？然后效果是什么？」「假设啊，你要论证，你要讲『怎么占了他们大部分的开发量』」。

方法与判据见 `research/地基/00-定位与方法论-v1.md`、`research/地基/00-目标与动机-v1.md`。材料来自 84 个真实开源项目的原始代码与 J++ 改写对照（选法与逐项改写记录见 `research/2026-09-26-rewrite-study.zh-CN.md`，84 个真实 JEV 项目改写逐字段比对的公开报告（可交互版 https://jpp.towow.net/rewrite-study/）；逐项目原始代码与改写代码本身是内部记录，未随本次公开）与内部生态快照（4041 个公开仓库快照，未随本次公开）。脚本在 `research/scripts/busywork_classify.py`（84 项目分类）与 `research/scripts/busywork_ecosystem_sample.py`（生态抽样，子代理新写）；数据在 `research/data/2026-09-27-repetition/`。

术语说明：本文写「运行时接管的八件事」处指 `00-定位与方法论-v1.md` 术语表里的「缝」，2026-09-27 起改叫这个名字，不用「缝」这个比喻（意图汇编第 16 条追加）。

---

## 一、假设与预测

Nature 的假设：围绕 JEV 调用写的「重复的杂活」占了这些项目大部分的开发量。把它拆成三条可证伪的预测，写在这里，下面「三、数」一节报实测结果，不倒过来改预测去凑。

**写作顺序**：本节的三条预测是在跑出任何统计数字之前定下的（照抄任务书给的例子操作化）。分类脚本本身是边写边看汇总数字调出来的——写完第一版正则、跑出「core 4/12、file 6/12 类命中 ≥30%」之后，加了 scope=host 和「三个范围取并集」的计分规则，把命中数拉到 8/12；后来自查时又发现两处问题：一是逐行关键词匹配会把一整段几十行的重试代码只算成两三行（据此把脚本从「逐行关键词」改成「块级归属 + 判断调用上下文邻近」，v2，见下文）；二是「校准与标注」类的正则里 `annotation` 这个词会命中 Python 极常见的语言写法 `from __future__ import annotations`（跟「标注数据」毫无关系），这个假阳性几乎抬高了这一类的每一个 Python 项目的命中——修掉之后这一类的命中数从 34/81（42.0%）掉到 8/81（9.9%），P2 的命中类别数从 8/12 掉到 7/12。这三次改动都发生在预测写下之后、最终数字出来之前，方向都是修正看得出来的方法缺陷；但「预测先写、方法不再动」这条纪律在方法细节上没有守住，最后一次修正直接把 P2 的结论从「成立」翻成「不成立」，这一点必须写清楚，不能因为已经写过一次「成立」就沿用旧结论。

**P1（行占比）**：在 84 个项目的判断核心（`measure.toml` 里 `[[core]]` 登记的精确行号范围）里，杂活行数占核心总行数的中位数 ≥ 50%。

**P2（类别广度）**：12 类杂活里，至少 8 类各自在 ≥ 30% 的项目里出现过（出现与否，不要求行数）。

**P3（生态外推）**：从 4041 个公开仓库快照里抽 100 个真的调用了 JEV 的仓库，只看「出现与否」，12 类的出现率排序与 84 个精选项目大体一致（差距不超过一两个名次的量级），说明 84 个精选项目不是靠挑样本挑出来的结果。

### 关于口径的说明（先说清楚，避免预测题目本身失真）

`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/） 对「判断核心」的定义是：「哪些代码是在定义题、调判断接口、按阈值分流、组合多道题的结果——这部分叫『核心』。命令行解析、网络请求、日志、重试这类和判断逻辑无关的代码不算核心」。按这个定义，杂活里的第 4、6、7 类（重试与失败、费用计数、并发限制）本来就被排除在「核心」之外，只可能落在「宿主共享」代码或核心文件里核心行号范围以外的部分。如果只用最窄的核心口径去数 P1，会系统性低估这三类杂活，让 P1 看起来比实际更容易被推翻。

为此本文同时用三种范围计数，都写明白，不只报一个数：

- **scope=core**：`measure.toml` `[[core]]` 精确行号范围。这是改写报告用的「判断核心」定义，最窄。
- **scope=file**：`[[core]]` 指向的文件，取整份文件（不截断到登记的行号），补上核心口径因定义排除、但实际写在同一个文件里的杂活。
- **scope=host**：`[[host_shared]]` 登记的文件（51/84 个项目登记过），取整份文件。这是改写流程明确定义为「与判断逻辑无关、但两边都要写」的胶水代码，命令行解析、网络请求、日志、重试大多按定义该落在这里。

P1 只对 scope=core 计分（这是任务书给的操作化定义，最贴近「判断核心」本身）；P2 用三个范围的并集计分（一个类只要在核心、核心文件、或宿主文件任一处出现过，就算这个项目「出现」这一类），因为窄口径会人为排除按定义就该落在宿主代码里的类别。

## 二、十二类杂活的定义与识别规则

「杂活」指：跟这个项目具体要判断什么（业务本身，比如「这条工单该不该转人工」这句话本身、这个领域的具体规则）无关，只要接入 JEV 就得写一遍、换个项目还得再写一遍的代码。识别规则以关键词/正则为主（脚本用的正则见 `research/scripts/busywork_classify.py`），允许语言相关的写法差异；每行代码只归一类用于算占比（按下面的优先顺序取第一个命中的类），但「出现与否」按多标签算，一行同时像两类就两类都记「出现」。

| 编号 | 类别 | 识别规则 |
|---|---|---|
| 1 | 调用循环与合批 | 逐条调用判断接口的循环；`Promise.all`/`asyncio.gather`/线程池/进程池；手动攒批（把多条攒进列表/队列再一起发） |
| 2 | 门槛 | 把概率/置信度/分数与写死的常数比较（`confidence > 0.7`）；这类常数字面量定义在调用方代码里 |
| 3 | 拿不准的处理 | 概率落中间地带时走的 else 分支；把结果打「uncertain/unsure/ambiguous/needs_review」标签；直接丢弃拿不准的结果 |
| 4 | 重试与失败 | 包住判断调用的 try/except（try/catch）；重试、退避、超时；调用失败后用的默认值 |
| 5 | 缓存 | dict/Map/lru_cache/文件/数据库缓存判断结果，按题目与材料内容算键或哈希 |
| 6 | 费用与调用计数 | 调用次数计数器、花费/token 统计、预算上限检查、专门为此记的日志 |
| 7 | 并发限制 | 信号量、限速器、并发上限参数 |
| 8 | 题面拼装 | 字符串拼接/模板/f-string 把问题和候选项拼成发给 JEV 的文本 |
| 9 | 材料裁剪 | 截断、分块、只取前 k 段、拼接上下文窗口，为了塞进请求里 |
| 10 | 结果回流 | 把上一轮判断结果写回、作为下一次判断的材料或题面的一部分（多轮循环、递进判断） |
| 11 | 多个判断的合成 | 对多道题的结果投票、加权、级联（先便宜后贵） |
| 12 | 校准与标注 | 人工标注数据集文件、调阈值脚本、ROC/AUC/precision-recall 这类校准评估代码 |

残余类「业务本身」：跟判断逻辑无关的领域代码（具体的判定文字、跟 JEV 无关的算术、跟这个项目本身相关但与「怎么接 JEV」无关的逻辑）。这一类不计入杂活占比的分子，只作分母的参照，不单独报数。

### 分类方法：两个版本，v2 是主口径

脚本有两版实现，都保留在 `research/scripts/busywork_classify.py` 里，报告两版数字都给，不藏掉旧版本：

- **v1（朴素逐行关键词）**：每行代码单独跑一遍 12 组正则，命中就算这一行是这一类。问题在校准时才看出来：一段 31 行、包含截止时间、错误分类、退避计算的完整重试逻辑（`jev_commit/jev.py:118-148`，即本文例 7），v1 只命中 `try:`/`except:` 那两三行，中间的业务判断、错误封装全部落进「业务本身」，把一整段杂活算成了几行；反过来，`for x in y:`、`try:`、`.catch(` 这类词在任何代码里都极常见，不少命中和 JEV 调用毫无关系（比如 `sathariels/jevtriage`（项目 B1-05）里 `except (TypeError, ValueError)` 包的是参数解析，不是判断调用）。v1 算出的「杂活行占比」因此系统性偏低，三个范围（core/file/host）算出几乎同一个中位数（6.1%），这个巧合本身就是方法有问题的信号，不是巧合。
- **v2（块级归属 + 判断调用上下文邻近，本文正文用这一版）**：对「调用循环与合批」「重试与失败」「并发限制」三类——这三类的本体是一段控制流块，不是一行——先找块的开启行（`for`/`try`/`with Semaphore` 等），只在开启行邻近（±20 行内）出现判断调用的上下文标记（`system_one`、`Choice(`、`noul(`、`confidence`、`criteria=` 等）时，才把整个块（Python 按缩进、花括号语言按括号配对找块尾）算进这一类；同时「调用循环」「重试」「费用计数」「并发限制」「题面拼装」五类都要求命中处邻近判断调用上下文，减少「任意一段不相关代码里的 try/except」这种假阳性。用 B3-01 的同一段代码验证：v1 记 225 行核心里 4 行杂活（1.8%），v2 记 66 行（29.3%）——杂活的定义没有变宽，只是把本来就该算进去的整块代码补全了。

局限：v2 的花括号块尾用朴素括号计数找，不处理字符串/注释里的花括号，会有噪声；「判断调用上下文」本身也是一组关键词代理，不做语义理解；这仍然是一个廉价代理，不是真值。为独立核验分类脚本的准确度，另派一个子代理不看脚本、不看脚本输出，对 10 个项目（覆盖 Python/JS/TS/Go/Rust/Dart 六种语言）做盲人工标注，进度与结果见「三、3」。

## 三、数

### 3.1 84 个项目（P1、P2）

84 个项目里 81 个有 `measure.toml`（3 个没有：`B2-05-hermes` 记录缺失、`B3-07-botcraft` 因原项目自身真实缺陷被判定「不可比」、`Q-05-jev-torneo-animales` 记录缺失）。81 个里又有 4 个（`B1-11`、`B4-02`、`B6-11`、`B7-01`）因为原仓库无许可证，`original/` 不放代码副本，`measure.toml` 里没有 `[[core]]` 条目（核心行数改写记录里是手写常量，见各自 `result.json`），本脚本读不到本地源码，无法参与分类。**能参与分类的项目实际是 77 个**，占 84 个总样本的 92%；行占比汇总排除没有可数行的记录。但出现率沿用公开脚本的汇总规则，保留 CSV 全部 81 条记录（含上述 4 条零行记录），并非只以 77 个可分类项目为分母。

**P1（行占比 ≥ 50%）：两版方法都不成立，v2（更准的那版）把差距缩小了但仍差得远。**

| 版本 | scope=core 中位数 | scope=file 中位数 | scope=host 中位数 |
|---|---|---|---|
| v1（朴素逐行，已知低估） | 6.1% | 6.1% | 6.1% |
| v2（块级归属+上下文，本文口径） | **11.1%** | 12.6% | 8.2% |

v1 三个范围给出几乎同一个数字不是巧合，是方法有问题的信号（见上文「分类方法」一节）；v2 修正后中位数明显上移（core 从 6.1% 升到 11.1%，均值升到 15.9%，四分位区间 [3.8%, 22.0%]——区间很宽，说明不同项目的杂活占比差异很大，中位数不代表全部）。中位数仍然远低于 50%，但分布有长尾：`research/data/2026-09-27-repetition/84项目-scope核心-v2块级.csv` 里排前几名的项目确实过半（B6-06 `diluteoxygen/JevMood` 64.4%、Q-04 `siroccomask/snake-jev` 58.6%、B1-10 `prantikmedhi/anchorlint` 56.6%、B5-10 `lalitsonawane/jev-snake` 54.5%、B5-06b `eachann1024/pi-jev-route` 50.6%），本文例 7 用的 B3-01（`jev-commit`）是 29.3%，不是这批项目里最重的一个，只是杂活写得最完整、最容易讲清楚的一个。多数项目（77 个里 35 个低于 10%、55 个低于 20%）的判断核心确实以业务判定文字为主，但有 5 个项目过半——中位数和「多数项目怎样」这两句话都成立，「所有项目都远低于一半」不成立，三句话不能互相替代。

**这个结果和 Nature 要论证的「占了大部分开发量」不是同一件事，值得把两者的关系说清楚。** 行占比回答的问题是「杂活在判断核心的文本里占多少行」；开发量是另一个维度，包括想清楚阈值该设多少、这个常数会不会跟另一个常数产生覆盖关系、每个项目重新踩一遍同样的坑要花多少时间去调试。`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/） 引用 Jones 的限定：「编码只占大型项目总工作量的约 30%」。一行「阈值比较」代码背后的调试和踩坑时间，本节的行数统计量不到。这份数据能诚实支持的结论是：**杂活不占判断核心的多数代码行，但几乎每个项目都要重复写**（见下面 P2）——「重复」这件事本身站得住，「占多数开发量」这句话目前没有可信的量化证据，本文不替它背书。

**P2（≥ 8 类各在 ≥30% 项目出现）：不成立，7/12。** 第一版数字（8/12）里「校准与标注」类算进了 42.0%，那个数字后来在自查里查出是假阳性造成的（`from __future__ import annotations` 这句极常见的 Python 写法命中了 `annotation` 这个词，跟标注数据毫无关系）；修掉这处假阳性后，「校准与标注」的真实出现率是 9.9%，命中类别数从 8 掉到 7，够不上任务定的 8 这条线。

v2 合并口径（一个类只要在 core/file/host 任一范围出现过就算这个项目命中，已修正「校准与标注」类的假阳性）：

| 类别 | 出现于（81 项目为分母） | 占比 |
|---|---|---|
| 4 重试与失败 | 64 | **79.0%** |
| 1 调用循环与合批 | 61 | **75.3%** |
| 2 门槛 | 53 | **65.4%** |
| 9 材料裁剪 | 44 | **54.3%** |
| 8 题面拼装 | 41 | **50.6%** |
| 5 缓存 | 27 | **33.3%** |
| 3 拿不准的处理 | 30 | **37.0%** |
| 10 结果回流 | 18 | 22.2% |
| 12 校准与标注 | 8 | 9.9% |
| 6 费用与调用计数 | 10 | 12.3% |
| 7 并发限制 | 9 | 11.1% |
| 11 多判断合成 | 5 | 6.2% |

最普遍的三类：**重试与失败（79.0%）、调用循环与合批（75.3%）、门槛（65.4%）**。几乎每个项目在接入 JEV 时都要自己写一遍「调用失败怎么办」「一批题怎么发」「概率和常数比多大算过线」。7 类过线（4、1、2、9、8、5、3），5 类不过线（10、12、6、7、11）。最少见的两类是「多个判断的合成」（6.2%）与「并发限制」（11.1%）——这批项目本身大多数只做一轮判断（分类、审核、路由），多题投票/级联和显式并发控制在真实项目里确实少见。

逐项数据：v1 在 `84项目-scope核心.csv`、`84项目-scope整文件.csv`、`84项目-scope宿主.csv`；v2 在同目录下 `-v2块级` 后缀的三份文件。复现命令：`python3 research/scripts/busywork_classify.py`（`--ids` 只跑子集时会覆盖全量 CSV，复现全量结果前不要带这个参数）。

### 3.2 生态抽样 100 个仓库（P3）

从 `内部生态快照（未随本次公开）/` 的 4041 个仓库快照里，先用 ripgrep 找出源码里真的出现 JEV/TypeSafe 调用证据的仓库，排除官方 SDK 组织、名字里带 SDK 字样的第三方客户端库、文件数少于阈值的教程/模板仓库、以及与 84 个精选项目重名的仓库，剩下候选池 1804 个；`random.seed(42)` 固定种子抽 100 个。只对每个仓库里真正含调用点的文件（不读全仓库）跑 12 类正则，只记「出现与否」，用的是 v1（朴素逐行关键词，未做块级/上下文过滤）的同一套正则，脚本 `research/scripts/busywork_ecosystem_sample.py`，数据 `research/data/2026-09-27-repetition/生态抽样100.csv`。

因为这里用的是 v1 正则、读的是整份文件，可比的应该是 84 个项目 v1 scope=file 的数字（同一种方法、同一种范围），不是 v2 或三范围并集——下表把两者并排放。精选样本以 `research/data/2026-09-27-repetition/84项目-scope整文件.csv` 全部 81 条记录为分母，包括 3.1 节所述的 4 条零行记录；生态样本以全部 100 条记录为分母。各类出现率为对应 `present_*` 列之和除以记录数；差值为生态减精选，单位是百分点，在四舍五入到一位小数前计算。这里保持公开分母规则；零行记录不代表该类别在原始源码里确实不存在。

**CSV 复核（2026-09-30）：** 从公开 CSV 重新计算了全部 12 行，修正其中 4 个精选出现率及其差值。本次未重跑源码分类，原始语料未随仓库公开。复核命令：`python3 -m pytest -q tests/test_research_repetition.py`，无需原始语料即可检查中英文两张表。

| 类别 | 精选样本 v1 scope=file（n=81） | 生态抽样（n=100） | 差（百分点） |
|---|---|---|---|
| 4 重试与失败 | 79.0% | **85.0%** | +6.0 |
| 1 调用循环与合批 | 71.6% | **72.0%** | +0.4 |
| 2 门槛 | 65.4% | 50.0% | −15.4 |
| 8 题面拼装 | 48.1% | 58.0% | +9.9 |
| 9 材料裁剪 | 45.7% | **60.0%** | +14.3 |
| 3 拿不准的处理 | 35.8% | 21.0% | −14.8 |
| 5 缓存 | 25.9% | 29.0% | +3.1 |
| 6 费用与调用计数 | 16.0% | 28.0% | +12.0 |
| 7 并发限制 | 11.1% | 25.0% | +13.9 |
| 12 校准与标注 | 8.6%（已修正假阳性，见 3.3） | 37.0%（未修正，见下） | 不可比 |
| 10 结果回流 | 21.0% | 6.0% | −15.0 |
| 11 多判断合成 | 4.9% | 14.0% | +9.1 |

**「校准与标注」这一格不能直接比。** 生态抽样脚本照抄了分类脚本第一版的正则，同样把 `from __future__ import annotations`（跟标注数据无关的 Python 语言写法）当成「校准与标注」的命中——这个假阳性在 84 项目那边已经查出并修正（42.0% 改成 8.6%，见 3.3），生态抽样脚本没有跟着重跑，37.0% 这个数字大概率同样被这个假阳性大幅抬高，本文不采信它，也不据此判断生态样本里「校准与标注」到底比 84 项目常见还是少见。这是本文材料的一处已知缺口，不是假装没有这个问题。

**P3（排序大体一致）：部分成立，且要去掉「校准与标注」这一格单独看。** 排第一、第二的类别完全一致（重试与失败、调用循环与合批）。排序从第三名往下开始明显不同：「材料裁剪」在生态抽样里排第三（60.0%），「门槛」在精选样本排第三（65.4%），在生态抽样里排第五（50.0%）。排除校准与标注后，绝对差距最大的类别是门槛（−15.4 个百分点）、结果回流（−15.0）、拿不准的处理（−14.8）、材料裁剪（+14.3）和并发限制（+13.9）；费用与调用计数也高出 12.0 个百分点。结果回流在精选样本的 12 类中排第八（17/81，21.0%），在生态样本排末位（6/100，6.0%），并非两边都属于最少见的两类。多判断合成在精选样本排末位、生态样本排倒数第二，生态出现率为 14.0%，精选为 4/81（4.9%），相差 +9.1 个百分点，约为精选的 2.8 倍（倍率按舍入前的比例计算）。

这个差异有一个原因，抽样脚本自己的局限说明里写了：判相关性用的是关键词证据，抓到的「含调用点的文件」里混进了一部分更像 SDK 适配层、跟业务判断逻辑关系不大的代码（比如把 JEV 封装成众多 provider 之一的通用框架适配层），这类文件天然「基础设施味」更重，会抬高重试、缓存、费用计数、并发限制这几个和「怎么接一个外部服务」相关的类别；另外生态抽样统计里 `matched_files` 一栏能看到部分仓库（比如 `Alberto-Codes/judgevet`）把 `tests/` 目录下的测试文件也算进了「含调用点的文件」，测试代码里 `try/except`、`assert` 密度天然更高，这会进一步抬高「重试与失败」这一类在生态抽样里的比例，是另一处已知但没有单独量化的偏差来源。84 个项目是人工深读、挑出「判断核心」之后的样本，天然会把适配层和测试代码筛掉一部分；生态抽样是关键词粗筛，两边不是同一层次的「相关代码」。**结论站得住的部分**：最普遍的两类杂活（重试、调用循环）不是挑样本挑出来的，在无偏样本里同样最常见；**站不住的部分**：具体排序、具体百分比不能跨样本直接套用，尤其中间几类的名次和量级会随抽样口径明显摆动，「校准与标注」的生态样本数字仍含已知假阳性。

### 3.3 校准：脚本准不准

原计划派一个独立子代理，不看脚本、不看脚本输出，对 10 个项目做盲人工标注，再和脚本比对。这个子代理确实跑完了（协调方确认过），但它的最终结果没有送达——重新去信索要后仍未在时限内收到完整内容。按规矩（子代理超 20 分钟没交回按失败处理，自己做），这一节改为我自己对本文「四、例子」一节已经逐行读过源码的 10 个项目做事后核对：不是完全独立的盲标注（我知道脚本的分类逻辑），但每一条都是照着真实源码逐行核对的，不是凭脚本输出转述。

核对方式：把脚本 v2 的逐行详情（`--dump-detail-for`）跟我读源码时记下的判断，对同一个项目、同一个范围（scope=core）逐条对表，不满足于「脚本说有就信有」。核出四处问题，三处已经用代码修掉（改进后的规则已经用在上文所有数字里），一处是脚本能力上的天花板：

- **`for index, _item in enumerate(items):` 这类多变量解包的 for 循环，脚本原来的正则只认单变量 `for x in y:`，漏判「调用循环与合批」（`B1-07 pulso-nps`、`B2-04 pi-jev-compaction`）**。已修（`for\s+[\w\s,]+\s+in`），重新跑 `--dump-detail-for B1-07:core` 核实：`classify_batch` 函数（`app/classifier.py:66-112`，正好在这个项目 `[[core]]` 登记的 `66-112` 范围内）里的两处 for 循环现在都被正确标出为「调用循环与合批」，不是我第一版写的「仍然没有单独触发」——那句话是错的，已经删掉重写。
- **`abs(m.p_a - 0.5) < gate_threshold` 这个「离 0.5 太近就算拿不准」的结构性写法，脚本原来「拿不准的处理」类的正则只认 `unsure`/`uncertain`/`needs_review` 这类字面词，完全没认出来（`B3-10 jev-bracket`，本文例 2 写法一）**。已修（加了 `abs\(...-0.5\)` 模式），重跑后 B3-10 的 core 范围已经正确标出「拿不准的处理」；这一条修复也是「三、1」里「拿不准的处理」类占比从 30.9% 升到 37.0% 的主要原因。
- **`from __future__ import annotations`（Python 极常见的语言写法，跟标注数据毫无关系）命中了「校准与标注」类的 `annotation` 关键词，造成大范围假阳性（`B4-01 RoboJEV` 首先发现，逐项核对后确认几乎每个用这行代码的 Python 项目都受影响）**。已修（排除 `from __future__ import annotations` 这个具体写法），重跑后「校准与标注」出现率从 42.0% 掉到 9.9%，这是本次核对里影响最大的一处修正，直接改变了 P2 的结论（见 3.1）。
- **项目 B4-13（`yodablocks/commitjev`）的 SQLite 缓存代码（本文例 4）不在这个项目的 `measure.toml` 登记范围里，脚本因此看不到、也算不上「漏检」——这是范围边界问题，不是正则问题，没有修，也不该修**：`measure.toml` 只把 `rules.py` 登记为核心、`rules.py` 另一段和 `gitio.py` 登记为宿主，例 4 引用的 `client.py`（`Cache` 类、`Usage` 花费统计）完全没有出现在 `[[core]]` 或 `[[host_shared]]` 任何一条里。这说明本文「三、1」的统计范围本身有边界——一些真实存在、确实是杂活的代码，因为不在改写者当初登记的核心/宿主范围内，从未进入过本文任何一个数字，出现率因此仍然可能整体偏低，不止是脚本准不准的问题。
- **没有修、也修不了的一处：`B1-09 antispam` 的 `parseAssessment` 用 `entries.reduce((strongest, current) => current[1] > strongest[1] ? current : strongest)` 挑出概率最高的一条作为 `strongestSignal`，这是真实的「多个判断的合成」（取最大值），但这种由 `reduce` 配合比较运算符表达、不带任何专门词汇的写法，关键词正则原理上认不出来**。这是这个方法本身的天花板：「11 多判断合成」6.2% 这个数字应该读作下限，不是精确值。

另核实一处看似不一致、实为脚本正确：`Q-03 jev-corrective-rag` 的 core 范围没有标出「重试与失败」，读代码发现这个项目真正的 `if self.offline: ... else: response = self.client.system_one(...)` 分支（`gates.py:91-104`）落在这个项目 `measure.toml` 登记的 `[[host_shared]]` 范围（`64-78,83-125`）里，不在 `[[core]]`（`79-80,129-238`）——脚本在 scope=host 确实标出了这一类，scope=core 没标是对的，不是漏检。

**结论**：脚本对有专门词汇支撑、且落在登记范围内的类别（门槛、缓存、材料裁剪、拿不准）判断基本可信；三类系统性问题——通用语言结构表达的合成运算、跟语言本身常见写法撞词、以及测量范围本身不含全部真实代码——分别属于「方法天花板」「已修」「范围边界」三种不同性质，不能混为一谈。读者用本文数字时，应把「出现率」理解为按当前测量范围与方法能给出的最佳估计，不是精确统计，且很可能是下限。

## 四、例子：同一件杂活，好几个项目各写了一遍，写法都不一样，都不完整

这一节的例子全部来自真实、未改过一行的开源代码（原始文件与行号见各例标注，对应仓库均为公开开源项目；内部改写语料库的目录结构未随本次公开）。挑的标准是「同一类杂活出现在不止一个项目里，每次的写法都不一样，每次都留了一个坑」——这正是 Nature 要的「有感受」的例子：不是抽象地说「你要写调用循环」，是给读者看三四份长得不一样、但干的是同一件事的代码。

### 例 1：门槛常数散落在调用方，多写一个常数就多一处能互相覆盖的坑

仓库 `web_attack_detection_jev`（项目 B1-03）`config.py`：

```python
ATTACK_THRESHOLD = 0.65
REVIEW_THRESHOLD = 0.40
BLOCK_THRESHOLD = 0.70
```

仓库 `web_attack_detection_jev`（项目 B1-03）`detector.py:44-51`：

```python
def decide_action(is_attack_prob, attack_type, should_block_prob):
    if is_attack_prob >= ATTACK_THRESHOLD and attack_type != "benign":
        if should_block_prob >= BLOCK_THRESHOLD:
            return "block"
        return "block" if should_block_prob >= REVIEW_THRESHOLD else "review"
    if is_attack_prob >= ATTACK_THRESHOLD and attack_type == "benign":
        return "review"
```

在干什么：判断结果出来之后，作者自己写了三个阈值常量、一套嵌套 if/else 决定放行、复核还是拦截。缺了什么：`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）「发现的缺陷」一节已经指出，`REVIEW_THRESHOLD`（0.40）覆盖的范围已经完全盖住了 `BLOCK_THRESHOLD`（0.70）本该覆盖的范围——`should_block_prob >= REVIEW_THRESHOLD` 这一行执行到的时候，`BLOCK_THRESHOLD` 早就已经判过了，这个更严格的常量从未真正单独生效过。三个常量、一段嵌套 if，读者读一遍都不一定看得出这处重叠，作者自己也没看出来。

### 例 2：「拿不准」怎么判，三个项目写了三种不一样的结构，各有各的坑

同一件事——判断器给出的概率不够肯定时该怎么办——三个项目各自发明了一套结构，互相学不到经验。

**写法一，离 0.5 太近就算拿不准**（`jev_bracket/bracket.py:77-93`，仓库 `meetr1912/jev-bracket`）：

```python
def choose_winners(matchups, league, gate_threshold=0.0):
    gated = 0
    for m in matchups:
        a, b = league.by_tid(m.a_tid), league.by_tid(m.b_tid)
        better = a if a.seed < b.seed else b
        if abs(m.p_a - 0.5) < gate_threshold:
            m.gated = True
            m.winner_tid = better.tid
            m.method = "gated-chalk"
            gated += 1
        else:
            m.winner_tid = a.tid if m.p_a >= 0.5 else b.tid
```

在干什么：概率离 0.5 太近（差值小于一个门槛）就算「拿不准」，改走「按种子小的一方晋级」这条兜底路径。缺了什么：两个十进制小数在双精度浮点数下相减不是精确运算——`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）「发现的缺陷」一节记录了这个仓库真实存在的一处后果：0.45 与 0.5 相减取绝对值，算出来的结果比预期的边界值略小，让本该落在边界上的概率被浮点误差意外纳入或排除出「拿不准」区间，不是作者设计的闭区间，是运算精度的副作用。

**写法二，两道独立门槛（概率 + 置信度），谁在什么情况下失效想清楚才写对**（`triage.js:79-96`，仓库 `emreozyoruk/hush`，节选）：

```javascript
const lab = answers.label;
if (lab?.choice) {
  const p = lab.probabilities?.[lab.choice] ?? 0;
  const conf = typeof lab.confidence === "number" ? lab.confidence : 0;
  // Two gates, because they fail differently: a flat distribution means the
  // options overlap, low confidence means the model does not trust its own read.
  const sure = p >= thresholds.label && conf >= thresholds.label_confidence;
  if (lab.choice === "none") {
    out.push({ kind: "label", action: ABSTAIN, confidence: p, why: `best fit was "none" (${pct(p)})` });
  } else if (!labels[lab.choice]) {
    out.push({ kind: "label", action: ABSTAIN, confidence: p, why: `model returned an unknown option "${lab.choice}"` });
  } else if (sure) {
    out.push({ kind: "label", action: "label", value: lab.choice, confidence: p,
      why: `${lab.choice} ${pct(p)} ≥ ${pct(thresholds.label)}, confidence ${pct(conf)} ≥ ${pct(thresholds.label_confidence)}` });
  } else {
    out.push({ kind: "label", action: ABSTAIN, confidence: p,
      why: `${lab.choice} ${pct(p)} / confidence ${pct(conf)} — below ${pct(thresholds.label)} / ${pct(thresholds.label_confidence)}` });
  }
```

在干什么：源代码自己的注释写得很清楚——「两道门槛，因为它们失败的方式不一样：分布太平说明候选项之间分不开，置信度低说明模型自己都不信这次读数」，`p`（概率）和 `conf`（置信度）是两个不同的量，分开设两条线，都过线才算「肯定」，否则一律弃权（`ABSTAIN`），另外还单独处理了「选中了一个不存在的候选项」这种模型答跑题的情况。这是本文九个例子里对 `confidence`/`probability` 区分得最清楚的一份代码，没有例 3、例 8 那种混淆。缺了什么：两道门槛该怎么组合（`&&` 还是 `\|\|`）、三个特殊分支（`none`、跑题、双重门槛不过）的优先级怎么排，这些设计决定完全靠这一个项目的作者从头想一遍——换一个项目，候选项结构不一样、要不要单独处理跑题、两道门槛该用什么布尔组合，都得重新设计，没有现成的结构可以照搬。

**写法三，置信度门槛可以覆盖判断结果本身**（`jevtriage/gate.py:23-31`，仓库 `sathariels/jevtriage`；代码逐字照录，行内没有任何注释是原作者标出「不管 verdict 说了什么」这一层意思，理解靠读者自己顺着 if 顺序推）：

```python
def gate_exit(verdict: str, confidence: float, *, min_confidence: float) -> int:
    if verdict == "risky":
        return EXIT_RISKY_OR_ERROR
    if confidence < min_confidence:
        return EXIT_NEEDS_REVIEW
    if verdict == "ready":
        return EXIT_READY
    if verdict == "needs_review":
        return EXIT_NEEDS_REVIEW
    return EXIT_RISKY_OR_ERROR
```

在干什么：这是一次三选一判断（ready/needs_review/risky），但作者又加了一条独立的置信度检查，可以在判断结果已经是「ready」的情况下，把它强行改判成「转人工」——用源码自己文档字符串里的话说，是「fail closed」（宁可多转人工，不可漏判）。缺了什么：置信度检查和三选一判断是两套独立写的逻辑，谁的优先级更高、什么时候该互相覆盖，只能靠作者在这一个项目里手写清楚，换一个项目要重新设计一遍这套优先级。

三种写法结构完全不同（对称区间、双重门槛、覆盖式检查），没有一种能直接搬到另一个项目里用，但要解决的是同一个问题：判断器不够肯定时程序该怎么办。

### 例 3：置信度门槛「看起来在把关」，其实因为 SDK 返回值的默认值设计，永远放行

`src/gates.py:112-114`：

```python
else:
    answers[key] = bool(ans.noul)
confidence[key] = float(getattr(ans, "confidence", 1.0))
```

`src/pipeline.py:181-183`：

```python
passes = grade.answers.get("answers_it") and grade.confidence.get(
    "answers_it", 0.0
) >= RELEVANCE_CONF_FLOOR
```

在干什么：是非题的官方 SDK 类型 `NoulAnswer.noul` 是一个 0 到 1 的浮点数，这里用 `bool(...)` 把它转成真假；置信度字段读的是 `getattr(ans, "confidence", 1.0)`，读不到就假设置信度是满分。缺了什么：`bool(p)` 只在 `p` 恰好等于 0.0 时才是假——真机上概率几乎不会恰好为 0，所以这道「是非题」实际上几乎总是真；`NoulAnswer` 这个类型上根本没有 `confidence` 这个字段，`getattr` 的默认值 1.0 因此恒定生效，`pipeline.py` 里那道「置信度门槛」形同虚设，从未真正拦下任何一次判断。这是本次改写发现的、原项目里真实存在的缺陷，不是 J++ 改写引入的。

### 例 4：每个项目都给自己写一套判断接口的缓存 + 花费统计

`client.py:76-99`：

```python
@dataclass
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    requests: int = 0
    cache_hits: int = 0

    @property
    def cost_usd(self) -> float:
        return self.input_tokens * USD_PER_INPUT_TOKEN


class Cache:
    def __init__(self, path):
        ...
        with self._conn() as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS answers ("
                "key TEXT PRIMARY KEY, model TEXT, payload TEXT, "
                "input_tokens INTEGER, created_at REAL)"
            )

    @staticmethod
    def key(model, state, questions):
        blob = json.dumps({"model": model, "state": state, "questions": questions},
                           sort_keys=True, ensure_ascii=False)
```

这份文件开头的注释自己写着：「Jev transport: the official SDK, plus a cache and a thread pool.」在干什么：作者在官方 SDK 之外，自己建了一张 SQLite 表存判断结果、自己拼 JSON 算缓存键、自己维护一个花费统计的 `dataclass`。缺了什么：这套缓存和计数逻辑跟这个项目要判断的「commit 该不该被拦」这件事完全无关，纯粹是「怎么接 JEV」这件事本身带来的开发量；项目 B1-10（`prantikmedhi/anchorlint`）的改写记录里也提到原项目「按内容 memo 缓存没有对应」——同一件事，两个项目，两套不同的手写实现，谁也没法复用对方的。

### 例 5：候选集变了就得手写两轮调用，用下标拼一个字典把两轮串起来

`app/classifier.py:66-100`（节选）：

```python
def classify_batch(items, taxonomy):
    ...
    first_questions = {}
    for index, _item in enumerate(items):
        first_questions[f"area_{index}"] = Choice(instructions=..., criteria=area_criteria)
        first_questions[f"tone_{index}"] = Choice(instructions=..., criteria=TONE_CRITERIA)
    with TypeSafeClient(model="jev-latest") as client:
        first = client.system_one(state=state, questions=first_questions)
        second_questions = {}
        for index, _item in enumerate(items):
            area_answer = _choice_payload(first.choices[f"area_{index}"])
            area = next((e for e in taxonomy if e["name"] == area_answer[0]), taxonomy[-1])
            criteria = {c["name"]: c.get("description") for c in area["categories"]}
            second_questions[f"category_{index}"] = Choice(instructions=..., criteria=criteria)
        second = client.system_one(state=state, questions=second_questions)
```

在干什么：先问一批工单属于哪个大类，再用第一轮的答案（`area`）决定第二轮该给哪些候选子类，两轮之间用字符串拼出来的键（`f"area_{index}"`、`f"category_{index}"`）手动对应。缺了什么：这是「结果回流」＋「候选集不同的选择题分开处理」两类杂活叠在一起的典型写法——第二轮的候选集完全由第一轮的结果决定，作者必须自己想清楚怎么用一个字典把两轮的答案对齐、不能用错下标。`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）「省得最少的例子」一节列的几个「J++ 反而写得更长」的项目（`jesusvillamarin/pulso-nps` 正是其中之一，64 行对 80 行），原因也在这——这类手写的两轮拼接结构，J++ 现在按候选各自成状态处理，还没有把这种「用上一轮结果决定下一轮候选集」的模式压成一条语句（缺口 B155/B156）。

### 例 6：材料裁剪，每个项目挑一个自己觉得够用的数字

`triage.js:9-21`：

```javascript
export function buildState({ title, body, author, isFirstTimeContributor, openTitles }) {
  const lines = [
    `New issue title: ${title}`,
    `New issue body:\n${(body || "(empty)").slice(0, 6000)}`,
    `Author: ${author}${isFirstTimeContributor ? " (first-time contributor to this repo)" : ""}`,
  ];
  if (openTitles?.length) {
    lines.push("", "Titles of other open issues in this repository:",
      ...openTitles.slice(0, 40).map((t, i) => `${i + 1}. ${t}`));
  }
  return lines.join("\n");
}
```

在干什么：issue 正文截到 6000 字符，候选标题列表截到 40 条，怕材料太长塞不进请求或者拖累判断质量。缺了什么：6000 和 40 这两个数字没有任何依据写在代码旁边，纯粹是作者拍的；换一个项目，`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/） 记录的 `MarissaFamularo/citation-verifier`（引文核验）、`amitvijapur/cortex` 等项目的 `gap_titles` 里也各自出现了独立的材料裁剪/分片逻辑（`diff 解析、分片、token 估算在宿主`），每个项目都得重新回答「材料太长了怎么办」这个问题，数字互不相同，也互相学不到经验。

### 例 7：判断接口掉线时怎么办，三个项目手写了三套不一样的应对

**写法一，全套重试基础设施**（`jev_commit/jev.py:118-148`，仓库 `valentynkit/jev-commit`）：

```python
for attempt in range(RETRIES + 1):
    left = stop - time.monotonic()
    if left <= 0:
        raise JevError("deadline reached: " + last)
    try:
        with urllib.request.urlopen(request, timeout=left) as response:
            body = json.loads(_read_by(response, stop).decode("utf-8", "replace"))
        break
    except urllib.error.HTTPError as err:
        text = err.read().decode("utf-8", "replace")[:300]
        if err.code == 413 or (err.code in (400, 422) and any(h in text.lower() for h in TOO_BIG_HINTS)):
            raise TooBig(last) from err
        if err.code != 429 and err.code < 500:
            raise JevError(last) from err
        wait = _retry_after(err.headers, delay)
    except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException) as err:
        last, wait = str(err), delay
    if attempt == RETRIES:
        raise JevError(last)
```

在干什么：一次判断调用背后，作者手写了重试次数上限、总体截止时间（deadline）、按 HTTP 状态码分类要不要重试、`retry-after` 头解析、「payload 太大」单独归类成 `TooBig` 异常。缺了什么：这套逻辑分支很多，改写时给这个项目登记的缺口清单（`research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv` 该行 `gap_titles` 字段）就记着「TooBig 二分重试路径未触发」——构造的测试输入没能覆盖到这条分支，说明这类代码不仅写起来费功夫，连测全也费功夫。

**写法二，按错误类型分别设退避倍率**（`jev_bracket/jev.py:77-93`，仓库 `meetr1912/jev-bracket`；代码逐字照录，不含任何行内注释）：

```python
for attempt in range(attempts):
    try:
        response = CLIENT.post(ENDPOINT, json=body, headers={"Authorization": f"Bearer {key}"})
    except httpx.HTTPError as error:
        last_error = error
        time.sleep(0.4 * 2**attempt)
        continue
    if response.status_code in {429, 529, 503} and attempt < attempts - 1:
        time.sleep(0.5 * 2**attempt)
        continue
    if response.is_error:
        raise JevError(f"TypeSafe returned HTTP {response.status_code}")
    try:
        payload = response.json()
        _validate(payload["answers"], questions)
    except (KeyError, TypeError, ValueError, JevError) as error:
        last_error = error
        time.sleep(0.3 * 2**attempt)
        continue
```

在干什么：跟例 7 写法一的思路相似（指数退避），但这个项目自己又发明了一套不同的具体数字——网络错误起步等 0.4 秒、限流/过载起步等 0.5 秒、响应解析失败起步等 0.3 秒，指数底数都是 2，起步秒数这三个数字没有写明依据，换一个项目大概率是另外三个数字。

**写法三，反复被判「非法答案」时不重试，直接给一个安全默认动作**（`system_one_poker/jev.py:198-204`，仓库 `dperezcabrera/system-one-poker`）：

```python
try:
    answer = await self._llm.choose(gateway, self._upstream, state, INSTRUCTIONS, criteria, attempts)
except IllegalAnswers as e:
    by_kind = {option.kind: option for option in options}
    safe = by_kind.get("fold") or by_kind.get("check") or options[0]
    return Decision(safe.key, {}, e.input_tokens, e.output_tokens, e.cost_usd, e.seconds, forfeit=True)
```

在干什么：这一局扑克里，模型的答案不是候选动作之一就算「非法」，重试次数用完后不再重试，直接选一个「安全」动作（弃牌，没有弃牌选项就过牌，都没有就选第一个选项），标记为「弃权」。这跟前两种写法完全不是同一个结构——前两种是「换个时间点再问一次」，这一种是「不再问了，用一个写死的兜底动作」。

三种写法分别代表三种不同的应对哲学（全套重试基建、分类型退避、超限后弃权走安全默认值），`research/data/2026-09-27-repetition/` 的统计显示 79.0%（v2 口径，见 3.1 节）的项目都写了某种版本的重试/失败处理，但没有统一的写法，每个项目各自设计一遍。

### 例 8：置信度字段，24 个项目都读错了同一个东西

`research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）「调用次数」一节记录：「84 个项目里有 24 个在原项目里直接读取判断器返回的 `confidence`（置信度）字段设阈值……已有的抽样核实显示，选择题里约 7% 的读数、打分题里几乎全部读数会让这个字段和『最大概率』产生分歧」。在干什么：24 个不同的项目，各自认定 SDK 返回的 `confidence` 字段就是「这个答案有多可信」，拿它去跟自己的门槛比。缺了什么：`confidence` 不等于读数分布里最大的那个概率，两者在真机上经常不一致；例 3 里 Q-03 的 `NoulAnswer` 甚至根本没有这个字段，`getattr` 默认值 1.0 让门槛形同虚设。24 个项目、24 份不同的代码，重复的是同一个误解，不是同一段代码——这恰好说明「重复」不只是复制粘贴那种重复，是同一类理解上的坑被独立踩了 24 次。

### 例 9：题面拼装，一份题面的措辞要跟着上下文变，作者手写模板去处理这种变化

`policy.py:64-76`（仓库 `lykycy123/RoboJEV`，机械臂抓取任务）：

```python
def axis_criteria(axis):
    """Describe task-conditioned alternatives; the model selects the branch and direction."""
    if axis in "xy":
        return {
            "zero": "No horizontal motion if cube is released and resting inside target. "
                    "When holding: zero if target XY already aligned OR cube_clear_of_table_for_transport is false. "
                    f"Otherwise zero if target_from_cube.directions.{axis} is zero. "
                    f"When not holding: zero if grasp_tcp_from_tcp.directions.{axis} is zero.",
            **{v: f"Move {axis.upper()} {v} when NOT holding and grasp_tcp_from_tcp.directions.{axis} is {v}. "
                  f"When holding, move {v} ONLY if cube_clear_of_table_for_transport is true AND "
                  f"target_from_cube.directions.{axis} is {v}. Never transport before lifting clear. "
               for v in ("negative", "positive")},
        }
```

在干什么：机械臂在 X、Y、Z 三个轴上各要问一次「该往哪个方向动」，每个轴的候选描述文字要点几乎一样（只是把轴名代进去），作者用一个函数、加一层 f-string 拼接、外加字典推导式去生成这三份几乎相同又略有差异的候选描述，避免把同一段英文抄三遍。缺了什么：这不是特别复杂的判断——三个轴的规则本质上是同一条规则代入不同变量——但作者仍然要专门写一个函数、想清楚哪部分该模板化、哪部分因轴而异，这层「怎么把变化的部分和不变的部分拆开」的工作跟这个项目要不要抓稳一个方块没有关系，纯粹是「怎么把材料和候选拼成一段能发给判断器的文字」这件事本身带来的开发量。跟例 5（`pulso-nps` 用字符串插值拼装两轮问题）和例 6（`hush` 用模板字面量拼装 issue 状态描述）放在一起看：三个项目、三种任务（机械臂、客服分诊、issue 分诊），题面拼装这件事各写各的，谁也不认识谁的写法。

## 五、对上我们的机制

每类杂活对应 J++ 里由语言（编译期规则）或运行时接管的机制。「已测效果」只写仓库里已经有真实记录的数字，没有数字的写「尚无数字」，不编造。

| 杂活类别 | J++ 机制（名字、在哪） | 接管了什么，作者还要写什么 | 已测效果 |
|---|---|---|---|
| 1 调用循环与合批 | 运行时按材料/题式自动合批（步 15i，内部记录 B0303，内部记录）；`00-目标与动机-v1.md` §三第一条「批调度」 | 作者只写「对这批材料各问一道题」，不写循环、不写 `Promise.all`、不写手动攒批的队列；运行时决定哪些题挤进同一次调用 | B0303（步 15i 刚合入时的记录）：58 项值全同，J++ 合计调用 1022 → 883，与原项目手写的 883 次打平；`B1-06` 20 → 10（1:1）；`B1-07` 当时记为 7 → 6（第二跳要改程序结构才到 2 次）。后续版本的定案数据（`research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv`）里 `B1-07` 变成 2 → 2，两个数字都如实引用——差异来自两次记录之间又合入了别的施工步，不是互相矛盾，是同一个项目在不同时间点测的结果，后者更新。反例见下文「反例」一节：跨候选比较的 4 个项目（`Q-04`、`B5-09`、`B5-12`、`B6-07`）J++ 把每个候选拆成单独判断，18 → 174 次，缺口 B155/B156 尚未补上，这一类项目运行成本明显高于原项目 |
| 2 门槛 | `cut(r, {declare: {hi, lo}})` 作者声明线；校准记录认证线（B128，内部记录） | 一条语句替代一段散落在调用方的常量比较和嵌套 if/else（例 1 那种「常量互相覆盖」的坑，因为线只写一处，不会有第二个常量去覆盖它） | 尚无跨项目的门槛正确率对照数字；内部记录 测过试用线的下限（字面 60–80 条、语义 80–100 条标注），不是门槛机制本身的效果数字 |
| 3 拿不准的处理 | `unsure` 三值出口 + J-05「未决必须消费」（`00-目标与动机-v1.md` §四；每个 `unsure` 必须有细化、升级、显式丢弃并记账、或转交调用者四条去向之一） | 例 2 那种「概率与 0.5 的差小于一个门槛就走兜底」的手写判断不用每个项目自己写，运行时按读数的多数块或作者的线统一判定；不允许像原项目那样把「拿不准」悄悄扔进一个失败列表不再处理 | `research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）「(c) 缺陷与正确性」一节记录：「原项目遇到不确定的情况，常见做法是直接塞进一个失败列表里不再处理（本次改写里至少两个项目就是这么写的），J++ 不允许这种隐式丢弃」；例 2 的浮点边界问题在 J++ 里由 `closed`（开闭区间显式声明，B153/20j-3）与并档容差统一处理，`B7-01` 改写记录了这一处容差差异（并档容差缺省为 0，8 张工单里 1 处取整边界不同，预注册已预测） |
| 4 重试与失败 | **判断力缺席（B32/K-039，已造出）**：判断器不可用（无凭据/画像/校准记录/模型不兼容/连续失败）时出口为 `Unsure(absent)`，按 J-05 四条去向路由；`budget` 块自带 `absent: {retry: n, backoff, then: escalate\|conservative\|fail}`，默认 `escalate`；连续失败 k 次熔断停发；账本记缺席事件（`research/地基/12-IR与类契约-v0.1.md` 194 行；`research/地基/18-设计总账-v1.md` K-039：已造出，`rust/crates/jpp/tests/b32_absent_latency.rs`） | 不用每个项目自己写例 7 那三种「重试次数、退避倍率、超限后走安全默认值」——`retry`/`backoff`/`then` 是画像和 `budget` 块里的字段，不是作者每次手写的代码 | K-039 有单元测试覆盖（`rust/crates/jpp/tests/b32_absent_latency.rs`），机制本身已造出，不是只停在设计文档；但没有一次专门用这批开源项目的「调用失败/超时」场景做真机对照（改一遍写法、比一遍失败处理的行为），所以「装了这个机制后，例 7 那三种写法能不能统一成一条语句」这个具体问题尚无数字 |
| 5 缓存 | 账本（ledger）+ `--replay` 按键重放（`00-目标与动机-v1.md` §三「缓存与增量」） | 不用每个项目自己建 SQLite 表、自己拼 JSON 算缓存键（例 4）；同一份输入重放时不必再花一次真实调用 | 案例 05（通爻网络）重放抽查：内部记录记录「从主运行 2309 个任务里每隔 150 个抽 1 个，共 15 个……按账本 `--replay` 重放：返回值与当时的报告逐字段相同，新调用 0 次，15/15 通过」——只抽查了 15 个，不是全部 2309 个都重放过；搜索骨架记录「真机一次约 0.0001 美元，重放零调用」（内部记录） |
| 6 费用与调用计数 | 账本自动记账，不需要作者写计数器/`Usage` dataclass（例 4） | 花了多少钱、调用几次，账本天然有，不用像 `commitjev` 那样自己维护 `cost_usd` 属性 | 案例 05：「花费约为生成模型一次读完的 1/74」（口径：`claude -p` 订阅按 list 价折算，非实付，报告写明）；这些数字都是账本自动出的 |
| 7 并发限制 | **调度 pass（K-200/H-011，已定未造）**：设计里 `judge` 按层批发、并发按状态大小自适应读画像的 `profile.concurrency` 字段（`research/地基/12-IR与类契约-v0.1.md` 743 行）；现状是 `rust/crates/jpp/src/interp.rs` 里 `Passes.schedule` 字段已经存在但恒为关闭（`research/地基/18-设计总账-v1.md` K-200：没做）。另有预算停机改停发（步 22-0，B93）是相邻但不同的机制——那条管的是「预算耗不起时不发」，不是「并发限几个」 | 设计上不用作者自己写信号量/限速器；**现状是这条还没造出来**，今天程序不会自动限并发 | 无——这条标「已定未造」，不编一个「已测效果」出来。步 22-0 的 846 个测试通过是预算停机那个相邻机制的验证，不能当成并发限速的数字 |
| 8 题面拼装 | `judge`/`choice`/`test` 等效应内嵌的题组装规则（`00-定位与方法论-v1.md` 术语表「题式」「效应」） | 一条判断语句替代例 9 那种手写的 f-string/模板拼接（RoboJEV 三个轴各写一遍的重复模板，例 5 `pulso-nps`、例 6 `hush` 是另外两种独立写法） | `research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）：83 个可比项目「只算判断核心」的折行倍率中位数 2.5 倍——这个数字里相当一部分来自题面拼装和阈值判断被一条语句取代，但报告没有单独拆出「题面拼装」这一项的贡献，不能精确到这一类杂活本身省了多少行 |
| 9 材料裁剪 | **裂变 pass（K-199/K-220/L-035，已定未造）**：设计里超窗材料（`noul` 500 字、`choice` 段落级 1.8k、代码级 1.5k）按窗切开、对象单独占位，合回按语义操作分派（`research/地基/18-设计总账-v1.md` K-199、L-035）；现状是 `rust/crates/jpp/src/interp.rs（第 245-295 行附近）` 里 `Passes.fission` 字段存在但恒为关闭——**这条今天没有造出来，例 6 的 `hush` 项目仍然要自己写 `.slice(0, 6000)`** | 设计上作者不用自己算截断长度；**现状是这条还没造出来**，材料裁剪今天在 J++ 里也要作者自己写 | 无。K-220 还额外要求裂变「须标为近似策略并做模型实验，不能作无条件等价重写」，这条实验也没做——本文发现的这处空白是设计已有、代码没有 |
| 10 结果回流 | 搭配层的 `search`/多跳判断骨架（`rust/lib/compose/search.jpp`，步 25c，B0302） | 不用像 `pulso-nps` 那样手写「用第一轮结果的下标拼第二轮候选集」的字典；`search` 把「提出 → 接地 → 可行性 → 目标」按轮次累积，未决候选按 carry/refine/函数去向，不丢弃 | 步 25c 记录：「真机一次约 0.0001 美元，重放零调用」；「搭配层三骨架（search、ground 待 25e、graph）已合其二」——search 骨架已合入并有真机数字，但没有专门针对「pulso-nps 这类两轮候选收窄」场景的对照改写数字 |
| 11 多个判断的合成 | `cut` 的 `stat` 统计量（max/expect/confidence，B153/B165/B167，步 20j-3）+ 作者声明合成规则（步 20j-4 第一段：作者写组合规则、再对合成分数声明判断线） | 不用像原项目那样手写加权求和/投票/级联的胶水代码；读数上的算术被拆成「先算统计量，再对统计量声明线」两步，都在语言里 | `research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/）：「设计已裁定（B153、B166、B167），对应施工步 15k（排序）与 20j-4 第一段……已合入，受阻项用新版本改写后大部分变成了全部等价」；仍有反例——`B6-11` 因为跨维度排序需要跨拟合比较，B153 现状不支持，检查器报 J-04，列为新缺口，不是全部解决了 |
| 12 校准与标注 | 校准阈值机制：作者声明线（declare）与认证线（校准记录）分离，`declare` 不需要标注数据集（B128） | 作者可以自己写判断线，不强制建标注数据集和调阈值脚本；要语言担保错误率时才需要认证线 | 内部记录：离线对照给出试用线的实际下限（字面 60–80 条、语义 80–100 条），正式线字面约 160 条——这条数字说明「要不要标注」本身有代价，J++ 把这个代价从「每个项目都得建一遍标注集」变成「只有明确要认证线的时候才需要」，不是把代价降到零 |

### 反例

- **材料裁剪（9）设计过、没造出来。** 裂变 pass（K-199/K-220/L-035）是「运行时接管的八件事」之外、`research/地基/12-IR与类契约-v0.1.md` 里单独写清楚的一条编译期机制，但 `rust/crates/jpp/src/interp.rs` 里对应的开关今天恒为关闭。例 6 里 `hush` 项目手写的 `.slice(0, 6000)`、`.slice(0, 40)` 这类裁剪，今天在 J++ 里仍然要作者自己决定。这一类杂活出现在 54.3% 的项目里（3.1 节），是本文样本里第四常见的一类，是本节里「设计已有、代码没有」这一类反例里最明确的一条。
- **并发限制（7）同样是设计过、没造出来**（调度 pass，K-200/H-011），跟材料裁剪是同一种性质的缺口，不是「完全没想过」。
- **重试与失败（4）的判断力缺席机制已经造出来、有单元测试**，但没有专门针对这批开源项目「调用失败时怎么办」场景的真机对照实验——已造出的机制和「对这批项目有实测效果」是两件事，不能因为前者成立就默认后者也成立。
- **多判断合成（11）虽然设计已裁定，仍有真实缺口**（`B6-11` 的跨维度排序），不是「机制一上线全部项目就等价」。
- **这四类杂活（4/7/9/11）程度各不相同，不能合并成一句话**：4 的机制造出来了但没测过对这批项目的效果；7 和 9 连机制都还没造；11 造了但留了已知缺口。「J++ 都处理了」和「J++ 都没处理」这两句概括都不准确，四类各自的状态需要分开看，见上表。




## 六、结论

**P1（行占比中位数 ≥ 50%）不成立。** 用更准的方法（v2 块级归属+上下文邻近）重新测，判断核心里杂活行数占比中位数是 11.1%，比朴素逐行关键词法测出的 6.1% 高，但离 50% 差得远。分布有长尾——`B6-06`（`diluteoxygen/JevMood`）等 5 个项目的核心过半是杂活，多数项目（77 个里 55 个低于 20%）不是。这条预测按写下的字面标准是错的，后面两条预测的结果不能替它挡过去。

**这条 11.1% 跟另一份已有数据明显对不上，两边都摆出来，不替哪一边说话。** `research/2026-09-26-rewrite-study.zh-CN.md`（84 个真实 JEV 项目改写逐字段比对的公开报告；可交互版见 https://jpp.towow.net/rewrite-study/） 用另一套完全不同的方法（数改写前后的总行数，不分类）测出：83 个可比项目「只算判断核心」的折行倍率中位数 2.5 倍——也就是说 J++ 版本平均只用了原判断核心 40% 的行数，另外约 60% 被压掉了；算上两边都要写的宿主胶水代码，倍率降到 1.66 倍，约 40% 被压掉。这两个数字（60%、40%）都远大于本文用 12 类正则测出的 11.1%。差距摆在这里，说明两件事至少有一件成立：要么本文的 12 类正则遗漏了大部分 J++ 真正替代掉的代码（前面 3.3 节已经确认存在漏检和范围边界问题），要么 J++ 压缩的不只是这 12 类杂活、还包括本文没有单独归类的其他重复模式。这份材料没有能力把这两种可能分开，是本文没有解决、必须摆出来的一处缺口——它比「11.1% 还是 60%」这个数字本身更重要，因为它直接关系到「杂活占了大部分开发量」这句话到底有没有独立证据支持。

**P2（≥ 8 类各在 ≥30% 项目出现）不成立，7/12。** 第一版数字（8/12）里「校准与标注」类算进了 42.0%；这个数字在自查里被查出是假阳性——`from __future__ import annotations`（Python 极常见的语言写法）命中了 `annotation` 这个词，跟标注数据毫无关系。修掉这个假阳性后，「校准与标注」的真实出现率是 9.9%，命中类别数从 8 掉到 7，够不上任务定的这条线。这里排名最高的两类——「重试与失败」（79.0%）和「调用循环与合批」（75.3%）——在生态抽样（P3）里也分别排第一、第二，这部分结论不依赖 P2 的整体计分结果。

**P3（生态抽样排序大体一致）部分成立。** 排第一、第二的类别（重试、调用循环）完全一致，但其余排序和出现率存在明显差异。生态样本的材料裁剪、并发限制、费用与调用计数高出 10 个百分点以上；门槛、结果回流、拿不准的处理则低了 10 个百分点以上。多判断合成高出 9.1 个百分点，约为精选的 2.8 倍；结果回流在精选样本的 12 类中排第八，在生态样本排末位。关键词粗筛可能混入更多 SDK 适配层和测试代码，是基础设施相关类别出现率偏高的可能原因，不能据此断定每项差异的成因（见 3.2）。生态样本的校准与标注出现率仍含已知假阳性，因此该行保持不可比。前两名一致，不足以支持将其余名次和百分比跨样本套用。

**最普遍的三类杂活：重试与失败、调用循环与合批、门槛。** 这三类在 84 项目和生态抽样里都排在前列，本文给出的九个例子里，例 2（拿不准的判定，横跨 3 个项目 3 种结构：对称区间、双重门槛、覆盖式检查）、例 7（重试与失败，横跨 3 个项目 3 种应对：全套重试基建、分类型退避、超限后弃权走安全默认值）、例 5（候选集变化后的两轮调用）、例 8（24 个项目共享的同一个 `confidence` 误解）都在正面回应「重复的地方是什么地方」这个问题。

**杂活占比到底多少：本文的窄口径和改写报告的宽口径给出两个不同量级的数字，取哪一个取决于问的是哪个问题。** 只数 12 类关键词能认出的杂活，判断核心里占一成出头；改写报告数「换成 J++ 之后省了多少行」，省下的部分接近六成。「重复」这个现象站得住——几乎每个项目都要为同一件事重新设计一遍；「占了大部分开发量」这句话，本文自己的 12 类分类法不支持，但改写报告的整体压缩比例支持它，只是那份数据测的是「省了多少行」，不是「杂活占多少行」，两者不是同一个问题的两个答案，本文没有把它们合并成一个数字，留给读者自己判断该信哪一个、或者两个都信一部分。工时和认知负荷这两个更直接的开发量指标，改写报告自己也承认没测过。

**J++ 的机制覆盖情况，按表逐类数，不笼统地说「大部分」或「没做」。** 12 类里 10 类有已造出的机制（1、2、3、4、5、6、8、10、11、12），其中「调用循环与合批」（B0303：1022→883）、「拿不准的处理」、「缓存」（案例 05 重放 15/15）、「费用与调用计数」（案例 05 花费 1/74）四类有真机或改写数字支持；「门槛」「多判断合成」「校准与标注」三类机制已造出但只有部分间接数字（试用线下限、B153 施工步合入记录）；「重试与失败」（判断力缺席 B32/K-039）机制已造出、有单元测试，但没有专门针对这批项目的真机对照；「题面拼装」有已造出的机制但效果数字混在整体行数倍率里，拆不出来。「并发限制」「材料裁剪」两类是设计过、没有造出来的明确空白，今天作者仍要自己写。

**反例，逐条列出。** 55 个项目的判断核心低于两成是杂活，不是所有项目杂活都多；材料裁剪和并发限制目前明确没造出来，不是所有杂活类别 J++ 都接管了；判断力缺席、门槛声明线都还缺专门针对这批项目的对照实验，不是所有已造出的机制都有真机数字；中间几类的排序和量级会随抽样口径明显摆动，「校准与标注」的生态样本数字仍含已知假阳性，不是生态抽样处处都印证精选项目的结论；11.1% 和约 60% 这两个数字差出五倍多，是这份材料自己没能力调和的一处真实缺口。以上反例决定了本文能给出的结论边界，不是本文论证失败的地方。
