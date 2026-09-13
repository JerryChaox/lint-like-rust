# Museon 静态扫描结果

时间：2026-09-13T00:46:11.111364+00:00；pyborrow 0.1.0。
目标：`/Users/jiaweichen/Documents/vscode/museon/museon`；HEAD：`3000a4d0c1fd898a557100b1391758a39d98a8fd`。

扫描文件：2585；耗时：91.698 秒；退出码：0。
诊断：0；分析缺口：556300；错误：0。
存在分析缺口的文件：2329；求解预算相关缺口：2060。这些不是覆盖率百分比。
Git 状态清单前后相同：True。检查器未执行或改写目标 Python 代码。

诊断是待人工复核的候选；分析缺口不是已经发现的 bug。无诊断不能解释为已证明安全。
配置中的借用约束属于额外策略，可能限制合法 Python 写法。

## 按规则统计

| 规则 | 数量 |
|---|---:|

## 最常见分析缺口

| 原因 | 数量 |
|---|---:|
| Keyword or variadic argument binding is not resolved | 96629 |
| Attribute or element identity is not resolved | 82504 |
| Unsupported expression: await | 23068 |
| Unsupported expression: not_operator | 11464 |
| Stored value identity is not tracked | 8778 |
| Unsupported expression: conditional_expression | 7665 |
| Unsupported expression: for_in_clause | 7546 |
| Unsupported expression: binary_operator | 7424 |
| Unresolved call: isinstance | 5843 |
| Object identity of `exc` is unresolved | 5636 |
| Unresolved call: row.get | 5584 |
| Exception dispatch and partial-expression failure states are conservatively approximated | 5181 |
| Object identity of `logger` is unresolved | 4649 |
| Object identity of `row` is unresolved | 4385 |
| Object identity of `item` is unresolved | 4159 |

## 诊断位置（前 30 条）


完整结果和实际命令见同目录 JSON 及 metadata JSON。
