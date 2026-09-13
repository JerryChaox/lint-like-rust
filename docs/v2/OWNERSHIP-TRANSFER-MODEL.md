# 权限转移模型（revision20）

资源身份与访问权限分开：资源记录open/closed/unknown；每个权限记录available/transferred/unknown并关联资源。赋值复制权限引用，转移撤销旧权限并为接收者创建新权限。已有别名继续指向旧权限，不因复制或字段读取恢复权限。

单一确定对象/权限可强更新；多个候选、循环分配摘要采用弱更新。分支合并取可能状态并集，旧权限在部分路径已转移时OWN001为possible。正常/异常调用结果都回写权限域，返回值保留实际权限。Unknown使可达权限不再可验证。借用、普通对象权限和原有无接收者Transfer仍保持未验证。

## 默认Python接入

无需源码标注。支持精确标准库io.BytesIO()或单个bytes字面量初始化；io.TextIOWrapper(buffer, 'utf-8')要求精确BytesIO来源和两个位置参数。成功构造为接收者转移管理权限；detach()再次转移到返回值；close传播到底层同一资源。其他流、codec、参数绑定及动态行为不套用此模型。

这是llr的管理所有权策略。Python允许经原buffer引用进行某些访问，OWN001不将其表述为Python语法或运行时必然错误。诊断class=safety_policy，修复约束要求使用当前权限、调整顺序或使用detach返回的权限。复制旧引用不能修复。

Python库依据：TextIOWrapper包装底层二进制流，detach返回底层buffer并使原文本包装器不可用：[io文档](https://docs.python.org/3/library/io.html#io.TextIOBase.detach)。关闭包装器同时关闭被包装流的行为由[Python官方问题记录](https://bugs.python.org/issue21363)确认。固定UTF8/内置buffer的初始化顺序参考[CPython实现](https://github.com/python/cpython/blob/v3.13.7/Modules/_io/textio.c)。本地标准库内存IO探针验证别名访问在Python中合法、close传播和detach身份；不执行扫描项目。

## 诊断与验证范围

含转移操作的程序在转移之前也建立权限检查义务，保证将访问前移后能匹配同一义务并明确验证。失效/未知权限不再获得资源仍可访问的肯定结论。报告提供资源取得、别名、调用、权限转移和违规访问的证据；这些是合并证据，不是声称存在单一路径的运行轨迹。

未解析属性写入、删除、增量赋值及动态setattr/delattr会保守禁用项目的标准库精确模型，避免显式替换后继续套用已知API。该处理可能影响不相关入口的精度，后续可在有证明的情况下缩小范围。

本批不包括任意Python对象move、共享/独占loan、通用用户契约加载、所有API的所有权推导、泄漏自由证明或任意运行环境的安全保证。固定Museon语料仍单独按11个原预期核验。

补充边界：已detach文本包装器的close不再关闭取回的buffer；旧buffer的close仍传播底层关闭效果。活动with清理期间detach暂时显式未验证，直到清理栈携带接收者协议信息，不能把它当作已支持的正常修复。
