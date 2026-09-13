# 显式入口契约（revision22）

`llr analyze --entry-contract entry.json` 支持调用方声明独立函数入口的参数前提。检查结果以这些前提为条件；工具不证明调用方确实满足它们。默认分析不读取契约，也不会把 Path 注解或名义类型服务结果升级为精确运行时类型。

当前支持 `exact_stdlib_path`：参数是标准 pathlib 在当前平台使用的 Path 对象，排除自定义子类。契约可用于未加注解的函数；不修改被扫描 Python 源码。

```json
{
  "schema_version": 1,
  "entries": [{
    "path": "case.py",
    "source_sha256": "填写当前 case.py 原始字节的 SHA-256",
    "symbol": "case::_sha256",
    "parameters": [{"name": "path", "kind": "exact_stdlib_path"}]
  }]
}
```

路径相对于分析输入根；参数名必须对应已解析函数。当前只允许已选择入口的声明，不支持用此契约断言内部调用实参。当函数存在已解析的项目内部调用时拒绝契约，避免把入口假设用于其他调用上下文。无法解析的函数值调用仍为 Unknown，不能借用入口证明。

## 准入与诊断

- 契约绑定精确源码哈希、路径、唯一函数声明和参数名。过期哈希、重定义、重复参数/入口、未知类型和未知字段均为输入错误（退出码 2）。
- 契约影响类型事实和后续 CFG 降低，但不能消除未知回调、动态替换及分支类型冲突。
- 文本输出显示 caller-supplied assumption；JSON 保留 `caller_supplied_entry_contract`，`verification_basis` 为 `conditional_on_explicit_caller_assumptions`，每项义务列明前提。
- 配置指纹包含契约语义。加入、移除或更换契约的两份报告不能直接计为修复。
- 源码哈希用于准入，不进入语义配置指纹。修复后应重新绑定新的源码快照；前提相同且同一义务明确验证时才能是 `resolved`。

不要通过生成一个新的输入假设来“修复”真实调用代码。实际调用方未证明满足条件时，条件化报告不能代替默认扫描结果。

## Museon 固定 hash 语料

冻结 manifest 本来就声明 `path` 是标准 Path 并排除自定义子类。`scripts/evaluate_declared_hash_scope.py` 只把这一已有前提提供给工具：原始、注入错误、修复三个文件原字节和哈希不变，入口集合也不变。原始/修复退出 0，错误退出 1；修复比较为 resolved。

这组显式前提下是 3/3，默认 hash 仍是 0/3，默认完整语料仍为 5/11。结果分别保存，不能把显式前提验收称为默认覆盖率提升。

## 日志语料为何仍未验证

原 manifest 没有声明任意 `record: dict` 都不触发回调。标准 `json.dumps` 可以调用字典子类的 `items()`；自定义实现可以改变外部状态。独立内存反例 `scripts/probe_json_callback_boundary.py` 使用标准 json 和字典子类，在序列化期间关闭局部流，后续写入失败。这个探针不导入或执行 Museon/语料文件。

日志组仍需真实调用方的数据来源/嵌套值效果证明；不能额外给它塞入“纯 JSON 值”假设后宣称原目标通过。
