---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 板块文件元数据

## 作用

查询板块文件在服务器上的字节大小与 MD5 摘要，用于判断本地缓存是否需要更新。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.resources().block_meta("block.dat", &options);
    let closed = client.close();
    let meta = result?;
    println!("{} 字节，服务端 MD5 {}", meta.size(), meta.md5());
    closed
}
```

## 参数

文件名应替换成服务器上实际存在的板块文件名。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

返回 `BlockMeta`，提供文件大小和 MD5 摘要，可用于比较缓存。请求失败时通过 `Result` 返回错误。



`md5()` 是服务器提供的摘要。若用于完整性校验，下载完成后另外计算文件摘要进行比较。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.ResourceService.html#method.block_meta)
