import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ArrowDown, ArrowDownUp, ArrowLeft, ArrowRight, ArrowUp, ArrowUpRight, BarChart3, Donut,
  ChartNoAxesCombined, ChevronDown, ChevronsUpDown, Layers3, RefreshCw, Search, Target, Wallet,
} from "lucide-react";
import { AllocationChart, SectorChart, ValueChart } from "./charts";
import { compact, dollars, issuerName, number, percent, price, quarterLabel, searchText } from "./format";
import { useLocale } from "./locale";
import { toIssue, type Activity, type HistoryPoint, type Holding, type Issue, type Portfolio, type Snapshot } from "./api";

export type Tab = "overview" | "holdings" | "activity" | "history";

export function Dashboard({ portfolio, tab, onTab, onMoreActivity }: {
  portfolio: Portfolio;
  tab: Tab;
  onTab: (tab: Tab) => void;
  onMoreActivity: () => Promise<void>;
}) {
  const { t } = useLocale();
  const snapshot = portfolio.snapshots[0];
  const history = useMemo(() => {
    const byPeriod = new Map(portfolio.history.map((point) => [point.period, point]));
    for (const cached of portfolio.snapshots) {
      byPeriod.set(cached.period, {
        period: cached.period, valueUsd: cached.valueUsd,
        leaders: cached.holdings.flatMap((holding) => holding.weight === null ? [] : [{
          ticker: holding.ticker, issuer: holding.issuer, weight: holding.weight,
        }]),
      });
    }
    return [...byPeriod.values()].sort((a, b) => a.period.localeCompare(b.period));
  }, [portfolio]);
  const recent = portfolio.activities.filter((activity) => activity.period === snapshot.period);

  switch (tab) {
    case "holdings":
      return <HoldingsPanel snapshots={portfolio.snapshots} />;
    case "activity":
      return <ActivityPanel portfolio={portfolio} onMore={onMoreActivity} />;
    case "history":
      return <HistoryPanel history={history} snapshots={portfolio.snapshots} />;
    default:
      return (
        <>
          <Metrics snapshot={snapshot} history={history} recent={recent} available={!!portfolio.activityThrough && portfolio.activityThrough >= snapshot.period} />
          <div className="chart-grid">
            <Card icon={<Donut />} title={t.allocation} subtitle={t.topFiveAllocation}>
              <AllocationChart snapshot={snapshot} />
            </Card>
            <ValueCard history={history} />
          </div>
          <HoldingTable snapshot={snapshot} preview onAll={() => onTab("holdings")} />
          <Card icon={<BarChart3 />} title={t.sectorAllocation} subtitle={t.reportedSectorWeights}>
            <SectorChart sectors={snapshot.sectors} />
          </Card>
        </>
      );
  }
}

function Card({ icon, title, subtitle, action, children, className = "" }: {
  icon: ReactNode;
  title: string;
  subtitle?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`card ${className}`}>
      <div className="card-header">
        <div className="card-heading">
          <span className="card-icon">{icon}</span>
          <div><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div>
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function Metrics({ snapshot, history, recent, available }: {
  snapshot: Snapshot; history: HistoryPoint[]; recent: Activity[]; available: boolean;
}) {
  const { locale, t } = useLocale();
  const [year, quarter] = snapshot.period.split(" Q").map(Number);
  const previousPeriod = quarter === 1 ? `${year - 1} Q4` : `${year} Q${quarter - 1}`;
  const prior = history.find((point) => point.period === previousPeriod)?.valueUsd;
  const change = prior ? ((snapshot.valueUsd - prior) / prior) * 100 : null;
  const ranked = snapshot.holdings.filter((holding) => holding.weight !== null);
  const topFive = ranked.length ? ranked.slice(0, 5).reduce((sum, holding) => sum + (holding.weight ?? 0), 0) : null;
  const increases = recent.filter((activity) => activity.action === "Buy" || activity.action === "Add").length;
  const decreases = recent.filter((activity) => activity.action === "Sell" || activity.action === "Reduce").length;
  return (
    <div className="metrics">
      <Metric icon={<Wallet />} tone="violet" title={t.portfolioValue} value={compact(snapshot.valueUsd, locale)} copy={t.reportedValue}>
        {change !== null && <span className={`metric-change ${change >= 0 ? "positive" : "negative"}`}>
          {change >= 0 ? <ArrowUp size={12} /> : <ArrowDown size={12} />}{percent(change, true)}
        </span>}
      </Metric>
      <Metric icon={<Layers3 />} tone="blue" title={t.holdingCount} value={number(snapshot.holdings.length)} copy={t.disclosedSecurities} />
      <Metric icon={<Target />} tone="gold" title={t.topFiveWeight} value={percent(topFive)} copy={t.concentration} />
      <Metric icon={<ArrowDownUp />} tone="teal" title={t.quarterChanges} value={available ? number(recent.length) : "—"}
        copy={available ? <><span className="positive"><ArrowUp size={12} /> {t.increases(increases)}</span><span className="negative"><ArrowDown size={12} /> {t.decreases(decreases)}</span></> : t.quarterActivityUnavailable} />
    </div>
  );
}

function Metric({ icon, tone, title, value, copy, children }: {
  icon: ReactNode; tone: string; title: string; value: string; copy: ReactNode; children?: ReactNode;
}) {
  return (
    <section className="metric">
      <div className="metric-title"><span className={`metric-icon ${tone}`}>{icon}</span>{title}</div>
      <div className="metric-value">{value}{children}</div>
      <div className="metric-copy">{copy}</div>
    </section>
  );
}

function ValueCard({ history }: { history: HistoryPoint[] }) {
  const { t } = useLocale();
  const [limit, setLimit] = useState(8);
  return (
    <Card icon={<ChartNoAxesCombined />} title={t.valueTrend} subtitle={t.quarterlyValue} className="value-card"
      action={
        <div className="segmented" aria-label={t.chartTimeRange}>
          {[8, 16, 0].map((range) => <button key={range} type="button" aria-pressed={limit === range}
            onClick={() => setLimit(range)}>{range ? t.quarters(range) : t.allTime}</button>)}
        </div>
      }>
      <ValueChart points={limit ? history.slice(-limit) : history} />
    </Card>
  );
}

function HoldingsPanel({ snapshots }: { snapshots: Snapshot[] }) {
  const { locale, t } = useLocale();
  const [selected, setSelected] = useState("");
  const snapshot = snapshots.find((snapshot) => snapshot.period === selected) ?? snapshots[0];
  return (
    <>
      <HoldingTable key={snapshot.period} snapshot={snapshot} action={snapshots.length > 1 && (
        <Select label={t.savedQuarter} value={snapshot.period} options={snapshots.map((s) => s.period)}
          onChange={setSelected} />
      )} />
      <div className="chart-grid">
        <Card icon={<Donut />} title={t.allocation} subtitle={quarterLabel(snapshot.period, locale)}>
          <AllocationChart snapshot={snapshot} />
        </Card>
        <Card icon={<BarChart3 />} title={t.sectorAllocation} subtitle={quarterLabel(snapshot.period, locale)}>
          <SectorChart sectors={snapshot.sectors} />
        </Card>
      </div>
    </>
  );
}

type Sortable = "ticker" | "weight" | "valueUsd" | "reportedPrice" | "shares";

function HoldingTable({ snapshot, preview, onAll, action }: {
  snapshot: Snapshot; preview?: boolean; onAll?: () => void; action?: ReactNode;
}) {
  const { locale, t } = useLocale();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<{ key: Sortable; desc: boolean }>({ key: "weight", desc: true });
  const [page, setPage] = useState(0);
  const normalized = searchText(query.trim());
  const limit = preview ? 5 : 25;
  const holdings = snapshot.holdings.filter((holding) => matchesSecurity(holding.ticker, holding.issuer, normalized));
  const sorted = [...holdings].sort((a, b) => {
    const left = a[sort.key];
    const right = b[sort.key];
    if (left === null) return right === null ? 0 : 1;
    if (right === null) return -1;
    const order = typeof left === "number" && typeof right === "number"
      ? left - right
      : String(left).localeCompare(String(right));
    return sort.desc ? -order : order;
  });
  const pageCount = Math.ceil(sorted.length / limit);
  const activePage = Math.min(page, Math.max(0, pageCount - 1));
  const visible = sorted.slice(activePage * limit, (activePage + 1) * limit);
  const sortHeader = (key: Sortable, title: string) => {
    const Icon = sort.key !== key ? ChevronsUpDown : sort.desc ? ArrowDown : ArrowUp;
    return <th scope="col" aria-sort={sort.key === key ? (sort.desc ? "descending" : "ascending") : "none"}>
      <button type="button" onClick={() => {
        setSort({ key, desc: sort.key === key ? !sort.desc : key !== "ticker" });
        setPage(0);
      }}>{title}<Icon size={12} /></button>
    </th>;
  };

  return (
    <Card icon={<Layers3 />} title={t.holdings} subtitle={t.holdingSummary(snapshot.holdings.length, quarterLabel(snapshot.period, locale))} className="table-card"
      action={preview ? <button type="button" className="button small" onClick={onAll}>{t.viewAll} <ArrowRight size={14} /></button> : action}>
      {!preview && <div className="table-tools">
        <label className="field"><Search size={15} /><input type="search" value={query}
          onChange={(event) => { setQuery(event.target.value); setPage(0); }} placeholder={t.searchSecurities} aria-label={t.searchHoldings} /></label>
        <span>{t.currencyNote}</span>
      </div>}
      <div className="table-scroll">
        <table className="data-table holding-table">
          <thead><tr>
            {sortHeader("ticker", t.ticker)}<th scope="col">{t.issuer}</th>{sortHeader("weight", t.weight)}{sortHeader("shares", t.shares)}
            {sortHeader("reportedPrice", t.reportedPrice)}{sortHeader("valueUsd", t.positionValue)}<th scope="col">{t.quarterActivity}</th>
          </tr></thead>
          <tbody>{visible.map((holding, index) => <HoldingRow holding={holding} key={`${holding.ticker}:${index}`} />)}</tbody>
        </table>
        {!visible.length && <div className="table-empty">{query ? t.noMatchingSecurities : t.noHoldings}</div>}
      </div>
      {!preview && pageCount > 1 && <Pagination page={activePage} total={pageCount} onChange={setPage} />}
    </Card>
  );
}

function HoldingRow({ holding }: { holding: Holding }) {
  const { locale } = useLocale();
  const [action, ...rest] = holding.recentActivity.split(" ");
  return (
    <tr>
      <td><span className="ticker">{holding.ticker}</span></td>
      <td className="issuer">{issuerName(holding.issuer, locale) || "—"}</td>
      <td><span className="weight"><span className="weight-track" aria-hidden="true">
        <i style={{ width: `${Math.min(100, (holding.weight ?? 0) * 3)}%` }} />
      </span>{percent(holding.weight)}</span></td>
      <td>{number(holding.shares)}</td>
      <td>{price(holding.reportedPrice)}</td>
      <td title={dollars(holding.valueUsd)}>{dollars(holding.valueUsd)}</td>
      <td>{action ? <ActionBadge action={action} text={rest.join(" ")} /> : <span className="muted">—</span>}</td>
    </tr>
  );
}

function ActivityPanel({ portfolio, onMore }: { portfolio: Portfolio; onMore: () => Promise<void> }) {
  const { locale, t } = useLocale();
  const periods = [...new Set([...portfolio.snapshots.map((s) => s.period), ...portfolio.activities.map((a) => a.period)])].sort().reverse();
  const [period, setPeriod] = useState("");
  const [action, setAction] = useState("");
  const [page, setPage] = useState(0);
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadError, setLoadError] = useState<Issue | null>(null);
  const chosen = periods.includes(period) ? period : periods[0];
  const available = !!portfolio.activityThrough && portfolio.activityThrough >= chosen;
  const rows = portfolio.activities.filter((row) => row.period === chosen);
  const partial = portfolio.activityPartialPeriod === chosen;
  const filtered = rows.filter((row) => !action || row.action === action);
  const pageCount = Math.ceil(filtered.length / 25);
  const activePage = Math.min(page, Math.max(0, pageCount - 1));

  return (
    <>
      <Card icon={<ArrowDownUp />} title={t.activitySummary} subtitle={t.activitySummaryCopy}
        action={<Select label={t.activityQuarter} value={chosen} options={periods} onChange={(value) => { setPeriod(value); setPage(0); }} />}>
        <div className="activity-metrics">
          {Object.entries(t.actions).map(([key, label]) => {
            const count = rows.filter((row) => row.action === key).length;
            return <button type="button" className={`activity-metric ${action === key ? "chosen" : ""}`}
              key={key} aria-pressed={action === key} onClick={() => { setAction(action === key ? "" : key); setPage(0); }}>
              <ActionBadge action={key} />
              <strong>{available ? number(count) : "—"}</strong>
              <span className="activity-track" aria-label={label}><i className={key} style={{ width: `${rows.length ? count / rows.length * 100 : 0}%` }} /></span>
            </button>;
          })}
        </div>
      </Card>
      <Card icon={<Layers3 />} title={t.activity} subtitle={t.activityCount(filtered.length, quarterLabel(chosen, locale), partial)} className="table-card"
        action={action && <button className="button small" onClick={() => { setAction(""); setPage(0); }}>{t.showAllActivity}</button>}>
        <div className="table-scroll">
          <table className="data-table activity-table">
            <thead><tr>{[t.ticker, t.issuer, t.activityType, t.shareChange, t.shareChangePercent, t.portfolioImpact].map((name) => <th key={name} scope="col">{name}</th>)}</tr></thead>
            <tbody>{filtered.slice(activePage * 25, (activePage + 1) * 25).map((row, index) =>
              <tr key={`${row.ticker}:${index}`}>
                <td><span className="ticker">{row.ticker}</span></td>
                <td className="issuer">{issuerName(row.issuer, locale) || "—"}</td>
                <td><ActionBadge action={row.action} /></td>
                <td className={signClass(row.shareChange)}>{row.shareChange && row.shareChange > 0 ? "+" : ""}{number(row.shareChange)}</td>
                <td className={signClass(row.changePercent)}>{percent(row.changePercent, true)}</td>
                <td className={signClass(row.portfolioImpact)}>{percent(row.portfolioImpact, true)}</td>
              </tr>)}</tbody>
          </table>
          {!filtered.length && <div className="table-empty">{!available ? t.quarterActivityUnavailable : action && rows.length ? t.noActivityOfType : t.noQuarterActivity}</div>}
        </div>
        {pageCount > 1 && <Pagination page={activePage} total={pageCount} onChange={setPage} />}
        {portfolio.activityHasMore && <div className="activity-more">
          {loadError && <span className="negative" role="status">{t.issues[loadError]}</span>}
          <button className="button small" disabled={loadingMore} onClick={async () => {
            setLoadingMore(true);
            setLoadError(null);
            try {
              await onMore();
            } catch (error) {
              setLoadError(toIssue(error, "activity"));
            } finally {
              setLoadingMore(false);
            }
          }}>
            {loadingMore ? <RefreshCw size={14} className="spinning" /> : <ArrowDown size={14} />}
            {loadingMore ? t.loading : t.loadMoreActivity}
          </button>
        </div>}
      </Card>
    </>
  );
}

function HistoryPanel({ history, snapshots }: { history: HistoryPoint[]; snapshots: Snapshot[] }) {
  const { locale, t } = useLocale();
  const latest = history.at(-1);
  const periods = history.slice(0, -1).reverse();
  const [selected, setSelected] = useState("");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const compare = periods.find((point) => point.period === selected) ?? periods[0];
  const latestSnapshot = snapshots.find((snapshot) => snapshot.period === latest?.period);
  const earlierSnapshot = snapshots.find((snapshot) => snapshot.period === compare?.period);
  const bothComplete = !!latestSnapshot && !!earlierSnapshot;
  const latestHoldings = new Map((latestSnapshot?.holdings ?? latest?.leaders ?? []).map((holding) => [holding.ticker, holding]));
  const earlierHoldings = new Map((earlierSnapshot?.holdings ?? compare?.leaders ?? []).map((holding) => [holding.ticker, holding]));
  const currentShares = new Map(latestSnapshot?.holdings.map((holding) => [holding.ticker, holding.shares]));
  const previousShares = new Map(earlierSnapshot?.holdings.map((holding) => [holding.ticker, holding.shares]));
  const tickers = [...new Set([...latestHoldings.keys(), ...earlierHoldings.keys()])];
  const normalized = searchText(query.trim());
  const rows = tickers.flatMap((ticker) => {
    const current = latestHoldings.get(ticker);
    const earlier = earlierHoldings.get(ticker);
    const issuer = current?.issuer ?? earlier?.issuer ?? "";
    if (!matchesSecurity(ticker, issuer, normalized)) return [];
    const currentWeight = current ? current.weight : latestSnapshot ? 0 : null;
    const earlierWeight = earlier ? earlier.weight : earlierSnapshot ? 0 : null;
    const difference = currentWeight === null || earlierWeight === null ? null : currentWeight - earlierWeight;
    const sharesNow = currentShares.get(ticker);
    const sharesBefore = previousShares.get(ticker);
    const shareChange = sharesNow === null || sharesBefore === null ? null : (sharesNow ?? 0) - (sharesBefore ?? 0);
    return [{
      ticker, issuer, difference, shareChange,
      current: current ? percent(current.weight) : latestSnapshot ? t.notHeld : t.notListed,
      earlier: earlier ? percent(earlier.weight) : earlierSnapshot ? t.notHeld : t.notListed,
    }];
  });
  const pageCount = Math.ceil(rows.length / 25);
  const activePage = Math.min(page, Math.max(0, pageCount - 1));

  return (
    <>
      <ValueCard history={history} />
      <Card icon={<ArrowDownUp />} title={t.compareWeights}
        subtitle={compare ? (bothComplete ? t.completeSnapshots : t.limitedHistory) : undefined}
        className="table-card"
        action={compare && <Select label={t.compareQuarter} value={compare.period} options={periods.map((p) => p.period)}
          onChange={(period) => { setSelected(period); setPage(0); }} />}>
        {latest && compare ? <>
          <div className="table-tools">
            <label className="field"><Search size={15} /><input type="search" value={query}
              onChange={(event) => { setQuery(event.target.value); setPage(0); }} placeholder={t.searchSecurities} aria-label={t.searchComparisons} /></label>
            <span>{t.percentagePoints}</span>
          </div>
          <div className="table-scroll">
            <table className="data-table history-table">
              <thead><tr>{[t.ticker, t.issuer, quarterLabel(latest.period, locale), quarterLabel(compare.period, locale), t.weightChange, ...(bothComplete ? [t.shareChange] : [])].map((name) => <th key={name} scope="col">{name}</th>)}</tr></thead>
              <tbody>{rows.slice(activePage * 25, (activePage + 1) * 25).map((row) => (
                <tr key={row.ticker}>
                  <td><span className="ticker">{row.ticker}</span></td>
                  <td className="issuer">{issuerName(row.issuer, locale) || "—"}</td>
                  <td>{row.current}</td>
                  <td>{row.earlier}</td>
                  <td className={signClass(row.difference)}>{row.difference === null ? "—" :
                    `${row.difference > 0 ? "+" : ""}${row.difference.toFixed(2)}`}</td>
                  {bothComplete && <td className={signClass(row.shareChange)}>
                    {row.shareChange && row.shareChange > 0 ? "+" : ""}{number(row.shareChange)}
                  </td>}
                </tr>
              ))}</tbody>
            </table>
            {!rows.length && <div className="table-empty">{t.noMatchingSecurities}</div>}
          </div>
          {pageCount > 1 && <Pagination page={activePage} total={pageCount} onChange={setPage} />}
        </> : <div className="table-empty">{t.noComparableQuarters}</div>}
      </Card>
    </>
  );
}

function matchesSecurity(ticker: string, issuer: string, query: string) {
  return [ticker, issuer, issuerName(issuer, "zh-CN")].some((value) => searchText(value).includes(query));
}

function signClass(value: number | null) {
  return value === null || value === 0 ? "" : value > 0 ? "positive" : "negative";
}

function ActionBadge({ action, text }: { action: string; text?: string }) {
  const { t } = useLocale();
  const up = action === "Buy" || action === "Add";
  const down = action === "Sell" || action === "Reduce";
  return <span className={`action-badge ${up ? "up" : down ? "down" : ""}`}>
    {up && <ArrowUpRight size={12} />}{down && <ArrowDown size={12} />}
    {Object.hasOwn(t.actions, action) ? t.actions[action as keyof typeof t.actions] : action}{text && ` ${text}`}
  </span>;
}

function Select({ label, value, options, onChange }: {
  label: string; value: string; options: string[]; onChange: (value: string) => void;
}) {
  const { locale } = useLocale();
  return <label className="select">
    <select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)}>
      {options.map((option) => <option key={option} value={option}>{quarterLabel(option, locale)}</option>)}
    </select>
    <ChevronDown size={14} />
  </label>;
}

function Pagination({ page, total, onChange }: { page: number; total: number; onChange: (page: number) => void }) {
  const { t } = useLocale();
  useEffect(() => {
    if (page >= total) onChange(Math.max(0, total - 1));
  }, [page, total, onChange]);
  return <div className="pagination">
    <span>{t.pagination(page + 1, total)}</span>
    <button type="button" className="button icon-button" aria-label={t.previousPage} disabled={page === 0} onClick={() => onChange(page - 1)}><ArrowLeft size={15} /></button>
    <button type="button" className="button icon-button" aria-label={t.nextPage} disabled={page + 1 === total} onClick={() => onChange(page + 1)}><ArrowRight size={15} /></button>
  </div>;
}
