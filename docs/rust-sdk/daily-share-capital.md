---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 每日股本（盘前）

## 作用

合并财务快照与服务器统计资源，返回总股本、流通股本和自由流通股本。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{DailySharesQuery};

fn main() -> Result<()> {
    let query = DailySharesQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().daily_share_capital(&query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("{}，总股本 {:?}，流通 {:?}，自由流通 {:?}", row.id(), row.total(), row.circulating(), row.free_float());
    }
    closed
}
```

## 参数

默认查询全部 A 股。`with_selection(SecuritySelection::Explicit(ids))?` 可指定证券，`with_stats` 设置统计文件，`with_refresh_stats(true)?` 刷新统计数据。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

每行的 `id()`、`total()`、`circulating()`、`free_float()` 分别提供证券、总股本、流通股本和自由流通股本。部分数据可能缺失，返回 `Option`。请求失败时通过 `Result` 返回错误。



自由流通股本是独立字段。来源没有给数值时保持 `None`，用 `sources()` 和 `meta()` 核对来源。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/helpers/struct.HelperService.html#method.daily_share_capital)
