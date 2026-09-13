# 模块上下文开发验收

语义版本 v2-resource-cfg-13；独立开发示例 examples/v2/module-json。

| 版本 | CLI | 与错误版比较 |
|---|---:|---|
| 先关闭、再跨函数 json.load | 1 | LIFE001 |
| 在 with 内调用读取函数 | 0 | resolved |
| 带未建模 object_hook 的调用 | 3 | became_unverified |

共同上下文包含 import json 和 FILE_NAME 字面量模块配置。检查通过源自解析后的名字/参数/资源事实，不按函数名文本猜测副作用。模型仅承诺声明的标准环境、无替换的 json 默认实现与精确文件来源；所有权转移、任意 Python 对象协议与 JSON 内容正确性不在本证明范围。

自定义回调、未知文件参数、局部库遮蔽、open/json 名字覆盖、带调用/插值/注解的模块初始化和 __builtins__ 替换均有反例检查。异常 cleanup 有模型，但处理器类型信息不足时继续 unknown。

266 个 Rust 回归通过；已安装 CLI 的三个退出码及两次 compare 实测通过。固定语料保持 5/11，不把本开发示例计入独立效果评估。
