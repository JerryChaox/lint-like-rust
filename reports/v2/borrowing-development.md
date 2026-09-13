# revision21 借用批次验收

已完成共享/独占借用模型到 Python 缓冲区视图及智能体诊断的连接，并更新本机 llr。

| 验收 | 结果 |
|---|---|
| 独占借用期间通过 owner 读取 | BORROW001，报告明确为 safety_policy |
| release 移到该访问之前 | 同一义务 resolved |
| 插入未知外部调用 | 未验证，compare 为 became_unverified |
| installed CLI before / after / unknown | 1 / 0 / 3 |
| 别名、共享派生、只读、字段、调用、分支、异常、循环摘要 | 24 项新增回归通过 |
| 全量 Rust 回归 | 367 通过 |
| fmt / clippy -D warnings | 通过 |
| 标准库内存协议探针 | 10 项通过，不执行扫描源码 |
| 旧所有权转移 CLI 验收 | 1 / 0 / 3 和 compare 保持通过 |
| 固定 Museon 语料 | 5/11；3 段源代码与 11 变体哈希校验通过 |

本次覆盖提升在新建的借用开发案例，不计入固定语料得分。当前借用机制保留同一视图身份、派生关系与 active/ended/unknown，沿 CFG 不动点传播。多候选或循环摘要采用保守释放，无法确定的效果不计为修复。

例子：

```python
raw = io.BytesIO(b'payload')
view = raw.getbuffer()
data = raw.read()  # BORROW001: competing exclusive loan is active
view.release()
```

将 `view.release()` 移到 `raw.read()` 前，同一检查义务得到验证。这里 owner 读取是更严格的借用策略限制，并非 Python 本身禁止。

未交付：任意 buffer provider、视图切片/cast/多维形状、自动推断 GC 或 NLL、通用对象所有权、递归效果摘要。完整规则和限制见 `docs/v2/BORROWING-MODEL.md`；后续工作返回固定语料中的输入来源及日志效果缺口。

机器证据：`reports/v2/borrowing/*.json`、`reports/v2/buffer-view-model-conformance.json`、`reports/v2/corpus.json`。
