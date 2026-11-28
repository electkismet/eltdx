---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 分类排行

## 作用

按证券分类读取报价排行。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, QuoteListQuery, Category};

fn query(client: &MacClient, category: Category) -> Result<()> {
    let query = QuoteListQuery::new(category);
    let result = client.quote_list(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

可设置排序方式、筛选和分页。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`with_sort` 与 `with_order` 设置排序，`with_exclude` 设置排除条件，`with_fields` 选择返回字段。用 `with_start`、`with_count` 和 `with_pages` 控制分页。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `Result<PageResult<Quote>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

`rows()` 是榜单的本页记录。每条记录可通过 `name()`、`fields()` 识别标的和读取所选字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.quote_list)
