# eltdx 7709 单线程非阻塞 Actor 改造方案

> 状态：实施规范（authoritative implementation spec）
> 适用仓库：`<repository-root>`
> 形成方案时的基准提交：`71089c0a2867a75dc79aa2c340213f4e3845b6e3`
> 目标分支：`actor-transport-refactor`
> 规范版本：1.4
> 最后更新：2026-07-19

本文档是 7709 transport Actor 改造的唯一实现规范。实现线程在开始工作、上下文压缩后恢复、网络中断后恢复以及最终验收前，都必须完整重读本文档，不能仅依赖聊天摘要或记忆。

版本 1.1 经用户明确选择，保留第 17 节 strict FIFO，不用旧锁 barging
换取较低的饱和 p50。首次 exact-`dcf6190` A-B-B-A 仍按 1.0 口径记录为
FAIL；1.1 只前瞻定义新的 fixed-cohort 调度开销门，不能追溯改判或删除
旧 artifact。

版本 1.2 经用户在完整 correctness、stress/resource、CI 和多轮冻结性能
证据之后明确选择：继续保持 Actor 独占 TCP socket、strict FIFO、同步 API
以及 `pool_size=N` 恰好 N 个 Actor/N 条连接，不用 caller 直接发送换取旧版
吞吐。所有 95% throughput 和 10%/0.2ms latency campaign 仍按运行时口径
保留为 **FAIL**，不得删除、重采、追溯改判为 PASS 或用其他 quantile 抵消。
用户批准这些旧实现对比门作为本次交付的架构性性能例外；它们从 1.2 起
不再阻挡本次 FINAL，但原始数据、失败项、差值、artifact hash 和例外理由
必须进入永久 result manifest。服务端并发=N、heartbeat、idle CPU、close、
唯一响应和资源归零仍是硬门。后续性能敏感变更以本次 FINAL Actor 实现为
新基线，并在采样前另行冻结回归门，不再把 `71089c0` 作为唯一完成判据。

版本 1.3 只统一本次 correction cycle 的恢复和 FINAL 协议，不改变 1.2
的用户授权、实现边界、性能 FAIL、硬门或完成政策。第 22 节 A00-A09
保留为原始实施历史；`994c49b` 之后的纠错使用 F00-F06/FINAL、临时账本
`ACTOR_REFACTOR_FIX_PROGRESS.md` 和 `Fix-Checkpoint:` trailer。凡后文章节
仍提到旧 `ACTOR_REFACTOR_PROGRESS.md`、`Actor-Checkpoint:` 或把 A00-A09
作为当前唯一恢复入口，均由本 1.3 correction 协议覆盖。

版本 1.4 只固化 F10 收尾的程序性规则：两个已确认竞态必须先以
精确暂停 stale read/epoch publication 的确定性 RED 测试证明，然后分阶段
进行最小生产修复和 heartbeat 测量边界修复。本版本不改变 Actor
架构、1.2 性能例外、硬门、公开接口或发布范围。revision 1.4 必须
先作为单独 checkpoint 提交并冻结；后续 Result 引用该提交中 Plan
blob 的准确 SHA256，不得再改写 Plan 造成 spec identity 漂移。

---

## 1. 背景与现状

当前 `SocketTransport` 使用业务线程、reader 线程和 heartbeat 线程共同管理一个可变 socket：

- `execute()` 持有 `_lock`，发送请求并等待 reader 投递响应。
- reader 通过动态的 `self._socket` 持续 `recv()`。
- heartbeat 调用公共 `execute()`，具备重连能力。
- 重连会重新 `clear()` 共享 stop Event。
- `close()` 在 `join(timeout=0.2)` 后无条件清空线程引用。
- pool 使用盲目 round-robin，`pin()` 并不真正独占连接槽位。
- push queue 无界。
- `read_exact()` 的半包缓冲是函数局部变量，timeout 后会丢失已读取字节。

这会产生以下已确认风险：

1. 旧 reader 没退出时，新 reader 已建立，两个线程可能同时读取新 socket。
2. 旧 heartbeat 等待锁期间被丢失引用，随后在共享 Event 被重新清除后继续存活。
3. 旧 reader 的异常可能失败新连接的 pending 请求。
4. `close()` 与 heartbeat、execute、reconnect 之间存在锁顺序和代际竞态。
5. 超时重连路径可以持续复制线程和 TCP 连接。
6. 半包 timeout 后可能造成协议流失步。
7. pool 某个槽位连接失败时，之前已连接的槽位没有回滚。
8. 未显式关闭的 transport 会被 bound thread target 强引用，无法由 GC 回收。

本次改造不继续修补旧 reader/heartbeat 模型，而是用每连接槽位一个完整的非阻塞 Actor 替换它。

---

## 2. 目标

### 2.1 功能目标

- 每个连接槽位最多一个 `ConnectionActor` 线程。
- Actor 是网络 socket 的唯一所有者。
- 内部重连只替换 socket，不替换 Actor 线程。
- `pool_size=N` 时最多 N 个网络请求同时在途。
- 保留同步 `TdxClient`、`SocketTransport` 和 `PooledSocketTransport` API。
- 保留首次业务请求前自动握手的现有行为。
- 保留主动请求、未匹配 push、后台 heartbeat、自动重连和一次重试能力。
- 修复半包、粘包、迟到响应、close/reopen、pool rollback 和 `pin()` 独占问题。
- 为任务、push、接收缓冲和连接资源建立硬上限。
- 显式 `close()` 后确定性释放 Actor、TCP socket、wakeup socket、selector 和所有 ticket。
- 忘记 `close()` 时，finalizer 可以尽力唤醒 Actor 退出，但不把 GC 当作确定性资源管理。

### 2.2 稳定性目标

- 任意内部重连次数下，线程数不增长。
- 旧 TCP generation 的事件、响应和错误永远不能作用于新 generation。
- 所有请求恰好完成一次：成功、规定异常或取消，不能永久悬挂。
- close 可以中断 connecting、partial send、waiting response 和 idle selector。
- Actor fatal error 必须 fail-closed，不能静默补线程。
- 过载必须背压或显式失败，不能无限排队。

### 2.3 性能目标

- `pool_size=1` 的正常顺序请求吞吐相对旧实现 95% 仍作为完整披露目标；
  本次交付按版本 1.2 的用户批准例外处理，不伪报为达标。
- `pool_size=N` 时，服务端观测到的最大业务并发严格为 N。
- pool 调度不把请求继续排到慢 slot 后面。
- 业务 payload parser 不在 Actor 内执行。
- 持续业务流量下 heartbeat 对吞吐的影响低于 1%。
- idle Actor 由 selector 阻塞，不允许轮询空转。

---

## 3. 非目标

本次改造明确不做：

- 不实现固定 10 worker。
- 不实现进程级全局 WorkerPool。
- 不引入 `asyncio`。
- 不改变 7709 二进制命令和业务模型。
- 不允许单 socket 多个业务请求同时在途。
- 不实现协议层多路复用。
- 不新增第三方运行时依赖。
- 不修改 7615 F10 transport。
- 不重构 Helpers、业务 API 或协议命令解析之外的代码。
- 不在本任务中发布 PyPI、创建 tag 或合并到 `main`。

---

## 4. 总体架构

```text
并发调用线程
    |
    v
PooledSocketTransport FIFO admission
    |
    v
空闲 SlotLease
    |
    v
SocketTransport 同步 facade
    |
    v
ConnectionActor（每 slot 一个）
    |
    +-- wakeup socketpair
    +-- selector
    +-- TCP generation
    +-- 单个 active RequestTicket
    +-- heartbeat timer
    +-- incremental FrameDecoder
    |
    v
不可变 FrameEnvelope
    |
    +-- Actor 清 active
    +-- exact-once 释放普通 lease
    +-- 唤醒 caller
    |
    v
caller 执行 parse_command_response()
```

Actor 只负责 wire-level 工作：连接、发送、收包、帧边界、安全解压、握手、heartbeat、响应路由、timeout 和重连。业务 payload 到模型的解析在调用线程完成。

---

## 5. 核心对象

### 5.1 `SocketTransport`

保留现有公开类名和方法，改为同步 facade：

- `connect()`：确保 runtime 存在，提交 connect 控制请求并等待。
- `execute()`：建立端到端 deadline，提交请求并等待不可变 `FrameEnvelope`，随后在 caller 解析。
- `close()`：原子停止 admission，唤醒 Actor，锁外等待退出。
- `poll_push()` / `drain_pushes()`：从注入的 `PushBuffer` 读取，在 caller 解析。
- `connected_host`、`last_handshake`、`last_heartbeat`：读取 runtime 发布的不可变 snapshot。

`SocketTransport` 不再持有：

- `_socket`
- `_reader_thread`
- `_heartbeat_thread`
- `_stop_reader`
- `_stop_heartbeat`
- `_pending`
- `_send_lock`

### 5.2 `ActorRuntime`

一次 Actor 从启动到退出的完整生命周期：

```python
ActorRuntime(
    runtime_epoch: int,
    state: RuntimeState,
    actor_thread: Thread,
    selector: BaseSelector,
    wake_reader: socket.socket,
    wake_writer: socket.socket,
    control_lock: Lock,
    stop_requested: bool,
    pending_task: RequestTicket | ConnectTicket | None,
    cancel_request: CancelToken | None,
    stopped: Event,
    fatal_error: BaseException | None,
)
```

线程 target 必须是静态 runner，例如 `_run_actor(runtime_core)`，不能是 `SocketTransport` 的 bound method。`ActorRuntime` 不得反向引用公开 transport。

### 5.3 `TcpGeneration`

每次新 TCP socket 都创建新的 generation：

```python
TcpGeneration(
    generation_id: int,
    sock: socket.socket,
    endpoint: ResolvedEndpoint,
    state: TcpState,
    decoder: ResponseFrameDecoder,
    tx_bytes: bytes,
    tx_offset: int,
    active_exchange: WireExchange | None,
    connected_at: float,
    last_activity_at: float,
)
```

generation 的任何缓冲、selector key、发送偏移和 active exchange 都不能跨代复用。

### 5.4 `AdmissionWaiter`

`AdmissionWaiter` 只属于 pool admission，在取得 slot lease 之前存在：

```python
AdmissionWaiter(
    pool_epoch: int,
    waiter_id: int,
    deadline: float,
    state: AdmissionState,
    assigned_lease: SlotLease | None,
    error: BaseException | None,
    completed: Event,
)
```

- pool 是 waiter 的唯一 terminal owner。
- 状态为 `WAITING -> ASSIGNED | TIMED_OUT | REJECTED | CLOSED`。
- queue timeout、queue full 和 pool close 不涉及 Actor，也不能创建 `RequestTicket`。
- 只有 `ASSIGNED` waiter 获得 lease 后，facade 才创建带确定 `runtime_epoch + lease_id` 的 `RequestTicket`。
- timeout/cancel waiter 必须从 FIFO 原子移除并唤醒下一有效 waiter。

### 5.5 `RequestTicket`

`RequestTicket` 是 caller 与 Actor 之间的单次请求结果槽：

```python
RequestTicket(
    runtime_epoch: int,
    lease_id: int,
    command: int,
    request_payload_snapshot: object,
    deadline: float,
    retry_safe: bool,
    attempts: int,
    state: RequestState,
    result: FrameEnvelope | None,
    error: BaseException | None,
    completed: Event,
)
```

只有 Actor 可以把 ticket 变为 terminal。caller 不得直接 `set_result()`、`set_exception()` 或释放 lease。ticket 在取得 lease 后才创建，因此不包含 `WAITING_FOR_SLOT` 状态。

### 5.6 `FrameEnvelope`

Actor 返回给 caller 的不可变对象：

```python
FrameEnvelope(
    runtime_epoch: int,
    tcp_generation: int,
    lease_id: int,
    msg_id: int,
    msg_type: int,
    command: int,
    connected_host: str,
    request_payload_snapshot: object,
    response: ResponseFrame,
)
```

caller parser 只能使用 envelope 中的数据，不得读取 transport 当前 generation 或其他可变状态。

### 5.7 `PushBuffer`

连接池持有一个共享的有界 push buffer；独立 `SocketTransport` 持有自己的 buffer：

```python
PushBuffer(
    owner_epoch: int,
    max_frames: int = 1024,
    max_bytes: int = 8 * 1024 * 1024,
)
```

buffer 满时：

1. 丢弃最旧帧，保留较新行情。
2. 增加累计 `dropped_total`。
3. 设置 sticky gap 标志。
4. 下一次 `poll_push()` 或 `drain_pushes()` 抛出一次 `PushOverflowError`。
5. Actor 的 `offer_nowait()` 永远不能阻塞。

PushBuffer 属于单个 pool/runtime epoch：

- close 会把旧 buffer 标记 closed、唤醒全部旧 poller，并拒绝后续 offer。
- normal close 唤醒的阻塞 `poll_push(None)` 返回 `None`；runtime fatal 唤醒时抛对应 `TransportError`。
- close 时旧帧和 sticky gap 不得迁移到下一 epoch。
- normal reopen 创建全新的 PushBuffer。
- Actor 只持有其 runtime 创建时注入的旧 buffer 或其弱引用，不能读取 facade 当前 buffer。
- facade/pool 在旧 runtime 全部退出后才发布新 buffer 指针。

### 5.8 `LeaseBroker`、`SlotLease` 与 pinned proxy

`LeaseBroker` 是独立于公开 pool facade 的内部调度核心，只持有 pool state、waiters、idle slots 和 leases，不反向引用 `PooledSocketTransport`、`SocketTransport` 或 Actor runtime。

- pool facade 强引用 broker。
- Actor runtime 只持有 broker 的 `weakref` 或不反向保活 facade 的 completion handle。
- broker 消失表示公开 pool 已被遗弃；Actor 完成 wire cleanup 后可以安全忽略 lease 通知。
- 禁止把 bound `pool.release` / `transport.release` callback 保存进 Actor runtime。

```python
SlotLease(
    pool_epoch: int,
    lease_id: int,
    slot_id: int,
    state: LeaseState,
    pinned: bool,
)
```

- 普通请求在 wire terminal 后由 Actor completion 路径释放 lease。
- `pin()` 的 lease 在整个 context 生命周期内保留。
- `pin()` 返回带 epoch 的临时 proxy，不能返回裸 `SocketTransport`。
- context 退出、pool close 或 epoch 变化后，旧 proxy 必须失效。

---

## 6. 状态机

### 6.1 Runtime 状态

```text
STOPPED
   |
   v
STARTING -> RUNNING -> CLOSING -> STOPPED
                |           |
                v           v
              FAILED   FAILED_CLOSING -> FAILED_CLOSED
```

规则：

- `STARTING -> RUNNING` 只在 Actor 线程和 wakeup/selector 全部就绪后发生。
- `RUNNING -> CLOSING` 是 `close()` 的线性化点。
- `FAILED` 表示 Actor 未预期异常，不能自动补线程。
- 只有确认旧线程已 join、旧 selector/wakeup 已关闭、所有 ticket 已 terminal，正常 `STOPPED` 才可创建新 runtime epoch。
- `close()` 等待 Actor 超过内部 1 秒硬上限时抛 `TransportCloseTimeoutError`，保留完整 runtime/thread 引用并进入 `FAILED_CLOSING`。
- `FAILED_CLOSING` 拒绝 `connect()`、`execute()` 和 reopen；Actor 最终退出后发布 `FAILED_CLOSED`。
- 后续 `close()` 可以再次写 wakeup 并等待同一旧线程；线程结束后 close 幂等返回，但 transport 仍保持 `FAILED_CLOSED`。
- `FAILED_CLOSED` 不允许在同一 transport 上 reopen；调用方应创建新 client。

### 6.2 TCP 状态

```text
DOWN
  |
  v
CONNECTING -> CONNECTED_UNHANDSHAKEN -> HANDSHAKING -> READY
     |                  |                    |          |
     +------------------+--------------------+----------+
                            error/timeout
                                  |
                                  v
                              RETIRING
                                  |
                                  v
                                DOWN
```

所有断线原因必须经过同一个 `_drop_generation(reason)`：

1. 从 selector 注销精确 socket。
2. 关闭精确 socket。
3. 清空该 generation 的 RX/TX 和 active exchange。
4. 使旧 selector event 失效。
5. 增加 generation counter 后才能创建下一 socket。

### 6.3 Admission 状态

```text
WAITING_FOR_SLOT
      |
      v
ASSIGNED | TIMED_OUT | REJECTED | CLOSED
```

pool 是 admission waiter 的唯一 terminal owner。`ASSIGNED` 后 waiter 结束，lease 和新建的 `RequestTicket` 接管后续生命周期。

### 6.4 Request 状态

```text
ADMITTED -> SENDING -> WAITING_RESPONSE
    |          |              |
    +----------+--------------+
               |
               v
      SUCCESS | FAILED | CANCELLED
```

只有一个 complete-once 入口可以进入 terminal。response、network timeout、cancel、close 和 actor fatal 必须在同一个 ticket 状态上竞争，输的一方只能成为 no-op。

### 6.5 Pool 状态

```text
STOPPED -> STARTING -> RUNNING -> CLOSING -> STOPPED
                         |
                         v
                       FAILED

CLOSING -> FAILED_CLOSING -> FAILED_CLOSED
```

- pool epoch 在每次启动和 close 线性化时增加。
- waiter 和 lease 必须带 pool epoch。
- pool condition 只保护 waiters、idle slots、lease 和 pool state。
- 持 pool condition 时禁止调用 Actor、等待 ticket、close 或 join。
- `FAILED_CLOSING` / `FAILED_CLOSED` 遵守与 slot runtime 相同的保留引用和禁止 reopen 规则。

---

## 7. 不可违反的不变量

1. 每个 slot 任意时刻最多一个 Actor 线程。
2. 每个 slot 任意时刻最多一个 TCP socket。
3. 每个 slot 任意时刻最多一个业务 wire request 在途。
4. 网络 socket 只能由对应 Actor 线程操作，包括 close。
5. selector key 必须校验 `runtime_epoch + tcp_generation + socket identity`。
6. 旧 generation 的事件、错误和帧不得修改当前 generation。
7. timeout 或发送后 cancel 必须退役当前 generation。
8. 普通 lease 只能由 wire-terminal completion 路径释放一次。
9. pinned lease 只能由 pin context 退出或 pool close 释放一次。
10. Actor、pool 和 push buffer 内部结构全部有硬上限。
11. 任何状态锁内都不能执行网络 I/O、join、用户 parser 或可能触发用户 callback 的操作。
12. `close()` 返回成功时，旧 Actor、socket、selector、wakeup 和 ticket 必须全部结束。
13. 未确认旧 runtime 退出时，绝不能启动新 runtime。
14. Actor fatal 不能静默补线程或降级为看似成功。
15. 所有 deadline 使用 `time.monotonic()` 的绝对时间。

---

## 8. 锁与所有权规则

| 保护对象 | 锁 | 允许持锁操作 | 禁止持锁操作 |
| --- | --- | --- | --- |
| facade runtime 指针和 epoch | lifecycle lock | 读取/替换 runtime、状态检查 | join、ticket wait、socket I/O |
| runtime 控制状态 | control lock | 设置 stop/cancel/pending、读取 snapshot | selector、socket I/O、ticket completion |
| pool idle/waiter/lease | pool condition | 分配/回收 lease、epoch/state | actor submit、ticket wait、close、join |
| push deque/bytes/gap | push condition | O(1) offer/pop、计数 | payload parse、阻塞 Actor |
| ticket terminal 数据 | ticket lock/event | complete-once 数据写入 | pool join、用户 callback |

实现应尽量不嵌套这些锁。必须跨组件通知时，先在本组件锁内形成不可变快照，释放锁，再调用另一个组件。

---

## 9. Actor 事件循环

伪代码：

```python
def run_actor(runtime):
    try:
        initialize_selector_and_wakeup(runtime)
        publish_running(runtime)

        while not runtime.stop_requested:
            drain_control_state(runtime)
            if runtime.stop_requested:
                break

            start_admitted_task_if_idle(runtime)
            schedule_idle_heartbeat_if_due(runtime)

            timeout = seconds_until_nearest_deadline(runtime)
            events = runtime.selector.select(timeout)

            handle_wakeup_events_first(runtime, events)
            if runtime.stop_requested:
                break

            handle_current_socket_events_with_budget(runtime, events)
            expire_deadlines(runtime)
    except BaseException as exc:
        mark_runtime_failed(runtime, exc)
    finally:
        fail_or_cancel_all_tickets(runtime)
        drop_current_generation(runtime)
        close_selector_and_wakeup(runtime)
        publish_stopped(runtime)
```

每轮处理网络数据必须有预算，例如最大读取字节数和最大解析帧数。达到预算后必须返回控制循环，优先检查 STOP、cancel、deadline 和用户任务，避免 push flood 饥饿。

---

## 10. Wakeup 协议

使用非阻塞 `socket.socketpair()`：

- read 端注册到 Actor selector。
- producer 先在 control lock 下修改共享状态，再尝试写一个字节。
- `BlockingIOError` 表示已有 wake 数据，不能视为通知失败。
- Actor 收到可读事件后一直读取到 `BlockingIOError`，然后重新读取全部共享控制状态。
- wake byte 只表示“状态可能变化”，绝不能假定一个字节对应一个任务。
- STOP 是独立布尔状态，不能仅依赖向已满 mailbox 塞 sentinel。

必须覆盖 producer 与 drain 并发、wakeup buffer 已满、writer EOF 和 close 时写满等竞态。

---

## 11. 非阻塞连接

### 11.1 Endpoint 解析

- 默认主站是数字 IP，直接构造 endpoint。
- 用户 hostname 在 Actor 外由 caller-side resolver 解析并缓存。
- 标准库阻塞 DNS 是显式 preflight，不计入本方案可证明的端到端 request deadline；默认数字 IP 路径没有该例外。
- DNS preflight 不能占用 pool lease、创建 RequestTicket 或启动 Actor 网络操作。
- 解析开始前捕获 facade/pool epoch，解析结束后必须重新检查；期间发生 close 时结果直接丢弃。
- 禁止 Actor 内调用阻塞 `getaddrinfo()`。
- 标准库 DNS 无法可靠取消；自定义 hostname 的这项限制必须在 API 文档中明确。若未来需要 DNS 硬 SLA，应单独设计可替换 resolver，不能把阻塞调用塞进 Actor。

### 11.2 `connect_ex()`

1. 创建 socket 后立即 `setblocking(False)`。
2. 调用 `connect_ex()`。
3. 平台 errno 集合从 `errno` / `socket` 动态构造，不能硬编码 Windows 数字。
4. 立即成功值包括 `0` 和平台存在的 `EISCONN`。
5. in-progress 值包括平台存在的 `EINPROGRESS`、`EWOULDBLOCK`、`EALREADY`、`EINTR` 及 Windows 对应值。
6. connecting socket 注册 READ/WRITE。
7. 任一事件到来都读取 `SO_ERROR`；只有 `SO_ERROR == 0` 才算成功。
8. 所有候选主站共享当前请求的绝对 deadline。

显式 pool `connect()` 应并行启动 N 个 Actor 的非阻塞连接。任一 slot 失败时，先向全部 slot 发 stop，再等待全部退出，最后抛异常；不能遗留部分连接。

---

## 12. Partial send

请求 frame 必须是不可变 `bytes`，Actor 保存 `tx_offset`：

- `send()` 返回正数：增加 offset。
- `send()` 返回部分长度：继续监听 WRITE。
- `BlockingIOError`：保留 offset，等待下一次 WRITE。
- `send()` 返回 0：视为连接关闭。
- 错误、timeout 或 cancel：整个 generation 退役。
- 剩余字节永远不能发送到下一 generation。

发送完成后取消 WRITE interest，只保留 READ，并把 exchange 置为 `WAITING_RESPONSE`。

---

## 13. 增量收包与安全解压

现有 `read_exact()` 不得在 Actor 路径中使用。新增 `ResponseFrameDecoder`：

- generation 独享持久 `bytearray`。
- prefix、16 字节 header 和 payload 可以在任意边界拆分。
- 一次 `recv()` 可以解析多个粘连 frame。
- 不完整 frame 保留到下一次 feed。
- 垃圾前缀使用 `find()` 重同步，只保留可能构成 prefix 的最长后缀。
- 重同步丢弃字节数和总 buffer 大小都有上限，越界即 `ProtocolError` 并退役 generation。
- 单帧长度必须受协议 16 位字段和内部常量双重限制。

压缩 payload 禁止直接无界 `zlib.decompress()`。使用 `decompressobj` 和 `max_length=declared_length + 1`，验证：

- 输出长度严格等于声明长度。
- `eof` 为真。
- 没有不允许的 `unconsumed_tail`。
- 没有不允许的 `unused_data`。
- 压缩和解压长度都不超过协议上限。

帧结构或解压错误会破坏 wire 信任，必须退役 generation。完整 `ResponseFrame` 的业务 payload parser 错误只失败当前调用，不需要关闭连接。

---

## 14. 响应路由

当前 active exchange 使用以下完整身份匹配：

```text
runtime_epoch
+ tcp_generation
+ lease_id
+ msg_id
+ msg_type
```

匹配成功顺序：

1. 构造不可变 `FrameEnvelope`。
2. 在 Actor 内原子把 ticket 状态置为 terminal `SUCCESS` 并保存 envelope，但暂不 set completed Event；从此 cancel/close 只能 no-op。
3. 清除 active exchange，Actor 回到 `READY`。
4. 普通请求 exact-once 释放 pool lease；pinned 请求不释放。
5. 发布 ticket completed Event，唤醒 caller。
6. caller 在所有内部锁之外解析业务模型。

lease 释放前 ticket 必须已经在线性化意义上 terminal；caller 被唤醒前 lease 必须已经释放。需要用 Barrier 精确覆盖 response、cancel、close 三方竞争窗口。

未匹配帧：

- heartbeat 响应按内部 heartbeat exchange 处理。
- 其他合法帧进入共享 `PushBuffer`。
- 旧 generation 或 identity 不匹配事件直接丢弃并计数。

---

## 15. 握手与 heartbeat

### 15.1 握手

- `connect()` 只要求 TCP 建立，保持现有“不立即握手”语义。
- 首个非握手业务请求前，Actor 自动进入 `HANDSHAKING`。
- 握手和业务请求不能同时在途。
- 握手响应很小，可以在 Actor 内解析并发布 `last_handshake`。
- 握手失败按 retry policy 退役 generation。

### 15.2 Heartbeat

- heartbeat 是 Actor 的内部 timer，不创建线程。
- 最近存在业务或网络活动时顺延。
- 有业务 waiter、active request 或 cancel 时不启动。
- heartbeat 已发送后按普通 wire exchange 等待，不能与业务请求并发。
- heartbeat 失败只退役当前 generation，不创建新 Actor。
- 没有业务时不得进行无上限紧密重连；只在下一 heartbeat deadline 或用户请求到来时尝试。

---

## 16. Deadline、重试与取消

### 16.1 单一端到端 deadline

数字 IP 和已缓存 endpoint 采用一个端到端 `timeout`。自定义 hostname 的首次阻塞 DNS 是第 11.1 节定义的 preflight 例外：

```text
deadline = monotonic() + timeout
```

deadline 在 endpoint 可用后、进入 pool admission 前创建，覆盖：

- pool admission 等待
- 候选主站连接
- 握手
- partial send
- 响应等待
- 最多一次新 generation 重试

重试不能重置 deadline。任何受 deadline 约束的阶段超时继续抛现有 `ResponseTimeoutError`，错误消息和内部 metrics 必须标注阶段，例如 `queue`、`connect`、`handshake`、`send`、`response`。

这是对旧实现可能远超配置 timeout 的行为收紧，需要在 changelog 和 API 文档中明确。自定义 hostname 首次 DNS 不受该硬上限保证，但解析期间不占用任何 Actor、lease 或 TCP 资源。

### 16.2 Admission 容量

新增公开容量参数：

```python
max_pending_requests: int = 256
push_queue_size: int = 1024
push_queue_bytes: int = 8 * 1024 * 1024
```

`max_pending_requests` 只统计等待 lease 的请求，不含 N 个 active 请求。队列已满时立即抛 `PoolBusyError(TransportError)`；不再增加独立 `queue_timeout` 参数。

### 16.3 重试

给 `CommandSpec` 增加 `retry_safe: bool`：

- 当前所有 7709 查询命令显式标为 `True`。
- retry-safe 请求遇到连接错误、EOF 或 response timeout 时，可在新 generation 最多重试一次。
- 只有 deadline 仍有剩余预算时才重试。
- 非 retry-safe 请求在发送任何字节后不得自动重试。
- close、cancel、Actor fatal、帧结构错误和 caller payload parser 错误不重试。

### 16.4 Caller 取消

caller 遇到 `KeyboardInterrupt` 或其他本地取消时：

1. 尚在 pool admission 时，由 pool 原子移除并 terminal `AdmissionWaiter`，不创建 ticket、不接触 Actor。
2. 已取得 lease 后，提交带 `runtime_epoch + tcp_generation + lease_id` 的 Cancel。
3. caller 不释放 lease。
4. 若响应已 terminal，Cancel 作为 stale no-op。
5. 若尚未发送，Actor 直接取消并释放普通 lease。
6. 若已发送但未匹配完整响应，Actor 先退役 generation，再取消并释放普通 lease。
7. pinned lease 仍由 pin context 管理。

---

## 17. Pool 调度与高并发

pool 内部使用：

```python
idle_slots: deque[int]
waiters: deque[AdmissionWaiter]
active_leases: dict[int, SlotLease]
pool_epoch: int
state: PoolState
condition: Condition
```

调度规则：

- 空闲 slot 从左侧取出，释放后放到右侧。
- waiter 严格 FIFO。
- slot 释放时优先直接交给最老有效 waiter。
- timeout/cancel waiter 必须原子移除；队首 tombstone 不能阻塞后续 waiter。
- pool condition 临界区内不能调用 Actor 或等待任何 Future/Event。
- 一个慢 slot 不得积压后续请求；所有等待只存在于全池 admission。
- Actor mailbox 逻辑容量为 1。

理论稳定吞吐：

```text
throughput ~= pool_size / average_wire_latency
```

队列只能吸收突发，不能提高长期吞吐。长期到达率超过容量时必须触发 timeout 或 `PoolBusyError`。

---

## 18. `pin()` 语义

- `pin()` 与普通请求一起进入 FIFO admission。
- 获得 lease 后，整个 context 独占同一 slot。
- context 内的请求仍然一次一个 wire exchange。
- 单次请求完成不能释放 pinned lease。
- proxy 带 pool epoch 和 active 标志。
- pool close 后 proxy 立即失效。
- close 不能等待用户退出 pin context，否则可能永久死锁。
- pin context 退出时若仍有 in-flight，先 cancel 并等待 Actor quiescent，再释放 lease。
- 多块 `0x06B9` 文件读取仍由现有业务逻辑检查 connected host 是否变化。

Pinned proxy 的兼容面必须明确：

- 实现 `Transport` 所需的 `execute()`、`request()`、`connect()`、`close()`、`poll_push()` 和只读连接属性。
- `connect()` 只验证并确保所租 slot 可用，不能切换 slot。
- proxy 的 `close()` 不得关闭底层共享 slot；它应原子使 proxy 失效，取消并等待当前 wire quiescent，提前 exact-once 释放 pinned lease，然后幂等返回。context exit 再次清理必须是 no-op，后续 proxy 调用抛 `ConnectionClosedError`。
- `connected_host` 必须来自所租 slot 的 snapshot。
- `poll_push()` / `drain_pushes()` 使用 pool epoch 对应的共享 PushBuffer。
- 同一 proxy 被多个线程调用时，使用 pin-local FIFO 串行等待并共享原始端到端 deadline；不得产生第二个 in-flight，也不能超出 pool `max_pending_requests` 的总体容量核算。
- context 退出会失败所有尚未 admitted 的 pin-local waiter，并按 cancel 规则处理 active ticket。

---

## 19. Close、reopen、fatal 与 finalizer

### 19.1 正常 close

1. 在 lifecycle/pool lock 下转为 `CLOSING` 并增加 epoch。
2. 拒绝新 admission。
3. 唤醒并失败全部 waiter 和阻塞 `poll_push(None)`。
4. 释放锁。
5. 对所有 runtime 设置 stop 并写 wakeup。
6. Actor 自己失败 active ticket、退役 socket、关闭 selector 和 wakeup。
7. 外部线程在不持状态锁时 join。
8. 所有 Actor 确认退出后进入 `STOPPED`。

pool 使用一个 1 秒绝对 close deadline 并先向全部 Actor 发 stop，再逐个使用剩余预算 join，不能给每个 slot 重新获得 1 秒。任一 Actor 到期仍存活时，pool/runtime 保留全部未结束引用，进入 `FAILED_CLOSING` 并抛 `TransportCloseTimeoutError`；已经退出的 Actor 不重启。后续 `close()` 可继续等待同一批旧 Actor，全部退出后进入不可 reopen 的 `FAILED_CLOSED`。

### 19.2 Reopen

- 只有正常 `STOPPED` 才允许 `connect()` / `execute()` 创建新 runtime。
- 新 runtime 使用新的 epoch、新 socketpair、新 selector 和新 thread。
- 旧 finalizer 必须 detach。
- 旧 waiter、ticket、proxy 和 Cancel 不能迁移到新 runtime。
- 旧 PushBuffer 在 close 时永久 closed；reopen 发布新的 epoch-scoped PushBuffer，旧帧、gap 和 poller 不能迁移。

### 19.3 Actor fatal

- 网络异常是正常状态机事件，不属于 fatal。
- 未预期异常由 top-level runner 捕获。
- runtime 转为 `FAILED`，失败所有 ticket，唤醒 waiter/push poller并释放资源。
- 不自动创建替代 Actor。
- 第一版策略为 pool fail-closed；调用方显式 close 后创建新 client。

### 19.4 Finalizer

- finalizer callback 只能捕获对应旧 `ActorRuntime`。
- callback 只执行非阻塞 stop+wakeup，不能 join。
- callback 不得读取 `transport._runtime`，避免旧 finalizer 关闭 reopen 后的新 runtime。
- `PooledSocketTransport` / `TdxClient` 整体被遗弃时，所有 slot runtime 都必须通过各自 finalizer 或 pool abandon 路径收到 stop+wakeup。
- Actor 向 pool 报告 wire terminal 时只能通过不反向保活 facade 的 `LeaseBroker` weak reference / completion handle；禁止保存 bound pool callback。
- `LeaseBroker` 不得持有 Actor runtime 或公开 facade。pool facade 消失后 broker 可被回收，Actor 仍先完成本地 generation/ticket cleanup，再把缺失 broker 当作无需通知。
- Actor thread 可以是 daemon，但 daemon 不能替代资源清理。

---

## 20. 公开兼容性

保持不变：

- `TdxClient(...)`
- `TdxClient.from_hosts(...)`
- `connect()` / `close()` / context manager
- 所有业务 API 和 Helpers
- `poll_push()` / `drain_pushes()`
- `pool_size` 和 `heartbeat_interval`
- 7709 命令、payload、返回模型

新增可选构造参数追加在参数列表末尾：

```python
max_pending_requests: int = 256
push_queue_size: int = 1024
push_queue_bytes: int = 8 * 1024 * 1024
```

新增异常：

```python
class PoolBusyError(TransportError): ...
class PushOverflowError(TransportError): ...
class TransportCloseTimeoutError(TransportError): ...
```

有意改变：

- `timeout` 成为端到端逻辑请求上限。
- `pin()` 从“挑选一次”升级为真正独占 slot。
- push overflow 从无界增长变为有界、显式 gap。
- concurrent close 会取消已经 admitted 但尚未完成的请求。

---

## 21. 文件级改造

### 新增

| 文件 | 内容 |
| --- | --- |
| `src/eltdx/transport/actor.py` | Runtime、Actor、Generation、Ticket、Envelope、事件循环 |
| `src/eltdx/transport/push.py` | 有界共享 PushBuffer 和 gap 语义 |
| `tests/test_frame_stream.py` | 增量帧、边界和安全解压测试 |
| `tests/test_transport_actor.py` | Actor 状态机、故障注入和 close/reopen 测试 |
| `tests/test_transport_stress.py` | 有界快速 stress；重型 soak 可按 marker 分离 |
| `scripts/benchmark_actor_transport.py` | 固定本地 server workload、旧/新实现可复核性能基准 |

### 修改

| 文件 | 内容 |
| --- | --- |
| `src/eltdx/transport/socket.py` | 删除旧 reader/heartbeat，实现同步 facade |
| `src/eltdx/transport/pool.py` | FIFO admission、lease、pin proxy、并行 connect、rollback |
| `src/eltdx/protocol/frame.py` | 增量 decoder、安全 zlib、长度上限 |
| `src/eltdx/protocol/commands/registry.py` | `retry_safe` metadata |
| `src/eltdx/hosts.py` | endpoint 解析与缓存 |
| `src/eltdx/client.py` | 透传容量参数 |
| `src/eltdx/exceptions.py` | `PoolBusyError`、`PushOverflowError`、`TransportCloseTimeoutError` |
| `src/eltdx/transport/__init__.py` | 必要导出，Actor 内部类型不公开 |
| `tests/test_socket_transport.py` | 改为新 facade 和兼容性测试 |
| `tests/test_transport_pool.py` | lease、公平性、pin、rollback、close 测试 |
| `tests/test_client.py` | 参数透传和异常兼容测试 |
| `docs/ARCHITECTURE.md` | 更新 transport 架构 |
| `docs/API_REFERENCE.md` | timeout、容量参数、push overflow |
| `docs/DEBUG_GUIDE.md` | Actor 状态、队列和诊断指标 |
| `.github/workflows/ci.yml` | 增加 Windows Actor 关键测试矩阵 |

另新增永久最终清单 `ACTOR_REFACTOR_RESULT.md`。它只在最终阶段生成，记录最终 commit、CI、完整测试、stress/soak、性能、资源指标、兼容性变化和剩余风险；临时账本删除后，它接替恢复和审计职责。

旧 `read_exact()` / `read_response_frame()` 可以暂时保留给非 Actor 测试或兼容导入，但新 transport 不得引用。确认无调用后再由独立清理提交决定是否删除。

---

## 22. 实施 Checkpoint

本节 A00-A09 描述原始实现阶段。当前 correction cycle 使用 F00-F06 和
FINAL；两套提交都必须是可独立验证、可追加修正的原子提交。当前提交信息
使用 trailer：

```text
Fix-Checkpoint: F03
```

### A00：基线与耐久账本

- 创建/恢复 `actor-transport-refactor` 分支。
- 记录实际 base SHA、工作区已有改动和远端状态。
- 原始阶段提交本文档和 `ACTOR_REFACTOR_PROGRESS.md`；correction 恢复使用
  `ACTOR_REFACTOR_FIX_PROGRESS.md`。
- 运行完整现有测试，记录基线。
- 记录机器、操作系统、CPU、Python 版本和当前资源基线，供性能结果复核。
- 建立 draft PR，使后续分支 push 触发 CI；禁止合并。

验收：基线测试结果、branch、HEAD、dirty paths 和远端同步状态都有持久记录。

### A01：故障注入基础设施与基线证据

- 建立可复用的本地 scripted server、fake selector/socket、Barrier 和 Event 测试工具。
- 用不进入永久测试套件的临时诊断或现有代码证据，稳定记录 reader/heartbeat 复制、半包 timeout 丢失、pool 部分 connect 失败和 `pin()` 非独占的基线现象。
- 把复现步骤、线程/连接变化和失败签名写入账本。
- 永久回归断言必须与修复对应行为的 checkpoint 一起提交，不能留下一个故意失败或 xfail 的检查点。
- A01 提交中的测试工具和兼容性测试必须全部通过。
- 提交固定的本地 benchmark 脚本和 scripted server workload，在旧提交上记录 pool size 1/2/4、并发 1/10/100 的原始数据、命令、环境和 JSON/表格摘要。
- 原始 benchmark 数据可以放在忽略的 artifacts 目录，但账本必须记录摘要和 hash，最终 `ACTOR_REFACTOR_RESULT.md` 必须保留可复核的旧/新对比。

验收：故障注入工具和 benchmark 不依赖 sleep 或真实行情主站；基线证据可复核；checkpoint 自身保持绿色。

### A02：增量 FrameDecoder

- 实现 prefix/header/payload 增量解析。
- 支持任意拆包、多帧粘包和垃圾前缀恢复。
- 实现安全 zlib 上限和错误检查。
- 对 buffer、frame、resync 建立硬上限。

验收：逐字节 feed、所有边界、畸形压缩和超限测试通过。

### A03：Actor Runtime 与 wakeup

- 实现 Runtime、Ticket、Generation 和静态 runner。
- 实现 socketpair、selector、control state 和 stop。
- 实现非阻塞 connect_ex、SO_ERROR 和 host failover。
- close 可以中断 connecting/idle selector。

验收：Actor 内无 blocking API；连接和 close 故障注入通过；线程可确定退出。

### A04：请求 wire lifecycle

- 实现握手、partial send、增量 recv、响应匹配和 FrameEnvelope。
- 实现单 active request、deadline、retry-safe 一次重试。
- 实现 cancel、迟到事件和 generation retire。

验收：partial send/recv、response-timeout、old-event、fd reuse 和 complete-once 测试通过。

### A05：SocketTransport facade、heartbeat 与 push

- 替换旧 `SocketTransport` 内部实现。
- 删除 reader/heartbeat 线程路径。
- 业务 parser 移到 caller。
- heartbeat 改为 Actor timer。
- 接入有界 PushBuffer 和 overflow gap。
- 保持公开属性与调用方式。

验收：原有 socket/client/protocol 测试通过；新线程数为每 slot 一个。

### A06：Pool lease 与 pin

- 实现全池 FIFO admission 和 max pending。
- 实现 first-idle 调度、exact-once lease release。
- 实现 pinned epoch proxy。
- 实现并行 connect 和失败全量 rollback。
- 实现 pool close 唤醒全部 waiter/poller。

验收：慢 slot 不产生 HOL；pin 真独占；close 无锁反转；部分失败零残留。

### A07：生命周期、finalizer 与诊断

- 实现 normal reopen 和旧 runtime 隔离。
- 实现 fatal fail-closed。
- 实现不反向引用 facade 的 finalizer。
- 增加只读诊断 snapshot：state、generation、queue depth、reconnect count、push dropped、last error。

验收：显式 close 确定归零；GC abandon 最终退出；旧 finalizer 不影响新 runtime。

### A08：压力、性能与跨平台

- 完成 fault matrix。
- 本地 Windows 跑快速 1,000 次重连和完整测试。
- 跑 10,000 次 generation 更换、100,000 次混合请求的 soak。
- 对 pool size 1/2/4、并发 1/10/100 建基准。
- 使用同一已冻结 revision 的 benchmark、verifier、server workload、机器和配置同时加载旧/新 source root，并保存全部原始对比。
- CI 覆盖 Ubuntu Python 3.10-3.13 和 Windows 关键版本。

验收：满足第 24 节全部硬门和版本 1.2 的性能披露/例外规则，无
skipped/xfail/flaky 掩盖失败。

### A09：文档、清理与最终审阅

- 更新架构、API、调试和 changelog。
- 搜索并删除不可达的旧 reader/heartbeat 路径。
- 逐文件审阅 base..HEAD diff。
- 完整测试、build、MkDocs strict、stress 和 CI 全绿。
- 确认无测试服务器、后台进程和临时文件。
- 提交并推送 A09 verification checkpoint，保留账本，等待该 commit 的 required CI 全绿。
- 根据已验证证据生成永久 `ACTOR_REFACTOR_RESULT.md`，包含最终 commit 链、命令、CI、stress/soak、性能、资源指标、兼容性变化和剩余风险。
- 在同一个 correction finalization commit 中加入 result manifest 并删除
  临时 `ACTOR_REFACTOR_FIX_PROGRESS.md`，trailer 使用
  `Fix-Checkpoint: FINAL`。原始 `ACTOR_REFACTOR_PROGRESS.md` 仅属于历史
  A00-A09 协议。
- 推送 finalization commit 并等待该 commit 的 required CI 全绿。
- draft PR 保持未合并，交付用户审阅。

最终 evidence 规则：最终 Result/Plan commit 只能记录可在提交前确定的
source SHA、命令、artifact hash 和 `SELF` 解析规则。仓库 commit 无法预先
嵌入该 commit push 后才生成的 GitHub Actions run ID 或 Pages build URL；在
exact final `SELF` 的 CI/Pages 完成后，把 URL、结论、测试数和“PR 仍为
Draft、未合并”写入 PR #12 的持久交付评论（或 PR body），并在最终报告中
引用。不要为了把这些 post-push URL 复制回 Result 再创建新 HEAD。

验收：账本存在期间所有进度可恢复；账本删除后，所有完成证据和恢复入口都可从 Git、CI 和永久 result manifest 复核。

---

## 23. 确定性测试矩阵

### FrameDecoder

- prefix 在每个字节边界拆分。
- header 在每个字节边界拆分。
- payload 在每个字节边界拆分。
- 一次 feed 包含多帧。
- 垃圾前缀、部分 prefix 后缀和超限垃圾。
- EOF 时仅有半帧。
- 声明长度错误。
- 正常压缩、畸形 zlib、尾随数据和超量解压。

### Connect

- `connect_ex()` 立即成功。
- 各平台 in-progress errno。
- 可写但 `SO_ERROR` 失败。
- 立即拒绝。
- host failover。
- deadline 用尽。
- close 发生在 CONNECTING。
- pool 第 N 个 slot 失败后全量回滚。

### Send/recv

- 每个 offset 的 partial send。
- 连续 `BlockingIOError`。
- `send()` 返回 0。
- 发一半后 EOF。
- 发完后 response timeout。
- 旧响应晚到。
- 未匹配合法帧进入 push。

### Wakeup/control

- producer 在 Actor drain 时提交。
- Actor 即将 select 时提交。
- wake buffer 已满。
- close 时 wake buffer 已满。
- writer EOF。
- 10 到 100 个提交线程竞争。

### Lifecycle

- close 在 pre-admission、post-admission、connect、partial send、wait response、frame terminal 前后发生。
- 双 close。
- close 后 reopen。
- Actor fatal。
- close 硬 deadline 到期后进入 `FAILED_CLOSING`，后续 close 继续等待，禁止第二 Actor。
- `FAILED_CLOSED` 禁止 reopen。
- standalone transport 和 pooled client 的 finalizer 分别在 idle、connected、waiting response 三种状态触发。
- response、cancel、close 在 ticket terminal 发布窗口三方竞争。

### Pool/pin

- FIFO 公平。
- admission waiter 在取得 lease 后才转换为 RequestTicket。
- timeout waiter 原子移除。
- 一个慢 slot、其他快 slot。
- queue 满立即显式失败。
- lease exact-once。
- pin context 真独占。
- pool close 不等待遗失的 pin context。
- 旧 pin proxy 在 epoch 变化后失效。
- 同一 pinned proxy 多线程调用仍只有一个 in-flight，proxy `close()` early-release 幂等。

### Push

- frame 数量上限。
- byte 上限。
- drop-oldest。
- sticky gap 只报告一次但累计计数保留。
- flood 下业务 response 不饥饿。
- `poll_push(None)` 在 close/fatal 后醒来。
- close 后旧 buffer 永久关闭，reopen 使用新 epoch buffer；旧 Actor late offer 不能进入新 buffer。

### 资源与响应归属

- thread 名称和数量。
- TCP socket、wakeup socket、selector 数量。
- Windows handle 或 Linux fd 基线。
- 所有 ticket terminal。
- 所有 waiter 和 lease 清空。
- 每个业务结果可追溯到同一 epoch/generation/lease/msg。
- 跨 generation 命中计数必须为 0。
- Actor/runtime 到 LeaseBroker 只存在弱引用安全 completion path，公开 pool/client 可以被 GC。

---

## 24. 验收指标

### 正确性

- 每 slot 始终 `actor <= 1`、`tcp_socket <= 1`、`inflight <= 1`。
- 10,000 次重连后 Actor thread identity 不变。
- close 后 Actor、TCP socket、wakeup、selector、ticket、waiter 和 lease 全部为 0。
- close 后等待两个 heartbeat 周期，服务端 accept 数不再增加。
- 旧 generation 结果完成新 ticket 的次数为 0。
- 数字 IP / 已缓存 endpoint 的每个请求在 deadline 内成功或返回规定异常，永久 hang 为 0；自定义 hostname 首次 DNS 遵守已文档化的 preflight 例外，解析后仍不得越过旧 epoch。
- 所有 buffer 和 queue 从未越过配置上限。

### 性能

版本 1.2 的用户批准例外只改变本次 FINAL 的完成判定，不改变任何历史
campaign 结果。以下旧实现对比已经执行；必须继续完整报告并保留 verifier
结论，但其 FAIL 不再单独阻挡本次交付：

- `pool_size=1`，10,000 顺序请求吞吐目标 >= 旧实现 95%。
- `pool_size=4`，100 并发、100,000 请求吞吐目标 >= 旧实现 95%。
- 服务端最大业务并发恰好等于 pool size。
- 100 并发持续饱和场景的 raw p50/p99 必须逐 trial 和 pooled 全量报告，
  但不再把 strict FIFO queue residence 冒充新增固定调度开销；吞吐 95%
  目标、原始 verifier 判定和披露口径保持不变，仅本次 FINAL 按版本 1.2
  例外不阻挡完成，p99 改善仍不能代偿其他 gate。
- 新增调度开销由同一 frozen campaign 的两个独立 latency probe 判定：
  `pool=1/concurrency=1` 顺序调用，以及 `pool=4/fixed cohort=4` 无积压
  调用。cohort 内同时放行，全部 future terminal 后才能创建下一 cohort；
  4-worker gate 使用 call latency。
- 上述两个 probe 的 p50 和 p99 分别要求
  `current - baseline <= max(baseline * 10%, 0.2ms)`，四个 gate 必须全部
  报告 verifier 结果。不得用吞吐、report-only 数据或另一个 quantile
  抵消失败；版本 1.2 允许将其原样记录为用户批准的架构性例外。
- `pool=4/fixed cohort=100` 有竞争 wave 仍保存共同 wave epoch 到完成的
  raw cohort latency，作为防跨 wave barging 的强制诊断，但不属于用户
  授权的无积压调度开销硬门。
- 一个 slot 500ms、其他 slot 10ms 时，新请求进入空闲 slot。
- caller parser 人为阻塞 50ms 时，slot 已能服务下一请求。
- heartbeat 对持续业务吞吐影响 < 1%。
- idle Actor 不持续消耗 CPU。

本次例外的证据规则：

- `fifo-v2-7923287-a` 及更早 campaign 的 FAIL、原始数组、bundle/report hash
  和独立审计结论保持不可变，不创建同 source 的有利重采。
- 永久结果必须明确列出最终保留实现的顺序/饱和 throughput ratio、顺序与
  no-backlog p50/p99、每个失败余量，以及用户在 2026-07-17 选择保留 Actor
  socket ownership 的授权。
- 服务端最大并发、两台真实 loopback、唯一响应、cross-request/cross-
  generation、heartbeat、close、idle CPU 和资源归零没有例外。
- 后续性能敏感变更以 FINAL Actor source 为基线；benchmark、阈值、停止
  规则仍必须在任何新样本前冻结，不能利用本次例外追溯放宽未来回归。

#### FIFO v1 前瞻 campaign

- 在首个样本前提交 benchmark、verifier、精确配置和停止规则。正式顺序
  固定为 `ABBA + BAAB`，即 baseline/current/current/baseline 后接
  current/baseline/baseline/current，共 8 个 trial；每个 cell 只允许
  `attempt=1`。
- 正式 campaign 必须分成两个独立命令：`declare` 只创建 declaration，
  主 Agent 在任何样本前先把其 canonical SHA256 写入外部任务记录；随后
  `run --expected-hash` 必须逐字匹配该记录。run 开始时目录只能包含这
  一个 declaration，任何隐藏文件、旧 attempt 或 terminal artifact 都
  直接失败。
- 每个 trial 使用同一 Windows/Python、同一脚本 SHA、同一 5.000ms
  loopback server delay、clean `71089c0` baseline 和 clean exact current
  HEAD。开始前写入带 canonical SHA256 的 declaration。
- 每个 trial 固定运行：顺序 1,000 warmup + 10,000 timed；持续饱和
  1,000 warmup + 100,000 timed；4-worker cohort 100 warmup + 2,500 timed
  cohorts；100-worker report-only wave 10 warmup + 50 timed cohorts。
- producer 保存每个 timed request 的未四舍五入 `latency_ns`、精确
  `elapsed_ns`、SHA/dirty/config identity、worker 数和 server max active。
  current cohort 在每个边界还必须证明 idle slots 全归还、waiter/pin
  waiter/active lease 全为 0。
- 独立 stdlib verifier 不导入 `eltdx`，只信原始数组。吞吐按所有固定
  trial 的 `sum(requests) / sum(elapsed)` 聚合并用整数交叉乘判门；p50
  使用完整 pooled raw 样本的中位数，p99 使用排序索引
  `floor((N-1)*0.99)`。
- 缺失、追加、换序、重复 trial/index、dirty source、SHA/hash/config
  不一致、非 Windows、trial 时间重叠、未知 schema 字段、请求错误、
  样本数/物理计时关系错误或任一 gate 失败都使 campaign FAIL。
  不允许查看中间结果后停止、补跑或替换 cell。客观基础设施中断必须
  保留原目录并用新 campaign ID 从 trial 0 完整重跑。

### Close

- idle/waiting network 时 close p99 < 100ms。
- 持续负载时 close p99 < 250ms。
- close 硬上限 1s；超过必须失败并保留可诊断 runtime 引用，不能伪报成功。

### 平台

- Ubuntu CI：Python 3.10、3.11、3.12、3.13。
- Windows 本地与 CI：至少 Python 3.11、3.13。
- `socketpair`、selector、connect errno 和资源回收必须使用真实 socket 测试，不能只有 mock。

---

## 25. 完成定义

只有同时满足以下条件，目标才可以标记 complete：

1. 原始 A00-A09 已完成；correction F00-F06 全部关闭且有 checkpoint
   commit，FINAL 按 1.3 协议完成。
2. 旧 reader/heartbeat 线程路径已删除或确认不可达。
3. 本地完整 pytest 通过。
4. package build 通过。
5. `mkdocs build --strict` 通过。
6. Windows 和 Ubuntu CI 要求矩阵通过。
7. 必需 stress/soak、heartbeat、close 和资源硬门通过；性能基准按第 24
   节完整披露，历史 FAIL 保留，并有版本 1.2 的用户批准例外。
8. 没有 skipped、xfail、flaky 或偶发 timeout 掩盖并发失败。
9. 没有未结束的测试命令、server、Actor 或后台进程。
10. 所有新公开参数、异常和 timeout 行为已文档化。
11. 逐文件 diff 审阅完成，没有调试代码和无关改动。
12. 工作分支已普通 fast-forward push，远端 HEAD 与本地一致。
13. draft PR 已更新并保持未合并。
14. 永久 `ACTOR_REFACTOR_RESULT.md` 已生成并与最终代码一致。
15. 临时进度账本已在 FINAL finalization commit 中删除。
16. 最终报告列出 commit、测试证据、CI、性能数据和剩余风险。

以下情况禁止提前 complete：

- 只验证正常路径或 150 次成功请求。
- 只证明线程数恒定，没有验证响应归属和资源基线。
- 只在一个操作系统或一个 Python 版本运行。
- 任何关键测试被 skip/xfail。
- close 后仍有线程、socket、Future、waiter 或 poller。
- Actor 内仍存在 blocking socket API。
- 旧 transport 路径仍可能被调用。
- 文档状态机与实际实现不一致。
- 网络不可用导致分支或 CI 未同步。

---

## 26. 实现禁区

1. 禁止固定 10 worker。
2. 禁止 Actor 内 blocking DNS、`create_connection`、`sendall`、`read_exact`。
3. 禁止 Actor 外线程操作网络 socket。
4. 禁止 writable event 未检查 `SO_ERROR` 就判定连接成功。
5. 禁止 timeout 后复用旧 generation。
6. 禁止旧 socket 未 unregister/close 就创建新 generation。
7. 禁止 selector event 只按 fd 匹配。
8. 禁止无界 task、pending、recv 或 push buffer。
9. 禁止 push 满时阻塞 Actor 或静默丢行情。
10. 禁止将 wake byte 当作任务计数。
11. 禁止仅用 queue sentinel 表示 close。
12. 禁止无上限 `zlib.decompress()`。
13. 禁止持状态锁执行 I/O、join、ticket completion 或用户 parser。
14. 禁止 Actor fatal 后静默补线程。
15. 禁止 `join(timeout)` 失败后清除线程引用。
16. 禁止 finalizer 间接引用公开 transport 或新 runtime。
17. 禁止 pool 持 condition 调 Actor、等待 ticket 或 join。
18. 禁止已 push 的 checkpoint commit amend、rebase 或 force-push。
19. 禁止用反复重跑掩盖 flaky 并发测试。
20. 禁止未经用户明确授权合并 PR、打 tag 或发布版本。

---

## 27. 耐久进度与恢复协议

原始实施期间使用 `ACTOR_REFACTOR_PROGRESS.md`；当前 correction cycle 使用
仓库根目录临时文件 `ACTOR_REFACTOR_FIX_PROGRESS.md`。聊天上下文不是进度
来源；Git 和账本才是。FINAL finalization 后 correction 账本由永久
`ACTOR_REFACTOR_RESULT.md` 接替。

### 27.1 每个 checkpoint 必须记录

- objective、scope、non-goals
- branch、base SHA、local HEAD、remote HEAD
- 原始 A00-A09 与 correction F00-F06/FINAL 状态和依赖
- 当前唯一 `in_progress`
- `last_completed`
- `next_exact_action`
- 修改文件和用户已有 dirty paths
- 测试命令、结果、平台、Python 版本和时间
- open decisions、risks、known failures
- failure signature、retry count、解除条件
- push/PR/CI 同步状态

### 27.2 Commit 与 push

- 每个 checkpoint 的代码、测试和账本状态进入同一原子 commit。
- 只显式 stage 任务相关文件，禁止 `git add -A`。
- correction commit trailer 使用 `Fix-Checkpoint: Fxx`；原始历史提交可保留
  `Actor-Checkpoint: Axx`。
- 每个 checkpoint 后普通 push 工作分支。
- 禁止 force-push。
- 已发布 commit 禁止 amend/rebase，修正使用追加 commit。
- push 失败不回滚本地 commit；账本标记 `push_pending` 并继续不依赖网络的工作。
- 网络恢复后幂等重推。

### 27.3 上下文压缩或中断后的固定恢复流程

```text
确认 cwd / branch / git status
-> fetch（禁止 reset）
-> 完整阅读本方案
-> 账本存在则完整阅读账本；账本不存在则完整阅读 result manifest
-> 阅读最新用户请求
-> 核对 git log 中 Actor-Checkpoint trailer
-> 比较 base..HEAD 和未提交 diff
-> 检查是否有仍在运行的测试/server
-> 重跑 last_completed 的最小验收
-> 从 next_exact_action 继续
```

恢复规则：

- 不相信压缩摘要中的“已完成”，必须以代码、commit 和测试证据复核。
- 账本缺失且存在有效 FINAL trailer/result manifest 时，按 final manifest 恢复并核对最终 CI；不能因为账本按计划删除而重建实现。
- 账本缺失且没有有效 final manifest 时，从最近包含账本的 checkpoint commit 读取其内容、重建临时账本并恢复，不能凭记忆继续。
- 代码领先账本时，先验证再补记，禁止重复实现。
- 账本领先代码时，将无证据项目退回 pending。
- 发现中断期间用户修改文件时，视为用户所有，保留并整合。
- 禁止 reset、checkout 丢弃或覆盖不明改动。
- 不重复启动仍在运行的长测试或本地 server。
- 最终 complete 前从零执行一次完整验收，不能拼接中断前后的零散结果。

### 27.4 网络错误

- 同一瞬态错误最多按 5s、15s、45s 退避重试三次。
- fetch/push/PR/CI 网络失败不等于实现 blocked。
- 本地 commit 是原子恢复点；继续可以离线完成的工作。
- 只有权限/凭据缺失、远端不可快进分叉需要用户决策，或同一外部阻塞连续三个目标回合且无替代路径时，才标记 BLOCKED。
- BLOCKED 必须记录错误原文、已尝试方案、当前 commit 和解除条件。

### 27.5 永久 Result Manifest

`ACTOR_REFACTOR_RESULT.md` 至少包含：

- spec revision、性能授权 spec commit、最终程序性 spec commit、base SHA、
  final local/remote SHA
- work branch、draft PR URL 和最终 CI run URL/status
- 原始 A00-A09 与 correction F00-F06/FINAL checkpoint commit 表
- 最终架构与公开兼容性变化摘要
- 所有测试/build/docs/stress/soak 的精确命令、环境、时间和结果
- Windows/Ubuntu、Python 版本矩阵
- 旧/新性能基准表和原始 artifact hash
- 线程、TCP socket、wakeup、selector、ticket、waiter、lease、handle/fd 的前后基线
- close p50/p95/p99、重连次数和跨 generation 命中计数
- push/admission/RX buffer 的观测最大值和配置上限
- skipped/xfail/flaky 审计结果
- base..HEAD diff 审阅结论
- 仍存在的限制、剩余风险和明确未执行操作（未 merge/tag/release）
- 账本删除 commit 和“后续恢复应以本 manifest + Git 为准”的声明

Delivery identity clarification: `SELF` is resolved by the newest first-parent
commit carrying `Fix-Checkpoint: FINAL`; the manifest may not claim to contain
its own SHA. Exact final CI and Pages URLs are post-push evidence and belong in
the persistent PR delivery comment and final report, not in a follow-up commit.

---

## 28. 最终交付物

- 完整 Actor transport 实现。
- 增量 FrameDecoder 和安全解压。
- FIFO pool lease、真正的 pin 和共享 PushBuffer。
- 确定性故障注入测试。
- stress、soak 和性能报告。
- Windows/Linux CI。
- 更新后的架构、API、调试和变更文档。
- 推送到 `actor-transport-refactor` 的未合并 draft PR。
- 永久 `ACTOR_REFACTOR_RESULT.md` 最终验收清单。
- 最终 commit/CI/测试/性能/风险总结。

---

## 29. F10 确定性竞态与测量边界收尾

F10 不重新设计 Actor 架构，只修复 standalone Push 正常关闭覆盖
fatal、Guard 返回旧 epoch fatal，heartbeat 业务测量窗口过宽，并更新
永久证据。

### 29.1 RED-first 和提交顺序

1. 先提交三项 behavior-only RED 证据，并在未修改生产逻辑的 source 上
   确认各自按预期原因失败。
2. 再单独提交 Push/Guard 最小修复，不引入新共享应用锁。
3. 再单独提交 heartbeat 测量协议修复。
4. 每个稳定阶段更新 `ACTOR_REFACTOR_FIX_PROGRESS.md`，使用
   `Fix-Checkpoint:` trailer 提交并普通 push；不 amend、squash、rebase
   或 force-push。

### 29.2 standalone Push fatal 线性化

- RED 必须精确暂停 owner `publish_close(None)` 已读取旧
  `_published_error is None` 但尚未赋值的窗口，不得仅暂停 Actor
  调用父方法之前。
- Actor 写入 `runtime.fatal_error` 后，owner 恢复不得把它覆盖为
  `None`；`push.poll()` 必须抛出同一异常对象。
- 生产修复为正常关闭永不向错误槽写入 `None`：仅当
  `error is not None and self._published_error is None` 时写入。
- 保持 standalone 每 epoch 一个 non-None fatal writer、pooled Push epoch
  resolver 以及 Actor fatal 热路径不等待 Push condition 的现有不变量。

### 29.3 Guard epoch snapshot 线性化

- RED 必须使 `failure()` 保存 epoch 1 publication snapshot，然后在解析
  过程中 configure 到 epoch 2，再延迟发布 epoch 1 fatal；该旧异常
  不得从当前 Guard 返回。
- 快路径首先保存对象身份
  `initial_snapshot = self._publication_snapshot`，并只从该 snapshot
  解包和无锁解析。
- non-None 返回前必须以
  `self._publication_snapshot is initial_snapshot` 作为快路径线性化点；
  identity 变化时不得返回旧结果。
- 慢路径在 Guard 锁内重读完整当前 snapshot，并按当前
  state、epoch、broker、failure cell 和 resolver 解析；该重读是慢路径
  线性化点。
- 保持 Guard -> resolver 锁序，禁止 resolver -> Guard 或
  resolver -> Push 反向获取。

### 29.4 Heartbeat 业务测量窗口

- 每个 phase 必须有唯一 phase ID、起始业务请求计数和目标业务
  请求数。
- 测量窗口从该 phase 首个业务请求开始，由服务端在发送
  该 phase 最后一个业务响应后关闭。
- 只把该窗口内收到的 heartbeat 计入
  `business_window_heartbeat`；phase 结束后、disable fence 前的合法空闲
  heartbeat 可进入总观测计数，但不得进入业务窗口计数。
- 永久断言保持并强化 `business_window_heartbeat == 0`；不得删除
  硬门、改成 `<= 1`、降低 throughput ratio、减少样本或弱化资源/
  唯一响应门。
- 新增确定性测试，模拟最后一个业务响应后、disable 之前
  收到 heartbeat，断言总计数增加但业务窗口计数仍为 0。

### 29.5 F10 验证、证据和交付

- 新增竞态/测量边界测试至少连续运行 20 个独立 pytest 进程。
- 运行 targeted Push/Guard/heartbeat/retirement/lifecycle/stress 矩阵，然后从零
  运行完整 pytest，不得拼接旧结果或用重跑遮盖失败。
- 重跑 10,000 generations / 100,000 requests 压力测试，所有归属/
  唯一性错误为 0，最大业务并发不超过 pool size，关闭后所有
  Actor、lease、waiter、pin、frame、byte、socket/selector/wakeup 资源归零，
  资源样本达到稳定平台。
- 重跑既有冻结配对性能活动并保留 raw artifact、manifest 和
  SHA256。Actor 相对 legacy 的历史例外始终记为
  **FAIL, user-approved exception**，不新增例外或改变门。
- 运行 wheel、sdist、`twine check`、MkDocs strict，并对最终 exact HEAD
  等待 Ubuntu Python 3.10-3.13、Windows Python 3.11/3.13 六个 CI job
  和 Pages strict build 全绿。
- Result 必须引用 revision 1.4 的准确 Plan commit/blob SHA256，F09
  checkpoint 表补入 `ee5a995`，并保留旧失败历史。明确
  `5925c02` 的 FINAL/CLEAN 已被 F10 确定性 RED 和完整测试
  失败推翻。
- FINAL 前至少两路独立代码复审和一路证据复审必须 CLEAN；
  任何 P1/P2 未闭环时不得声明完成。
- FINAL 提交删除临时账本并使用 `Fix-Checkpoint: FINAL`。本地、远端
  分支和 PR HEAD 必须一致；PR #12 保持 OPEN、Draft、未合并。
- exact-head CI/Pages 后在 PR #12 发布持久 F10 Final Delivery 评论，
  supersede 旧 F09 评论，包含 final HEAD、production SHA、完整测试数、
  stress/performance artifact hash、CI/Pages URL、状态和未合并声明。
