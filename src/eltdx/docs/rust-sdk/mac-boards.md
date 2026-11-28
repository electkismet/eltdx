---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC 板块列表

## 作用

读取板块目录。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。示例函数接收已配置的 MAC 客户端。调用方使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](mac-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::{MacClient, BoardsQuery};

fn query(client: &MacClient) -> Result<()> {
    let query = BoardsQuery::new();
    let result = client.boards(&query, &OperationOptions::default())?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

可在查询条件中筛选板块类型。示例使用默认操作选项。标识和文件名应使用目标服务器支持的值。

通过 `with_board_type` 选择板块类型，`with_count`、`with_pages` 调整读取预算。

## 返回

返回 `Result<Vec<BoardInfo>>`。请求和参数错误通过 `Result` 返回。分页结果应检查行数据与分页状态。

数组中每项代表一个板块。先从这里确认板块，再请求板块成分股或板块汇总。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacClient.html#method.boards)
