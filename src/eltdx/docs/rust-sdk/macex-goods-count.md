---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# MAC-EX 商品数量

## 作用

查询商品数量。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.3", features = ["mac"] }`。函数接收已配置的 MAC-EX 客户端。调用方需要配置经验证的登录响应规则，并在使用结束后关闭客户端。

客户端的服务器地址与连接步骤见 [连接配置](macex-connection.md)。

```rust
use eltdx::{OperationOptions, Result};
use eltdx::mac::MacExClient;
use eltdx::mac::ex_query::GoodsCountQuery;

fn query(client: &MacExClient) -> Result<()> {
    let query = GoodsCountQuery::new();
    let result = client.goods_count(&query, &OperationOptions::default())?;
    println!("{result:?}");
    Ok(())
}
```

## 参数

`with_market` 可限定市场，`with_pages` 设置分页预算。示例使用默认操作选项。扩展市场及标的身份可从对应商品目录取得。

用 `with_market(Some(market))` 指定市场。`with_pages` 控制查询所需的分页预算。

## 返回

返回商品数量 `u32`。请求及参数错误通过 `Result` 返回。

返回对应市场的商品数量。使用 [商品目录](macex-goods-list.md) 从该市场读取具体商品。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/mac/struct.MacExClient.html#method.goods_count)
