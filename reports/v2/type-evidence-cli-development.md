# 类型证据CLI入口验收

新增 src/type_evidence_v2.rs 与 analyze --type-evidence。读取既有Python归一化器的结构，核对源码SHA256、UTF16/字节范围和nominal边界，随JSON报告附上名义候选。不调用类型服务或执行扫描目标；不更改solver事实和语义revision19。

tests/type_evidence_v2.rs 四项覆盖中文/非BMP/CRLF、源码和范围漂移、伪造exact、缺失绑定、混合候选，以及实际CLI附证据仍Unknown、编辑后拒绝旧证据。完整测试与安装结果见本轮LoopX证据。类型提供者到AST声明/调用图映射仍待实现，固定语料5/11未改善。
