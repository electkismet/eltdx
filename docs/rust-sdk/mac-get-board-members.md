---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 板块成分股报价

## 作用

读取指定板块的成分股报价。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, BoardMembersQuery, BoardSymbol};

fn query(client: &MacClient, board: BoardSymbol) -> Result<()> {
    let query = BoardMembersQuery::new(board);
    let result = client.board_members(&query, &OperationOptions::default())?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

传入板块标识，可设置分页和排序。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`with_sort`、`with_order` 控制排序，`with_exclude` 设排除条件，`with_fields` 选择报价字段。用 `with_start`、`with_count`、`with_pages` 控制分页。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `Result<PageResult<Quote>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

`rows()` 返回板块成分的行情，每个 `Quote` 提供 `id()`、`name()` 和选中的 `fields()`。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.board_members)
