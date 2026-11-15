use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use eltdx::{ClientBuilder, ErrorKind, HqMarket, SecurityId, TdxClient, TradingDate};

#[test]
fn defaults_build_and_clones_share_terminal_close() -> eltdx::Result<()> {
    let client = TdxClient::builder().build()?;
    let clone = client.clone();

    assert_eq!(client.config().pool_size(), 1);
    assert_eq!(client.config().request_timeout(), Duration::from_secs(8));
    assert_eq!(client.config().close_timeout(), Duration::from_secs(1));

    client.close()?;
    clone.close()?;
    assert!(matches!(clone.close(), Ok(())));
    Ok(())
}

#[test]
fn invalid_zero_capacity_and_timeouts_are_rejected() {
    let zero_pool = ClientBuilder::new().pool_size(0).build();
    assert!(matches!(zero_pool, Err(error) if error.kind() == ErrorKind::InvalidArgument));

    let zero_request_timeout = ClientBuilder::new()
        .request_timeout(Duration::ZERO)
        .build();
    assert!(matches!(zero_request_timeout, Err(error) if error.kind() == ErrorKind::InvalidArgument));

    let zero_close_timeout = ClientBuilder::new()
        .close_timeout(Duration::ZERO)
        .build();
    assert!(matches!(zero_close_timeout, Err(error) if error.kind() == ErrorKind::InvalidArgument));

    let zero_cleanup_timeout = ClientBuilder::new()
        .cleanup_timeout(Duration::ZERO)
        .build();
    assert!(matches!(zero_cleanup_timeout, Err(error) if error.kind() == ErrorKind::InvalidArgument));
}

#[test]
fn explicit_endpoint_port_zero_is_rejected() {
    let endpoint = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let result = ClientBuilder::new().endpoints(vec![endpoint]).build();

    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::InvalidArgument));
}

#[test]
fn empty_explicit_endpoint_list_is_rejected() {
    let result = ClientBuilder::new().endpoints(Vec::new()).build();
    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::InvalidArgument));
}

#[test]
fn explicit_endpoint_order_is_preserved() -> eltdx::Result<()> {
    let endpoints = vec![
        SocketAddr::from((Ipv4Addr::LOCALHOST, 7709)),
        SocketAddr::from((Ipv4Addr::LOCALHOST, 7710)),
    ];
    let config = ClientBuilder::new().endpoints(endpoints.clone()).build()?.config().clone();

    assert_eq!(config.endpoints(), Some(endpoints.as_slice()));
    Ok(())
}

#[test]
fn close_before_runtime_start_does_not_need_a_real_host() -> eltdx::Result<()> {
    let client = TdxClient::builder()
        .endpoints(vec![SocketAddr::from((Ipv4Addr::LOCALHOST, 7709))])
        .build()?;

    client.close()?;
    Ok(())
}

#[test]
fn strict_hq_security_identity_preserves_known_inference_rules() -> eltdx::Result<()> {
    let cases = [
        ("sh600000", HqMarket::Shanghai),
        ("510300", HqMarket::Shanghai),
        ("sz980712", HqMarket::Shenzhen),
        ("430047", HqMarket::Beijing),
    ];
    for (text, market) in cases {
        let security = text.parse::<SecurityId>()?;
        assert_eq!(security.market(), market);
    }
    assert!(SecurityId::new(HqMarket::Shanghai, "12a456").is_err());
    assert!("00700".parse::<SecurityId>().is_err());
    assert!("019123".parse::<SecurityId>().is_err());
    assert!("127001".parse::<SecurityId>().is_err());
    Ok(())
}

#[test]
fn trading_dates_validate_gregorian_calendar_without_trading_day_lookup() -> eltdx::Result<()> {
    assert!(TradingDate::new(2024, 2, 29).is_ok());
    assert!(TradingDate::new(2025, 2, 29).is_err());
    assert!(TradingDate::new(2026, 11, 16).is_ok());
    Ok(())
}
