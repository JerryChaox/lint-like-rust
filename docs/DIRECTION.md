# 方向重定（2026-09-13）

## 初衷不变

给 AI 编码智能体一个像 Rust 编译器一样严格、可验证、可修复的诊断反馈回路。Python 是第一个语言。

## 为什么改路线

截至 revision23 的实验事实：

- 固定 Museon 语料 11 个目标通过 5 个；每过一个目标都要专门建一个库模型。
- 旧原型扫 Museon 2585 个文件：0 条诊断，556300 个分析缺口。
- 两轮配对 AI 修复试验（12 次修复、12 次盲评）：加 llr 组与只用 Ruff+Pyright 组修复率相同，llr 组多耗 9%–31% token。收益为零。

结论：追求 sound（"未验证即失败"）的全程序分析在 Python 上不可达，而且抓的那类问题不是 AI 写 Python 的主要失败模式。

## 新路线

1. **先证需求，再造工具。** 从 Museon 真实修复历史统计 AI 写 Python 最常犯的错误类型，判断哪些 Ruff / Pyright strict 已能抓、哪些抓不住。只有"抓不住且高频"的类别才值得做规则。
2. **clippy，不是 borrowck。** 新命令 `llr lint` 是 unsound 的高精度 lint：只报有把握的模式，未知一律按良性处理，不再有 exit 3。以真实项目上的 precision 和有效发现数为唯一指标。
3. **配对实验前置。** 每加一条规则，先用配对 AI 修复实验证明它改变了修复率、轮数或新增错误率；证不出就删。
4. **sound 分析链 (`llr analyze`) 冻结。** 保留代码与测试，不再投入；语言无关 IR 与 Go/TS 前端不排期。

## 里程碑

| 里程碑 | 交付 | 验收 |
|---|---|---|
| N1 需求证据 | `docs/research/museon-bug-taxonomy.md` | ≥30 个真实修复提交分类；每类标注现有工具能否抓 |
| N2 clippy 模式 | `llr lint <root>` | Museon 全仓 4580 个 Python 文件：0 条发现，precision N/A（0/0），33.60s（≤120s） |
| N3 规则收益 | 每条规则一份配对实验 | 修复率 / 轮数 / 新增错误率有统计差异，否则删除规则 |
| N4 接入 CI | Museon CI 中以 warning 运行 | 两周内 FP 投诉 < 1 次/周 |

## 边界

- 公开仓库不得包含 Museon 源码；研究报告只放聚合统计与 ≤3 行的模式片段。
- 不执行、不导入目标 Python 代码。
- 不为提高通过率修改语料或预期。

## 相关工作（2026-09-13 调研）

- [Factory.ai: Using Linters to Direct Agents](https://factory.ai/news/using-linters-to-direct-agents)：把结构性规则（可 grep、可 glob、架构边界、安全、可测试性、可观测性、文档）编码进 agent 循环，lint 绿即合并门。生产验证了"linter 作为 agent 自纠正信号"这条路；明确不覆盖语义、数据流、资源、并发。本项目定位在其空白处。结构性类别用 Ruff + import-linter 覆盖，不自研。
- [AI Coding Agents Need Better Compiler Remarks (arXiv 2604.13927)](https://arxiv.org/abs/2604.13927)：精确结构化诊断比模糊诊断让 agent 成功率高 3.3 倍，模糊诊断诱发破坏语义的修改。支持本项目的诊断协议设计。
- [PyFlow (arXiv 2608.07026)](https://arxiv.org/abs/2608.07026)：Python 的 IFDS 跨函数分析框架，只演示了 taint。若 N1 选出的规则需要跨函数数据流，参考其 IR 与求解器设计，不自研求解器。
- [DataFlowBench](https://github.com/BrokkAi/dataflowbench)：typestate 赛道尚无用例；Python 上的资源 typestate 检查目前是空白。
- Ruff 无插件机制（[FAQ](https://docs.astral.sh/ruff/faq/)）。AST 级通用规则的归宿是贡献上游；Rust 底座候选为 `ruff_python_parser` + `ruff_python_semantic`，类型从 Pyright/ty 取。

## 结论与归档（2026-09-13）

N1 对 Museon 30 个真实修复提交的分类：Ruff 命中 1/30，Pyright strict 0/30；零个属于资源生命周期、漏 await 这类语言级语义错误，21/30 是外部契约、状态分支、分页完整性、部署环境这类系统边界问题。N2 用现有规则 best-effort 扫 Museon 4580 个文件，0 条发现。

判断：
- Rust 式规则只对直接管理内核/原生资源生命周期的代码有意义（数据管道、训练脚本、原生 GUI 胶水层）。业务后端把生命周期交给了框架和 SDK，没有可检查的对象。
- Museon 静态工具的天花板约为 10/30，由四条配置级改动达到：从 Supabase schema 生成 Pydantic 模型、Pyright strict、禁 Any（ANN401）、SQL 走类型化查询层，加 Ruff BLE001。其余 20 个需要真实依赖的契约测试、运行时不变量与状态矩阵设计，任何 linter 都不覆盖。
- 项目特有模式（同步 client 不许 await、ACK 前必须写终态）用 Semgrep 写一两条规则即可，不值得独立工具。

项目归档。若将来面向数据管道/训练脚本重启，先按 N1 的方法拿目标用户的真实 bug 历史验证需求，再写代码。
