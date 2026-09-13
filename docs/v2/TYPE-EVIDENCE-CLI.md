# 导入名义类型证据

llr analyze ROOT --entry MODULE::FUNCTION --type-evidence facts.json --format json

facts.json 为 {"schema_version":1,"facts":[{"path":"case.py","normalized":NORMALIZED}]}。path 必须是扫描根下已有源码的相对路径。NORMALIZED 是 scripts/type_facts.py normalize 的结果，包含 binding、status、candidates、reasons、dispatch 和可选 span。

CLI 用 SHA256 重新核对源码，以 UTF-16 query_range 重新计算字节跨度，拒绝源码漂移、未知文件、分割代理对、缺少绑定、反向范围和冒称 exact 的输入。候选保持 nominal_candidates_only。绑定中的提供者/stub/配置指纹是记录身份，不是独立验证这些依赖的证明；导入器不启动类型服务器，也不读取候选声明指向的文件。

JSON 增加 nominal_type_evidence；文本报告显示证据数量与名义限制。此证据目前只辅助诊断，不修改IR、候选调用目标、义务、退出码或 compare 的安全结论。compare 忽略这个信息字段；证明上下文仍为原有内部模型。旧证据无效时退出2，不输出新的成功报告。

这是证据入口的交付，尚未完成类型服务到调用解析的集成。下一步将声明位置映射至当前扫描的AST定义，并对动态替换、混合候选和未解析声明保留Unknown。不能把类型服务的单一名义候选当作唯一运行时实现。

## 映射扫描项目声明

bundle 可增加 documents 数组，每项为 {"uri":"file:///helper.py","path":"helper.py","document_sha256":"..."}。只绑定当前已扫描文件，不按URI读取外部文件。目标文件内容变化、重复URI或未扫描路径会被拒绝。

CLI 返回 nominal_declaration_matches，按 fact_index/candidate_index 对应输入。symbol 只在候选的名字和UTF16范围精确匹配已支持AST定义的名字节点或完整定义节点时填写；同一symbol重定义、装饰器或不支持的类均保留空值和原因。reason=scanned_definition_location_match_only 表示声明位置匹配，不表示运行时目标唯一。原有 unknown/混合候选状态不被清除。

当前映射仍是诊断证据，未改变IR或调用边。下一批次需一次贯通受控TSP归一化输出→CLI输入→声明映射→诊断定位，并测动态替换与混合候选；只有额外的静态唯一性依据才允许使用函数体效果。
