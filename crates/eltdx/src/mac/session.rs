//! MAC owners reuse the domain-neutral operation clock and EX's serialized,
//! registered worker/frame transport. No second socket loop or executor.
use std::sync::{Arc, Mutex};
use std::time::Instant;
use bytes::Bytes;
use crate::config::{ClientConfig, ConfigDraft};
use crate::operation::{ChildPurpose, OperationContext, WorkLimits};
use crate::session::SessionOwner;
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta, SourceStamp};
use crate::{Error, ErrorKind, OperationOptions, RawPolicy, Result, TimeoutPhase};
use crate::ex::{config::ExConfig, models::Decoded};
use crate::ex::hosts::{DiagnosticSnapshot, DiscoveryOptions, Endpoint, HostProbe, ProbeMode};
use crate::ex::profile::SetupProfile;
use crate::ex::session::{Lease, Session};
use crate::ex::transport::Work;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Domain { Mac, MacEx }
impl Domain {
    pub const fn name(self) -> &'static str { match self { Self::Mac => "mac", Self::MacEx => "mac-ex" } }
}
#[derive(Debug)]
pub(crate) struct MacOwner {
    pub config: ExConfig, pub session: Arc<Session>, common: ClientConfig,
    pub catalogs: Mutex<super::catalog::Catalogs>,
    owner: Arc<SessionOwner>, domain: Domain,
}
pub(crate) struct Reply<T> {
    pub value: T, pub meta: ResultMeta, pub wire_count: Option<u16>, pub response_offset: Option<u32>,
}
impl MacOwner {
    pub fn new(config: ExConfig, profile: Arc<dyn SetupProfile>, domain: Domain) -> Result<Arc<Self>> {
        let common = ConfigDraft {
            request_timeout: config.timeout(), close_timeout: config.close_timeout(), cleanup_timeout: config.cleanup_timeout(),
            heartbeat_interval: None, wire_attempt_limit: config.wire_attempt_limit(), extra_attempt_budget: config.extra_attempt_budget(),
            ..ConfigDraft::default()
        }.validate()?;
        let owner = Arc::new(SessionOwner::new(config.close_timeout(),config.cleanup_timeout()));
        let session = Session::with_profile(config.endpoints().to_vec(),profile);
        Ok(Arc::new(Self { config, session, common, owner, domain, catalogs: Mutex::new(super::catalog::Catalogs::default()) }))
    }
    pub fn context(&self, entry: Instant, name: &'static str, options: &OperationOptions) -> Result<OperationContext> {
        let options = options.inheriting_raw(self.config.raw());
        let mut context = OperationContext::rust_public(entry,name,Arc::clone(&self.owner),&self.common,&options)?;
        context.set_work_limits(WorkLimits { bytes: Some(self.config.max_operation_bytes().get()), ..WorkLimits::default() })?;
        Ok(context)
    }
    pub fn close(&self) -> Result<()> { self.owner.close() }
    pub fn diagnostics(&self) -> Result<DiagnosticSnapshot> { self.session.snapshot() }
    pub fn healthy_hosts(&self) -> Result<Vec<Endpoint>> {
        let snapshot = self.diagnostics()?; let excluded = self.session.unhealthy()?;
        Ok(snapshot.probes().iter().filter(|probe| probe.data_ready()
            && snapshot.candidates().contains(&probe.endpoint()) && !excluded.contains(&probe.endpoint()))
            .map(HostProbe::endpoint).collect())
    }
    pub fn connect(&self, context: &OperationContext) -> Result<()> {
        let lease = self.session.lease(context)?;
        if self.session.snapshot()?.active_endpoint().is_some() {
            context.check_boundary(TimeoutPhase::Connect)?; lease.accept(); return Ok(());
        }
        let authorization = context.begin_child(ChildPurpose::Required,None)?.into_transport_authorization();
        crate::ex::worker::execute(Arc::clone(&self.session),self.config.clone(),Work::Connect,
            self.config.wire_attempt_limit().get(),authorization,context,lease.worker_slot(), |_| Ok(()))
            .map_err(|error| context.annotate(error,TimeoutPhase::Connect,None).with_context("domain",self.domain.name()))?;
        context.check_boundary(TimeoutPhase::Connect)?; lease.accept(); Ok(())
    }
    pub fn mac_work(&self, request: super::commands::MacRequest) -> Result<Work> {
        let wire = if self.domain == Domain::MacEx { super::commands::rewrite_mac_ex_head(&request.wire)? } else { request.wire };
        Ok(Work::Encoded(Bytes::from(wire)))
    }
    pub fn pinned_mac_work(&self, request: super::commands::MacRequest, source: (Endpoint,u64)) -> Result<Work> {
        match self.mac_work(request)? {
            Work::Encoded(wire) => Ok(Work::PinnedEncoded { wire,source }),
            _ => Err(Error::new(ErrorKind::Internal,"MAC source pin requires an encoded command")),
        }
    }
    pub fn exchange<T: Send + 'static>(&self, context: &OperationContext, work: Work, command: u16,
        decode: impl FnOnce(Bytes,RawPolicy) -> Result<Decoded<T>> + Send + 'static) -> Result<Reply<T>> {
        self.exchange_with_purpose(context,work,command,ChildPurpose::Required,decode)
    }
    pub fn exchange_with_purpose<T: Send + 'static>(&self, context: &OperationContext, work: Work, command: u16,
        purpose: ChildPurpose, decode: impl FnOnce(Bytes,RawPolicy) -> Result<Decoded<T>> + Send + 'static) -> Result<Reply<T>> {
        let lease = self.session.lease(context)?;
        let reply = self.exchange_leased_with_purpose(context,&lease,work,command,purpose,decode)?;
        lease.accept(); Ok(reply)
    }
    /// A composite that must keep one source holds this same lease across its
    /// requests and checks the returned source stamps. It never creates a
    /// second Session or resets the operation's budget.
    pub fn exchange_leased<T: Send + 'static>(&self, context: &OperationContext, lease: &Lease, work: Work, command: u16,
        decode: impl FnOnce(Bytes,RawPolicy) -> Result<Decoded<T>> + Send + 'static) -> Result<Reply<T>> {
        self.exchange_leased_with_purpose(context,lease,work,command,ChildPurpose::Required,decode)
    }
    pub fn exchange_leased_with_purpose<T: Send + 'static>(&self, context: &OperationContext, lease: &Lease, work: Work, command: u16,
        purpose: ChildPurpose, decode: impl FnOnce(Bytes,RawPolicy) -> Result<Decoded<T>> + Send + 'static) -> Result<Reply<T>> {
        let authorization = context.begin_child(purpose,None)?.into_transport_authorization();
        let raw = context.raw(); let operation_session = context.session_id;
        let maximum_bytes = self.config.max_operation_bytes().get(); let session = Arc::clone(&self.session);
        let domain = self.domain;
        let reply = crate::ex::worker::execute(Arc::clone(&self.session),self.config.clone(),work,
            self.config.wire_attempt_limit().get(),authorization,context,lease.worker_slot(),move |reply| {
                let frame = reply.frame.ok_or_else(|| Error::new(ErrorKind::Internal,"MAC business reply lacks a frame"))?;
                let mut decoded = decode(frame.payload().clone(),raw)?;
                if !frame.association_verified() {
                    decoded.diagnostics.push(Diagnostic::new(domain.name(),"association",EnrichmentStatus::Unavailable,None,
                        Some("production response magic/method/sequence rules lack capture evidence".into())));
                }
                let retained = crate::ex::models::project_raw(frame.payload().clone(),Some(frame.wire().clone()),&decoded.ranges,raw,maximum_bytes)?;
                let wire_count = decoded.wire_row_count; let response_offset = decoded.response_offset;
                let source = SourceStamp::new(Some(command),Some(reply.endpoint),format!("{operation_session}:{}",reply.generation),None,None);
                let (value,meta) = decoded.with_meta(source,retained);
                session.record_meta(meta.clone())?;
                Ok(Reply { value,meta,wire_count,response_offset })
            }).map_err(|error| context.annotate(error,TimeoutPhase::Response,Some(command)).with_context("domain",self.domain.name()))?;
        context.record_work(usize::from(reply.wire_count.unwrap_or(0)),0)?;
        context.check_boundary(TimeoutPhase::Decode)?;
        Ok(reply)
    }
    pub fn probe(&self, context: &OperationContext, endpoints: Vec<Endpoint>, options: DiscoveryOptions,
        replace: bool, rank: bool) -> Result<Vec<HostProbe>> {
        if endpoints.is_empty() { context.check_boundary(TimeoutPhase::Admission)?; return Ok(Vec::new()); }
        let endpoints = self.config.clone().with_endpoints(endpoints)?.endpoints().to_vec();
        let lease = self.session.lease(context)?;
        let requests = endpoints.iter().map(|endpoint| {
            let authorization = context.begin_child(ChildPurpose::Required,None)?.into_transport_authorization();
            Ok((*endpoint,authorization))
        }).collect::<Result<Vec<_>>>()?;
        let profile: Arc<dyn crate::ex::probe::DataProbe> = Arc::new(MacDataProbe { domain: self.domain, setup: Arc::clone(&self.session.profile) });
        let config = self.config.clone();
        let probes = crate::ex::worker::run(Arc::clone(&self.session.stopped),context,lease.worker_slot(),move |control| {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()
                .map_err(|error| Error::new(ErrorKind::Internal,"cannot start MAC discovery runtime").with_source(error))?;
            let result = runtime.block_on(crate::ex::probe::run_with_profile(config,requests,options,control,profile));
            drop(runtime); result
        }).map_err(|error| context.annotate(error,TimeoutPhase::Response,None).with_context("domain",self.domain.name()))?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        self.session.update_probes(probes.clone(),replace.then_some(endpoints),rank)?;
        lease.accept(); Ok(probes)
    }
    pub fn best(&self, context: &OperationContext, options: DiscoveryOptions) -> Result<()> {
        if options.probe().mode() != ProbeMode::DataReady { return Err(super::query::invalid("MAC best host selection requires DataReady evidence")); }
        let probes = self.probe(context,self.diagnostics()?.candidates().to_vec(),options,false,true)?;
        if !probes.iter().any(HostProbe::data_ready) { return Err(Error::new(ErrorKind::DataNotReady,"no MAC endpoint completed its profile data probe")); }
        self.connect(context)
    }
}

#[derive(Debug)]
struct MacDataProbe { domain: Domain, setup: Arc<dyn SetupProfile> }
impl crate::ex::probe::DataProbe for MacDataProbe {
    fn setup(&self) -> Arc<dyn SetupProfile> { Arc::clone(&self.setup) }
    fn request(&self) -> Result<Work> {
        let request = match self.domain {
            Domain::Mac => super::commands::server_info_request()?,
            Domain::MacEx => super::commands::goods_request(super::MacMarketId::new(38),0,1)?,
        };
        let wire = if self.domain == Domain::MacEx { super::commands::rewrite_mac_ex_head(&request.wire)? } else { request.wire };
        Ok(Work::Encoded(Bytes::from(wire)))
    }
    fn validate(&self, frame: &crate::ex::frame::Frame) -> Result<()> {
        match self.domain {
            Domain::Mac => {
                let server = super::decode::server_info(frame.payload())?;
                if server.today().parsed().is_none() { return Err(Error::new(ErrorKind::Protocol,"MAC data probe lacks a valid today date")); }
            }
            Domain::MacEx => {
                let goods = super::decode::goods(frame.payload())?;
                if goods.len() != 1 { return Err(Error::new(ErrorKind::Protocol,"MAC-EX data probe requires one market38 goods record after login")); }
            }
        }
        Ok(())
    }
}

pub(crate) fn decoded<T>(value: T, count: Option<usize>, ranges: Vec<(usize,usize)>) -> Result<Decoded<T>> {
    let wire_row_count = count.map(|value| u16::try_from(value).map_err(|_| super::query::invalid("MAC wire row count exceeds u16"))).transpose()?;
    Ok(Decoded { value,diagnostics: Vec::new(),ranges,wire_row_count,response_offset: None })
}
pub(crate) fn rows<T>(value: Vec<T>, requested: u16, offset: usize, width: usize) -> Result<Decoded<Vec<T>>> {
    if value.len() > usize::from(requested) { return Err(Error::new(ErrorKind::Protocol,"MAC response rows exceed the requested page").with_context("domain","mac")); }
    let ranges = (0..value.len()).map(|index| {
        let start = index.checked_mul(width).and_then(|value| offset.checked_add(value))
            .ok_or_else(|| Error::new(ErrorKind::Protocol,"MAC raw row range overflow"))?;
        let end = start.checked_add(width).ok_or_else(|| Error::new(ErrorKind::Protocol,"MAC raw row end overflow"))?;
        Ok((start,end))
    }).collect::<Result<Vec<_>>>()?;
    let count = value.len(); decoded(value,Some(count),ranges)
}
