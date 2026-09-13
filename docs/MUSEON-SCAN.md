# Museon 扫描与验收案例

本页记录只读源码盘点和实际扫描范围。扫描统计见 [运行报告](../reports/museon.md)，能力边界见 [实现状态](STATUS.md)。目标检出目录为 `/Users/jiaweichen/Documents/vscode/museon/museon`。盘点未修改目标仓库，已有修改保持原状；后续扫描也不得导入、执行或改写目标 Python 代码。

## 范围与语言版本

根 `pyproject.toml` 声明 uv workspace，七个成员均要求 Python `>=3.11`；已配置 Ruff 的成员使用 `py311`。API 与 agents 已使用 Ruff、Mypy；新检查器用于增加所有权、借用和有效期分析。

首轮产品源码根目录如下，路径相对于目标仓库：

- `apps/api/app`
- `apps/agents/museon_agents`
- `apps/agents/infra`
- `apps/agents/profiles`
- `apps/agents/server`
- `apps/sandbox-runtime/museon_sandbox_runtime`
- `apps/render-service/render_service`
- `packages/museon-alerting/museon_alerting`
- `packages/museon-rendering/museon_rendering`
- `packages/museon-sandbox-contract/museon_sandbox_contract`

CLI 默认排除 `.git`、`.venv`、`venv`、`node_modules`、`__pycache__` 和 gitignore 忽略文件。扫描脚本另外排除 `.mypy_cache`、`.ruff_cache`、`.pytest_cache`、`dist`、`build`，以及 `**/tests/**`、`**/test_*.py`、`**/*_test.py`、`**/deprecated_legacy/**`、`apps/api/app/script/**`。`samples/**`、仓库级 `scripts/**`、`apps/deprecated_legacy/**` 已在上述源码根范围之外。

实际排除项完整记录在运行元数据中。`profiles` 包含实际运行代码，因此纳入扫描。

## 可复现的只读命令

在 lint-like-rust 项目根目录执行；报告写入 lint-like-rust 的 `reports/`，不写入 Museon：

```sh
cd ~/Documents/lint-like-rust
python3 scripts/scan_museon.py
# 将分析缺口也视为未通过：
python3 scripts/scan_museon.py --strict --output reports/museon-strict.json
```

脚本指定上面的十个源码根，并明确排除 tests、test_*.py、*_test.py、deprecated_legacy、script、缓存与构建目录；完整参数记录在 `reports/museon.metadata.json`。CLI 同时尊重 gitignore。报告包含耗时、目标 HEAD、配置来源、二进制摘要及 Git 状态前后对比。普通模式没有发现诊断不等于所有代码已被证明安全。

## 零标注语义验收案例

下表以现有源码模式为依据；行号来自盘点时的工作树，后续可能变化。负例描述用于合成测试，**不是对当前 Museon 代码存在 bug 的断言**。不向目标仓库添加任何标注或测试代码。

| 源码参考（相对 Museon 根目录） | 应接受的行为 | 应检查的负例或覆盖边界 |
| --- | --- | --- |
| `apps/sandbox-runtime/museon_sandbox_runtime/events.py:140` | 局部变量引用全局字典后写入仍是合法 Python | 效果摘要必须识别这是共享对象写入，不能误认为局部新对象；是否禁止由策略决定 |
| `apps/sandbox-runtime/museon_sandbox_runtime/events.py:148` | 文件工厂调用后在 `finally` 关闭，关闭前完成写入 | 关闭后经原变量或别名读写应报告；无法解析 `_io_open` 工厂时应显示模型缺口 |
| `apps/sandbox-runtime/museon_sandbox_runtime/replacement_marker.py:177` | `mkstemp` 返回 fd，成功交给 `os.fdopen` 后由 wrapper 关闭 | wrapper 关闭后再用旧 fd 应报告；`fdopen` 抛异常时不能误认为转移已完成 |
| `apps/sandbox-runtime/museon_sandbox_runtime/replacement_marker.py:188` | 原始 fd 在 `finally` 中关闭 | 关闭后使用、经过别名二次关闭应检查；异常分支也需传播状态 |
| `apps/sandbox-runtime/museon_sandbox_runtime/driver.py:247` | 遍历 `list(dict.items())` 快照时移除原字典项目 | 直接遍历原字典或其动态视图时改变字典大小，应检查；仅替换已有键的值不能混为同一种错误 |
| `apps/agents/server/v1/endpoints/agent_runtime_releases.py:212` | HTTP client 上下文结束后读取已完整缓冲的 response 内容 | 不能把所有返回对象都视作 client 的借用；流式响应需要单独的生命周期模型 |
| `apps/agents/server/main.py:50` | 新任务加入持有集合，完成回调移除 | 不能因函数没有返回 task 就认定无人持有；需要建模任务逃逸和集合持有 |
| `apps/sandbox-runtime/museon_sandbox_runtime/late_task.py:507` | 明确解释有意启动后台任务的检查策略 | 丢弃 task 返回值可作任务持有警告候选，不能仅凭调用判定必然失效或把它当成完整所有权证明 |
| `apps/agents/infra/db/database_v2.py:652` | 异步锁 guard 跨合法 await，退出上下文释放 | 不能把任意锁内 await 一律报错；未知共享对象访问关系应标记未验证 |
| `apps/agents/server/live_phone/worker.py:266` | WebSocket 存入字段集合并跨 await 使用 | 字段逃逸、长期任务共享、关闭后的其他访问需要跨函数状态；局部分析不足时不得宣称已验证 |

## 后续验收记录要求

对每条真实诊断保存文件位置、规则、简短证据和人工复核结论，区分确定违反已建模协议、额外借用策略限制、误报及未覆盖。解析错误必须单列。该项目第一版不保证已支持表内全部模型；表格用于暴露能力差距，不能替代测试结果或扫描记录。
