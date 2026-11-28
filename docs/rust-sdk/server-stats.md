---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 服务器统计文件解析

## 作用

下载并解析 zhb.zip 中的 tdxstat.cfg 与 tdxstat2.cfg。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{StatsQuery};

fn main() -> Result<()> {
    let query = StatsQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.resources().read_stats(&query, &options);
    let closed = client.close();
    let stats = result?;
    println!("{}：统计 {} 条，扩展统计 {} 条", stats.source_path(), stats.stat().len(), stats.stat2().len());
    closed
}
```

## 参数

默认读取 `zhb.zip`，每块 30000 字节。`with_path` 修改服务器路径，`with_chunk_size` 设置 1 至 60000 字节的块大小。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

返回 `StatsResource`，`stat()` 和 `stat2()` 对应压缩包中的两份统计表。`stat_for()` 和 `stat2_for()` 按证券查一行，可能返回 `None`。请求失败时通过 `Result` 返回错误。



[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.ResourceService.html#method.read_stats)
