# Rust SDK 与原生工具

本指南适用于 `v3.4.0-beta.2`。Rust SDK 提供同步 API，可直接用 Cargo 安装。原生 CLI、HTTP/WebSocket 网关和 MCP 服务可从该版本的源码构建，运行时无需 Python。[测试版发布说明](https://github.com/electkismet/eltdx/releases/tag/v3.4.0-beta.2)

## 安装 SDK

使用 Rust 1.89 或更新版本。在应用的 `Cargo.toml` 中添加以下依赖。

```toml
[dependencies]
eltdx = "=3.4.0-beta.2"
```

默认支持 7709 行情，包括证券目录、快照、K 线、分时、成交、竞价和订阅。需要其他数据时，在依赖声明中添加相应的 Cargo feature。

| Feature | 能力 |
| --- | --- |
| `f10` | 7615 公司资料、财务、资讯和历史资金流 |
| `offline` | 本地行情文件、股本和专业财报 |
| `ex` | EX 扩展行情 |
| `mac` | MAC 与 MAC-EX，包含 `ex` |
| `serde` | 支持 Serde 的数据类型 |
| `full` | 启用以上全部 feature |

## 查询行情快照

```rust
use std::time::Duration;

use eltdx::{OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default().with_timeout(Duration::from_secs(8));

    let result = client.quotes().snapshots(&[id], &options);
    let closed = client.close();

    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

`build()` 创建客户端配置，行情查询时才建立所需连接。`snapshots()` 按输入返回批量结果，使用 `state()` 检查每项状态。需要在部分证券无数据时仍取得其他结果，可调用 `snapshots_with_policy()`。[行情接口](https://docs.rs/eltdx/3.4.0-beta.2/eltdx/services/struct.QuoteService.html)

## API 参考

- 查看服务入口、类型、方法签名和 Cargo feature。[Rust API 参考（docs.rs）](https://docs.rs/eltdx/3.4.0-beta.2/eltdx/)
- 查看客户端配置、服务入口和会话生命周期。[`TdxClient`](https://docs.rs/eltdx/3.4.0-beta.2/eltdx/struct.TdxClient.html)
- 查看 HQ、F10、EX、MAC 和离线读取的逐服务示例。[Rust 使用指南与示例](https://github.com/electkismet/eltdx/blob/v3.4.0-beta.2/docs/rust-sdk/service-examples.md)

## 构建原生工具

```sh
git clone --branch v3.4.0-beta.2 https://github.com/electkismet/eltdx.git
cd eltdx
cargo build --locked --release -p eltdx-tools --bins
```

| 工具 | 用法 |
| --- | --- |
| CLI | 运行 `./target/release/eltdx --help` 查看子命令。 |
| HTTP/WebSocket | 运行 `./target/release/eltdx-http --host 127.0.0.1 --port 8000`，打开 `/docs` 查看 HTTP 接口，连接 `/ws` 后调用 `quotes.subscribe` 订阅行情。 |
| MCP | 在 MCP 客户端中配置 `target/release/eltdx-mcp` 的绝对路径，通过 stdio 启动。 |

Windows 的可执行文件带 `.exe` 后缀。Rust 网关的部分原始响应字段与 Python 网关有差异。MAC-EX、联网场外基金和 MCP 宏观工具需设置 `ELTDX_MAC_EX_LOGIN_RULES`。[登录规则配置说明](https://github.com/electkismet/eltdx/tree/v3.4.0-beta.2#从源码构建-rust-工具)
