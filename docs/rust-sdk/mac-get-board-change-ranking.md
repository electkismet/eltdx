---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 板块多日涨幅排行

## 作用

读取板块多日涨幅排行。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, BoardChangeRankingQuery};

fn query(client: &MacClient) -> Result<()> {
    let query = BoardChangeRankingQuery::new();
    let result = client.board_change_ranking(&query, &OperationOptions::default())?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

可通过查询条件设置排行范围。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

用 `with_target_date` 设置目标交易日，`with_days` 设置区间天数。`with_board_type`、`with_top_n`、`with_ascending` 控制板块筛选与排序。`with_pages` 和 `with_history_pages` 限制读取预算。

## 返回

返回 `Result<Vec<BoardChangeRank>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

数组中每项为一个板块在所选区间的表现。用同一交易日和天数比较不同板块。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacClient.html#method.board_change_ranking)
