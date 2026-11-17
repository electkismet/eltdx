use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use eltdx_runtime::engine::{Engine, PinHandle, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::push::PushFrame;
use eltdx_runtime::slot::GenerationIdentity;

use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{CancellationToken, ChildPurpose, OperationContext, OperationOptions};
use crate::services::{
    aggregate_meta, checked_ids, pinned_snapshot_batch, project_refresh_updates, refresh_command,
};
use crate::session::{OwnerTaskGroup, PinnedSession, SessionOwner};
use crate::types::{
    BatchItem, GapReason, PushEvent, PushGap, QuoteUpdates, RawPolicy, RefreshRequest, SecurityId,
    SnapshotBatch, SubscriptionOptions,
};

use super::cursor::CursorBook;
use super::dispatcher::{Dispatcher, PushSink, RouteLease};
use super::queue::{EventQueue, QueueAction};

#[derive(Debug)]
struct Group {
    pin: PinHandle,
    source: GenerationIdentity,
    ids: Vec<SecurityId>,
    cursors: Mutex<CursorBook>,
}

#[derive(Debug)]
struct State {
    queue: EventQueue,
    routes: Vec<RouteLease>,
    armed: bool,
    stopping: bool,
    own_closed: bool,
    deadline: Option<Instant>,
    worker: Option<JoinHandle<()>>,
    retiring: bool,
    retired: bool,
    cleanup_error: Option<Error>,
}

/// Background work retains only this owner-free state. SessionOwner may retain
/// it as a task, but each worker upgrade lasts for a finite operation only.
#[derive(Debug)]
struct Control {
    owner: Weak<SessionOwner>,
    engine: Engine,
    config: ClientConfig,
    metadata: Arc<MetadataSnapshot>,
    raw: RawPolicy,
    policy: SubscriptionOptions,
    dispatcher: Arc<Dispatcher>,
    groups: Vec<Group>,
    token: CancellationToken,
    state: Mutex<State>,
    changed: Condvar,
}

/// Owned single-reader subscription. Dropping it signals bounded retirement;
/// explicit close confirms retirement without affecting other subscriptions.
#[derive(Debug)]
pub struct Subscription {
    owner: Arc<SessionOwner>,
    control: Arc<Control>,
}

impl Subscription {
    pub fn next_event(&mut self, options: &OperationOptions) -> Result<Option<PushEvent>> {
        let entry = Instant::now();
        if self
            .control
            .state
            .lock()
            .map_err(|_| poisoned())?
            .own_closed
        {
            return Ok(None);
        }
        let context = OperationContext::rust_public(
            entry,
            "subscription_next_event",
            self.owner.clone(),
            &self.control.config,
            options,
        )?;
        loop {
            context.check_boundary(TimeoutPhase::Queue)?;
            let mut state = self.control.state.lock().map_err(|_| poisoned())?;
            if state.own_closed {
                return Ok(None);
            }
            // Parent close is checked before the pending Gap/data. It never
            // depends on a user performing another read to trigger retirement.
            self.owner.ensure_open()?;
            if state.queue.ready() {
                // Arbitrate cancellation before removing an event, so a
                // cancelled read leaves the subscription and its event intact.
                context.finish()?;
                return state.queue.pop();
            }
            state = self
                .control
                .changed
                .wait_timeout(state, SIGNAL_POLL_INTERVAL)
                .map_err(|_| poisoned())?
                .0;
            drop(state);
        }
    }

    pub fn close(&mut self) -> Result<()> {
        self.control.own_stop();
        let deadline = Instant::now()
            .checked_add(self.control.config.close_timeout())
            .ok_or_else(clock_overflow)?;
        self.control.close_until(deadline)?;
        let task: Arc<dyn OwnerTaskGroup> = self.control.clone();
        self.owner.unregister_task_group(&task);
        Ok(())
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.control.own_stop();
    }
}

pub(crate) fn establish(
    entry: Instant,
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: &str,
    ids: &[SecurityId],
    policy: &SubscriptionOptions,
    options: &OperationOptions,
) -> Result<Subscription> {
    let context = executor.begin_context_at(entry, "quotes_subscribe", &config, options)?;
    checked_ids(ids)?;
    let mut seen = HashSet::new();
    seen.try_reserve(ids.len()).map_err(allocation)?;
    let mut unique = Vec::new();
    unique.try_reserve(ids.len()).map_err(allocation)?;
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id.clone());
        }
    }
    if unique.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "subscription requires at least one security",
        ));
    }
    let group_count = unique.len().div_ceil(100);
    if group_count > config.pool_size() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "subscription groups exceed configured pool capacity",
        )
        .with_context("groups", group_count.to_string())
        .with_context("pool_size", config.pool_size().to_string()));
    }
    let host_bytes = crate::client::configured_hosts(&config)
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(0);
    let owner_bytes = format!("owner-{:x}", executor.owner().generation()).len();
    let maximum =
        super::memory::maximum_event_bytes(unique.len(), host_bytes, owner_bytes, context.raw())?
            .checked_add(EventQueue::storage_bytes(policy.max_events())?)
            .ok_or_else(clock_overflow)?;
    policy.validate(maximum, entry)?;
    let queue = EventQueue::new(policy)?;
    let requests = unique
        .chunks(100)
        .map(|part| {
            refresh_command(
                &part
                    .iter()
                    .cloned()
                    .map(RefreshRequest::new)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    // Cold metadata must complete before taking even the first reservation.
    let metadata = executor.prepare_metadata(&engine, &context, &config, namespace, &requests)?;
    let dispatcher = executor.owner().push_dispatcher(&engine)?;
    let pause = dispatcher.pause(&context)?;
    let mut pins: Vec<PinnedSession> = Vec::new();
    pins.try_reserve(group_count).map_err(allocation)?;
    let mut groups = Vec::new();
    groups.try_reserve(group_count).map_err(allocation)?;
    let mut batches = Vec::new();
    batches.try_reserve(group_count).map_err(allocation)?;
    let setup = (|| {
        for (part, request) in unique.chunks(100).zip(requests) {
            context.check_boundary(TimeoutPhase::Admission)?;
            // Push the new guard before activation: the failing group's own
            // reservation participates in the same rollback as prior groups.
            pins.push(executor.acquire_pin(&engine, &context)?);
            let pin = pins.last().expect("newly acquired pin").handle()?;
            let raw = executor.execute_pinned_raw(
                &engine,
                pin,
                &context,
                request,
                ChildPurpose::Required,
                None,
            )?;
            let source = GenerationIdentity {
                engine_epoch: raw.engine_epoch,
                slot_id: raw.slot_id,
                generation: raw.generation,
            };
            let decoded = executor.decode_received(&context, raw)?;
            let updates = project_refresh_updates(
                part,
                &context,
                &config,
                &metadata,
                &decoded,
                "quotes.subscription",
            )?;
            let mut cursors = CursorBook::new(source, part)?;
            let (_, mut updates, _) = updates.into_parts();
            cursors.remember_ordered(source, decoded.source.receive_sequence, &mut updates)?;
            let (batch, floors) = pinned_snapshot_batch(
                &executor, &engine, pin, &context, &config, &metadata, part, source,
            )?;
            cursors.snapshot_floors(&floors)?;
            batches.push(batch);
            groups.push(Group {
                pin: pin.clone(),
                source,
                ids: part.to_vec(),
                cursors: Mutex::new(cursors),
            });
        }
        combine_batches(&context, &metadata, batches)
    })();
    let initial = match setup {
        Ok(initial) => initial,
        Err(root) => return Err(rollback(&executor, &context, &mut pins, root)),
    };
    let control = Arc::new(Control {
        owner: Arc::downgrade(executor.owner()),
        engine,
        config,
        metadata,
        raw: context.raw(),
        policy: policy.clone(),
        dispatcher: dispatcher.clone(),
        groups,
        token: CancellationToken::default(),
        state: Mutex::new(State {
            queue,
            routes: Vec::new(),
            armed: false,
            stopping: false,
            own_closed: false,
            deadline: None,
            worker: None,
            retiring: false,
            retired: false,
            cleanup_error: None,
        }),
        changed: Condvar::new(),
    });
    let finish = (|| {
        if control
            .state
            .lock()
            .map_err(|_| poisoned())?
            .queue
            .publish(PushEvent::Initial(initial))?
            == QueueAction::Stop
        {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "initial subscription event exceeds its configured capacity",
            ));
        }
        let target: Arc<dyn PushSink> = control.clone();
        let mut routes = Vec::new();
        routes
            .try_reserve(control.groups.len())
            .map_err(allocation)?;
        for group in &control.groups {
            routes.push(dispatcher.register(group.source, &target)?);
        }
        control.state.lock().map_err(|_| poisoned())?.routes = routes;
        executor
            .owner()
            .register_task_group(control.clone() as Arc<dyn OwnerTaskGroup>)?;
        Ok(())
    })();
    if let Err(root) = finish {
        return Err(rollback(&executor, &context, &mut pins, root));
    }
    // From this point the registered task owns retirement, including startup
    // failure. The user-facing guards must not independently release these pins.
    let publish = (|| {
        for pin in pins.drain(..) {
            let _ = pin.transfer_to_task()?;
        }
        control.start()?;
        let mut state = control.state.lock().map_err(|_| poisoned())?;
        if state.stopping {
            return Err(Error::new(
                ErrorKind::Closed,
                "subscription stopped before establishment completed",
            ));
        }
        context.finish()?;
        state.armed = true;
        control.changed.notify_all();
        Ok(())
    })();
    if let Err(root) = publish {
        control.own_stop();
        let cleanup = (|| {
            context.ensure_cleanup_budget()?;
            let deadline = Instant::now()
                .checked_add(context.cleanup_remaining()?)
                .ok_or_else(clock_overflow)?;
            control.close_until(deadline)
        })();
        return Err(match cleanup {
            Ok(()) => {
                let task: Arc<dyn OwnerTaskGroup> = control.clone();
                executor.owner().unregister_task_group(&task);
                root
            }
            // An unconfirmed task stays registered so parent close still owns
            // every reservation and can report its retirement outcome.
            Err(cleanup) => root.with_cleanup_failure(cleanup),
        });
    }
    drop(pause);
    Ok(Subscription {
        owner: executor.owner().clone(),
        control,
    })
}

fn rollback(
    executor: &HqExecutor,
    context: &OperationContext,
    pins: &mut [PinnedSession],
    root: Error,
) -> Error {
    if let Err(cleanup) = context.ensure_cleanup_budget() {
        return root.with_cleanup_failure(cleanup);
    }
    let mut error = root;
    for pin in pins.iter_mut().rev() {
        // All rollback retirements use the same cleanup deadline. Keep the
        // original request cause even if more than one retirement fails.
        if let Err(retired) = executor.release_pin(context, pin, Some(error.clone())) {
            error = retired;
        }
    }
    error
}

fn combine_batches(
    context: &OperationContext,
    metadata: &MetadataSnapshot,
    batches: Vec<SnapshotBatch>,
) -> Result<SnapshotBatch> {
    let mut items = Vec::new();
    let mut metas = Vec::new();
    for batch in batches {
        let (rows, meta) = batch.into_parts();
        items.try_reserve(rows.len()).map_err(allocation)?;
        for row in rows {
            items.push(BatchItem::new(
                items.len(),
                row.security().clone(),
                row.state().clone(),
            ));
        }
        metas.push(meta);
    }
    Ok(SnapshotBatch::with_meta(
        items,
        aggregate_meta(context, &metas, Some(metadata.version()))?,
    ))
}

impl Control {
    fn start(self: &Arc<Self>) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.stopping {
            return Err(Error::new(
                ErrorKind::Closed,
                "subscription stopped during establishment",
            ));
        }
        let control = self.clone();
        state.worker = Some(
            thread::Builder::new()
                .name("eltdx-subscription".to_owned())
                .spawn(move || control.run())
                .map_err(|source| {
                    Error::new(ErrorKind::Internal, "unable to start subscription worker")
                        .with_source(source)
                })?,
        );
        Ok(())
    }

    fn own_stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.own_closed = true;
        state.stopping = true;
        state.queue.end();
        self.token.cancel();
        self.changed.notify_all();
    }

    fn context(&self, name: &'static str) -> Result<(HqExecutor, OperationContext)> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| Error::new(ErrorKind::Closed, "subscription owner was dropped"))?;
        let executor = HqExecutor::new(owner);
        // Preserve only RawPolicy from setup. Timeout/deadline and setup token
        // are never retained; each operation receives a fresh finite budget.
        let options = OperationOptions::default()
            .with_cancellation(self.token.clone())
            .with_raw(self.raw);
        let context = executor.begin_context(name, &self.config, &options)?;
        context.install_metadata(self.metadata.clone())?;
        Ok((executor, context))
    }

    fn refresh(
        &self,
        executor: &HqExecutor,
        context: &OperationContext,
        group: &Group,
        reset: bool,
    ) -> Result<(QuoteUpdates, u64)> {
        let requests = if reset {
            group.ids.iter().cloned().map(RefreshRequest::new).collect()
        } else {
            group.cursors.lock().map_err(|_| poisoned())?.requests()?
        };
        let raw = executor.execute_pinned_raw(
            &self.engine,
            &group.pin,
            context,
            refresh_command(&requests)?,
            ChildPurpose::Renewal,
            None,
        )?;
        let source = GenerationIdentity {
            engine_epoch: raw.engine_epoch,
            slot_id: raw.slot_id,
            generation: raw.generation,
        };
        if source != group.source {
            return Err(Error::new(
                ErrorKind::PinLost,
                "subscription renewal changed its immutable source",
            ));
        }
        let decoded = executor.decode_received(context, raw)?;
        let updates = project_refresh_updates(
            &group.ids,
            context,
            &self.config,
            &self.metadata,
            &decoded,
            "quotes.subscription",
        )?;
        Ok((updates, decoded.source.receive_sequence))
    }

    fn publish_updates(&self, group: &Group, updates: QuoteUpdates, sequence: u64) -> Result<()> {
        let (_, mut updates, _) = updates.into_parts();
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.stopping {
            return Ok(());
        }
        group
            .cursors
            .lock()
            .map_err(|_| poisoned())?
            .remember_ordered(group.source, sequence, &mut updates)?;
        for update in updates {
            if state.queue.publish(PushEvent::Update(update))? == QueueAction::Stop {
                state.stopping = true;
                self.token.cancel();
                break;
            }
        }
        self.changed.notify_all();
        Ok(())
    }

    fn resynchronize(&self) -> Result<()> {
        let (executor, context) = self.context("subscription_resynchronize")?;
        let _pause = self.dispatcher.pause(&context)?;
        let mut batches = Vec::new();
        batches.try_reserve(self.groups.len()).map_err(allocation)?;
        for group in &self.groups {
            // Restore source-local cursors first; the complete snapshot then
            // supersedes that activation's values without losing newer pushes.
            let (updates, sequence) = self.refresh(&executor, &context, group, true)?;
            let (_, mut updates, _) = updates.into_parts();
            {
                let mut cursors = group.cursors.lock().map_err(|_| poisoned())?;
                cursors.reset();
                cursors.remember_ordered(group.source, sequence, &mut updates)?;
            }
            let (batch, floors) = pinned_snapshot_batch(
                &executor,
                &self.engine,
                &group.pin,
                &context,
                &self.config,
                &self.metadata,
                &group.ids,
                group.source,
            )?;
            group
                .cursors
                .lock()
                .map_err(|_| poisoned())?
                .snapshot_floors(&floors)?;
            batches.push(batch);
        }
        let batch = combine_batches(&context, &self.metadata, batches)?;
        context.finish()?;
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.stopping && state.queue.restore_event(batch)? == QueueAction::Stop {
            state.stopping = true;
            self.token.cancel();
        }
        self.changed.notify_all();
        Ok(())
    }

    fn run(self: Arc<Self>) {
        let result = self.run_active();
        if let Err(error) = result {
            self.failed(error);
        }
        self.retire();
    }

    fn run_active(&self) -> Result<()> {
        let mut next_renewal = self
            .policy
            .renew_interval()
            .and_then(|interval| Instant::now().checked_add(interval));
        let mut confirmed_until = Instant::now()
            .checked_add(self.config.request_timeout())
            .ok_or_else(clock_overflow)?;
        loop {
            {
                let mut state = self.state.lock().map_err(|_| poisoned())?;
                while !state.armed && !state.stopping {
                    state = self
                        .changed
                        .wait_timeout(state, SIGNAL_POLL_INTERVAL)
                        .map_err(|_| poisoned())?
                        .0;
                }
                if state.stopping {
                    return Ok(());
                }
                if state.queue.recovery_requested() {
                    drop(state);
                    self.resynchronize()?;
                    continue;
                }
            }
            if next_renewal.is_some_and(|at| Instant::now() >= at) {
                let (executor, context) = self.context("subscription_renewal")?;
                let mut received = Vec::new();
                received
                    .try_reserve(self.groups.len())
                    .map_err(allocation)?;
                for group in &self.groups {
                    let (updates, sequence) = self.refresh(&executor, &context, group, false)?;
                    received.push((group, updates, sequence));
                }
                // Publish no group until the whole finite renewal succeeds.
                // Interleaved pushes remain ordered by their receive sequence.
                context.finish()?;
                for (group, updates, sequence) in received {
                    self.publish_updates(group, updates, sequence)?;
                }
                next_renewal = self
                    .policy
                    .renew_interval()
                    .and_then(|interval| Instant::now().checked_add(interval));
                confirmed_until = Instant::now()
                    .checked_add(self.config.request_timeout())
                    .ok_or_else(clock_overflow)?;
            }
            let mut confirmed = true;
            for group in &self.groups {
                if self.token.is_cancelled() {
                    return Ok(());
                }
                match group.pin.source_identity(SIGNAL_POLL_INTERVAL) {
                    Ok(Some(source)) if source == group.source => {}
                    Ok(Some(_)) => {
                        return Err(Error::new(
                            ErrorKind::PinLost,
                            "subscription source generation was replaced",
                        ))
                    }
                    Ok(None) | Err(eltdx_runtime::RuntimeError::PoolBusy { .. }) => {
                        confirmed = false
                    }
                    Err(error) => {
                        return Err(Error::new(
                            ErrorKind::PinLost,
                            "subscription source connection was lost",
                        )
                        .with_source(error))
                    }
                }
            }
            if confirmed {
                confirmed_until = Instant::now()
                    .checked_add(self.config.request_timeout())
                    .ok_or_else(clock_overflow)?;
            } else if Instant::now() >= confirmed_until {
                return Err(Error::new(
                    ErrorKind::Timeout,
                    "subscription source inspection exceeded its finite budget",
                ));
            }
            let state = self.state.lock().map_err(|_| poisoned())?;
            if state.stopping {
                return Ok(());
            }
            drop(
                self.changed
                    .wait_timeout(state, SIGNAL_POLL_INTERVAL)
                    .map_err(|_| poisoned())?,
            );
        }
    }

    fn retire(&self) {
        let deadline = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if state.retired || state.retiring {
                return;
            }
            state.retiring = true;
            let local = Instant::now()
                .checked_add(self.config.close_timeout())
                .unwrap_or_else(Instant::now);
            state.deadline = Some(state.deadline.map_or(local, |deadline| deadline.min(local)));
            state.routes.clear();
            state.deadline.expect("retirement deadline installed")
        };
        let mut failure = None;
        for group in &self.groups {
            let result = if let Some(owner) = self.owner.upgrade() {
                owner.retire_registered_pin(&group.pin, deadline)
            } else {
                group.pin.close_until(deadline).map_err(Error::from_runtime)
            };
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.cleanup_error = failure;
        state.retired = true;
        state.retiring = false;
        self.changed.notify_all();
    }
}

impl PushSink for Control {
    fn frame(&self, frame: &PushFrame) -> Result<()> {
        if frame.response.msg_type != eltdx_protocol::commands::quotes::TYPE_REFRESH_STREAM {
            return Ok(());
        }
        let source = GenerationIdentity {
            engine_epoch: frame.engine_epoch,
            slot_id: frame.slot_id,
            generation: frame.generation,
        };
        let Some(group) = self.groups.iter().find(|group| group.source == source) else {
            return Ok(());
        };
        if self.token.is_cancelled() {
            return Ok(());
        }
        let (_, context) = self.context("subscription_push_decode")?;
        let request = refresh_command(
            &group
                .ids
                .iter()
                .cloned()
                .map(RefreshRequest::new)
                .collect::<Vec<_>>(),
        )?;
        let metadata = context.begin_decode()?;
        let decoded = crate::hq::decode::decode_push(frame, request, self.raw, metadata.as_ref())
            .map_err(Error::from_protocol)?;
        let updates = project_refresh_updates(
            &group.ids,
            &context,
            &self.config,
            &metadata,
            &decoded,
            "quotes.subscription",
        )?;
        context.finish()?;
        self.publish_updates(group, updates, frame.receive_sequence)
    }

    fn gap(&self, dropped: u64, _cumulative: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.stopping {
            return;
        }
        if state.queue.loss(PushGap::new(
            GapReason::RuntimeOverflow,
            dropped,
            None,
            None,
        )) == QueueAction::Stop
        {
            state.stopping = true;
            self.token.cancel();
        }
        self.changed.notify_all();
    }

    fn failed(&self, error: Error) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.own_closed {
            return;
        }
        if !state.stopping && matches!(error.kind(), ErrorKind::PinLost | ErrorKind::Connection) {
            state
                .queue
                .loss(PushGap::new(GapReason::SourceLost, 0, None, None));
        }
        state.queue.fail(error);
        state.stopping = true;
        self.token.cancel();
        self.changed.notify_all();
    }
}

impl OwnerTaskGroup for Control {
    fn completed(&self) -> bool {
        self.state
            .lock()
            .map(|state| {
                state.retired
                    && state.cleanup_error.is_none()
                    && state.worker.as_ref().map_or(true, JoinHandle::is_finished)
            })
            .unwrap_or(false)
    }
    fn stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.stopping = true;
        if !state.own_closed {
            state
                .queue
                .fail(Error::new(ErrorKind::Closed, "parent session is closing"));
        }
        self.token.cancel();
        self.changed.notify_all();
    }
    fn stop_before(&self, deadline: Instant) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.deadline = Some(
            state
                .deadline
                .map_or(deadline, |previous| previous.min(deadline)),
        );
        state.stopping = true;
        if !state.own_closed {
            state
                .queue
                .fail(Error::new(ErrorKind::Closed, "parent session is closing"));
        }
        self.token.cancel();
        self.changed.notify_all();
    }
    fn close_until(&self, deadline: Instant) -> Result<()> {
        self.stop_before(deadline);
        let inline = self.state.lock().map_err(|_| poisoned())?.worker.is_none();
        if inline {
            self.retire();
        }
        let handle = {
            let mut state = self.state.lock().map_err(|_| poisoned())?;
            while !state.retired
                || state
                    .worker
                    .as_ref()
                    .is_some_and(|worker| !worker.is_finished())
            {
                if state.worker.as_ref().is_some_and(JoinHandle::is_finished) && !state.retired {
                    drop(state);
                    self.retire();
                    state = self.state.lock().map_err(|_| poisoned())?;
                    if state.retired {
                        break;
                    }
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(Error::new(
                        ErrorKind::CloseTimeout,
                        "subscription retirement was not confirmed",
                    ));
                }
                state = self
                    .changed
                    .wait_timeout(state, remaining.min(SIGNAL_POLL_INTERVAL))
                    .map_err(|_| poisoned())?
                    .0;
            }
            state.worker.take()
        };
        if let Some(handle) = handle {
            handle
                .join()
                .map_err(|_| Error::new(ErrorKind::Internal, "subscription worker panicked"))?;
        }
        self.state
            .lock()
            .map_err(|_| poisoned())?
            .cleanup_error
            .clone()
            .map_or(Ok(()), Err)
    }
}

fn allocation(source: std::collections::TryReserveError) -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "unable to reserve subscription state",
    )
    .with_source(source)
}
fn clock_overflow() -> Error {
    Error::new(
        ErrorKind::InvalidArgument,
        "subscription deadline or capacity overflows",
    )
}
fn poisoned() -> Error {
    Error::new(ErrorKind::Internal, "subscription state is poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn lazy_control(owner: &Arc<SessionOwner>, raw: RawPolicy) -> Arc<Control> {
        let config = crate::config::ConfigDraft::default().validate().unwrap();
        let engine = crate::client::engine_with_hosts(&config, vec!["127.0.0.1:9".into()]).unwrap();
        let dispatcher = Dispatcher::new(&engine).unwrap();
        let policy = SubscriptionOptions::default().with_renew_interval(None);
        let queue = EventQueue::new(&policy).unwrap();
        Arc::new(Control {
            owner: Arc::downgrade(owner),
            engine,
            config,
            metadata: Arc::new(MetadataSnapshot::empty("subscription-source-case")),
            raw,
            policy,
            dispatcher,
            groups: Vec::new(),
            token: CancellationToken::default(),
            state: Mutex::new(State {
                queue,
                routes: Vec::new(),
                armed: false,
                stopping: false,
                own_closed: false,
                deadline: None,
                worker: None,
                retiring: false,
                retired: false,
                cleanup_error: None,
            }),
            changed: Condvar::new(),
        })
    }

    #[test]
    fn renewal_contexts_have_fresh_budgets_and_only_inherit_raw_policy() {
        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        let control = lazy_control(&owner, RawPolicy::RetainWire);
        let setup_token = CancellationToken::default();
        let setup_deadline = Instant::now() + Duration::from_secs(1);
        let setup = OperationContext::rust_public(
            Instant::now(),
            "setup",
            owner.clone(),
            &control.config,
            &OperationOptions::default()
                .with_deadline(setup_deadline)
                .with_cancellation(setup_token.clone()),
        )
        .unwrap();
        setup.finish().unwrap();
        drop(setup);
        setup_token.cancel();
        let (_, first) = control.context("subscription_renewal").unwrap();
        assert_eq!(first.raw(), RawPolicy::RetainWire);
        assert!(first.sponsor().deadline().unwrap() > setup_deadline);
        assert_eq!(first.counters().unwrap().logical_requests, 0);
        let (_, second) = control.context("subscription_renewal").unwrap();
        assert_ne!(first.id, second.id);
        assert!(second.sponsor().deadline().unwrap() >= first.sponsor().deadline().unwrap());
        control.token.cancel();
        assert_eq!(
            first
                .check_boundary(TimeoutPhase::Queue)
                .unwrap_err()
                .kind(),
            ErrorKind::Cancelled
        );
        assert_eq!(
            second
                .check_boundary(TimeoutPhase::Queue)
                .unwrap_err()
                .kind(),
            ErrorKind::Cancelled
        );
        drop(first);
        drop(second);
        owner.close().unwrap();
    }

    #[test]
    fn a_registered_worker_and_control_do_not_keep_the_last_owner_alive() {
        let owner = Arc::new(SessionOwner::new(
            Duration::from_secs(1),
            Duration::from_secs(1),
        ));
        let weak = Arc::downgrade(&owner);
        let control = lazy_control(&owner, RawPolicy::Omit);
        owner.register_task_group(control.clone()).unwrap();
        control.start().unwrap();
        let subscription = Subscription {
            owner: owner.clone(),
            control: control.clone(),
        };
        drop(owner);
        drop(subscription);
        // This confirms the real worker exit after Drop signalled it, without
        // making another public read or retaining a strong owner for joining.
        control
            .close_until(Instant::now() + Duration::from_secs(1))
            .unwrap();
        assert!(weak.upgrade().is_none());
        assert!(control.state.lock().unwrap().retired);
    }
}
