---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 详情正文

## 作用

根据资讯列表提供的类型和记录 ID，读取对应详情。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。示例函数接收已创建的 F10 客户端，调用方应在使用后关闭客户端。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{DetailQuery, F10Client};

fn read_detail(client: &F10Client, detail_type: &str, record_id: &str) -> Result<()> {
    let query = DetailQuery::new(detail_type, record_id)?;
    let response = client.detail(&query, &OperationOptions::default())?;
    if !response.ok() {
        eprintln!("服务端错误码：{:?}", response.error_code());
    } else if let Some(table) = response.first_table() {
        println!("列：{:?}", table.columns());
        for row in table.rows().iter().take(3) {
            for cell in row.cells() {
                println!("{} = {:?}", cell.name(), cell.value());
            }
        }
    }
    Ok(())
}
```

## 参数

`detail_type` 和 `record_id` 应使用资讯查询返回的值。`OperationOptions::default()` 使用默认请求选项。

详情类型和记录 ID 来自对应的资讯列表。列表与详情必须使用匹配的栏目和标识。

## 返回

返回 `Result<Response>`。请求成功后通过 `response.ok()` 检查服务端业务状态，读取返回内容。

`ok()` 检查业务状态，`error_code()` 提供服务端错误码。`result_sets()` 遍历结果表，`columns()` 查看列名，`rows()` 和 `cells()` 读取记录与字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/f10/struct.F10Client.html#method.detail)
