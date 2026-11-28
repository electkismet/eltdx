---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 板块排行榜

## 作用

读取板块排行榜。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, BoardRankingQuery};

fn query(client: &MacClient) -> Result<()> {
    let query = BoardRankingQuery::new();
    let result = client.board_ranking(&query, &OperationOptions::default())?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

可通过查询条件指定排行榜设置。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

用 `with_board_type` 选择板块类别，`with_sort` 和 `with_ascending` 控制排序，`with_top_n` 限制返回行数。`with_pages` 与 `with_member_pages` 分别设置板块和成分股的读取预算。

## 返回

返回 `Result<Vec<BoardRank>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

数组中每项为一个板块的排行结果，按查询中的排名字段和方向排列。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.board_ranking)
