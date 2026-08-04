# eltdx 1.0 架构说明

## 项目说明

本仓库是 `eltdx` 的 Python 项目根目录，用于开发、测试和发布。

协议文档是 `7709` 二进制接口的证据来源，发布版以仓库内公开文档为准。

历史实现中已验证过的协议解析、测试样本和 transport 组件，已按模块整理到当前项目。

## 分层

| 层 | 目录 | 职责 |
| --- | --- | --- |
| 产品入口 | `client.py` | 暴露 `TdxClient`，挂载各业务 API |
| 业务 API | `api/` | 面向使用者的调用方法，比如快照、K线、分时 |
| 传输层 | `transport/` | socket 连接、重连、收包、请求执行 |
| 协议层 | `protocol/` | 二进制帧、命令注册、payload 编解码 |
| 模型层 | `models/` | 对外返回的数据结构 |
| 测试 | `tests/` | 单元测试、协议样本回放测试 |

## 调用链

以批量行情快照为例：

```text
用户代码
  -> TdxClient.quotes.get_snapshots(["sz000001"])
  -> QuoteApi
  -> command registry: snapshots -> 0x054c
  -> transport.execute(0x054c, payload)
  -> protocol command builder
  -> socket frame
  -> protocol parser
  -> Quote models
```

当前默认 `TdxClient()` 使用真实 7709 transport；内存实现通过 `TdxClient.in_memory()` 显式启用，用来固定 API 形状和测试行为。真实连接的每个 slot 都有一个长期存在的 `ConnectionActor`：它独占一个非阻塞 TCP socket、一个 selector 和一个 socketpair wakeup。没有 reader 线程、heartbeat 线程、共享可变 socket 或 round-robin 请求分派路径。

真实连接调用链：

```text
TdxClient() / TdxClient.from_hosts()
  -> PooledSocketTransport
  -> FIFO admission -> SlotLease
  -> SocketTransport synchronous facade
  -> ConnectionActor (one thread per slot)
  -> connect_ex / SO_ERROR / partial send / incremental FrameDecoder
  -> immutable FrameEnvelope
  -> caller-side parse_command_response()
```

Actor 是 TCP socket 的唯一所有者。它只使用非阻塞 `connect_ex()`、selector、`send()` 和增量 `recv()`；每次 TCP 重连只替换 generation，不替换 Actor thread。selector 事件同时校验 runtime epoch、TCP generation 和 socket identity，响应还校验 lease、msg id 与 msg type，旧 generation 永远不能完成新请求。

普通池请求先进入全池 FIFO admission，只有拿到空闲 `SlotLease` 后才创建 `RequestTicket`。wire terminal 时 Actor 先使 ticket terminal、exact-once 归还普通 lease，再唤醒 caller；业务 parser 因而不会占用 slot。`pin()` 返回 epoch-scoped proxy，在 context 生命周期内独占一个 slot；proxy 的 `close()` 只归还 pin lease，不会关闭共享 socket。

每个 pool epoch 还拥有一个只会从 unset 变为 set、不会复位的 retirement Event；`LeaseBroker` 与 `PushBuffer` 各自另有永久 local-close Event。任一 Event set 后，新的 lease、pin reservation 和 push frame 都不能发布。Broker admission 在修改 canonical container 和 immutable snapshot 后再次检查 retirement，若终止已发生就在同一个 condition 内回滚并 drain waiter、lease、idle slot 与 pin reservation。`_closed` / `_drained` 只表示 condition 内的真实清理已经完成；第一次 close 因 deadline 失败后，可以对同一对象重试完成清理。

Actor fatal 先写自己的 single-writer reason cell 和当前 epoch 的 bounded fatal-handle identity，再 set epoch retirement，发布 Broker/Push local close，唤醒 waiter、pin 和 push 的 immutable snapshot，并向同 epoch 的所有 Actor 发布 stop+wakeup。owner 路径通过该 epoch 专属的 sticky resolver 选择一次 exact exception object；Guard、Push、`finish_epoch()` 和重复读取都复用同一对象，旧 epoch 的 resolver 不会被新 epoch 读取。这样 owner 即使在 retirement publication 与 runtime registration 之间 finalization，也能恢复真实 fatal reason。该路径不等待 Pool、Broker、Proxy、Push 或 sibling Actor 的应用锁；`threading.Event.set()` 与 socket wakeup 是允许的运行时信号，因此不把它描述为严格 wait-free，也不需要后台 janitor 或额外线程。

任何持有 Broker condition 的 owner 若在修改或读取 canonical state 后观察到 retirement，都负责执行同一个幂等 drain，以接替未能取得 condition 的非阻塞 fatal publisher。Push `poll()` / `drain()` 在 pop/copy 后再次检查 fatal/error/retirement；fatal 赢得临界窗时清空残留并优先返回 terminal/error，而 `close(None)` 的普通缓冲消费语义保持不变。

每个 Actor mailbox 的逻辑容量为 1，pool admission 和 push buffer 都有硬上限。未匹配合法帧进入 pool epoch 共享的 `PushBuffer`；满时丢弃最旧帧并在下一次读取显式报告 gap。Actor 每轮 recv/parse 有预算，push flood 不能饿死控制、deadline 或业务响应。

正常 `close()` 先停止 admission、关闭旧 push buffer 并唤醒全部 Actor，再在锁外 join。1 秒内无法确认退出时 transport 进入 `FAILED_CLOSING`，保留 runtime 引用并抛 `TransportCloseTimeoutError`；最终进入不可 reopen 的 `FAILED_CLOSED`。正常 `STOPPED` 才允许新 epoch reopen。finalizer 只捕获旧 runtime 并执行非阻塞 stop+wakeup，不会影响 reopen 后的新 runtime。

`probe_hosts=True` 时，连接池创建前会先对候选主站做 TCP connect 测速，把可连通、延迟低的主站排在前面。测速只代表 TCP 连接成功，不等于业务命令一定成功。

不传 `host` / `hosts` 时，主站列表从包内 `tdx_server.json` 读取；如果配置文件不可用，会退回代码内置列表。数字 IP 和已缓存 endpoint 的一个 `timeout` 覆盖 admission、连接、握手、发送、响应与最多一次 retry。自定义 hostname 的首次标准库 DNS 在 Actor 外 preflight，不能可靠取消，也不占 lease/Actor；解析完成后会重新检查 epoch。真实 socket 默认在空闲时由 Actor timer 发送 `0x0004` 心跳，业务活跃时会顺延。

## 目录整理思路

历史实现承担了较多职责，当前项目按下面的方式重新拆分：

| 旧问题 | 新处理 |
| --- | --- |
| `client.py` 太厚，混合连接、分页、复权、业务辅助逻辑 | `TdxClient` 只做门面，业务放 `api/` |
| `protocol/model_*.py` 同时做请求构造和响应解析 | 新项目按命令模块拆成 builder / parser |
| `models.py` 所有模型在一个文件 | 新项目按业务拆到 `models/` |
| 研究脚本、工具服务和扩展计算与核心行情接口耦合 | 放到扩展或工具目录 |

## 1.0 核心优先级

首版优先接入：

| 能力 | 命令 |
| --- | --- |
| 握手 / 心跳 | `0x000d`, `0x0004` |
| 代码数量 / 代码表 | `0x044e`, `0x044d` |
| 批量快照 | `0x054c` |
| 分类行情 | `0x054b` |
| 行情增量刷新 | `0x0547` |
| K线 | `0x052d` |
| 当日分时 | `0x0537` |
| 历史分时 | `0x0fb4` |
| 当日成交明细 | `0x0fc5` |

上述主动请求类能力已经接入真实请求和解析。`0x0547` 已完成协议层构包、响应解析、单次刷新和推送队列；实盘单次最多返回 100 个代码，因此构包层拒绝更大的请求。

第二阶段接入：

| 能力 | 命令 |
| --- | --- |
| 集合竞价 | `0x056a` |
| 历史成交明细增强 | `0x0fc6` |
| 近期历史分时 | `0x0feb` |
| 分时副图 | `0x051b` |
| 小走势图 | `0x0fd1` |
| 股本变迁 / 财务 / 涨跌停限制 | `0x000f`, `0x0010`, `0x0452` |

第二阶段主动查询类能力也已经完成首版接入。架构重点转向推送 transport、样本回放测试和发布前 API 打磨。
