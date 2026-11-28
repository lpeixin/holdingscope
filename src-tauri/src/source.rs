use std::{collections::BTreeMap, sync::LazyLock, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{header, Client, StatusCode, Url};
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;

use crate::model::{Activity, HistoryHolding, HistoryPoint, Holding, Manager, Sector, Snapshot};

const BASE: &str = "https://www.dataroma.com";
pub const DIRECTORY: &str = "https://www.dataroma.com/m/managers.php";
pub const HOME: &str = "https://www.dataroma.com/m/home.php";
pub const RSS: &str = "https://feeds.feedburner.com/dataroma";
pub const RSS_ORIGIN: &str = "https://www.dataroma.com/m/rss.xml";
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

pub struct Document {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
    pub modified: Option<String>,
}

impl Document {
    pub fn html(&self) -> String {
        String::from_utf8(self.bytes.clone())
            .unwrap_or_else(|_| self.bytes.iter().map(|&byte| char::from(byte)).collect())
    }
}

pub fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!(
            "HoldingScope/",
            env!("CARGO_PKG_VERSION"),
            " (local Dataroma portfolio viewer)"
        ))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| error.to_string())
}

pub async fn fetch(
    client: &Client,
    url: &str,
    previous: Option<&Document>,
) -> Result<Option<Document>, String> {
    let mut request = client.get(url);
    if let Some(previous) = previous {
        if let Some(etag) = &previous.etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &previous.modified {
            request = request.header(header::IF_MODIFIED_SINCE, modified);
        }
    }
    let mut response = request.send().await.map_err(|error| error.to_string())?;
    if response.status() == StatusCode::NOT_MODIFIED && previous.is_some() {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let read_header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok().map(str::to_owned))
    };
    let etag = read_header(header::ETAG);
    let modified = read_header(header::LAST_MODIFIED);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            return Err("响应超过大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(Document {
        bytes,
        etag,
        modified,
    }))
}

pub fn holdings_url(id: &str) -> Result<Url, String> {
    manager_url("holdings.php", "m", id)
}

pub fn activity_url(id: &str) -> Result<Url, String> {
    let mut url = manager_url("m_activity.php", "m", id)?;
    url.query_pairs_mut()
        .append_pair("typ", "a")
        .append_pair("o", "a");
    Ok(url)
}

pub fn history_url(id: &str) -> Result<Url, String> {
    manager_url("hist/p_hist.php", "f", id)
}

fn manager_url(path: &str, key: &str, id: &str) -> Result<Url, String> {
    if !valid_id(id) {
        return Err("无效的持仓大户标识".into());
    }
    Url::parse_with_params(&format!("{BASE}/m/{path}"), [(key, id)])
        .map_err(|error| error.to_string())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn manager_id(link: &str) -> Option<String> {
    let url = Url::parse(BASE).ok()?.join(link).ok()?;
    if !matches!(url.host_str(), Some("www.dataroma.com" | "dataroma.com"))
        || url.path() != "/m/holdings.php"
    {
        return None;
    }
    url.query_pairs()
        .find(|(key, value)| key == "m" && valid_id(value))
        .map(|(_, value)| value.into_owned())
}

static ROWS: LazyLock<Selector> = LazyLock::new(|| selector("#grid tbody tr"));
static CELLS: LazyLock<Selector> = LazyLock::new(|| selector("td"));
static LINKS: LazyLock<Selector> = LazyLock::new(|| selector("a[href]"));
static SPANS: LazyLock<Selector> = LazyLock::new(|| selector("span"));

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("valid static selector")
}

fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn txt(element: ElementRef<'_>) -> String {
    clean(&element.text().collect::<String>())
}

fn date(value: &str) -> Option<String> {
    NaiveDate::parse_from_str(value, "%d %b %Y")
        .ok()
        .map(|date| date.to_string())
}

fn quarter(value: &str) -> Option<String> {
    let mut parts = value.split_whitespace();
    let (left, right) = (parts.next()?, parts.next()?);
    let (year, quarter) = if left.starts_with('Q') {
        (right, left)
    } else {
        (left, right)
    };
    if parts.next().is_none()
        && year.len() == 4
        && year.parse::<u16>().is_ok()
        && matches!(quarter, "Q1" | "Q2" | "Q3" | "Q4")
    {
        Some(format!("{year} {quarter}"))
    } else {
        None
    }
}

fn number(value: &str) -> Option<f64> {
    let normalized = value.trim().replace([',', '$', '%'], "").replace('−', "-");
    let number: f64 = normalized.trim().parse().ok()?;
    number.is_finite().then_some(number)
}

fn integer(value: &str) -> Option<i64> {
    value.trim().replace(',', "").replace('−', "-").parse().ok()
}

fn currency(value: &str) -> Option<f64> {
    let (amount, multiplier) = match value.chars().last()? {
        'T' => (&value[..value.len() - 1], 1e12),
        'B' => (&value[..value.len() - 1], 1e9),
        'M' => (&value[..value.len() - 1], 1e6),
        'K' => (&value[..value.len() - 1], 1e3),
        _ => (value, 1.0),
    };
    number(amount).map(|x| x * multiplier)
}

fn stock(cell: ElementRef<'_>) -> Option<(String, String)> {
    let link = cell.select(&LINKS).next()?;
    let url = Url::parse(BASE).ok()?.join(link.attr("href")?).ok()?;
    let ticker = url.query_pairs().find(|(key, _)| key == "sym")?.1;
    let issuer = link
        .select(&SPANS)
        .next()
        .map(txt)
        .map(|name| name.trim_start_matches('-').trim().to_owned())
        .unwrap_or_default();
    Some((ticker.into_owned(), issuer))
}

pub fn directory(html: &str) -> Result<Vec<Manager>, String> {
    let html = Html::parse_document(html);
    let managers = html
        .select(&ROWS)
        .filter_map(|row| {
            let cells: Vec<_> = row.select(&CELLS).take(3).collect();
            let link = cells.first()?.select(&LINKS).next()?;
            Some(Manager {
                id: manager_id(link.attr("href")?)?,
                name: txt(link),
                updated_on: None,
                period: None,
                value_usd: cells.get(1).and_then(|cell| currency(&txt(*cell))),
                holding_count: cells
                    .get(2)
                    .and_then(|cell| txt(*cell).replace(',', "").parse().ok()),
                cached: false,
            })
        })
        .collect::<Vec<_>>();
    if managers.is_empty() {
        return Err("无法识别持仓大户目录".into());
    }
    Ok(managers)
}

pub fn home(html: &str) -> Result<Vec<Manager>, String> {
    let html = Html::parse_document(html);
    let mut managers = Vec::new();
    let mut period = None;
    for element in html.select(&selector("#port_body p, #port_body a[href]")) {
        let value = txt(element);
        if element.value().name() == "p" {
            period = quarter(&value);
            continue;
        }
        let Some(id) = element.attr("href").and_then(manager_id) else {
            continue;
        };
        let (name, updated_on) = value.split_once(" Updated ").unwrap_or((&value, ""));
        managers.push(Manager {
            id,
            name: name.to_owned(),
            updated_on: date(updated_on),
            period: period.clone(),
            value_usd: None,
            holding_count: None,
            cached: false,
        });
    }
    if managers.is_empty() {
        return Err("无法识别首页的组合更新列表".into());
    }
    Ok(managers)
}

#[derive(Deserialize)]
struct RssFeed {
    channel: RssChannel,
}

#[derive(Deserialize)]
struct RssChannel {
    #[serde(default, rename = "item")]
    items: Vec<RssItem>,
}

#[derive(Deserialize)]
struct RssItem {
    title: String,
    link: String,
    #[serde(default, rename = "pubDate")]
    date: String,
    #[serde(default)]
    guid: String,
}

pub struct Notice {
    pub manager: Manager,
    pub timestamp: String,
}

pub fn rss(bytes: &[u8]) -> Result<BTreeMap<String, Notice>, String> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.read_event().map_err(|error| error.to_string())?;
    let xml = reader
        .encoding()
        .decode_without_bom_handling_and_without_replacement(bytes)
        .ok_or("无法解码 RSS 文档")?;
    let feed: RssFeed = quick_xml::de::from_str(&xml).map_err(|error| error.to_string())?;
    let mut notices = BTreeMap::<String, Notice>::new();
    for item in feed.channel.items {
        let Some(id) = manager_id(&item.link) else {
            continue;
        };
        let Ok(date) = DateTime::parse_from_rfc2822(&item.date) else {
            continue;
        };
        let timestamp = date.with_timezone(&Utc).to_rfc3339();
        let period = item
            .guid
            .rsplit_once('#')
            .and_then(|(_, code)| code.split_at_checked(4))
            .and_then(|(year, q)| quarter(&format!("{year} Q{q}")));
        let key = id.to_ascii_uppercase();
        if notices
            .get(&key)
            .is_some_and(|notice| notice.timestamp >= timestamp)
        {
            continue;
        }
        notices.insert(
            key,
            Notice {
                manager: Manager {
                    id,
                    name: item.title,
                    updated_on: Some(date.date_naive().to_string()),
                    period,
                    value_usd: None,
                    holding_count: None,
                    cached: false,
                },
                timestamp,
            },
        );
    }
    Ok(notices)
}

pub struct HoldingsPage {
    pub snapshot: Snapshot,
    pub total: usize,
    pub next_page: Option<u32>,
}

pub struct ActivityPage {
    pub changes: Vec<Activity>,
    pub next_page: Option<u32>,
}

fn next_page(doc: &Html, requested: u32) -> Result<Option<u32>, String> {
    let current = doc
        .select(&selector("#pages a.sel"))
        .next()
        .and_then(|element| txt(element).parse::<u32>().ok())
        .unwrap_or(1);
    if current != requested {
        return Err(format!("来源未返回所请求的第 {requested} 页"));
    }
    let next = doc
        .select(&selector("#pages a[href]"))
        .find(|element| txt(*element).eq_ignore_ascii_case("Next"))
        .and_then(|element| Url::parse(BASE).ok()?.join(element.attr("href")?).ok())
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "L")?
                .1
                .parse::<u32>()
                .ok()
        });
    Ok(next.filter(|page| *page == current + 1))
}

pub fn holdings(html: &str, page: u32) -> Result<HoldingsPage, String> {
    let doc = Html::parse_document(html);
    let details: Vec<_> = doc
        .select(&selector("#p2 > span"))
        .take(4)
        .map(txt)
        .collect();
    let [period, report_date, count, value] = details.as_slice() else {
        return Err("无法识别组合的报告信息".into());
    };
    let period = quarter(period).ok_or("无法识别报告季度")?;
    let count: usize = count
        .replace(',', "")
        .parse()
        .map_err(|_| "无法识别持仓数量")?;
    let value_usd = currency(value).ok_or("无法识别组合市值")?;
    let mut holdings = Vec::new();
    for row in doc.select(&ROWS) {
        let cells: Vec<_> = row.select(&CELLS).take(7).collect();
        let [_, name, weight, activity, shares, price, value] = cells.as_slice() else {
            continue;
        };
        let Some((ticker, issuer)) = stock(*name) else {
            continue;
        };
        holdings.push(Holding {
            ticker,
            issuer,
            weight: number(&txt(*weight)),
            shares: integer(&txt(*shares)),
            reported_price: currency(&txt(*price)),
            value_usd: currency(&txt(*value)),
            recent_activity: txt(*activity),
        });
    }
    let sectors = doc
        .select(&selector("#sect table tr"))
        .filter_map(|row| {
            let mut cells = row.select(&CELLS);
            let name = txt(cells.next()?);
            let weight = number(&txt(cells.next()?))?;
            Some(Sector { name, weight })
        })
        .collect();
    Ok(HoldingsPage {
        snapshot: Snapshot {
            period,
            report_date: date(report_date),
            value_usd,
            holdings,
            sectors,
        },
        total: count,
        next_page: next_page(&doc, page)?,
    })
}

pub fn activities(html: &str, page: u32) -> Result<ActivityPage, String> {
    let doc = Html::parse_document(html);
    if doc.select(&selector("#f_name")).next().is_none()
        || doc.select(&selector("#grid thead")).next().is_none()
    {
        return Err("无法识别季度变动页面".into());
    }
    let mut period = None;
    let mut changes = Vec::new();
    for row in doc.select(&ROWS) {
        let cells: Vec<_> = row.select(&CELLS).take(5).collect();
        if row.value().classes().any(|class| class == "q_chg") {
            period = cells.first().and_then(|cell| quarter(&txt(*cell)));
            if period.is_none() {
                return Err("无法识别变动记录的季度".into());
            }
            continue;
        }
        let [_, name, action, shares, impact] = cells.as_slice() else {
            continue;
        };
        let Some((ticker, issuer)) = stock(*name) else {
            continue;
        };
        let Some(period) = &period else {
            return Err("变动记录缺少季度".into());
        };
        let label = txt(*action);
        let (action, percent) = label.split_once(' ').unwrap_or((&label, ""));
        let decrease = matches!(action, "Reduce" | "Sell");
        let signed_percent =
            number(percent).map(|value| if decrease { -value.abs() } else { value });
        let shares =
            integer(&txt(*shares)).map(|value| if decrease { -value.abs() } else { value });
        let impact = number(&txt(*impact)).map(|value| if decrease { -value.abs() } else { value });
        changes.push(Activity {
            period: period.clone(),
            ticker,
            issuer,
            action: action.to_owned(),
            change_percent: signed_percent,
            share_change: shares,
            portfolio_impact: impact,
        });
    }
    Ok(ActivityPage {
        changes,
        next_page: next_page(&doc, page)?,
    })
}

pub fn history(html: &str) -> Result<Vec<HistoryPoint>, String> {
    let doc = Html::parse_document(html);
    if doc.select(&selector("#grid thead")).next().is_none() {
        return Err("无法识别组合历史页面".into());
    }
    let names = selector("div b");
    let weights = selector("div");
    let mut history = Vec::new();
    for row in doc.select(&ROWS) {
        let mut cells = row.select(&CELLS);
        let Some(period) = cells.next().and_then(|cell| quarter(&txt(cell))) else {
            continue;
        };
        let value_usd = cells.next().and_then(|cell| currency(&txt(cell)));
        let leaders = cells
            .filter_map(|cell| {
                let (ticker, _) = stock(cell)?;
                let issuer = txt(cell.select(&names).next()?);
                let detail = cell.select(&weights).next()?.text().collect::<Vec<_>>();
                let weight = detail
                    .iter()
                    .find_map(|value| value.split_once('%').and_then(|(part, _)| number(part)))?;
                Some(HistoryHolding {
                    ticker,
                    issuer,
                    weight,
                })
            })
            .collect();
        history.push(HistoryPoint {
            period,
            value_usd,
            leaders,
        });
    }
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss_with_latin1_titles_and_latest_revision() {
        let mut xml = br#"<?xml version="1.0" encoding="ISO-8859-1"?>
<rss version="2.0"><channel>
<item><title><![CDATA[Andr"#
            .to_vec();
        xml.push(0xe9);
        xml.extend_from_slice(br#" & Firm]]></title><link>http://www.dataroma.com/m/holdings.php?m=ab</link>
<guid isPermaLink="false">example#20263</guid><pubDate>Mon, 16 Nov 2026 12:11:30 -0700</pubDate></item>
<item><title>Older &amp; Firm</title><link>https://dataroma.com/m/holdings.php?m=AB</link>
<pubDate>Sat, 15 Aug 2026 12:11:30 -0700</pubDate></item>
<item><title>External</title><link>https://example.com/m/holdings.php?m=zz</link>
<pubDate>Mon, 16 Nov 2026 12:11:30 -0700</pubDate></item></channel></rss>"#);
        let notices = rss(&xml).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices["AB"].manager.name, "André & Firm");
        assert_eq!(notices["AB"].manager.period.as_deref(), Some("2026 Q3"));
        assert_eq!(notices["AB"].timestamp, "2026-11-16T19:11:30+00:00");
    }

    #[test]
    fn parses_activity_group_with_missing_opening_tr() {
        let html = r#"<p id="f_name">Example</p><table id="grid"><thead><tr><th>Stock</th></tr></thead>
<tbody><tr class="q_chg"><td>Q3 &nbsp;<b>2026</b></td></tr><td></td>
<td><a href="/m/stock.php?sym=BRK.B">BRK.B<span> - Berkshire Hathaway CL B</span></a></td>
<td>Reduce 12.5%</td><td>1,200</td><td>0.20</td></tr></tbody></table>"#;
        let data = activities(html, 1).unwrap().changes;
        assert_eq!(data[0].ticker, "BRK.B");
        assert_eq!(data[0].period, "2026 Q3");
        assert_eq!(data[0].share_change, Some(-1200));
        assert_eq!(data[0].portfolio_impact, Some(-0.2));
    }
}
