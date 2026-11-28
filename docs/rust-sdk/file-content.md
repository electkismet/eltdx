---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 服务器文件分块读取

## 作用

从主站的资源路径读取指定范围的文件数据。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.resources().read("T0002/hq_cache/block.dat", 0, 4096, &options);
    let closed = client.close();
    let chunk = result?;
    println!("读取 {} 字节，下一偏移 {:?}", chunk.data.len(), chunk.next_offset);
    closed
}
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `path: &str` | 服务器资源路径，实际可用路径由主站决定。 |
| `offset: u64` | 从文件开头计算的字节偏移。 |
| `size: usize` | 希望读取的字节数。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`Result<DownloadResult>` 包含 `data`（已读字节）、`completion`（完成状态）和 `next_offset`（可能的下一偏移）。读取指定范围时，检查结果状态以判断是否到达文件末尾。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.ResourceService.html#method.read)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
