---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 板块汇总

## 作用

读取指定板块的汇总信息。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, BoardSummaryQuery, BoardSymbol};

fn query(client: &MacClient, board: BoardSymbol) -> Result<()> {
    let query = BoardSummaryQuery::new(board);
    let result = client.board_summary(&query, &OperationOptions::default())?;
    println!("{result:?}");
    Ok(())
}
```

## 参数

传入板块标识。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

`BoardSummaryQuery` 使用板块标识。用 `with_sort`、`with_order` 和 `with_pages` 调整汇总时读取板块成分的设置。

## 返回

返回 `Result<BoardSummary>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

返回单个板块的汇总。可与板块成分查询搭配，先读取整体情况再展开个股列表。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.board_summary)
