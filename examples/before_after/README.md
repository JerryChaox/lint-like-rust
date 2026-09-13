# 检查前后对比

这些是合成示例，不是 Museon 已确认的 bug。检查器不会执行示例；不要直接运行它们。修改由人工完成，llr 负责诊断和复查。

```sh
llr check examples/before_after/before.py
llr check examples/before_after/after.py
```

| 示例 | 诊断 | 分析缺口 | 退出码 |
|---|---:|---:|---:|
| before.py | 3（OWN001、BOR002、LIFE001） | 4 | 1 |
| after.py | 0 | 3 | 0 |

- 所有权：fd 交给 fdopen 后，读取改经文件包装器；这是额外的所有权策略。with 负责关闭包装器。
- 借用：遍历字典快照再删除原字典元素，避免改变正在遍历的字典大小。
- 生命周期：别名的读取移到资源关闭之前，with 负责清理。

修改后的三个缺口来自 os 模块身份解析以及 fdopen 构造异常结果，仍未形成全路径安全证明；strict 模式依然退出 1。
