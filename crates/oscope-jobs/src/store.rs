//! The job history, in SQLite (ADR-0016): one row a job, its latest state as
//! JSON beside the columns lists filter and sort by.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Utc};
use oscope_core::JobId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{JobError, JobFilter, JobInfo, JobState};

/// The schema, one step a version: `PRAGMA user_version` says how many are
/// applied. A step is never changed once released; a new one is added.
const MIGRATIONS: &[&str] = &["CREATE TABLE jobs (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        state TEXT NOT NULL,
        submitted INTEGER NOT NULL,
        info TEXT NOT NULL
    );
    CREATE INDEX jobs_by_time ON jobs (submitted);"];

/// The most a list returns.
const MOST: usize = 1000;

fn failed(error: impl std::fmt::Display) -> JobError {
    JobError::Store(error.to_string())
}

/// The job history.
pub struct JobStore {
    connection: Mutex<Connection>,
}

impl JobStore {
    /// The history in the SQLite file at `path`, made or brought up to the
    /// newest schema.
    pub fn open(path: &Path) -> Result<JobStore, JobError> {
        let mut connection = Connection::open(path).map_err(failed)?;
        // Write-ahead logging: a commit doesn't wait for the disk, and
        // readers don't wait for writers.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(failed)?;
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(failed)?;
        let applied: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(failed)?;
        for (step, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            let transaction = connection.transaction().map_err(failed)?;
            transaction.execute_batch(migration).map_err(failed)?;
            transaction
                .pragma_update(None, "user_version", step as i64 + 1)
                .map_err(failed)?;
            transaction.commit().map_err(failed)?;
        }
        Ok(JobStore {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `job` as it is now.
    pub fn save(&self, job: &JobInfo) -> Result<(), JobError> {
        let info = serde_json::to_string(job).map_err(failed)?;
        self.connection()
            .execute(
                "INSERT INTO jobs (id, kind, state, submitted, info) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (id) DO UPDATE SET state = excluded.state, info = excluded.info",
                params![
                    job.id.to_string(),
                    job.spec.kind,
                    job.state.name(),
                    job.submitted.timestamp_micros(),
                    info
                ],
            )
            .map(drop)
            .map_err(failed)
    }

    /// The job with this ID, as last recorded.
    pub fn get(&self, id: JobId) -> Result<Option<JobInfo>, JobError> {
        let info: Option<String> = self
            .connection()
            .query_row(
                "SELECT info FROM jobs WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(failed)?;
        info.map(|info| serde_json::from_str(&info).map_err(failed))
            .transpose()
    }

    /// The jobs `filter` lets through, the newest first.
    pub fn list(&self, filter: &JobFilter) -> Result<Vec<JobInfo>, JobError> {
        let limit = filter.limit.unwrap_or(50).min(MOST);
        let connection = self.connection();
        let mut query = connection
            .prepare(
                "SELECT info FROM jobs
                 WHERE (?1 IS NULL OR state = ?1) AND (?2 IS NULL OR kind = ?2)
                 ORDER BY submitted DESC, id DESC LIMIT ?3",
            )
            .map_err(failed)?;
        let state = filter.state.map(JobState::name);
        let rows = query
            .query_map(params![state, filter.kind, limit as i64], |row| {
                row.get::<_, String>(0)
            })
            .map_err(failed)?;
        rows.map(|info| serde_json::from_str(&info.map_err(failed)?).map_err(failed))
            .collect()
    }

    /// Marks every job that hadn't ended as interrupted, at `now`: those a
    /// stop cut short. Them, as they are now.
    pub fn interrupt(&self, now: DateTime<Utc>) -> Result<Vec<JobInfo>, JobError> {
        let unfinished = [JobState::Queued, JobState::Running, JobState::Paused];
        let mut cut = Vec::new();
        for state in unfinished {
            let filter = JobFilter {
                state: Some(state),
                limit: Some(MOST),
                ..JobFilter::default()
            };
            cut.extend(self.list(&filter)?);
        }
        for job in &mut cut {
            job.error = Some(format!(
                "the server stopped while it was {}",
                job.state.name()
            ));
            job.state = JobState::Interrupted;
            job.finished = Some(now);
            job.position = None;
            self.save(job)?;
        }
        Ok(cut)
    }
}
