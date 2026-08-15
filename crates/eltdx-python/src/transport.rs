use std::time::{Duration, Instant};

use eltdx_protocol::CommandResponse;
use eltdx_runtime::diagnostics::{
    ActorSnapshot, BrokerSnapshot, PoolDiagnostics, TransportDiagnostics,
};
use eltdx_runtime::engine::{PendingPoll, PinHandle, CANCEL_CONFIRM_TIMEOUT, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::{Engine, EngineConfig};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::IntoPyObjectExt;

use crate::{error, request, response};

#[pyclass(name = "NativeEngine", module = "eltdx._native", frozen)]
pub struct NativeEngine {
    engine: Engine,
}

#[pyclass(name = "NativePin", module = "eltdx._native", frozen)]
pub struct NativePin {
    pin: PinHandle,
}

#[pymethods]
impl NativeEngine {
    #[new]
    #[pyo3(signature = (
        hosts,
        *,
        timeout=8.0,
        pool_size=1,
        heartbeat_interval=Some(30.0),
        max_pending_requests=256,
        push_queue_size=1024,
        push_queue_bytes=8_388_608
    ))]
    fn new(
        hosts: Vec<String>,
        timeout: f64,
        pool_size: usize,
        heartbeat_interval: Option<f64>,
        max_pending_requests: usize,
        push_queue_size: usize,
        push_queue_bytes: usize,
    ) -> PyResult<Self> {
        let config = EngineConfig::new(
            hosts,
            timeout,
            pool_size,
            heartbeat_interval,
            max_pending_requests,
            push_queue_size,
            push_queue_bytes,
        )
        .map_err(error::from_runtime)?;
        let engine = Engine::new(config).map_err(error::from_runtime)?;
        Ok(Self { engine })
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
                PendingPoll::Ready(pin) => return Ok(NativePin { pin }),
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
                .detach(|| self.engine.poll_push(wait))
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
            .detach(|| self.engine.drain_pushes())
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
