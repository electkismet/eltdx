---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 港股全量成交

## 作用

自动分页读取港股成交明细。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC-EX 商品目录](macex-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExInstrumentId;
use eltdx::mac::ex_query::AllTransactionsQuery;

fn query(client: &MacExClient, id: ExInstrumentId) -> Result<()> {
    let query = AllTransactionsQuery::new(id)?;
    let result = client.all_transactions(&query, &OperationOptions::default())?;
    println!("返回 {} 条", result.len());
    Ok(())
}
```

## 参数

查询支持分页预算，上限为 50 页和 90000 条。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

用 `with_date` 指定交易日，`with_start` 指定开始位置。`with_pages` 限制自动翻页。

## 返回

返回 `Vec<Transaction>`。请求及参数错误通过 `Result` 返回。

在分页预算内自动读取多页，返回组合后的成交列表。每项可用 `time()` 查看时间。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/mac/struct.MacExClient.html#method.all_transactions)
