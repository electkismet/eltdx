---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 日期范围历史 K 线

## 作用

按日期范围查询扩展商品历史 K 线。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result, TradingDate};
use eltdx::ex::{ExClient, BarsRangeQuery, ExInstrumentId};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.bars_range(&BarsRangeQuery::new(id, TradingDate::new(2025, 1, 2)?, TradingDate::new(2025, 1, 10)?)?, &options)?;
    for bar in result.iter().take(5) {
        println!("{:?} {:?}", bar.time(), bar.close());
    }
    Ok(())
}
```

## 参数

构造函数依次接收商品、开始日期和结束日期。可用 `with_bar_time` 设置 K 线时间表示方式。`options` 是本次操作选项，示例使用默认值。

`BarsRangeQuery::new(id, start_date, end_date)` 设置日期范围，起始日期应早于或等于结束日期。`with_bar_time` 调整时间标签。

## 返回

返回 `Vec<Bar>`。请求或参数错误通过 `Result` 返回。

每条记录包含时间和开高低收。服务器若提供区间结算价，可通过 `settlement()` 读取。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/ex/struct.ExClient.html#method.bars_range)
