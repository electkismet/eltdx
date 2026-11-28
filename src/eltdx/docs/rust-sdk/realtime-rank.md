---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 实时榜单

## 作用

基于分类行情分页获取排行，整理名称和排名。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{RankQuery};

fn main() -> Result<()> {
    let query = RankQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().realtime_rank(&query, &options);
    let closed = client.close();
    for row in result?.rows().iter().take(5) {
        println!("{} {} {:?}", row.rank(), row.id(), row.name());
    }
    closed
}
```

## 参数

默认 A 股按涨幅降序取前 80 条。`with_category`、`with_sort`、`with_ascending`、`with_count` 和 `with_pages` 可调整范围、排序和分页。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

每行的 `rank()`、`id()`、`name()` 和 `quote()` 分别提供排名、证券、名称和行情。请求失败时通过 `Result` 返回错误。



`with_count(None)?` 会按预算扫描，把 `end()` 与结果一起检查。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/helpers/struct.HelperService.html#method.realtime_rank)
