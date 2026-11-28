---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 板块成分股行情

## 作用

按板块标识读取板块成分股及行情。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{BoardQuery, BoardId};

fn main() -> Result<()> {
    let board: BoardId = "880001".parse()?;
    let query = BoardQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().board_member_quotes(&board, &query, &options);
    let closed = client.close();
    let members = result?;
    println!("展示成分 {} 个，行情 {} 条", members.display_members().len(), members.rows().len());
    closed
}
```

## 参数

`board` 是六位板块代码，需替换成板块行情结果中实际存在的代码。`BoardQuery` 可选择类别和数据来源。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

返回板块及成分股行情，结果中保留来源及诊断信息。请求失败时通过 `Result` 返回错误。



通过 `raw_members()`、`display_members()` 和 `excluded_members()` 分别查看源成分、展示成分和筛除成分。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.board_member_quotes)
