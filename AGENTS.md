# lint-like-rust

Build a language-independent Rust semantic analysis core and a Python frontend. Python is the first target; do not couple the core to tree-sitter or Python AST nodes.

The first release includes ownership, shared/exclusive borrowing and resource validity within an explicitly supported subset. Zero source annotations is the default; infer known effects and accept optional project contracts. Never pretend Python assignment is a Rust move. Unknown behavior must remain visible, and strict mode must fail when analysis is incomplete.

Do not execute or import scanned Python code. Do not edit a scanned repository. Keep frontend resolution, analysis facts, configuration and output separate. Preserve legitimate Python semantics and label additional borrowing policy restrictions accurately.

Use `cargo test`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`. Include positive, negative, alias, branch, unknown and exception-path regression tests for semantic changes. No unsafe automatic fixes or silent dropping of parse errors.

Subagents are authorized for bounded parallel implementation. Respect assigned file ownership and coordinate shared interfaces before changing them.


## 验收驱动的接续

`docs/v2/ACCEPTANCE-REPLAN.md` 的固定 read_file 五变体里程碑已达标（整体 5/11）。后续仍按完整语料组设定验收批次，剩余六项及所有权/借用未完成。不要再次把单个语法/IR 机制加少量测试作为结束条件。未知诊断只能在相关效果覆盖得到证明后消除；不能修改固定语料或预期制造提升。全量测试数是回归证据，不是用户目标进展指标。

## 持续飞书看板

用户最新要求每轮进度只更新多维表格 TODO 与里程碑。遵循 `docs/v2/LARK-STATUS-SYNC.md` 最新同步范围，坐标见 `.loopx/lark/base.json`。先回写 LoopX，再更新既有记录并读回验证。目标文档与可视化总览停止例行进度/时间更新，仅目标规则变化或用户明确要求时修改。同步不能替代产品推进。

## 持续执行（用户纠正）

心跳间隔不是单轮工作时限。连续完成完整功能验收批次，包含实现、集成、必要测试和状态回写；不要做完一个格式字段、几项测试或看板同步就结束。可修复问题在本轮处理。批次验收完成、确有外部阻碍无法继续或用户要求暂停才结束；测试数量不能代替覆盖或功能效果。
