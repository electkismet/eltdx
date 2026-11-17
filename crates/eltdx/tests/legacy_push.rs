#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Authored compatibility source cases; execution is deferred with the full plan.
use std::time::{Duration, Instant};

use eltdx::__python_compat::{LegacyChildPurpose, LegacyExecutor};
use eltdx::RawPolicy;
use eltdx_protocol::commands::quotes::RefreshStreamRequest;
use eltdx_protocol::CommandRequest;
use eltdx_runtime::{Engine, EngineConfig, EngineConfigOptions, RuntimeError};

#[path = "support/hq_server.rs"]
mod hq_server;

#[test]
fn exclusive_legacy_reader_preserves_runtime_overflow_and_original_frames() {
    let server = hq_server::Server::start_controlled(|request, connection| {
        assert_eq!(request.msg_type, 0x0547);
        for _ in 0..3 {
            connection.push(0x0547, &hq_server::refresh(&[])).unwrap();
        }
        hq_server::refresh(&[])
    })
    .unwrap();
    let engine = Engine::new(
        EngineConfig::new(
            vec![format!("{}", server.address)],
            2.0,
            EngineConfigOptions {
                pool_size: 1,
                runtime_workers: Some(1),
                server_count: 1,
                max_connections_per_host: None,
                connect_concurrency: None,
                connect_concurrency_per_host: None,
                global_raw_bytes: None,
                global_decoded_bytes: None,
                heartbeat_interval: None,
                max_pending_requests: 8,
                push_queue_size: 2,
                push_queue_bytes: 1024,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let legacy = LegacyExecutor::new(
        engine.clone(),
        Duration::from_secs(2),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let reader = legacy.claim_raw_push_reader().unwrap();
    let operation = legacy
        .begin_operation("raw-push-source-case", RawPolicy::RetainWire, None)
        .unwrap();
    let reply = operation
        .execute_raw(
            CommandRequest::RefreshStream(RefreshStreamRequest::new(Vec::new()).unwrap()),
            LegacyChildPurpose::Required,
            None,
        )
        .unwrap();
    operation.finish().unwrap();
    // Observe the actual runtime buffer condition without consuming it. The
    // fixture has already sent all frames before the matching response.
    let deadline = Instant::now() + Duration::from_secs(2);
    while engine.pool_diagnostics().unwrap().push_dropped < 1 {
        assert!(
            Instant::now() < deadline,
            "all fixture pushes must reach the runtime buffer"
        );
        std::thread::yield_now();
    }
    let overflow = reader.poll_push(Duration::ZERO).unwrap_err();
    assert!(matches!(
        &overflow,
        RuntimeError::PushOverflow {
            dropped_total: 1,
            ..
        }
    ));
    assert_eq!(overflow.kind(), "PushOverflow");
    assert!(overflow
        .context()
        .iter()
        .any(|(key, value)| key == "dropped_total" && value == "1"));
    let frames = reader.drain_pushes().unwrap();
    assert_eq!(frames.len(), 2);
    assert!(frames[0].receive_sequence < frames[1].receive_sequence);
    assert!(frames[1].receive_sequence < reply.receive_sequence);
    for frame in &frames {
        assert_eq!(
            frame.response.data.as_ref(),
            hq_server::refresh(&[]).as_slice()
        );
        assert_eq!(frame.response.raw.len(), 18);
        assert_eq!(frame.engine_epoch, reply.engine_epoch);
    }
    assert!(
        reader.drain_pushes().unwrap().is_empty(),
        "one gap is reported per new cumulative loss"
    );
    assert!(matches!(
        engine.poll_push(Duration::ZERO),
        Err(RuntimeError::PoolBusy { .. })
    ));
    let pin = engine.begin_pin().unwrap().wait().unwrap();
    let source = pin
        .source_identity(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    assert_eq!(source.engine_epoch, reply.engine_epoch);
    assert_eq!(source.slot_id, reply.slot_id);
    assert_eq!(source.generation, reply.generation);
    pin.close().unwrap();
    assert!(matches!(
        pin.source_identity(Duration::from_secs(1)),
        Err(RuntimeError::ConnectionClosed { .. })
    ));
    drop(reader);
    legacy.close().unwrap();
    server.finish().unwrap();
}
