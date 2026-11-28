---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 板块行情

## 作用

按 BoardQuery 选择板块类别并取得板块行情。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};
use eltdx::types::{BoardQuery};

fn main() -> Result<()> {
    let query = BoardQuery::new();
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().board_quotes(&query, &options);
    let closed = client.close();
    let table = result?;
    println!("返回 {} 条板块行情", table.rows().len());
    closed
}
```

## 参数

默认概念板块。`with_category` 选择板块类别，`with_refresh` 控制刷新，`with_data_dir`、`with_definitions_dir` 和 `with_file` 指定本地数据来源。

`options` 控制本次操作，示例使用默认值。查询对象的 `with_...` 方法返回错误时，使用 `?` 将错误交给调用方处理。

## 返回

使用 `rows()` 获取板块行情列表，结果也包含来源信息。请求失败时通过 `Result` 返回错误。



需要展开成分时，把选中的板块编号交给 [板块成分股行情](board-member-quotes.md)。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/helpers/struct.HelperService.html#method.board_quotes)
