use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use rusqlite::{Connection, DatabaseName, OptionalExtension, Row, params};
use serde_json::json;

#[derive(Debug, Parser)]
#[command(about = "Merge ActivityWatch aw-server-rust SQLite databases")]
struct Cli {
    #[arg(long)]
    base: PathBuf,

    #[arg(long)]
    output: PathBuf,

    #[arg(long)]
    overlay: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BucketKey {
    name: String,
    type_: String,
    client: String,
    hostname: String,
}

#[derive(Debug, Clone)]
struct Bucket {
    rowid: i64,
    id: Option<String>,
    key: BucketKey,
    created: i64,
    data_deprecated: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EventKey {
    starttime: i64,
    endtime: i64,
    data: String,
}

#[derive(Debug, Default)]
struct MergeStats {
    inserted_buckets: usize,
    inserted_events: usize,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if !cli.base.is_file() {
        bail!("Base DB not found: {}", cli.base.display());
    }
    ensure_parent(&cli.output)?;
    let tmp_output = tmp_output_path(&cli.output);
    if tmp_output.exists() {
        fs::remove_file(&tmp_output)
            .with_context(|| format!("remove stale tmp output {}", tmp_output.display()))?;
    }
    copy_sqlite_via_backup(&cli.base, &tmp_output)?;

    let mut stats = MergeStats::default();
    if let Some(overlay) = &cli.overlay {
        if overlay.exists() {
            stats = merge_overlay(&tmp_output, overlay)?;
        }
    }

    fs::rename(&tmp_output, &cli.output)
        .with_context(|| format!("move {} to {}", tmp_output.display(), cli.output.display()))?;
    println!(
        "{}",
        json!({
            "base": cli.base.to_string_lossy(),
            "overlay": cli.overlay.as_ref().map(|p| p.to_string_lossy().to_string()),
            "output": cli.output.to_string_lossy(),
            "inserted_buckets": stats.inserted_buckets,
            "inserted_events": stats.inserted_events,
        })
    );
    Ok(())
}

fn copy_sqlite_via_backup(src: &Path, dst: &Path) -> Result<()> {
    ensure_parent(dst)?;
    let source =
        Connection::open(src).with_context(|| format!("open base DB {}", src.display()))?;
    source
        .backup(DatabaseName::Main, dst, None)
        .with_context(|| format!("backup {} to {}", src.display(), dst.display()))
}

fn merge_overlay(dest_path: &Path, overlay_path: &Path) -> Result<MergeStats> {
    let dest =
        connect(dest_path).with_context(|| format!("open output DB {}", dest_path.display()))?;
    let source = connect(overlay_path)
        .with_context(|| format!("open overlay DB {}", overlay_path.display()))?;

    let mut stats = MergeStats::default();
    let source_buckets = load_buckets(&source)?;
    let mut dest_bucket_map = load_dest_bucket_key_map(&dest)?;
    let mut dest_id_map = load_dest_bucket_id_map(&dest)?;

    for src_bucket in source_buckets {
        let dest_rowid = if let Some(src_id) = &src_bucket.id {
            dest_id_map.get(src_id).copied()
        } else {
            None
        }
        .or_else(|| dest_bucket_map.get(&src_bucket.key).copied())
        .or_else(|| {
            find_bucket_by_name(&dest, &src_bucket.key.name)
                .ok()
                .flatten()
        });

        let dest_rowid = match dest_rowid {
            Some(rowid) => {
                update_bucket_by_rowid(&dest, rowid, &src_bucket)?;
                rowid
            }
            None => {
                stats.inserted_buckets += 1;
                insert_bucket(&dest, &src_bucket)?
            }
        };

        dest_bucket_map.insert(src_bucket.key.clone(), dest_rowid);
        if let Some(id) = &src_bucket.id {
            dest_id_map.insert(id.clone(), dest_rowid);
        }

        let mut existing_events = load_existing_events(&dest, dest_rowid)?;
        let source_events = load_source_events(&source, src_bucket.rowid)?;
        for event in source_events {
            if existing_events.contains(&event) {
                continue;
            }
            dest.execute(
                "insert into events (bucketrow, starttime, endtime, data) values (?1, ?2, ?3, ?4)",
                params![dest_rowid, event.starttime, event.endtime, event.data],
            )
            .with_context(|| format!("insert event into bucketrow {dest_rowid}"))?;
            existing_events.insert(event);
            stats.inserted_events += 1;
        }
    }
    dest.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("checkpoint output DB")?;
    Ok(stats)
}

fn connect(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    Ok(connection)
}

fn load_buckets(connection: &Connection) -> Result<Vec<Bucket>> {
    let mut stmt = connection.prepare(
        "select rowid as bucketrow, id, name, type, client, hostname, created, data_deprecated, data from buckets order by rowid",
    )?;
    let rows = stmt.query_map([], bucket_from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("load buckets")
}

fn bucket_from_row(row: &Row<'_>) -> rusqlite::Result<Bucket> {
    Ok(Bucket {
        rowid: row.get("bucketrow")?,
        id: row.get("id")?,
        key: BucketKey {
            name: row.get("name")?,
            type_: row.get("type")?,
            client: row.get("client")?,
            hostname: row.get("hostname")?,
        },
        created: row.get("created")?,
        data_deprecated: row.get("data_deprecated")?,
        data: row.get("data")?,
    })
}

fn load_dest_bucket_key_map(connection: &Connection) -> Result<HashMap<BucketKey, i64>> {
    Ok(load_buckets(connection)?
        .into_iter()
        .map(|bucket| (bucket.key, bucket.rowid))
        .collect())
}

fn load_dest_bucket_id_map(connection: &Connection) -> Result<HashMap<String, i64>> {
    Ok(load_buckets(connection)?
        .into_iter()
        .filter_map(|bucket| bucket.id.map(|id| (id, bucket.rowid)))
        .collect())
}

fn find_bucket_by_name(connection: &Connection, name: &str) -> Result<Option<i64>> {
    connection
        .query_row(
            "select rowid as bucketrow from buckets where name = ?1 order by rowid limit 1",
            [name],
            |row| row.get::<_, i64>("bucketrow"),
        )
        .optional()
        .context("find bucket by name")
}

fn insert_bucket(connection: &Connection, bucket: &Bucket) -> Result<i64> {
    connection.execute(
        "insert into buckets (id, name, type, client, hostname, created, data_deprecated, data) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            bucket.id,
            bucket.key.name,
            bucket.key.type_,
            bucket.key.client,
            bucket.key.hostname,
            bucket.created,
            bucket.data_deprecated,
            bucket.data
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

fn update_bucket_by_rowid(connection: &Connection, rowid: i64, bucket: &Bucket) -> Result<()> {
    connection.execute(
        "update buckets set type = ?1, client = ?2, hostname = ?3, created = ?4, data_deprecated = ?5, data = ?6 where rowid = ?7",
        params![
            bucket.key.type_,
            bucket.key.client,
            bucket.key.hostname,
            bucket.created,
            bucket.data_deprecated,
            bucket.data,
            rowid
        ],
    )?;
    Ok(())
}

fn load_existing_events(connection: &Connection, bucketrow: i64) -> Result<HashSet<EventKey>> {
    let mut stmt =
        connection.prepare("select starttime, endtime, data from events where bucketrow = ?1")?;
    let rows = stmt.query_map([bucketrow], |row| {
        Ok(EventKey {
            starttime: row.get(0)?,
            endtime: row.get(1)?,
            data: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<HashSet<_>>>()?)
}

fn load_source_events(connection: &Connection, bucketrow: i64) -> Result<Vec<EventKey>> {
    let mut stmt = connection
        .prepare("select starttime, endtime, data from events where bucketrow = ?1 order by id")?;
    let rows = stmt.query_map([bucketrow], |row| {
        Ok(EventKey {
            starttime: row.get(0)?,
            endtime: row.get(1)?,
            data: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory {}", parent.display()))?;
    }
    Ok(())
}

fn tmp_output_path(output: &Path) -> PathBuf {
    let mut os = output.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_output_appends_tmp_like_python_suffix() {
        assert_eq!(
            tmp_output_path(Path::new("/tmp/sqlite.db")),
            PathBuf::from("/tmp/sqlite.db.tmp")
        );
    }

    #[test]
    fn merge_inserts_new_bucket_and_skips_duplicate_event() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.db");
        let overlay = dir.path().join("overlay.db");
        let output = dir.path().join("out.db");
        create_fixture_db(&base, &[("bucket-a", "id-a")], &[(1, 10, 20, r#"{"a":1}"#)]);
        create_fixture_db(
            &overlay,
            &[("bucket-a", "id-a"), ("bucket-b", "id-b")],
            &[(1, 10, 20, r#"{"a":1}"#), (2, 30, 40, r#"{"b":1}"#)],
        );

        copy_sqlite_via_backup(&base, &output).unwrap();
        let stats = merge_overlay(&output, &overlay).unwrap();
        assert_eq!(stats.inserted_buckets, 1);
        assert_eq!(stats.inserted_events, 1);

        let conn = Connection::open(&output).unwrap();
        let bucket_count: i64 = conn
            .query_row("select count(*) from buckets", [], |row| row.get(0))
            .unwrap();
        let event_count: i64 = conn
            .query_row("select count(*) from events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bucket_count, 2);
        assert_eq!(event_count, 2);
    }

    fn create_fixture_db(path: &Path, buckets: &[(&str, &str)], events: &[(i64, i64, i64, &str)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "create table buckets (
                id text,
                name text unique,
                type text,
                client text,
                hostname text,
                created integer,
                data_deprecated text,
                data text
            );
            create table events (
                id integer primary key autoincrement,
                bucketrow integer,
                starttime integer,
                endtime integer,
                data text
            );",
        )
        .unwrap();
        for (name, id) in buckets {
            conn.execute(
                "insert into buckets (id, name, type, client, hostname, created, data_deprecated, data) values (?1, ?2, 'type', 'client', 'host', 1, null, '{}')",
                params![id, name],
            )
            .unwrap();
        }
        for (bucketrow, starttime, endtime, data) in events {
            conn.execute(
                "insert into events (bucketrow, starttime, endtime, data) values (?1, ?2, ?3, ?4)",
                params![bucketrow, starttime, endtime, data],
            )
            .unwrap();
        }
    }
}
