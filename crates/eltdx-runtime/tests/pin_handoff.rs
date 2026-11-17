#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eltdx_runtime::endpoint::Endpoint;
use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions, PendingPoll};
use eltdx_runtime::RuntimeError;

fn engine_config(address: String) -> Result<EngineConfig, RuntimeError> {
    EngineConfig::from_endpoints(
        vec![Endpoint::numeric(&address)?],
        Duration::from_secs(10),
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
            max_pending_requests: 1,
            push_queue_size: 8,
            push_queue_bytes: 1024,
        },
    )
}

#[test]
fn cancel_after_ready_does_not_revoke_delivered_pin() -> Result<(), RuntimeError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| RuntimeError::internal(format!("fixture bind: {error}")))?;
    let address = listener
        .local_addr()
        .map_err(|error| RuntimeError::internal(format!("fixture address: {error}")))?
        .to_string();
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("runtime connects to test listener");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("bounded handshake read");
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .expect("bounded handshake write");
        accepted_tx
            .send(())
            .expect("test observes accepted connection");
        let mut header = [0_u8; 12];
        stream
            .read_exact(&mut header)
            .expect("setup request header");
        assert_eq!(u16::from_le_bytes([header[10], header[11]]), 0x000d);
        let size = usize::from(u16::from_le_bytes([header[6], header[7]]));
        let mut body = vec![0; size.checked_sub(2).expect("setup length")];
        stream.read_exact(&mut body).expect("setup request body");
        let mut reply = eltdx_protocol::frame::RESPONSE_PREFIX.to_vec();
        reply.push(1);
        reply.extend_from_slice(&header[1..5]);
        reply.push(0);
        reply.extend_from_slice(&header[10..12]);
        reply.extend_from_slice(&189_u16.to_le_bytes());
        reply.extend_from_slice(&189_u16.to_le_bytes());
        reply.extend_from_slice(&[0; 189]);
        stream.write_all(&reply).expect("setup reply");
        release_rx.recv().expect("test releases server stream");
        drop(stream);
    });

    let engine = Engine::new(engine_config(address)?)?;
    let mut pending = engine.begin_pin()?;
    accepted_rx.recv().expect("pin connection is accepted");
    let handle = match pending.wait_timeout(Duration::from_secs(10))? {
        PendingPoll::Ready(handle) => handle,
        PendingPoll::Pending => {
            return Err(RuntimeError::internal(
                "pin did not reach its published result after connection acceptance",
            ));
        }
    };

    let source = handle
        .source_identity(Duration::from_secs(1))?
        .ok_or_else(|| RuntimeError::internal("live pin inspection elapsed"))?;
    pending.cancel_and_confirm(Duration::ZERO)?;
    // Pool diagnostics are cached. Inspect the live reservation instead of
    // assuming its next 25ms diagnostic refresh has already happened.
    assert_eq!(
        handle.source_identity(Duration::from_secs(1))?,
        Some(source)
    );

    let mut next_pending = engine.begin_pin()?;
    assert!(matches!(
        next_pending.wait_timeout(Duration::from_millis(25))?,
        PendingPoll::Pending
    ));
    handle.begin_close()?.wait()?;
    assert!(handle.source_identity(Duration::from_secs(1)).is_err());

    let next_handle = match next_pending.wait_timeout(Duration::from_secs(10))? {
        PendingPoll::Ready(handle) => handle,
        PendingPoll::Pending => {
            return Err(RuntimeError::internal(
                "a released Slot did not admit the next pin reservation",
            ));
        }
    };
    assert!(next_handle
        .source_identity(Duration::from_secs(1))?
        .is_some());
    next_handle.begin_close()?.wait()?;
    assert!(next_handle.source_identity(Duration::from_secs(1)).is_err());

    engine.close()?;
    release_tx
        .send(())
        .expect("server stream waiter is present");
    server.join().expect("test server exits");
    Ok(())
}
