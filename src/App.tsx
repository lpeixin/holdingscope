import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity, ArrowUpRight, CalendarDays, ChartNoAxesCombined, ChevronDown, ChevronRight, CircleAlert,
  Clock3, Database, FolderOpen, History, Languages, LayoutDashboard, Layers3,
  Menu, RefreshCw, Search, X,
} from "lucide-react";
import { api, toIssue, type Catalog, type Issue, type Manager, type Portfolio } from "./api";
import { Dashboard, type Tab } from "./dashboard";
import { dateLabel, managerName, monogram, quarterLabel, searchText, timeLabel } from "./format";
import { useLocale } from "./locale";
import type { LanguagePreference } from "./messages";

const tabs = [
  { id: "overview", Icon: LayoutDashboard },
  { id: "holdings", Icon: Layers3 },
  { id: "activity", Icon: Activity },
  { id: "history", Icon: History },
] as const;

type DetailState = { id: string; value: Portfolio | null; loading: boolean; error: Issue | null };

export default function App() {
  const { locale, t } = useLocale();
  const [catalog, setCatalog] = useState<Catalog>({ managers: [], checkedAt: null });
  const [selected, setSelected] = useState(() => window.localStorage.getItem("holdingscope.last-manager") ?? "BRK");
  const [tab, setTab] = useState<Tab>("overview");
  const [syncing, setSyncing] = useState(true);
  const [syncIssues, setSyncIssues] = useState<Issue[]>([]);
  const [check, setCheck] = useState<{ version: number; forceId?: string }>({ version: 0 });
  const [detail, setDetail] = useState<DetailState | null>(null);
  const [sourceError, setSourceError] = useState<Issue | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [retry, setRetry] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const scroll = useRef<HTMLElement>(null);
  const forceUsed = useRef(0);
  const manager = catalog.managers.find((manager) => manager.id === selected) ?? catalog.managers[0];
  const managerId = manager?.id;

  const sync = useCallback(async (forceId?: string) => {
    setSyncing(true);
    setSyncIssues([]);
    try {
      const result = await api.sync();
      setCatalog(result.catalog);
      setSyncIssues(result.warnings);
    } catch (error) {
      setSyncIssues([toIssue(error, "catalogUnavailable")]);
    } finally {
      setCheck((previous) => ({ version: previous.version + 1, forceId }));
      setSyncing(false);
    }
  }, []);

  useEffect(() => {
    let active = true;
    async function start() {
      try {
        const cached = await api.list();
        if (active) setCatalog(cached);
      } catch (error) {
        if (active) setSyncIssues([toIssue(error, "storage")]);
      }
      if (active) void sync();
    }
    void start();
    return () => { active = false; };
  }, [sync]);

  useEffect(() => {
    if (!managerId) return;
    let active = true;
    const id = managerId;
    setDetail((previous) => ({
      id, value: previous?.id === id ? previous.value : null, loading: true, error: null,
    }));
    async function load() {
      const force = check.forceId === id && check.version > forceUsed.current;
      try {
        const cached = await api.get(id);
        if (!active) return;
        if (cached) setDetail({ id, value: cached, loading: true, error: null });
      } catch (error) {
        if (active) setDetail({ id, value: null, loading: false, error: toIssue(error, "storage") });
      }
      if (!active) return;
      if (force) forceUsed.current = check.version;
      try {
        const updated = await api.refresh(id, force);
        if (active) setDetail({ id, value: updated, loading: false, error: null });
      } catch (error) {
        if (active) setDetail((previous) => ({
          id, value: previous?.id === id ? previous.value : null, loading: false, error: toIssue(error, "holdingsUnavailable"),
        }));
      }
    }
    void load();
    return () => { active = false; };
  }, [managerId, check, retry]);

  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSidebarOpen(true);
        requestAnimationFrame(() => searchRef.current?.focus());
      }
      if (event.key === "Escape") setSidebarOpen(false);
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, []);

  useEffect(() => {
    scroll.current?.scrollTo({ top: 0 });
  }, [managerId, tab]);

  const current = detail?.id === managerId ? detail : null;
  const portfolio = current?.value ?? undefined;
  const hasWarning = !!syncIssues.length || !!current?.error || !!portfolio?.warnings.length;
  const status = syncing ? t.checking : hasWarning ? t.syncWarning : catalog.checkedAt ? t.synced : t.waitingForSync;

  return (
    <div className="app-shell">
      {sidebarOpen && <button type="button" className="sidebar-scrim" aria-label={t.closeDirectory} onClick={() => setSidebarOpen(false)} />}
      <Sidebar
        managers={catalog.managers} selected={managerId} open={sidebarOpen} onClose={() => setSidebarOpen(false)}
        searchRef={searchRef} checkedAt={catalog.checkedAt} status={status} warning={hasWarning} syncing={syncing}
        onSelect={(id) => {
          setSelected(id);
          window.localStorage.setItem("holdingscope.last-manager", id);
          setTab("overview");
          setSourceError(null);
          setSidebarOpen(false);
        }}
      />
      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumbs">
            <button type="button" className="button icon-button mobile-toggle" aria-label={t.openDirectory} onClick={() => setSidebarOpen(true)}><Menu size={19} /></button>
            <LayoutDashboard size={17} />
            <span>{t.portfolioTracker}</span>
            {manager && <><ChevronRight size={14} /><strong>{managerName(manager, locale)[0]}</strong></>}
          </div>
          <div className="top-actions">
            <span className="local-tag"><Database size={13} /> {t.localDatabase}</span>
            <LanguageSelector />
            <button type="button" className="button primary" onClick={() => void sync(managerId)} disabled={syncing || !!current?.loading}>
              <RefreshCw size={15} className={syncing ? "spinning" : ""} />{syncing ? t.syncing : t.sync}
            </button>
          </div>
        </header>
        <main className="workspace-scroll" ref={scroll}>
          <div className="content">
            {manager ? (
              <>
                <PortfolioHeading manager={manager} portfolio={portfolio} onOpen={() => {
                  setSourceError(null);
                  void api.openSource(manager.id).catch((error) => setSourceError(toIssue(error, "sourceLink")));
                }} />
                <nav className="tabs" aria-label={t.portfolioViews}>
                  {tabs.map(({ id, Icon }) => <button key={id} type="button"
                    aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}>
                    <Icon size={16} />{t[id]}
                  </button>)}
                </nav>
                {(syncIssues.length > 0 || sourceError || (current?.error && portfolio) || !!portfolio?.warnings.length) && <div className="notice" role="status">
                  <CircleAlert size={17} /><div>
                    {syncIssues.map((issue) => <p key={issue}>{t.issues[issue]}</p>)}
                    {sourceError && <p>{t.issues[sourceError]}</p>}
                    {current?.error && portfolio && <p>{t.showingCache} {t.issues[current.error]}</p>}
                    {portfolio?.warnings.map((issue) => <p key={issue}>{t.issues[issue]}</p>)}
                  </div>
                </div>}
                {portfolio
                  ? <Dashboard key={manager.id} portfolio={portfolio} tab={tab} onTab={setTab} onMoreActivity={async () => {
                    const updated = await api.loadMoreActivity(manager.id);
                    if (updated) setDetail((previous) => previous?.id === manager.id
                      ? { ...previous, value: updated } : previous);
                  }} />
                  : <Empty busy={!current || current.loading} title={current?.error ? t.holdingsLoadFailed : t.portfolioLoading}
                    copy={current?.error ? t.issues[current.error] : t.portfolioLoadingCopy} onRetry={current?.error ? () => setRetry((n) => n + 1) : undefined} />}
                {portfolio && <footer className="content-footer"><span>{t.sourceFootnote}</span><span>{t.savedAt(timeLabel(portfolio.syncedAt, locale))}</span></footer>}
              </>
            ) : <Empty busy={syncing} title={syncing ? t.loadingDirectory : t.directoryLoadFailed}
              copy={syncIssues.length ? syncIssues.map((issue) => t.issues[issue]).join(" ") : t.directoryFirstLoad} onRetry={syncing ? undefined : () => void sync()} />}
          </div>
        </main>
      </div>
    </div>
  );
}

function Sidebar({ managers, selected, open, onClose, searchRef, onSelect, checkedAt, status, warning, syncing }: {
  managers: Manager[]; selected?: string; open: boolean; onClose: () => void;
  searchRef: React.RefObject<HTMLInputElement | null>; onSelect: (id: string) => void;
  checkedAt: string | null; status: string; warning: boolean; syncing: boolean;
}) {
  const { locale, t } = useLocale();
  const [query, setQuery] = useState("");
  const [order, setOrder] = useState("recent");
  const selectedRow = useRef<HTMLButtonElement>(null);
  const normalized = searchText(query.trim());
  const results = useMemo(() => {
    const collator = new Intl.Collator(locale);
    return managers
      .map((manager) => ({ manager, labels: managerName(manager, locale) }))
      .filter(({ manager, labels }) =>
        [manager.name, manager.id, ...managerName(manager, "zh-CN"), ...labels]
          .some((label) => label && searchText(label).includes(normalized)),
      )
      .sort((a, b) => {
        const byDate = (b.manager.updatedOn ?? "").localeCompare(a.manager.updatedOn ?? "");
        return (order === "recent" && byDate) || collator.compare(a.labels.join(" "), b.labels.join(" "));
      });
  }, [managers, normalized, order, locale]);

  useEffect(() => {
    if (normalized || !selectedRow.current) return;
    selectedRow.current.scrollIntoView({ block: "nearest" });
  }, [selected, normalized, order, locale]);

  return (
    <aside className={`sidebar ${open ? "visible" : ""}`} aria-label={t.investorDirectory}>
      <div className="brand">
        <span className="brand-icon"><ChartNoAxesCombined size={22} strokeWidth={2.3} /></span>
        <div><strong>HoldingScope</strong><span>{t.brandSubtitle}</span></div>
        <button type="button" className="sidebar-close" aria-label={t.closeDirectory} onClick={onClose}><X size={19} /></button>
      </div>
      <div className="sidebar-main">
        <label className="sidebar-search"><Search size={16} />
          <input type="search" placeholder={t.searchInvestors} aria-label={t.searchInvestors} ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} />
          {!query && <kbd>{/Mac/.test(navigator.platform) ? "⌘" : "Ctrl"} K</kbd>}
        </label>
        <div className="sidebar-heading">
          <span>{t.investors} <b>{normalized ? results.length : managers.length}</b></span>
          <label className="sidebar-sort">
            <select aria-label={t.sortInvestors} value={order} onChange={(event) => setOrder(event.target.value)}>
              <option value="recent">{t.recentUpdates}</option><option value="name">{t.byName}</option>
            </select><ChevronDown size={12} />
          </label>
        </div>
        <nav className="manager-list" aria-label={t.investors}>
          {results.map(({ manager, labels: [name, firm] }) => {
            const period = manager.period && quarterLabel(manager.period, locale);
            const updated = manager.updatedOn && t.updatedAt(dateLabel(manager.updatedOn, locale));
            return <button type="button" key={manager.id} className="manager-item" title={[name, firm, period, updated].filter(Boolean).join(" · ")}
              ref={manager.id === selected ? selectedRow : undefined}
              aria-current={manager.id === selected ? "true" : undefined}
              onClick={() => onSelect(manager.id)}>
              <span className="manager-monogram" aria-hidden="true">{monogram(name)}</span>
              <span className="manager-label"><strong>{name}</strong><span>{firm || period || t.portfolioProfile}</span></span>
              <time dateTime={manager.updatedOn ?? undefined}>{manager.updatedOn ? dateLabel(manager.updatedOn, locale, true) : "—"}</time>
            </button>;
          })}
          {!results.length && <p className="sidebar-empty">{query ? t.noInvestorsFound : syncing ? t.fetchingDirectory : t.noDirectory}</p>}
        </nav>
      </div>
      <div className="sidebar-footer">
        <span className={`sync-dot ${warning ? "warn" : ""} ${syncing ? "pulsing" : ""}`} />
        <div><strong>{status}</strong><span>{checkedAt ? t.lastChecked(timeLabel(checkedAt, locale)) : "Dataroma RSS"}</span></div>
        <Database size={17} />
      </div>
    </aside>
  );
}

function PortfolioHeading({ manager, portfolio, onOpen }: {
  manager: Manager; portfolio?: Portfolio; onOpen: () => void;
}) {
  const { locale, t } = useLocale();
  const [name, firm] = managerName(manager, locale);
  const current = portfolio?.snapshots[0];
  const period = current?.period ?? manager.period;
  return (
    <section className="portfolio-heading">
      <div>
        <div className="portfolio-kicker"><span>{period ? quarterLabel(period, locale) : t.portfolioReport}</span></div>
        <h1>{name}</h1>
        {firm && <p className="firm">{firm}</p>}
        <div className="portfolio-meta">
          {current?.reportDate && <span><CalendarDays size={15} /> {t.reportDate} {dateLabel(current.reportDate, locale)}</span>}
          {manager.updatedOn && <span><Clock3 size={15} /> {t.sourceUpdated} {dateLabel(manager.updatedOn, locale)}</span>}
        </div>
      </div>
      <button type="button" className="button source-button" onClick={onOpen}>{t.openDataroma} <ArrowUpRight size={15} /></button>
    </section>
  );
}

function Empty({ busy, title, copy, onRetry }: { busy: boolean; title: string; copy: string; onRetry?: () => void }) {
  const { t } = useLocale();
  return (
    <section className="empty-state card" role="status" aria-busy={busy}>
      <div className="empty-icon">{busy ? <RefreshCw size={26} className="spinning" /> : <FolderOpen size={28} />}</div>
      <h2>{title}</h2><p>{copy}</p>
      {onRetry && <button type="button" className="button primary" onClick={onRetry}><RefreshCw size={15} /> {t.retry}</button>}
    </section>
  );
}

function LanguageSelector() {
  const { preference, setPreference, t } = useLocale();
  return (
    <label className="language-setting">
      <Languages size={15} aria-hidden="true" />
      <select aria-label={t.appLanguage} value={preference}
        onChange={(event) => setPreference(event.target.value as LanguagePreference)}>
        <option value="system">{t.systemLanguage}</option>
        <option value="zh-CN" lang="zh-CN">简体中文</option>
        <option value="en" lang="en">English</option>
      </select>
      <ChevronDown size={13} aria-hidden="true" />
    </label>
  );
}
