---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 连接与主机管理

## 作用

创建 MAC 客户端，管理连接并探测主机。

## 示例

在 `Cargo.toml` 中添加 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收服务器地址，由调用方提供。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::ex::hosts::{DiscoveryOptions, Endpoint};
use eltdx::mac::MacClient;

fn run(endpoint: Endpoint) -> Result<()> {
    let client = MacClient::builder().with_endpoints(vec![endpoint.clone()]).build()?;
    let options = OperationOptions::default();
    let operation = (|| -> Result<()> {
        for probe in client.probe_endpoints(&[endpoint], &DiscoveryOptions::default(), &options)? {
            println!("{} {:?}", probe.endpoint(), probe.status());
        }
        client.connect(&options)
    })();
    let closed = client.close();
    operation?;
    closed
}
```

## 参数和使用

`endpoint` 是实际服务器地址，可以用 `"IP:端口".parse()?` 创建。`builder` 创建客户端，`connect` 建立连接，完成后调用 `close`。`probe_endpoints` 可在连接前探测提供的服务器地址。默认探测会验证该协议的数据可用性，`data_ready()` 为真时表示探测通过。`latency()` 和 `error_kind()` 分别提供可用的耗时或错误类别。

## 返回

`connect` 和 `close` 成功时返回 `()`。主机探测返回 `Vec<HostProbe>`，可查看各主机探测结果。 网络和参数错误通过 `Result` 返回。

- `配置客户端` 调用 `MacClient::builder()`，返回 `MacClientBuilder`。
- `连接` 调用 `MacClient::connect(&self, options: &OperationOptions)`，返回 `Result<()>`。
- `探测主机` 调用 `MacClient::probe_endpoints(&self, endpoints: &[Endpoint], discovery: &DiscoveryOptions, options: &OperationOptions)`，返回 `Result<Vec<HostProbe>>`。
- `关闭` 调用 `MacClient::close(&self)`，返回 `Result<()>`。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.builder)
