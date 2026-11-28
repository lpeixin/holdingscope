use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use chrono::{DateTime, TimeDelta, Utc};
use reqwest::{Client, Url};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    db::{Database, DetailUpdate, IndexedManager},
    model::{ActivityBatch, Catalog, Issue, Manager, Portfolio, Snapshot, SyncReport},
    source::{self, Document},
};

type Result<T> = std::result::Result<T, String>;
type CommandResult<T> = std::result::Result<T, Issue>;

const ACTIVITY_PAGES_PER_REQUEST: u32 = 5;

#[derive(Clone)]
pub struct Service(Arc<Inner>);

struct Inner {
    db: Database,
    client: Client,
    catalog_lock: Mutex<()>,
    manager_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    request_slots: Semaphore,
}

struct SourceData<T> {
    value: Option<T>,
    document: Option<(&'static str, Document)>,
    reachable: bool,
    warning: Option<Issue>,
}

impl Service {
    pub fn new(path: &Path) -> Result<Self> {
        Ok(Self(Arc::new(Inner {
            db: Database::open(path)?,
            client: source::client()?,
            catalog_lock: Mutex::new(()),
            manager_locks: Mutex::default(),
            request_slots: Semaphore::new(4),
        })))
    }

    pub fn catalog(&self) -> Result<Catalog> {
        self.0.db.catalog()
    }

    pub fn portfolio(&self, id: &str) -> Result<Option<Portfolio>> {
        self.0.db.portfolio(id)
    }

    async fn document(&self, url: &str, previous: Option<&Document>) -> Result<Option<Document>> {
        let _slot = self
            .0
            .request_slots
            .acquire()
            .await
            .map_err(|error| error.to_string())?;
        source::fetch(&self.0.client, url, previous).await
    }

    async fn catalog_source<T>(
        &self,
        url: &'static str,
        issue: Issue,
        parse: impl Fn(&Document) -> Result<T>,
    ) -> Result<SourceData<T>> {
        let previous = self.0.db.document(url)?;
        let remote = self.document(url, previous.as_ref()).await;
        let error = match remote {
            Ok(Some(document)) => match parse(&document) {
                Ok(value) => {
                    return Ok(SourceData {
                        value: Some(value),
                        document: Some((url, document)),
                        reachable: true,
                        warning: None,
                    })
                }
                Err(error) => error,
            },
            Ok(None) => match previous.as_ref().map(&parse).transpose() {
                Ok(value) => {
                    return Ok(SourceData {
                        value,
                        document: None,
                        reachable: true,
                        warning: None,
                    })
                }
                Err(error) => error,
            },
            Err(error) => error,
        };
        Ok(SourceData {
            value: previous.as_ref().and_then(|document| parse(document).ok()),
            document: None,
            reachable: false,
            warning: Some(issue.log(error)),
        })
    }

    async fn rss(&self) -> Result<SourceData<BTreeMap<String, source::Notice>>> {
        let primary = self
            .catalog_source(source::RSS, Issue::Feed, |doc| source::rss(&doc.bytes))
            .await?;
        if primary.reachable {
            return Ok(primary);
        }
        let origin = self
            .catalog_source(source::RSS_ORIGIN, Issue::Feed, |doc| {
                source::rss(&doc.bytes)
            })
            .await?;
        if origin.reachable || primary.value.is_none() {
            return Ok(origin);
        }
        Ok(primary)
    }

    pub async fn sync_catalog(&self) -> CommandResult<SyncReport> {
        let _lock = self.0.catalog_lock.lock().await;
        let (directory, home, rss) = tokio::join!(
            self.catalog_source(
                source::DIRECTORY,
                Issue::Directory,
                |doc| source::directory(&doc.html())
            ),
            self.catalog_source(source::HOME, Issue::Homepage, |doc| source::home(
                &doc.html()
            )),
            self.rss()
        );
        let (directory, home, rss) = (directory?, home?, rss?);
        let warnings: Vec<_> = [directory.warning, home.warning, rss.warning]
            .into_iter()
            .flatten()
            .collect();
        if !directory.reachable && !home.reachable && !rss.reachable {
            return Err(Issue::CatalogUnavailable);
        }
        let authoritative = directory.reachable && home.reachable && directory.value.is_some();
        let mut merged: BTreeMap<_, _> = directory
            .value
            .unwrap_or_default()
            .into_iter()
            .map(|manager| {
                (
                    manager.id.to_ascii_uppercase(),
                    IndexedManager {
                        manager,
                        rss_timestamp: None,
                    },
                )
            })
            .collect();
        for manager in home.value.into_iter().flatten() {
            merge_manager(&mut merged, manager, None);
        }
        let rss_only = merged.is_empty();
        for (key, notice) in rss.value.into_iter().flatten() {
            if rss_only || merged.contains_key(&key) {
                merge_manager(&mut merged, notice.manager, Some(notice.timestamp));
            }
        }
        if merged.is_empty() && self.catalog()?.managers.is_empty() {
            return Err(Issue::EmptyCatalog);
        }
        self.0.db.save_catalog(
            merged.into_values().collect(),
            authoritative,
            [directory.document, home.document, rss.document]
                .into_iter()
                .flatten()
                .collect(),
            &Utc::now().to_rfc3339(),
        )?;
        let catalog = self.catalog()?;
        let outdated = self.0.db.changed_cached_managers()?;
        if !outdated.is_empty() {
            let service = self.clone();
            tauri::async_runtime::spawn(async move {
                for id in outdated {
                    if let Err(error) = service.refresh(&id, false).await {
                        eprintln!("HoldingScope refresh {id}: {error:?}");
                    }
                }
            });
        }
        Ok(SyncReport { catalog, warnings })
    }

    pub async fn refresh_portfolio(
        &self,
        id: &str,
        force: bool,
    ) -> CommandResult<Option<Portfolio>> {
        self.refresh(id, force).await?;
        Ok(self.portfolio(id)?)
    }

    async fn refresh(&self, id: &str, force: bool) -> CommandResult<()> {
        let key = id.to_ascii_uppercase();
        if self.0.db.manager_revision(&key)?.is_none() {
            return Err(Issue::ManagerUnavailable);
        }
        let lock = self
            .0
            .manager_locks
            .lock()
            .await
            .entry(key)
            .or_default()
            .clone();
        let _lock = lock.lock().await;
        let (manager, revision) = self
            .0
            .db
            .manager_revision(id)?
            .ok_or(Issue::ManagerUnavailable)?;
        if !force
            && self.0.db.cache_state(id)?.is_some_and(|state| {
                let ttl = if state.warnings.is_empty() {
                    TimeDelta::days(7)
                } else {
                    TimeDelta::minutes(10)
                };
                state.revision == revision
                    && DateTime::parse_from_rfc3339(&state.attempted_at)
                        .is_ok_and(|previous| Utc::now().signed_duration_since(previous) < ttl)
            })
        {
            return Ok(());
        }

        let id = &manager.id;
        let history_url =
            source::history_url(id).map_err(|error| Issue::ManagerUnavailable.log(error))?;
        let (holdings, activities, history) = tokio::join!(
            self.fetch_holdings(id),
            self.fetch_recent_activity(id),
            self.detail_source(history_url.as_str(), source::history)
        );
        let snapshot = holdings.map_err(|error| Issue::HoldingsUnavailable.log(error))?;
        if manager
            .period
            .as_ref()
            .is_some_and(|period| snapshot.period < *period)
        {
            return Err(Issue::HoldingsPending);
        }
        let mut warnings = Vec::new();
        let activities =
            optional(activities, Issue::Activity, &mut warnings).map(|activity| ActivityBatch {
                through: snapshot.period.clone(),
                ..activity
            });
        let history = optional(history, Issue::History, &mut warnings);
        self.0.db.save_detail(
            id,
            &revision,
            DetailUpdate {
                snapshot,
                activities,
                history,
                warnings,
                fetched_at: Utc::now().to_rfc3339(),
            },
        )?;
        Ok(())
    }

    async fn fetch_holdings(&self, id: &str) -> Result<Snapshot> {
        let url = source::holdings_url(id)?;
        let first = self
            .detail_source(url.as_str(), |html| source::holdings(html, 1))
            .await?;
        let mut snapshot = first.snapshot;
        let mut next = first.next_page;
        while let Some(page) = next {
            if snapshot.holdings.len() >= first.total {
                return Err("持仓数量与分页信息不一致".into());
            }
            let url = page_url(&url, page);
            let additional = self
                .detail_source(url.as_str(), |html| source::holdings(html, page))
                .await?;
            if additional.snapshot.period != snapshot.period
                || additional.snapshot.report_date != snapshot.report_date
                || additional.snapshot.value_usd != snapshot.value_usd
                || additional.total != first.total
                || additional.snapshot.holdings.is_empty()
            {
                return Err("持仓分页的报告信息不一致，请重新同步".into());
            }
            snapshot.holdings.extend(additional.snapshot.holdings);
            next = additional.next_page;
        }
        let distinct = snapshot
            .holdings
            .iter()
            .map(|row| &row.ticker)
            .collect::<BTreeSet<_>>();
        if distinct.len() != first.total || snapshot.holdings.len() != first.total {
            return Err(format!(
                "持仓记录不完整（预期 {} 条，获取 {} 条）",
                first.total,
                distinct.len()
            ));
        }
        Ok(snapshot)
    }

    async fn activity_page(&self, url: &Url, page: u32) -> Result<source::ActivityPage> {
        let url = page_url(url, page);
        self.detail_source(url.as_str(), |html| source::activities(html, page))
            .await
    }

    async fn fetch_recent_activity(&self, id: &str) -> Result<ActivityBatch> {
        let url = source::activity_url(id)?;
        let first = self.activity_page(&url, 1).await?;
        let mut changes = first.changes;
        let mut next = first.next_page;
        while changes.first().map(|row| &row.period) == changes.last().map(|row| &row.period) {
            let Some(page) = next else { break };
            let additional = self.activity_page(&url, page).await?;
            if additional.changes.is_empty() {
                return Err("季度变动分页缺少记录".into());
            }
            changes.extend(additional.changes);
            next = additional.next_page;
        }
        let partial_period = next.and_then(|_| changes.last().map(|row| row.period.clone()));
        Ok(ActivityBatch {
            changes,
            through: String::new(),
            partial_period,
            next_page: next,
        })
    }

    pub async fn load_more_activity(&self, id: &str) -> CommandResult<Option<Portfolio>> {
        self.refresh(id, false).await?;
        let lock = self
            .0
            .manager_locks
            .lock()
            .await
            .entry(id.to_ascii_uppercase())
            .or_default()
            .clone();
        let _lock = lock.lock().await;
        let Some(mut batch) = self.0.db.activity_batch(id)? else {
            return Err(Issue::Activity);
        };
        let Some(first_page) = batch.next_page else {
            return Ok(self.portfolio(id)?);
        };
        let url = source::activity_url(id).map_err(|error| Issue::ManagerUnavailable.log(error))?;
        let mut extra = Vec::new();
        for _ in 0..ACTIVITY_PAGES_PER_REQUEST {
            let Some(page) = batch.next_page else { break };
            let additional = self
                .activity_page(&url, page)
                .await
                .map_err(|error| Issue::Activity.log(error))?;
            if additional.changes.is_empty() {
                return Err(Issue::Activity);
            }
            extra.extend(additional.changes);
            batch.next_page = additional.next_page;
        }
        let first = &extra.first().expect("nonempty pages").period;
        let last = &extra.last().expect("nonempty pages").period;
        if batch
            .partial_period
            .as_ref()
            .is_some_and(|period| first > period)
        {
            return Err(Issue::ActivityChanged);
        }
        batch.changes.retain(|row| {
            &row.period > first
                || &row.period < last
                || batch.partial_period.as_ref() == Some(&row.period)
        });
        batch.partial_period = batch.next_page.map(|_| last.clone());
        batch.changes.extend(extra);
        batch.changes.sort_by(|a, b| b.period.cmp(&a.period));
        let mut seen = BTreeSet::new();
        batch.changes.retain(|entry| {
            seen.insert((
                entry.period.clone(),
                entry.ticker.clone(),
                entry.action.clone(),
                entry.share_change,
            ))
        });
        debug_assert!(batch.next_page.is_none_or(|page| page > first_page));
        self.0.db.save_activity_batch(id, batch)?;
        Ok(self.portfolio(id)?)
    }

    async fn detail_source<T>(
        &self,
        url: &str,
        parse: impl FnOnce(&str) -> Result<T>,
    ) -> Result<T> {
        let document = self.document(url, None).await?.ok_or("收到空响应")?;
        parse(&document.html())
    }
}

fn page_url(url: &Url, page: u32) -> Url {
    let mut url = url.clone();
    if page > 1 {
        url.query_pairs_mut().append_pair("L", &page.to_string());
    }
    url
}

fn optional<T>(value: Result<T>, issue: Issue, warnings: &mut Vec<Issue>) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(error) => {
            warnings.push(issue.log(error));
            None
        }
    }
}

fn merge_manager(
    managers: &mut BTreeMap<String, IndexedManager>,
    manager: Manager,
    timestamp: Option<String>,
) {
    let key = manager.id.to_ascii_uppercase();
    if let Some(existing) = managers.get_mut(&key) {
        existing.manager.updated_on = existing.manager.updated_on.take().max(manager.updated_on);
        existing.manager.period = existing.manager.period.take().max(manager.period);
        existing.rss_timestamp = existing.rss_timestamp.take().max(timestamp);
    } else {
        managers.insert(
            key,
            IndexedManager {
                manager,
                rss_timestamp: timestamp,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "uses the public Dataroma endpoints"]
    async fn live_dataroma() {
        let service = Service::new(Path::new(":memory:")).unwrap();
        let report = service.sync_catalog().await.unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!(report.catalog.managers.len() > 50);
        let dated = report
            .catalog
            .managers
            .iter()
            .filter(|manager| manager.updated_on.is_some())
            .count();
        assert!(dated > 50);
        eprintln!(
            "{} managers, {dated} with update dates",
            report.catalog.managers.len()
        );
        for id in ["BRK", "mc", "TCA"] {
            let portfolio = service.refresh_portfolio(id, false).await.unwrap().unwrap();
            let snapshot = &portfolio.snapshots[0];
            assert!(!snapshot.holdings.is_empty());
            assert!(snapshot
                .holdings
                .iter()
                .all(|holding| !holding.issuer.is_empty()));
            assert!(
                portfolio.activity_through.is_some(),
                "{id}: {:?}",
                portfolio.warnings
            );
            assert!(
                portfolio.history_available,
                "{id}: {:?}",
                portfolio.warnings
            );
            assert!(portfolio
                .history
                .iter()
                .any(|point| !point.leaders.is_empty()));
            assert!(!portfolio.activities.is_empty());
            assert_ne!(
                portfolio.activity_partial_period.as_ref(),
                Some(&snapshot.period)
            );
            if id == "BRK" && portfolio.activity_has_more {
                let expanded = service.load_more_activity(id).await.unwrap().unwrap();
                assert!(expanded.activities.len() > portfolio.activities.len());
                eprintln!(
                    "{id}: {} changes after loading more",
                    expanded.activities.len()
                );
            }
            let reread = service.portfolio(id).unwrap().unwrap();
            assert_eq!(reread.snapshots[0].period, snapshot.period);
            eprintln!(
                "{id}: {} holdings, {} changes, {} history points",
                snapshot.holdings.len(),
                portfolio.activities.len(),
                portfolio.history.len()
            );
        }
    }
}
