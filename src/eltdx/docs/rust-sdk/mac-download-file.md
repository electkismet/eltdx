---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 服务器文件完整下载

## 作用

下载完整服务器文件。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, DownloadFileQuery, RemoteFileName};

fn query(client: &MacClient, filename: RemoteFileName) -> Result<()> {
    let query = DownloadFileQuery::new(filename);
    let result = client.download_file(&query, &OperationOptions::default())?;
    println!("{result:?}");
    Ok(())
}
```

### 其他调用

`download_file` 返回 `DownloadResult`，通过 `data`、`completion` 和 `next_offset` 获取字节、完成状态和后续偏移。`download_complete` 要求从偏移 0 读取，确认完整后返回 `Bytes`。在关闭客户端前调用。

```rust
let options = OperationOptions::default();
let bytes = client.download_complete(&query, &options)?;
println!("完整文件 {} 字节", bytes.len());
```

## 参数

传入服务器文件名，可用 `with_options` 传入 `DownloadOptions`，设置最大字节数、分块大小、最大块数、期望大小或起始偏移。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

## 返回

`download_file` 返回完成状态和已读字节。`download_complete` 遇到截断或结果不完整时返回错误。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.download_file)
