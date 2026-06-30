//! Snapshot cache helpers for the portal request path.
//!
//! CONTRACT: this module only owns short-lived in-process cache behavior.
//! It must not change snapshot payloads, source collection, API routes or
//! business calculations.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{Cli, HealthResponse, Snapshot, build_health, build_snapshot, now};

const SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(120);

pub(crate) type SnapshotCache = Arc<Mutex<SnapshotCacheState>>;

#[derive(Clone, Debug, Default)]
pub(crate) struct SnapshotCacheState {
    pub(crate) entry: Option<CachedSnapshot>,
    pub(crate) refresh_in_progress: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedSnapshot {
    created: Instant,
    snapshot: Snapshot,
}

pub(crate) fn new_snapshot_cache() -> SnapshotCache {
    Arc::new(Mutex::new(SnapshotCacheState::default()))
}

pub(crate) fn clone_snapshot_cache(cache: &SnapshotCache) -> SnapshotCache {
    Arc::clone(cache)
}

pub(crate) fn cached_snapshot(args: &Cli, cache: &SnapshotCache) -> Snapshot {
    {
        let guard = cache.lock().expect("snapshot cache mutex poisoned");
        if let Some(cached) = guard.entry.as_ref() {
            if cached.created.elapsed() <= SNAPSHOT_CACHE_TTL {
                return cached.snapshot.clone();
            }
        }
    }

    let snapshot = build_snapshot(args);
    let mut guard = cache.lock().expect("snapshot cache mutex poisoned");
    guard.entry = Some(CachedSnapshot {
        created: Instant::now(),
        snapshot: snapshot.clone(),
    });
    guard.refresh_in_progress = false;
    snapshot
}

pub(crate) fn cached_snapshot_or_refresh(args: &Cli, cache: &SnapshotCache) -> Option<Snapshot> {
    let mut should_spawn = false;
    let mut snapshot_to_return = None;
    {
        let mut guard = cache.lock().expect("snapshot cache mutex poisoned");
        if let Some(cached) = guard.entry.as_ref() {
            let snapshot = cached.snapshot.clone();
            if cached.created.elapsed() <= SNAPSHOT_CACHE_TTL {
                return Some(snapshot);
            }
            if !guard.refresh_in_progress {
                guard.refresh_in_progress = true;
                should_spawn = true;
            }
            snapshot_to_return = Some(snapshot);
        } else if !guard.refresh_in_progress {
            guard.refresh_in_progress = true;
            should_spawn = true;
        }
    }

    if should_spawn {
        spawn_snapshot_refresh(args.clone(), clone_snapshot_cache(cache));
    }
    snapshot_to_return
}

fn spawn_snapshot_refresh(args: Cli, cache: SnapshotCache) {
    thread::spawn(move || {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build_snapshot(&args)));
        let mut guard = cache.lock().expect("snapshot cache mutex poisoned");
        match result {
            Ok(snapshot) => {
                guard.entry = Some(CachedSnapshot {
                    created: Instant::now(),
                    snapshot,
                });
            }
            Err(_) => {
                eprintln!("detmir-portal snapshot cache refresh panicked");
            }
        }
        guard.refresh_in_progress = false;
    });
}

pub(crate) fn build_fast_health(cache: &SnapshotCache) -> HealthResponse {
    match cache.try_lock() {
        Ok(guard) => guard
            .entry
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
