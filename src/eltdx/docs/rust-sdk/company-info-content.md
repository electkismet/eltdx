---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 公司信息内容

## 作用

按目录读取某只证券 F10 公司信息的一段 GBK 正文。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};
use eltdx::types::CompanySectionQuery;

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let operation = (|| -> Result<()> {
        let directory = client.company_info().category(&id, &options)?;
        if let Some(section) = directory.rows().first() {
            let query = CompanySectionQuery::new(section.filename().to_owned(), section.offset(), section.length())?;
            let content = client.company_info().content(&id, &query, &options)?;
            println!("{}", content.text());
        }
        Ok(())
    })();
    let closed = client.close();
    operation?;
    closed
}
```

## 参数和使用

先调用公司信息目录，使用某条目录记录的文件名、偏移和长度构造 `CompanySectionQuery`。`id` 指定证券，`options` 设置本次操作。

## 返回

返回 `CompanyText`，通过 `text()` 获取正文，`decoding_had_errors()` 检查解码是否遇到错误。 网络和参数错误通过 `Result` 返回。



[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.CompanyInfoService.html#method.content)
