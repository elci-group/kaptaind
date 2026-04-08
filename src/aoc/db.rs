use rusqlite::{params, Connection};
use std::path::Path;
use crate::aoc::tracer::TraceRecord;
use chrono::{DateTime, Utc};

pub fn init_db(repo_path: &Path) -> anyhow::Result<()> {
    let db_path = repo_path.join(".kaptaind").join("traces.db");
    let conn = Connection::open(db_path)?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS traces (
            cluster_id TEXT PRIMARY KEY,
            aoc_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            data JSON NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_traces_aoc_id ON traces (aoc_id)",
        [],
    )?;

    Ok(())
}

pub fn save_trace(repo_path: &Path, record: &TraceRecord) -> anyhow::Result<()> {
    let db_path = repo_path.join(".kaptaind").join("traces.db");
    let conn = Connection::open(db_path)?;
    
    let data = serde_json::to_string(record)?;
    
    conn.execute(
        "INSERT OR REPLACE INTO traces (cluster_id, aoc_id, started_at, ended_at, duration_ms, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.cluster_id,
            record.aoc_id,
            record.started_at.to_rfc3339(),
            record.ended_at.to_rfc3339(),
            record.duration_ms,
            data
        ],
    )?;

    Ok(())
}

pub fn get_traces_for_aoc(repo_path: &Path, aoc_id: &str) -> anyhow::Result<Vec<TraceRecord>> {
    let db_path = repo_path.join(".kaptaind").join("traces.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT data FROM traces WHERE aoc_id = ?1 ORDER BY started_at ASC")?;
    
    let trace_iter = stmt.query_map([aoc_id], |row| {
        let data: String = row.get(0)?;
        Ok(data)
    })?;

    let mut traces = Vec::new();
    for data_res in trace_iter {
        let data = data_res?;
        let record = serde_json::from_str::<TraceRecord>(&data)
            .map_err(|e| anyhow::anyhow!("failed to deserialize trace: {}", e))?;
        traces.push(record);
    }

    Ok(traces)
}

pub fn prune_old_traces(repo_path: &Path, days: i64) -> anyhow::Result<usize> {
    let db_path = repo_path.join(".kaptaind").join("traces.db");
    if !db_path.exists() {
        return Ok(0);
    }
    
    let conn = Connection::open(db_path)?;
    let cutoff = Utc::now() - chrono::Duration::days(days);
    let deleted = conn.execute(
        "DELETE FROM traces WHERE started_at < ?1",
        params![cutoff.to_rfc3339()],
    )?;

    Ok(deleted)
}
