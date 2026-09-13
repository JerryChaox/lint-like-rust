# Museon 真实代码验收语料

语料位于 [tests/corpus_v2](../../tests/corpus_v2/manifest.json)。包含 3 个 Museon 函数原文和 11 个可静态解析的变体。**错误变体是人为注入，不是发现的 Museon 缺陷。** 清单中的预期是验收目标，不是已经通过的分析结果；初始化状态全部是 `not_evaluated`。

| 案例 | 原始位置 | 错误注入 | 前端需求 |
|---|---|---|---|
| `read_file` | sandbox-runtime / runtime_update.py:38–44 | 将读取移到 `with` 结束后 | 标准 open、上下文清理、异常边、资源使用 |
| `path_sha256` | render-service / media_io.py:151–157 | 在迭代读取前关闭文件 | Path 参数类型、方法解析、闭包捕获、sentinel iter 调用、循环 |
| `append_brain_log` | mobile-agent / brain/tick.py:42–46 | 写入前关闭文件 | Path 构造与 `/` 运算、路径方法、格式化字符串、json 效果 |

每个案例均有 `source_excerpt.py`（逐字原文）、`original.py`（原文加最小导入脚手架）、`mutated_error.py`（错误副本）和 `repaired.py`（还原原文的修复副本）。清单记录当前工作树源文件 SHA-256、原文 SHA-256、包含端点的行范围、Git HEAD、导入脚手架和每个变体 SHA-256。Git HEAD 只标识获取时 checkout，不承诺工作树等于 commit；源文件散列才是精确内容标识。

`read_file` 额外包含 `cross_function_error.py` / `cross_function_repaired.py`。它们把读取提取到 `_read_contents(handle)`，用于验证参数效果摘要和调用证据链。**这是人为重构的测试变体，不是 Museon 原始调用链。** 正确分析应把 helper 中的读取义务带回调用方，在资源关闭后调用时报 `LIFE001`，并同时给出创建、关闭、调用、被调函数使用位置。

## 验收含义

- `expected_rules` 是资源生命周期这个限定范围内要求命中的规则，不表示不存在其它类别问题。
- `verified_within_scope` 是预期目标。零诊断但相关操作无法解析，必须记作 incomplete，不能计为通过。
- `path_sha256` 刻意保留真实闭包写法，不为配合当前分析器改成简单循环。不能理解 sentinel iterator 时应公开缺口。
- 读写关闭的文件即使被外层异常处理吞掉，仍应定位失败操作；这里不声称一定造成未捕获异常。
- 最小导入脚手架帮助单独解析。它没有原项目全部类型和依赖环境，不能把对片段的证明扩大到整个模块。
- `Path` 注解按标准类型契约解释；自定义子类重写、猴子补丁等假设明确列在 scope 中。
- 本批是资源生命周期纵向场景，不覆盖全部所有权、借用规则，也不证明不存在泄漏。

清单的 `semantic_events`、`required_capabilities` 和逐变体 `scope` 可供后续驱动器逐项比对。驱动器应保留 observed diagnostics、相关 incomplete 项、分析版本、配置、退出状态；不要覆盖 expected 字段。当前尚未建立完整自动验收驱动器。

## 静态完整性检查

```sh
python3 tests/corpus_v2/verify.py
python3 tests/corpus_v2/verify.py --source-root ~/Documents/vscode/museon/museon
```

该命令仅读取文本、核验散列并解析 Python AST，**不导入、运行原项目或任意测试变体**。后续 lint 验收也应遵守此边界。原 Museon checkout 未被修改。
