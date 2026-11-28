#[test]
#[allow(deprecated)]
fn legacy_recent_selector_base_remains_importable() {
    use eltdx_protocol::commands::minutes::RECENT_DATE_SELECTOR_BASE;

    assert_eq!(RECENT_DATE_SELECTOR_BASE, 0xfed6_2304);
}
