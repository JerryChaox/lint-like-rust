# V2 跨文件资源链

这些是用于验证分析链的合成案例，不是 Museon 现有 bug。扫描只读，不要执行例子。

`main.py` 创建文件，`resources.py` 中的 helper 关闭参数。项目级解析确定调用目标和参数类型；求解器把关闭效果传回原对象及其别名。

```sh
llr analyze examples/v2/before --entry 'main::run' --format json --output reports/v2/before.json
llr analyze examples/v2/after --entry 'main::run' --format json --output reports/v2/after.json
llr analyze examples/v2/unknown --entry 'main::run' --format json --output reports/v2/unknown.json
llr compare reports/v2/before.json reports/v2/after.json
llr compare reports/v2/before.json reports/v2/unknown.json
```

预期：before 退出 1，after 退出 0，unknown 退出 3；两次比较分别是 resolved 与 became_unverified。实际报告保存于 `reports/v2/`。0 只覆盖报告列出的义务及其正常调用路径假设，不能解释为所有异常路径或整个程序安全。

实际 Museon 语料见 `tests/corpus_v2/manifest.json`，其验收状态独立于这里的合成链。
