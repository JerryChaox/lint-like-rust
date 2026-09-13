# 缓冲区视图借用模型（revision21）

Python 前端将已解析的缓冲区视图 API 降为语言无关的 Borrow / EndBorrow。求解器在 CFG 上传播借用身份、共享/独占权限及 active/ended/unknown 状态，不依靠源码字符串匹配借用规则。

## 可以检查什么

- 普通赋值保留同一个视图和借用身份；任一别名释放后，其他别名不能继续访问。
- 共享借用可以共存，阻止底层写入、关闭、转移；独占借用阻止竞争访问。
- 派生借用保留父身份。子借用访问不与祖先本身冲突；共享子借用限制父视图写入，独占子借用限制父视图访问。
- 显式释放父视图不释放独立派生视图；所有相关视图释放后，底层所有者恢复访问。
- 借用通过直接字段、别名、函数参数/返回值和正常/异常 CFG 传播。分支合并保留可能性，重复分配站点和多候选释放不强行清除其他视图。
- 所有权转移和借用冲突相连；借用权限不能冒充所有者权限。

## Python API 范围

| Python | 语义 |
|---|---|
| 精确 `io.BytesIO()` / bytes 字面量构造后 `getbuffer()` | 创建独占视图 |
| `memoryview(b'...')` | 新建不可写底层数据的共享视图 |
| 已知视图 `.toreadonly()` | 新建共享派生视图，不改变原视图 |
| `.release()` | 结束该视图；重复释放幂等 |
| `.tobytes()` / `.tolist()` / `.hex()` 无参数 | 读取视图 |
| 整数常量索引读取/写入 | 检查读/写权限，保留操作失败分支 |
| `with` 已知视图 | 正常退出、返回和异常清理释放该视图 |
| `BytesIO.close()` | 活跃导出视图存在时不能关闭底层缓冲区 |

`memoryview.__enter__` 的失败边意味着该视图已经释放，使用显式状态假设传播到异常处理器；不凭空制造“有效视图进入失败却仍保持借用”的路径。该假设不是一次运行时 release 调用。

## 策略和 Python 运行时事实

`BORROW001` 的报告类别为 `safety_policy`。Python 允许持有可写视图时读取 BytesIO，也允许修改共享只读视图背后的原视图；本检查采用更严格的借用访问策略。释放后访问、只读视图写入、持有导出视图时关闭 BytesIO 同时受 Python 运行时限制。报告不能把全部策略限制称为 Python 语言错误。

诊断包含具体冲突类别、底层对象身份、借用创建/别名/释放证据和修复约束。证据是合并事实集合，不保证构成单条可执行路径。同一义务明确验证才算修复；变成 Unknown 或消失不算修复。

## 明确保留的范围外情况

切片、cast、多维缓冲区、自定义 buffer provider、C 扩展导出、未知副作用、动态替换及非精确来源仍未验证。循环中的多个视图可能合并为摘要；对具体视图的访问保守保留 Unknown。未实现 Python GC/析构时间推断或 Rust NLL，不因变量最后一次出现而自动结束借用；本策略要求显式 release 或已建模 with 清理。通用对象的完整所有权、泄漏自由及 Rust 等价安全保证均未交付。

## 证据

- `tests/loans_v2.rs`：语言无关借用、转移冲突、堆/分支/循环保守性。
- `tests/borrowing_python_v2.rs`：Python API、异常路径、字段/调用和修复协议。
- `scripts/verify_borrowing.py`：安装 CLI，before/after/unknown 及 compare。
- `scripts/probe_buffer_view_model.py`：只执行自建的内存标准库探针，不执行扫描的项目源码。

运行时依据：[BytesIO.getbuffer](https://docs.python.org/3/library/io.html#io.BytesIO.getbuffer)、[memoryview.toreadonly](https://docs.python.org/3/library/stdtypes.html#memoryview.toreadonly)、[memoryview.release](https://docs.python.org/3/library/stdtypes.html#memoryview.release)。
