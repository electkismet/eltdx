---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 完整行情与五档盘口

## 作用

批量读取完整行情及盘口信息。

## 示例

在 `Cargo.toml` 中添加 `eltdx = "=3.4.0-beta.3"`。

```rust
use eltdx::{OperationOptions, Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    let options = OperationOptions::default();
    let result = client.helpers().full_quotes(&["sh600000".parse()?], &options);
    let closed = client.close();
    for item in result?.items() {
        println!("{} {:?}", item.security(), item.state());
    }
    closed
}
```

## 参数

传入 `SecurityId` 列表，示例查询上海证券 600000。`options` 控制本次操作，示例使用默认选项。

## 返回

返回 `BatchOutcome<FullQuote>`，分别检查每个证券的结果。请求和参数错误通过 `Result` 返回。

批量返回保留输入顺序。通过 `state()` 区分 `Present` 和 `ConfirmedMissing`，`input_index()` 可映射回原输入。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.3/eltdx/helpers/struct.HelperService.html#method.full_quotes)
