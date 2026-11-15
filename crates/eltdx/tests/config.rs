use std::io::ErrorKind as IoErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;

use eltdx::{
    ClientBuilder, ErrorKind, EventTime, EventTimeSource, HqMarket, Price, PriceSource,
    RoundingMode, SecurityId, TdxClient, TradingDate,
};

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
fn build_and_close_do_not_connect_to_configured_socket() -> Result<(), Box<dyn std::error::Error>> {
    // SocketAddr avoids DNS; a live listener lets the test observe any accidental connection.
    // The implementation has no runtime field or startup call, so thread and filesystem
    // startup paths are also excluded by source structure rather than global process probes.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let endpoint = listener.local_addr()?;
    let client = TdxClient::builder().endpoints(vec![endpoint]).build()?;
    client.close()?;

    let accept = listener.accept();
    assert!(matches!(accept, Err(error) if error.kind() == IoErrorKind::WouldBlock));
    Ok(())
}

#[test]
fn failed_configuration_does_not_connect_to_configured_socket() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let endpoint = listener.local_addr()?;
    let result = TdxClient::builder()
        .pool_size(0)
        .endpoints(vec![endpoint])
        .build();

    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::InvalidArgument));
    let accept = listener.accept();
    assert!(matches!(accept, Err(error) if error.kind() == IoErrorKind::WouldBlock));
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
    assert_eq!("sz127001".parse::<SecurityId>()?.market(), HqMarket::Shenzhen);
    Ok(())
}

#[test]
fn trading_dates_validate_gregorian_calendar_without_trading_day_lookup() -> eltdx::Result<()> {
    assert!(TradingDate::new(2024, 2, 29).is_ok());
    assert!(TradingDate::new(2025, 2, 29).is_err());
    assert!(TradingDate::new(2026, 11, 16).is_ok());
    Ok(())
}

#[test]
fn event_time_exposes_source_date_and_clock() -> eltdx::Result<()> {
    let date = TradingDate::new(2026, 11, 16)?;
    let local = EventTime::local(date, 9, 31, 5)?;
    assert_eq!(local.source_kind(), EventTimeSource::Local);
    assert_eq!(local.date(), Some(date));
    assert_eq!(local.clock(), Some((9, 31, 5)));

    let time = EventTime::time_of_day(15, 0, 0)?;
    assert_eq!(time.date(), None);
    assert_eq!(time.clock(), Some((15, 0, 0)));
    assert!(EventTime::shanghai(date, 24, 0, 0).is_err());
    Ok(())
}

#[test]
fn price_reports_provenance_and_requires_explicit_rounding() -> eltdx::Result<()> {
    let scaled = Price::scaled_integer(125528, 3);
    assert_eq!(scaled.source(), PriceSource::ScaledInteger { scale: 3 });
    assert_eq!(scaled.to_scaled_integer_exact(3)?, 125528);
    assert!(scaled.to_scaled_integer_exact(1).is_err());
    assert_eq!(Price::scaled_integer(123400, 3).to_scaled_integer_exact(2)?, 12340);

    let wire = Price::from_f32(1.25);
    assert_eq!(wire.source(), PriceSource::WireF32);
    assert!(wire.to_scaled_integer_exact(2).is_err());
    assert_eq!(wire.to_scaled_integer(1, RoundingMode::HalfToEven)?, 12);
    assert_eq!(wire.to_scaled_integer(1, RoundingMode::HalfAwayFromZero)?, 13);
    Ok(())
}
