//! Agenda data types and parsing utilities.

use anyhow::{Result, bail};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use log::{debug, warn};
use serde::Deserialize;

/// How far ahead to look for events.
#[derive(Debug, Clone)]
pub enum AgendaPeriod {
    Today,
    Duration(Duration),
}

/// Plugin configuration (deserialized from TOML).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AgendaConfig {
    pub period: String,
    pub refresh_interval: u64,
    pub lookahead: String,
}

impl Default for AgendaConfig {
    fn default() -> Self {
        Self {
            period: "today".to_string(),
            refresh_interval: 300,
            lookahead: String::new(),
        }
    }
}

/// A single calendar event.
#[derive(Debug, Clone)]
pub struct AgendaEvent {
    pub uid: String,
    pub summary: String,
    pub start_time: i64,
    pub end_time: i64,
    pub all_day: bool,
    pub description: Option<String>,
    /// HTML description from X-ALT-DESC property (Exchange/O365/Google).
    pub alt_description: Option<String>,
    pub location: Option<String>,
}

impl AgendaEvent {
    /// Unique key for storing this occurrence.
    ///
    /// Recurring events share the same UID but have different start times.
    /// Using `uid@start_time` prevents later-delivered old instances from
    /// overwriting today's occurrence in the store.
    pub fn occurrence_key(&self) -> String {
        format!("{}@{}", self.uid, self.start_time)
    }
}

/// Meeting link provider.
#[derive(Debug, Clone)]
pub enum MeetingProvider {
    GoogleMeet,
    Zoom,
    Teams,
}

/// A detected meeting link from an event.
#[derive(Debug, Clone)]
pub struct MeetingLink {
    pub url: String,
    pub provider: MeetingProvider,
}

/// A calendar source discovered from EDS.
#[derive(Debug, Clone)]
pub struct CalendarSource {
    pub uid: String,
    pub display_name: String,
}

/// Parse a period string into an `AgendaPeriod`.
///
/// Accepts `"today"` or an ISO 8601 duration like `"P3D"`.
/// Rejects durations longer than 31 days.
pub fn parse_period(input: &str) -> Result<AgendaPeriod> {
    let trimmed = input.trim().to_lowercase();
    if trimmed == "today" {
        return Ok(AgendaPeriod::Today);
    }
    let dur = parse_iso8601_duration(input)?;
    let max = Duration::days(31);
    if dur > max {
        bail!("Period exceeds maximum of 31 days (P1M)");
    }
    if dur <= Duration::zero() {
        bail!("Period must be positive");
    }
    Ok(AgendaPeriod::Duration(dur))
}

/// Extract meeting links from an event's description and location fields.
///
/// Uses substring scanning to find URLs even inside HTML attributes or
/// inline angle brackets (`text<URL>`).
pub fn extract_meeting_links(event: &AgendaEvent) -> Vec<MeetingLink> {
    let mut links = Vec::new();
    let fields: Vec<&str> = [
        event.description.as_deref(),
        event.alt_description.as_deref(),
        event.location.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();

    for text in fields {
        extract_urls_from_text(text, &mut links);
    }
    links
}

/// Known meeting URL anchors to scan for directly.
const MEETING_ANCHORS: &[(&str, MeetingProvider)] = &[
    ("https://meet.google.com/", MeetingProvider::GoogleMeet),
    ("https://teams.microsoft.com/meet/", MeetingProvider::Teams),
    ("https://teams.live.com/meet/", MeetingProvider::Teams),
];

/// Scan text for meeting URLs by looking for known anchors and extracting
/// the full URL until a termination character.
fn extract_urls_from_text(text: &str, links: &mut Vec<MeetingLink>) {
    // Scan for known anchors
    for &(anchor, ref provider) in MEETING_ANCHORS {
        let mut search_from = 0;
        while let Some(pos) = text[search_from..].find(anchor) {
            let abs_pos = search_from + pos;
            let url = extract_url_at(text, abs_pos);
            if !links.iter().any(|l| l.url == url) {
                links.push(MeetingLink {
                    url,
                    provider: provider.clone(),
                });
            }
            search_from = abs_pos + anchor.len();
        }
    }

    // Scan for Zoom: find all https:// occurrences and check for zoom.us/
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find("https://") {
        let abs_pos = search_from + pos;
        let url = extract_url_at(text, abs_pos);
        if url.contains("zoom.us/") {
            if !links.iter().any(|l| l.url == url) {
                links.push(MeetingLink {
                    url,
                    provider: MeetingProvider::Zoom,
                });
            }
        }
        // Advance past this https:// to avoid infinite loop
        search_from = abs_pos + "https://".len();
    }
}

/// Extract a URL starting at `start` in `text`, stopping at the first
/// termination character: `"`, `'`, `<`, `>`, `(`, `)`, or whitespace.
fn extract_url_at(text: &str, start: usize) -> String {
    let rest = &text[start..];
    let end = rest
        .find(|c: char| {
            c == '"'
                || c == '\''
                || c == '<'
                || c == '>'
                || c == '('
                || c == ')'
                || c.is_whitespace()
        })
        .unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Parse a subset of ISO 8601 durations: `P[nW] | P[nD][T[nH][nM][nS]]`.
///
/// Years and months are not supported (ambiguous day count).
pub fn parse_iso8601_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if !s.starts_with('P') && !s.starts_with('p') {
        bail!("ISO 8601 duration must start with 'P'");
    }
    let rest = &s[1..];

    let mut total = Duration::zero();
    let mut in_time = false;
    let mut buf = String::new();

    for ch in rest.chars() {
        match ch {
            'T' | 't' => {
                in_time = true;
            }
            '0'..='9' | '.' => {
                buf.push(ch);
            }
            'Y' | 'y' => {
                bail!("Year durations are not supported (ambiguous day count)");
            }
            'M' | 'm' if !in_time => {
                bail!("Month durations are not supported (ambiguous day count)");
            }
            'W' | 'w' => {
                let n: i64 = buf.parse().unwrap_or(0);
                total = total + Duration::weeks(n);
                buf.clear();
            }
            'D' | 'd' => {
                let n: i64 = buf.parse().unwrap_or(0);
                total = total + Duration::days(n);
                buf.clear();
            }
            'H' | 'h' => {
                let n: i64 = buf.parse().unwrap_or(0);
                total = total + Duration::hours(n);
                buf.clear();
            }
            'M' | 'm' => {
                // in_time == true
                let n: i64 = buf.parse().unwrap_or(0);
                total = total + Duration::minutes(n);
                buf.clear();
            }
            'S' | 's' => {
                let n: i64 = buf.parse().unwrap_or(0);
                total = total + Duration::seconds(n);
                buf.clear();
            }
            _ => {
                bail!("Unexpected character '{}' in ISO 8601 duration", ch);
            }
        }
    }

    Ok(total)
}

/// Compute the EDS query time range for a given period.
///
/// Returns `(since_utc_timestamp, until_utc_timestamp, next_period_start)`.
/// When a lookahead is provided and `now >= original_until - lookahead`,
/// extends `until` by one more period and returns `next_period_start = Some(original_until)`.
pub fn compute_time_range(
    period: &AgendaPeriod,
    lookahead: Option<&Duration>,
) -> (i64, i64, Option<i64>) {
    let now = Local::now();
    let today_midnight = now
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let since = Local
        .from_local_datetime(&today_midnight)
        .single()
        .unwrap_or(now)
        .timestamp();

    let period_duration = match period {
        AgendaPeriod::Today => Duration::days(1),
        AgendaPeriod::Duration(dur) => *dur,
    };

    let end_naive = today_midnight + period_duration;
    let original_until = Local
        .from_local_datetime(&end_naive)
        .single()
        .unwrap_or(now)
        .timestamp();

    // Check if lookahead should extend the range
    if let Some(la) = lookahead {
        let threshold = original_until - la.num_seconds();
        if now.timestamp() >= threshold {
            let extended_naive = end_naive + period_duration;
            let extended_until = Local
                .from_local_datetime(&extended_naive)
                .single()
                .unwrap_or(now)
                .timestamp();
            return (since, extended_until, Some(original_until));
        }
    }

    (since, original_until, None)
}

/// Format a UTC timestamp pair as the EDS S-expression time range query.
///
/// Produces: `(occur-in-time-range? (make-time "20250126T000000Z") (make-time "20250127T000000Z"))`
pub fn format_time_range_query(since: i64, until: i64) -> String {
    use chrono::Utc;
    let since_dt = Utc.timestamp_opt(since, 0).single().unwrap();
    let until_dt = Utc.timestamp_opt(until, 0).single().unwrap();
    format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\"))",
        since_dt.format("%Y%m%dT%H%M%SZ"),
        until_dt.format("%Y%m%dT%H%M%SZ")
    )
}

/// Parse a VEVENT from an iCalendar string.
///
/// This is a minimal line-by-line scanner — no external iCal crate needed.
pub fn parse_vevent(ical_str: &str) -> Option<AgendaEvent> {
    // Unfold continuation lines (RFC 5545: lines starting with space/tab are continuations)
    let unfolded = unfold_ical(ical_str);

    let mut in_vevent = false;
    // Nesting depth inside VEVENT: 0 = VEVENT level, >0 = inside VALARM/etc.
    let mut nest_depth: u32 = 0;
    let mut uid = None;
    let mut summary = None;
    let mut dtstart: Option<i64> = None;
    let mut dtend: Option<i64> = None;
    let mut dtstart_raw = String::new();
    let mut dtend_raw = String::new();
    let mut all_day = false;
    let mut description = None;
    let mut alt_description = None;
    let mut location = None;

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');

        if line == "BEGIN:VEVENT" {
            in_vevent = true;
            continue;
        }
        if line == "END:VEVENT" {
            break;
        }
        if !in_vevent {
            continue;
        }

        // Track nested components (VALARM, VTIMEZONE, etc.) — skip their
        // properties so e.g. a VALARM DESCRIPTION doesn't overwrite the
        // VEVENT DESCRIPTION.
        if line.starts_with("BEGIN:") {
            nest_depth += 1;
            continue;
        }
        if line.starts_with("END:") {
            nest_depth = nest_depth.saturating_sub(1);
            continue;
        }
        if nest_depth > 0 {
            continue;
        }

        if let Some(rest) = line.strip_prefix("UID:") {
            uid = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("SUMMARY:") {
            summary = Some(rest.to_string());
        } else if line.starts_with("DTSTART") {
            let (params, value) = split_ical_property(line, "DTSTART");
            if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") {
                all_day = true;
            }
            dtstart_raw = format!("{} [params: {}]", value, params);
            dtstart = parse_ical_datetime(&value, &params);
        } else if line.starts_with("DTEND") {
            let (params, value) = split_ical_property(line, "DTEND");
            dtend_raw = format!("{} [params: {}]", value, params);
            dtend = parse_ical_datetime(&value, &params);
        } else if line.starts_with("DESCRIPTION") {
            let (_params, value) = split_ical_property(line, "DESCRIPTION");
            if !value.is_empty() {
                description = Some(unescape_ical(&value));
            }
        } else if line.starts_with("X-ALT-DESC") {
            let (_params, value) = split_ical_property(line, "X-ALT-DESC");
            if !value.is_empty() {
                alt_description = Some(unescape_ical(&value));
            }
        } else if line.starts_with("LOCATION") {
            let (_params, value) = split_ical_property(line, "LOCATION");
            if !value.is_empty() {
                location = Some(unescape_ical(&value));
            }
        }
    }

    let uid = uid?;
    let summary = summary.unwrap_or_default();
    let start_time = dtstart?;
    let end_time = dtend.unwrap_or(start_time + 3600);

    debug!(
        "[agenda] parsed '{}' (uid={}): start={} (raw: {}) end={} (raw: {}) desc={:?} alt_desc={}chars loc={:?}",
        summary,
        uid,
        start_time,
        dtstart_raw,
        end_time,
        dtend_raw,
        description.as_deref().map(|d| &d[..d.len().min(80)]),
        alt_description.as_ref().map(|d| d.len()).unwrap_or(0),
        location.as_deref().map(|d| &d[..d.len().min(120)]),
    );

    Some(AgendaEvent {
        uid,
        summary,
        start_time,
        end_time,
        all_day,
        description,
        alt_description,
        location,
    })
}

/// Unfold iCal continuation lines (lines starting with a space or tab are appended
/// to the previous line).
fn unfold_ical(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for line in s.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation: append without the leading whitespace
            result.push_str(&line[1..]);
        } else {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line.trim_end_matches('\r'));
        }
    }
    result
}

/// Unescape iCal text values: `\\n` → newline, `\\,` → `,`, `\\;` → `;`, `\\\\` → `\\`.
fn unescape_ical(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split an iCal property line like `DTSTART;TZID=Europe/Prague:20250126T100000`
/// into (params_string, value_string).
fn split_ical_property(line: &str, prop_name: &str) -> (String, String) {
    // Strip the property name prefix
    let rest = &line[prop_name.len()..];

    if let Some(colon_pos) = rest.find(':') {
        let params = rest[..colon_pos].to_string();
        let value = rest[colon_pos + 1..].to_string();
        (params, value)
    } else {
        (String::new(), rest.to_string())
    }
}

/// Extract the TZID value from iCal property parameters.
///
/// Parses `;TZID=Europe/Prague` or `;TZID="America/New_York"` from the params string.
/// Returns `None` if no TZID is present.
fn extract_tzid(params: &str) -> Option<String> {
    // Look for TZID= in the params (may be preceded by ; or at start)
    let tzid_start = params.find("TZID=")?;
    let after_key = &params[tzid_start + 5..];

    if after_key.starts_with('"') {
        // Quoted value: TZID="America/New_York"
        let end = after_key[1..].find('"')?;
        Some(after_key[1..1 + end].to_string())
    } else {
        // Unquoted: take until next ; or end of string
        let end = after_key.find(';').unwrap_or(after_key.len());
        let val = after_key[..end].trim();
        if val.is_empty() {
            None
        } else {
            Some(val.to_string())
        }
    }
}

/// Parse an iCal datetime value into a UTC timestamp.
///
/// Handles:
/// - `VALUE=DATE` format: `20250126` → midnight local time
/// - UTC format: `20250126T100000Z` → direct UTC
/// - `TZID=...` format: `20250126T100000` with timezone → UTC conversion
/// - Plain format: `20250126T100000` → local time
fn parse_ical_datetime(value: &str, params: &str) -> Option<i64> {
    let value = value.trim();

    // VALUE=DATE: date only
    if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        let dt = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
        return Some(
            Local
                .from_local_datetime(&dt)
                .single()
                .map(|d| d.timestamp())
                .unwrap_or_else(|| chrono::Utc.from_utc_datetime(&dt).timestamp()),
        );
    }

    // UTC: ends with Z
    if value.ends_with('Z') {
        let without_z = &value[..value.len() - 1];
        let dt = NaiveDateTime::parse_from_str(without_z, "%Y%m%dT%H%M%S").ok()?;
        return Some(chrono::Utc.from_utc_datetime(&dt).timestamp());
    }

    // Try TZID-based conversion
    if let Some(tzid_str) = extract_tzid(params) {
        if let Ok(tz) = tzid_str.parse::<chrono_tz::Tz>() {
            let dt = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
            let converted = tz
                .from_local_datetime(&dt)
                .earliest()
                .map(|d| d.timestamp());
            if let Some(ts) = converted {
                return Some(ts);
            }
            // DST gap — fall through to local time
        } else {
            warn!(
                "[agenda] Unknown TZID '{}', falling back to local time",
                tzid_str
            );
        }
    }

    // Plain local time fallback
    let dt = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    Some(
        Local
            .from_local_datetime(&dt)
            .single()
            .map(|d| d.timestamp())
            .unwrap_or_else(|| chrono::Utc.from_utc_datetime(&dt).timestamp()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an AgendaEvent with optional description/location.
    fn make_event(desc: Option<&str>, loc: Option<&str>) -> AgendaEvent {
        AgendaEvent {
            uid: "test-uid".to_string(),
            summary: "Test Event".to_string(),
            start_time: 1700000000,
            end_time: 1700003600,
            all_day: false,
            description: desc.map(|s| s.to_string()),
            alt_description: None,
            location: loc.map(|s| s.to_string()),
        }
    }

    /// Helper: create an AgendaEvent with an alt_description (HTML from X-ALT-DESC).
    fn make_event_with_alt_desc(alt_desc: &str) -> AgendaEvent {
        AgendaEvent {
            uid: "test-uid".to_string(),
            summary: "Test Event".to_string(),
            start_time: 1700000000,
            end_time: 1700003600,
            all_day: false,
            description: None,
            alt_description: Some(alt_desc.to_string()),
            location: None,
        }
    }

    // ── parse_iso8601_duration ──────────────────────────────────────

    #[test]
    fn parse_duration_hours() {
        let dur = parse_iso8601_duration("PT8H").unwrap();
        assert_eq!(dur, Duration::hours(8));
    }

    #[test]
    fn parse_duration_days() {
        let dur = parse_iso8601_duration("P3D").unwrap();
        assert_eq!(dur, Duration::days(3));
    }

    #[test]
    fn parse_duration_minutes() {
        let dur = parse_iso8601_duration("PT30M").unwrap();
        assert_eq!(dur, Duration::minutes(30));
    }

    #[test]
    fn parse_duration_weeks() {
        let dur = parse_iso8601_duration("P2W").unwrap();
        assert_eq!(dur, Duration::weeks(2));
    }

    #[test]
    fn parse_duration_combined_day_and_time() {
        let dur = parse_iso8601_duration("P1DT12H").unwrap();
        assert_eq!(dur, Duration::days(1) + Duration::hours(12));
    }

    #[test]
    fn parse_duration_hours_and_minutes() {
        let dur = parse_iso8601_duration("PT1H30M").unwrap();
        assert_eq!(dur, Duration::hours(1) + Duration::minutes(30));
    }

    #[test]
    fn parse_duration_seconds() {
        let dur = parse_iso8601_duration("PT90S").unwrap();
        assert_eq!(dur, Duration::seconds(90));
    }

    #[test]
    fn parse_duration_lowercase() {
        let dur = parse_iso8601_duration("p1dt2h30m").unwrap();
        assert_eq!(
            dur,
            Duration::days(1) + Duration::hours(2) + Duration::minutes(30)
        );
    }

    #[test]
    fn parse_duration_no_p_prefix_fails() {
        assert!(parse_iso8601_duration("T8H").is_err());
    }

    #[test]
    fn parse_duration_year_fails() {
        assert!(parse_iso8601_duration("P1Y").is_err());
    }

    #[test]
    fn parse_duration_month_fails() {
        assert!(parse_iso8601_duration("P1M").is_err());
    }

    #[test]
    fn parse_duration_invalid_char_fails() {
        assert!(parse_iso8601_duration("P1X").is_err());
    }

    // ── unfold_ical ────────────────────────────────────────────────

    #[test]
    fn unfold_no_continuations() {
        let input = "LINE1\nLINE2\nLINE3";
        assert_eq!(unfold_ical(input), "LINE1\nLINE2\nLINE3");
    }

    #[test]
    fn unfold_space_continuation() {
        let input = "DESCRIPTION:This is a long\n description that wraps";
        assert_eq!(
            unfold_ical(input),
            "DESCRIPTION:This is a longdescription that wraps"
        );
    }

    #[test]
    fn unfold_tab_continuation() {
        let input = "DESCRIPTION:Start\n\tcontinued";
        assert_eq!(unfold_ical(input), "DESCRIPTION:Startcontinued");
    }

    #[test]
    fn unfold_multiple_continuations() {
        let input = "DESC:A\n B\n C\nNEXT:D";
        assert_eq!(unfold_ical(input), "DESC:ABC\nNEXT:D");
    }

    // ── unescape_ical ──────────────────────────────────────────────

    #[test]
    fn unescape_newline_lowercase() {
        assert_eq!(unescape_ical("hello\\nworld"), "hello\nworld");
    }

    #[test]
    fn unescape_newline_uppercase() {
        assert_eq!(unescape_ical("hello\\Nworld"), "hello\nworld");
    }

    #[test]
    fn unescape_backslash() {
        assert_eq!(unescape_ical("back\\\\slash"), "back\\slash");
    }

    #[test]
    fn unescape_comma() {
        assert_eq!(unescape_ical("a\\,b"), "a,b");
    }

    #[test]
    fn unescape_semicolon() {
        assert_eq!(unescape_ical("a\\;b"), "a;b");
    }

    #[test]
    fn unescape_trailing_backslash() {
        assert_eq!(unescape_ical("trail\\"), "trail\\");
    }

    #[test]
    fn unescape_no_escapes() {
        assert_eq!(unescape_ical("plain text"), "plain text");
    }

    #[test]
    fn unescape_unknown_escape_preserved() {
        assert_eq!(unescape_ical("a\\xb"), "a\\xb");
    }

    // ── extract_meeting_links ──────────────────────────────────────

    #[test]
    fn extract_google_meet_from_description() {
        let event = make_event(Some("Join at https://meet.google.com/abc-defg-hij"), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://meet.google.com/abc-defg-hij");
        assert!(matches!(links[0].provider, MeetingProvider::GoogleMeet));
    }

    #[test]
    fn extract_zoom_from_location() {
        let event = make_event(None, Some("https://us02web.zoom.us/j/123456789?pwd=abc"));
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Zoom));
    }

    #[test]
    fn extract_teams_microsoft_com() {
        let event = make_event(Some("https://teams.microsoft.com/meet/abc"), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
    }

    #[test]
    fn extract_teams_live_com() {
        let event = make_event(Some("https://teams.live.com/meet/abc123"), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
    }

    #[test]
    fn extract_no_meeting_links() {
        let event = make_event(Some("Regular meeting in the office"), Some("Room 301"));
        let links = extract_meeting_links(&event);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_no_fields_no_links() {
        let event = make_event(None, None);
        let links = extract_meeting_links(&event);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_multiple_providers() {
        let event = make_event(
            Some("Meet: https://meet.google.com/abc Zoom: https://zoom.us/j/123"),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn extract_deduplicates_same_url() {
        let event = make_event(
            Some("https://meet.google.com/abc-def-ghi"),
            Some("https://meet.google.com/abc-def-ghi"),
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn extract_link_in_angle_brackets() {
        let event = make_event(Some("Join: <https://meet.google.com/abc-def-ghi>"), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://meet.google.com/abc-def-ghi");
    }

    #[test]
    fn extract_link_in_quotes() {
        let event = make_event(Some("Link: \"https://zoom.us/j/999\""), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(links[0].url.starts_with("https://"));
    }

    #[test]
    fn extract_ignores_http_zoom() {
        // Only https should match
        let event = make_event(Some("http://zoom.us/j/123"), None);
        let links = extract_meeting_links(&event);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_ignores_teams_non_meet_urls() {
        // Only /meet/ path should match, not /l/meetup-join/ or /meetingOptions/
        let event = make_event(
            Some(
                "https://teams.microsoft.com/l/meetup-join/abc https://teams.microsoft.com/meetingOptions/?org=123",
            ),
            None,
        );
        let links = extract_meeting_links(&event);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_from_both_description_and_location() {
        let event = make_event(
            Some("https://meet.google.com/aaa-bbb-ccc"),
            Some("https://zoom.us/j/111"),
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 2);
    }

    // ── parse_vevent ───────────────────────────────────────────────

    #[test]
    fn parse_vevent_basic_utc() {
        let ical = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
UID:evt-001\r
SUMMARY:Team Standup\r
DTSTART:20250126T100000Z\r
DTEND:20250126T103000Z\r
END:VEVENT\r
END:VCALENDAR";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.uid, "evt-001");
        assert_eq!(event.summary, "Team Standup");
        assert!(!event.all_day);
        assert!(event.start_time < event.end_time);
        assert_eq!(event.end_time - event.start_time, 1800); // 30 minutes
        assert!(event.description.is_none());
        assert!(event.location.is_none());
    }

    #[test]
    fn parse_vevent_with_description() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-002\r
SUMMARY:Planning\r
DTSTART:20250126T140000Z\r
DTEND:20250126T150000Z\r
DESCRIPTION:Sprint planning for Q1\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.description.as_deref(), Some("Sprint planning for Q1"));
    }

    #[test]
    fn parse_vevent_with_location() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-003\r
SUMMARY:Interview\r
DTSTART:20250126T160000Z\r
DTEND:20250126T170000Z\r
LOCATION:Room 42\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.location.as_deref(), Some("Room 42"));
    }

    #[test]
    fn parse_vevent_with_description_and_location() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-004\r
SUMMARY:All Hands\r
DTSTART:20250126T180000Z\r
DTEND:20250126T190000Z\r
DESCRIPTION:Quarterly review\r
LOCATION:https://meet.google.com/abc-def-ghi\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.description.as_deref(), Some("Quarterly review"));
        assert_eq!(
            event.location.as_deref(),
            Some("https://meet.google.com/abc-def-ghi")
        );
    }

    #[test]
    fn parse_vevent_folded_description() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-005\r
SUMMARY:Long Desc\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
DESCRIPTION:This is a very long description that has been\r
 folded across multiple lines according to RFC 5545\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        let desc = event.description.unwrap();
        assert!(desc.contains("very long description"));
        assert!(desc.contains("folded across multiple lines"));
        // The fold should be removed (no literal newline from unfolding)
        assert!(!desc.contains('\n'));
    }

    #[test]
    fn parse_vevent_escaped_description() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-006\r
SUMMARY:Escaped\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
DESCRIPTION:Line one\\nLine two\\, with comma\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        let desc = event.description.unwrap();
        assert_eq!(desc, "Line one\nLine two, with comma");
    }

    #[test]
    fn parse_vevent_all_day() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-007\r
SUMMARY:Holiday\r
DTSTART;VALUE=DATE:20250126\r
DTEND;VALUE=DATE:20250127\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert!(event.all_day);
    }

    #[test]
    fn parse_vevent_missing_uid_returns_none() {
        let ical = "\
BEGIN:VEVENT\r
SUMMARY:No UID\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
END:VEVENT";

        assert!(parse_vevent(ical).is_none());
    }

    #[test]
    fn parse_vevent_missing_dtstart_returns_none() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-missing-start\r
SUMMARY:No Start\r
DTEND:20250126T110000Z\r
END:VEVENT";

        assert!(parse_vevent(ical).is_none());
    }

    #[test]
    fn parse_vevent_missing_dtend_defaults_to_one_hour() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-no-end\r
SUMMARY:No End\r
DTSTART:20250126T100000Z\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.end_time - event.start_time, 3600);
    }

    #[test]
    fn parse_vevent_description_with_params() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-008\r
SUMMARY:Parameterized\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
DESCRIPTION;LANGUAGE=en:English description\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(event.description.as_deref(), Some("English description"));
    }

    #[test]
    fn parse_vevent_meeting_link_in_description() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-meet\r
SUMMARY:Video Call\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
DESCRIPTION:Join at https://meet.google.com/abc-def-ghi\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::GoogleMeet));
    }

    // ── compute_time_range ─────────────────────────────────────────

    #[test]
    fn compute_time_range_today_no_lookahead() {
        let (since, until, nps) = compute_time_range(&AgendaPeriod::Today, None);
        assert!(since < until, "since must be before until");
        // Period is 1 day = 86400 seconds (may differ slightly around DST changes)
        let diff = until - since;
        assert!(
            (86000..=90000).contains(&diff),
            "Today period should be ~86400s, got {}",
            diff
        );
        assert!(nps.is_none(), "No lookahead means no next_period_start");
    }

    #[test]
    fn compute_time_range_duration_no_lookahead() {
        let period = AgendaPeriod::Duration(Duration::days(3));
        let (since, until, nps) = compute_time_range(&period, None);
        assert!(since < until);
        let diff = until - since;
        let expected = 3 * 86400;
        assert!(
            ((expected - 7200)..=(expected + 7200)).contains(&diff),
            "3-day period should be ~{}s, got {}",
            expected,
            diff
        );
        assert!(nps.is_none());
    }

    #[test]
    fn compute_time_range_since_is_midnight_today() {
        let (since, _, _) = compute_time_range(&AgendaPeriod::Today, None);
        let now = Local::now();
        // since should be <= now (it's midnight of today)
        assert!(since <= now.timestamp());
        // since should be within the last 24 hours
        assert!(now.timestamp() - since < 86400);
    }

    #[test]
    fn compute_time_range_with_large_lookahead_extends() {
        // A lookahead of 25 hours guarantees we're always within the lookahead
        // window for a 1-day period (since 25h > 24h).
        let la = Duration::hours(25);
        let (since, until, nps) = compute_time_range(&AgendaPeriod::Today, Some(&la));
        assert!(since < until);
        // Should be extended to 2 days
        let diff = until - since;
        let expected = 2 * 86400;
        assert!(
            ((expected - 7200)..=(expected + 7200)).contains(&diff),
            "Extended period should be ~{}s, got {}",
            expected,
            diff
        );
        assert!(
            nps.is_some(),
            "Lookahead within window should set next_period_start"
        );
        let nps = nps.unwrap();
        assert!(nps > since);
        assert!(nps < until);
    }

    #[test]
    fn compute_time_range_with_zero_lookahead_never_extends() {
        let la = Duration::zero();
        let (_, _, nps) = compute_time_range(&AgendaPeriod::Today, Some(&la));
        // zero lookahead means threshold = until - 0 = until, now < until always
        assert!(nps.is_none());
    }

    // ── format_time_range_query ────────────────────────────────────

    #[test]
    fn format_time_range_query_produces_eds_sexp() {
        // 2025-01-26 00:00:00 UTC = 1737849600
        // 2025-01-27 00:00:00 UTC = 1737936000
        let query = format_time_range_query(1737849600, 1737936000);
        assert!(query.starts_with("(occur-in-time-range?"));
        assert!(query.contains("make-time"));
        assert!(query.contains("20250126T000000Z"));
        assert!(query.contains("20250127T000000Z"));
    }

    // ── parse_period ───────────────────────────────────────────────

    #[test]
    fn parse_period_today() {
        let period = parse_period("today").unwrap();
        assert!(matches!(period, AgendaPeriod::Today));
    }

    #[test]
    fn parse_period_today_case_insensitive() {
        let period = parse_period("TODAY").unwrap();
        assert!(matches!(period, AgendaPeriod::Today));
    }

    #[test]
    fn parse_period_duration() {
        let period = parse_period("P7D").unwrap();
        assert!(matches!(period, AgendaPeriod::Duration(_)));
    }

    #[test]
    fn parse_period_too_long_fails() {
        assert!(parse_period("P60D").is_err());
    }

    // ── AgendaConfig default ───────────────────────────────────────

    #[test]
    fn agenda_config_default_has_empty_lookahead() {
        let config = AgendaConfig::default();
        assert_eq!(config.period, "today");
        assert_eq!(config.refresh_interval, 300);
        assert!(config.lookahead.is_empty());
    }

    // ── extract_tzid ─────────────────────────────────────────────

    #[test]
    fn extract_tzid_basic() {
        assert_eq!(
            extract_tzid(";TZID=Europe/Prague"),
            Some("Europe/Prague".to_string())
        );
    }

    #[test]
    fn extract_tzid_quoted() {
        assert_eq!(
            extract_tzid(";TZID=\"America/New_York\""),
            Some("America/New_York".to_string())
        );
    }

    #[test]
    fn extract_tzid_with_other_params() {
        assert_eq!(
            extract_tzid(";VALUE=DATE-TIME;TZID=UTC"),
            Some("UTC".to_string())
        );
    }

    #[test]
    fn extract_tzid_missing() {
        assert_eq!(extract_tzid(";VALUE=DATE"), None);
    }

    #[test]
    fn extract_tzid_empty_value() {
        assert_eq!(extract_tzid(""), None);
    }

    // ── parse_ical_datetime with TZID ────────────────────────────

    #[test]
    fn parse_ical_datetime_with_tzid_utc() {
        // TZID=UTC, 2025-01-26T10:00:00 → should be same as Z suffix
        let ts_tzid = parse_ical_datetime("20250126T100000", ";TZID=UTC").unwrap();
        let ts_z = parse_ical_datetime("20250126T100000Z", "").unwrap();
        assert_eq!(ts_tzid, ts_z);
    }

    #[test]
    fn parse_ical_datetime_with_tzid_europe_prague() {
        // Europe/Prague is UTC+1 in winter (CET)
        // 2025-01-26T10:00:00 Europe/Prague = 2025-01-26T09:00:00 UTC
        let ts = parse_ical_datetime("20250126T100000", ";TZID=Europe/Prague").unwrap();
        let utc_ts = parse_ical_datetime("20250126T090000Z", "").unwrap();
        assert_eq!(ts, utc_ts);
    }

    #[test]
    fn parse_ical_datetime_with_tzid_america_new_york() {
        // America/New_York is UTC-5 in winter (EST)
        // 2025-01-26T10:00:00 EST = 2025-01-26T15:00:00 UTC
        let ts = parse_ical_datetime("20250126T100000", ";TZID=America/New_York").unwrap();
        let utc_ts = parse_ical_datetime("20250126T150000Z", "").unwrap();
        assert_eq!(ts, utc_ts);
    }

    // ── extract_meeting_links: HTML and inline formats ───────────

    #[test]
    fn extract_zoom_from_html_description() {
        let event = make_event(
            Some(
                r#"<b>Zoom:</b><br><a href="https://us06web.zoom.us/j/86257749546?pwd=abc"><u>https://us06web.zoom.us/j/86257749546?pwd=abc</u></a>"#,
            ),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Zoom));
        assert_eq!(
            links[0].url,
            "https://us06web.zoom.us/j/86257749546?pwd=abc"
        );
    }

    #[test]
    fn extract_teams_from_inline_angle_brackets() {
        // Teams URL embedded as text<URL> without whitespace before <
        let event = make_event(
            Some(
                "Need help?<https://aka.ms/JoinTeamsMeeting> | Join meeting<https://teams.microsoft.com/meet/abc123>",
            ),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
        assert_eq!(links[0].url, "https://teams.microsoft.com/meet/abc123");
    }

    #[test]
    fn extract_teams_primary_url_from_description() {
        // Regression: whitespace-separated Teams URL still works
        let event = make_event(
            Some("Join: https://teams.microsoft.com/meet/12345?p=abc"),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
    }

    #[test]
    fn extract_google_meet_from_czech_description() {
        // Regression: meet.txt format with Czech text
        let event = make_event(
            Some(
                "Připojte se přes Google Meet: https://meet.google.com/cyz-ksav-zba\nNebo zavolejte na: (CZ) +420 234 610 901",
            ),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::GoogleMeet));
        assert_eq!(links[0].url, "https://meet.google.com/cyz-ksav-zba");
    }

    #[test]
    fn extract_deduplicates_zoom_in_href_and_text() {
        // Same Zoom URL appears in href attribute and as anchor text
        let event = make_event(
            Some(
                r#"<a href="https://us06web.zoom.us/j/123?pwd=abc">https://us06web.zoom.us/j/123?pwd=abc</a>"#,
            ),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Zoom));
    }

    // ── X-ALT-DESC (HTML description) extraction ────────────────

    #[test]
    fn extract_zoom_from_alt_description_html() {
        // Zoom URL in X-ALT-DESC HTML content (description is None)
        let event = make_event_with_alt_desc(
            r#"<b>Zoom:</b><br><a href="https://us06web.zoom.us/j/86257749546?pwd=abc"><u>https://us06web.zoom.us/j/86257749546?pwd=abc</u></a>"#,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Zoom));
    }

    #[test]
    fn extract_teams_from_alt_description_html() {
        let event = make_event_with_alt_desc(
            r#"<a href="https://teams.microsoft.com/meet/abc123">Join Teams</a>"#,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
    }

    #[test]
    fn parse_vevent_with_x_alt_desc() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-alt\r
SUMMARY:Alt Desc Event\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
X-ALT-DESC;FMTTYPE=text/html:<html><body><a href=\"https://zoom.us/j/999\">Join</a></body></html>\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert!(event.alt_description.is_some());
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Zoom));
    }

    #[test]
    fn parse_vevent_x_alt_desc_with_colons_in_html() {
        // X-ALT-DESC value contains colons (common in HTML) — split_ical_property
        // must only split on the FIRST colon after params.
        let ical = "\
BEGIN:VEVENT\r
UID:evt-colon\r
SUMMARY:Colon Test\r
DTSTART:20250126T100000Z\r
DTEND:20250126T110000Z\r
X-ALT-DESC;FMTTYPE=text/html:<html><body>Time: 10:00<br><a href=\"https://meet.google.com/abc\">Meet</a></body></html>\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        let alt = event.alt_description.unwrap();
        assert!(
            alt.contains("https://meet.google.com/abc"),
            "alt_description should contain the meet URL, got: {}",
            alt
        );
    }

    // ── VALARM nesting ──────────────────────────────────────────

    #[test]
    fn parse_vevent_valarm_description_does_not_overwrite_event_description() {
        let ical = "\
BEGIN:VCALENDAR\r
BEGIN:VEVENT\r
UID:evt-valarm\r
SUMMARY:Teams Call\r
DTSTART;TZID=Europe/Prague:20260127T130000\r
DTEND;TZID=Europe/Prague:20260127T140000\r
DESCRIPTION:Join: https://teams.microsoft.com/meet/123\r
LOCATION:Microsoft Teams Meeting\r
BEGIN:VALARM\r
ACTION:DISPLAY\r
DESCRIPTION:This is an event reminder\r
TRIGGER:-PT15M\r
END:VALARM\r
END:VEVENT\r
END:VCALENDAR";

        let event = parse_vevent(ical).unwrap();
        // The VEVENT DESCRIPTION must survive — VALARM's must not overwrite it
        assert_eq!(
            event.description.as_deref(),
            Some("Join: https://teams.microsoft.com/meet/123"),
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert!(matches!(links[0].provider, MeetingProvider::Teams));
    }

    #[test]
    fn parse_vevent_without_valarm_keeps_description() {
        let ical = "\
BEGIN:VEVENT\r
UID:evt-no-alarm\r
SUMMARY:Simple Event\r
DTSTART:20260127T100000Z\r
DTEND:20260127T110000Z\r
DESCRIPTION:https://meet.google.com/abc-def-ghi\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert_eq!(
            event.description.as_deref(),
            Some("https://meet.google.com/abc-def-ghi"),
        );
    }

    #[test]
    fn parse_vevent_no_description_with_valarm() {
        // If VEVENT has no DESCRIPTION but VALARM does, description should be None
        let ical = "\
BEGIN:VEVENT\r
UID:evt-alarm-only\r
SUMMARY:Quick Sync\r
DTSTART:20260127T100000Z\r
DTEND:20260127T103000Z\r
BEGIN:VALARM\r
ACTION:DISPLAY\r
DESCRIPTION:This is an event reminder\r
TRIGGER:-PT10M\r
END:VALARM\r
END:VEVENT";

        let event = parse_vevent(ical).unwrap();
        assert!(
            event.description.is_none(),
            "VALARM description should not leak into event: {:?}",
            event.description
        );
    }
}
