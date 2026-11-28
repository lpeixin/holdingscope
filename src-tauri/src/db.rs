use std::{collections::BTreeMap, path::Path, sync::Mutex, time::Duration};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    model::{ActivityBatch, Catalog, HistoryPoint, Issue, Manager, Portfolio, Snapshot},
    source::Document,
};

pub struct Database(Mutex<Connection>);

pub struct IndexedManager {
    pub manager: Manager,
    pub rss_timestamp: Option<String>,
}

pub struct CachedState {
    pub revision: String,
    pub attempted_at: String,
    pub warnings: Vec<Issue>,
}

pub struct DetailUpdate {
    pub snapshot: Snapshot,
    pub activities: Option<ActivityBatch>,
    pub history: Option<Vec<HistoryPoint>>,
    pub warnings: Vec<Issue>,
    pub fetched_at: String,
}

type Result<T> = std::result::Result<T, String>;

const MANAGER_COLUMNS: &str = "m.id, m.name, m.updated_on, m.period, m.value_usd, m.holding_count,
    EXISTS(SELECT 1 FROM snapshots s WHERE s.manager_id = m.id)";

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).map_err(err)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(err)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS managers (
                    id TEXT PRIMARY KEY COLLATE NOCASE,
                    name TEXT NOT NULL,
                    updated_on TEXT,
                    period TEXT,
                    value_usd REAL,
                    holding_count INTEGER,
                    rss_timestamp TEXT,
                    active INTEGER NOT NULL DEFAULT 1
                );
                CREATE TABLE IF NOT EXISTS snapshots (
                    manager_id TEXT COLLATE NOCASE REFERENCES managers(id),
                    period TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    PRIMARY KEY (manager_id, period)
                );
                CREATE TABLE IF NOT EXISTS details (
                    manager_id TEXT PRIMARY KEY COLLATE NOCASE REFERENCES managers(id),
                    revision TEXT NOT NULL,
                    attempted_at TEXT NOT NULL,
                    activities TEXT,
                    history TEXT,
                    warnings TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS documents (
                    url TEXT PRIMARY KEY,
                    body BLOB NOT NULL,
                    etag TEXT,
                    modified TEXT
                );
                CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .map_err(err)?;
        Ok(Self(Mutex::new(connection)))
    }

    pub fn document(&self, url: &str) -> Result<Option<Document>> {
        self.0
            .lock()
            .map_err(err)?
            .query_row(
                "SELECT body, etag, modified FROM documents WHERE url = ?1",
                [url],
                |row| {
                    Ok(Document {
                        bytes: row.get(0)?,
                        etag: row.get(1)?,
                        modified: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(err)
    }

    pub fn save_catalog(
        &self,
        managers: Vec<IndexedManager>,
        authoritative_directory: bool,
        documents: Vec<(&str, Document)>,
        checked_at: &str,
    ) -> Result<()> {
        let mut connection = self.0.lock().map_err(err)?;
        let transaction = connection.transaction().map_err(err)?;
        if authoritative_directory {
            transaction
                .execute("UPDATE managers SET active = 0 WHERE active = 1", [])
                .map_err(err)?;
        }
        for IndexedManager {
            manager,
            rss_timestamp,
        } in managers
        {
            transaction.execute(
                "INSERT INTO managers (id, name, updated_on, period, value_usd, holding_count, rss_timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, active = 1,
                 updated_on = COALESCE(MAX(managers.updated_on, excluded.updated_on), excluded.updated_on, managers.updated_on),
                 period = COALESCE(MAX(managers.period, excluded.period), excluded.period, managers.period),
                 value_usd = COALESCE(excluded.value_usd, managers.value_usd),
                 holding_count = COALESCE(excluded.holding_count, managers.holding_count),
                 rss_timestamp = COALESCE(MAX(managers.rss_timestamp, excluded.rss_timestamp), excluded.rss_timestamp, managers.rss_timestamp)",
                params![
                    manager.id, manager.name, manager.updated_on, manager.period, manager.value_usd,
                    manager.holding_count, rss_timestamp
                ],
            ).map_err(err)?;
        }
        for (url, document) in documents {
            transaction.execute(
                "INSERT OR REPLACE INTO documents (url, body, etag, modified) VALUES (?1, ?2, ?3, ?4)",
                params![url, document.bytes, document.etag, document.modified],
            ).map_err(err)?;
        }
        transaction
            .execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('checked_at', ?1)",
                [checked_at],
            )
            .map_err(err)?;
        transaction.commit().map_err(err)
    }

    pub fn catalog(&self) -> Result<Catalog> {
        let connection = self.0.lock().map_err(err)?;
        let managers = {
            let mut statement = connection
                .prepare(&format!(
                    "SELECT {MANAGER_COLUMNS} FROM managers m WHERE m.active = 1
                 ORDER BY m.updated_on DESC NULLS LAST, m.name COLLATE NOCASE"
                ))
                .map_err(err)?;
            let rows = statement.query_map([], manager_row).map_err(err)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)?
        };
        let checked_at = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'checked_at'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(err)?;
        Ok(Catalog {
            managers,
            checked_at,
        })
    }

    pub fn manager_revision(&self, id: &str) -> Result<Option<(Manager, String)>> {
        self.0.lock().map_err(err)?.query_row(
            &format!("SELECT {MANAGER_COLUMNS}, m.rss_timestamp FROM managers m WHERE m.id = ?1 AND active = 1"),
            [id],
            |row| {
                let manager = manager_row(row)?;
                let timestamp: Option<String> = row.get(7)?;
                let revision = format!("{}|{}|{}",
                    manager.period.as_deref().unwrap_or_default(),
                    manager.updated_on.as_deref().unwrap_or_default(),
                    timestamp.as_deref().unwrap_or_default(),
                );
                Ok((manager, revision))
            },
        ).optional().map_err(err)
    }

    pub fn cache_state(&self, id: &str) -> Result<Option<CachedState>> {
        let row: Option<(String, String, String)> = self
            .0
            .lock()
            .map_err(err)?
            .query_row(
                "SELECT revision, attempted_at, warnings FROM details WHERE manager_id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(err)?;
        row.map(|(revision, attempted_at, warnings)| {
            Ok(CachedState {
                revision,
                attempted_at,
                warnings: decode_warnings(&warnings)?,
            })
        })
        .transpose()
    }

    pub fn changed_cached_managers(&self) -> Result<Vec<String>> {
        let connection = self.0.lock().map_err(err)?;
        let mut statement = connection.prepare(
            "SELECT m.id FROM managers m JOIN details d ON d.manager_id = m.id
             WHERE m.active = 1 AND d.revision !=
                (COALESCE(m.period, '') || '|' || COALESCE(m.updated_on, '') || '|' || COALESCE(m.rss_timestamp, ''))
             ORDER BY m.updated_on DESC NULLS LAST"
        ).map_err(err)?;
        let rows = statement.query_map([], |row| row.get(0)).map_err(err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)
    }

    pub fn portfolio(&self, id: &str) -> Result<Option<Portfolio>> {
        let connection = self.0.lock().map_err(err)?;
        let Some(manager) = connection
            .query_row(
                &format!("SELECT {MANAGER_COLUMNS} FROM managers m WHERE m.id = ?1"),
                [id],
                manager_row,
            )
            .optional()
            .map_err(err)?
        else {
            return Ok(None);
        };
        let Some((synced_at, activities, history, warnings)) = connection.query_row(
            "SELECT attempted_at, activities, history, warnings FROM details WHERE manager_id = ?1",
            [id],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?, row.get::<_, String>(3)?,
            )),
        ).optional().map_err(err)? else { return Ok(None) };
        let mut statement = connection
            .prepare("SELECT payload FROM snapshots WHERE manager_id = ?1 ORDER BY period DESC")
            .map_err(err)?;
        let rows = statement
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(err)?;
        let snapshots = rows
            .map(|row| decode(&row.map_err(err)?))
            .collect::<Result<Vec<_>>>()?;
        let batch: Option<ActivityBatch> = activities.as_deref().map(decode).transpose()?;
        Ok(Some(Portfolio {
            manager,
            snapshots,
            synced_at,
            activity_through: batch.as_ref().map(|batch| batch.through.clone()),
            activity_partial_period: batch
                .as_ref()
                .and_then(|batch| batch.partial_period.clone()),
            activity_has_more: batch
                .as_ref()
                .is_some_and(|batch| batch.next_page.is_some()),
            history_available: history.is_some(),
            activities: batch.map(|batch| batch.changes).unwrap_or_default(),
            history: history
                .as_deref()
                .map(decode)
                .transpose()?
                .unwrap_or_default(),
            warnings: decode_warnings(&warnings)?,
        }))
    }

    pub fn activity_batch(&self, id: &str) -> Result<Option<ActivityBatch>> {
        let value: Option<Option<String>> = self
            .0
            .lock()
            .map_err(err)?
            .query_row(
                "SELECT activities FROM details WHERE manager_id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(err)?;
        value.flatten().as_deref().map(decode).transpose()
    }

    pub fn save_activity_batch(&self, id: &str, batch: ActivityBatch) -> Result<()> {
        self.0
            .lock()
            .map_err(err)?
            .execute(
                "UPDATE details SET activities = ?2 WHERE manager_id = ?1",
                params![id, encode(&batch)?],
            )
            .map_err(err)?;
        Ok(())
    }

    pub fn save_detail(&self, id: &str, revision: &str, update: DetailUpdate) -> Result<()> {
        let mut connection = self.0.lock().map_err(err)?;
        let transaction = connection.transaction().map_err(err)?;
        transaction.execute(
            "INSERT OR REPLACE INTO snapshots (manager_id, period, payload) VALUES (?1, ?2, ?3)",
            params![id, update.snapshot.period, encode(&update.snapshot)?],
        ).map_err(err)?;

        let (old_activities, old_history, old_revision) = transaction
            .query_row(
                "SELECT activities, history, revision FROM details WHERE manager_id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(err)?
            .unwrap_or_default();
        let activities = match update.activities {
            Some(mut batch) => {
                let old: Option<ActivityBatch> =
                    old_activities.as_deref().map(decode).transpose()?;
                if let Some(old) = old {
                    let cutoff = batch
                        .changes
                        .last()
                        .map(|row| row.period.clone())
                        .unwrap_or_else(|| batch.through.clone());
                    let preserve_cursor = old_revision == revision
                        && batch
                            .next_page
                            .is_some_and(|next| old.next_page.is_none_or(|old| old > next));
                    if preserve_cursor {
                        batch.changes.retain(|row| row.period > cutoff);
                        batch
                            .changes
                            .extend(old.changes.into_iter().filter(|row| row.period <= cutoff));
                        batch.next_page = old.next_page;
                        batch.partial_period = old.partial_period;
                    } else {
                        batch
                            .changes
                            .extend(old.changes.into_iter().filter(|row| row.period < cutoff));
                    }
                }
                Some(encode(&batch)?)
            }
            None => old_activities,
        };
        let history = match update.history {
            Some(points) => {
                let old: Vec<HistoryPoint> = old_history
                    .as_deref()
                    .map(decode)
                    .transpose()?
                    .unwrap_or_default();
                let by_period: BTreeMap<_, _> = old
                    .into_iter()
                    .chain(points)
                    .map(|point| (point.period.clone(), point))
                    .collect();
                Some(encode(&by_period.into_values().collect::<Vec<_>>())?)
            }
            None => old_history,
        };
        transaction.execute(
            "INSERT OR REPLACE INTO details (manager_id, revision, attempted_at, activities, history, warnings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, revision, update.fetched_at, activities, history, encode(&update.warnings)?],
        ).map_err(err)?;
        transaction.commit().map_err(err)
    }
}

fn manager_row(row: &Row<'_>) -> rusqlite::Result<Manager> {
    Ok(Manager {
        id: row.get(0)?,
        name: row.get(1)?,
        updated_on: row.get(2)?,
        period: row.get(3)?,
        value_usd: row.get(4)?,
        holding_count: row.get(5)?,
        cached: row.get(6)?,
    })
}

fn encode(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(err)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(err)
}

fn decode_warnings(value: &str) -> Result<Vec<Issue>> {
    let labels: Vec<serde_json::Value> = decode(value)?;
    let mut warnings = Vec::new();
    for label in labels {
        let issue = serde_json::from_value(label).unwrap_or(Issue::PreviousSync);
        if !warnings.contains(&issue) {
            warnings.push(issue);
        }
    }
    Ok(warnings)
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_warnings_written_before_language_support() {
        let cached = r#"["季度变动：HTTP 503","组合历史：HTTP 503","history"]"#;
        assert_eq!(
            decode_warnings(cached).unwrap(),
            vec![Issue::PreviousSync, Issue::History]
        );
        assert_eq!(
            encode(&[Issue::HoldingsPending]).unwrap(),
            r#"["holdingsPending"]"#
        );
    }

    #[test]
    fn preserves_quarters_and_supplements_when_a_source_fails() {
        let db = Database::open(Path::new(":memory:")).unwrap();
        let manager = Manager {
            id: "BRK".into(),
            name: "Example".into(),
            updated_on: Some("2026-08-14".into()),
            period: Some("2026 Q2".into()),
            value_usd: Some(120.0),
            holding_count: Some(0),
            cached: false,
        };
        db.save_catalog(
            vec![IndexedManager {
                manager: manager.clone(),
                rss_timestamp: Some("2026-08-14T08:20:00+00:00".into()),
            }],
            true,
            vec![],
            "2026-08-14T10:00:00+00:00",
        )
        .unwrap();
        let initial_revision = db.manager_revision("brk").unwrap().unwrap().1;
        let snapshot = Snapshot {
            period: "2026 Q2".into(),
            report_date: Some("2026-06-30".into()),
            value_usd: 120.0,
            holdings: vec![],
            sectors: vec![],
        };
        db.save_detail(
            "brk",
            &initial_revision,
            DetailUpdate {
                snapshot: snapshot.clone(),
                activities: Some(ActivityBatch {
                    changes: vec![],
                    through: "2026 Q2".into(),
                    partial_period: None,
                    next_page: None,
                }),
                history: Some(vec![HistoryPoint {
                    period: "2026 Q1".into(),
                    value_usd: Some(100.0),
                    leaders: vec![],
                }]),
                warnings: vec![],
                fetched_at: "2026-08-14T10:00:00+00:00".into(),
            },
        )
        .unwrap();
        db.save_catalog(
            vec![IndexedManager {
                manager: Manager {
                    period: Some("2026 Q3".into()),
                    updated_on: Some("2026-11-16".into()),
                    ..manager
                },
                rss_timestamp: None,
            }],
            true,
            vec![],
            "2026-11-16T10:00:00+00:00",
        )
        .unwrap();
        assert_eq!(db.changed_cached_managers().unwrap(), ["BRK"]);
        let new_revision = db.manager_revision("BRK").unwrap().unwrap().1;
        db.save_detail(
            "BRK",
            &new_revision,
            DetailUpdate {
                snapshot: Snapshot {
                    period: "2026 Q3".into(),
                    value_usd: 130.0,
                    ..snapshot
                },
                activities: None,
                history: None,
                warnings: vec![Issue::History],
                fetched_at: "2026-11-16T10:00:00+00:00".into(),
            },
        )
        .unwrap();
        let saved = db.portfolio("brk").unwrap().unwrap();
        assert_eq!(saved.snapshots[0].period, "2026 Q3");
        assert_eq!(saved.snapshots[1].period, "2026 Q2");
        assert_eq!(saved.history[0].value_usd, Some(100.0));
        assert_eq!(saved.activity_through.as_deref(), Some("2026 Q2"));
        assert!(db.changed_cached_managers().unwrap().is_empty());
    }
}
