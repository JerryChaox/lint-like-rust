# 普通无状态类调用链开发验收

v2-resource-cfg-15 已安装。examples/v2/class-flow 包含跨模块 Worker 类、实例别名、read → self.contents 调用链，以及 finish 的资源关闭效果。

| 版本 | analyze | compare |
|---|---:|---|
| finish 关闭后再 read | 1，LIFE001 | — |
| read 后再 finish，相同方法集合 | 0 | resolved |
| 构造后、打开资源前调用未知 external | 3 | became_unverified（compare exit 3） |

前端按 AST 收集限定方法身份和隐式 self 参数，复用既有 Invoke、异常边和资源求解器；没有按 read/finish 的方法名猜测效果。实例及文件别名经过调用链传递。

## 支持边界

仅接受无基类、无装饰器、无自定义构造器/dunder hook、无属性/字段的普通类，方法必须是唯一的普通同步函数。默认零参数构造的精确来源允许方法解析。跨模块导入和 self 调用可解析。

当前要求整个输入项目的效果图不含 Unknown，才启用此类精确调度。若任何函数（包括未调用方法）仍有未知效果，会保守退回不确定，并明确报告需要 closed project effect graph。这防止 helper 在文件获取之前替换方法却被遗漏；也意味着它尚不适用于有大量外部依赖的完整 Museon 项目。下一阶段需要用调用范围内的效果摘要缩小这个全局限制。

继承、property、__init__、方法替换、别名修改、未知外部效果和重复类绑定有反例检查。没有实现字段对象身份、完整所有权/借用，也没有接入外部 Type Server。

279 项 Rust 回归及 fmt/clippy/release 通过；安装 CLI 三版本及比较实测通过。固定 Museon 语料仍 5/11，3 个原片段与 11 个变体哈希不变；目标 Python 未执行。该开发验收不计入独立 AI 修复收益。

后续：revision16 已将全局限制缩小到调用连通分量，见 [调用范围验收](class-scope-development.md)。以上全局限制描述属于 revision15。
