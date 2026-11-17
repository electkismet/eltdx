//! Explicit, bounded discovery. A single registered runtime owns every child
//! socket; early exit destroys all remaining tasks before returning to caller.
use std::sync::Arc;
use std::time::Instant;
use eltdx_runtime::execution::AttemptAuthorization;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::config::ExConfig;
use super::control::{connection, Control};
use super::hosts::{DiscoveryOptions, Endpoint, HostProbe, ProbeMode, ProbeStatus};
use super::session::Session;
use super::transport::Work;

/// Protocol-specific readiness on the same bounded discovery scheduler. The
/// supplied profile is immutable for each probe socket, just as for owners.
pub(crate) trait DataProbe: std::fmt::Debug + Send + Sync {
    fn setup(&self) -> Arc<dyn super::profile::SetupProfile>;
    fn request(&self) -> Result<Work>;
    fn validate(&self, frame: &super::frame::Frame) -> Result<()>;
}
#[derive(Debug)]
struct ExDataProbe;
impl DataProbe for ExDataProbe {
    fn setup(&self) -> Arc<dyn super::profile::SetupProfile> { Arc::new(super::profile::ExProfile) }
    fn request(&self) -> Result<Work> { Ok(Work::Request(super::commands::markets_request())) }
    fn validate(&self, frame: &super::frame::Frame) -> Result<()> {
        super::commands::decode_markets(frame.payload().clone()).map(|_| ())
    }
}

pub(crate) type ProbeRequest = (Endpoint, Arc<dyn AttemptAuthorization>);
pub(crate) async fn run(
    config: ExConfig, requests: Vec<ProbeRequest>, options: DiscoveryOptions, control: Control,
) -> Result<Vec<HostProbe>> {
    run_with_profile(config, requests, options, control, Arc::new(ExDataProbe)).await
}
pub(crate) async fn run_with_profile(
    config: ExConfig, requests: Vec<ProbeRequest>, options: DiscoveryOptions, control: Control,
    profile: Arc<dyn DataProbe>,
) -> Result<Vec<HostProbe>> {
    let mut output = vec![None; requests.len()];
    let mut pending = requests.into_iter().enumerate();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..options.max_workers().get() {
        if let Some((index, request)) = pending.next() {
            spawn(&mut tasks, index, request, config.clone(), options, control.clone(), Arc::clone(&profile));
        }
    }
    while !tasks.is_empty() {
        let joined = control.wait(TimeoutPhase::Response, async {
            tasks.join_next().await.ok_or_else(|| Error::new(ErrorKind::Internal, "EX probe task disappeared"))?
                .map_err(|error| Error::new(ErrorKind::Internal, "EX probe task failed").with_source(error))
        }).await?;
        let (index, probe) = joined?;
        output[index] = Some(probe);
        control.check(TimeoutPhase::Response)?;
        if let Some((index, request)) = pending.next() {
            spawn(&mut tasks, index, request, config.clone(), options, control.clone(), Arc::clone(&profile));
        }
    }
    output.into_iter().map(|probe| probe.ok_or_else(|| Error::new(ErrorKind::Internal, "EX probe did not complete"))).collect()
}
fn spawn(
    tasks: &mut tokio::task::JoinSet<Result<(usize, HostProbe)>>, index: usize,
    (endpoint, authorization): ProbeRequest, config: ExConfig, options: DiscoveryOptions, parent: Control,
    profile: Arc<dyn DataProbe>,
) {
    tasks.spawn(async move {
        let entry = Instant::now();
        let local = entry.checked_add(options.probe().timeout())
            .ok_or_else(|| super::invalid("EX probe timeout overflows monotonic clock"))?;
        let control = Control { deadline: parent.deadline.min(local), ..parent.clone() };
        let result = match options.probe().mode() {
            ProbeMode::TcpOnly => tcp(endpoint, authorization, &control).await,
            ProbeMode::DataReady => data(endpoint, authorization, config, &control, profile).await,
        };
        // Parent cancellation/deadline and shared work limits are terminal,
        // never downgraded to one failed host in an otherwise successful list.
        parent.check(TimeoutPhase::Response)?;
        let probe = match result {
            Ok(()) => HostProbe::new(endpoint, Some(entry.elapsed()), ProbeStatus::Ready, None),
            Err(error) if matches!(error.kind(), ErrorKind::Connection | ErrorKind::Protocol | ErrorKind::Timeout) =>
                HostProbe::new(endpoint, None, ProbeStatus::Failed, Some(error.kind())),
            Err(error) => return Err(error),
        }.with_mode(options.probe().mode());
        Ok((index, probe))
    });
}
async fn tcp(endpoint: Endpoint, authorization: Arc<dyn AttemptAuthorization>, control: &Control) -> Result<()> {
    control.check(TimeoutPhase::Connect)?;
    let reservation = authorization.reserve(1, None).map_err(Error::from_runtime)?;
    reservation.start_before(control.deadline).map_err(Error::from_runtime)?;
    let stream = control.connect(async { tokio::net::TcpStream::connect(endpoint).await.map_err(connection) }).await?;
    drop(stream);
    Ok(())
}
async fn data(endpoint: Endpoint, authorization: Arc<dyn AttemptAuthorization>, config: ExConfig, control: &Control,
    profile: Arc<dyn DataProbe>) -> Result<()> {
    let config = config.with_endpoints(vec![endpoint])?.with_auto_failover(false);
    let session = Session::with_profile(vec![endpoint], profile.setup());
    let (reply, connection) = super::transport::execute(&session, &config,
        &profile.request()?, 1, &authorization, control).await?;
    let frame = reply.frame.ok_or_else(|| Error::new(ErrorKind::Internal, "extension data probe lacks its business response"))?;
    profile.validate(&frame)?;
    control.check(TimeoutPhase::Decode)?;
    drop(connection);
    Ok(())
}
