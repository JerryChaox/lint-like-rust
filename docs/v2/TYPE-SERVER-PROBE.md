# 结构化类型服务受控探针

2026-09-13：合成项目探针通过。**尚未接入 llr，也未对 Museon 启动类型服务**。Rust 回归数仍为 260，原语料仍 5/11；这次没有新增资源分析通过率。

## 重现

```sh
npm install --ignore-scripts --no-audit --no-fund --prefix /tmp/llr-tsp-runtime pyright-typeserver@1.1.414
python3 scripts/probe_type_server.py --node /opt/homebrew/bin/node --server-package /tmp/llr-tsp-runtime/node_modules/pyright-typeserver
```

来源：[Pyright Type Server 官方说明](https://github.com/microsoft/pyright/blob/main/docs/type-server.md)、[1.1.414 协议源码](https://github.com/microsoft/pyright/blob/1.1.414/packages/pyright-internal/src/typeServer/protocol/typeServerProtocol.ts)。包固定 1.1.414，协议握手 0.4.1；npm distribution integrity 为 `sha512-1x+EA91JZaQ8CMcon42LihoWsqajwW+aX8umrtgkupQ9pzXTFrtjoVlSPECNQpZeI3T+ejYY83MTib/YJDKH0w==`。

## 实际断言

| 检查 | 本次结果 |
|---|---|
| Path.open 结构化类型 | overloads 中声明指向 bundled typeshed/pathlib |
| 同名 Dialog.open | 声明指向合成 sample.py，不与 Path.open 混淆 |
| 跨模块 helper | 函数声明指向 helper.py |
| 中文及 emoji 前缀 | 按 UTF-16 单元定位后，取得正确方法声明 |
| 未知调用 | 明确的 unknown 类型 |
| 修改后的旧快照 | 错误 -32802，未作为类型事实使用 |
| 快照 | 本次有效查询 snapshot 5，修改后 7；数值不是固定协议常量 |
| Python/环境发现子进程 | 6 次 execFileSync 尝试被拦截，没有放行 |
| helper/启动代码执行 canary | 均未触发 |

JSON 证据在 `reports/v2/type-server-probe.json`，只包含合成源码与类型 stub 信息，临时目录前缀已替换。探针对期望声明、未知、旧快照及 canary 做实际断言，失败不会报成功。请求有总超时和有界快照重试。

## 执行边界

临时 cwd/HOME，最小环境，不加载 Museon 配置。Node preload 拦截 child_process 标准启动 API 并同步 ESM 导出；这验证了本固定服务走到的环境发现路径，不是针对恶意 Node 包的操作系统沙箱。正式后端仍必须保持受控 host、固定 stub/配置版本和“不执行目标 Python”的边界，不能移除 guard 后直接指向用户环境。

## 下一段接入要求

把线上的结构化结果规范化为提供者事实：来源 URI、UTF-16→字节 span、snapshot、提供者/stub 指纹、声明/调用候选、unknown 原因。响应内 TypeReference 需按各响应图解析，不能仅凭重复数字 id 跨查询复用。名义 Path 类型只产生候选，不产生 exact dispatch；效果仍由 llr 从已解析函数体或明确标准库模型推导。先通过协议 fixture/失效测试再接前端，最后对真实调用者做只读分析。

## 规范化接入进度

同日新增 `scripts/type_facts.py`，探针现在把每个实际响应转换为独立的候选事实，并断言三个已知调用的候选及 unknown 分流。每条记录绑定请求 range、URI、源码 SHA-256、请求时 snapshot、协议版本、provider JS/package 指纹、全部 bundled pyi 指纹及受控配置/guard 指纹；不再仅记录循环最后一个 snapshot。过期响应、身份不一致、部分未知 overload 均保留 unknown。TypeReference 只在当前响应图内查找，不跨请求复用 id。

`python3 scripts/test_type_facts.py` 验证身份变更、源码修改、中文/emoji/CRLF 字节定位、代理项内部非法位置、同名不同声明、响应内引用、未知 overload 和旧快照。线上受控探针也再次通过，执行 canary 未触发。

这仍是提供者接入边界，**不是 llr 的运行时后端**。输出固定标明 `nominal_candidates_only`，不直接生成 exact receiver 或资源效果证明；原语料仍 5/11。下一验收需把这些事实与类方法体/调用图连接，完成含关闭错误及修复的完整类方法场景，再接 Museon 真实调用者。
