# hash-file：名义类型与可证明调用上下文

2026-09-13。结论：当前孤立 `_sha256(path: Path)` 根入口没有足够证据证明 `path.open` 的标准资源效果。继续增加标准库函数名模型不能消除这个边界。原有 11 个语料/预期不变，整体保持 5/11；没有把本次审计计为通过率提升。

## 语义依据与静态反例

Python 允许派生类覆盖基类方法：[官方类文档](https://docs.python.org/3/tutorial/classes.html#inheritance)。PosixPath 是 Path 的子类：[官方 pathlib 文档](https://docs.python.org/3/library/pathlib.html#pathlib.PosixPath)。据此，以下是对“Path 注解保证打开资源有效”的反例（未执行，仅静态检查）：

```python
from pathlib import PosixPath

class ClosedPath(PosixPath):
    def open(self, *args, **kwargs):
        stream = super().open(*args, **kwargs)
        stream.close()
        return stream
```

假设底层打开成功，open 返回的是已关闭的真实文件。将该 Path 子类实例传入 `_sha256`，后续 with 进入不能满足文件有效性要求。这是按 Python 语义推导的反例，不是已运行的 Museon 故障，也不是声称 Museon 实际使用了该子类。

## 静态回归证据

`tests/dispatch_boundary_v2.rs` 三个案例，目标 Python 均未执行：

| 分析入口 | 结果/断言 |
|---|---|
| 仅 `read(path: Path)` 函数根 | 无标准 Acquire 断言，保留 unverified |
| 同一函数由 `run()` 内的 `Path('input')` 调用 | 构造器事实传入，资源义务全部 verified |
| 加入覆盖 open 的 PosixPath 子类 | 不继承已验证工厂效果，保留 unverified |

全量 251 测试通过，fmt/clippy 通过。没有改动已安装分析器的语义或伪造运行时反例验证。最初把调用放在模块顶层会触发既有模块初始化未知；最终对照用普通函数 run 表达明确调用上下文，原被测函数不变。

## Museon 中下一步的证据入口

只读检查 `apps/render-service/render_service/media_io.py`：`_sha256` 在第 61、85 行被调用，第 40、71 行存在 Path 构造及 resolve。仅这些行不足以证明传入参数的全部来源；destination/source 的调用链和分支仍需解析，不能直接标成 exact Path。

下一批应从这些真实调用者追踪对象来源，同时建立 hashlib 的可审计效果模型；先新增独立的上下文集成测试，再检查原孤立根入口还有哪些合理前置条件未证明。保持原 manifest/hash/预期及开放参数诊断，不以新增上下文替换原测试刷高通过率。类型服务可以帮助找到声明与调用候选，但不能把名义类型提升为唯一运行时实现。

## 标准 hash 效果模型与上下文集成结果

新增按已解析身份选择的 sha256 空构造、update、hexdigest 模型，依据 [官方 hashlib 接口](https://docs.python.org/3/library/hashlib.html#hash-objects)。IR LibraryEffect 表达对已跟踪资源没有成功路径修改的模型事实；失败仍进入异常 CFG。hash 对象自身的内容改变不等于文件生命周期改变。未建模摘要正确性或密码学安全。

update 仅接受已知标准文件 read/readline 结果的 FileData 事实；普通注解、未知 buffer/转换、关键字/任意构造输入仍未验证。FileData 可能是标准 bytes 或 str：模型只证明资源效果，类型不兼容的失败路径仍保留，不声称所有输入能成功更新摘要。hashlib 标准导入列入模型环境；项目同名模块优先，不能误用标准库效果。

独立集成测试逐字包含原 original/mutated_error/repaired fixture，再添加函数内 `Path('input')` 调用上下文，没有改原 fixture/hash/预期。release 结果 original=0、mutated_error=1、repaired=0，compare=resolved；JSON 存于 reports/v2/hash-context-*.json。原语料统计仍 5/11。

全量 256 测试、fmt/clippy/release 通过，本机 llr 已更新，语义/环境版本为 11。真实调用链仍有类方法、Path 派生运算与分支合流缺口：engine.py:46 从临时目录构造 root，:76 派生 rendered，:92 传 publish，:131 调 materialize；这些定位仅是下一段来源证据，不是全链已验证。

## 派生路径上下文

新增 exact Path 的 `/`、parent、无参数 resolve 模型，语义依据 [pathlib 文档](https://docs.python.org/3/library/pathlib.html)。只传播已证明构造器来源：`root=Path('workspace').resolve(); output=root/'subdir'/'rendered.mp4'; _sha256(output.parent/'rendered.mp4')`。

`tests/path_flow_v2.rs` 用该上下文覆盖原 hash 三变体，并检查开放 Path 根、动态/bytes/f-string 操作数、未知路径构造协议保持不确定性。Path 构造器的协议输入也保守处理，普通标量注解不能代替精确对象来源。所有模型的失败沿异常 CFG 传播，没有实际 resolve 文件系统。

全量 260 项测试、fmt/clippy/release 通过，原固定语料依然 5/11，源/变体哈希验证通过。本机更新，语义/环境版本 12。类方法、动态字符串格式和复杂分支仍未覆盖；这组上下文测试不是实际 render-service 全链验证。
