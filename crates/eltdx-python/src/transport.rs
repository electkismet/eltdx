use std::time::{Duration, Instant};

use eltdx::__python_compat::{LegacyBarTime, LegacyExecutor, LegacyPushReader};
use eltdx_protocol::CommandResponse;
use eltdx_runtime::diagnostics::{
    ActorSnapshot, BrokerSnapshot, PoolDiagnostics, TransportDiagnostics,
};
use eltdx_runtime::engine::{PendingPoll, PinHandle, CANCEL_CONFIRM_TIMEOUT, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::{Engine, EngineConfig, EngineConfigOptions};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::IntoPyObjectExt;

use crate::{error, request, response, service};

#[pyclass(name = "NativeEngine", module = "eltdx._native", frozen)]
pub struct NativeEngine {
    engine: Engine,
    legacy: LegacyExecutor,
    raw_push: LegacyPushReader,
    namespace: String,
}

#[pyclass(name = "NativePin", module = "eltdx._native", frozen)]
pub struct NativePin {
    pin: PinHandle,
    legacy: LegacyExecutor,
}

#[pymethods]
impl NativeEngine {
    #[new]
    #[pyo3(signature = (
        hosts,
        *,
        timeout=8.0,
        pool_size=1,
        runtime_workers=None,
        server_count=1,
        max_connections_per_host=None,
        connect_concurrency=None,
        connect_concurrency_per_host=None,
        global_raw_bytes=None,
        global_decoded_bytes=None,
        heartbeat_interval=Some(30.0),
        max_pending_requests=256,
        push_queue_size=1024,
        push_queue_bytes=8_388_608
    ))]
    // PyO3 must expose these as independent keyword arguments; the runtime
    // immediately groups them into EngineConfigOptions below.
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        hosts: Vec<String>,
        timeout: f64,
        pool_size: usize,
        runtime_workers: Option<usize>,
        server_count: usize,
        max_connections_per_host: Option<usize>,
        connect_concurrency: Option<usize>,
        connect_concurrency_per_host: Option<usize>,
        global_raw_bytes: Option<usize>,
        global_decoded_bytes: Option<usize>,
        heartbeat_interval: Option<f64>,
        max_pending_requests: usize,
        push_queue_size: usize,
        push_queue_bytes: usize,
    ) -> PyResult<Self> {
        let namespace = format!("python:7709:{}", hosts.join(","));
        let config = EngineConfig::new(
            hosts,
            timeout,
            EngineConfigOptions {
                pool_size,
                runtime_workers,
                server_count,
                max_connections_per_host,
                connect_concurrency,
                connect_concurrency_per_host,
                global_raw_bytes,
                global_decoded_bytes,
                heartbeat_interval,
                max_pending_requests,
                push_queue_size,
                push_queue_bytes,
            },
        )
        .map_err(error::from_runtime)?;
        let engine = Engine::new(config).map_err(error::from_runtime)?;
        let request_timeout = duration("timeout", timeout)?;
        let legacy = LegacyExecutor::new(
            engine.clone(),
            request_timeout,
            request_timeout,
            CANCEL_CONFIRM_TIMEOUT,
        )
        .map_err(|sdk_error| error::from_sdk(py, sdk_error))?;
        let raw_push = legacy.claim_raw_push_reader().map_err(error::from_runtime)?;
        Ok(Self {
            engine,
            legacy,
            raw_push,
            namespace,
        })
    }

    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let mut pending = py
            .detach(|| self.engine.begin_connect())
            .map_err(error::from_runtime)?;
        loop {
            let polled = py.detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL));
            if let Err(signal_error) = py.check_signals() {
                let cleanup = py.detach(move || pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT));
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
                return Err(signal_error);
            }
            match polled.map_err(error::from_runtime)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let mut pending = py
            .detach(|| self.engine.begin_close())
            .map_err(error::from_runtime)?;
        loop {
            let polled = py.detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL));
            if let Err(signal_error) = py.check_signals() {
                let cleanup = py.detach(move || pending.wait());
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
                return Err(signal_error);
            }
            match polled.map_err(error::from_runtime)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }

    #[pyo3(signature = (command, payload=None))]
    fn execute(
        &self,
        py: Python<'_>,
        command: u16,
        payload: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, payload)?;
        let initial = py.detach(|| {
            self.engine.begin_execute(request).map(|mut pending| {
                let polled = pending.wait_timeout(SIGNAL_POLL_INTERVAL);
                (pending, polled)
            })
        });
        if let Err(signal_error) = py.check_signals() {
            if let Ok((pending, _)) = initial {
                let cleanup = py.detach(move || pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT));
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
            }
            return Err(signal_error);
        }
        let (mut pending, initial_poll) = initial.map_err(error::from_runtime)?;
        let result = match initial_poll.map_err(error::from_runtime)? {
            PendingPoll::Ready(response) => response,
            PendingPoll::Pending => loop {
                let polled = py.detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL));
                if let Err(signal_error) = py.check_signals() {
                    let cleanup =
                        py.detach(move || pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT));
                    if let Err(cleanup_error) = cleanup {
                        signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                    }
                    return Err(signal_error);
                }
                match polled.map_err(error::from_runtime)? {
                    PendingPoll::Ready(response) => break response,
                    PendingPoll::Pending => {}
                }
            },
        };
        response::to_python(py, result)
    }

    fn sdk_snapshots(&self, py: Python<'_>, codes: Vec<String>) -> PyResult<Py<PyAny>> {
        service::snapshots(py, self.legacy.clone(), self.namespace.clone(), codes)
    }

    fn sdk_security_pages(
        &self,
        py: Python<'_>,
        market: String,
        page_size: usize,
    ) -> PyResult<Vec<Py<PyAny>>> {
        service::security_pages(
            py,
            self.legacy.clone(),
            self.namespace.clone(),
            market,
            page_size,
        )
    }

    #[pyo3(signature = (command, payload, *, collect=false, max_pages=None, bar_time="start"))]
    fn sdk_bars(
        &self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
        collect: bool, max_pages: Option<usize>, bar_time: &str,
    ) -> PyResult<Py<PyAny>> {
        // Reuse the existing extractor to retain input aliases, bool handling,
        // overflow messages and the order in which request fields are checked.
        let request = request::from_python(py, command, Some(payload))?;
        let alignment = match bar_time {
            "start" => LegacyBarTime::Start,
            "end" => LegacyBarTime::End,
            other => {
                let representation = pyo3::types::PyString::new(py, other).repr()?.to_str()?.to_owned();
                LegacyBarTime::DeferredInvalid(format!("bar_time 必须是 'start' 或 'end'，得到: {representation}"))
            }
        };
        service::bars(py, self.legacy.clone(), request, collect, max_pages, alignment)
    }

    fn sdk_minutes(
        &self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        let original_code = payload.get_item("code")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("minute code is required"))?
            .extract::<String>()?;
        service::minutes(py, self.legacy.clone(), self.namespace.clone(), request, original_code)
    }

    fn sdk_security_request(
        &self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::directory(py, self.legacy.clone(), request)
    }

    fn sdk_quotes(&self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::quotes(py, self.legacy.clone(), request, None)
    }

    fn sdk_trades(&self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::trades(py, self.legacy.clone(), request)
    }

    fn sdk_auction(&self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::auction(py, self.legacy.clone(), request)
    }

    #[pyo3(signature = (command, payload, *, collect=false, opening=false, max_pages=None, bulk=false))]
    fn sdk_trade_read(&self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
        collect: bool, opening: bool, max_pages: Option<usize>, bulk: bool) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::trade_read(py, self.legacy.clone(), request, collect, opening, max_pages, bulk)
    }

    fn sdk_suspended(&self, py: Python<'_>, payload: &Bound<'_, PyDict>) -> PyResult<Vec<String>> {
        let request = request::from_python(py, 0x053e, Some(payload))?;
        service::suspended(py, self.legacy.clone(), request)
    }

    fn sdk_session(
        &self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::session(py, self.legacy.clone(), request)
    }

    fn sdk_company_info(
        &self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        service::company_info(py, self.legacy.clone(), request)
    }

    fn pin(&self, py: Python<'_>) -> PyResult<NativePin> {
        let mut pending = py
            .detach(|| self.engine.begin_pin())
            .map_err(error::from_runtime)?;
        loop {
            let polled = py
                .detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL))
                .map_err(error::from_runtime)?;
            if let Err(signal_error) = py.check_signals() {
                let cleanup = match polled {
                    PendingPoll::Ready(pin) => py.detach(move || pin.close()),
                    PendingPoll::Pending => {
                        py.detach(move || pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT))
                    }
                };
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
                return Err(signal_error);
            }
            match polled {
                PendingPoll::Ready(pin) => return Ok(NativePin { pin, legacy: self.legacy.clone() }),
                PendingPoll::Pending => {}
            }
        }
    }

    #[pyo3(signature = (timeout=0.0, parse=false))]
    fn poll_push(&self, py: Python<'_>, timeout: f64, parse: bool) -> PyResult<Option<Py<PyAny>>> {
        let timeout = duration("timeout", timeout)?;
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            pyo3::exceptions::PyOverflowError::new_err("timeout deadline is too large")
        })?;
        loop {
            let now = Instant::now();
            let wait = deadline
                .saturating_duration_since(now)
                .min(SIGNAL_POLL_INTERVAL);
            let result = py
                .detach(|| self.raw_push.poll_push(wait))
                .map_err(error::from_runtime)?;
            py.check_signals()?;
            if let Some(item) = result {
                return response::push_to_python(py, item, parse).map(Some);
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }

    #[pyo3(signature = (parse=false))]
    fn drain_pushes(&self, py: Python<'_>, parse: bool) -> PyResult<Vec<Py<PyAny>>> {
        let items = py
            .detach(|| self.raw_push.drain_pushes())
            .map_err(error::from_runtime)?;
        items
            .into_iter()
            .map(|item| response::push_to_python(py, item, parse))
            .collect()
    }

    fn pool_diagnostics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let diagnostics = self
            .engine
            .pool_diagnostics()
            .map_err(error::from_runtime)?;
        diagnostics_tuple(py, diagnostics)
    }

    fn transport_diagnostics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let diagnostics = self
            .engine
            .transport_diagnostics()
            .map_err(error::from_runtime)?;
        transport_tuple(py, diagnostics)
    }

    #[pyo3(signature = (slot_index=0))]
    fn session_snapshot(&self, py: Python<'_>, slot_index: usize) -> PyResult<Py<PyAny>> {
        let (handshake, heartbeat) = self
            .engine
            .session_snapshot(slot_index)
            .map_err(error::from_runtime)?;
        session_tuple(py, handshake, heartbeat)
    }
}

#[pymethods]
impl NativePin {
    fn sdk_quotes(&self, py: Python<'_>, command: u16, payload: &Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, Some(payload))?;
        // The Python subscription resolver runs before pin acquisition. Never
        // submit cold metadata through the ordinary pool while it is reserved.
        service::quotes(py, self.legacy.clone(), request, Some(self.pin.clone()))
    }

    #[pyo3(signature = (command, payload=None))]
    fn execute(
        &self,
        py: Python<'_>,
        command: u16,
        payload: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let request = request::from_python(py, command, payload)?;
        let mut pending = py
            .detach(|| self.pin.begin_execute(request))
            .map_err(error::from_runtime)?;
        loop {
            let polled = py.detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL));
            if let Err(signal_error) = py.check_signals() {
                let cleanup = py.detach(move || pending.cancel_and_confirm(CANCEL_CONFIRM_TIMEOUT));
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
                return Err(signal_error);
            }
            match polled.map_err(error::from_runtime)? {
                PendingPoll::Ready(result) => return response::to_python(py, result),
                PendingPoll::Pending => {}
            }
        }
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let mut pending = py
            .detach(|| self.pin.begin_close())
            .map_err(error::from_runtime)?;
        loop {
            let polled = py.detach(|| pending.wait_timeout(SIGNAL_POLL_INTERVAL));
            if let Err(signal_error) = py.check_signals() {
                let cleanup = py.detach(move || pending.wait());
                if let Err(cleanup_error) = cleanup {
                    signal_error.set_cause(py, Some(error::from_runtime(cleanup_error)));
                }
                return Err(signal_error);
            }
            match polled.map_err(error::from_runtime)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
        }
    }

    #[getter]
    fn connected_host(&self) -> PyResult<Option<String>> {
        self.pin.connected_host().map_err(error::from_runtime)
    }

    fn session_snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let (handshake, heartbeat) = self.pin.session_snapshot().map_err(error::from_runtime)?;
        session_tuple(py, handshake, heartbeat)
    }
}

fn session_tuple(
    py: Python<'_>,
    handshake: Option<eltdx_protocol::commands::session::HandshakeInfo>,
    heartbeat: Option<eltdx_protocol::commands::session::HeartbeatAck>,
) -> PyResult<Py<PyAny>> {
    let handshake = match handshake {
        Some(value) => response::to_python(py, CommandResponse::Handshake(value))?,
        None => py.None(),
    };
    let heartbeat = match heartbeat {
        Some(value) => response::to_python(py, CommandResponse::Heartbeat(value))?,
        None => py.None(),
    };
    Ok(PyTuple::new(py, vec![handshake, heartbeat])?
        .into_any()
        .unbind())
}

fn actor_tuple(py: Python<'_>, actor: &ActorSnapshot) -> PyResult<Py<PyAny>> {
    Ok(PyTuple::new(
        py,
        vec![
            object(py, actor.runtime_epoch)?,
            object(py, actor.state.as_str())?,
            object(py, actor.tcp_state.as_str())?,
            object(py, actor.tcp_generation)?,
            object(py, actor.connected_host.as_deref())?,
            object(py, actor.actor_alive)?,
            object(py, actor.pending_depth)?,
            object(py, actor.reconnect_count)?,
            object(py, actor.stale_event_count)?,
            object(py, actor.last_error.as_deref())?,
        ],
    )?
    .into_any()
    .unbind())
}

fn broker_tuple(py: Python<'_>, broker: Option<BrokerSnapshot>) -> PyResult<Py<PyAny>> {
    let Some(broker) = broker else {
        return Ok(py.None());
    };
    Ok(PyTuple::new(
        py,
        vec![
            object(py, broker.pool_epoch)?,
            object(py, broker.idle_slots)?,
            object(py, broker.waiter_count)?,
            object(py, broker.pin_waiter_count)?,
            object(py, broker.active_leases)?,
            object(py, broker.closed)?,
        ],
    )?
    .into_any()
    .unbind())
}

fn diagnostics_tuple(py: Python<'_>, diagnostics: PoolDiagnostics) -> PyResult<Py<PyAny>> {
    let actors = diagnostics
        .actors
        .iter()
        .map(|actor| actor_tuple(py, actor))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(PyTuple::new(
        py,
        vec![
            object(py, diagnostics.epoch)?,
            object(py, diagnostics.state.as_str())?,
            broker_tuple(py, diagnostics.broker)?,
            PyList::new(py, actors)?.into_any().unbind(),
            object(py, diagnostics.push_frames)?,
            object(py, diagnostics.push_bytes)?,
            object(py, diagnostics.push_dropped)?,
            object(py, diagnostics.runtime_workers)?,
            object(py, diagnostics.pool_size)?,
            object(py, diagnostics.server_count)?,
            object(py, diagnostics.max_connections_per_host)?,
            object(py, diagnostics.connect_concurrency)?,
            object(py, diagnostics.connect_concurrency_per_host)?,
            object(py, diagnostics.raw_bytes)?,
            object(py, diagnostics.raw_max_bytes)?,
            object(py, diagnostics.raw_peak_bytes)?,
            object(py, diagnostics.decoded_bytes)?,
            object(py, diagnostics.decoded_max_bytes)?,
            object(py, diagnostics.decoded_peak_bytes)?,
            object(py, diagnostics.push_max_bytes)?,
        ],
    )?
    .into_any()
    .unbind())
}

fn transport_tuple(py: Python<'_>, diagnostics: TransportDiagnostics) -> PyResult<Py<PyAny>> {
    let actor = match diagnostics.actor.as_ref() {
        Some(actor) => actor_tuple(py, actor)?,
        None => py.None(),
    };
    Ok(PyTuple::new(
        py,
        vec![
            object(py, diagnostics.epoch)?,
            actor,
            object(py, diagnostics.push_frames)?,
            object(py, diagnostics.push_bytes)?,
            object(py, diagnostics.push_dropped)?,
            object(py, diagnostics.push_max_frames)?,
            object(py, diagnostics.push_max_bytes)?,
            object(py, diagnostics.runtime_workers)?,
            object(py, diagnostics.raw_bytes)?,
            object(py, diagnostics.raw_max_bytes)?,
            object(py, diagnostics.raw_peak_bytes)?,
            object(py, diagnostics.decoded_bytes)?,
            object(py, diagnostics.decoded_max_bytes)?,
            object(py, diagnostics.decoded_peak_bytes)?,
        ],
    )?
    .into_any()
    .unbind())
}

fn object<'py, T>(py: Python<'py>, value: T) -> PyResult<Py<PyAny>>
where
    T: IntoPyObject<'py>,
{
    Ok(value.into_bound_py_any(py)?.unbind())
}

fn duration(name: &str, seconds: f64) -> PyResult<Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{name} must be a finite number >= 0"
        )));
    }
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| pyo3::exceptions::PyOverflowError::new_err(format!("{name} is too large")))
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeEngine>()?;
    module.add_class::<NativePin>()
}
