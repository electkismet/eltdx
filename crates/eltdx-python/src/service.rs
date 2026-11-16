use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use eltdx::__python_compat::{LegacyBarTime, LegacyCodeSelection, LegacyExecutor, LegacyOperation};
use eltdx::types::HqMarket;
use eltdx::{CancellationToken, Error, ErrorKind, RawPolicy};
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_protocol::unit::{Market, NormalizedCode};
use eltdx_runtime::engine::SIGNAL_POLL_INTERVAL;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::{error, response};

pub fn snapshots(
    py: Python<'_>,
    executor: LegacyExecutor,
    namespace: String,
    codes: Vec<String>,
) -> PyResult<Py<PyAny>> {
    let markets = precision_markets(&codes);
    let batch = run_legacy(py, move |token| {
        let operation = executor.begin_operation(
            "python_quotes_snapshots",
            RawPolicy::Omit,
            Some(token),
        )?;
        operation.prepare_hq_metadata(&namespace, &markets)?;
        let normalized = codes
            .iter()
            .map(|code| {
                NormalizedCode::parse(code)
                    .map_err(|source| Error::new(ErrorKind::InvalidArgument, format!("{source}")))
            })
            .collect::<eltdx::Result<Vec<_>>>()?;
        let batch = operation.recover_snapshots(&normalized)?;
        operation.finish()?;
        Ok(batch)
    })?;
    response::legacy_snapshot_batch_to_python(py, &batch)
}

pub fn security_pages(
    py: Python<'_>,
    executor: LegacyExecutor,
    namespace: String,
    market: String,
    page_size: usize,
) -> PyResult<Vec<Py<PyAny>>> {
    let market = hq_market(Market::normalize(&market).map_err(|source| {
        Error::new(ErrorKind::InvalidArgument, format!("{source}"))
    })?)
    .map_err(|sdk_error| error::from_sdk(py, sdk_error))?;
    let pages = run_legacy(py, move |token| {
        let operation = executor.begin_operation(
            "python_codes_all",
            RawPolicy::Omit,
            Some(token),
        )?;
        let pages = operation.security_pages(&namespace, market, page_size)?;
        operation.finish()?;
        Ok(pages)
    })?;
    pages
        .into_iter()
        .map(|page| response::to_python(py, CommandResponse::SecurityList(page.rows)))
        .collect()
}

pub fn bars(
    py: Python<'_>, executor: LegacyExecutor, request: CommandRequest,
    collect: bool, maximum_pages: Option<usize>, bar_time: LegacyBarTime,
) -> PyResult<Py<PyAny>> {
    let raw = match &request {
        CommandRequest::Klines(request) => request.include_raw,
        CommandRequest::DelistedKlines(request) => request.include_raw,
        _ => return Err(pyo3::exceptions::PyValueError::new_err("native bars require an OHLC command")),
    };
    let series = run_legacy(py, move |token| {
        let operation = executor.begin_operation("python_bars",
            if raw { RawPolicy::RetainPayload } else { RawPolicy::Omit }, Some(token))?;
        let series = operation.bars(request, collect, maximum_pages, bar_time)?;
        operation.finish()?;
        Ok(series)
    })?;
    response::legacy_bar_series_to_python(py, &series)
}

pub fn minutes(
    py: Python<'_>, executor: LegacyExecutor, namespace: String, request: CommandRequest,
    original_code: String,
) -> PyResult<Py<PyAny>> {
    let (include_raw, priced) = match &request {
        CommandRequest::TodayIntraday(request) => (request.include_raw, true),
        CommandRequest::HistoricalIntraday(request) => (request.include_raw, true),
        CommandRequest::RecentIntraday(request) => (request.include_raw, true),
        CommandRequest::IntradayAux(request) => (request.include_raw, false),
        CommandRequest::Sparkline(request) => (request.include_raw, false),
        _ => return Err(pyo3::exceptions::PyValueError::new_err("native minutes require a minute-domain command")),
    };
    // The original resolver inferred markets before normalization. For example,
    // bare019xxx scans sz even though the request extractor normalizes it to sh.
    let markets = precision_markets(&[original_code]);
    let result = run_legacy(py, move |token| {
        let operation = executor.begin_operation("python_minutes",
            if include_raw { RawPolicy::RetainPayload } else { RawPolicy::Omit }, Some(token))?;
        if priced {
            operation.prepare_hq_metadata(&namespace, &markets)?;
        }
        let result = operation.minutes(request)?;
        operation.finish()?;
        Ok(result)
    })?;
    response::to_python(py, result)
}

pub fn directory(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest) -> PyResult<Py<PyAny>> {
    unpriced_domain(py, executor, request, "python_directory", LegacyOperation::directory)
}

pub fn trades(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest) -> PyResult<Py<PyAny>> {
    unpriced_domain(py, executor, request, "python_trades", LegacyOperation::trades)
}

pub fn trade_read(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest,
    collect: bool, opening: bool, maximum_pages: Option<usize>, bulk: bool) -> PyResult<Py<PyAny>> {
    let include_raw = match &request {
        CommandRequest::TodayTicks(query) => query.include_raw,
        CommandRequest::HistoricalTicks(query) => query.include_raw,
        _ => return Err(pyo3::exceptions::PyValueError::new_err("native trade read requires ticks")),
    };
    let value = run_legacy(py, move |token| {
        let operation = executor.begin_operation("python_trade_read",
            if include_raw { RawPolicy::RetainPayload } else { RawPolicy::Omit }, Some(token))?;
        let result = operation.trade_read(request, collect, opening, maximum_pages)?;
        operation.finish()?;
        Ok(result)
    })?;
    response::legacy_trade_read_to_python(py, &value, bulk)
}

pub fn quotes(
    py: Python<'_>, executor: LegacyExecutor, request: CommandRequest,
    pin: Option<eltdx_runtime::engine::PinHandle>,
) -> PyResult<Py<PyAny>> {
    let result = run_legacy(py, move |token| {
        let operation = executor.begin_operation("python_quotes", RawPolicy::RetainPayload, Some(token))?;
        // Python's original resolver already chose and loaded its markets. A
        // missing resolver/pinned refresh must keep the old no-prefetch path.
        let result = operation.quotes(request, pin.as_ref())?;
        operation.finish()?;
        Ok(result)
    })?;
    response::to_python(py, result)
}

pub fn suspended(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest) -> PyResult<Vec<String>> {
    run_legacy(py, move |token| {
        let operation = executor.begin_operation("python_suspended", RawPolicy::RetainPayload, Some(token))?;
        let result = operation.suspended(request)?;
        operation.finish()?;
        Ok(result)
    })
}

#[pyfunction]
pub fn select_security_rows(py: Python<'_>, rows: Vec<(String, String)>, selection: &str) -> PyResult<Vec<usize>> {
    let selection = LegacyCodeSelection::from_name(selection).map_err(|source| error::from_sdk(py, source))?;
    let mut selected = Vec::new();
    for (index, (category, name)) in rows.iter().enumerate() {
        if index % 256 == 0 { py.check_signals()?; }
        if selection.matches_legacy(category, name) { selected.push(index); }
    }
    Ok(selected)
}

pub fn session(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest) -> PyResult<Py<PyAny>> {
    unpriced_domain(py, executor, request, "python_session", LegacyOperation::session)
}

pub fn company_info(py: Python<'_>, executor: LegacyExecutor, request: CommandRequest) -> PyResult<Py<PyAny>> {
    unpriced_domain(py, executor, request, "python_company_info", LegacyOperation::company_info)
}

fn unpriced_domain(
    py: Python<'_>, executor: LegacyExecutor, request: CommandRequest, name: &'static str,
    domain: fn(&LegacyOperation, CommandRequest) -> eltdx::Result<CommandResponse>,
) -> PyResult<Py<PyAny>> {
    let result = run_legacy(py, move |token| {
        // Session's original public models retain raw buffers unconditionally.
        let operation = executor.begin_operation(name, RawPolicy::RetainPayload, Some(token))?;
        let result = domain(&operation, request)?;
        operation.finish()?;
        Ok(result)
    })?;
    response::to_python(py, result)
}

fn hq_market(market: Market) -> eltdx::Result<HqMarket> {
    Ok(match market {
        Market::Shenzhen => HqMarket::Shenzhen,
        Market::Shanghai => HqMarket::Shanghai,
        Market::Beijing => HqMarket::Beijing,
    })
}

fn precision_markets(codes: &[String]) -> Vec<HqMarket> {
    let mut markets = Vec::new();
    for value in codes {
        let code = value.trim().to_ascii_lowercase();
        let market = if code.starts_with("sh") {
            Some(HqMarket::Shanghai)
        } else if code.starts_with("sz") {
            Some(HqMarket::Shenzhen)
        } else if code.starts_with("bj") {
            Some(HqMarket::Beijing)
        } else if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
            let first = code.as_bytes().first().copied();
            if code.starts_with("92") || first == Some(b'8') {
                Some(HqMarket::Beijing)
            } else if matches!(first, Some(b'6' | b'9')) || ["110", "113", "118"].iter().any(|p| code.starts_with(p)) {
                Some(HqMarket::Shanghai)
            } else if matches!(first, Some(b'0' | b'1' | b'2' | b'3')) {
                Some(HqMarket::Shenzhen)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(market) = market {
            if !markets.contains(&market) {
                markets.push(market);
            }
        }
    }
    if markets.is_empty() {
        markets.extend([HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing]);
    }
    markets.sort_by_key(|market| market.code());
    markets
}

fn run_legacy<T>(
    py: Python<'_>,
    task: impl FnOnce(CancellationToken) -> eltdx::Result<T> + Send + 'static,
) -> PyResult<T>
where
    T: Send + 'static,
{
    let token = CancellationToken::default();
    let worker_token = token.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("eltdx-python-service".to_owned())
        .spawn(move || {
            let _ = sender.send(task(worker_token));
        })
        .map_err(|error| PyRuntimeError::new_err(format!("{error}")))?;
    let receiver = Arc::new(Mutex::new(receiver));

    loop {
        let pending = Arc::clone(&receiver);
        let received = py.detach(move || {
            pending
                .lock()
                .ok()
                .map(|receiver| receiver.recv_timeout(SIGNAL_POLL_INTERVAL))
        });
        match received {
            Some(Ok(value)) => return value.map_err(|error| error::from_sdk(py, error)),
            Some(Err(mpsc::RecvTimeoutError::Timeout)) => {}
            Some(Err(mpsc::RecvTimeoutError::Disconnected)) | None => {
                return Err(PyRuntimeError::new_err(
                    "native SDK service worker ended without a result",
                ));
            }
        }

        if let Err(signal_error) = py.check_signals() {
            token.cancel();
            let pending = Arc::clone(&receiver);
            let cleanup = py.detach(move || {
                pending
                    .lock()
                    .ok()
                    .map(|receiver| receiver.recv())
            });
            match cleanup {
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    signal_error.set_cause(py, Some(error::from_sdk(py, error)));
                }
                None => {
                    signal_error.set_cause(
                        py,
                        Some(PyRuntimeError::new_err(
                            "native SDK service cancellation could not be confirmed",
                        )),
                    );
                }
            }
            return Err(signal_error);
        }
    }
}
