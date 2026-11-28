---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 服务器文件元信息

## 作用

读取服务器文件元信息。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, FileMetaQuery, RemoteFileName};

fn query(client: &MacClient, filename: RemoteFileName) -> Result<()> {
    let query = FileMetaQuery::new(filename);
    let result = client.file_meta(&query, &OperationOptions::default())?;
    println!("大小 {} 字节，摘要 {}", result.size(), result.hash());
    Ok(())
}
```

## 参数

传入服务器文件名。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`FileMetaQuery` 接收服务器文件名。用 `with_offset` 修改请求偏移。

## 返回

返回 `Result<FileMeta>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

使用 `size()`、`offset()`、`hash()` 读取文件大小、偏移和服务端提供的摘要。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.file_meta)
