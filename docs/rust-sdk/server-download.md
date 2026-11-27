---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 服务器文件下载

## 作用

按块读取服务器文件。download_complete() 可用于要求文件下载完整的场景。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.resources().download_complete("zhb.zip", 16 * 1024 * 1024, &options);
    let closed = client.close();
    let bytes = result?;
    println!("完整文件 {} 字节", bytes.len());
    closed
}
```

### 其他调用

只希望在给定上限内读取数据时调用 `download_file`。若必须得到整个文件，用上面的 `download_complete`，它会在无法确认完整时返回错误。在关闭客户端前调用。

```rust
let part = client.resources().download_file("zhb.zip", 16 * 1024 * 1024, &options)?;
println!("已读 {} 字节，状态 {:?}", part.data.len(), part.completion);
```

## 参数

第一个参数是服务器文件路径，第二个参数是允许下载的最大字节数。`download_file` 返回下载状态，`download_complete` 要求完整下载并返回字节。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

`download_complete` 返回 `Bytes`。`download_file` 返回 `DownloadResult`，可检查是否完整后再使用数据。请求失败时通过 `Result` 返回错误。

- `带上限下载` 使用 `ResourceService::download_file(&self, path: &str, max_bytes: usize, options: &OperationOptions)`，返回 `Result<DownloadResult>`。
- `要求完整下载` 使用 `ResourceService::download_complete(&self, path: &str, max_bytes: usize, options: &OperationOptions)`，返回 `Result<Bytes>`。

下载结果的 `data` 为已读字节，`completion` 说明是否读完整，`next_offset` 提供可能的后续偏移。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.ResourceService.html#method.download_file)
