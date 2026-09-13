# lint-like-rust

> **已归档（2026-09-13）。** 项目结论：Rust 风格的所有权/借用/生命周期规则只对自己管理资源生命周期的代码有意义；业务后端代码的错误在系统边界契约上，静态 lint 看不到。对 Museon 30 个真实修复提交的分类见 [docs/research/museon-bug-taxonomy.md](docs/research/museon-bug-taxonomy.md)，方向演变与结论见 [docs/DIRECTION.md](docs/DIRECTION.md)。代码保留供参考，不再维护。

面向 Python 的 Rust 风格所有权、借用和资源有效期检查器。核心以 Rust 编写，消费语言无关的中间表示；Python 是第一个前端。

这是一个早期静态分析项目。默认不需要修改业务源码或添加标注，使用已知 API 模型和可解析函数效果；可选项目契约用于补充无法推断的所有权边界。检查器不执行或导入目标 Python 代码。

## 当前方向

2026-09-13 起路线重定：先用真实修复历史证明需求，再做 unsound 的高精度 `llr lint`；sound 分析链冻结。见 [方向重定](docs/DIRECTION.md)。

## V2 重写：面向智能体的安全诊断（已冻结）

新路线采用项目语义解析、CFG 数据流分析和可比较的检查义务。当前仅完成第一条跨函数资源分析链，完整所有权／借用重写仍在进行。

```sh
llr analyze examples/v2/before --entry 'main::run'
llr analyze examples/v2/after --entry 'main::run'
llr analyze examples/v2/unknown --entry 'main::run'
```

三者分别退出 1（违规）、0（声明范围内已验证）、3（未验证）。未知不再作为成功；旧 `llr check` 保留原型行为。

[重写编排](docs/v2/ROADMAP.md) · [语义规范](docs/v2/SEMANTICS.md) · [当前实现与验收](docs/v2/STATUS.md) · [前端选型](docs/v2/FRONTEND-DECISION.md)

## 原型构建与运行

```sh
cargo build --release
./target/release/llr check path/to/python/project
./target/release/llr check path/to/python/project --format json
./target/release/llr check path/to/python/project --strict
./target/release/llr rules
```

安装本地命令：

```sh
uv tool install .
# 或：cargo install --path . --locked
llr check path/to/python/project
```

正常模式中，未覆盖行为会出现在报告中；strict 模式将未覆盖行为也视为未通过。无诊断不等于对任意 Python 程序的完整安全证明。

## 试用默认规则

```sh
llr check examples/safe.py        # 无诊断、无分析缺口
llr check examples/violations.py  # OWN001、BOR002、LIFE001；退出 1
```

示例只用于静态扫描，不要运行。其中描述符转移属于额外的所有权策略；构造失败分支仍会报告分析缺口。

更多见 [实际检查过的修改前后对比](examples/before_after/README.md)。

## 检查 Museon

本机已安装 `llr`。重新扫描选定的产品源码范围并生成 JSON、摘要和运行元数据：

```sh
cd ~/Documents/lint-like-rust
python3 scripts/scan_museon.py
```

报告在 `reports/museon.md`；实际范围和限制见 [Museon 扫描说明](docs/MUSEON-SCAN.md)。脚本需要 Python 3.11+。

## 规则目标

| 规则 | 目标 |
|---|---|
| OWN001 | 通过原绑定或别名使用已转移的对象 |
| OWN002 | 重复转移对象 |
| BOR001 | 共享 / 独占借用重叠冲突 |
| BOR002 | 在借用约束下执行不允许的访问 |
| BOR003 | 对仍被借用的对象执行转移或失效操作 |
| LIFE001 | 继续使用已经失效的资源 |
| LIFE002 | 返回的资源在 with / finally 清理中失效 |
| ERR001 | 无意丢弃要求使用的返回值 |

具体实现范围、验证结果和限制以 [实现状态](docs/STATUS.md) 为准；不能将规则目标当作对所有 Python 语法的覆盖承诺。

## 所有权从哪里来

普通 Python 赋值建立别名；普通传参不自动等于 move。默认已建模 `os.open → os.fdopen` 的成功转移路径；业务消耗型 API 可以通过项目配置补充：

```toml
[tool.llr.contracts."app.queue.submit"]
consumes = [0]

[tool.llr.contracts."app.storage.persist"]
must_use = true

[tool.llr.contracts."app.analysis.compare"]
mutable = [0]
readonly = [1]
```

参数位置从零开始；方法接收者不计入显式参数。配置是可信契约，应与真实实现一致。配置不改变 Python 的运行时行为。

借用规则包含额外的 Rust 风格限制策略。例如，某些 Python 允许的迭代期间修改操作仍可能被报告为借用违例。这种诊断不是 Python 语法错误，也不必然代表已经发生运行时故障。

## 架构

```text
Python 解析 / 符号解析 / 调用效果 / 库模型
                       ↓
统一 IR：对象、别名、读取、写入、转移、借用、失效、控制流
                       ↓
Rust 核心：状态传播、活跃借用、资源有效期、诊断证据
                       ↓
CLI / JSON / CI
```

`src/ir.rs` 与 `src/engine.rs` 不依赖 Python AST 或 tree-sitter。未来的 Go / TS 前端可以复用核心，但必须独立表达其复制、别名、异常和并发语义。

第一版使用 tree-sitter-python，而非绑定 Ruff 的内部 crate。它可与 Ruff 和现有类型检查器一起运行。

## 开发

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

测试应同时包括错误用法、合法用法和无法判断的情况，覆盖别名、分支、异常、资源清理与借用最后使用点。不能通过删除合法反例或静默跳过未知行为来获得“通过”。

- [实施计划](docs/PLAN.md)
- [实现状态](docs/STATUS.md)
- [Museon 扫描](docs/MUSEON-SCAN.md)

当前为本地项目，未发布到公共包仓库。

