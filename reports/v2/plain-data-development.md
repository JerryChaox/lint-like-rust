# revision23 日志数据来源批次

已实现精确字符串与递归内置 JSON 数据事实，连接格式化、字符串拼接、Path/mkdir、默认 JSON 编解码和文件写入/清理，更新本机 llr。

| 验收 | 结果 |
|---|---|
| 不变日志正文 + 明确标注的开发调用方 | 原始/错误/修复/未知0/1/0/3 |
| 修复比较 | resolved / became_unverified |
| 单独选择函数入口 | 不复用其他调用方实参证明 |
| 未知条件求真、嵌套值、别名修改、未知调用、自定义编码器/hooks | 保持未验证 |
| JSON编码异常→处理器关闭→后续写入 | 检出资源违规 |
| 默认固定语料 | 5/11 |
| hash原manifest显式前提 | 3/3 |
| 所有权/借用 CLI 回归 | 保持1/0/3和修复比较 |
| 全量 Rust 回归、fmt、clippy | 383通过，检查通过 |
| Museon源摘录/11变体 | 哈希校验通过，未执行/修改 |

真实调用点审计：tick.py中7个_append_brain_log调用都构造字典，但字段值不是全部已证明。共同时间戳来源_now_iso(now_utc)涉及datetime调用协议，另有状态、阶段和计划来源。实际brain_tick及无前提日志入口继续未验证。本批的开发调用方不计入固定语料覆盖，不替代真实调用方证据。

代码：`src/frontend_v2.rs`、`src/cli_v2.rs`、`src/entry_contracts_v2.rs`。回归：`tests/plain_json_v2.rs`。机器证据：`reports/v2/log-data-scope/summary.json`及同目录报告、`reports/v2/corpus.json`。模型范围：`docs/v2/PLAIN-DATA-PROVENANCE.md`。
