//! The index (ADR-0021): every project and record, in SQLite, for listing
//! and searching without reading the folders. The folders are the truth: the
//! index can be rebuilt from them.
//!
//! It shares `index.sqlite` with the job service (ADR-0016), so its tables
//! are named `store_…`, and its schema's version is in `store_schema`, not in
//! `user_version`, which the job service counts with.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::records::RecordKind;
use crate::{Project, Query, Result, StoreError};

/// The schema, one step a version.
const MIGRATIONS: &[&str] = &["CREATE TABLE store_projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        sample TEXT NOT NULL,
        owner TEXT NOT NULL,
        tags TEXT NOT NULL,
        created TEXT NOT NULL,
        modified TEXT NOT NULL,
        document TEXT NOT NULL
    );
    CREATE TABLE store_records (
        id TEXT PRIMARY KEY,
        project TEXT NOT NULL,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        created TEXT NOT NULL,
        document TEXT NOT NULL
    );
    CREATE INDEX store_records_by_project ON store_records (project, kind, created);"];

/// The most projects a page holds.
pub(crate) const PAGE_MAX: usize = 500;

fn failed(error: rusqlite::Error) -> StoreError {
    StoreError::Index(error.to_string())
}

/// A project's tags as the index keeps them: each between newlines, so that
/// a tag is found whole.
fn tag_text(tags: &[String]) -> String {
    tags.iter()
        .fold("\n".to_owned(), |text, tag| text + tag + "\n")
}

/// A record, as the index keeps it.
pub(crate) struct RecordRow {
    pub kind: RecordKind,
    pub id: String,
    pub project: String,
    pub name: String,
    pub created: String,
    pub document: String,
}

fn insert_project(connection: &Connection, project: &Project, document: &str) -> Result<()> {
    connection
        .execute(
            "INSERT OR REPLACE INTO store_projects VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project.id.to_string(),
                project.name,
                project.sample,
                project.owner,
                tag_text(&project.tags),
                project.created.to_rfc3339(),
                project.modified.to_rfc3339(),
                document
            ],
        )
        .map(drop)
        .map_err(failed)
}

fn insert_record(connection: &Connection, row: &RecordRow) -> Result<()> {
    connection
        .execute(
            "INSERT OR REPLACE INTO store_records VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.id,
                row.project,
                row.kind.name(),
                row.name,
                row.created,
                row.document
            ],
        )
        .map(drop)
        .map_err(failed)
}

/// A project as a page lists it: its document, and how many acquisitions,
/// results and reports it has.
pub(crate) type Listed = (String, [usize; 3]);

pub(crate) struct Index(Connection);

impl Index {
    /// Opens the index at `path`, or in memory for `:memory:`, and brings its
    /// schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path).map_err(failed)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(failed)?;
        let migrate = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(failed)?;
        migrate
            .execute_batch("CREATE TABLE IF NOT EXISTS store_schema (version INTEGER NOT NULL)")
            .map_err(failed)?;
        let version: Option<i64> = migrate
            .query_row("SELECT version FROM store_schema", [], |row| row.get(0))
            .optional()
            .map_err(failed)?;
        let version = version.unwrap_or(0) as usize;
        if version > MIGRATIONS.len() {
            return Err(StoreError::Index(format!(
                "its schema is version {version}, newer than this OScope's {}",
                MIGRATIONS.len()
            )));
        }
        for step in &MIGRATIONS[version..] {
            migrate.execute_batch(step).map_err(failed)?;
        }
        migrate
            .execute("DELETE FROM store_schema", [])
            .map_err(failed)?;
        migrate
            .execute(
                "INSERT INTO store_schema (version) VALUES (?1)",
                [MIGRATIONS.len() as i64],
            )
            .map_err(failed)?;
        migrate.commit().map_err(failed)?;
        Ok(Self(connection))
    }

    /// Adds `project`, or replaces it.
    pub fn put_project(&self, project: &Project, document: &str) -> Result<()> {
        insert_project(&self.0, project, document)
    }

    /// Removes a project and its records.
    pub fn remove_project(&mut self, id: &str) -> Result<()> {
        let remove = self.0.transaction().map_err(failed)?;
        remove
            .execute("DELETE FROM store_records WHERE project = ?1", [id])
            .map_err(failed)?;
        remove
            .execute("DELETE FROM store_projects WHERE id = ?1", [id])
            .map_err(failed)?;
        remove.commit().map_err(failed)
    }

    /// Adds a record, or replaces it.
    pub fn put_record(&self, row: &RecordRow) -> Result<()> {
        insert_record(&self.0, row)
    }

    /// The projects `query` asks for, the most recently changed first, and
    /// how many there are in all.
    pub fn page(&self, query: &Query) -> Result<(usize, Vec<Listed>)> {
        let text = query.text.as_deref().unwrap_or_default().trim();
        let escaped: String = text
            .chars()
            .flat_map(|c| match c {
                '%' | '_' | '\\' => vec!['\\', c],
                _ => vec![c],
            })
            .collect();
        let pattern = format!("%{escaped}%");
        let tag = query.tag.as_deref().map(|tag| format!("\n{tag}\n"));
        let filter =
            "(name LIKE ?1 ESCAPE '\\' OR sample LIKE ?1 ESCAPE '\\' OR tags LIKE ?1 ESCAPE '\\')
            AND (?2 IS NULL OR instr(tags, ?2) > 0)";
        let total: i64 = self
            .0
            .query_row(
                &format!("SELECT COUNT(*) FROM store_projects WHERE {filter}"),
                params![pattern, tag],
                |row| row.get(0),
            )
            .map_err(failed)?;
        let count = |kind: RecordKind| {
            format!(
                "(SELECT COUNT(*) FROM store_records r WHERE r.project = p.id AND r.kind = '{}')",
                kind.name()
            )
        };
        let [a, r, d] = RecordKind::ALL.map(count);
        let sql = format!(
            "SELECT document, {a}, {r}, {d} FROM store_projects p WHERE {filter}
             ORDER BY modified DESC, id DESC LIMIT ?3 OFFSET ?4"
        );
        let limit = query.limit.unwrap_or(50).min(PAGE_MAX) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        let mut statement = self.0.prepare(&sql).map_err(failed)?;
        let rows = statement
            .query_map(params![pattern, tag, limit, offset], |row| {
                let count = |i| row.get::<_, i64>(i).map(|n| n as usize);
                Ok((row.get(0)?, [count(1)?, count(2)?, count(3)?]))
            })
            .map_err(failed)?;
        let listed = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(failed)?;
        Ok((total as usize, listed))
    }

    /// The documents of a project's records of `kind`, the oldest first.
    pub fn records(&self, project: &str, kind: RecordKind) -> Result<Vec<String>> {
        let mut statement = self
            .0
            .prepare(
                "SELECT document FROM store_records WHERE project = ?1 AND kind = ?2
                 ORDER BY created, id",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map(params![project, kind.name()], |row| row.get(0))
            .map_err(failed)?;
        rows.collect::<rusqlite::Result<Vec<String>>>()
            .map_err(failed)
    }

    /// Replaces everything with `projects` and `records`, at once.
    pub fn replace(&mut self, projects: &[(Project, String)], records: &[RecordRow]) -> Result<()> {
        let replace = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(failed)?;
        replace
            .execute_batch("DELETE FROM store_records; DELETE FROM store_projects;")
            .map_err(failed)?;
        for (project, document) in projects {
            insert_project(&replace, project, document)?;
        }
        for row in records {
            insert_record(&replace, row)?;
        }
        replace.commit().map_err(failed)
    }

    /// Every row, as text, sorted.
    pub fn rows(&self) -> Result<Vec<String>> {
        let mut rows = Vec::new();
        for table in ["store_projects", "store_records"] {
            let mut statement = self
                .0
                .prepare(&format!("SELECT * FROM {table}"))
                .map_err(failed)?;
            let columns = statement.column_count();
            let found = statement
                .query_map([], |row| {
                    let cells = (0..columns)
                        .map(|i| row.get::<_, String>(i))
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    Ok(format!("{table}: {}", cells.join(" | ")))
                })
                .map_err(failed)?;
            rows.extend(
                found
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(failed)?,
            );
        }
        rows.sort();
        Ok(rows)
    }
}
