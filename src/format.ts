import type { Manager } from "./api";
import type { Locale } from "./messages";
import { issuersZh, managersZh, sectorsZh } from "./names.zh-CN";

const decimal = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 });
const usd = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

const dates = {
  "zh-CN": new Intl.DateTimeFormat("zh-CN", { dateStyle: "long", timeZone: "UTC" }),
  en: new Intl.DateTimeFormat("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" }),
};
const shortDates = {
  "zh-CN": new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric", timeZone: "UTC" }),
  en: new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", timeZone: "UTC" }),
};
const times = {
  "zh-CN": new Intl.DateTimeFormat("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hourCycle: "h23" }),
  en: new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }),
};

function issuerKey(name: string) {
  return name.normalize("NFKD").toLowerCase().replace(/[^a-z0-9]/g, "");
}

const issuerNames = new Map(Object.entries(issuersZh).map(([name, translation]) => [issuerKey(name), translation]));

export function managerName(manager: Manager, locale: Locale): readonly [string, string?] {
  const translation = managersZh[manager.id.toUpperCase()];
  if (locale === "zh-CN" && translation) return translation;
  const [person, ...rest] = manager.name.split(" - ");
  return [person, rest.join(" - ")];
}

export function issuerName(raw: string, locale: Locale) {
  return locale === "zh-CN" ? (issuerNames.get(issuerKey(raw)) ?? raw) : raw;
}

export function sectorName(raw: string, locale: Locale, unknown: string) {
  const key = raw.trim().toLowerCase();
  if (!key) return unknown;
  return locale === "zh-CN" && Object.hasOwn(sectorsZh, key) ? sectorsZh[key] : raw;
}

export function searchText(value: string) {
  return value.normalize("NFKC").toLowerCase();
}

export function monogram(name: string) {
  if (/^\p{Script=Han}/u.test(name)) return name.split("·")[0].slice(0, 2);
  return name.split(/\s+/).filter(Boolean).slice(0, 2).map((part) => part[0].toUpperCase()).join("");
}

export function dateLabel(date: string, locale: Locale, short = false) {
  const value = new Date(`${date.slice(0, 10)}T00:00:00Z`);
  return Number.isNaN(value.getTime()) ? date : (short ? shortDates : dates)[locale].format(value);
}

export function timeLabel(date: string, locale: Locale) {
  const value = new Date(date);
  return Number.isNaN(value.getTime()) ? date : times[locale].format(value);
}

export function quarterLabel(period: string, locale: Locale, short = false) {
  const match = /^(\d{4}) Q([1-4])$/.exec(period);
  if (!match) return period;
  const [, year, quarter] = match;
  if (locale === "zh-CN") return short ? `${year.slice(2)}年${quarter}季` : `${year} 年第 ${quarter} 季度`;
  return short ? `Q${quarter} '${year.slice(2)}` : `Q${quarter} ${year}`;
}

export function number(value: number | null) {
  return value === null ? "—" : decimal.format(value);
}

export function dollars(value: number | null) {
  return value === null ? "—" : usd.format(value);
}

export function price(value: number | null) {
  return value === null ? "—" : "$" + value.toLocaleString("en-US", {
    minimumFractionDigits: 2, maximumFractionDigits: 2,
  });
}

const compactUnits = {
  "zh-CN": [[1e12, "万亿"], [1e8, "亿"], [1e4, "万"]],
  en: [[1e12, "T"], [1e9, "B"], [1e6, "M"], [1e3, "K"]],
} as const;

export function compact(value: number | null, locale: Locale, precision = 2) {
  if (value === null) return "—";
  const unit = compactUnits[locale].find(([threshold]) => Math.abs(value) >= threshold);
  if (!unit) return dollars(value);
  return `$${+(value / unit[0]).toFixed(precision)}${unit[1]}`;
}

export function percent(value: number | null, signed = false) {
  return value === null ? "—" : `${signed && value > 0 ? "+" : ""}${value.toFixed(2)}%`;
}
