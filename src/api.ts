import { invoke } from "@tauri-apps/api/core";

const issues = [
  "storage", "directory", "homepage", "feed", "catalogUnavailable", "emptyCatalog",
  "managerUnavailable", "holdingsUnavailable", "holdingsPending", "activity", "history",
  "activityChanged", "sourceLink", "previousSync", "unexpected",
] as const;

export type Issue = (typeof issues)[number];

export function toIssue(value: unknown, fallback: Issue): Issue {
  return typeof value === "string" && (issues as readonly string[]).includes(value)
    ? value as Issue
    : fallback;
}

export interface Manager {
  id: string;
  name: string;
  updatedOn: string | null;
  period: string | null;
  valueUsd: number | null;
  holdingCount: number | null;
  cached: boolean;
}

export interface Catalog {
  managers: Manager[];
  checkedAt: string | null;
}

export interface SyncReport {
  catalog: Catalog;
  warnings: Issue[];
}

export interface Holding {
  ticker: string;
  issuer: string;
  weight: number | null;
  shares: number | null;
  reportedPrice: number | null;
  valueUsd: number | null;
  recentActivity: string;
}

export interface Sector {
  name: string;
  weight: number;
}

export interface Snapshot {
  period: string;
  reportDate: string | null;
  valueUsd: number;
  holdings: Holding[];
  sectors: Sector[];
}

export interface Activity {
  period: string;
  ticker: string;
  issuer: string;
  action: string;
  changePercent: number | null;
  shareChange: number | null;
  portfolioImpact: number | null;
}

export interface HistoryHolding {
  ticker: string;
  issuer: string;
  weight: number;
}

export interface HistoryPoint {
  period: string;
  valueUsd: number | null;
  leaders: HistoryHolding[];
}

export interface Portfolio {
  manager: Manager;
  snapshots: Snapshot[];
  activities: Activity[];
  history: HistoryPoint[];
  activityThrough: string | null;
  activityPartialPeriod: string | null;
  activityHasMore: boolean;
  historyAvailable: boolean;
  syncedAt: string;
  warnings: Issue[];
}

export const api = {
  list: () => invoke<Catalog>("list_managers"),
  sync: () => invoke<SyncReport>("sync_catalog"),
  get: (id: string) => invoke<Portfolio | null>("get_portfolio", { id }),
  refresh: (id: string, force: boolean) =>
    invoke<Portfolio | null>("refresh_portfolio", { id, force }),
  loadMoreActivity: (id: string) => invoke<Portfolio | null>("load_more_activity", { id }),
  openSource: (id: string) => invoke<void>("open_source", { id }),
};
