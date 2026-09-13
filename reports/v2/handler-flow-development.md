# 异常处理器类型流开发验收

语义 v2-resource-cfg-14；安装后的 CLI 在 examples/v2/handler-flow 三版本上实测：

| 版本 | 退出码 | 比较结果 |
|---|---:|---|
| with 清理后，except 经 alias 读取原文件 | 1 | LIFE001 |
| 在 with 内处理读取异常，alias 读取后再清理 | 0 | resolved |
| except 调用未建模 custom(alias) | 3 | became_unverified |

该例验证资源使用范围，不声称两种异常处理范围在所有业务异常上完全等价：将 handler 移入 with 会改变 cleanup 异常是否被捕获。模型修复建议不能省略这一业务审查。

实现复用 AST 重绑定集合分析，在 try/handler/else 中没有重绑定的入口变量保留类型身份；控制流求解器继续决定对象是否已关闭。赋值/解构、导入、删除、with 别名、其他 handler 赋值会保守丢弃相关类型。未知调用仍污染资源证明。保留 File 类型从不等于保留 open 状态。

271 项 Rust 回归及 fmt/clippy/release 通过，CLI 重装完成；固定语料 5/11，来源哈希检查通过。上一版 json 异常测试的处理器类型缺口现在已变成明确 LIFE001。类方法及完整所有权/借用未完成，处理器中新建/重绑定的变量和项目调用符号仍有保守缺口。
