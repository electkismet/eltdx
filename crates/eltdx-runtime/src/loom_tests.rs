use std::time::{Duration, Instant};

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex, MutexGuard};
use loom::thread;

use crate::deadline::Deadline;
use crate::slot::RequestId;
use crate::supervisor::{CloseClaim, EngineState, StartClaim, Supervisor};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn running_supervisor() -> Supervisor {
    let mut supervisor = match Supervisor::with_admission(1, 1) {
        Ok(supervisor) => supervisor,
        Err(error) => panic!("failed to create Loom supervisor: {error}"),
    };
    let attempt = match supervisor.begin_start() {
        Ok(StartClaim::Owner(attempt)) => attempt,
        Ok(other) => panic!("unexpected Loom start claim: {other:?}"),
        Err(error) => panic!("failed to claim Loom startup: {error}"),
    };
    match supervisor.publish_start(attempt) {
        Ok(true) => supervisor,
        Ok(false) => panic!("Loom startup publication was rejected"),
        Err(error) => panic!("failed to publish Loom startup: {error}"),
    }
}

#[test]
fn loom_submit_and_close_share_one_linearization_gate() {
    loom::model(|| {
        let supervisor = Arc::new(Mutex::new(running_supervisor()));
        let submit_outcome = Arc::new(AtomicUsize::new(0));

        let submit_supervisor = Arc::clone(&supervisor);
        let submit_result = Arc::clone(&submit_outcome);
        let submitter = thread::spawn(move || {
            let request_id = match RequestId::new(1) {
                Ok(request_id) => request_id,
                Err(error) => panic!("failed to create Loom request id: {error}"),
            };
            let deadline = Deadline::at(Instant::now() + Duration::from_secs(1));
            let outcome = if lock(&submit_supervisor)
                .submit(request_id, deadline, Instant::now())
                .is_ok()
            {
                1
            } else {
                2
            };
            submit_result.store(outcome, Ordering::SeqCst);
        });

        let close_supervisor = Arc::clone(&supervisor);
        let closer = thread::spawn(move || {
            let claim = lock(&close_supervisor).begin_close();
            assert!(matches!(claim, Ok(CloseClaim::Owner(_))));
        });

        assert!(submitter.join().is_ok());
        assert!(closer.join().is_ok());
        let outcome = submit_outcome.load(Ordering::SeqCst);
        let mut supervisor = lock(&supervisor);
        assert!(matches!(outcome, 1 | 2));
        assert_eq!(supervisor.state(), EngineState::Closing);
        assert_eq!(supervisor.waiting_count(), 0);
        assert_eq!(supervisor.active_count(), 0);
        assert_eq!(
            supervisor.take_lifecycle_notifications().len(),
            usize::from(outcome == 1)
        );
        assert!(supervisor.check_admission_invariants().is_ok());
    });
}

#[test]
fn loom_concurrent_close_claims_share_one_identity() {
    loom::model(|| {
        let supervisor = Arc::new(Mutex::new(running_supervisor()));
        let identities = Arc::new(Mutex::new(Vec::with_capacity(2)));
        let mut threads = Vec::with_capacity(2);

        for _ in 0..2 {
            let close_supervisor = Arc::clone(&supervisor);
            let close_identities = Arc::clone(&identities);
            threads.push(thread::spawn(move || {
                let identity = match lock(&close_supervisor).begin_close() {
                    Ok(CloseClaim::Owner(attempt) | CloseClaim::Existing(attempt)) => {
                        attempt.id().get()
                    }
                    Ok(other) => panic!("unexpected concurrent Loom close claim: {other:?}"),
                    Err(error) => panic!("failed concurrent Loom close claim: {error}"),
                };
                lock(&close_identities).push(identity);
            }));
        }

        for close_thread in threads {
            assert!(close_thread.join().is_ok());
        }
        let identities = lock(&identities);
        assert_eq!(identities.len(), 2);
        assert_ne!(identities[0], 0);
        assert_eq!(identities[0], identities[1]);
        assert_eq!(lock(&supervisor).state(), EngineState::Closing);
    });
}
