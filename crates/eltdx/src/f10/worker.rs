use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bytes::Bytes;
use eltdx_runtime::execution::AttemptAuthorization;
use url::Url;

use super::control::Control;
use super::http::WireReply;
use super::F10Config;
use crate::operation::OperationContext;
use crate::session::OwnerTaskGroup;
use crate::{Error, ErrorKind, Result, TimeoutPhase};

pub(crate) enum Work {
    Connect(Url),
    Post {
        url: Url,
        body: Bytes,
        retain_payload: bool,
        retain_wire: bool,
    },
}
struct State {
    handle: Option<JoinHandle<()>>,
    result: Option<Result<WireReply>>,
    done: bool,
}
pub(crate) struct Job {
    state: Mutex<State>,
    changed: Condvar,
    stop: Arc<AtomicBool>,
    _slot: Mutex<Option<Slot>>,
}
impl std::fmt::Debug for Job {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("F10Job").finish_non_exhaustive()
    }
}
impl Job {
    pub fn execute(
        config: F10Config,
        work: Work,
        authorization: Arc<dyn AttemptAuthorization>,
        context: &OperationContext,
        slot: Slot,
    ) -> Result<WireReply> {
        let job = Arc::new(Self {
            state: Mutex::new(State {
                handle: None,
                result: None,
                done: false,
            }),
            changed: Condvar::new(),
            stop: Arc::new(AtomicBool::new(false)),
            _slot: Mutex::new(Some(slot)),
        });
        let registered: Arc<dyn OwnerTaskGroup> = job.clone();
        context
            .owner()
            .register_task_group(Arc::clone(&registered))?;
        let deadline = context
            .sponsor()
            .deadline()
            .unwrap_or_else(|| Instant::now() + config.timeout);
        let control = Control {
            sponsor: context.sponsor(),
            stop: Arc::clone(&job.stop),
            deadline,
        };
        let task = Arc::clone(&job);
        let mut state = job.state.lock().map_err(|_| poisoned())?;
        let handle = thread::Builder::new()
            .name("eltdx-f10-operation".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    control.check(TimeoutPhase::Startup)?;
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| {
                            Error::new(ErrorKind::Internal, "cannot start F10 runtime")
                                .with_source(error)
                        })?;
                    let result = runtime.block_on(async {
                        match work {
                            Work::Connect(url) => {
                                super::http::connect(&config, &url, authorization, &control).await
                            }
                            Work::Post {
                                url,
                                body,
                                retain_payload,
                                retain_wire,
                            } => {
                                super::http::post(
                                    &config,
                                    url,
                                    body,
                                    authorization,
                                    &control,
                                    retain_payload,
                                    retain_wire,
                                )
                                .await
                            }
                        }
                    });
                    // No blocking async tasks are spawned. Runtime drop cancels and
                    // destroys all remaining DNS/connection futures synchronously.
                    drop(runtime);
                    result
                }))
                .unwrap_or_else(|_| Err(Error::new(ErrorKind::Internal, "F10 worker panicked")));
                let mut state = task
                    .state
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                state.result = Some(result);
                state.done = true;
                task.changed.notify_all();
            });
        match handle {
            Ok(handle) => state.handle = Some(handle),
            Err(error) => {
                state.done = true;
                drop(state);
                context.owner().unregister_task_group(&registered);
                return Err(
                    Error::new(ErrorKind::Internal, "cannot spawn F10 worker").with_source(error)
                );
            }
        }
        drop(state);
        let guard = StopOnDrop(Arc::clone(&job));
        let output = loop {
            if let Err(root) = context.check_boundary(TimeoutPhase::Response) {
                job.stop();
                context.ensure_cleanup_budget()?;
                let deadline = Instant::now()
                    .checked_add(context.cleanup_remaining()?)
                    .ok_or_else(poisoned)?;
                match job.close_until(deadline) {
                    Ok(()) => {
                        context.owner().unregister_task_group(&registered);
                        break Err(root);
                    }
                    Err(cleanup) => {
                        break Err(Error::new(
                            ErrorKind::CancelUnconfirmed,
                            "F10 cancellation could not confirm task release",
                        )
                        .with_source(root)
                        .with_cleanup_failure(cleanup))
                    }
                }
            }
            let mut state = job.state.lock().map_err(|_| poisoned())?;
            if state.done
                && state
                    .handle
                    .as_ref()
                    .is_none_or(|handle| handle.is_finished())
            {
                let result = state.result.take().ok_or_else(poisoned)?;
                drop(state);
                // A completed call must not start the parent's terminal cleanup
                // budget: later pages/prerequisites still share that parent.
                if let Err(cleanup) = job.close_until(Instant::now()) {
                    let root = match result {
                        Err(root) => root,
                        Ok(_) => cleanup.clone(),
                    };
                    return Err(Error::new(
                        ErrorKind::CancelUnconfirmed,
                        "F10 completed worker could not be joined",
                    )
                    .with_source(root)
                    .with_cleanup_failure(cleanup));
                }
                context.owner().unregister_task_group(&registered);
                break result;
            }
            state = job
                .changed
                .wait_timeout(state, Duration::from_millis(10))
                .map_err(|_| poisoned())?
                .0;
            drop(state);
        };
        drop(guard);
        output
    }
}
struct StopOnDrop(Arc<Job>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}
impl OwnerTaskGroup for Job {
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.changed.notify_all();
    }
    fn close_until(&self, deadline: Instant) -> Result<()> {
        self.stop();
        let handle = {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            while !state.done
                || state
                    .handle
                    .as_ref()
                    .is_some_and(|handle| !handle.is_finished())
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::new(
                        ErrorKind::CloseTimeout,
                        "F10 worker did not retire before cleanup deadline",
                    ));
                }
                state = self
                    .changed
                    .wait_timeout(state, remaining.min(Duration::from_millis(2)))
                    .map_err(|_| poisoned())?
                    .0;
            }
            state.handle.take()
        };
        if let Some(handle) = handle {
            handle
                .join()
                .map_err(|_| Error::new(ErrorKind::Internal, "F10 worker join failed"))?;
        }
        self._slot.lock().map_err(|_| poisoned())?.take();
        Ok(())
    }
    fn completed(&self) -> bool {
        self.state.lock().map(|state| state.done).unwrap_or(false)
    }
}
fn poisoned() -> Error {
    Error::new(ErrorKind::Internal, "F10 job state unavailable")
}

/// Limit active per-call runtimes before starting threads. Waiting callers are
/// synchronous, cancellation-aware and covered by the original operation clock.
#[derive(Debug)]
pub(crate) struct Concurrency {
    active: Mutex<usize>,
    changed: Condvar,
    maximum: usize,
}
impl Concurrency {
    pub fn new(maximum: usize) -> Self {
        Self {
            active: Mutex::new(0),
            changed: Condvar::new(),
            maximum,
        }
    }
    pub fn acquire(self: &Arc<Self>, context: &OperationContext) -> Result<Slot> {
        let mut active = self.active.lock().map_err(|_| poisoned())?;
        loop {
            // Completed jobs whose callers observed unconfirmed cleanup stay
            // registered. Reap them outside the gate lock before waiting for a
            // slot, so they cannot strand capacity or deadlock slot release.
            drop(active);
            context.owner().reap_completed_tasks();
            active = self.active.lock().map_err(|_| poisoned())?;
            context.check_boundary(TimeoutPhase::Queue)?;
            if *active < self.maximum {
                *active += 1;
                return Ok(Slot {
                    _lease: Arc::new(Lease(Arc::clone(self))),
                    prerequisites: Vec::new(),
                });
            }
            active = self
                .changed
                .wait_timeout(active, Duration::from_millis(10))
                .map_err(|_| poisoned())?
                .0;
        }
    }
}
#[derive(Clone)]
pub(crate) struct Slot {
    _lease: Arc<Lease>,
    prerequisites: Vec<Slot>,
}
impl Slot {
    /// A probe's serial admission must remain held by the registered job if
    /// its caller cannot confirm cleanup. Clones share the underlying leases.
    pub fn holding(mut self, prerequisite: Slot) -> Self {
        self.prerequisites.push(prerequisite);
        self
    }
}
struct Lease(Arc<Concurrency>);
impl Drop for Lease {
    fn drop(&mut self) {
        let mut active = self
            .0
            .active
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *active = active.saturating_sub(1);
        self.0.changed.notify_one();
    }
}
