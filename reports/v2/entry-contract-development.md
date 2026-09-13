# revision22 入口契约与 hash 整组验收

本批完成显式入口契约准入、前端事实连接、条件化诊断/配置指纹、CLI 与 hash 三变体验收。

| 验收 | 结果 |
|---|---|
| 默认固定语料 | 5/11，未提升 |
| hash 原始/错误/修复，使用原 manifest 已有标准 Path 前提 | 0/1/0，三项均符合原预期 |
| 同前提的错误→修复报告 | resolved |
| 移除或改变契约 | 比较不认定修复 |
| 过期源码/错误声明参数/重复契约/不支持的类型字段 | 输入错误 |
| 已解析内部调用 | 拒绝入口级特化，避免前提泄漏 |
| 未解析调用、回调、动态替换、分支类型冲突 | 保留未验证 |
| 全量 Rust 回归 | 375 项通过 |
| fmt / clippy -D warnings | 通过 |
| 原 Museon 源摘录与 11 变体哈希 | 保持不变并校验通过 |

没有把 Path 注解变为证明，没有增改原 manifest 前提。显式前提下的 hash 3/3 与默认完整语料5/11分开保存。

日志三项仍未验证。独立内存反例确认：标准 json 在字典子类上可触发 items 回调，该回调可在序列化期间关闭局部写入流。dict 注解不足以消除相应效果缺口。需要真实调用方数据来源和相关副作用证明。

证据：`reports/v2/declared-hash-scope/summary.json`、同目录各报告与 compare、`reports/v2/corpus.json`、`reports/v2/json-callback-boundary.json`、`tests/entry_contracts_v2.rs`。细节见 `docs/v2/ENTRY-CONTRACTS.md`。
