//! One registered, cancellation-aware job per admitted complete exchange.
//! A worker holds no SessionOwner. Unconfirmed retirement retains its gate
//! lease in the owner's task registry, preventing another call using that slot.
use super::config::ExConfig;
use super::control::Control;
use super::session::{poisoned, Session, WorkerSlot};
use super::transport::{WireReply, Work};
use crate::operation::OperationContext;
use crate::session::OwnerTaskGroup;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use eltdx_runtime::execution::AttemptAuthorization;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct State<T> {
    handle: Option<JoinHandle<()>>,
    output: Option<Result<T>>,
    done: bool,
}
struct Job<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
    stopped: Arc<AtomicBool>,
    slot: Mutex<Option<WorkerSlot>>,
}
impl<T> std::fmt::Debug for Job<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExtensionJob")
            .finish_non_exhaustive()
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "Keep the checked operation context, query and source provenance explicit."
)]
pub(crate) fn execute<T: Send + 'static>(
    session: Arc<Session>,
    config: ExConfig,
    work: Work,
    maximum_attempts: u32,
    authorization: Arc<dyn AttemptAuthorization>,
    context: &OperationContext,
    slot: WorkerSlot,
    decode: impl FnOnce(WireReply) -> Result<T> + Send + 'static,
) -> Result<T> {
    let owner_stopped = Arc::clone(&session.stopped);
    run(owner_stopped, context, slot, move |control| {
        control.check(TimeoutPhase::Startup)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                Error::new(ErrorKind::Internal, "cannot start extension runtime").with_source(error)
            })?;
        let exchange = runtime.block_on(super::transport::execute(
            &session,
            &config,
            &work,
            maximum_attempts,
            &authorization,
            &control,
        ));
        // All network futures are confined to this runtime. The successful
        // socket is ordinary nonblocking std I/O before the reactor is dropped.
        drop(runtime);
        let (reply, connection) = exchange?;
        // The socket remains local throughout strict command decoding. A
        // decoder error retires it and cannot enter the transport retry loop.
        let value = decode(reply)?;
        control.check(TimeoutPhase::Decode)?;
        session.publish(connection, &control.stopped)?;
        Ok(value)
    })
}

/// The same registered execution shell also owns a bounded discovery runtime.
/// The action receives only owner-free control, never an Arc<SessionOwner>.
pub(crate) fn run<T: Send + 'static>(
    owner_stopped: Arc<AtomicBool>,
    context: &OperationContext,
    slot: WorkerSlot,
    action: impl FnOnce(Control) -> Result<T> + Send + 'static,
) -> Result<T> {
    let job = Arc::new(Job {
        state: Mutex::new(State {
            handle: None,
            output: None,
            done: false,
        }),
        changed: Condvar::new(),
        stopped: Arc::new(AtomicBool::new(false)),
        slot: Mutex::new(Some(slot)),
    });
    let registered: Arc<dyn OwnerTaskGroup> = job.clone();
    context
        .owner()
        .register_task_group(Arc::clone(&registered))?;
    let control = Control {
        sponsor: context.sponsor(),
        stopped: Arc::clone(&job.stopped),
        owner_stopped,
        deadline: context.sponsor().deadline().ok_or_else(|| {
            Error::new(
                ErrorKind::Internal,
                "EX public worker needs an absolute deadline",
            )
        })?,
    };
    let task = Arc::clone(&job);
    let mut state = job.state.lock().map_err(|_| poisoned())?;
    let handle = thread::Builder::new()
        .name("eltdx-extension-operation".to_owned())
        .spawn(move || {
            let output = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(control)))
                .unwrap_or_else(|_| {
                    Err(Error::new(ErrorKind::Internal, "extension worker panicked"))
                });
            let mut state = task
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.output = Some(output);
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
                Error::new(ErrorKind::Internal, "cannot spawn extension worker").with_source(error),
            );
        }
    }
    drop(state);
    let guard = StopOnDrop(Arc::clone(&job));
    let result = loop {
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
                        "extension cancellation could not confirm retirement",
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
            let output = state.output.take().ok_or_else(poisoned)?;
            drop(state);
            // Normal child completion never starts its parent's terminal
            // cleanup budget; later collection pages share the same parent.
            if let Err(cleanup) = job.close_until(Instant::now()) {
                let root = match output {
                    Err(root) => root,
                    Ok(_) => cleanup.clone(),
                };
                return Err(Error::new(
                    ErrorKind::CancelUnconfirmed,
                    "extension completed worker join unconfirmed",
                )
                .with_source(root)
                .with_cleanup_failure(cleanup));
            }
            context.owner().unregister_task_group(&registered);
            break output;
        }
        state = job
            .changed
            .wait_timeout(state, Duration::from_millis(10))
            .map_err(|_| poisoned())?
            .0;
        drop(state);
    };
    drop(guard);
    result
}
struct StopOnDrop<T: Send + 'static>(Arc<Job<T>>);
impl<T: Send + 'static> Drop for StopOnDrop<T> {
    fn drop(&mut self) {
        self.0.stop();
    }
}
impl<T: Send + 'static> OwnerTaskGroup for Job<T> {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
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
                        "extension worker retirement deadline elapsed",
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
                .map_err(|_| Error::new(ErrorKind::Internal, "extension worker join failed"))?;
        }
        self.slot.lock().map_err(|_| poisoned())?.take();
        Ok(())
    }
    fn completed(&self) -> bool {
        self.state.lock().map(|state| state.done).unwrap_or(false)
    }
}
