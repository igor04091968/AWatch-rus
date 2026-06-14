//! Snapshot cache helpers for the portal request path.
//!
//! CONTRACT: this module only owns short-lived in-process cache behavior.
//! It must not change snapshot payloads, source collection, API routes or
//! business calculations.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Cli, HealthResponse, Snapshot, build_health, build_snapshot, now};

const SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(120);

pub(crate) type SnapshotCache = Arc<Mutex<Option<CachedSnapshot>>>;

#[derive(Clone, Debug)]
pub(crate) struct CachedSnapshot {
    created: Instant,
    snapshot: Snapshot,
}

pub(crate) fn new_snapshot_cache() -> SnapshotCache {
    Arc::new(Mutex::new(None))
}

pub(crate) fn clone_snapshot_cache(cache: &SnapshotCache) -> SnapshotCache {
    Arc::clone(cache)
}

pub(crate) fn cached_snapshot(args: &Cli, cache: &SnapshotCache) -> Snapshot {
    let mut guard = cache.lock().expect("snapshot cache mutex poisoned");
    if let Some(cached) = guard.as_ref() {
        if cached.created.elapsed() <= SNAPSHOT_CACHE_TTL {
            return cached.snapshot.clone();
        }
    }
    let snapshot = build_snapshot(args);
    *guard = Some(CachedSnapshot {
        created: Instant::now(),
        snapshot: snapshot.clone(),
    });
    snapshot
}

pub(crate) fn build_fast_health(cache: &SnapshotCache) -> HealthResponse {
    match cache.try_lock() {
        Ok(guard) => guard
            .as_ref()
            .map(|cached| build_health(&cached.snapshot))
            .unwrap_or_else(lightweight_health),
        Err(_) => lightweight_health(),
    }
}

fn lightweight_health() -> HealthResponse {
    let mut sources = BTreeMap::new();
    sources.insert("portal".to_string(), true);
    HealthResponse {
        ok: true,
        generated_at_utc: now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        sources,
    }
}
