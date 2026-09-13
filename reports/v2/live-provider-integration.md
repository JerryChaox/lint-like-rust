# 实时类型服务集成验收

固定pyright-typeserver 1.1.414 / TSP 0.4.1，在临时合成项目中完成真实查询→Python归一化→已安装llr→扫描AST声明映射→JSON诊断。命令：scripts/probe_type_server.py --node <node> --server-package <pinned-package> --llr <llr> --output reports/v2/live-provider-integration.json。

验收结果：
- Path.open的7个重载保持外部名义候选；不按名称授予文件模型。
- 同名Dialog.open映射为sample::Dialog.open，跨模块close_it映射为helper::close_it。
- 实时混合目标与被参数替换的目标均为Unknown。
- 无附加证据/附加证据均退出3，义务和gap逐项相同。附证据不是修复。
- 修改被引用声明文件、修改查询文件后均退出2。
- TSP旧snapshot查询被拒绝，新snapshot与旧snapshot不同。
- 六次分析器启动子进程尝试被guard阻止，目标与环境启动探针均未触发。

证据：reports/v2/live-provider-integration.json。未运行Museon、未读取其环境配置、未导入目标模块。

本集成批次已完成，但调用目标不因此变得运行时唯一，固定语料安全覆盖没有提高。停止追加仅展示候选的功能；下一批次回到权限转移与借用语义。类型服务的自动项目采集和名义候选效果分析仍需独立设计，不能宣称已经接入任意项目。
