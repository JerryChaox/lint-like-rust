# Python 语义前端复用决策

日期：2026-09-13。状态：受控合成项目 TSP 探针已通过；**尚未接入 llr 或对 Museon 启动服务**。证据与边界见 [TYPE-SERVER-PROBE.md](TYPE-SERVER-PROBE.md)。

## 决策

首选验证 **版本固定的 Pyright Type Server 进程适配器**，向 Rust 前端提供类型、声明和导入解析事实。保留 tree-sitter 负责语法与位置，llr 自己负责对象身份、控制流、函数效果与安全约束。不要解析 hover 的显示字符串，也不要把类型检查通过当作所有权检查通过。

先交付封闭、明确的 Python 子集内的完整跨函数资源链；外部语义提供者缺席时必须降低验证范围，不能声称已理解任意项目类型。不要为等类型服务而停止可独立实现的效果摘要、IR 和诊断工作，也不要继续无边界自建整个 Python 类型系统。

## 候选比较

| 方案 | 可复用内容 | 接入成本及缺口 | 选择 |
|---|---|---|---|
| Pyright Type Server | 结构化类型、声明位置、导入解析、分析快照 | Node 子进程；协议仍为 0.x；不提供我们的资源效果摘要 | 优先做受控探针 |
| basedpyright | Pyright 派生类型分析器及工具分发 | 本机版本未必包含新 TSP；不能假定 CLI JSON 导出语义图 | 保留备选，不强制替换用户安装 |
| ty 内部 Rust crate | 原生 Rust 类型推断、模块解析，可避免跨进程 | `ty_python_semantic` 自称内部组件，依赖大量 workspace crates；需绑定版本并维护内部 API | 后续性能／部署需求明确时再评估 |
| mypy | 成熟语义／类型分析，内部 BuildResult 可持有 AST 与类型映射 | 文档公开的 `mypy.api.run` 仅返回报告字符串；插件是类型扩展接口且无兼容保证；插件可执行 Python | 不作为首个适配器 |
| 自建 tree-sitter 前端 | 精确语法位置、自有控制流降级 | parser 没有现成项目类型系统，容易再次退化成名称猜测 | 保留语法层，只承诺明确子集 |

Pyright 和 basedpyright 的项目许可证为 MIT；ty 为 MIT，Ruff 仓库另有第三方声明；mypy 主体为 MIT，部分附属文件采用其他许可。真正打包依赖时仍须收集其对应版本及 typeshed 等附带资源的声明。依据：[Pyright](https://github.com/microsoft/pyright/blob/main/LICENSE.txt)、[basedpyright](https://github.com/DetachHead/basedpyright/blob/main/LICENSE.txt)、[ty](https://github.com/astral-sh/ty/blob/main/LICENSE)、[Ruff](https://github.com/astral-sh/ruff/blob/main/LICENSE)、[mypy](https://github.com/python/mypy/blob/master/LICENSE)。

## 已核实的 Pyright 接口

官方提供 `pyright-typeserver --stdio`，走 JSON-RPC；其类型求值与其他 Pyright 前端共用分析器。查询包含 computed/declared/expected type、import resolution、search paths 和 snapshot。它是直接结构化语义查询接口，并非通过编辑器 hover 间接猜测。[官方说明](https://github.com/microsoft/pyright/blob/main/docs/type-server.md)

协议 `FunctionType` 携带声明与签名；声明可带源码位置，因此存在将调用目标连接回项目函数体的基础。它不等于完整、唯一的运行时调用图；重载、联合类型、动态替换、装饰器仍需保留候选和未知分支。协议使用文档位置，适配器必须转换 UTF-16 位置与 Rust／tree-sitter 的字节位置，并固定内容快照。[协议源码](https://github.com/microsoft/pyright/blob/main/packages/pyright-internal/src/typeServer/protocol/typeServerProtocol.ts)

查询时的 npm 元数据实际存在 `pyright-typeserver@1.1.414`，解包大小约 19 MB；其 package 声明 Node >=14，实际部署应选受支持的 Node 版本。协议源码当前版本为 0.4.1，并明确提示版本协商不能发现同版本字段漂移。因此固定依赖和协议样本，不能只比较协议版本号。[包定义](https://github.com/microsoft/pyright/blob/main/packages/pyright-typeserver/package.json)、[发布元数据](https://registry.npmjs.org/pyright-typeserver/1.1.414)

本机只执行了分析器自身的版本命令：`basedpyright 1.39.3`，基于 `pyright 1.1.409`。没有安装新依赖，没有启动目标项目，没有进行运行时导入。

## 不执行目标代码的接入门槛

TSP 服务使用 `FullAccessHost`；该 host 的环境搜索可能启动 Python 解释器。不能把“静态检查器”直接等同于“不会触发环境初始化”。首次实验只用隔离测试目录，正式扫描须使用可控解释器和显式静态搜索路径，或提供不执行解释器的 host 适配；验证不会运行目标的 `.pth`、`sitecustomize` 或项目插件。不得自动安装或导入 Museon 依赖。[Server](https://github.com/microsoft/pyright/blob/main/packages/pyright-internal/src/typeServer/server.ts)、[FullAccessHost](https://github.com/microsoft/pyright/blob/main/packages/pyright-internal/src/common/fullAccessHost.ts)

mypy 的插件由导入机制加载，不可直接继承目标配置中的任意插件。公开 `api.run` 是 CLI 的编程封装，非稳定 typed-AST 导出协议；插件文档明确无向后兼容保证。[集成与插件文档](https://mypy.readthedocs.io/en/stable/extending_mypy.html)、[内部 BuildResult](https://github.com/python/mypy/blob/master/mypy/build.py)

ty 可复用 crate 路线在技术上成立，但其 manifest 标记为内部组件，直接嵌入意味着耦合其数据库、AST、模块解析和增量查询接口，不能只因我们使用 Rust 就当成最低成本。[crate manifest](https://github.com/astral-sh/ruff/blob/main/crates/ty_python_semantic/Cargo.toml)

## llr 与类型提供者的责任边界

建议边界事实协议表达：

- 文件内容 hash、提供者版本、配置／stub 版本和分析快照。
- 表达式的类型候选、精确程度、来源和无法解析的原因。
- 调用目标候选的模块、声明位置、参数对应关系、是否仍存在未知目标。
- 返回类型及绑定接收者；不要把类型相同误当成对象相同。

llr 自己计算函数对参数和字段的读取、修改、关闭、保存、返回别名和借用效果。类型信息只帮助找到定义并选择适用的外部契约。外部 `.pyi` 一般不表达 close／escape 等行为，类型服务不会自动补齐它们。

效果摘要必须从函数体组合产生，在调用图递归分量内求不动点。未知调用影响可达对象的哪些属性应由语义规范决定，不能默认 pure。标准库模型以解析后的语义身份和签名为键，并标注依据与适用版本。

## 首个贯通场景与验收

两模块 fixture：调用者用 `Path.open` 创建文件，别名交给另一个模块的 helper，helper 关闭文件，调用者随后读取；修复版把读取移动到关闭前。另加同名 `Dialog.open`、导入别名、局部遮蔽、未知装饰器、联合接收者作为反例。代码不需要新增所有权注解。

TSP 探针成功必须同时满足：

1. 得到 `Path.open` 的结构化类型和可用声明，并区别于同名用户方法。
2. 解析跨模块 helper 的声明，能映射回本地源码 span。
3. 文件修改后 snapshot 失效得到明确处理，中文及非 BMP 字符位置测试正确。
4. 未解析调用保留未知，不能凭函数名命中模型。
5. 不启动目标代码，不依赖目标环境初始化。

随后 llr 验收：从 helper 函数体推导关闭参数效果，跨调用传播同一资源身份，输出创建→别名→调用→关闭→违规读取的证据链；修复版得到相同分析范围内的通过，替换为动态调用则变为未验证，不能算修复成功。

若 TSP 无法提供所需调用目标，在适配器内部增加固定版本的小型导出补丁，或评估 mypy／ty 内部接口；不得用 hover 文本和方法末尾名字作为静默替代。此决策是下一步依赖实验的依据，不是已经完成语义覆盖的声明。
