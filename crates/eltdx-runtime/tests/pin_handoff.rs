use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions, PendingPoll};
use eltdx_runtime::endpoint::Endpoint;
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
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?.to_string();
    let (accepted_tx, accepted_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("runtime connects to test listener");
        accepted_tx.send(()).expect("test observes accepted connection");
        release_rx.recv().expect("test releases server stream");
        drop(stream);
    });

    let engine = Engine::new(engine_config(address)?)?;
    let mut pending = engine.begin_pin()?;
    accepted_rx.recv().expect("pin connection is accepted");
    let mut handle = match pending.wait_timeout(Duration::from_secs(10))? {
        PendingPoll::Ready(handle) => handle,
        PendingPoll::Pending => {
            return Err(RuntimeError::internal(
                "pin did not reach its published result after connection acceptance",
            ));
        }
    };

    pending.cancel_and_confirm(Duration::ZERO)?;
    assert_eq!(engine.pool_diagnostics()?.broker.unwrap().active_leases, 1);

    handle.begin_close()?.wait()?;
    assert_eq!(engine.pool_diagnostics()?.broker.unwrap().active_leases, 0);

    let mut next_pending = engine.begin_pin()?;
    let mut next_handle = match next_pending.wait_timeout(Duration::from_secs(10))? {
        PendingPoll::Ready(handle) => handle,
        PendingPoll::Pending => {
            return Err(RuntimeError::internal(
                "a released Slot did not admit the next pin reservation",
            ));
        }
    };
    assert_eq!(engine.pool_diagnostics()?.broker.unwrap().active_leases, 1);
    next_handle.begin_close()?.wait()?;
    assert_eq!(engine.pool_diagnostics()?.broker.unwrap().active_leases, 0);

    engine.close()?;
    release_tx.send(()).expect("server stream waiter is present");
    server.join().expect("test server exits");
    Ok(())
}
