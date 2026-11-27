---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 扩展市场 K 线

## 作用

读取扩展标的 K 线。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。 证券身份应取自同一服务器的市场和代码。[查看 MAC-EX 商品目录](macex-goods-list.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::ex::ExInstrumentId;
use eltdx::mac::ex_query::BarsQuery;

fn query(client: &MacExClient, id: ExInstrumentId) -> Result<()> {
    let query = BarsQuery::new(id)?;
    let result = client.bars(&query, &OperationOptions::default())?;
    for bar in result.iter().take(5) {
        println!("{:?} {:?}", bar.time(), bar.close());
    }
    Ok(())
}
```

## 参数

查询条件支持设置周期、分页等。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

使用 `with_period`、`with_times` 选择周期及倍数，`with_start`、`with_count`、`with_pages` 调整查询范围。复权通过 `with_adjust` 设置。

## 返回

返回 `Vec<Bar>`。请求及参数错误通过 `Result` 返回。

每条 `Bar` 的 `time()` 对应周期，`close()` 等方法提供价格及来源信息。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.bars)
