//! Immutable source-scoped directory snapshots. No I/O or process-global cache.
use std::sync::Arc;
use crate::{Error, ErrorKind, RawPolicy, Result};
use crate::types::ResultMeta;
use super::{ExMarketId, hosts::Endpoint};

pub const CATALOG_SCHEMA_VERSION: u32 = 1;
pub(crate) const CACHE_ENTRIES: usize = 16;
pub(crate) const CACHE_ROWS: usize = 65_536;
pub(crate) const CACHE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogKind { MacGoods, ExInstruments }
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Key {
    pub owner: usize, pub endpoint: Endpoint, pub generation: u64,
    pub profile: &'static str, pub market: ExMarketId,
    pub kind: CatalogKind, pub raw: RawPolicy, pub schema: u32,
}

/// A snapshot remains readable after refresh, source replacement or owner close.
/// Its source generation labels the observation; it is not a server transaction.
#[derive(Clone, Debug)]
pub struct CatalogSnapshot<T> {
    pub(crate) key: Key, pub(crate) version: u64, pub(crate) rows: Vec<T>,
    pub(crate) meta: ResultMeta, pub(crate) global_count: Option<u32>,
    pub(crate) sorted_bounds: Option<(u32,u32)>, pub(crate) heap_bytes: usize,
}
impl<T> CatalogSnapshot<T> {
    pub fn rows(&self) -> &[T] { &self.rows }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub const fn owner_id(&self) -> usize { self.key.owner }
    pub const fn endpoint(&self) -> Endpoint { self.key.endpoint }
    pub const fn generation(&self) -> u64 { self.key.generation }
    pub fn profile(&self) -> &str { self.key.profile }
    pub const fn market(&self) -> ExMarketId { self.key.market }
    pub const fn kind(&self) -> CatalogKind { self.key.kind }
    pub const fn schema_version(&self) -> u32 { self.key.schema }
    pub const fn raw_policy(&self) -> RawPolicy { self.key.raw }
    pub const fn version(&self) -> u64 { self.version }
    pub const fn global_count(&self) -> Option<u32> { self.global_count }
    /// Present only after the entire observed EX directory is verified sorted.
    pub const fn sorted_global_bounds(&self) -> Option<(u32,u32)> { self.sorted_bounds }
}

/// The caller serializes this cache with its owner-local lock. Explicit clear
/// advances an epoch, preventing an older in-flight load from being republished.
#[derive(Debug)]
pub(crate) struct Cache<T> {
    epoch: u64, next_version: u64, entries: Vec<Arc<CatalogSnapshot<T>>>,
    rows: usize, bytes: usize,
}
impl<T> Default for Cache<T> {
    fn default() -> Self { Self { epoch: 0,next_version: 1,entries: Vec::new(),rows: 0,bytes: 0 } }
}
impl<T> Cache<T> {
    pub fn get(&mut self, key: &Key) -> Option<Arc<CatalogSnapshot<T>>> {
        self.retain_source(key);
        let index = self.entries.iter().position(|entry| entry.key == *key)?;
        let entry = self.entries.remove(index); self.entries.push(Arc::clone(&entry)); Some(entry)
    }
    pub fn begin_load(&mut self) -> Result<(u64,u64)> {
        let version = self.next_version;
        self.next_version = version.checked_add(1).ok_or_else(exhausted)?;
        Ok((self.epoch,version))
    }
    pub fn publish(&mut self, epoch: u64, snapshot: Arc<CatalogSnapshot<T>>) -> Result<()> {
        if epoch != self.epoch { return Ok(()); }
        self.retain_source(&snapshot.key);
        if let Some(index) = self.entries.iter().position(|entry| entry.key == snapshot.key) { self.remove(index); }
        if snapshot.rows.len() > CACHE_ROWS || snapshot.heap_bytes > CACHE_BYTES { return Ok(()); }
        while self.entries.len() >= CACHE_ENTRIES || self.rows > CACHE_ROWS-snapshot.rows.len()
            || self.bytes > CACHE_BYTES-snapshot.heap_bytes { self.remove(0); }
        self.entries.try_reserve(1).map_err(crate::services::allocation_error)?;
        self.rows += snapshot.rows.len(); self.bytes += snapshot.heap_bytes; self.entries.push(snapshot); Ok(())
    }
    pub fn clear(&mut self) -> Result<()> {
        self.epoch = self.epoch.checked_add(1).ok_or_else(exhausted)?;
        self.entries.clear(); self.rows = 0; self.bytes = 0; Ok(())
    }
    fn retain_source(&mut self, key: &Key) {
        let mut index = 0;
        while index < self.entries.len() {
            let source = &self.entries[index].key;
            if source.owner != key.owner || source.endpoint != key.endpoint || source.generation != key.generation
                || source.profile != key.profile || source.schema != key.schema { self.remove(index); }
            else { index += 1; }
        }
    }
    fn remove(&mut self, index: usize) {
        let removed = self.entries.remove(index); self.rows -= removed.rows.len(); self.bytes -= removed.heap_bytes;
    }
}
fn exhausted() -> Error { Error::new(ErrorKind::LimitExceeded,"directory cache generation exhausted") }

// Authored source contracts. Execution is deferred with the rest of T01-T30.
#[cfg(test)]
mod tests {
    use super::*;
    fn key(market: u8) -> Key {
        Key { owner: 17,endpoint: "127.0.0.1:7727".parse().unwrap(),generation: 3,
            profile: "synthetic-catalog",market: ExMarketId::from_wire(market),kind: CatalogKind::MacGoods,
            raw: RawPolicy::Omit,schema: CATALOG_SCHEMA_VERSION }
    }
    fn snapshot(key: Key, version: u64, bytes: usize) -> Arc<CatalogSnapshot<u8>> {
        Arc::new(CatalogSnapshot { key,version,rows: vec![7],meta: ResultMeta::default(),
            global_count: None,sorted_bounds: None,heap_bytes: bytes })
    }
    #[test]
    fn clear_rejects_late_publication_and_retained_handles_remain_readable() {
        let mut cache = Cache::default(); let (epoch,version) = cache.begin_load().unwrap();
        let old = snapshot(key(38),version,1); cache.clear().unwrap();
        cache.publish(epoch,Arc::clone(&old)).unwrap();
        assert!(cache.get(&key(38)).is_none()); assert_eq!(old.rows(),&[7]);
        let (_,next) = cache.begin_load().unwrap(); assert!(next>version);
    }
    #[test]
    fn entry_eviction_is_least_recently_used_and_raw_policy_is_part_of_identity() {
        let mut cache = Cache::default();
        for market in 0..16 { let (epoch,version) = cache.begin_load().unwrap(); cache.publish(epoch,snapshot(key(market),version,1)).unwrap(); }
        let old = cache.get(&key(0)).unwrap();
        let (epoch,version) = cache.begin_load().unwrap(); cache.publish(epoch,snapshot(key(16),version,1)).unwrap();
        assert!(cache.get(&key(1)).is_none()); assert_eq!(cache.get(&key(0)).unwrap().version(),old.version());
        let mut raw_key = key(0); raw_key.raw = RawPolicy::RetainWire; assert!(cache.get(&raw_key).is_none());
        assert!(cache.get(&key(0)).is_some());
    }
    #[test]
    fn source_generation_profile_owner_and_schema_changes_evict_eligibility() {
        for replacement in 0..4 {
            let mut cache = Cache::default(); let (epoch,version) = cache.begin_load().unwrap();
            let old = snapshot(key(38),version,1); cache.publish(epoch,Arc::clone(&old)).unwrap();
            let mut changed = key(38);
            match replacement { 0 => changed.generation += 1,1 => changed.profile = "other-profile",2 => changed.owner += 1,_ => changed.schema += 1 }
            assert!(cache.get(&changed).is_none()); assert!(cache.get(&key(38)).is_none()); assert_eq!(old.rows(),&[7]);
        }
    }
    #[test]
    fn byte_and_row_bounds_evict_before_retention_and_oversized_results_stay_uncached() {
        let mut cache = Cache::default(); let (epoch,version) = cache.begin_load().unwrap();
        cache.publish(epoch,snapshot(key(0),version,CACHE_BYTES)).unwrap();
        let (epoch,version) = cache.begin_load().unwrap(); cache.publish(epoch,snapshot(key(1),version,1)).unwrap();
        assert!(cache.get(&key(0)).is_none()); assert_eq!(cache.bytes,1);
        let oversized = snapshot(key(2),version,CACHE_BYTES+1); cache.publish(epoch,Arc::clone(&oversized)).unwrap();
        assert!(cache.get(&key(2)).is_none()); assert_eq!(oversized.rows(),&[7]);
        let rows = Arc::new(CatalogSnapshot { key: key(3),version,rows: vec![0;CACHE_ROWS+1],meta: ResultMeta::default(),
            global_count: None,sorted_bounds: None,heap_bytes: 1 });
        cache.publish(epoch,rows).unwrap(); assert!(cache.get(&key(3)).is_none());
    }
    #[test]
    fn cache_kinds_and_markets_do_not_collide() {
        let mut cache = Cache::default(); let (epoch,version) = cache.begin_load().unwrap();
        cache.publish(epoch,snapshot(key(38),version,1)).unwrap();
        assert!(cache.get(&key(39)).is_none()); let mut other = key(38); other.kind = CatalogKind::ExInstruments;
        assert!(cache.get(&other).is_none()); assert!(cache.get(&key(38)).is_some());
    }
    #[test]
    fn generation_exhaustion_never_wraps_or_mutates_existing_snapshot() {
        let mut cache = Cache::<u8>::default(); cache.next_version = u64::MAX;
        assert_eq!(cache.begin_load().unwrap_err().kind(),ErrorKind::LimitExceeded);
        cache.epoch = u64::MAX; assert_eq!(cache.clear().unwrap_err().kind(),ErrorKind::LimitExceeded);
    }
}
