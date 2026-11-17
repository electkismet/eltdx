//! Private measurement seam: real SDK requests with actual engine budgets.
//! This module is compiled only into the ignored library test executable.
#[path = "../tests/support/hq_server.rs"] mod hq_server;
#[path = "../tests/support/stress_log.rs"] mod stress_log;
use crate::{CodeQuery, HqMarket, OperationOptions, RawPolicy, TdxClient};
use std::io;
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use stress_log::Log;
type Case = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn sample(client: &TdxClient, log: &Log) -> Case {
    let diagnostic = client.engine.pool_diagnostics()?;
    assert!(diagnostic.raw_bytes <= diagnostic.raw_max_bytes);
    assert!(diagnostic.raw_peak_bytes <= diagnostic.raw_max_bytes);
    assert!(diagnostic.decoded_bytes <= diagnostic.decoded_max_bytes);
    assert!(diagnostic.decoded_peak_bytes <= diagnostic.decoded_max_bytes);
    let queue = diagnostic.broker.as_ref().map_or(0, |broker| broker.waiter_count + broker.pin_waiter_count);
    assert!(queue <= 256);
    let live = diagnostic.actors.iter().filter(|actor| actor.actor_alive).count();
    log.write(format!("\"kind\":\"budget\",\"raw\":{},\"raw_max\":{},\"raw_peak\":{},\"decoded\":{},\"decoded_max\":{},\"decoded_peak\":{},\"configured_workers\":{},\"live_actors\":{live},\"queue\":{queue},\"queue_max\":256",
        diagnostic.raw_bytes, diagnostic.raw_max_bytes, diagnostic.raw_peak_bytes,
        diagnostic.decoded_bytes, diagnostic.decoded_max_bytes, diagnostic.decoded_peak_bytes, diagnostic.runtime_workers))?;
    for (slot, actor) in diagnostic.actors.iter().enumerate() {
        log.write(format!("\"kind\":\"generation\",\"slot\":{slot},\"epoch\":{},\"generation\":{},\"alive\":{},\"pending\":{},\"stale_events\":{}",
            actor.runtime_epoch, actor.tcp_generation, actor.actor_alive, actor.pending_depth, actor.stale_event_count))?;
    }
    Ok(())
}

#[test]
#[ignore = "candidate-only: private SDK budget/generation telemetry with real slow loopback requests"]
fn stress_actual_sdk_budgets_with_slow_server_and_bounded_queue() -> Case {
    let workers = 32; let per_worker = 1_000;
    let log = Log::start("budgets", workers * per_worker, workers)?;
    let captured = log.clone();
    let server = hq_server::Server::start_controlled(move |request, connection| {
        assert_eq!(request.msg_type, 0x044d);
        let id = hq_server::directory_offset(request) as usize;
        if id < workers * per_worker { captured.wire_for(id, connection.identity(), request.msg_id, request.msg_type).expect("wire evidence"); }
        else { captured.wire(connection.identity(), request.msg_id, request.msg_type).expect("warmup wire evidence"); }
        thread::park_timeout(Duration::from_millis(1));
        hq_server::directory(&[(&format!("{:06}", 100_000 + id), 2)])
    })?;
    let client = TdxClient::builder().endpoints(vec![server.address]).pool_size(4)
        .heartbeat_interval(None).request_timeout(Duration::from_secs(10)).build()?;
    let warmup = CodeQuery::new().with_start((workers * per_worker) as u32)?.with_limit(1)?;
    for _ in 0..128 {
        assert_eq!(client.codes().list(HqMarket::Shanghai, &warmup,
            &OperationOptions::default().with_raw(RawPolicy::RetainWire))?.items().len(), 1);
    }
    log.write("\"kind\":\"configuration\",\"pool\":4,\"max_pending\":256,\"directory_payload_bytes\":39,\"server_delay_ms\":1,\"warmup_requests\":128".into())?;
    log.warmed()?;
    sample(&client, &log)?;
    let finished = Arc::new(AtomicBool::new(false));
    let sampler = {
        let client = client.clone(); let log = log.clone(); let finished = Arc::clone(&finished);
        thread::spawn(move || -> Case {
            while !finished.load(Ordering::Acquire) {
                sample(&client, &log)?; thread::park_timeout(Duration::from_millis(10));
            }
            sample(&client, &log)
        })
    };
    let barrier = Arc::new(Barrier::new(workers));
    let mut jobs = Vec::new();
    for worker in 0..workers {
        let client = client.clone(); let log = log.clone(); let barrier = Arc::clone(&barrier);
        jobs.push(thread::spawn(move || -> Case {
            barrier.wait();
            for index in 0..per_worker {
                let id = worker * per_worker + index; let started = Instant::now();
                let query = CodeQuery::new().with_start(id as u32)?.with_limit(1)?;
                let rows = client.codes().list(HqMarket::Shanghai, &query,
                    &OperationOptions::default().with_raw(RawPolicy::RetainWire))?;
                assert_eq!(rows.items().len(), 1);
                assert_eq!(rows.items()[0].id().number(), format!("{:06}", 100_000 + id));
                log.terminal(id, "completed", started.elapsed())?;
            }
            Ok(())
        }));
    }
    let mut failure: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for job in jobs {
        let result = job.join().unwrap_or_else(|_| Err(io::Error::other("budget workload panicked").into()));
        if let Err(error) = result { failure.get_or_insert(error); }
    }
    finished.store(true, Ordering::Release);
    let sampled = sampler.join().map_err(|_| io::Error::other("budget sampler panicked"));
    let shutdown = client.close();
    let peers = server.finish();
    sampled??; shutdown?; peers?;
    sample(&client, &log)?;
    let final_budget = client.engine.pool_diagnostics()?;
    assert_eq!(final_budget.raw_bytes, 0);
    assert_eq!(final_budget.decoded_bytes, 0);
    assert!(final_budget.raw_peak_bytes > 0);
    assert!(final_budget.decoded_peak_bytes > 0);
    if let Some(error) = failure { return Err(error); }
    log.finish(workers * per_worker)?; Ok(())
}
