# 类型声明位置映射验收

frontend_v2 复用统一的parse_sources，导出与现有前端接纳规则一致的定义索引。类型证据支持目标documents的URI/扫描路径/源码哈希绑定，映射候选的UTF16范围到实际AST定义。CLI JSON附带nominal_declaration_matches。

新增测试覆盖两个模块同名定义、目标编辑、重复URI、未知外部文件、错位range、装饰器和运行时同名重定义。映射只输出名义声明证据；未扩大任何安全证明范围或清除Unknown。320基础测试加4项新回归，语义revision19保持不变。

剩余：受控类型服务器输出尚未一次贯通到本CLI映射；调用图仍只使用原有静态确定性。下轮按完整集成批次验证，不再把单个格式/展示步骤作为交付目标。固定语料5/11未改善；原始目标、所有权和借用均未完成。

Python normalize输出→已安装CLI→helper::close声明映射已验证，仍退出3；证据 reports/v2/type-declaration-cli.json。输入为合成提供者响应，不是实时TSP查询。全量324测试、fmt、clippy、release安装与固定源哈希核验通过。
