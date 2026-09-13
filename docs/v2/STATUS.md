# V2 重写状态

## revision23：日志内置数据来源与效果链

精确字符串/递归内置JSON数据事实已连接日志操作。冻结日志正文在明确标注的开发调用方下为0/1/0/3；默认语料仍5/11，不混算。修复了显式入口复用外部调用方事实、未知条件求真遗漏效果的边界。Museon真实7个日志调用的datetime/状态来源仍未证明。见 [PLAIN-DATA-PROVENANCE.md](PLAIN-DATA-PROVENANCE.md)。

## revision22：显式入口前提与 hash 验收

入口契约已接入源码快照准入、前端事实、条件化报告和修复比较。冻结 hash 三变体在原 manifest 已有标准 Path 前提下为3/3；默认完整语料仍5/11，二者分开报告。日志仍需数据来源/回调效果证明。见 [ENTRY-CONTRACTS.md](ENTRY-CONTRACTS.md) 和 reports/v2/entry-contract-development.md。

## revision21：缓冲区视图借用批次

共享/独占借用身份、释放、冲突检查已贯通 CFG 和 Python BytesIO/memoryview 子集，含字段、调用、分支与异常传播。具体规则与边界见 [BORROWING-MODEL.md](BORROWING-MODEL.md)。以下各节为历史验收；当前固定 Museon 语料仍 5/11，通用对象所有权和任意 Python 借用推断未交付。

本轮交付是第一条可运行的资源分析链，**不是完整重写完成，也不是 Rust 等价安全证明**。旧原型保留在原模块和 `llr check` 中，新链通过 `llr analyze` 与 `llr compare` 使用。

## 最新验收：read_file 整批已通过

固定 Museon 语料现为 **5/11**，其中 read_file 五变体全部符合原有预期：原始/两种修复 verified_within_scope，两种注入错误 LIFE001，均无 gap。来源及变体哈希未变。全量 **248 项测试通过**，fmt/clippy/release 通过，本机 llr 已更新；以下较早批次的数字是历史记录。

本批连接异常类别（Exception 家族、其他 BaseException、未知）、有序匹配、正常/异常资源状态、标准文件失败分支与清理失败。return/raise/break/continue 清理沿 CFG 执行；清理失败替换退出原因并继续外层清理。with 进入检查资源有效性。未知匹配、自定义 context manager、动态文件参数转换与超过清理展开上限仍有 gap；finally 未实现。

compare 验证：样例 before→after 为 resolved，before→unknown 为 became_unverified。报告语义/环境版本升至 10，报告假设明确包含已建模正常和异常路径。

尚余 path_sha256 和 append_brain_log 六个目标；完整所有权/借用和智能体修复实验仍未完成。Path 注解不能证明精确动态类型，后续不能为了通过率信任子类方法或改动语料。

## 已实现

- 明确的语言无关 CFG：基本块、分支、返回、对象位置、资源读写关闭、调用及未知影响。
- 项目模块／导入／函数身份解析，有限类型事实传播；Path 和 File 的调用模型依赖已解析身份与类型，普通同名方法不自动匹配。Path/File 注解仅给出候选类型，不证明子类动态方法行为；构造器或已知调用参数可提供更精确事实。
- 源码重绑定、默认参数执行、模块初始化副作用及 coroutine/generator 延迟执行已有保守回归；复杂情况保持未验证。
- 可替换的语义事实入口；尚未接入 Pyright Type Server，当前提供者仅覆盖封闭子集。
- 资源别名、跨函数参数效果、返回别名／新资源、控制流状态合并与工作队列不动点。
- 调用通过上下文实例化执行分析，**尚不是缓存式符号效果摘要**；递归保持未验证。
- 未知效果污染依赖对象，字段投影／借用／转移等尚未实现的 V2 操作明确未验证。
- 稳定于部分位置变化的检查义务标识，跨文件支持证据、假设、修复约束。
- 前后报告比较：只有同一义务在可比上下文中得到明确验证才是 resolved；消失或变未知是 became_unverified。

证据合并来自多个控制流分支，当前是支持事实集合，**不是保证可执行的单条有序路径**。验证只针对列出的资源义务和已建模正常调用路径，不包括任意异常、泄漏自由或整个项目安全。

## 本轮实际验收

完整回归 195 项通过，其中 V2 新增 64 项（前端 26、求解器 17、诊断协议 13、报告适配 5、CLI 3），旧原型 131 项保留。`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 release 构建通过。测试数不代表真实项目覆盖率。


两模块样例 `examples/v2/`：Path.open 创建资源，helper 关闭资源，调用者别名读取。

| 输入 | analyze 退出码 | 比较结果 |
|---|---:|---|
| before | 1：LIFE001 | 原始违规 |
| after：读取移到关闭前 | 0：列出义务已验证 | resolved |
| unknown：替换为未知外部调用 | 3：未验证 | became_unverified |

Museon 原始／变异／修复语料的完整结果在 [验收报告](../../reports/v2/corpus.md)。首次验收 2/11 目标通过：能发现 `_read_file` 两种注入错误；其余目标因异常、循环、Path 运算等缺口未通过。不存在“11 个案例全部有效”的结论，也没有修改 Museon。

## 使用

```sh
llr analyze examples/v2/before --entry 'main::run' --format json --output reports/v2/before.json
llr analyze examples/v2/after --entry 'main::run' --format json --output reports/v2/after.json
llr compare reports/v2/before.json reports/v2/after.json
python3 scripts/evaluate_corpus_v2.py
```

新命令退出码：0 = 列出义务在声明范围内已验证；1 = 存在违规；2 = 工具／输入错误；3 = 未验证或无检查义务。`compare` 的 0 仅表示所跟踪违规满足比较条件，不代表整个代码库安全。

`analyze` 接收显式项目根，尊重 gitignore；可用 `--entry 'module::function'` 限定入口。不读取旧原型的用户契约／规则选择／抑制配置，报告上下文明确使用默认实验语义。报告路径均相对输入根；比较不同目录副本时应保持文件相对路径和入口一致。

## 下一个阻塞里程碑

1. 先让三组 Museon 原始／变异／修复语料达到规范要求；不以补一个函数名作为完成标准。
2. 验证外部类型事实适配器，包含导入、声明、方法候选、快照与不执行目标初始化的约束。
3. 补足异常 CFG、字段身份与可组合摘要，随后按语义规范实现权限转移和借用。
4. 最后开展真实智能体修复实验；本轮尚未声称测得 AI 修复成功率。

## LoopX 接续：2026-09-13

已接入目标 `lint-like-rust-goal`、执行身份 `llr-rewrite`，并为当前会话启用 heartbeat。任务、证据与下一步保存在项目本地 LoopX 状态中，后续运行按 quota 决定是否执行，不依赖聊天是否仍在当前轮。

本段新增源代码 while/else、break/continue CFG 降低：条件在回边重新求值，break 跳过 else，continue 不执行后续语句，只清理循环内新进入的 with。循环中可能重绑定的名字先丢弃精确类型；else 分支也不能恢复陈旧的 Path 类型。未知条件的布尔转换保留效果不确定性。新增 9 项回归，全量 204 项测试通过，Clippy 和 release 构建通过。

分析语义与前端上下文版本已更新，避免旧报告与新语义错误地比较为已修复。`iter(callable, sentinel)` 尚未支持，Museon 11 项验收仍为 2/11；没有修改原始语料或预期。当前 P0 保持 open，下一段接入延迟调用迭代与类型事实，再重跑同一语料。

### 延迟迭代调用接续

新增直接 `for x in iter(lambda: expression, literal_sentinel)` 的有界降低，要求解析为内建 iter、无参数 lambda、普通标识符目标与无插值的字面量 sentinel。lambda 的创建不执行函数体；每次 next 的回调在循环头执行。复用 while 的循环体、回边、break/continue、else 和上下文清理逻辑；循环目标及体内重绑定会使捕获名字的精确类型失效。

sentinel 比较只在已解析标准 File.read/readline 且参数为整数字面量时采用纯比较假设；其他返回类型即使带 Scalar 注解也保留比较效果未知。任意 iterable、保存后再消费的迭代器、带参数/default 的 lambda、动态 sentinel、异步迭代仍未建模。未知路径不会视为修复。普通 lambda 创建也已修正为不执行其函数体，默认值仍在创建时分析。

新增 8 项回归，全量 **212 项通过**，fmt/clippy/release 通过。语义版本为 `v2-resource-cfg-3`，前端环境为 `python-closed-subset-3`。同一组 Museon 来源哈希与 11 个案例保持不变，验收仍为 **2/11**。真实 hash-file 案例现在能进入迭代回调分析，但仍受 Path 子类方法身份、hashlib 模块初始化和调用效果、算术参数及异常 CFG 缺口阻塞。缺口数不能直接视为进步指标：深入分析会暴露更多具体未知。

下一步：为这些缺口建立可审计的类型/标准库效果事实边界，优先处理已有 P0 异常 CFG；不靠信任 Path 注解或忽略未知调用提升语料通过率。完整所有权/借用与智能体修复评测仍未完成。

### 字面量事实与格式化副作用接续

为 `1024 * 1024` 等递归整数字面量表达式增加 AST 推导的精确整数事实（加减乘、位运算、一元正负/取反）。不执行目标表达式，不信任 int/Scalar 注解或动态操作数，不覆盖除法、幂和移位。标准文件迭代回调的参数纯度使用同一事实函数。另修复 f-string 将插值中的资源调用遗漏的问题：分析插值求值，格式转换本身保持未知。

4 项新增回归，全量 **216 项测试通过**，fmt/clippy/release 通过。语义/环境版本升至 4。Museon 原始与变异来源校验通过，验收仍 **2/11**；不变的通过率要求切到剩余 P0 异常 CFG，而非持续扩展小型表达式子集。迭代器 todo 保留 open，解除当前领取；异常 CFG todo 由同一执行身份领取，避免后续 quota 再选中已交付的前置步骤。

### 显式异常控制流基础

引入前端异常目标栈：无 finally 的单一裸 `except:` 可接收保护区内的显式 raise；处理器再次抛出与 else 抛出使用外层目标。跳转只清理保护区内新增的 with；处理器入口与汇合点丢弃未证明的类型/名称事实，防止从分支重新恢复被遮蔽的 builtins 身份。新增 7 项回归，全量 **223 项通过**，fmt/clippy/release 通过，报告语义/环境版本升至 5。

这只交付显式异常边基础。隐式异常边仍是可见缺口；带类型处理器和 finally 保持原有未验证行为。未声称无显式 raise 就能证明处理器不可达。Museon 3 份来源与 11 个变体哈希校验通过，验收仍为 **2/11**。下一段必须处理可能抛出的操作及异常分派语义，不能仅消除现有 Unknown 标记；跨函数调用也需要区分正常返回和异常退出摘要。当前异常 P0 保持领取和 open。

### 跨函数异常退出保守传播

求解器现在区分正常返回结果与 may_raise：只有异常退出的已解析调用不再继续分析调用后的正常基本块；同时存在正常和异常退出时，调用保持未验证，不能因为正常分支可分析就掩盖异常缺口。异常标记跨多层调用传播。新增 3 项 IR 和 2 项 Python 前端回归，全量 **228 项通过**，fmt/clippy/release 通过；语义/环境版本升至 6。本机安装更新。

目前只传播异常存在性，尚未把异常资源状态送到调用者 handler，因而 CALL_EFFECT 明确未验证。没有声称 handler 不会执行。Museon 来源/变体校验通过，验收仍 **2/11**。

下一段应推进成体系的 Invoke/异常后继接口：分别返回正常与异常资源状态，由前端把调用点连接到最近 handler/cleanup；再接入可能抛出的文件操作与异常类型分派。仅增加 may_raise 标记不是异常系统完成；不得移除现有缺口来提升通过率。

### Invoke 异常后继与状态传递

语言无关 IR 新增 Invoke：正常继续与 unwind 后继分开，返回目标只在正常完成时赋值。求解器分开汇合正常/异常资源状态，再用异常资源状态和调用者原有局部绑定进入 handler。Python 前端为裸 except 保护区内已解析的项目调用生成 Invoke 和内层 with 清理块。未设置异常后继的旧 Call 仍保留覆盖缺口。

新增 5 项回归验证关闭资源传入 handler、正常/异常状态隔离、不覆盖异常路径返回目标、非法后继、源代码 handler 可达与清理次数。全量 **233 项通过**，fmt/clippy/release 通过，语义/环境版本为 7。本机 llr 更新。Museon 来源/变体哈希校验通过，验收仍 **2/11**。

仍未建模：文件操作等隐式异常、异常类型匹配、finally、未解析调用的异常效果。下一段把可能抛出的标准文件操作接入同一异常后继接口，并验证资源创建失败不能产生已创建资源；然后处理标准 Exception 分派。不能把当前 Invoke 子集等同于完整异常检查。

### 标准文件操作失败边

裸 except 保护区内的文件打开、读取、写入与关闭现在具有失败分支，复用 Invoke 的清理块构造。打开失败在 Acquire/赋值之前分叉；读写失败携带已有资源状态；关闭失败同时允许未关闭和已关闭状态到达 handler。没有修改目标代码，也没有执行目标 Python。

新增 3 项回归（覆盖打开、读、写、关闭与失败前绑定），全量 **236 项通过**，fmt/clippy/release 通过，语义/环境版本为 8。Museon 来源与 11 变体校验通过，验收仍 **2/11**。

边界：目前只在已建模保护区内接失败边，未覆盖任意异常和 cleanup 失败。带类型 except、finally、未知调用仍有明确缺口。下一段必须接异常类型分派与 with 的异常清理语义；不能删除 try/with 的未知诊断来制造通过率。真实 read_file 使用 except Exception，仍未达到验收。

### 有序类型处理器分派骨架

多个 except 以及带类型的 except 现在按源码顺序降低为匹配/不匹配分支；未匹配异常进入外层保护区。缺少运行时异常类型事实，因此匹配本身仍有明确 Unknown，不能宣称精确匹配或整个函数已验证。处理器体已实际分析；异常别名和头部名称保守失效，避免 `except Exception as open` 恢复内建打开函数身份。

新增 4 项回归，全量 **240 项通过**，fmt/clippy/release 通过，语义/环境版本为 9。Museon 源码/变体哈希校验通过，验收保持 **2/11**。真实 read_file 的 Exception 处理器现在进入 CFG，但类型匹配、with 清理异常等仍阻止安全案例验收。

下一步需引入异常类别事实并检验传播/匹配，与清理失败语义一起消除有证据支持的缺口；当前 nondeterministic 分派仅是保守控制流覆盖，不是异常类型分析完成。完整所有权/借用仍未交付。

### hash 上下文集成批次

原 11 项验收保持 **5/11**。新增独立上下文集成已验证 hash 原始/注入错误/修复三变体，compare 确认为 resolved；不将这些附加上下文计入旧验收分母。标准 hash 模型与边界详见 [HASH-BOUNDARY.md](HASH-BOUNDARY.md)。全量 **256 测试通过**，fmt/clippy/release 通过，本机更新，语义/环境版本为 11。

已补标准模块与项目同名模块的来源区分。下一段需处理真实调用链的类方法、Path 派生与分支来源，并保留开放 Path 参数的未验证状态；没有声称 Museon 整个 render-service 已通过。

### 派生 Path 传播验证

exact Path 的连接、parent、无参数 resolve 可在已知构造器上下文传播到 hash 函数；未知协议输入仍有 gap。四项新增测试覆盖完整 hash 三变体与反例，全量 **260 项通过**，fmt/clippy/release 通过，本机更新，语义/环境版本 12。原 11 项依然 **5/11**，未改预期；下一步仍需类方法和复杂分支的来源解析。

## 模块常量与默认 JSON 文件读取（2026-09-13，语义 revision 13）

已交付独立开发场景 examples/v2/module-json：安装后的 llr 在错误/修复/自定义回调未知版本上分别 exit 1/0/3，compare 分别 resolved / became_unverified。不是把未知诊断消失当成修复。

前端把无调用、无插值、无注解求值的简单字面量全局赋值视为无执行效果；名字仍参与 shadow 检查，__builtins__ 重绑定不在此支持范围。标准 json 导入加入受控环境模型；精确标准文件上的单参数默认 json.load 被建模为可能抛错的 Read 和默认解码效果。custom decoder/hooks、未知文件协议、项目 json 模块遮蔽、动态初始化仍不会被当作纯标准库调用。

回归 266 项、fmt/clippy/release 均通过，CLI 已重装。固定 Museon 语料仍 5/11，3 个原片段及 11 个变体哈希检查通过，目标代码未执行或修改。未对已曝光的探索题重复打分。异常处理器仍可能丢失文件变量类型并产生 unknown；类方法分析及完整所有权/借用依旧未完成。

## 异常处理器稳定类型流（2026-09-13，revision 14）

已复用 AST 重绑定集合计算，保留 try/handler/else 中未改变绑定的入口类型，并由 CFG 独立跟踪资源关闭状态。examples/v2/handler-flow 的安装 CLI 错误/修复/未知结果为 1/0/3，compare 为 resolved / became_unverified。上一版 JSON 异常处理器 f.read 不再因无谓类型丢失而 unknown。

271 项回归与 fmt/clippy/release 通过，安装完成；固定语料仍 5/11，未对已曝光先导重打分。处理器重绑定保持保守，类方法/项目符号跨 handler 解析及完整所有权仍待做。示例调整异常捕获范围仅用于资源安全验收，不保证业务异常语义等价，见 reports/v2/handler-flow-development.md。

## 普通无状态类调用（2026-09-13，revision 15）

已接通 AST 限定方法集合、精确默认构造实例、隐式 self 实参、实例别名、self 调用及跨模块方法 Invoke。examples/v2/class-flow 的已安装 CLI 前/后/未知为 1/0/3，比较为 resolved / became_unverified。279 项回归和必要检查通过，固定语料仍 5/11。

精确类调度目前要求整个项目效果图闭合，任意 Unknown 会禁用；这保证方法替换等未知效果不会被误当稳定调度，但也会被无关函数的未知阻止。仅无状态、无基类/装饰器/构造 hook 的普通类受支持。实际有字段的 Museon 类、类型服务整合和完整所有权未完成。下一步改为有证据的调用范围效果摘要，不能直接移除全局保护；细节见 reports/v2/class-flow-development.md。

## 2026-09-13 revision16：调用范围效果传播

类调度改为 Call/Invoke 双向连通分量内的 Unknown 传播；独立未知函数不再污染选定入口，未知调用者在获取资源前替换方法的反例仍被阻止。安装 CLI before/after/unknown 为 1/0/3，比较 resolved/became_unverified。报告明确排除未建模的先前入口调用历史。284 Rust 测试及 fmt/clippy/release 通过，固定语料仍 5/11。详见 reports/v2/class-scope-development.md；字段、构造器、所有权/借用未完成。

## 2026-09-13 revision17：普通构造器效果

无状态普通类 __init__ 通过 Invoke 建模参数、self 调用、正常/异常效果；实例表达式不被初始化器的 None 返回覆盖。安装 CLI 构造器关闭/修复/未知为1/0/3，compare为resolved/became_unverified。292测试与fmt/clippy/release通过；固定语料仍5/11，字段身份与完整所有权未完成。详见 reports/v2/constructor-flow-development.md。

## 2026-09-13 revision18：堆对象与字段身份

语言无关 AllocateObject 和对象字段堆图已接通；普通类构造现在分配对象身份。核心支持字段别名、强弱更新、嵌套投影、跨调用正常/异常堆写回与传递Unknown污染。Python字段语法仍未开放，下一步是直接数据字段访问证明与字段类型事实。详见 reports/v2/heap-field-development.md；真实语料和完整所有权验收未完成。

## revision19：Python 直接字段连接堆图

普通类构造器存储、方法读取/关闭、实例别名和字段重绑定已接通。311测试、fmt/clippy和已安装CLI开发案例1/0/3通过；compare只认明确验证的修复，未知仍became_unverified。固定语料5/11未变。证据：reports/v2/python-field-development.md。下一步补跨模块/嵌套/类型冲突边界组合，再推进真实语料整组验收。例行进度仅同步Base TODO与里程碑。

## revision20：权限转移贯通Python与智能体诊断

资源与权限分离，BytesIO/TextIOWrapper/detach管理策略已接通；旧别名、调用、字段、分支和异常传播有验收。OWN001明确为safety_policy；同义务修复compare为resolved，Unknown不计修复。模型与局限见OWNERSHIP-TRANSFER-MODEL.md，开发证据见reports/v2/ownership-transfer-development.md。共享/独占借用仍待实现，固定语料继续5/11单独记录。
