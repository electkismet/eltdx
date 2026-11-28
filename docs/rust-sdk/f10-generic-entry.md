---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# F10 高级调用

## 作用

通过通用入口发送已构造的 F10 请求。

## 示例

在 `Cargo.toml` 中启用 F10 功能。

```toml
eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }
```

```rust
use eltdx::{OperationOptions, Result};
use eltdx::f10::{F10Client, MarketNewsQuery};

fn main() -> Result<()> {
    let query = MarketNewsQuery::new().to_call()?;
    let client = F10Client::builder().build()?;
    let options = OperationOptions::default();
    let result = client.call(&query, &options);
    let closed = client.close();
    let response = result?;
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
    closed
}
```

### 其他调用

`call` 可以用 `RequestPayload::Body` 发送已有 JSON 请求体，也可以使用 `RequestPayload::Params`。确定手头是有序 Params 列表时，可以用 `params` 入口。`entry` 和 `params` 必须来自对应接口定义。 `entry` 为对应的 `EntryName`，`params` 为有序 `Vec<JsonValue>`。在关闭客户端前调用。

```rust
let query = eltdx::f10::ParamsQuery::new(entry, params);
let response = client.params(&query, &options)?;
```

## 参数

示例将首页要闻查询转换为通用请求。`CallQuery` 也可按已知的 Entry 和参数构造，参数应与目标 Entry 一致。

`options` 控制本次操作，示例使用默认值。

## 返回

返回 `Result<Response>`。请求错误通过 `Result` 返回。请求成功后还应检查 `Response::ok()`，它反映服务端的业务状态。

`ok()` 检查业务状态，`error_code()` 提供服务端错误码。`result_sets()` 遍历结果表，`columns()` 查看列名，`rows()` 和 `cells()` 读取记录与字段。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/f10/struct.F10Client.html#method.call)
