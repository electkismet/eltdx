#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg(feature = "f10")]

use eltdx::f10::{
    EntryName, F10Client, Header, JsonNumber, JsonValue, NewsChannel, ProxyUrl, Route,
};

#[test]
fn ordered_duplicate_members_and_large_numeric_text_roundtrip() -> eltdx::Result<()> {
    let json = br#"{"x":18446744073709551616,"x":-0,"a":1.2300e+04}"#;
    let value = JsonValue::parse(json)?;
    let members: Vec<_> = value.get_all("x").collect();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].as_number().and_then(JsonNumber::as_u64), None);
    assert_eq!(
        members[0].as_number().map(JsonNumber::lexeme),
        Some("18446744073709551616")
    );
    assert_eq!(members[1].as_number().map(JsonNumber::lexeme), Some("-0"));
    assert_eq!(value.to_json_bytes()?, json);
    Ok(())
}

#[test]
fn bom_unicode_escapes_and_surrogate_pairs_are_decoded_without_ascii_reencoding(
) -> eltdx::Result<()> {
    let value = JsonValue::parse(b"\xef\xbb\xbf{\"name\":\"\\u4e2d\\u6587\\ud83d\\ude00\"}")?;
    assert_eq!(
        value.get("name").and_then(JsonValue::as_str),
        Some("中文😀")
    );
    assert_eq!(
        String::from_utf8_lossy(&value.to_json_bytes()?),
        "{\"name\":\"中文😀\"}"
    );
    Ok(())
}

#[test]
fn invalid_json_number_utf8_and_syntax_are_rejected() {
    for text in ["", "+1", "01", "-", "1.", "1e", "NaN", "Infinity", " 1"] {
        assert!(JsonNumber::new(text).is_err(), "{text}");
    }
    for bytes in [
        &b"[1,]"[..],
        &b"{\"a\":1,}"[..],
        &b"\"\\ud800\""[..],
        &b"\xff"[..],
        &b"true false"[..],
    ] {
        assert!(JsonValue::parse(bytes).is_err());
    }
    assert!(JsonNumber::new("1e999999")
        .ok()
        .and_then(|number| number.as_f64())
        .is_none());
}

#[test]
fn route_names_and_local_header_proxy_rejections_are_explicit() -> eltdx::Result<()> {
    assert_eq!(
        Route::for_entry(&EntryName::new("CWServ.cfg_fx_lbtt")?),
        Route::Ladder
    );
    assert_eq!(
        Route::for_entry(&EntryName::new("CWServ.skef10_hy_info")?),
        Route::International
    );
    assert_eq!(
        Route::for_entry(&EntryName::new("CWServ.qhf9_info")?),
        Route::International
    );
    assert_eq!(
        Route::for_entry(&EntryName::new("CWServ.tdxf10_gg_info")?),
        Route::Static
    );
    assert!(Header::new("Accept\r\nInjected", "json").is_err());
    assert!(Header::new("Accept", "json\r\nInjected: true").is_err());
    assert!(ProxyUrl::new("http://user:secret@localhost:3128/").is_err());
    assert!(ProxyUrl::new("http://localhost:3128/#fragment").is_err());
    assert!(ProxyUrl::new("socks5://localhost:1080/").is_err());
    assert!(EntryName::new("").is_err());
    Ok(())
}

#[test]
fn independent_f10_build_close_and_header_order_need_no_network() -> eltdx::Result<()> {
    let client = F10Client::builder()
        .with_base_url("https://unresolved.invalid:65535/TQLEX")
        .with_headers(vec![
            Header::new("Accept", "application/custom")?,
            Header::new("X-Repeated", "first")?,
            Header::new("X-Repeated", "second")?,
        ])
        .build()?;
    let clone = client.clone();
    assert!(client.diagnostics()?.active_endpoint().is_none());
    assert_eq!(
        client
            .config()
            .headers()
            .iter()
            .map(|header| (header.name(), header.value()))
            .collect::<Vec<_>>(),
        vec![
            ("Content-Type", "application/json"),
            ("Accept", "application/custom"),
            ("X-Repeated", "first"),
            ("X-Repeated", "second")
        ]
    );
    clone.close()?;
    client.close()?;
    Ok(())
}

#[test]
fn operation_byte_limit_is_independent_of_single_response_limits() -> eltdx::Result<()> {
    let client = F10Client::builder()
        .with_max_operation_bytes(
            std::num::NonZeroU64::new(19)
                .ok_or_else(|| eltdx::Error::new(eltdx::ErrorKind::Internal, "test constant"))?,
        )
        .build()?;
    assert_eq!(client.config().max_operation_bytes().get(), 19);
    assert_eq!(client.config().max_response_bytes().get(), 16 * 1024 * 1024);
    client.close()
}

#[test]
fn news_channel_registry_is_sorted_independent_and_available_without_io() -> eltdx::Result<()> {
    let client = F10Client::builder()
        .with_base_url("https://unresolved.invalid:65535/TQLEX")
        .build()?;
    let mut channels = client.news_channels();
    assert!(channels.windows(2).all(|pair| pair[0].id() < pair[1].id()));
    assert_eq!(
        channels
            .first()
            .map(|value| (value.id(), value.label(), value.group())),
        Some((200, "最新", Some("财经")))
    );
    assert_eq!(channels.last().map(|value| value.id()), Some(1804));
    assert!(channels
        .iter()
        .any(|value| value.id() == 318 && value.label() == "科创板"));
    channels.clear();
    assert!(!client.news_channels().is_empty());
    assert_eq!(NewsChannel::from_wire(999999), NewsChannel::Unknown(999999));
    assert_eq!(NewsChannel::Unknown(999999).wire(), 999999);
    assert_eq!(client.diagnostics()?.completed_requests(), 0);
    assert!(client.diagnostics()?.active_endpoint().is_none());
    client.close()
}
