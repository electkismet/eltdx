---
hide:
  - navigation
---

[← 返回 Rust 接口目录](../index.md?lang=rust){ .interface-detail-back }

# 资讯频道列表

## 作用

读取 SDK 提供的资讯频道列表，用于选择频道资讯查询条件。列表由 SDK 本地提供。

## 示例

在 `Cargo.toml` 中启用 `eltdx = { version = "=3.4.0-beta.4", features = ["f10"] }`。

```rust
use eltdx::{Result};
use eltdx::f10::F10Client;

fn main() -> Result<()> {
    let client = F10Client::builder().build()?;
    for channel in client.news_channels() {
        println!("{} {} {:?}", channel.id(), channel.label(), channel.group());
    }
    client.close()
}
```

## 参数与返回

`news_channels()` 不需要查询参数，返回 `Vec<NewsChannelInfo>`，列表按频道 ID 排序。每项的 `id()`、`label()`、`group()` 分别给出编号、名称和所属分组。使用 [频道资讯](f10-channel-news.md) 按频道查询内容。

[查看方法与类型](https://electkismet.github.io/eltdx/rust-api/3.4.0-beta.4/eltdx/f10/struct.F10Client.html#method.news_channels)
