# 表达力换算与 DSL 实测倍率

2026-09-24 调研整理。本文补充 `地基/评估/语言表达量对比-方法与参考倍率.md`（下称“评估文档”），供验收①「一句顶一百句」的 A-6 口径与通过线裁定参考。验收①的定义见 `地基/00-目标与动机-v1.md` §六第 1 条：同一件事，用这门语言写与直接调接口手写相比，比较行数与所需思考量。

本文覆盖三件事：COCOMO II 的语言换算表与计数口径，Berkholz 表达力排名的原文数字，以及 DSL 相对通用语言的实测倍率。第三件是评估文档覆盖最薄的部分，也最接近 J++ 的处境。

通用语言之间的倍率（C 对 C++、Rust 对 OCaml 等）见同目录的 `07-语言倍率参考系-出处核实.md`，本文不重复。

本文只记录查到的方法与数字，不裁定验收线。每个数字后附出处，并用方括号标明核实程度：

- **[全文]**：下载原文 PDF 或 HTML，逐句核对。
- **[网页]**：读取了原网页。
- **[摘要]**：只读到摘要。
- **[二手]**：转引自其他文献。

查不到的数字写“未查到”。标“计算得出”的比值是按原文数字算出来的，原文没有直接写。

---

## 一、要点

**DSL 相对通用语言的代码量倍率，主要由参考系决定。** 参考系指计量时把哪些代码算进去。只算 DSL 文本时，需求对等、基线实测的案例在 2.7× 到约 17× 之间，高低主要看领域：

- LLM 编排语言 LMQL 是 2.7–4.3×，这是与 J++ 最接近的一组数据。
- 数据结构生成器 P2 是 4×。
- 显卡驱动语言 GAL 至少 9×。
- 分布式调度语言 Overlog（BOOM-MR）约 16.6×。

**把生成器或胶水代码也算进来，倍率会大幅下降。** GAL 摊入生成器后是 3.5×（这一步是原作者外推），BOOM-MR 计入配套的 Java 代码后约 3.9×。两个案例都跌到 4× 以下。两种口径都没有把语言运行时本身算进去。

**开发时间倍率比代码量倍率低。** 报告了具体数字的受控实验只有一项（Sprat），完成时间缩短到原来的 1/1.45 至 1/2.3。另一项工业模型驱动开发实验（Panach 2015）没有测到显著差异。

**达到或超过 10× 的案例都附带一项限制。** 有的功能不对等（BOOM-FS），有的只有作者概括而没有逐程序数据（Sawzall），有的基线是估计值（Polar）。

**COCOMO II 的计数规则可以直接借来划参考系。** 写给代码生成器的输入计入源代码行，生成出来的代码、语言运行时和商业库都不计。

---

## 二、对评估文档的核对意见

以下五处需要评估文档作者核对。第 1 处影响方法选择的理由，其余四处是补充或归类。

**1. Berkholz 指标对 DSL 的方向写反了。** 评估文档 §1.5 写“榜单最低分（表达力被判定为最差）是 Augeas 和 Puppet”，§三第 3 点写该方法“对声明式专用语言系统性不利”。原文恰好相反。Berkholz 按每次提交改动行数的中位数从小到大排名，数字越小表示表达力越强：

- Augeas 以中位数 48 排第 1，Puppet 以 52 排第 2。
- 原文写道：“Domain-specific languages are biased toward high expressiveness”。
- 作者推荐通用语言时特意 “rule out Puppet and Augeas because they are DSLs”。

所以这个指标对 DSL 系统性**有利**，它会高估 J++ 这类语言。评估文档“不采用该方法”的结论不变，理由需要改成“会虚高”。出处见本文第四节。

**2. COCOMO II 手册的换算表已逐条读出。** 评估文档 §1.2 写“手册内表格逐条数值未查到”。本文第三节列出了 2000 版 Table 4 的全部数值，以及 1998 版与之不同的两项。

**3. COCOMO II 的计数口径可以用在验收①上。** 评估文档 §3.2 写“COCOMO II 换算……同样不适用”。就功能点换算表而言，这个判断成立。但手册里写给生成器的输入怎么计、生成代码怎么计的规则，正好回答了“J++ 源码计多少行、生成的调用代码计不计”，可以直接借用。细节见本文 3.3 节。

**4. RISLA 有量化结果。** 评估文档 §1.6 引 van Deursen 1997 的金融 DSL 案例，写“综述未给出量化倍率”。同一项目在 van Deursen & Klint 1998 中给出了交付周期数字：新产品上线从估计的 3 个月缩短到 2–3 周。见本文 5.2 节。

**5. Sprat 实验被放进了错误的层级。** 评估文档 §3.1 第 1 点把 Johanson & Hasselbring 2017 列为“同代通用语言之间”的证据。这个实验比较的是 DSL 与通用语言，它的 1.45–2.3× 属于“专用语言对通用语言”这一层。

---

## 三、COCOMO II 的语言换算

### 3.1 换算表

COCOMO II 有一张功能点到代码行的换算表，叫“UFP to SLOC Conversion Ratios”，也称 backfiring 表。UFP（未调整功能点）是一种与实现语言无关的需求规模单位。换算表给出每种语言实现一个 UFP 平均要写多少行逻辑源代码（SLOC）。COCOMO II 没有叫“Language Level”的换算系数，这个概念来自 Capers Jones 的 SPR 表，见 3.4 节。

COCOMO II Model Definition Manual v2.1（©1995–2000 USC CSE）§2.3 Table 4 [全文]：原文说这些比率 “are from [Jones 1996]”。默认 SLOC/UFP 如下。

| 语言 | SLOC/UFP | 语言 | SLOC/UFP |
|---|---|---|---|
| Machine Code | 640 | Lisp | 64 |
| Assembly - Basic | 320 | Prolog | 64 |
| Assembly - Macro | 213 | Forth | 64 |
| C | 128 | Java | 53 |
| Fortran 77 | 107 | C++ | 55 |
| Jovial | 107 | Ada 95 | 49 |
| Unix Shell Scripts | 107 | Visual C++ | 34 |
| Cobol (ANSI 85) | 91 | APL | 32 |
| Pascal | 91 | Visual Basic 5.0 | 29 |
| Basic - Compiled | 91 | PERL | 27 |
| Modula 2 | 80 | Access | 38 |
| Ada 83 | 71 | HTML 3.0 | 15 |
| Fortran 95 | 71 | PowerBuilder | 16 |

按语言代际或类别的默认值：

- 第一代语言 320，第二代语言 107，第三代语言 80，第四代语言 20，第五代语言 4。
- High Level Language 64，Database 40，Query 13，Simulation 46，Report Generator 80，Spreadsheet 6。

表里另有 USR_1 到 USR_5 五个用户自定义槽，默认值为 1。手册建议 “It would be prudent to determine your own ratios for your local environment”。

出处：https://www.rose-hulman.edu/class/cs/csse372/201310/Homework/CII_modelman2000.pdf

### 3.2 早期版本的差异

较早的 COCOMO II 手册（对应工具版本 USC COCOMO II.1998.0）Table II-4 [全文] 引用的是 Jones 1991，两个数字与 2000 版不同：

- C++：1998 版 29，2000 版 55。
- Fortran 77：1998 版 105，2000 版 107。

Ada 71、C 128、Lisp 64、Spreadsheet 6 等其余数值两版相同。差异原因未查到。

出处：https://athena.ecs.csus.edu/~buckley/CSc231_files/Cocomo_II_Manual.pdf

### 3.3 计数口径

2000 版 §2.1 与附录 Table 64 检查表 [全文] 规定了哪些代码计入 SLOC。

计量目标的原文是 “to measure the amount of intellectual work put into program development”，单位采用 SEI 检查表定义的逻辑源语句。

对代码生成器，原文规定：“Code generated with source code generators is handled by counting separate operator directives as lines of source code.” 也就是说，写给生成器的输入（相当于 DSL 源码）计入，生成器产出的代码不计。检查表的“How produced”一栏也是这样标的：

| 代码来源 | 是否计入 |
|---|---|
| Programmed（手写） | 计入 |
| Generated with source code generators（生成器产出） | 不计 |
| Converted with automated translators（自动翻译转换） | 计入 |
| Copied or reused without change（原样复用） | 计入 |

手册还写明，COTS、语言支持库、操作系统和其他商业库都不计入。

放到 J++ 上，这套规则的含义是：J++ 源码计行；J++ 编译或生成出来的调接口代码不计；手写基线调用的 SDK 和标准库也不计。

### 3.4 语言级别表（Jones / SPR）

“Language Level”（语言级别）来自 Capers Jones 的 SPR Programming Languages Table。COCOMO II 2000 手册只提过一次，放在推迟事项里：“effects of language level (reduced Construction effort for very high level languages)” 被列为 “deferred for later versions” [全文]。

SPR 表 release 8.2 [全文，取自葡语镜像，页脚注明 “Extraída de Jones (1996)”] 有 LEVEL 与 SS/FP（每功能点平均源语句数）两列。评估文档 §1.1 已从 Wayback 存档取得同一张表，数值一致。评估文档没有列、而与本调研相关的几项：

| 条目 | LEVEL | SS/FP |
|---|---|---|
| Haskell | 8.50 | 38 |
| Prgram Generator default（原文拼写） | 20.00 | 16 |
| Spreadsheet default | 50.00 | 6 |
| EXCEL 5 | 57.00 | 6 |
| Reuse default | 60.00 | 5 |
| Natural language | 0.10 | 3200 |

表内数字基本满足 SS/FP ≈ 320 ÷ LEVEL（计算得出）。例如 C 是 320 ÷ 2.5 = 128，Java 是 320 ÷ 6 ≈ 53。Python 和 Ruby 不在表中。

出处：https://engenhariasoftware.wordpress.com/wp-content/uploads/2008/06/conversao.pdf

Mernik, Heering & Sloane 2005 年的综述 [全文] 转录了 Jones 1996 的“语言级别对生产力”表，单位是每人月完成的功能点数：

| 语言级别 | 每人月功能点 |
|---|---|
| 1–3 | 5–10 |
| 4–8 | 10–20 |
| 9–15 | 16–23 |
| 16–23 | 15–30 |
| 24–55 | 30–50 |
| > 55 | 40–100 |

同文写道，DSL 收益的 “quantitative validation in general as well as in particular cases, is hard and an important open problem”。

出处：https://www.rose-hulman.edu/class/cs/csse490-mbse/Readings/DSL-Survey-WhenHow.pdf

### 3.5 其他换算来源

**QSM Function Point Languages Table release 5.0** [全文] 用 2192 个单语言项目统计，覆盖 126 种语言，其中 37 种样本足够进表。SLOC/FP 按“平均 / 中位 / 最低 / 最高”：

| 语言 | 平均 | 中位 | 最低 | 最高 |
|---|---|---|---|---|
| Java | 53 | 53 | 14 | 134 |
| C | 97 | 99 | 39 | 333 |
| C++ | 50 | 53 | 25 | 80 |
| C# | 54 | 59 | 29 | 70 |
| JavaScript | 47 | 53 | 31 | 63 |
| SQL | 21 | 21 | 13 | 37 |
| Perl | 24 | 15 | 15 | 60 |
| Excel | 209 | 191 | 131 | 315 |

Excel 在 SPR 表里是 6，在 QSM 表里平均 209，差了一个数量级以上。这说明两张表的统计口径差别很大，跨表比较倍率不可靠。QSM 表未收录 Python、Ruby。本页未写发布年份，`07-语言倍率参考系-出处核实.md` 据 QSM 发布稿标为 2013 年。

出处：https://www.qsm.com/resources/function-point-languages-table

**USC Unified CodeCount（UCC）** 手册 v2018.07 [全文] 只提供跨语言的物理与逻辑 SLOC 计数和版本差分，没有语言换算系数。

出处：https://github.com/cl0ne/Unified-Code-Counter/blob/master/UCC_user_manual_v.2018.07.pdf

---

## 四、Berkholz 的表达力排名

Berkholz 没有比较同一项目的多语言实现。他用 Ohloh 数据统计各语言每次提交改动的行数（LOC/commit），文章发在 RedMonk 博客。Dr. Dobb's 上的版本未查到，InfoQ 有转述：https://www.infoq.com/news/2013/03/Language-Expressiveness/ （仅见于搜索结果）。

### 4.1 原文数据

原文是“Programming languages ranked by expressiveness”，发表于 2013-03-25 [全文]。

- **数据规模：** Ohloh，“some 7.5 million project-months”，约 20 年。
- **统计方法：** 取每月 LOC/commit 的分布，按当月提交数加权，按中位数排名。箱体为第 25 和 75 百分位，须线为第 10 和 90 百分位。只收入 RedMonk 流行度榜上的 52 种语言。
- **全域跨度：** 从 Augeas 的 48（第 1）、Puppet 的 52（第 2），到定长格式 Fortran 的 1629（第 52）。原文称 “31x variation”。
- **中位数与 IQR 都进前 10 的语言**（括号内为中位数与 IQR）：Augeas (48, 28)、Puppet (52, 65)、REBOL (57, 47)、eC (75, 75)、CoffeeScript (100, 23)、Clojure (101, 51)。Vala 和 Haskell 也在此列。
- **第一梯队流行语言：** 中位数在 309–1485 之间，原文说这 “equates to 6x–30x lower expressiveness than the top languages”。Java 与 C++ 的中位数都是 823。
- **其他排名：** Haskell 第 10，Go 第 24，Python 第 27，Ruby 第 34，Java 第 44，C++ 第 45，C 第 50。

关于 DSL，原文说 “Domain-specific languages are biased toward high expressiveness”，举 Augeas、Puppet、R、Scilab 为例，VHDL（第 38）是例外。

作者声明了两条局限：

- 方法假设 “commits are generally used to add a single conceptual piece”。
- 这个指标 “won't tell you how readable … or how long it takes to write it … so it's not a measure of maintainability or productivity”。

出处：https://redmonk.com/dberkholz/2013/03/25/programming-languages-ranked-by-expressiveness/

### 4.2 后续讨论

- 同一作者次日的文章 [网页] 说这个指标测的是 “expressiveness in practice rather than in theory”。标准库、开发文化（例如 JavaScript 项目常把 jQuery 直接提交进仓库）、开发者群体和语言年代都会影响结果。https://redmonk.com/dberkholz/2013/03/26/what-does-expressiveness-via-loc-per-commit-measure-in-practice/
- 作者另一篇文章 [全文] 把排名与 Hammer Principle 上约 2,500 名开发者对 “This language is expressive” 的投票对照，称 “a very clear correlation”，但没有给相关系数。https://redmonk.com/dberkholz/2013/03/26/some-external-validation-on-expressive-languages/
- Gousios 2013-03-27 的复核 [网页] 用 GHTorrent 约 850 万次提交，只统计修改过的文件，按扩展名识别语言，结论是结果 “contradicts many observations” of Berkholz。https://gousios.org/blog/commits-and-programming-languages.html

---

## 五、DSL 相对通用语言的实测倍率

### 5.1 汇总

| 案例 | 对照 | 单位 | 倍率 | 主要限制 |
|---|---|---|---|---|
| LMQL 2023 | LLM 编排 DSL 对作者自写的 Python | 功能代码行 | 2.7–4.3×（计算得出） | 基线由 DSL 作者编写 |
| P2 / LEAPS 1997 | 数据结构生成器对手写 C | 代码行 | 4× | 生成器本身 50K 行未计 |
| GAL 1999 | 显卡驱动 DSL 对手写 C | 代码行 | 规格至少 9×；摊入生成器 3.5× | 3.5× 是作者外推 |
| BOOM-MR 2010 | Overlog 对 Hadoop Java | 代码行 | 仅 Overlog 约 16.6×；计入 Java 约 3.9×（计算得出） | 基于 Hadoop 改造 |
| BOOM-FS 2010 | Overlog + Java 对 HDFS | 代码行 | 约 11×（计算得出） | 功能不对等 |
| Sawzall 2005 | 日志分析 DSL 对 C++ MapReduce | 代码行 | 10–20× | 作者概括，无逐程序数据 |
| Polar 2009 | 图形化 DSM 对现行手写 | 开发时间 | 7.5–10× | 基线为估计值；作者含厂商 |
| MetaCase 2002 | 图形化 DSM 对同框架内手写 Java | 开发时间 | 4.0–5.3× | 受试 2 人；作者为厂商 |
| RISLA 1998 | 金融 DSL 对原流程 | 交付周期 | 约 4–6×（计算得出） | 基线为估计值 |
| FAST（Bell Labs） | 产品线 DSL | 生产力 | 4–5× | 二手转引 |
| Sprat 2017 | 生态模拟 DSL 对 C++ | 任务用时 | 约 1.45–2.3×（计算得出） | 任务简化；样本量未查到 |
| Kieburtz 1996 | 生成器 DSL 对 Ada 模板 | 生产力、错误数 | 显著更优，具体倍率未查到 | 4 名受试 |
| Panach 2015 | 工业 MDD 工具对手写 | 工作量、生产力 | 无显著差异 | 学生受试，统计功效低 |

### 5.2 逐项说明

**Kieburtz et al. 1996**，ICSE-18，pp. 542–553，DOI 10.1109/ICSE.1996.493448 [摘要]。

- **对照与任务：** 用领域规格语言构造的程序生成器，对比可复用的 Ada 模板。任务是美军 C³I 系统的消息翻译与校验模块，测试用例取自空军的消息规格。
- **实验设计：** 4 名受试者，由独立承包商提供并监督。
- **结果：** 原文是 “greater productivity was achieved and fewer error were introduced when subjects used the program generator … statistically significant at confidence levels exceeding 99 percent”。
- **缺口：** 全文未开放，具体倍率未查到。

出处：https://dl.acm.org/doi/10.5555/227726.227842

**van Deursen & Klint 1998**，“Little Languages: Little Maintenance?”，Journal of Software Maintenance 10:75–92 [全文]。RISLA 是描述利率产品的 DSL，编译成 COBOL。

- **结果：** “the time it costs to introduce a new product is down from an estimated three months to two or three weeks”。三个月是估计值。
- **反面经验：** RISLA 产品定义 “have become longer and longer”；每次扩展语言都要改编译器和 COBOL 库，需要编译器技术，这不是 COBOL 团队的常规背景。
- **转引：** 同文引用 Bell Labs 的 FAST 方法 “a productivity increase with a factor of four to five”（Weiss 1997）[二手]。

出处：https://homepages.cwi.nl/~paulk/publications/JSM98.pdf

**Thibault, Marlet & Consel 1999**，IEEE TSE 25(3)，GAL 显卡驱动 DSL [全文]。这个案例同时给出两种口径，是“参考系决定倍率”最直接的例子。

- **只算规格：** “GAL specifications that have been written are at least a factor of 9 smaller than the corresponding existing C driver”。这是已写规格与对应驱动的实测对比。手写驱动平均约 1,500 行，X server 现有驱动合计 35,000 行。
- **摊入生成器：** 生成器约 5,500 行 C。作者估计全部驱动可以由不到 4,000 行 GAL 生成，加上生成器合计不到 10,000 行，称 “an estimated productivity gain of a factor of 3.5”，前提是 “assuming code size proportional to effort”。这一步是外推。
- **代价：** 生成的驱动目标码平均比手写的大 30%。

出处：https://john.cs.olemiss.edu/~hcc/csci658/notes/localcopy/DSLDesignImpleVideo.pdf

**Batory 1997**，“Intelligent Components and Software Generators”，UT Austin TR-97-06 [全文]。

- **结果：** 用 P2 数据结构生成器重写 LEAPS，原文是 “a 4-fold reduction in code volume: LEAPS is 20K lines of code; RL was about 5K”。P2 本身有 50K 行，未计入。
- **性能：** RL 生成的程序平均快约 50%。

出处：https://www.cs.utexas.edu/ftp/predator/intelligent.pdf

**Pike et al. 2005**，Sawzall（Google）[全文]。原文两处表述：

- “shorter—by a factor of ten or more—than the corresponding C++ code in MapReduce”
- “Sawzall programs tend to be around 10 to 20 times shorter than the equivalent MapReduce programs in C++”

这是作者的概括，文中没有逐程序数据。

出处：https://research.google.com/archive/sawzall-sciprog.pdf

**Alvaro et al. 2010**，BOOM Analytics，EuroSys'10 [全文]。

- **BOOM-FS**（Table 2）：HDFS 约 21,700 行 Java；BOOM-FS 用 1,431 行 Java 加 469 行 Overlog，按总行数约 11×（计算得出）。BOOM-FS 不支持文件访问权限、状态监控网页和数据块主动再平衡，功能不对等。四个人月只是 Overlog 元数据处理部分的工时，不是整个系统的总工时。
- **BOOM-MR：** 55 条 Overlog 规则共 396 行，另有 1,269 行 Java，替换了 Hadoop 88,864 行中的 6,573 行，性能 “very similar to that of Hadoop MapReduce”。只算 Overlog 约 16.6×，计入 Java 约 3.9×（均为计算得出）。原文只说 Overlog 补丁 “an order of magnitude fewer lines”。
- **LATE 调度策略：** Overlog 5 条规则 30 行；原版 Hadoop 实现 “adding or modifying over 800 lines of Java”，约 27×（计算得出）。

出处：https://www.neilconway.org/docs/booma_eurosys2010.pdf

**Kärnä（Polar）, Tolvanen & Kelly（MetaCase）2009**，DSM 研讨会 [全文]。DSM 指领域专用建模，用图形化模型生成代码。

- **任务：** 心率表的界面应用，6 名开发者。
- **DSM 一侧：** 实测用时 75–125 分钟，平均 105 分钟。
- **基线：** 用的是估计值，原文是 “would take about 960 minutes”。
- **结论：** 原文是 “at least 7.5 times and on average 10 times as productive”。
- **作者：** 两位来自工具厂商 MetaCase。

出处：https://www.dsmforum.org/events/dsm09/papers/karna.pdf

**Pohjonen & Kelly（MetaCase）2002**，Dr. Dobb's “Domain-Specific Modeling” [全文]。

- **任务：** 同一个秒表计圈功能，一次用 DSM 建模生成，一次在同一框架内手写 Java。
- **结果：** 资深开发者 200 秒对 38 秒（5.3:1），初级开发者 639 秒对 160 秒（4.0:1），合计 23.6 对 117.2 个功能每小时（5.0:1）。
- **限制：** 受试只有两人，作者是厂商。
- **厂商引述：** 同文称 Nokia “develops mobile phones up to 10 times faster”，来源是 MetaCase 自己的案例。

出处：https://www.metacase.com/papers/drdobbs_domain-specific_modeling.html

**Johanson & Hasselbring 2017**，Empirical Software Engineering 22(8)，Sprat 生态模拟 DSL 对 C++ [摘要]。评估文档 §1.6 已有这一条，数字一致。

- **设计：** 受试是生态学家，实验嵌在在线问卷里。
- **结果：** DSL 组正确率得分高 61–63%；原文是 “average time spent per task was reduced by 31 % up to 56 %”，折合约 1.45–2.3×（计算得出）。
- **局限：** 任务经过简化，编辑器没有完整的 IDE 支持。样本量未查到。

出处：https://oceanrep.geomar.de/34903/

**Beurer-Kellner et al. 2023**，LMQL，PLDI [全文]。LMQL 是一门查询大语言模型的语言，与 J++“调判断接口”的场景最接近。

- **计量口径：** “functional lines of code … excluding comments, empty lines, and fixed prompt parts (e.g. few-shot samples)”。基线是作者用 Python 和 HuggingFace generate 自写的程序。

| 任务 | Python 基线 | LMQL | 倍率（计算得出） |
|---|---|---|---|
| Odd One Out | 34 | 9 | 3.8× |
| Date Understanding | 38 | 13 | 2.9× |
| Arithmetic Reasoning | 59 | 22 | 2.7× |
| ReAct | 78 | 18 | 4.3× |

- **表与正文矛盾：** 正文说 ReAct 用了 “22 LOC … 63% fewer lines”，算术推理是 “18 LOC, compared to 78 LOC”。表里这两行的标签与正文对调了，但四个倍率的取值不受影响。
- **成本：** 同文报告 API 调用成本节省 26–85%。

出处：https://arxiv.org/pdf/2212.06094

**APPL**（ACL 2025）[全文]。这一项比较的是 DSL 与 DSL，不是 DSL 与手写 Python，只作单位参考。

- **单位：** AST 节点数（AST-size），用 Python 的 ast.parse 统计。
- **结果：** 在 CoT-SC、ReAct、SoT、MemWalk 四个任务上，LMQL 是 APPL 的 1.63–2.03 倍，SGLang/Guidance 是 1.67–2.20 倍。

出处：https://aclanthology.org/2025.acl-long.63.pdf

**Panach et al. 2015**，Information and Software Technology，DOI 10.1016/j.infsof.2015.02.012 [摘要]。

- **设计：** 用工业模型驱动开发工具 INTEGRANOVA 对比手写代码，受试是硕士生，任务是小型网页应用。
- **结论：** “no significant differences between both methods with regard to effort, productivity and satisfaction”。
- **局限：** 统计功效偏低。

出处：https://doi.org/10.1016/j.infsof.2015.02.012

**Schiedermeier et al.**，MODELS 2024，扩展版刊于 Software and Systems Modeling 2026 [摘要]。

- **设计：** 28 人交叉实验，任务是把遗留代码迁移到 REST 接口。
- **结果：** 两个迁移对象中只有一个测到显著提速；DSL 组的样板配置类错误明显减少。
- **缺口：** 具体倍率未查到。

出处：https://link.springer.com/article/10.1007/s10270-026-01391-9

---

## 六、同规格多实现对比的补充

评估文档已详细记录 Prechelt 2000 和 Nanz & Furia 2015。这里补一项，另订正一处表述。

**Hudak & Jones 1994**，“Haskell vs. Ada vs. C++ vs. Awk vs. …” [全文]。这是美国海军水面作战中心（NSWC）几何区域服务器原型的多语言实现对比。

| 实现 | 代码行 | 开发小时 |
|---|---|---|
| Haskell | 85 | 10 |
| Ada | 767 | 23 |
| Ada9X | 800 | 28 |
| C++ | 1105 | 未报告 |
| Awk/Nawk | 250 | 未报告 |
| Rapide | 157 | 54 |
| Griffin | 251 | 34 |
| Proteus | 293 | 26 |
| Relational Lisp | 274 | 3 |
| 新手写的 Haskell | 156 | 8 |

Ada 对 Haskell 的代码量约 9 倍（计算得出）。作者自己提醒这张表 “should not be used alone to infer any conclusions”：部分原型从未运行，Awk 的作者起初把代码塞进了 101 行。

出处：https://web.cecs.pdx.edu/~apt/cs457_2005/hudak-jones.pdf

**Nanz & Furia 2015 的一处表述：** 原文 “Java programs are on average 2.2–2.9 times longer than programs in functional and scripting languages” 说的是 Java，不是整个过程式与面向对象语言组。Java 在该组里 “tends to be slightly more concise”，所以整组相对函数式和脚本语言的差距不会小于这个数。

出处：https://arxiv.org/pdf/1409.0252

---

## 七、对验收①的参考

**参考系需要写进口径，并分别报告。** GAL 只算规格是 9×，摊入生成器是 3.5×；BOOM-MR 只算 Overlog 约 16.6×，计入 Java 约 3.9×。同一个案例换一种算法，倍率就能相差 2.5 到 4 倍多。A-6 口径需要事先写明以下几项：

- J++ 源码计入，按 COCOMO II 的做法把它当作写给生成器的输入。
- J++ 编译或生成出来的调接口代码不计。
- J++ 运行时和手写基线用到的 SDK、标准库都不计。
- 固定的提示词文本（问题模板、示例）两边是否计入。LMQL 的做法是不计。
- 倍率同时报“仅 J++ 源码”和“计入运行时或胶水代码”两个数。

**关于 10× 通过线，现有证据只能说明两点。** 第一，在“只算 DSL 文本”的口径下，某些领域有 10× 以上的实测先例。第二，一旦计入生成器或胶水代码，同样的案例都落到 4× 以下。与 J++ 场景最接近的 LMQL，在第一种口径下也只有 2.7–4.3×。至于通过线本身是否合适，这批证据不足以判断。

**代码量倍率不能直接当生产力倍率用。** 按 Jones 的“语言级别对生产力”表，Java 所在的 4–8 级每人月 10–20 功能点，生成器所在的 16–23 级每人月 15–30 功能点，只差约 1.5 倍。受控实验里的时间倍率（Sprat 1.45–2.3×）也明显低于代码量倍率。验收①的“所需思考量”若用完成时间来量，预期会低于行数倍率。

**Jones 旧表可以给一个量级先验**（计算得出）：

- 程序生成器类（级别 20，每功能点 16 行）相对 Java（53 行）约 3.3 倍，相对 C（128 行）约 8 倍。
- 要相对 Java 达到 10 倍，需要约每功能点 5 行，对应表中级别 60–70 一档（Reuse default、第五代语言）。

**实验设计上可以借用的做法：**

- 手写基线由不参与 J++ 设计的人编写。LMQL 的基线由 DSL 作者自写，有利于 DSL 一方。
- 多名实现者，取中位数。Prechelt 发现同一语言内程序员之间的差异往往大于语言之间的差异。
- 行数之外同时报 AST 节点数或 token 数，减少排版习惯带来的偏差。APPL 用的是 AST 节点数。

---

## 八、未查到

- Kieburtz 1996 的具体倍率（全文未开放）。
- Berkholz 文章的 Dr. Dobb's 版本。
- COCOMO II 1998 版与 2000 版系数差异的原因。
- UCC 的语言换算系数（手册中没有）。
- QSM 表中 Python、Ruby 的数据。
- Sprat 实验的样本量。
- Schiedermeier 实验的具体倍率。
- Berkholz 与 Hammer Principle 对照的相关系数。
