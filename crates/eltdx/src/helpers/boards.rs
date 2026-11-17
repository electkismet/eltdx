use bytes::Bytes;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::board_decode::{self, BoardData};
use super::cache::{BoardCacheKey, CachedSource, Supplement};
use super::service::{failed_diagnostic, optional_failure, HelperService};
use crate::operation::{OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, allocation_error, ResourceService};
use crate::types::boards::{MAX_BOARD_FILE_BYTES, MAX_BOARD_TOTAL_BYTES};
use crate::types::{
    BatchItemState, BoardCategory, BoardDefinition, BoardFile, BoardId, BoardMember,
    BoardMemberQuotes, BoardQuery, BoardQuote, CodeScanQuery, Diagnostic, DownloadCompletion,
    EnrichmentStatus, HqMarket, MissingPolicy, RawData, RawPolicy, ResultMeta, SecurityId,
    Snapshot, SourceStamp, Table, TradingDate,
};
use crate::{Error, ErrorKind, Result, TimeoutPhase};

const MARKETS: [HqMarket; 3] = [HqMarket::Shanghai, HqMarket::Shenzhen, HqMarket::Beijing];

impl HelperService {
    pub fn board_quotes(
        &self,
        query: &BoardQuery,
        options: &OperationOptions,
    ) -> Result<Table<BoardQuote>> {
        let mut context = self.executor.begin_context_at(
            Instant::now(),
            "helpers_board_quotes",
            &self.config,
            options,
        )?;
        query.validate()?;
        board_limits(&mut context)?;
        let (target, handshake) = self.source_date_in_context(&context)?;
        let (data, cache_diagnostic) = self.boards_in_context(&context, query, target)?;
        let mut selected = Vec::new();
        for definition in &data.definitions {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            if query.category() != BoardCategory::All && query.category() != definition.category() {
                continue;
            }
            // A requested region/industry/concept category must have its actual
            // required membership source; definitions alone are not readiness.
            data.members(definition, &mut || {
                context.check_boundary(TimeoutPhase::Decode)
            })?;
            selected.try_reserve(1).map_err(allocation_error)?;
            selected.push((definition.clone(), definition.security_id()?));
        }
        if selected.is_empty() {
            return Err(format_error(
                "selected board category is unavailable in these sources",
            ));
        }
        let ids = selected
            .iter()
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        let (quotes, quotes_meta) = self.board_snapshots_in_context(&context, &ids)?;
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let common = aggregate_meta(
            &context,
            &[handshake.meta().clone(), data.meta.clone(), quotes_meta],
            version,
        )?
        .with_diagnostic(cache_diagnostic);
        let mut rows = Vec::new();
        rows.try_reserve(selected.len()).map_err(allocation_error)?;
        for (board, id) in selected {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let source = quotes
                .get(&id)
                .ok_or_else(|| internal("board quote supplement disappeared"))?;
            let meta = board_row_meta(&context, &data.meta, source, version)?;
            rows.push(BoardQuote {
                board,
                security_id: id,
                quote: source.value.clone(),
                meta,
            });
        }
        context.finish()?;
        Ok(Table::new(rows, common))
    }

    pub fn board_member_quotes(
        &self,
        board: &BoardId,
        query: &BoardQuery,
        options: &OperationOptions,
    ) -> Result<BoardMemberQuotes> {
        let mut context = self.executor.begin_context_at(
            Instant::now(),
            "helpers_board_members",
            &self.config,
            options,
        )?;
        query.validate()?;
        board_limits(&mut context)?;
        let (target, handshake) = self.source_date_in_context(&context)?;
        let (data, cache_diagnostic) = self.boards_in_context(&context, query, target)?;
        let matching = data
            .definitions
            .iter()
            .filter(|definition| {
                definition.id().number() == board.number()
                    && board
                        .market()
                        .is_none_or(|market| market == definition.market())
            })
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "board lookup has no unique matching definition",
            )
            .with_context("board", board.number()));
        }
        let definition: BoardDefinition = (*matching[0]).clone();
        let members = data.members(&definition, &mut || {
            context.check_boundary(TimeoutPhase::Decode)
        })?;
        let catalog =
            self.codes()
                .collect_in_context(&context, &MARKETS, &CodeScanQuery::new(), |_| true)?;
        let mut current = HashSet::new();
        current
            .try_reserve(catalog.items().len())
            .map_err(allocation_error)?;
        for security in catalog.items() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            current.insert(security.id().clone());
        }
        let (raw_members, display_members, excluded_members) =
            filter_members(members, &current, || {
                context.check_boundary(TimeoutPhase::Aggregate)
            })?;
        let ids = display_members
            .iter()
            .map(|member| member.id().clone())
            .collect::<Vec<_>>();
        let (quotes, quote_meta) = self.board_snapshots_in_context(&context, &ids)?;
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let meta = aggregate_meta(
            &context,
            &[
                handshake.meta().clone(),
                data.meta.clone(),
                catalog.meta().clone(),
                quote_meta,
            ],
            version,
        )?
        .with_diagnostic(cache_diagnostic);
        let mut rows = Vec::new();
        rows.try_reserve(ids.len()).map_err(allocation_error)?;
        for id in ids {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let source = quotes
                .get(&id)
                .ok_or_else(|| internal("member quote supplement disappeared"))?;
            rows.push(BoardQuote {
                board: definition.clone(),
                security_id: id,
                quote: source.value.clone(),
                meta: board_row_meta(&context, &data.meta, source, version)?,
            });
        }
        context.finish()?;
        Ok(BoardMemberQuotes {
            board: definition,
            prepared_date: target,
            raw_members,
            display_members,
            excluded_members,
            rows,
            meta,
        })
    }

    fn boards_in_context(
        &self,
        context: &OperationContext,
        query: &BoardQuery,
        target: TradingDate,
    ) -> Result<(Arc<BoardData>, Diagnostic)> {
        let key = BoardCacheKey {
            query: query.source_key(),
            target,
        };
        let stamp = self.cache_stamp(context, None)?;
        let cache = context.owner().helpers();
        let (generation, hit) = cache.boards(&key, stamp)?;
        if !query.refresh() {
            if let Some(source) = hit {
                let diagnostic = source.diagnostic(
                    "board_files",
                    "boards.cache",
                    true,
                    EnrichmentStatus::Succeeded,
                );
                return Ok((source.result?, diagnostic));
            }
        }
        let mut files = Vec::new();
        let mut metas = Vec::new();
        let mut size = 0_usize;
        // Fixed names only. Explicit bytes/directories have precedence and no
        // ambient environment, cwd/downloads or installed-TDX discovery occurs.
        for name in [
            "tdxzs.cfg",
            "tdxzs3.cfg",
            "infoharbor_block.dat",
            "tdxhy.cfg",
            "base.dbf",
        ] {
            context.check_boundary(TimeoutPhase::Admission)?;
            let mut source_meta = None;
            let bytes = if let Some(file) = query.files().iter().find(|file| file.name() == name) {
                source_meta = Some(local_meta(
                    context,
                    name,
                    file.data().clone(),
                    "explicit_memory",
                ));
                Some(file.data().clone())
            } else {
                let definition = matches!(name, "tdxzs.cfg" | "tdxzs3.cfg");
                let directory = if definition {
                    query.definitions_dir().or(query.data_dir())
                } else {
                    query.data_dir()
                };
                match directory {
                    Some(directory) => match read_explicit(context, &directory.join(name))? {
                        Some(bytes) => {
                            source_meta = Some(local_meta(
                                context,
                                name,
                                bytes.clone(),
                                "explicit_directory",
                            ));
                            Some(bytes)
                        }
                        None if definition && query.definitions_dir().is_some() => {
                            return Err(format_error(
                                "explicit definitions directory lacks a required definition file",
                            )
                            .with_context("filename", name))
                        }
                        None => None,
                    },
                    None => None,
                }
            };
            let bytes = if bytes.is_none() && matches!(name, "infoharbor_block.dat" | "tdxhy.cfg") {
                // Fixed HQ resource pair, as in the existing preparation flow.
                // Optional counterpart failure never suppresses an independently
                // complete source for the category actually requested.
                let service = ResourceService::new(
                    self.executor.clone(),
                    self.engine.clone(),
                    self.config.clone(),
                );
                match service.download_in_context(
                    context,
                    name,
                    MAX_BOARD_FILE_BYTES + 1,
                    30_000,
                    true,
                ) {
                    Ok(download) => {
                        if download.result.data.len() > MAX_BOARD_FILE_BYTES
                            || download.result.completion != DownloadCompletion::Complete
                        {
                            return Err(limit(
                                "board resource exceeds its complete-file byte bound",
                            ));
                        }
                        source_meta = Some(download.meta.with_diagnostic(Diagnostic::new("hq.resources", "boards.source_date", EnrichmentStatus::Unavailable,
                            None, Some(format!("{name}: fetched in target={target} preparation; the file's trading date is not independently asserted")))));
                        (!download.result.data.is_empty()).then_some(download.result.data)
                    }
                    Err(error) => {
                        optional_failure(context, &error)?;
                        metas.push(ResultMeta::default().with_diagnostic(failed_diagnostic(
                            "hq.resources",
                            &format!("boards.{name}"),
                            &error,
                        )));
                        None
                    }
                }
            } else {
                bytes
            };
            if let Some(source_meta) = source_meta {
                metas.push(source_meta);
            }
            if let Some(bytes) = bytes {
                size = size
                    .checked_add(bytes.len())
                    .ok_or_else(|| limit("aggregate board source size overflows"))?;
                if size > MAX_BOARD_TOTAL_BYTES {
                    return Err(limit("aggregate board sources exceed 64 MiB"));
                }
                files.try_reserve(1).map_err(allocation_error)?;
                files.push(BoardFile::new(name, bytes)?);
            }
        }
        let meta = aggregate_meta(context, &metas, None)?;
        let data = Arc::new(board_decode::parse(files, meta, &mut || {
            context.check_boundary(TimeoutPhase::Decode)
        })?);
        let complete = !data
            .meta
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status() == EnrichmentStatus::Failed);
        let source = CachedSource::new(Ok(data.clone()), stamp, complete)?;
        let diagnostic = source.diagnostic(
            "board_files",
            "boards.cache",
            false,
            EnrichmentStatus::Succeeded,
        );
        context.check_boundary(TimeoutPhase::Aggregate)?;
        cache.publish_boards(generation, key, source)?;
        Ok((data, diagnostic))
    }

    fn board_snapshots_in_context(
        &self,
        context: &OperationContext,
        ids: &[SecurityId],
    ) -> Result<(HashMap<SecurityId, Supplement<Snapshot>>, ResultMeta)> {
        let mut rows = HashMap::new();
        rows.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut metas = Vec::new();
        let quotes = self.quotes();
        if ids.is_empty() {
            return Ok((rows, ResultMeta::default()));
        }
        // Parsed board sources may contain more members than one public batch.
        // In that case prepare the complete three-market scope once; each wire
        // batch below still contains at most 80 occurrences. Do not turn a local
        // batch-size rejection into an optional source failure for every row.
        let preparation = if ids.len() > 65_535 {
            quotes.prepare_composition_in_context(context, &[], true)
        } else {
            quotes.prepare_in_context(context, ids)
        };
        if let Err(error) = preparation {
            optional_failure(context, &error)?;
            let diagnostic = failed_diagnostic("hq.quotes", "boards.quotes", &error);
            for id in ids {
                rows.insert(
                    id.clone(),
                    Supplement {
                        value: None,
                        status: EnrichmentStatus::Failed,
                        diagnostic: diagnostic.clone(),
                    },
                );
            }
            return Ok((rows, ResultMeta::default().with_diagnostic(diagnostic)));
        }
        for chunk in ids.chunks(80) {
            context.check_boundary(TimeoutPhase::Admission)?;
            match quotes.snapshots_in_context(context, chunk, MissingPolicy::AllowPartial, 80) {
                Ok(batch) => {
                    if batch.len() != chunk.len() {
                        return Err(internal("board snapshot batch lost input occurrences"));
                    }
                    let mut batch_meta = batch.meta().clone();
                    let mut missing = 0_usize;
                    for (expected, item) in batch.into_items().into_iter().enumerate() {
                        let (index, id, state) = item.into_parts();
                        if index != expected || chunk.get(index) != Some(&id) {
                            return Err(internal("board snapshot identity association changed"));
                        }
                        let (value, status) = match state {
                            BatchItemState::Present(value) => {
                                (Some(value), EnrichmentStatus::Succeeded)
                            }
                            BatchItemState::ConfirmedMissing(_) => {
                                (None, EnrichmentStatus::Unavailable)
                            }
                        };
                        let message = if value.is_none() {
                            missing += 1;
                            Some(format!(
                                "{id}: quote confirmed missing after snapshot recovery"
                            ))
                        } else {
                            None
                        };
                        rows.insert(
                            id,
                            Supplement {
                                value,
                                status,
                                diagnostic: Diagnostic::new(
                                    "hq.quotes",
                                    "boards.quotes",
                                    status,
                                    None,
                                    message,
                                ),
                            },
                        );
                    }
                    if missing != 0 {
                        batch_meta = batch_meta.with_diagnostic(Diagnostic::new(
                            "hq.quotes",
                            "boards.quotes",
                            EnrichmentStatus::Unavailable,
                            None,
                            Some(format!(
                                "{missing} quotes confirmed missing in this {}-member batch",
                                chunk.len()
                            )),
                        ));
                    }
                    metas.try_reserve(1).map_err(allocation_error)?;
                    metas.push(batch_meta);
                }
                Err(error) => {
                    optional_failure(context, &error)?;
                    let diagnostic = failed_diagnostic("hq.quotes", "boards.quotes", &error);
                    metas.push(ResultMeta::default().with_diagnostic(diagnostic.clone()));
                    for id in chunk {
                        rows.insert(
                            id.clone(),
                            Supplement {
                                value: None,
                                status: EnrichmentStatus::Failed,
                                diagnostic: diagnostic.clone(),
                            },
                        );
                    }
                }
            }
        }
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        Ok((rows, aggregate_meta(context, &metas, version)?))
    }
}

type MemberGroups = (Vec<BoardMember>, Vec<BoardMember>, Vec<BoardMember>);
fn filter_members(
    members: Vec<SecurityId>,
    current: &HashSet<SecurityId>,
    mut boundary: impl FnMut() -> Result<()>,
) -> Result<MemberGroups> {
    let mut raw = Vec::new();
    let mut display = Vec::new();
    let mut excluded = Vec::new();
    raw.try_reserve(members.len()).map_err(allocation_error)?;
    display
        .try_reserve(members.len())
        .map_err(allocation_error)?;
    excluded
        .try_reserve(members.len())
        .map_err(allocation_error)?;
    for id in members {
        boundary()?;
        let active = current.contains(&id);
        raw.push(BoardMember {
            id: id.clone(),
            excluded_reason: (!active).then(|| "absent_from_current_security_list".to_owned()),
        });
        if active {
            display.push(BoardMember {
                id,
                excluded_reason: None,
            });
        } else {
            excluded.push(BoardMember {
                id,
                excluded_reason: Some("absent_from_current_security_list".to_owned()),
            });
        }
    }
    boundary()?;
    Ok((raw, display, excluded))
}

fn board_limits(context: &mut OperationContext) -> Result<()> {
    let records = CodeScanQuery::default()
        .pages()
        .max_records()
        .checked_mul(3)
        .and_then(|count| count.checked_add(1_000_000 + 65_535))
        .ok_or_else(|| limit("board operation allowance overflows"))?;
    context.set_work_limits(WorkLimits {
        pages: None,
        records: Some(records),
        bytes: None,
    })
}
fn board_row_meta(
    context: &OperationContext,
    source: &ResultMeta,
    quote: &Supplement<Snapshot>,
    version: Option<u64>,
) -> Result<ResultMeta> {
    let mut metas = vec![source.clone()];
    if let Some(snapshot) = &quote.value {
        metas.push(snapshot.meta().clone());
    }
    Ok(aggregate_meta(context, &metas, version)?.with_diagnostic(quote.diagnostic.clone()))
}
fn local_meta(context: &OperationContext, name: &str, bytes: Bytes, source: &str) -> ResultMeta {
    let raw = (context.raw() != RawPolicy::Omit)
        .then(|| RawData::new(vec![bytes.clone()], None, vec![(0, 0, bytes.len())]));
    ResultMeta::new(
        SourceStamp::new(
            None,
            None,
            format!("owner-{:x}", context.session_id),
            None,
            None,
        ),
        vec![Diagnostic::new(
            source,
            "boards.source_date",
            EnrichmentStatus::Unavailable,
            None,
            Some(format!(
                "{name}: explicit file bytes; trading-date/freshness evidence unavailable"
            )),
        )],
        raw,
    )
}
fn read_explicit(context: &OperationContext, path: &Path) -> Result<Option<Bytes>> {
    context.check_boundary(TimeoutPhase::Admission)?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format_error("unable to open explicit board source")
                .with_context("path", format!("{}", path.display()))
                .with_source(error))
        }
    };
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        context.check_boundary(TimeoutPhase::Decode)?;
        let remaining = (MAX_BOARD_FILE_BYTES + 1).saturating_sub(bytes.len());
        if remaining == 0 {
            return Err(limit("explicit board source exceeds 32 MiB"));
        }
        let count = file
            .read(&mut buffer[..remaining.min(16 * 1024)])
            .map_err(|error| {
                format_error("unable to read explicit board source").with_source(error)
            })?;
        if count == 0 {
            break;
        }
        bytes.try_reserve(count).map_err(allocation_error)?;
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_BOARD_FILE_BYTES {
            return Err(limit("explicit board source exceeds 32 MiB"));
        }
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(Some(Bytes::from(bytes)))
}
fn format_error(message: &str) -> Error {
    Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "HQ boards")
}
fn limit(message: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, message).with_context("domain", "HQ boards")
}
fn internal(message: &str) -> Error {
    Error::new(ErrorKind::Internal, message)
}

#[cfg(test)]
mod source_cases {
    use super::*;
    #[test]
    fn member_filter_retains_raw_facts_exclusion_reasons_and_order_without_substitution() {
        let members = ["sh600001", "sh600000", "sh600099"]
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect::<Vec<_>>();
        let current = ["sh600000", "sh600001"]
            .into_iter()
            .map(|id| id.parse().unwrap())
            .collect::<HashSet<_>>();
        let (raw, display, excluded) = filter_members(members, &current, || Ok(())).unwrap();
        assert_eq!(raw.len(), 3);
        assert_eq!(
            display
                .iter()
                .map(|member| member.id().number())
                .collect::<Vec<_>>(),
            vec!["600001", "600000"]
        );
        assert_eq!(excluded[0].id().number(), "600099");
        assert_eq!(
            raw[2].excluded_reason(),
            Some("absent_from_current_security_list")
        );
        assert_eq!(excluded[0].excluded_reason(), raw[2].excluded_reason());
    }
}
