# 跨语言 HTTP / WebSocket 网关

FastAPI 网关用于从 Java、Go、C#、Node.js 等语言调用 `eltdx`。Python 用户继续直接使用 `TdxClient` 即可。

## 安装和启动

普通安装不包含网关依赖：

```bash
pip install eltdx
```

需要网关时再安装 `http` 可选依赖：

```bash
pip install "eltdx[http]"
eltdx-http
```

默认监听 `127.0.0.1:8000`。启动后可访问：

| 地址 | 用途 |
| --- | --- |
| `GET /health` | 服务状态和版本 |
| `GET /methods` | 通用 RPC 方法和 WebSocket 专用方法列表 |
| `POST /rpc` | HTTP JSON RPC |
| `WebSocket /ws` | WebSocket RPC 和实时行情订阅 |
| `GET /docs` | FastAPI 交互文档 |

## HTTP 调用

`method` 与 Python API 名称一致，`params` 对应方法参数。例如查询前复权日 K：

```http
POST /rpc
Content-Type: application/json

{
  "id": 1,
  "method": "bars.get",
  "params": {
    "code": "sz000001",
    "period": "day",
    "count": 120,
    "adjust": "qfq"
  }
}
```

成功响应：

```json
{
  "id": 1,
  "ok": true,
  "result": {}
}
```

所有公开的 `TdxClient` 模块化 API 都可按相同方式调用，例如：

- `quotes.get_snapshots`
- `bars.get`
- `minutes.today`
- `trades.all_today`
- `corporate.capital_changes`
- `corporate.adjustment_factors`
- `f10.company_profile`
- `helpers.shortline_indicators`

返回的 dataclass、日期和二进制字段会转换为 JSON；二进制字段使用十六进制字符串。

## WebSocket 调用

连接 `ws://127.0.0.1:8000/ws` 后，可以发送与 HTTP 相同的 RPC 消息。普通方法仍然是一次请求对应一次响应：

```json
{
  "id": 2,
  "method": "quotes.get_snapshots",
  "params": {"codes": ["sz000001", "sh600000"]}
}
```

HTTP 与 WebSocket 共用同一个长期运行的 `TdxClient` 和 7709 连接池，不会为每次调用重新连接主站。

## 实时行情订阅

`quotes.subscribe` 仅用于 WebSocket。它先通过原生 `0x0547` 返回一份五档行情基线，然后转发后续增量：

```json
{
  "id": 3,
  "method": "quotes.subscribe",
  "params": {"codes": ["sz000001", "sh600000"]}
}
```

订阅响应中的 `initial` 是建立订阅时的基线：

```json
{
  "id": 3,
  "ok": true,
  "result": {
    "subscription_id": "SUBSCRIPTION_ID",
    "codes": ["sz000001", "sh600000"],
    "initial": {}
  }
}
```

之后连接会收到行情事件：

```json
{
  "event": "quote",
  "subscription_id": "SUBSCRIPTION_ID",
  "data": {}
}
```

这是 7709 主站的原生增量，不是网关按固定间隔轮询。主站有更新时才会发送，频率由主站决定。单个订阅最多 100 只证券。

取消订阅：

```json
{
  "id": 4,
  "method": "quotes.unsubscribe",
  "params": {"subscription_id": "SUBSCRIPTION_ID"}
}
```

客户端消费过慢时，网关会保留最新行情并淘汰积压的旧行情；普通 RPC 响应不会被行情挤掉。需要逐条无缺口数据时，应使用适合落盘和重放的数据链路。

## 启动参数

```bash
eltdx-http \
  --host 127.0.0.1 \
  --port 8000 \
  --server-count 2 \
  --connections-per-server 2
```

| 参数 | 默认值 | 含义 |
| --- | --- | --- |
| `--host` | `127.0.0.1` | 网关监听地址 |
| `--port` | `8000` | 网关监听端口 |
| `--tdx-host` | 自动选择 | 单个 7709 主站，格式为 `host:port` |
| `--tdx-hosts` | 自动选择 | 多个 7709 主站，以逗号分隔 |
| `--timeout` | `8` | 7709 请求超时秒数 |
| `--server-count` | `2` | 选用的主站数量 |
| `--connections-per-server` | 项目默认值 | 每个主站的连接数 |
| `--pool-size` | 项目默认值 | 连接池大小兼容参数 |
| `--log-level` | `info` | Uvicorn 日志级别 |

网关应保持单进程运行。不要使用多个 Uvicorn worker；每个 worker 都会各自创建一套 `TdxClient` 和连接池。

默认只监听本机。需要让其他机器访问时，应在反向代理或内网网关中配置鉴权、TLS 和访问限制，再修改监听地址。

## 错误响应

```json
{
  "id": 1,
  "ok": false,
  "error": {
    "type": "GatewayRequestError",
    "message": "method must be a non-empty string"
  }
}
```

参数或 JSON RPC 格式错误返回 HTTP `400`，未知方法返回 `404`，主站连接错误返回 `502`，网关内部错误返回 `500`。WebSocket 错误使用相同的 JSON 结构。
