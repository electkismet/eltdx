# 排错指南

这份文档用来定位三类问题：连不上主站、数据返回异常、协议字段需要核对。

## 连不上

先确认 TCP 连接状态：

```python
from eltdx.hosts import DEFAULT_HOSTS, probe_hosts

results = probe_hosts(DEFAULT_HOSTS[:10], timeout=1.2)
for item in results:
    print(item.host, item.ok, item.latency_ms, item.error)
```

如果大部分主站都连不上，优先检查网络、防火墙、代理和当前环境是否允许直连 `7709` TCP 端口。

如果只是个别主站慢，可以打开测速排序：

```python
from eltdx import TdxClient

with TdxClient(probe_hosts=True, timeout=3) as client:
    print(client.transport.hosts[:5])
    print(client.helpers.full_quotes("sz000001")[0].last_price)
```

也可以手动指定主站：

```python
with TdxClient(host="116.205.183.150:7709", timeout=3) as client:
    print(client.codes.count("sz"))
```

## 主站列表

默认主站来自包内 `tdx_server.json`：

```python
from eltdx.hosts import DEFAULT_HOSTS, load_server_config

print(load_server_config()["schema_version"])
print(DEFAULT_HOSTS[:3])
```

如果包内 JSON 缺失或格式坏了，客户端会退回代码内置主站列表。

## 长时间空闲

真实 socket 默认每 30 秒发一次 `0x0004` 心跳。普通短脚本不用处理。

需要调整：

```python
from eltdx import TdxClient

client = TdxClient(heartbeat_interval=60)
```

需要关闭：

```python
client = TdxClient(heartbeat_interval=None)
```

程序需要创建较多客户端实例时，使用 `with TdxClient(...) as client:` 自动关闭 Actor、TCP socket、selector 和 wakeup。心跳由 Actor timer 驱动，不会额外创建 heartbeat 线程。

## 请求超时

超时通常有三种原因：

| 现象 | 处理 |
| --- | --- |
| 第一次请求就超时 | 换主站或打开 `probe_hosts=True` |
| 偶发超时 | 查看异常中的 `queue`、`connect`、`handshake`、`send` 或 `response` 阶段，再决定是否增大 `timeout` |
| 长时间运行后超时 | 确认没有忘记关闭旧客户端，必要时降低连接池数量 |

## 字段对不上

需要看原始 payload 时，打开 `include_raw=True`：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    kline = client.bars.get("sz000001", period="day", count=5, include_raw=True)

print(kline.raw_payload.hex())
print(kline.bars[0].record_hex)
```

常见 raw 字段：

| 字段 | 意思 |
| --- | --- |
| `raw_payload` | 当前响应 payload 原始 bytes |
| `record_hex` | 单条记录原始十六进制 |
| `decoded_payload` | 少数 XOR 编码接口解码后的 payload |

这些字段主要用于抓包对照和排查解析问题。

## 推送队列

部分接口会有服务端主动推送帧。真实 transport 会把没匹配到请求的帧放进本地队列：

```python
from eltdx import TdxClient

with TdxClient(timeout=3) as client:
    client.helpers.full_quotes("sz000001")
    print(client.transport.pending_push_count)
    print(client.transport.drain_pushes(parse=True))
```

主动查询可以直接使用 `client.quotes.refresh()`，服务端主动推送帧可以通过 `poll_push()` / `drain_pushes()` 读取。

如果读取时收到 `PushOverflowError`，说明有界 buffer 已丢弃旧帧并留下 gap。记录异常中的累计丢弃数，然后继续 `drain_pushes()`；不要把它当作普通空队列处理。

## Actor 诊断

真实 transport 提供只读诊断快照，不会触发网络 I/O：

```python
with TdxClient(pool_size=4, timeout=3) as client:
    snapshot = client.transport.diagnostics
    print(snapshot.state, snapshot.epoch)
    print(snapshot.broker)
    for actor in snapshot.actors:
        print(actor.runtime_epoch, actor.tcp_generation, actor.pending_depth, actor.reconnect_count)
```

单连接 `SocketTransport` 的 `diagnostics` 包含 Actor snapshot 和 push frame/byte/drop 计数；pool snapshot 还包含 FIFO waiter、active lease 和所有 slot Actor。`stale_event_count` 应保持为 0。发生 Actor fatal 时 pool 会 fail-closed，push poller 会收到对应 `TransportError`，并且需要关闭旧 client 后创建新实例。

fatal publication 一旦发生，epoch sticky resolver 先确定 exact exception object；push terminal/error 优先于已经缓冲或正在 pop/copy 的 frame，旧 epoch 数据不会在错误之后继续可见。Broker diagnostics 在任一 owner 取得 condition 并观察到 retirement 后应显示 waiter、pin waiter、active lease 和 idle slot 都已 drain；Push diagnostics 应显示 frame/bytes 为 0。若第一次 `close()` 因内部 condition 被占用而抛 `TransportCloseTimeoutError`，释放占用后应重试同一个 `close()`，以完成保留 runtime、Broker 和 PushBuffer 的实际清理。正常 `publish_close(None)` 不触发 fatal lazy drain，已有缓冲仍可按序消费。

## Close And Reopen

正常 close 后可在同一实例上再次 `connect()` 或发起业务请求，系统会创建新 runtime epoch。`TransportCloseTimeoutError` 不是普通超时：旧 runtime 仍被保留以便诊断，实例进入 `FAILED_CLOSING` / `FAILED_CLOSED`，不能 reopen。检查 thread dump、`diagnostics` 和 socket 资源后创建新 client。
