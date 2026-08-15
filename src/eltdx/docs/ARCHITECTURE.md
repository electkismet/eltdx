# eltdx 3.0 架构说明

`eltdx 3.0` 仍是 Python 包，公开入口、业务 API、dataclass、Helpers、F10、MCP 和 CLI 保持 Python 形态。变化发生在包内部：完整 7709 wire 编解码和网络运行核心由 Rust 实现，不提供纯 Python 7709 fallback。

## 分层

| 层 | 目录 | 职责 |
| --- | --- | --- |
| Python 产品层 | `src/eltdx/` | `TdxClient`、业务 API、公开模型、异常、Helpers、F10、MCP、CLI |
| Python 兼容门面 | `src/eltdx/protocol/`、`transport/` | 保留公开 import、frame/snapshot 类型和同步 Transport 接口 |
| PyO3 边界 | `crates/eltdx-python` | ABI 校验、强类型请求转换、紧凑 DTO、错误映射、GIL/信号边界 |
| 协议核心 | `crates/eltdx-protocol` | 21 个命令、帧、增量解码、zlib、GBK、长度和内存限制 |
| 运行核心 | `crates/eltdx-runtime` | Engine、Supervisor、Slot、FIFO admission、pin、push、心跳、关闭 |

三个 Rust crate 的职责刻意分离。协议 crate 不依赖 Tokio 或 Python；runtime crate 不依赖 PyO3；Python crate 只做私有扩展 `eltdx._native`，不会向用户暴露新的业务 `PyClass`。

## 调用链

```text
TdxClient
  -> api/*
  -> SocketTransport / PooledSocketTransport Python facade
  -> eltdx._native.NativeEngine.execute(command, payload)
  -> Rust CommandRequest
  -> Pool Supervisor -> Slot task -> TcpStream
  -> Rust CommandResponse
  -> compact tuple/list/bytes DTO
  -> existing Python dataclass
```

`Transport.execute(command, payload)` 仍是公开动态边界，因此自定义 Transport 和 `InMemoryTransport` 不受影响。进入 native 后，payload 在一次 PyO3 调用内转换成 21 分支的强类型枚举；Rust 核心不传递 Python dict，也不使用 JSON 或 MessagePack 作为 FFI 中间格式。

公开 `eltdx.protocol` 继续提供 `RequestFrame`、`ResponseFrame`、`build_command_frame()`、`decode_response()` 和 `parse_command_response()`。这些函数全部委托无状态 Rust 入口，不创建 Engine，也不能组合成纯 Python backend。

## Engine 和 Slot

每个 `NativeEngine` 拥有一个后台 OS 线程和一个 Tokio current-thread runtime。没有进程全局 runtime，也不会为每个请求创建 runtime。

Supervisor 是以下状态的唯一写入者：Engine epoch、FIFO 等待队列、Slot lease、pin 归属、push buffer、terminal owner 和 diagnostics。每个 Slot task 独占一个 `TcpStream`、incremental decoder、endpoint cursor、TCP generation 和当前 wire request。Supervisor 不直接操作 socket，Slot 也不直接修改池状态。

`SocketTransport` 使用 `pool_size=1` 的同一 Engine；`PooledSocketTransport` 使用多个 Slot。显式 `connect()` 会并发连接并握手全部 Slot，任一失败都回滚未发布 epoch。未显式连接时，请求只懒启动实际分配到的 Slot。

每个响应必须同时匹配 engine epoch、Slot、TCP generation、request id、message id、message type 和发送边界。需要废弃连接时必须经过 `Retiring`，关闭 socket、清空 decoder 并递增 generation；旧 generation 的迟到事件不能完成新请求。

## Admission、deadline 和 retry

普通请求容量固定为 `pool_size + max_pending_requests`，但 active Slot lease 与 waiting permit 分开计数。入队前已经取得 waiting permit，因此 ingress channel 不会再隐式扩大容量。等待请求晋升时，在同一个 Supervisor mutation 中归还 permit 并取得 lease。

每个请求只有一个 monotonic absolute deadline，覆盖排队、连接、握手、发送、响应和最多一次安全重试。自定义 hostname 的首次标准库 DNS 是兼容例外：它在公开 timeout 和 Slot 外完成，发布 endpoint 前会重新检查 epoch 和 close 状态。

当前 21 个命令都在 manifest 中显式标记为 retry-safe，最多重试一次；未来命令默认不可重试。partial send 后的重试仍必须先完成旧 generation retirement，并继续使用原 deadline。

## Pin、heartbeat 和 push

`pin()` 与普通请求进入同一全局 FIFO。获得 pin 后，proxy 以 engine epoch、Slot、lease 和 pin id 绑定该 Slot；同一 proxy 的并发调用使用独立 pin-local FIFO，并共享全局 waiting permit 上限。proxy `close()` 只释放 pin，不关闭共享 Engine；pool close/reopen 后旧 proxy 永久失效。

心跳是低优先级内部请求，只在普通队列为空、Slot 空闲且未被 pin 时调度。它使用同一 deadline 和 generation 规则，不会覆盖更早的业务 fatal。

每个 Slot 单轮最多读取 256 KiB、路由 64 帧并生成 4 MiB 解压数据。decoded queue 上限为 1024 帧和 8 MiB；超限只 retirement 当前 generation。用户可见 push buffer 也同时限制帧数和 bytes，满时丢弃最旧帧并在下一次读取抛一次 `PushOverflowError`，随后仍可读取保留的新帧。

## 关闭、重开、信号和 fork

正常 `close()` 在线性化点停止 admission，终结等待 owner，retire 已触碰连接的 generation，清空 push，停止 Slot task 和 runtime，并 join 后回到 `STOPPED`。只有正常 `STOPPED` 可以 reopen。

公开关闭硬门为 1 秒。无法证明线程、socket、请求和 waiter 都已结束时抛 `TransportCloseTimeoutError` 并进入 `FAILED_CLOSING`；后续可再次 `close()` 完成清理，但最终只能到 `FAILED_CLOSED`，不能 reopen。

PyO3 网络等待每 25 ms 重新取得 GIL 并检查 Python signal。`KeyboardInterrupt` 保留为主异常，本地取消/retirement 失败通过 `__cause__` 暴露。Engine 在访问任何锁、channel 或 runtime handle 前检查 PID；fork 子进程不能复用父进程 Engine，必须创建新 client。

公开 diagnostics 继续使用 `ActorSnapshot` 和 `actors` 字段以保持兼容。这里的 `actor_alive` 表示 Rust Slot task/worker 是否存活，不表示 Python `ConnectionActor` 仍然存在。

## ABI 和打包

Python 期望 ABI 常量与 `_native.ABI_VERSION` 不一致时立即抛 `ImportError`。版本权威来源是 Cargo workspace；安装后的 `eltdx.__version__` 读取 distribution metadata。

首发产物是 Windows x64、manylinux x64/ARM64、macOS x64/ARM64 五个 `cp310-abi3` wheel 和一个 sdist。CPython 3.10 到 3.14 共用 ABI3 wheel。没有匹配 wheel 的平台需要 Rust 工具链从 sdist 构建，或继续使用最后一个 2.x；3.0 不提供纯 Python fallback。
