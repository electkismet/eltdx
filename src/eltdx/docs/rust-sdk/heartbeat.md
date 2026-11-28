---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 7709 心跳

## 作用

向行情主站发送一次心跳请求并读取应答。需要检查主站能否响应时可以手动调用。

| 项目 | 内容 |
| --- | --- |
| 调用方法 | `client.session().heartbeat(&options)` |
| 返回类型 | `Result<HeartbeatAck>` |
| 功能开关 | 默认功能，无需额外开启 feature |

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`，然后运行以下程序。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();

    let result = client.session().heartbeat(&options);
    let closed = client.close();
    let ack = result?;

    println!("主站日期：{:?}", ack.server_date());
    closed
}
```

这个请求需要能连接到行情主站。示例在检查请求结果之前关闭客户端，以便请求失败时也释放资源。

## 参数

| 参数 | 含义 |
| --- | --- |
| `options: &OperationOptions` | 本次操作的选项。`OperationOptions::default()` 使用默认配置，也可以按需设置超时时间。 |

## 返回字段

请求成功时返回 `HeartbeatAck`，通过方法读取字段。

| 方法 | 含义 |
| --- | --- |
| `server_date()` | 解析后的服务端日期，无法取得有效日期时为 `None`。 |
| `server_date_raw()` | 服务端返回的原始日期数值。 |
| `reserved()` | 应答中保留的原始数据。 |
| `meta()` | 本次结果的来源等元数据。 |

请求失败时返回错误，可以用 `?` 向上返回，或使用 `match` 自行处理。`client.session().ping()` 是本地应答，要检查主站响应应使用心跳或握手。

[查看 HeartbeatAck 类型参考](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/types/session/struct.HeartbeatAck.html)
