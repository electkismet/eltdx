---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 7709 握手

## 作用

向行情主站发送握手请求，读取主站名称、服务端时间和交易时段等信息。普通行情查询会处理连接初始化，通常只有需要检查主站信息时才需要手动调用。

| 项目 | 内容 |
| --- | --- |
| 调用方法 | `client.session().handshake(&options)` |
| 返回类型 | `Result<HandshakeInfo>` |
| 功能开关 | 默认功能，无需额外开启 feature |

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`，然后运行以下程序。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();

    let result = client.session().handshake(&options);
    let closed = client.close();
    let info = result?;

    println!("主站名称：{}", info.server_name());
    println!("服务端时间：{:?}", info.server_datetime());
    println!("交易时段：{:?}", info.session_minutes1());
    closed
}
```

这个请求需要能连接到行情主站。`close()` 释放客户端资源，示例在检查请求结果之前调用它，以便请求失败时也执行关闭操作。

## 参数

| 参数 | 含义 |
| --- | --- |
| `options: &OperationOptions` | 本次操作的选项。`OperationOptions::default()` 使用默认配置，也可以按需设置超时时间。 |

## 返回字段

请求成功时返回 `HandshakeInfo`，通过方法读取字段。返回 `None` 表示对应的日期或时间没有可用值。

| 方法 | 含义 |
| --- | --- |
| `server_datetime()` | 服务端日期和时间。 |
| `server_date1()`、`server_date2()` | 服务端提供的两组日期。 |
| `session_minutes1()`、`session_minutes2()` | 服务端提供的两组交易时段信息。 |
| `server_name()` | 主站名称。 |
| `product_tag()` | 产品标识。 |
| `unknown_time1()`、`unknown_time2()` | 含义尚未确认的原始时间相关值。 |
| `flags()`、`control_tail()` | 保留的原始标志和控制数据。 |
| `meta()` | 本次结果的来源等元数据。 |

请求失败时返回错误，可以用 `?` 向上返回，或使用 `match` 自行处理。日期由主站提供，不能直接当作已确认的交易日。

[查看 HandshakeInfo 类型参考](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/types/session/struct.HandshakeInfo.html)
