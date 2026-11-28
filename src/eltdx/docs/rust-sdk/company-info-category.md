---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 公司信息目录

## 作用

读取一只证券的公司信息目录。目录记录段落名称、文件名、偏移和长度，可用于后续读取正文。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.4"`。

```rust
use eltdx::{OperationOptions, Result, SecurityId, TdxClient};

fn main() -> Result<()> {
    let id: SecurityId = "sh600000".parse()?;
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.company_info().category(&id, &options);
    let closed = client.close();
    for section in result?.rows() {
        println!("{} {} 偏移 {} 长度 {}", section.name(), section.filename(), section.offset(), section.length());
    }
    closed
}
```

## 参数

| 参数 | 含义 |
| --- | --- |
| `id: &SecurityId` | 要查询的证券。 |
| `options: &OperationOptions` | 本次请求的操作选项。 |

## 返回

`category` 返回公司信息目录。使用目录中的文件名和范围构造正文查询，再调用 [`content` 读取相应段落](company-info-content.md)。

[查看方法和返回类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/services/struct.CompanyInfoService.html#method.category)

## 出错与关闭

请求失败时通过 `Result` 返回错误。示例先保存查询结果并关闭客户端，再检查查询结果。
