# 修复实验：初始工具反馈

2026-09-13；仅静态工具运行，AI 修复次数为 0。该组是开发语料的流程烟测，不是独立效果评估。

| 输入 | Ruff 0.12.12 | Pyright 1.1.414 | llr |
|---|---|---|---|
| 单函数注入错误 | 0 条诊断 | 0 错误/警告 | exit 1，违规 |
| 跨函数注入错误 | 0 条诊断 | 0 错误/警告 | exit 1，违规 |
| 正确原版 | 0 条诊断 | 0 错误/警告 | exit 0 |

复现入口 `scripts/collect_repair_feedback.py`：复制指定输入到临时目录，Ruff 使用 isolated 配置与 E4/E7/E9/F；Pyright 使用固定 standard、Python 3.11/Linux、禁用 useLibraryCodeForTypes 的显式配置。Node guard 阻止环境发现子进程，每份输入拦截了 2 次。扫描目标未执行或导入；扫描后源码字节一致。llr 选择 task::_read_file，JSON 记录实际二进制 SHA-256 与分析上下文。目标目录路径被替换为固定占位符。

证据：同目录 repair-smoke-feedback-01/02/03.json（本地忽略的 JSON）。所有工具 JSON 成功解析，Pyright 每份分析 1 个文件。非协议退出码/无 JSON 为工具失败，不会当成零诊断。

目前只验证“在这两个人工注入的开发样本上有额外诊断信号”。不推断真实缺陷召回率、一般误报率或 AI 修复收益。双方运行时需接收相同 Ruff/Pyright 输出；B 组额外收到 llr，不能收到本表、输入身份和正确答案。

参与者运行边界仍待验证：官方配置参考记录了 shell_tool、multi_agent、web_search 等开关，但关闭单一工具或设置只读不证明所有读路径均隔离。下一步用受控执行 host 验证仅任务文本输入、无开发目录/历史访问，再冻结模型和预算。
来源：[OpenAI 配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)。
