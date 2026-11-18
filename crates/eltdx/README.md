# eltdx Rust SDK

Rust **1.89.0** or later, edition 2021. The Rust SDK and Python package share
the eltdx project version. The repository's non-commercial learning and research
[LICENSE](LICENSE) applies to this crate.

Rust applications depend on this crate and do not install Python, PyO3, pip or
maturin. Python users continue to install their separate `eltdx` wheel; a
supported prebuilt wheel does not require the user to install Rust tools.

```toml
[dependencies]
# For development from a local checkout:
eltdx = { path = "/absolute/candidate/crates/eltdx" }
```

```rust
use eltdx::{Result, TdxClient};

fn main() -> Result<()> {
    let client = TdxClient::builder().build()?;
    // Construction and obtaining service handles perform no network I/O.
    let quotes = client.quotes();
    drop(quotes);
    client.close()?;
    client.close()?;
    Ok(())
}
```

The synchronous public API covers HQ codes, quotes, bars, trades, capital,
calendar, resources, company information and composition helpers. A finite
whole-operation deadline includes queueing, startup and child requests. Clone
shares one owner; explicit close seals that owner and confirms resource cleanup
or returns a cleanup error. A new builder creates an independent owner.

| Feature | Public domain |
| --- | --- |
| Default (empty) | HQ synchronous services and HQ helpers |
| `serde` | Serialization on supported public result types |
| `offline` | Explicit local files and professional financial ZIP/DAT |
| `f10` | HTTP/TQLEX clients, named endpoints and F10 compositions |
| `ex` | EX markets, directory, quotes, bars, minutes and transactions |
| `mac` | MAC and MAC-EX, automatically enabling `ex` |
| `full` | All five selections above |

Internal Tokio use does not provide a public async API. `raw=false` does not
promise zero copies. Source-native numbers preserve their unit/evidence labels;
unknown economic meanings remain unknown. MAC-EX currently requires explicit
verified login reply rules before production use; no proven production default
is supplied. Synthetic fixture rules only exercise authored contracts.

Examples require explicit server addresses and close clients on error paths.
`hq_query` uses the default features; `ex_query` needs `ex`;
`financial_read` and `financial_download` need `offline`.

`measure_candidate` also needs `offline` and is a measurement worker for the
fixed-fixture performance collector, with an explicit implementation-complete
gate. It is not a general query example. Build and run it only during unified
verification, after the whole implementation source gate and correctness runs.

Repository documentation lives in `docs/rust-sdk/`: architecture, public API,
compatibility decisions, verification, source inventories and release policy.
Start with `user-guide.md` and the release notes for the version you install.
