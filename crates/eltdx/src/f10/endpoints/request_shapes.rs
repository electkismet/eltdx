//! Ordered object payload layouts consumed by Rust and compatibility adapters.

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum CacheValue {
    Literal(&'static str),
    Key,
    Qsid,
}

#[doc(hidden)]
pub const CACHE_ENTRY: &str = "CWSearch.tzx_rcache";

#[doc(hidden)]
pub fn cache_payload<T, E>(
    mut value: impl FnMut(CacheValue) -> Result<T, E>,
) -> Result<Vec<(&'static str, T)>, E> {
    use CacheValue::{Key, Literal, Qsid};
    [
        ("action", Literal("get")),
        ("key", Key),
        ("bin", Literal("1")),
        ("qsid", Qsid),
    ]
    .into_iter()
    .map(|(name, slot)| Ok((name, value(slot)?)))
    .collect()
}

#[doc(hidden)]
pub fn cache_alias(method: &str) -> Option<&'static str> {
    match method {
        "announcements" => Some("gg"),
        "news" => Some("xw"),
        "roadshows" => Some("ly"),
        _ => None,
    }
}

#[doc(hidden)]
pub const CACHE_KINDS: &[&str] = &["xw", "gg", "ly"];
