# 所有权权限转移整批验收

revision20贯通语言无关权限域、Python内置内存IO模型、跨调用/字段/分支/异常传播、策略诊断与CLI修复对比。

examples/v2/ownership-transfer/before中，包装器取得管理权限后old.read触发OWN001；after把同一读取移到转移前，退出0且compare明确resolved；unknown加入未知调用后退出3，compare为became_unverified。报告位于reports/v2/ownership-transfer/。

tests/ownership_python_v2.rs覆盖旧别名、接收者、detach、关闭传播、普通赋值、重新绑定、多个候选的弱更新、互不相交资源、方法字段、跨模块返回、异常结果、未知缓冲区、标准库显式替换及策略诊断稳定修复。修复了显式属性替换后仍套用标准库模型的缺口。

scripts/probe_io_ownership_model.py仅在内存中验证标准库API语义，报告明确原buffer别名读取在Python中合法，因此OWN001属于附加所有权策略。未执行或修改Museon。

共享/独占借用、普通对象转移和其他流的模型仍未交付；固定语料覆盖没有增加。模型边界见docs/v2/OWNERSHIP-TRANSFER-MODEL.md。

最终验收：343个Rust测试通过，fmt/clippy通过，本地CLI重新构建安装；静态CLI整批1/0/3与compare复验通过。内存IO运行契约探针通过。固定Museon11项仍5/11，3原文及11变体哈希未变。LoopX公开边界检查通过。

最终补充回归区分失效文本包装器close与旧buffer close；标准库内存探针复核前者不关闭已取回buffer。with清理中的detach保留明确Unknown，避免错误传播关闭效果。
