---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 分类行情

## 作用

按分类和排序条件读取一页行情。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{CategoryQuery};

fn main() -> Result<()> {
    let query = CategoryQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.quotes().category(&query, &options);
    let closed = client.close();
    let page = result?;
    println!("本页 {} 条，结束状态 {:?}", page.items().len(), page.end());
    closed
}
```

## 参数

`CategoryQuery::new()` 默认 A 股、按代码排序，从第 0 条开始，每页 80 条。使用 `with_category`、`with_sort`、`with_start`、`with_count` 和 `with_ascending` 调整，单页数量为 1 到 80。

`options` 是本次请求的操作选项。示例使用默认选项。

## 返回和方法

返回 `DataPage<CategoryQuote>`，结果包含本页数据和分页信息。 网络、超时和参数错误通过 `Result` 返回。

- `QuoteService::category(&self, query: &CategoryQuery, options: &OperationOptions)`，返回 `Result<DataPage<CategoryQuote>>`。

不同分类可使用的排序方式取决于服务端。需要连续读取时，检查 `end()` 后按起点请求下一页。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/services/struct.QuoteService.html#method.category)
