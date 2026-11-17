//! Owner-local helper caches. Entries preserve failed/partial source states;
//! local load day is a cache key, never proof of a response's trading date.
use std::sync::{Arc, Mutex};

use super::board_decode::BoardData;
use crate::services::allocation_error;
use crate::types::{
    BoardQuery, Collected, Diagnostic, EnrichmentStatus, FinanceRecord, HqMarket, RawPolicy,
    ResultMeta, Security, SecurityId, StatsQuery, StatsResource, TradingDate,
};
use crate::{Error, ErrorKind, Result};

const MAX_ENTRIES: usize = 16;
const MAX_ROWS: usize = 8_192;
const SUCCESS_TTL_SECONDS: i64 = 900;
const FAILURE_TTL_SECONDS: i64 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CacheStamp {
    pub date: TradingDate,
    pub version: Option<u64>,
    pub raw: RawPolicy,
    pub loaded_at: i64,
}

#[derive(Debug)]
pub(super) struct CachedSource<T> {
    pub result: Result<Arc<T>>,
    pub stamp: CacheStamp,
    pub expires_at: i64,
}
impl<T> Clone for CachedSource<T> {
    fn clone(&self) -> Self {
        Self {
            result: self.result.clone(),
            stamp: self.stamp,
            expires_at: self.expires_at,
        }
    }
}
impl<T> CachedSource<T> {
    pub fn new(result: Result<Arc<T>>, stamp: CacheStamp, complete: bool) -> Result<Self> {
        let ttl = if complete && result.is_ok() {
            SUCCESS_TTL_SECONDS
        } else {
            FAILURE_TTL_SECONDS
        };
        let expires_at = stamp.loaded_at.checked_add(ttl).ok_or_else(|| {
            Error::new(
                ErrorKind::LimitExceeded,
                "helper cache expiry exceeds the epoch range",
            )
        })?;
        Ok(Self {
            result,
            stamp,
            expires_at,
        })
    }
    pub fn diagnostic(
        &self,
        source: &str,
        stage: &str,
        hit: bool,
        status: EnrichmentStatus,
    ) -> Diagnostic {
        Diagnostic::new(source, stage, status, self.result.as_ref().err().map(Error::kind),
            Some(format!("cache_hit={hit}; loaded_at={}; expires_at={}; metadata_version={:?}; load_date={:?}",
                self.stamp.loaded_at, self.expires_at, self.stamp.version, self.stamp.date)))
    }
}

#[derive(Clone, Debug)]
pub(super) struct Supplement<T> {
    pub value: Option<T>,
    pub status: EnrichmentStatus,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Debug)]
pub(super) struct FinanceBundle {
    pub rows: Vec<Supplement<FinanceRecord>>,
    pub meta: ResultMeta,
    pub any_failed: bool,
}

#[derive(Debug)]
struct Entry<K, T> {
    key: K,
    source: CachedSource<T>,
    weight: usize,
}

#[derive(Debug)]
struct SourceCache<K, T> {
    entries: Vec<Entry<K, T>>,
    rows: usize,
}
impl<K, T> Default for SourceCache<K, T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            rows: 0,
        }
    }
}
impl<K: Eq, T> SourceCache<K, T> {
    fn get(&self, key: &K, now: CacheStamp) -> Option<CachedSource<T>> {
        self.entries
            .iter()
            .find(|entry| {
                entry.key == *key
                    && entry.source.stamp.date == now.date
                    && entry.source.stamp.version == now.version
                    && entry.source.stamp.raw == now.raw
                    && now.loaded_at >= entry.source.stamp.loaded_at
                    && now.loaded_at < entry.source.expires_at
            })
            .map(|entry| entry.source.clone())
    }

    fn put(&mut self, key: K, source: CachedSource<T>, weight: usize) -> Result<()> {
        self.put_bounded(key, source, weight, MAX_ENTRIES, MAX_ROWS)
    }

    fn put_bounded(
        &mut self,
        key: K,
        source: CachedSource<T>,
        weight: usize,
        max_entries: usize,
        max_weight: usize,
    ) -> Result<()> {
        let weight = weight.max(1);
        // Oversized source results are delivered normally but never retained.
        if weight > max_weight {
            return Ok(());
        }
        if self.entries.iter().any(|entry| {
            entry.key == key
                && (entry.source.stamp.version > source.stamp.version
                    || (entry.source.stamp.version == source.stamp.version
                        && entry.source.stamp.loaded_at > source.stamp.loaded_at))
        }) {
            return Ok(());
        }
        self.entries.try_reserve(1).map_err(allocation_error)?;
        let mut index = 0;
        while index < self.entries.len() {
            if self.entries[index].key == key {
                self.rows -= self.entries.remove(index).weight;
            } else {
                index += 1;
            }
        }
        while self.entries.len() >= max_entries || self.rows > max_weight - weight {
            self.rows -= self.entries.remove(0).weight;
        }
        self.rows += weight;
        self.entries.push(Entry {
            key,
            source,
            weight,
        });
        Ok(())
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.rows = 0;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StatsCacheKey {
    pub query: StatsQuery,
    pub target: TradingDate,
    pub previous: TradingDate,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BoardCacheKey {
    pub query: BoardQuery,
    pub target: TradingDate,
}

#[derive(Debug, Default)]
struct CacheState {
    generation: u64,
    disabled: bool,
    catalog: SourceCache<HqMarket, Collected<Security>>,
    finance: SourceCache<Vec<SecurityId>, FinanceBundle>,
    stats: SourceCache<StatsCacheKey, StatsResource>,
    boards: SourceCache<BoardCacheKey, BoardData>,
}

/// Cloned services all refer to this owner's memory. Clear invalidates every
/// in-flight publication; no locks are held while source I/O is in progress.
#[derive(Debug, Default)]
pub(crate) struct HelperCache {
    state: Mutex<CacheState>,
}
impl HelperCache {
    pub(super) fn boards(
        &self,
        key: &BoardCacheKey,
        now: CacheStamp,
    ) -> Result<(u64, Option<CachedSource<BoardData>>)> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        Ok((state.generation, state.boards.get(key, now)))
    }
    pub(super) fn publish_boards(
        &self,
        generation: u64,
        key: BoardCacheKey,
        source: CachedSource<BoardData>,
    ) -> Result<()> {
        let weight = source.result.as_ref().map_or(1, |data| data.weight());
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.disabled && state.generation == generation {
            state.boards.put_bounded(key, source, weight, 2, 131_072)?;
        }
        Ok(())
    }
    pub(super) fn stats(
        &self,
        key: &StatsCacheKey,
        now: CacheStamp,
    ) -> Result<(u64, Option<CachedSource<StatsResource>>)> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        Ok((state.generation, state.stats.get(key, now)))
    }
    pub(super) fn publish_stats(
        &self,
        generation: u64,
        key: StatsCacheKey,
        source: CachedSource<StatsResource>,
    ) -> Result<()> {
        let rows = source.result.as_ref().map_or(1, |resource| {
            resource.stat().len().saturating_add(resource.stat2().len())
        });
        let raw_bytes = source
            .result
            .as_ref()
            .ok()
            .and_then(|resource| resource.meta().raw())
            .map_or(0_usize, |raw| {
                raw.payloads()
                    .iter()
                    .chain(raw.wire_frames().unwrap_or(&[]))
                    .fold(0_usize, |sum, bytes| sum.saturating_add(bytes.len()))
            });
        // One row or 4 KiB retained raw consumes one weight; maximum two
        // entries and 65536 weights. Large results are delivered, not cached.
        let weight = rows.saturating_add(raw_bytes.div_ceil(4096));
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.disabled && state.generation == generation {
            state.stats.put_bounded(key, source, weight, 2, 65_536)?;
        }
        Ok(())
    }
    pub(super) fn catalog(
        &self,
        market: HqMarket,
        now: CacheStamp,
    ) -> Result<(u64, Option<CachedSource<Collected<Security>>>)> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        Ok((state.generation, state.catalog.get(&market, now)))
    }
    pub(super) fn finance(
        &self,
        ids: &[SecurityId],
        now: CacheStamp,
    ) -> Result<(u64, Option<CachedSource<FinanceBundle>>)> {
        let state = self.state.lock().map_err(|_| poisoned())?;
        let source = state
            .finance
            .entries
            .iter()
            .find(|entry| {
                entry.key.as_slice() == ids
                    && entry.source.stamp.date == now.date
                    && entry.source.stamp.version == now.version
                    && entry.source.stamp.raw == now.raw
                    && now.loaded_at >= entry.source.stamp.loaded_at
                    && now.loaded_at < entry.source.expires_at
            })
            .map(|entry| entry.source.clone());
        Ok((state.generation, source))
    }
    pub(super) fn publish_catalog(
        &self,
        generation: u64,
        market: HqMarket,
        source: CachedSource<Collected<Security>>,
    ) -> Result<()> {
        let weight = source
            .result
            .as_ref()
            .map_or(1, |catalog| catalog.items().len());
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.disabled && state.generation == generation {
            state.catalog.put(market, source, weight)?;
        }
        Ok(())
    }
    pub(super) fn publish_finance(
        &self,
        generation: u64,
        ids: &[SecurityId],
        source: CachedSource<FinanceBundle>,
    ) -> Result<()> {
        if ids.len() > MAX_ROWS {
            return Ok(());
        }
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if !state.disabled && state.generation == generation {
            let mut key = Vec::new();
            key.try_reserve(ids.len()).map_err(allocation_error)?;
            key.extend_from_slice(ids);
            state.finance.put(key, source, ids.len())?;
        }
        Ok(())
    }
    pub(super) fn clear(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.generation == u64::MAX {
            state.disabled = true;
        }
        state.generation = state.generation.saturating_add(1);
        state.catalog.clear();
        state.finance.clear();
        state.stats.clear();
        state.boards.clear();
    }
}
fn poisoned() -> Error {
    Error::new(ErrorKind::Internal, "helper cache is poisoned")
}

#[cfg(test)]
mod source_cases {
    use super::*;

    fn stamp(at: i64) -> CacheStamp {
        CacheStamp {
            date: TradingDate::new(2026, 11, 16).unwrap(),
            version: Some(7),
            raw: RawPolicy::Omit,
            loaded_at: at,
        }
    }

    #[test]
    fn successful_failure_and_partial_entries_have_explicit_expiry_and_never_become_empty_success()
    {
        let mut cache = SourceCache::default();
        let success = CachedSource::new(Ok(Arc::new(13)), stamp(100), true).unwrap();
        let failure = CachedSource::<i32>::new(
            Err(Error::new(ErrorKind::Connection, "unavailable")),
            stamp(100),
            false,
        )
        .unwrap();
        let partial = CachedSource::new(Ok(Arc::new(17)), stamp(100), false).unwrap();
        assert_eq!(success.expires_at, 1000);
        assert_eq!(failure.expires_at, 130);
        assert_eq!(partial.expires_at, 130);
        cache.put(1, success, 1).unwrap();
        cache.put(2, failure, 1).unwrap();
        cache.put(3, partial, 1).unwrap();
        assert_eq!(
            **cache.get(&1, stamp(999)).unwrap().result.as_ref().unwrap(),
            13
        );
        assert!(cache.get(&1, stamp(1000)).is_none());
        assert!(cache.get(&1, stamp(99)).is_none());
        assert_eq!(
            cache
                .get(&2, stamp(129))
                .unwrap()
                .result
                .unwrap_err()
                .kind(),
            ErrorKind::Connection
        );
        assert!(cache.get(&2, stamp(130)).is_none());
        assert!(cache.get(&3, stamp(130)).is_none());
        for mismatch in [
            CacheStamp {
                version: Some(8),
                ..stamp(101)
            },
            CacheStamp {
                raw: RawPolicy::RetainPayload,
                ..stamp(101)
            },
            CacheStamp {
                date: TradingDate::new(2026, 11, 17).unwrap(),
                ..stamp(101)
            },
        ] {
            assert!(cache.get(&1, mismatch).is_none());
        }
    }

    #[test]
    fn clear_rejects_old_publication_and_caches_have_cardinality_and_row_bounds() {
        let cache = HelperCache::default();
        let (generation, _) = cache.catalog(HqMarket::Shanghai, stamp(100)).unwrap();
        cache.clear();
        let source = CachedSource::new(
            Ok(Arc::new(Collected::new(
                Vec::new(),
                crate::EndEvidence::EmptyPage,
                1,
                ResultMeta::default(),
            ))),
            stamp(100),
            true,
        )
        .unwrap();
        cache
            .publish_catalog(generation, HqMarket::Shanghai, source)
            .unwrap();
        assert!(cache
            .catalog(HqMarket::Shanghai, stamp(101))
            .unwrap()
            .1
            .is_none());
        let mut bounded = SourceCache::default();
        for key in 0..32 {
            bounded
                .put(
                    key,
                    CachedSource::new(Ok(Arc::new(key)), stamp(100), true).unwrap(),
                    1,
                )
                .unwrap();
        }
        assert_eq!(bounded.entries.len(), MAX_ENTRIES);
        assert!(bounded.get(&0, stamp(101)).is_none());
        bounded
            .put(
                99,
                CachedSource::new(Ok(Arc::new(99)), stamp(100), true).unwrap(),
                MAX_ROWS,
            )
            .unwrap();
        assert_eq!(bounded.entries.len(), 1);
        assert_eq!(bounded.rows, MAX_ROWS);
        bounded
            .put(
                100,
                CachedSource::new(Ok(Arc::new(100)), stamp(100), true).unwrap(),
                MAX_ROWS + 1,
            )
            .unwrap();
        assert!(bounded.get(&100, stamp(101)).is_none());
    }

    #[test]
    fn late_older_source_cannot_overwrite_a_newer_version_or_load() {
        let mut cache = SourceCache::default();
        cache
            .put(
                1,
                CachedSource::new(Ok(Arc::new(20)), stamp(200), true).unwrap(),
                1,
            )
            .unwrap();
        cache
            .put(
                1,
                CachedSource::new(Ok(Arc::new(10)), stamp(100), true).unwrap(),
                1,
            )
            .unwrap();
        assert_eq!(*cache.get(&1, stamp(201)).unwrap().result.unwrap(), 20);
        let older_version = CacheStamp {
            version: Some(6),
            ..stamp(300)
        };
        cache
            .put(
                1,
                CachedSource::new(Ok(Arc::new(6)), older_version, true).unwrap(),
                1,
            )
            .unwrap();
        assert_eq!(*cache.get(&1, stamp(301)).unwrap().result.unwrap(), 20);
    }

    #[test]
    fn statistics_keys_keep_target_previous_query_and_raw_and_clear_rejects_inflight_publication() {
        let cache = HelperCache::default();
        let target = stamp(100).date;
        let previous = target.add_days(-3).unwrap();
        let key = StatsCacheKey {
            query: StatsQuery::new(),
            target,
            previous,
        };
        let resource = StatsResource {
            stat: Vec::new(),
            stat2: Vec::new(),
            source_path: "tdx://zhb.zip".to_owned(),
            meta: ResultMeta::default(),
        };
        let source = CachedSource::new(Ok(Arc::new(resource)), stamp(100), true).unwrap();
        let generation = cache.stats(&key, stamp(100)).unwrap().0;
        cache
            .publish_stats(generation, key.clone(), source.clone())
            .unwrap();
        assert!(cache.stats(&key, stamp(101)).unwrap().1.is_some());
        for other in [
            StatsCacheKey {
                target: target.add_days(1).unwrap(),
                ..key.clone()
            },
            StatsCacheKey {
                previous: previous.add_days(-1).unwrap(),
                ..key.clone()
            },
            StatsCacheKey {
                query: StatsQuery::new().with_path("other.zip".to_owned()).unwrap(),
                ..key.clone()
            },
        ] {
            assert!(cache.stats(&other, stamp(101)).unwrap().1.is_none());
        }
        assert!(cache
            .stats(
                &key,
                CacheStamp {
                    raw: RawPolicy::RetainPayload,
                    ..stamp(101)
                }
            )
            .unwrap()
            .1
            .is_none());
        cache.clear();
        cache
            .publish_stats(generation, key.clone(), source)
            .unwrap();
        assert!(cache.stats(&key, stamp(101)).unwrap().1.is_none());
    }

    #[test]
    fn board_cache_keys_keep_explicit_sources_dates_raw_and_clear_is_generation_scoped() {
        let cache = HelperCache::default();
        let key = BoardCacheKey {
            query: BoardQuery::new(),
            target: stamp(100).date,
        };
        let data = BoardData {
            definitions: Vec::new(),
            files: Vec::new(),
            concepts: std::collections::HashMap::new(),
            meta: ResultMeta::default(),
        };
        let source = CachedSource::new(Ok(Arc::new(data)), stamp(100), true).unwrap();
        let generation = cache.boards(&key, stamp(100)).unwrap().0;
        cache
            .publish_boards(generation, key.clone(), source.clone())
            .unwrap();
        assert!(cache.boards(&key, stamp(101)).unwrap().1.is_some());
        let different = BoardCacheKey {
            query: key
                .query
                .clone()
                .with_data_dir(Some(std::path::PathBuf::from("explicit")))
                .unwrap(),
            ..key.clone()
        };
        assert!(cache.boards(&different, stamp(101)).unwrap().1.is_none());
        assert!(cache
            .boards(
                &BoardCacheKey {
                    target: key.target.add_days(1).unwrap(),
                    ..key.clone()
                },
                stamp(101)
            )
            .unwrap()
            .1
            .is_none());
        assert!(cache
            .boards(
                &key,
                CacheStamp {
                    raw: RawPolicy::RetainPayload,
                    ..stamp(101)
                }
            )
            .unwrap()
            .1
            .is_none());
        cache.clear();
        cache
            .publish_boards(generation, key.clone(), source)
            .unwrap();
        assert!(cache.boards(&key, stamp(101)).unwrap().1.is_none());
    }
}
