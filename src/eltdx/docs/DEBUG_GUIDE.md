# 排错指南

这份文档用于定位安装、native ABI、主站连接、请求超时、push、关闭和协议字段问题。

## 安装和 native ABI

普通受支持平台应安装 `cp310-abi3` wheel，不需要本机 Rust：

```bash
python -m pip install -U eltdx
python -c "import eltdx, eltdx._native; print(eltdx.__version__, eltdx._native.ABI_VERSION)"
```

如果没有匹配 wheel，pip 会尝试从 sdist 构建，此时需要项目锁定的 Rust 工具链。3.0 没有纯 Python 7709 fallback。

`ImportError: eltdx native ABI mismatch` 表示 Python 文件和 `_native` 二进制来自不同构建。卸载所有旧副本，确认虚拟环境和 `python -m pip` 属于同一解释器，再重新安装；不要复制单独的 `.so` / `.pyd` 文件。

## 连不上主站

先检查 TCP 可达性：

```python
from eltdx.hosts import refresh_server_ranking

for item in refresh_server_ranking(timeout=1.2):
    print(item.host, item.ok, item.latency_ms, item.error)
```

该调用会测速包内全部43台候选服务器并更新当前用户的 `tdx_server_ranking.json`。大部分主站失败时检查网络、防火墙、代理和 7709 端口；也可以显式传 `host` / `hosts`。当前测速只说明 TCP connect 成功，不保证业务响应。

默认候选主站来自包内 `tdx_server.json`；文件不可用时才使用内置列表。用户测速排名与安装包名单分开保存，因此重新安装或升级不会主动清空排名。

默认构造 `TdxClient` 时，会先探测普通 43 台和集合竞价、资金流向专用 35 台候选服务器并缓存排名；构造耗时可能包含探测失败地址的 `probe_timeout` 等待，尚未建立业务连接或完成握手。`probe_hosts=False` 可跳过这一阶段。同一客户端后续不会自动重测；手动刷新磁盘排名只影响之后新建的客户端或 transport，不替换现有客户端的排名快照。

## Timeout 阶段

请求使用一个 absolute deadline，不会在每个阶段重新获得完整 timeout。错误 context 中的 phase 常见值：

| phase | 含义 |
| --- | --- |
| `queue` | 等待 permit 或 Slot lease |
| `connect` | TCP 连接或 endpoint failover |
| `handshake` | 新 generation 的 `0x000d` 验证 |
| `send` | 请求帧发送 |
| `response` | 等待匹配响应 |
| `cancel_confirm` | signal 后等待本地取消/retirement 确认 |
| `close` | 关闭和本地资源证明 |

自定义 hostname 的首次 DNS 是兼容例外，位于 request timeout 外。需要严格 deadline 时使用数字 IP 或预先解析的主站。

`PoolBusyError` 表示 `pool_size + max_pending_requests` 的有界 admission 已满，应降低调用并发、提高合理容量或让 caller 做退避，而不是无限重试。

## Heartbeat

真实 socket 默认每 30 秒发送一次低优先级 `0x0004` 心跳。业务请求或 pin waiter 存在时心跳会顺延，不会抢占业务 Slot。

```python
client = TdxClient(heartbeat_interval=60)
client = TdxClient(heartbeat_interval=None)  # disable
```

建议始终使用 context manager；退出时 native Engine 会停止 admission、retire socket、join Slot task 和 runtime thread。

## Diagnostics

诊断快照不触发网络 I/O：

```python
from eltdx import TdxClient

with TdxClient(pool_size=4, timeout=3) as client:
    snapshot = client.transport.diagnostics
    print(
        snapshot.state,
        snapshot.epoch,
        snapshot.runtime_workers,
        snapshot.server_count,
        snapshot.raw_bytes,
        snapshot.decoded_bytes,
    )
    for slot in snapshot.actors:
        print(slot.tcp_state, slot.tcp_generation, slot.pending_depth)
```

为兼容 2.x，字段仍叫 `actors` / `ActorSnapshot`；它们映射 Rust Slot task，不是 Python Actor。正常空闲时 `pending_depth`、broker waiter 和 active lease 应为 0，`stale_event_count` 通常为 0。`reconnect_count` 是该 Slot 在当前 epoch 已成功 retirement 的 TCP generation 数。`raw_bytes` / `decoded_bytes` 不应超过对应 `*_max_bytes`，正常关闭后必须归零；`*_peak_bytes` 用于判断实际负载是否接近上限。

`FAILED_CLOSING` 表示 1 秒硬门内无法证明全部本地资源结束。修复阻塞源后可以再次调用同一个 `close()`；清理成功后状态为 `FAILED_CLOSED`，该实例仍不能 reopen。正常 `STOPPED` 才能 reopen。

## Push overflow

未匹配但合法的同 generation 帧进入 Engine push buffer：

```python
with TdxClient(timeout=3) as client:
    client.quotes.refresh("sz000001")
    frame = client.quotes.poll_push(timeout=0.2, parse=False)
    parsed = client.quotes.drain_pushes(parse=True)
```

`PushOverflowError` 只报告一次 pending gap，表示最旧帧已被丢弃。记录累计 `push_dropped` 后继续读取保留的新帧。close 成功后旧 epoch push 和 gap 都被清空。

## 原始字段

支持的业务接口可传 `include_raw=True`：

```python
with TdxClient(timeout=3) as client:
    series = client.bars.get("sz000001", count=5, include_raw=True)

print(series.raw_payload.hex())
print(series.bars[0].record_hex)
```

`raw_payload` 是响应 payload，`record_hex` 是单条记录原始 bytes，`decoded_payload` 是部分 XOR 接口的解码结果。exact fixture 使用带类型 canonical JSON 和 IEEE bits，不应使用近似 float 比较掩盖协议差异。

## Signal 和 fork

主线程提交请求使用非阻塞 ingress，队列满时立即抛 `PoolBusyError`；进入 native 等待后每25 ms检查 Python signal。`KeyboardInterrupt` 后 Engine 只等待最多1秒的本地取消确认，不等待服务器回应；清理异常保存在原中断的 `__cause__`。

fork 子进程复用父进程 client 会立即收到错误：

```python
# Create a new TdxClient inside the child process.
```

父进程 Engine 的锁、runtime 和 socket 都不能在子进程继续使用。`multiprocessing` 的 spawn 模式同样应在 child 内创建 client。
