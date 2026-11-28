---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# EX 历史成交明细

## 作用

分页读取扩展商品指定交易日的成交明细。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["ex"] }`。示例函数接收调用方已配置的 EX 客户端。使用结束后调用方应关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](ex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 EX 商品目录](ex-info.md)。

```rust
use eltdx::{OperationOptions, Result, TradingDate};
use eltdx::ex::{ExClient, ExInstrumentId, HistoryTransactionsQuery};

fn query(client: &ExClient, id: ExInstrumentId) -> Result<()> {
    let options = OperationOptions::default();
    let result = client.history_transactions(&HistoryTransactionsQuery::new(id, TradingDate::new(2025, 1, 2)?)?, &options)?;
    println!("本页 {} 条，下一页位置 {:?}", result.rows().len(), result.next_offset());
    Ok(())
}
```

## 参数

构造函数接收商品身份和交易日期。`with_start` 和 `with_count` 设置分页。`options` 是本次操作选项，示例使用默认值。

构造查询时传入证券和交易日，用 `with_start` 和 `with_count` 逐页读取。

## 返回

`rows()` 读取本页，`next_offset()` 获取服务器或 SDK 提供的下一页位置，`completion()` 查看结束状态。

返回 `PageResult<Transaction>`，检查行和分页状态。请求或参数错误通过 `Result` 返回。

每条成交可通过 `time()`、`price_raw()` 和 `volume()` 读取。交易方向原始值可用 `direction_raw()` 查看。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/ex/struct.ExClient.html#method.history_transactions)
