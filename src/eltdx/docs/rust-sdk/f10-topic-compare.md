---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 题材内对比

## 作用

查询某股票在指定题材中的对比数据。题材 ID 可以从该股票的题材查询结果取得。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。示例函数接收已创建的 F10 客户端，调用方应在使用后关闭客户端。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{AshareCode, F10Client, F10MarketCode, TopicCompareQuery};

fn compare(client: &F10Client, topic_id: &str) -> Result<()> {
    let code = AshareCode::new(F10MarketCode::new(1), "600000")?;
    let query = TopicCompareQuery::new(code.clone(), topic_id)?;
    let options = OperationOptions::default();
    let response = client.topic_compare(&query, &options)?;
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

同一功能的其他调用可以使用以下写法。`client`、`code` 和 `options` 沿用上面的设置。

```rust
// 由服务端数据选出首个题材
let response = client.topic_compare_first(&eltdx::f10::TopicCompareFirstQuery::new(code.clone()), &options)?;
```

## 参数

`topic_id` 是服务端提供的题材 ID。`with_sort_by` 设置排序字段，`with_section` 选择受支持的查询栏目。`OperationOptions::default()` 使用默认请求选项。

`TopicCompareQuery` 接受已知的题材 ID。可用 `with_topic_id` 修改题材，`with_section` 选择栏目，`with_sort_by` 选择服务端排序字段。`TopicCompareFirstQuery` 从证券关联的题材中自动选取第一个，可设置栏目和排序。

## 返回

返回 `Result<Response>`。请求成功后通过 `response.ok()` 检查服务端业务状态，读取返回内容。

`ok()` 检查业务状态，`error_code()` 提供服务端错误码。`result_sets()` 遍历结果表，`columns()` 查看列名，`rows()` 和 `cells()` 读取记录与字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/f10/struct.F10Client.html#method.topic_compare)
