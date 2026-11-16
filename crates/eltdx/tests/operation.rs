//! Public options/bridge source cases; authored now, executed only in final plan validation.
use std::error::Error as _;
use std::time::{Duration, Instant};

use eltdx::{CancellationToken, Error, ErrorKind, OperationOptions, RawPolicy, TdxClient};

#[test]
fn public_options_are_owned_snapshots_and_tokens_can_outlive_clients() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<OperationOptions>();
    send_sync::<CancellationToken>();
    let token = CancellationToken::default();
    let options = OperationOptions::default().with_timeout(Duration::ZERO)
        .with_deadline(Instant::now()).with_cancellation(token.clone()).with_raw(RawPolicy::Omit);
    let snapshot = options.clone();
    let client = TdxClient::builder().extra_attempt_budget(0).build().expect("lazy client");
    client.close().expect("close independent of cancelled business state");
    token.cancel();
    assert!(token.is_cancelled());
    drop(client);
    drop(snapshot);
    assert!(token.is_cancelled());
}

#[test]
fn runtime_errors_remain_typed_language_neutral_causes() {
    let source = eltdx_runtime::RuntimeError::connection_closed("fixture connection lost")
        .with_context("host", "fixture:7709").with_context("attempt", "2");
    let error = Error::new(ErrorKind::Connection, "connection failed")
        .with_context("operation", "fixture_pages").with_source(source.clone());
    assert_eq!(error.source().and_then(|cause| cause.downcast_ref::<eltdx_runtime::RuntimeError>()), Some(&source));
    assert_eq!(error.clone().kind(), ErrorKind::Connection);
}

#[test]
fn legacy_bridge_is_cross_crate_visible_without_python_runtime() {
    use eltdx::__python_compat::{LegacyChildPurpose, LegacyExecutor};
    use eltdx_runtime::endpoint::Endpoint;
    use eltdx_runtime::{Engine, EngineConfig, EngineConfigOptions};
    let engine = Engine::new(EngineConfig::from_endpoints(
        vec![Endpoint::numeric("127.0.0.1:7709").expect("numeric only")], Duration::from_secs(8),
        EngineConfigOptions {
            pool_size: 1, runtime_workers: Some(1), server_count: 1,
            max_connections_per_host: None, connect_concurrency: None, connect_concurrency_per_host: None,
            global_raw_bytes: None, global_decoded_bytes: None, heartbeat_interval: None,
            max_pending_requests: 8, push_queue_size: 8, push_queue_bytes: 1024,
        },
    ).expect("config")).expect("lazy engine");
    let executor = LegacyExecutor::new(engine, Duration::from_secs(8), Duration::from_secs(1), Duration::from_secs(1))
        .expect("independent legacy generation");
    let operation = executor.begin_operation("binding_fixture", RawPolicy::Omit, None)
        .expect("context").with_page_limit(None).expect("unlimited legacy pages");
    let _purpose = LegacyChildPurpose::Metadata;
    operation.finish().expect("empty legacy operation succeeds");
    executor.close().expect("no runtime/network was started");
}
