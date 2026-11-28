---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 服务器文件分片

## 作用

读取服务器文件的一部分。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, DownloadChunkQuery, RemoteFileName};

fn query(client: &MacClient, filename: RemoteFileName) -> Result<()> {
    let query = DownloadChunkQuery::new(filename);
    let result = client.download_chunk(&query, &OperationOptions::default())?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

传入服务器文件名，可设置偏移和分片大小。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

用 `with_index` 设置块序号，`with_offset` 设置字节位置，`with_size` 设置读取大小。

## 返回

返回 `Result<Vec<u8>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

返回这一块的字节，可用 `len()` 查看实际长度。需要 SDK 确认完整下载时，使用 [服务器文件下载](mac-download-file.md)。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.download_chunk)
