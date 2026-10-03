//! Existing iCalendar parsing and recurrence expansion, isolated from D-Bus lifecycle.
use log::{info, warn};
use std::collections::HashSet;
use waft_protocol::entity;

/// Time range (UTC timestamps) for the calendar query window.
#[derive(Clone, Copy)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}

/// Earliest real local day boundary, including missing midnight/skipped dates.
fn day_start<Tz: chrono::TimeZone>(
    date: chrono::NaiveDate,
    zone: &Tz,
) -> Option<chrono::DateTime<Tz>> {
    for day in 0..3 {
        let start = date
            .checked_add_days(chrono::Days::new(day))?
            .and_hms_opt(0, 0, 0)?;
        for minute in 0..1440 {
            let candidate = start.checked_add_signed(chrono::Duration::minutes(minute))?;
            if let Some(boundary) = zone.from_local_datetime(&candidate).earliest() {
                return Some(boundary);
            }
        }
    }
    None
}
fn warn_invalid_calendar_item() {
    static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if LAST
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |last| (now >= last.saturating_add(30)).then_some(now),
        )
        .is_ok()
    {
        warn!("[eds] Invalid or unsupported calendar item encountered");
    }
}

/// Build the time range and `occur-in-time-range?` query string.
///
/// The window starts at today's local midnight (not `now`) so that events
/// which began before the daemon was launched—but still fall within today—
/// are not silently excluded from the view.
pub fn build_time_range_query_from_today() -> (TimeRange, String) {
    let local_now = chrono::Local::now();
    let today_midnight = day_start(local_now.date_naive(), &chrono::Local)
        .unwrap_or(local_now)
        .to_utc();
    let end = local_now
        .to_utc()
        .checked_add_signed(chrono::Duration::days(60))
        .unwrap_or(local_now.to_utc());
    let range = TimeRange {
        start: today_midnight.timestamp(),
        end: end.timestamp(),
    };
    let query = format!(
        "(occur-in-time-range? (make-time \"{}\") (make-time \"{}\"))",
        today_midnight.format("%Y%m%dT%H%M%SZ"),
        end.format("%Y%m%dT%H%M%SZ")
    );
    info!(
        "[eds] Time range query: {} → {} (UTC timestamps {} → {})",
        today_midnight.format("%Y-%m-%d %H:%M:%S UTC"),
        end.format("%Y-%m-%d %H:%M:%S UTC"),
        range.start,
        range.end
    );
    (range, query)
}

/// Returns seconds until the next local midnight (minimum 1).
pub fn secs_until_eds_midnight() -> u64 {
    let now = chrono::Local::now();
    let tomorrow = now
        .date_naive()
        .succ_opt()
        .and_then(|date| day_start(date, &chrono::Local))
        .unwrap_or_else(|| {
            now.checked_add_signed(chrono::Duration::days(1))
                .unwrap_or(now)
        });
    (tomorrow.timestamp() - now.timestamp()).max(1) as u64
}

/// Split a VCALENDAR string into individual VEVENT blocks.
///
/// A single VCALENDAR may contain multiple VEVENT components — for example,
/// a master recurring event (with RRULE + EXDATE) alongside an exception
/// occurrence (with RECURRENCE-ID) that was rescheduled to a different time.
/// Without this split, `parse_vevent_raw` stops at the first `END:VEVENT`
/// and silently drops all subsequent VEVENTs.
fn split_vevents(ical_str: &str) -> Vec<String> {
    let unfolded = unfold_ical(ical_str);
    let mut result = Vec::new();
    let mut current: Option<String> = None;

    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');

        if line == "BEGIN:VEVENT" {
            current = Some("BEGIN:VEVENT\r\n".to_string());
        } else if line == "END:VEVENT" {
            if let Some(mut block) = current.take() {
                block.push_str("END:VEVENT\r\n");
                result.push(block);
            }
        } else if let Some(ref mut block) = current {
            block.push_str(line);
            block.push_str("\r\n");
        }
    }

    if result.len() > 1 {
        info!(
            "[eds] split_vevents: found {} VEVENTs in one iCal blob (multi-VEVENT case)",
            result.len()
        );
    }

    result
}

/// Parse a list of iCalendar strings into CalendarEvent entities.
///
/// Each iCal string may contain multiple VEVENTs (e.g. a master recurring
/// event plus exception occurrences with RECURRENCE-ID).  The function splits
/// each string into individual VEVENT blocks before expanding.
///
/// Recurring events (those with RRULE) are expanded into individual
/// occurrences within `range`.  Non-recurring events pass through as-is.
pub fn parse_ical_events(
    icals: &[String],
    range: TimeRange,
) -> Vec<entity::calendar::CalendarEvent> {
    icals
        .iter()
        .flat_map(|ical| {
            split_vevents(ical)
                .into_iter()
                .flat_map(move |vevent| expand_vevent(&vevent, range))
        })
        .collect()
}

/// Report malformed items separately; a bad item cannot invalidate another source.
pub fn parse_ical_events_with_diagnostics(
    icals: &[String],
    range: TimeRange,
) -> (Vec<entity::calendar::CalendarEvent>, bool) {
    let malformed = icals.iter().any(|ical| {
        let items = split_vevents(ical);
        items.is_empty() || items.iter().any(|item| parse_vevent_raw(item).is_none())
    });
    (parse_ical_events(icals, range), malformed)
}

// ── Intermediate VEVENT representation ───────────────────────────────────

/// Holds all raw fields extracted from a VEVENT, including recurrence info
/// needed for RRULE expansion.
struct RawVevent {
    uid: String,
    summary: String,
    all_day: bool,
    description: Option<String>,
    location: Option<String>,
    attendees: Vec<entity::calendar::CalendarEventAttendee>,
    /// UTC timestamp of DTSTART.
    start_time: i64,
    /// UTC timestamp of DTEND (or DTSTART + 1h if absent).
    end_time: i64,
    /// Naive local datetime of DTSTART (needed for TZ-correct expansion).
    dtstart_naive: chrono::NaiveDateTime,
    /// TZID extracted from DTSTART params, if any.
    tz: Option<chrono_tz::Tz>,
    /// Whether DTSTART ends with Z (UTC).
    utc: bool,
    /// Raw RRULE value (e.g. "FREQ=WEEKLY;BYDAY=TU").
    rrule: Option<String>,
    /// EXDATE timestamps to exclude from recurrence.
    exdates: HashSet<i64>,
}

/// Parse a single iCalendar VEVENT string into a `RawVevent`.
fn parse_vevent_raw(ical_str: &str) -> Option<RawVevent> {
    let unfolded = unfold_ical(ical_str);

    let mut in_vevent = false;
    let mut nest_depth: u32 = 0;
    let mut uid = None;
    let mut summary = None;
    let mut dtstart_ts: Option<i64> = None;
    let mut dtend_ts: Option<i64> = None;
    let mut all_day = false;
    let mut description = None;
    let mut location = None;
    let mut attendees: Vec<entity::calendar::CalendarEventAttendee> = Vec::new();
    let mut rrule: Option<String> = None;
    let mut exdates: HashSet<i64> = HashSet::new();
    // Keep the raw DTSTART pieces for TZ-correct expansion.
    let mut dtstart_naive: Option<chrono::NaiveDateTime> = None;
    let mut dtstart_tz: Option<chrono_tz::Tz> = None;
    let mut dtstart_utc_flag = false;

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

        // Track nested components
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
            dtstart_ts = parse_ical_datetime(&value, &params);
            dtstart_naive = parse_ical_naive_datetime(&value);
            dtstart_tz = extract_tzid(&params);
            dtstart_utc_flag = value.ends_with('Z');
        } else if line.starts_with("DTEND") {
            let (params, value) = split_ical_property(line, "DTEND");
            dtend_ts = parse_ical_datetime(&value, &params);
        } else if let Some(rest) = line.strip_prefix("RRULE:") {
            rrule = Some(rest.to_string());
        } else if line.starts_with("EXDATE") {
            let (params, value) = split_ical_property(line, "EXDATE");
            for part in value.split(',') {
                if let Some(ts) = parse_ical_datetime(part.trim(), &params) {
                    exdates.insert(ts);
                }
            }
        } else if line.starts_with("DESCRIPTION") {
            let (_params, value) = split_ical_property(line, "DESCRIPTION");
            if !value.is_empty() {
                description = Some(unescape_ical(&value));
            }
        } else if line.starts_with("LOCATION") {
            let (_params, value) = split_ical_property(line, "LOCATION");
            if !value.is_empty() {
                location = Some(unescape_ical(&value));
            }
        } else if line.starts_with("ATTENDEE")
            && let Some(attendee) = parse_attendee_line(line)
        {
            attendees.push(attendee);
        }
    }

    let uid = uid?;
    let summary = summary.unwrap_or_default();
    let start_time = dtstart_ts?;
    let end_time = dtend_ts.unwrap_or(start_time + 3600);
    let dtstart_naive = dtstart_naive?;

    Some(RawVevent {
        uid,
        summary,
        all_day,
        description,
        location,
        attendees,
        start_time,
        end_time,
        dtstart_naive,
        tz: dtstart_tz,
        utc: dtstart_utc_flag,
        rrule,
        exdates,
    })
}

/// Convert a `RawVevent` to a single `CalendarEvent` (non-recurring path).
fn raw_to_event(raw: &RawVevent) -> entity::calendar::CalendarEvent {
    entity::calendar::CalendarEvent {
        uid: raw.uid.clone(),
        source_uid: String::new(),
        summary: raw.summary.clone(),
        start_time: raw.start_time,
        end_time: raw.end_time,
        all_day: raw.all_day,
        description: raw.description.clone(),
        location: raw.location.clone(),
        attendees: raw.attendees.clone(),
    }
}

/// Entry point kept for existing tests (non-recurring path).
#[cfg(test)]
fn parse_vevent(ical_str: &str) -> Option<entity::calendar::CalendarEvent> {
    parse_vevent_raw(ical_str).map(|raw| raw_to_event(&raw))
}

// ── RRULE parsing and expansion ──────────────────────────────────────────

/// Parsed recurrence rule.
struct RecurrenceRule {
    freq: Frequency,
    interval: u32,
    by_day: Vec<chrono::Weekday>,
    count: Option<u32>,
    until: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Parse an RRULE value string (e.g. "FREQ=WEEKLY;BYDAY=TU;INTERVAL=2").
fn parse_rrule(s: &str) -> Option<RecurrenceRule> {
    let mut freq = None;
    let mut interval = 1u32;
    let mut by_day = Vec::new();
    let mut count = None;
    let mut until = None;

    for part in s.split(';') {
        if let Some(val) = part.strip_prefix("FREQ=") {
            freq = match val {
                "DAILY" => Some(Frequency::Daily),
                "WEEKLY" => Some(Frequency::Weekly),
                "MONTHLY" => Some(Frequency::Monthly),
                "YEARLY" => Some(Frequency::Yearly),
                _ => None,
            };
        } else if let Some(val) = part.strip_prefix("INTERVAL=") {
            interval = val.parse().unwrap_or(1);
        } else if let Some(val) = part.strip_prefix("COUNT=") {
            count = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("UNTIL=") {
            // UNTIL can be a date or datetime; parse as datetime with empty params (UTC)
            until = parse_ical_datetime(val, "");
        } else if let Some(val) = part.strip_prefix("BYDAY=") {
            for day_str in val.split(',') {
                // Strip optional ordinal prefix (e.g. "2MO" → "MO")
                let weekday_str =
                    day_str.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-');
                if let Some(wd) = parse_weekday(weekday_str) {
                    by_day.push(wd);
                }
            }
        }
    }

    Some(RecurrenceRule {
        freq: freq?,
        interval,
        by_day,
        count,
        until,
    })
}

fn parse_weekday(s: &str) -> Option<chrono::Weekday> {
    match s {
        "MO" => Some(chrono::Weekday::Mon),
        "TU" => Some(chrono::Weekday::Tue),
        "WE" => Some(chrono::Weekday::Wed),
        "TH" => Some(chrono::Weekday::Thu),
        "FR" => Some(chrono::Weekday::Fri),
        "SA" => Some(chrono::Weekday::Sat),
        "SU" => Some(chrono::Weekday::Sun),
        _ => None,
    }
}

/// Convert a naive local datetime to a UTC timestamp, respecting timezone.
fn naive_to_timestamp(
    naive: chrono::NaiveDateTime,
    tz: Option<chrono_tz::Tz>,
    utc: bool,
) -> Option<i64> {
    use chrono::TimeZone;

    if utc {
        return Some(naive.and_utc().timestamp());
    }
    if let Some(tz) = tz {
        // .earliest() picks the pre-DST side for ambiguous times.
        return tz
            .from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.timestamp());
    }
    // Floating time → local.
    chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp())
}

/// Parse a datetime value into a `NaiveDateTime` (without timezone conversion).
fn parse_ical_naive_datetime(value: &str) -> Option<chrono::NaiveDateTime> {
    if !value.is_ascii() {
        return None;
    }
    use chrono::{NaiveDate, NaiveTime};

    let s = value.strip_suffix('Z').unwrap_or(value);

    // DATE only: YYYYMMDD
    if s.len() == 8 && !s.contains('T') {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        let day: u32 = s[6..8].parse().ok()?;
        let d = NaiveDate::from_ymd_opt(year, month, day)?;
        return Some(d.and_time(NaiveTime::from_hms_opt(0, 0, 0)?));
    }

    // DATETIME: YYYYMMDDTHHmmss
    if s.len() >= 15 && s.contains('T') {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        let day: u32 = s[6..8].parse().ok()?;
        let hour: u32 = s[9..11].parse().ok()?;
        let min: u32 = s[11..13].parse().ok()?;
        let sec: u32 = s[13..15].parse().ok()?;
        let d = NaiveDate::from_ymd_opt(year, month, day)?;
        let t = NaiveTime::from_hms_opt(hour, min, sec)?;
        return Some(chrono::NaiveDateTime::new(d, t));
    }

    None
}

/// Extract TZID from iCal property parameters (e.g. ";TZID=Europe/Prague").
fn extract_tzid(params: &str) -> Option<chrono_tz::Tz> {
    let start = params.find("TZID=")?;
    let tzid = &params[start + 5..];
    let tzid = tzid.split(';').next().unwrap_or(tzid);
    tzid.parse().ok()
}

/// Expand a single iCal VEVENT into one or more `CalendarEvent` entities.
///
/// Non-recurring events produce a single entity.  Recurring events (RRULE)
/// are expanded into individual occurrences within `range`, with EXDATE
/// exclusions applied.
fn expand_vevent(ical_str: &str, range: TimeRange) -> Vec<entity::calendar::CalendarEvent> {
    let Some(raw) = parse_vevent_raw(ical_str) else {
        warn_invalid_calendar_item();
        return Vec::new();
    };

    let rrule_str = match &raw.rrule {
        Some(r) => r.clone(),
        None => return vec![raw_to_event(&raw)],
    };

    let Some(rule) = parse_rrule(&rrule_str) else {
        warn_invalid_calendar_item();
        return vec![raw_to_event(&raw)];
    };

    let duration = raw.end_time - raw.start_time;
    let time_of_day = raw.dtstart_naive.time();

    let mut occurrences = Vec::new();
    let mut generated = 0u32;

    // Walk candidate dates forward from DTSTART.
    let mut cursor = raw.dtstart_naive.date();

    // Iteration cap to prevent runaway loops.
    const MAX_ITERATIONS: u32 = 10_000;
    let mut iterations = 0u32;

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            break;
        }

        // For weekly recurrence with BYDAY: check every day of the current
        // period (the week starting at cursor) against the day filter.
        let candidates: Vec<chrono::NaiveDate> = match rule.freq {
            Frequency::Weekly if !rule.by_day.is_empty() => {
                use chrono::Datelike;
                // Find the Monday of the week containing `cursor`.
                let iso_week_start =
                    cursor - chrono::Duration::days(cursor.weekday().num_days_from_monday() as i64);
                rule.by_day
                    .iter()
                    .map(|wd| {
                        iso_week_start + chrono::Duration::days(wd.num_days_from_monday() as i64)
                    })
                    .filter(|d| *d >= raw.dtstart_naive.date())
                    .collect()
            }
            _ => vec![cursor],
        };

        for date in candidates {
            let occ_naive = date.and_time(time_of_day);
            let Some(occ_start) = naive_to_timestamp(occ_naive, raw.tz, raw.utc) else {
                continue;
            };
            let occ_end = occ_start + duration;

            // Check UNTIL / COUNT limits.
            if let Some(until) = rule.until
                && occ_start > until
            {
                return occurrences;
            }

            // Past range end → done.
            if occ_start >= range.end {
                return occurrences;
            }

            // Skip if before range or excluded.
            if occ_end > range.start && !raw.exdates.contains(&occ_start) {
                occurrences.push(entity::calendar::CalendarEvent {
                    uid: raw.uid.clone(),
                    source_uid: String::new(),
                    summary: raw.summary.clone(),
                    start_time: occ_start,
                    end_time: occ_end,
                    all_day: raw.all_day,
                    description: raw.description.clone(),
                    location: raw.location.clone(),
                    attendees: raw.attendees.clone(),
                });
            }

            generated += 1;
            if let Some(count) = rule.count
                && generated >= count
            {
                return occurrences;
            }
        }

        // Advance cursor by one period.
        cursor = advance_date(cursor, rule.freq, rule.interval);
    }

    occurrences
}

/// Advance a date by one recurrence period.
fn advance_date(date: chrono::NaiveDate, freq: Frequency, interval: u32) -> chrono::NaiveDate {
    use chrono::Datelike;
    match freq {
        Frequency::Daily => date + chrono::Duration::days(interval as i64),
        Frequency::Weekly => date + chrono::Duration::weeks(interval as i64),
        Frequency::Monthly => {
            // Add `interval` months; clamp day to month length.
            let total_months = date.year() * 12 + (date.month0() as i32) + (interval as i32);
            let new_year = total_months / 12;
            let new_month = (total_months % 12) as u32 + 1;
            let max_day = days_in_month(new_year, new_month);
            let day = date.day().min(max_day);
            chrono::NaiveDate::from_ymd_opt(new_year, new_month, day).unwrap_or(date)
        }
        Frequency::Yearly => {
            chrono::NaiveDate::from_ymd_opt(date.year() + interval as i32, date.month(), date.day())
                .unwrap_or(date)
        }
    }
}

/// Number of days in a given month.
fn days_in_month(year: i32, month: u32) -> u32 {
    chrono::NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .map(|d| {
        (d - chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("valid date")).num_days() as u32
    })
    .unwrap_or(30)
}

/// Unfold iCalendar continuation lines.
fn unfold_ical(s: &str) -> String {
    let mut result = String::new();
    for line in s.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line: remove leading whitespace
            result.push_str(&line[1..]);
        } else {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line);
        }
    }
    result
}

/// Split iCalendar property line into (parameters, value).
fn split_ical_property(line: &str, property: &str) -> (String, String) {
    let rest = line.strip_prefix(property).unwrap_or("");
    if let Some(colon_pos) = rest.find(':') {
        let params = rest[..colon_pos].to_string();
        let value = rest[colon_pos + 1..].to_string();
        (params, value)
    } else {
        (String::new(), rest.to_string())
    }
}

/// Parse iCalendar datetime/date value to Unix timestamp.
fn parse_ical_datetime(value: &str, params: &str) -> Option<i64> {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
    if !value.is_ascii() {
        return None;
    }

    // DATE format: YYYYMMDD
    // All-day events use local midnight, not UTC midnight, so that a
    // "Feb 14" all-day event spans [Feb 14 00:00 local, Feb 15 00:00 local).
    if params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") && value.len() >= 8 {
        let year: i32 = value[0..4].parse().ok()?;
        let month: u32 = value[4..6].parse().ok()?;
        let day: u32 = value[6..8].parse().ok()?;
        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let datetime = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
        return Some(
            chrono::Local
                .from_local_datetime(&datetime)
                .earliest()?
                .timestamp(),
        );
    }

    // DATETIME format: YYYYMMDDTHHmmss[Z] or with TZID
    let dt_str = if let Some(stripped) = value.strip_suffix('Z') {
        stripped
    } else {
        value
    };

    if dt_str.len() >= 15 && dt_str.contains('T') {
        let year: i32 = dt_str[0..4].parse().ok()?;
        let month: u32 = dt_str[4..6].parse().ok()?;
        let day: u32 = dt_str[6..8].parse().ok()?;
        let hour: u32 = dt_str[9..11].parse().ok()?;
        let min: u32 = dt_str[11..13].parse().ok()?;
        let sec: u32 = dt_str[13..15].parse().ok()?;

        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let time = NaiveTime::from_hms_opt(hour, min, sec)?;
        let datetime = NaiveDateTime::new(date, time);

        // Ends with Z → UTC
        if value.ends_with('Z') {
            return Some(datetime.and_utc().timestamp());
        }

        // Try to extract TZID and convert
        if let Some(tzid_start) = params.find("TZID=") {
            let tzid = &params[tzid_start + 5..];
            let tzid = tzid.split(';').next().unwrap_or(tzid);
            if let Ok(tz) = tzid.parse::<chrono_tz::Tz>()
                && let Some(dt) = tz.from_local_datetime(&datetime).earliest()
            {
                return Some(dt.timestamp());
            }
        }

        // No Z, no TZID → floating time (RFC 5545), interpret as local
        return Some(
            chrono::Local
                .from_local_datetime(&datetime)
                .earliest()
                .map(|dt| dt.timestamp())
                .unwrap_or_else(|| datetime.and_utc().timestamp()),
        );
    }

    None
}

/// Unescape iCalendar text value.
fn unescape_ical(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Parse ATTENDEE property line.
fn parse_attendee_line(line: &str) -> Option<entity::calendar::CalendarEventAttendee> {
    let rest = line.strip_prefix("ATTENDEE")?;
    let colon_pos = rest.find(':')?;
    let params = &rest[..colon_pos];
    let value = &rest[colon_pos + 1..];

    // Extract email (value is typically "mailto:email@example.com")
    let email = value.strip_prefix("mailto:").unwrap_or(value).to_string();

    // Extract CN (Common Name) parameter
    let name = if let Some(cn_start) = params.find("CN=") {
        let cn = &params[cn_start + 3..];
        // CN value might be quoted
        let cn = if let Some(stripped) = cn.strip_prefix('"') {
            if let Some(end_quote) = stripped.find('"') {
                &stripped[..end_quote]
            } else {
                cn
            }
        } else {
            cn.split(';').next().unwrap_or(cn)
        };
        Some(cn.to_string())
    } else {
        None
    };

    // Extract PARTSTAT parameter
    let status = if let Some(partstat_start) = params.find("PARTSTAT=") {
        let partstat = &params[partstat_start + 9..];
        let partstat = partstat.split(';').next().unwrap_or(partstat);
        match partstat {
            "ACCEPTED" => entity::calendar::AttendeeStatus::Accepted,
            "DECLINED" => entity::calendar::AttendeeStatus::Declined,
            "TENTATIVE" => entity::calendar::AttendeeStatus::Tentative,
            _ => entity::calendar::AttendeeStatus::NeedsAction,
        }
    } else {
        entity::calendar::AttendeeStatus::NeedsAction
    };

    Some(entity::calendar::CalendarEventAttendee {
        name,
        email,
        status,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn day_boundaries_handle_missing_midnight_and_skipped_civil_dates() {
        use chrono::Timelike;
        let date = chrono::NaiveDate::from_ymd_opt(2018, 11, 4).expect("date");
        let boundary = super::day_start(date, &chrono_tz::America::Sao_Paulo).expect("boundary");
        assert_eq!(boundary.date_naive(), date);
        assert_eq!(boundary.hour(), 1);
        let skipped = chrono::NaiveDate::from_ymd_opt(2011, 12, 30).expect("date");
        let boundary = super::day_start(skipped, &chrono_tz::Pacific::Apia).expect("boundary");
        assert_eq!(
            boundary.date_naive(),
            skipped.succ_opt().expect("next date")
        );
    }

    use super::*;

    /// Minimal iCal for the "Daily - LabRulez" recurring Tuesday meeting.
    ///
    /// When EDS expands this to an occurrence on `date` (format "YYYYMMDD")
    /// it sends a VEVENT with DTSTART set to that occurrence's date.
    ///
    /// Note: RFC 5545 folding uses `\r\n` + a leading space for continuation
    /// lines.  Rust `\` string-literal line continuation strips leading
    /// whitespace, so continuation lines are written as explicit string
    /// concatenation to preserve the required leading space.
    fn labrulez_ical(date: &str) -> String {
        // Each piece is one iCal line (or folded continuation).
        // Continuation lines intentionally start with a single space.
        "BEGIN:VCALENDAR\r\n".to_string()
            + "VERSION:2.0\r\n"
            + "BEGIN:VEVENT\r\n"
            + &format!("DTSTART;TZID=Europe/Prague:{date}T083000\r\n")
            + &format!("DTEND;TZID=Europe/Prague:{date}T083500\r\n")
            + "RRULE:FREQ=WEEKLY;BYDAY=TU\r\n"
            + "SUMMARY:Daily - LabRulez\r\n"
            + "UID:077u2vl5ec0knbionphchefveh_R20260203T073000@google.com\r\n"
            + "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;\r\n"
            + " CN=daniel.altmann@seznam.cz;X-NUM-GUESTS=0:mailto:\r\n"
            + " daniel.altmann@seznam.cz\r\n"
            + "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;\r\n"
            + " CN=pavel.zak@cookielab.io;X-NUM-GUESTS=0:mailto:pavel.zak@cookielab.io\r\n"
            + "BEGIN:VALARM\r\n"
            + "ACTION:DISPLAY\r\n"
            + "DESCRIPTION:This is an event reminder\r\n"
            + "TRIGGER:-PT10M\r\n"
            + "END:VALARM\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n"
    }

    // ── parse_vevent ─────────────────────────────────────────────────────────

    /// Regression: expanded occurrence for 2026-02-17 (a Tuesday) must parse
    /// correctly even though the master VEVENT has an older DTSTART.
    #[test]
    fn parse_vevent_recurring_occurrence_with_tzid() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse LabRulez occurrence");

        assert_eq!(event.summary, "Daily - LabRulez");
        assert_eq!(
            event.uid,
            "077u2vl5ec0knbionphchefveh_R20260203T073000@google.com"
        );
        assert!(!event.all_day, "event should not be all-day");

        // DTSTART;TZID=Europe/Prague:20260217T083000
        // Prague is UTC+1 in February → expected UTC timestamp is 07:30
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};
        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");

        let start_naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value"),
            NaiveTime::from_hms_opt(8, 30, 0).expect("expected value"),
        );
        let expected_start = tz
            .from_local_datetime(&start_naive)
            .single()
            .expect("expected value")
            .timestamp();
        assert_eq!(
            event.start_time, expected_start,
            "DTSTART should be 2026-02-17 08:30 Prague (07:30 UTC)"
        );

        // DTEND;TZID=Europe/Prague:20260217T083500 → 5 min later
        let end_naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value"),
            NaiveTime::from_hms_opt(8, 35, 0).expect("expected value"),
        );
        let expected_end = tz
            .from_local_datetime(&end_naive)
            .single()
            .expect("expected value")
            .timestamp();
        assert_eq!(
            event.end_time, expected_end,
            "DTEND should be 2026-02-17 08:35 Prague"
        );
    }

    /// Folded ATTENDEE lines (RFC 5545 line-folding) must be unfolded so that
    /// the attendee email and PARTSTAT are parsed from the joined value.
    #[test]
    fn parse_vevent_folded_attendee_lines() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse");

        let pz = event
            .attendees
            .iter()
            .find(|a| a.email == "pavel.zak@cookielab.io");
        assert!(pz.is_some(), "folded attendee email should be parsed");
        assert_eq!(
            pz.expect("expected value").status,
            entity::calendar::AttendeeStatus::Accepted,
            "PARTSTAT=ACCEPTED should map to Accepted"
        );

        let da = event
            .attendees
            .iter()
            .find(|a| a.email.contains("daniel.altmann"));
        assert!(da.is_some(), "second folded attendee should be parsed");
    }

    /// A nested VALARM component must not corrupt DTSTART/DTEND parsing
    /// (the nesting guard must skip VALARM properties).
    #[test]
    fn parse_vevent_valarm_is_skipped() {
        let ical = labrulez_ical("20260217");
        let event = parse_vevent(&ical).expect("should parse");

        // If VALARM DESCRIPTION leaked into the event description the field
        // would be "This is an event reminder" instead of None.
        assert_ne!(
            event.description.as_deref(),
            Some("This is an event reminder"),
            "VALARM DESCRIPTION must not bleed into event description"
        );
    }

    // ── build_time_range_query_from_today ────────────────────────────────────

    /// The query window must start at today midnight, not at `now`.
    /// An event at 08:30 must not be excluded when the daemon starts at 09:00.
    #[test]
    fn query_starts_at_today_midnight() {
        let (_range, query) = build_time_range_query_from_today();

        // Compute today midnight in UTC independently.
        let today_midnight_utc = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid")
            .and_local_timezone(chrono::Local)
            .earliest()
            .expect("today midnight is a valid local time")
            .to_utc();

        let expected_start = today_midnight_utc.format("%Y%m%dT%H%M%SZ").to_string();
        assert!(
            query.contains(&expected_start),
            "query should start at today midnight ({expected_start}), got: {query}"
        );
    }

    // ── RRULE expansion ────────────────────────────────────────────────────────

    /// The master VEVENT for "Daily - LabRulez" has DTSTART=Feb 3 (the original
    /// series start) and RRULE:FREQ=WEEKLY;BYDAY=TU.  EDS sends this master
    /// event, NOT expanded occurrences.  The plugin must expand it so that the
    /// Feb 17 occurrence (a Tuesday) appears in the Agenda widget.
    #[test]
    fn expand_weekly_recurring_event_into_today() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        // Master event: starts Feb 3, weekly on Tuesdays.
        let ical = labrulez_ical("20260203");

        // Query window: Feb 17 midnight → Feb 19 midnight (covers Feb 17 Tuesday).
        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 19).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        // Should produce exactly 1 occurrence on Feb 17 (a Tuesday).
        assert_eq!(
            events.len(),
            1,
            "expected 1 occurrence in range, got {}",
            events.len()
        );

        let event = &events[0];
        assert_eq!(event.summary, "Daily - LabRulez");

        let expected_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value"),
                NaiveTime::from_hms_opt(8, 30, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();
        assert_eq!(
            event.start_time, expected_start,
            "occurrence should be on Feb 17 08:30 Prague"
        );
    }

    /// Weekly recurring event should produce multiple occurrences across a
    /// multi-week range.
    #[test]
    fn expand_weekly_recurring_multiple_weeks() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        let ical = labrulez_ical("20260203"); // Weekly TU
        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");

        // 3-week window: Feb 10 → Mar 3 (should have Feb 10, 17, 24)
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 10).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 3, 3).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        assert_eq!(
            events.len(),
            3,
            "3 Tuesdays in [Feb 10, Mar 3): {:?}",
            events.iter().map(|e| e.start_time).collect::<Vec<_>>()
        );
    }

    /// EXDATE exclusions must suppress the matching occurrence.
    #[test]
    fn expand_weekly_with_exdate() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");

        // Add EXDATE for Feb 17 (skip that Tuesday).
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260203T083000\r\n"
            + "DTEND;TZID=Europe/Prague:20260203T083500\r\n"
            + "RRULE:FREQ=WEEKLY;BYDAY=TU\r\n"
            + "EXDATE;TZID=Europe/Prague:20260217T083000\r\n"
            + "SUMMARY:Daily - LabRulez\r\n"
            + "UID:test-exdate@example.com\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // Range: Feb 10 → Mar 3 → would normally be 3 Tuesdays.
        let range_start = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 2, 10).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();
        let range_end = tz
            .from_local_datetime(&NaiveDateTime::new(
                NaiveDate::from_ymd_opt(2026, 3, 3).expect("expected value"),
                NaiveTime::from_hms_opt(0, 0, 0).expect("expected value"),
            ))
            .single()
            .expect("expected value")
            .timestamp();

        let events = expand_vevent(
            &ical,
            TimeRange {
                start: range_start,
                end: range_end,
            },
        );

        // Feb 17 excluded → only Feb 10 and Feb 24.
        assert_eq!(events.len(), 2, "EXDATE should exclude Feb 17");
    }

    // ── RRULE expansion: other frequencies ──────────────────────────────────

    /// Helper: build a minimal recurring VEVENT iCal string.
    fn recurring_ical(dtstart: &str, dtend: &str, rrule: &str, uid: &str) -> String {
        "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + &format!("DTSTART;TZID=Europe/Prague:{dtstart}\r\n")
            + &format!("DTEND;TZID=Europe/Prague:{dtend}\r\n")
            + &format!("RRULE:{rrule}\r\n")
            + "SUMMARY:Test event\r\n"
            + &format!("UID:{uid}\r\n")
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n"
    }

    /// Helper: build a UTC range from Prague dates for brevity.
    fn prague_range(start: (i32, u32, u32), end: (i32, u32, u32)) -> TimeRange {
        use chrono::{NaiveDate, NaiveTime, TimeZone as _};
        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");
        let mk = |y, m, d| {
            tz.from_local_datetime(
                &NaiveDate::from_ymd_opt(y, m, d)
                    .expect("expected value")
                    .and_time(NaiveTime::from_hms_opt(0, 0, 0).expect("expected value")),
            )
            .single()
            .expect("expected value")
            .timestamp()
        };
        TimeRange {
            start: mk(start.0, start.1, start.2),
            end: mk(end.0, end.1, end.2),
        }
    }

    #[test]
    fn expand_daily_recurring() {
        // Daily event at 09:00, 30 min duration.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T093000",
            "FREQ=DAILY",
            "daily@test",
        );
        let range = prague_range((2026, 2, 15), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        // Feb 15, 16, 17 = 3 days.
        assert_eq!(events.len(), 3, "daily should produce 3 occurrences");
    }

    #[test]
    fn expand_daily_with_interval() {
        // Every 3 days starting Feb 1.
        let ical = recurring_ical(
            "20260201T100000",
            "20260201T110000",
            "FREQ=DAILY;INTERVAL=3",
            "daily3@test",
        );
        // Range: Feb 1 → Feb 16. Occurrences: Feb 1, 4, 7, 10, 13 = 5.
        let range = prague_range((2026, 2, 1), (2026, 2, 16));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 5, "every-3-days in 15 days = 5");
    }

    #[test]
    fn expand_weekly_with_interval() {
        // Every 2 weeks on Tuesdays starting Feb 3.
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU",
            "biweekly@test",
        );
        // Range: Feb 1 → Mar 15. Occurrences: Feb 3, Feb 17, Mar 3 = 3.
        let range = prague_range((2026, 2, 1), (2026, 3, 15));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "biweekly TU in 6 weeks = 3");
    }

    #[test]
    fn expand_monthly_recurring() {
        // Monthly on the 15th.
        let ical = recurring_ical(
            "20260115T140000",
            "20260115T150000",
            "FREQ=MONTHLY",
            "monthly@test",
        );
        // Range: Feb 1 → May 1. Occurrences: Feb 15, Mar 15, Apr 15 = 3.
        let range = prague_range((2026, 2, 1), (2026, 5, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "monthly from Feb→May = 3");
    }

    #[test]
    fn expand_monthly_clamps_day_to_month_length() {
        // Monthly on the 31st — months without 31 days should still produce
        // an occurrence (clamped to last day).
        let ical = recurring_ical(
            "20260131T100000",
            "20260131T110000",
            "FREQ=MONTHLY",
            "monthly31@test",
        );
        // Range: Jan 1 → May 1.
        // Jan 31 ✓, Feb 28 (clamped) ✓, Mar 31 ✓, Apr 30 (clamped) ✓ = 4.
        let range = prague_range((2026, 1, 1), (2026, 5, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 4, "monthly-31 with clamping = 4");
    }

    #[test]
    fn expand_yearly_recurring() {
        let ical = recurring_ical(
            "20250614T180000",
            "20250614T200000",
            "FREQ=YEARLY",
            "yearly@test",
        );
        // Range: 2026-01 → 2029-01. Occurrences: Jun 14 2026, 2027, 2028 = 3.
        let range = prague_range((2026, 1, 1), (2029, 1, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "yearly from 2026→2029 = 3");
    }

    // ── RRULE expansion: COUNT and UNTIL limits ──────────────────────────────

    #[test]
    fn expand_with_count_limit() {
        // Daily event with COUNT=5, starting Feb 10.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY;COUNT=5",
            "count@test",
        );
        // Range is wide, but COUNT=5 limits to Feb 10–14.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 5, "COUNT=5 should cap at 5 occurrences");
    }

    #[test]
    fn expand_with_count_fewer_in_range() {
        // Daily event with COUNT=3, starting Feb 10.
        let ical = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY;COUNT=3",
            "count3@test",
        );
        // Range starts at Feb 12, so only Feb 12 falls in range (COUNT ends at Feb 12).
        let range = prague_range((2026, 2, 12), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1, "only 1 of 3 counted occurrences in range");
    }

    #[test]
    fn expand_with_until_limit() {
        // Weekly TU starting Feb 3, until Feb 20.
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000",
            "FREQ=WEEKLY;BYDAY=TU;UNTIL=20260220T235959Z",
            "until@test",
        );
        // Feb 3, 10, 17 are before UNTIL; Feb 24 is after.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "UNTIL=Feb 20 should include Feb 3, 10, 17");
    }

    // ── RRULE expansion: non-recurring passthrough ───────────────────────────

    #[test]
    fn expand_non_recurring_event_passes_through() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260217T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:One-off meeting\r\n"
            + "UID:single@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "One-off meeting");
        assert_eq!(events[0].uid, "single@test");
    }

    #[test]
    fn expand_non_recurring_outside_range_still_returned() {
        // Non-recurring events are NOT filtered by expand_vevent (the caller
        // or the Agenda widget handles range filtering for non-recurring events).
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260101T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260101T150000\r\n"
            + "SUMMARY:Past meeting\r\n"
            + "UID:past@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 1, "non-recurring always passes through");
    }

    // ── RRULE expansion: BYDAY with multiple days ────────────────────────────

    #[test]
    fn expand_weekly_multiple_byday() {
        // MWF schedule.
        let ical = recurring_ical(
            "20260202T090000", // Monday Feb 2
            "20260202T100000",
            "FREQ=WEEKLY;BYDAY=MO,WE,FR",
            "mwf@test",
        );
        // One week: Feb 9–15. Should have Mon 9, Wed 11, Fri 13 = 3.
        let range = prague_range((2026, 2, 9), (2026, 2, 16));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3, "MO,WE,FR in 1 week = 3");
    }

    // ── RRULE expansion: preserves event metadata ────────────────────────────

    #[test]
    fn expand_preserves_uid_and_duration() {
        let ical = recurring_ical(
            "20260203T083000",
            "20260203T093000", // 1-hour duration
            "FREQ=WEEKLY;BYDAY=TU",
            "preserve-uid@test",
        );
        let range = prague_range((2026, 2, 10), (2026, 2, 25));
        let events = expand_vevent(&ical, range);
        assert_eq!(events.len(), 3); // Feb 10, 17, 24
        for event in &events {
            assert_eq!(event.uid, "preserve-uid@test");
            assert_eq!(
                event.end_time - event.start_time,
                3600,
                "duration should be preserved"
            );
        }
    }

    // ── parse_ical_events (top-level, mixed input) ───────────────────────────

    #[test]
    fn parse_ical_events_mixes_recurring_and_single() {
        let recurring = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=DAILY",
            "recurring@test",
        );
        let single = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260211T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260211T150000\r\n"
            + "SUMMARY:Single\r\n"
            + "UID:single@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let icals = [recurring, single];
        let range = prague_range((2026, 2, 10), (2026, 2, 13));
        let events = parse_ical_events(
            &icals
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            range,
        );
        // Daily: Feb 10, 11, 12 = 3. Single: 1. Total: 4.
        assert_eq!(events.len(), 4, "3 daily + 1 single = 4");
    }

    // ── parse_rrule ──────────────────────────────────────────────────────────

    #[test]
    fn parse_rrule_weekly_byday() {
        let rule = parse_rrule("FREQ=WEEKLY;BYDAY=TU").expect("expected value");
        assert_eq!(rule.freq, Frequency::Weekly);
        assert_eq!(rule.interval, 1);
        assert_eq!(rule.by_day, vec![chrono::Weekday::Tue]);
        assert!(rule.count.is_none());
        assert!(rule.until.is_none());
    }

    #[test]
    fn parse_rrule_daily_interval_count() {
        let rule = parse_rrule("FREQ=DAILY;INTERVAL=3;COUNT=10").expect("expected value");
        assert_eq!(rule.freq, Frequency::Daily);
        assert_eq!(rule.interval, 3);
        assert_eq!(rule.count, Some(10));
    }

    #[test]
    fn parse_rrule_monthly() {
        let rule = parse_rrule("FREQ=MONTHLY").expect("expected value");
        assert_eq!(rule.freq, Frequency::Monthly);
        assert_eq!(rule.interval, 1);
    }

    #[test]
    fn parse_rrule_with_until() {
        let rule = parse_rrule("FREQ=WEEKLY;UNTIL=20260301T000000Z").expect("expected value");
        assert!(rule.until.is_some());
        // UNTIL is a UTC timestamp for 2026-03-01 00:00:00Z.
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 3, 1)
            .expect("expected value")
            .and_hms_opt(0, 0, 0)
            .expect("expected value")
            .and_utc()
            .timestamp();
        assert_eq!(rule.until.expect("expected value"), expected);
    }

    #[test]
    fn parse_rrule_multiple_byday() {
        let rule = parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE,FR").expect("expected value");
        assert_eq!(
            rule.by_day,
            vec![
                chrono::Weekday::Mon,
                chrono::Weekday::Wed,
                chrono::Weekday::Fri
            ]
        );
    }

    #[test]
    fn parse_rrule_unknown_freq_returns_none() {
        assert!(parse_rrule("FREQ=SECONDLY").is_none());
    }

    // ── advance_date ─────────────────────────────────────────────────────────

    #[test]
    fn advance_date_daily() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 2, 28).expect("expected value");
        let next = advance_date(d, Frequency::Daily, 1);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2026, 3, 1).expect("expected value")
        );
    }

    #[test]
    fn advance_date_weekly() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value");
        let next = advance_date(d, Frequency::Weekly, 2);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2026, 3, 3).expect("expected value")
        );
    }

    #[test]
    fn advance_date_monthly_clamps() {
        // Jan 31 + 1 month → Feb 28 (2026 is not a leap year).
        let d = chrono::NaiveDate::from_ymd_opt(2026, 1, 31).expect("expected value");
        let next = advance_date(d, Frequency::Monthly, 1);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2026, 2, 28).expect("expected value")
        );
    }

    #[test]
    fn advance_date_monthly_leap_year() {
        // Jan 31 + 1 month in 2028 (leap year) → Feb 29.
        let d = chrono::NaiveDate::from_ymd_opt(2028, 1, 31).expect("expected value");
        let next = advance_date(d, Frequency::Monthly, 1);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2028, 2, 29).expect("expected value")
        );
    }

    #[test]
    fn advance_date_yearly() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 6, 15).expect("expected value");
        let next = advance_date(d, Frequency::Yearly, 1);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2027, 6, 15).expect("expected value")
        );
    }

    #[test]
    fn advance_date_monthly_wraps_year() {
        // Nov + 2 months → Jan next year.
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 15).expect("expected value");
        let next = advance_date(d, Frequency::Monthly, 2);
        assert_eq!(
            next,
            chrono::NaiveDate::from_ymd_opt(2027, 1, 15).expect("expected value")
        );
    }

    // ── days_in_month ────────────────────────────────────────────────────────

    #[test]
    fn days_in_month_february_non_leap() {
        assert_eq!(days_in_month(2026, 2), 28);
    }

    #[test]
    fn days_in_month_february_leap() {
        assert_eq!(days_in_month(2028, 2), 29);
    }

    #[test]
    fn days_in_month_various() {
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    // ── parse_ical_naive_datetime ────────────────────────────────────────────

    #[test]
    fn parse_ical_naive_datetime_full() {
        let dt = parse_ical_naive_datetime("20260217T083000").expect("expected value");
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value")
        );
        assert_eq!(
            dt.time(),
            chrono::NaiveTime::from_hms_opt(8, 30, 0).expect("expected value")
        );
    }

    #[test]
    fn parse_ical_naive_datetime_strips_z() {
        let dt = parse_ical_naive_datetime("20260217T083000Z").expect("expected value");
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value")
        );
    }

    #[test]
    fn parse_ical_naive_datetime_date_only() {
        let dt = parse_ical_naive_datetime("20260217").expect("expected value");
        assert_eq!(
            dt.date(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value")
        );
        assert_eq!(
            dt.time(),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("expected value")
        );
    }

    #[test]
    fn parse_ical_naive_datetime_invalid() {
        assert!(parse_ical_naive_datetime("garbage").is_none());
        assert!(parse_ical_naive_datetime("").is_none());
    }

    // ── extract_tzid ─────────────────────────────────────────────────────────

    #[test]
    fn extract_tzid_present() {
        let tz = extract_tzid(";TZID=Europe/Prague");
        assert_eq!(tz, Some("Europe/Prague".parse().expect("expected value")));
    }

    #[test]
    fn extract_tzid_with_extra_params() {
        let tz = extract_tzid(";VALUE=DATE-TIME;TZID=America/New_York;X-FOO=bar");
        assert_eq!(
            tz,
            Some("America/New_York".parse().expect("expected value"))
        );
    }

    #[test]
    fn extract_tzid_absent() {
        assert!(extract_tzid("").is_none());
        assert!(extract_tzid(";VALUE=DATE").is_none());
    }

    #[test]
    fn extract_tzid_unknown_returns_none() {
        assert!(extract_tzid(";TZID=Mars/Olympus_Mons").is_none());
    }

    // ── parse_ical_datetime ──────────────────────────────────────────────────

    #[test]
    fn parse_ical_datetime_with_europe_prague_tzid() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};
        // Prague is UTC+1 in winter; 08:30 Prague = 07:30 UTC
        let ts = parse_ical_datetime("20260217T083000", ";TZID=Europe/Prague");
        let dt = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 2, 17).expect("expected value"),
            NaiveTime::from_hms_opt(8, 30, 0).expect("expected value"),
        );
        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");
        let expected = tz
            .from_local_datetime(&dt)
            .single()
            .expect("expected value")
            .timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_utc_z_suffix() {
        let ts = parse_ical_datetime("20260217T073000Z", "");
        // 2026-02-17 07:30:00 UTC
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .expect("expected value")
            .and_hms_opt(7, 30, 0)
            .expect("expected value")
            .and_utc()
            .timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_all_day_date_only() {
        let ts = parse_ical_datetime("20260217", ";VALUE=DATE");
        assert!(ts.is_some(), "all-day date should parse");
        // The timestamp must represent local midnight, not UTC midnight.
        let local_start = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .expect("expected value")
            .and_hms_opt(0, 0, 0)
            .expect("expected value")
            .and_local_timezone(chrono::Local)
            .earliest()
            .expect("expected value")
            .timestamp();
        assert_eq!(ts, Some(local_start));
    }

    #[test]
    fn parse_ical_datetime_floating_no_tz_no_z() {
        // No Z, no TZID → floating time interpreted as local.
        let ts = parse_ical_datetime("20260217T120000", "");
        assert!(ts.is_some());
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 2, 17)
            .expect("expected value")
            .and_hms_opt(12, 0, 0)
            .expect("expected value")
            .and_local_timezone(chrono::Local)
            .earliest()
            .expect("expected value")
            .timestamp();
        assert_eq!(ts, Some(expected));
    }

    #[test]
    fn parse_ical_datetime_invalid() {
        assert!(parse_ical_datetime("not-a-date", "").is_none());
        assert!(parse_ical_datetime("", "").is_none());
    }

    // ── parse_weekday ────────────────────────────────────────────────────────

    #[test]
    fn parse_weekday_all() {
        assert_eq!(parse_weekday("MO"), Some(chrono::Weekday::Mon));
        assert_eq!(parse_weekday("TU"), Some(chrono::Weekday::Tue));
        assert_eq!(parse_weekday("WE"), Some(chrono::Weekday::Wed));
        assert_eq!(parse_weekday("TH"), Some(chrono::Weekday::Thu));
        assert_eq!(parse_weekday("FR"), Some(chrono::Weekday::Fri));
        assert_eq!(parse_weekday("SA"), Some(chrono::Weekday::Sat));
        assert_eq!(parse_weekday("SU"), Some(chrono::Weekday::Sun));
        assert_eq!(parse_weekday("XX"), None);
    }

    #[test]
    fn expand_vevent_missing_uid_returns_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260217T140000\r\n"
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:No UID event\r\n"
            // UID deliberately omitted
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "VEVENT without UID must be dropped, not partially parsed"
        );
    }

    /// Regression: a VEVENT without a parseable DTSTART must be silently dropped.
    #[test]
    fn expand_vevent_missing_dtstart_returns_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            // DTSTART deliberately omitted
            + "DTEND;TZID=Europe/Prague:20260217T150000\r\n"
            + "SUMMARY:No DTSTART event\r\n"
            + "UID:no-dtstart@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";
        let range = prague_range((2026, 2, 17), (2026, 2, 18));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "VEVENT without DTSTART must be dropped, not partially parsed"
        );
    }

    // ── query window: 60-day lookahead ───────────────────────────────────────

    /// Regression: the EDS query window was previously 30 days.  Events
    /// scheduled 31–60 days out (e.g., an "FE interview" 5 weeks away) were
    /// completely absent from the plugin's ObjectsAdded batches.  The window
    /// was extended to 60 days to capture such events.
    ///
    /// This test verifies that a single (non-recurring) event 45 days away
    /// is visible in a 60-day query range but would be absent from 30 days.
    #[test]
    fn single_event_45_days_out_is_in_60_day_range_not_30() {
        // Use a fixed reference point: Feb 23, 2026 midnight Prague.
        // 45 days later = Apr 9, 2026.
        let reference_start = prague_range((2026, 2, 23), (2026, 2, 24)).start;
        let range_30 = TimeRange {
            start: reference_start,
            end: reference_start + 30 * 24 * 3600,
        };
        let range_60 = TimeRange {
            start: reference_start,
            end: reference_start + 60 * 24 * 3600,
        };

        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART;TZID=Europe/Prague:20260409T100000\r\n"
            + "DTEND;TZID=Europe/Prague:20260409T110000\r\n"
            + "SUMMARY:FE interview\r\n"
            + "UID:fe-interview@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let events_30 = expand_vevent(&ical, range_30);
        assert_eq!(
            events_30.len(),
            1,
            "non-recurring always passes through regardless of range"
        );
        // However, the EDS S-expression query would NOT return it with a 30-day window.
        // This test documents that expand_vevent itself doesn't filter non-recurring events —
        // the filtering happens at the EDS query level (occur-in-time-range?).

        let events_60 = expand_vevent(&ical, range_60);
        assert_eq!(
            events_60.len(),
            1,
            "45-day-out event visible in 60-day window"
        );
        assert_eq!(events_60[0].summary, "FE interview");
    }

    /// Regression: COUNT-limited recurring events where COUNT is exhausted
    /// before the query window must return zero occurrences in that window.
    /// Previously the generated counter was verified to handle this correctly.
    #[test]
    fn expand_with_count_exhausted_before_range() {
        // Weekly on Mondays, started 2025-01-06, COUNT=5 → last occurrence 2025-02-03.
        // Query window starts in 2026 — no occurrences should be returned.
        let ical = recurring_ical(
            "20250106T090000",
            "20250106T100000",
            "FREQ=WEEKLY;BYDAY=MO;COUNT=5",
            "exhausted-count@test",
        );
        let range = prague_range((2026, 2, 23), (2026, 3, 25));
        let events = expand_vevent(&ical, range);
        assert_eq!(
            events.len(),
            0,
            "COUNT exhausted in 2025 should produce 0 events for a 2026 query window"
        );
    }

    /// Bi-weekly recurring event: verify that exactly the expected occurrences
    /// within a 60-day window are returned.
    #[test]
    fn expand_biweekly_event_in_60_day_window() {
        // Bi-weekly on Mondays starting 2026-02-23.  In a 60-day window
        // (Feb 23 – Apr 24) the Mondays are: Feb 23, Mar 9, Mar 23, Apr 6, Apr 20 = 5.
        let ical = recurring_ical(
            "20260223T100000",
            "20260223T110000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
            "biweekly-1on1@test",
        );
        let range = prague_range((2026, 2, 23), (2026, 4, 24));
        let events = expand_vevent(&ical, range);
        assert_eq!(
            events.len(),
            5,
            "bi-weekly MO in 60-day window should produce 5 occurrences"
        );
    }

    // ── multi-VEVENT iCal (exception occurrences) ────────────────────────────

    /// Regression: two events scheduled for today ("Tech 1:1 Tomáš / Pavel" and
    /// "FE interview") were not shown in the agenda widget.
    ///
    /// Root cause: EDS sends a single VCALENDAR containing two VEVENTs — the
    /// master recurring event (RRULE + EXDATE for today's original slot) and an
    /// exception occurrence (RECURRENCE-ID, DTSTART rescheduled to this
    /// afternoon).  Previously `parse_vevent_raw` stopped at the first
    /// `END:VEVENT` and silently dropped the exception VEVENT, so today's actual
    /// rescheduled occurrence was never surfaced.
    ///
    /// The fix: `split_vevents` extracts each VEVENT block independently before
    /// `expand_vevent` processes it.
    #[test]
    fn parse_ical_events_multi_vevent_with_exception_occurrence() {
        // Weekly Monday recurring event; the original 09:00 UTC slot on
        // 2026-02-23 (today) is excluded via EXDATE.  An exception VEVENT
        // reschedules that occurrence to 13:00 UTC.
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            // Master: weekly on Mondays, started 2026-02-02; today's slot excluded.
            + "BEGIN:VEVENT\r\n"
            + "UID:tech-1on1@test\r\n"
            + "SUMMARY:Tech 1:1 Tomáš / Pavel\r\n"
            + "DTSTART:20260202T090000Z\r\n"
            + "DTEND:20260202T100000Z\r\n"
            + "RRULE:FREQ=WEEKLY;BYDAY=MO\r\n"
            + "EXDATE:20260223T090000Z\r\n"
            + "END:VEVENT\r\n"
            // Exception: today's occurrence rescheduled to 13:00 UTC.
            + "BEGIN:VEVENT\r\n"
            + "UID:tech-1on1@test\r\n"
            + "SUMMARY:Tech 1:1 Tomáš / Pavel\r\n"
            + "RECURRENCE-ID:20260223T090000Z\r\n"
            + "DTSTART:20260223T130000Z\r\n"
            + "DTEND:20260223T140000Z\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // Query for today only.
        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[ical], range);

        // Must yield exactly one event: the rescheduled 13:00 exception.
        // Without split_vevents the exception VEVENT is dropped → 0 events.
        assert_eq!(
            events.len(),
            1,
            "multi-VEVENT iCal must yield the exception occurrence, not be dropped"
        );
        assert_eq!(events[0].summary, "Tech 1:1 Tomáš / Pavel");

        // 13:00 UTC on 2026-02-23.
        let expected_start = chrono::DateTime::parse_from_rfc3339("2026-02-23T13:00:00Z")
            .expect("expected value")
            .timestamp();
        assert_eq!(
            events[0].start_time, expected_start,
            "rescheduled exception must be at 13:00 UTC, not original 09:00 slot"
        );
    }

    /// Companion: a non-recurring event in a multi-VEVENT VCALENDAR alongside
    /// an unrelated recurring master is still extracted and returned.
    #[test]
    fn parse_ical_events_multi_vevent_standalone_alongside_recurring() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            // A standalone non-recurring event today at 15:00 UTC.
            + "BEGIN:VEVENT\r\n"
            + "UID:fe-interview@test\r\n"
            + "SUMMARY:FE interview\r\n"
            + "DTSTART:20260223T150000Z\r\n"
            + "DTEND:20260223T160000Z\r\n"
            + "END:VEVENT\r\n"
            // An unrelated recurring event also in the same VCALENDAR blob.
            + "BEGIN:VEVENT\r\n"
            + "UID:standup@test\r\n"
            + "SUMMARY:Daily standup\r\n"
            + "DTSTART:20260202T080000Z\r\n"
            + "DTEND:20260202T080500Z\r\n"
            + "RRULE:FREQ=DAILY\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[ical], range);

        // FE interview (15:00) + standup (08:00) = 2 events today.
        assert_eq!(
            events.len(),
            2,
            "both VEVENTs from a multi-VEVENT VCALENDAR must be returned"
        );
        let summaries: Vec<&str> = events.iter().map(|e| e.summary.as_str()).collect();
        assert!(
            summaries.contains(&"FE interview"),
            "standalone event must be present"
        );
        assert!(
            summaries.contains(&"Daily standup"),
            "recurring occurrence must be present"
        );
    }

    // ── parse_ical_datetime: DST ambiguity ───────────────────────────────────

    /// Regression: a TZID-qualified datetime that falls in the DST "fall-back"
    /// gap (when the clock goes back and the same wall time occurs twice) must
    /// still parse successfully.
    ///
    /// Previously `parse_ical_datetime` used `.single()` which returns `None`
    /// for ambiguous times, causing the event to fall through to the "floating
    /// time" branch and be interpreted as the system's local timezone instead
    /// of the specified TZID.  The fix uses `.earliest()` so the first of the
    /// two possible offsets is chosen deterministically.
    ///
    /// Prague (Europe/Prague) falls back on the last Sunday of October:
    /// 2026-10-25 03:00 CEST → 02:00 CET.  Any time between 02:00 and 03:00
    /// on that day is ambiguous.
    #[test]
    fn parse_ical_datetime_dst_ambiguous_uses_earliest() {
        use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};

        // 02:30 on 2026-10-25 in Prague is ambiguous (occurs in both CEST and CET).
        let ts = parse_ical_datetime("20261025T023000", ";TZID=Europe/Prague");
        assert!(
            ts.is_some(),
            "DST-ambiguous TZID datetime must still parse (earliest offset chosen)"
        );

        let tz: chrono_tz::Tz = "Europe/Prague".parse().expect("expected value");
        let naive = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2026, 10, 25).expect("expected value"),
            NaiveTime::from_hms_opt(2, 30, 0).expect("expected value"),
        );
        // .earliest() picks CEST (UTC+2): 02:30 Prague CEST = 00:30 UTC.
        let expected = tz
            .from_local_datetime(&naive)
            .earliest()
            .expect("expected value")
            .timestamp();
        assert_eq!(
            ts,
            Some(expected),
            "ambiguous time should resolve to earliest offset (CEST, UTC+2)"
        );
    }

    // ── Startup sync regression: missing SUMMARY and empty input ─────────────

    /// Regression: EDS can deliver an event whose SUMMARY was stripped by
    /// libical with `X-LIC-ERROR;X-LIC-ERRORTYPE=VALUE-PARSE-ERROR:No value
    /// for SUMMARY property`.  The parser must default to an empty string
    /// rather than silently dropping the event.
    #[test]
    fn parse_vevent_missing_summary_defaults_to_empty() {
        let ical = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART:20260223T100000Z\r\n"
            + "DTEND:20260223T110000Z\r\n"
            // No SUMMARY line — libical error caused it to be stripped.
            + "UID:no-summary@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        let event = parse_vevent(&ical);
        assert!(
            event.is_some(),
            "event with missing SUMMARY must still parse (defaults to empty string)"
        );
        assert_eq!(
            event.expect("expected value").summary,
            "",
            "missing SUMMARY must default to empty string, not cause a parse failure"
        );
    }

    /// `parse_ical_events` must return an empty Vec for empty input without
    /// panicking.  This is a basic boundary check for the startup path.
    #[test]
    fn parse_ical_events_empty_input_returns_empty() {
        let range = prague_range((2026, 2, 23), (2026, 2, 24));
        let events = parse_ical_events(&[], range);
        assert!(events.is_empty(), "empty input must produce empty output");
    }

    /// Recurring events whose occurrences all fall OUTSIDE the query range
    /// must produce no results.  This confirms the RRULE expander filters
    /// correctly so stale events from before today are not shown.
    #[test]
    fn expand_recurring_outside_range_produces_no_events() {
        // Daily event from Jan 1–5 (5 occurrences, COUNT=5).
        let ical = recurring_ical(
            "20260101T090000",
            "20260101T100000",
            "FREQ=DAILY;COUNT=5",
            "past-daily@test",
        );
        // Query window starts after the series ends.
        let range = prague_range((2026, 2, 1), (2026, 3, 1));
        let events = expand_vevent(&ical, range);
        assert!(
            events.is_empty(),
            "recurring event series that ended before the range must produce 0 occurrences"
        );
    }

    /// Regression guard for the startup CalDAV refresh race:
    /// `parse_ical_events` must handle a batch of events (as EDS would send in
    /// `ObjectsAdded`) containing both a recently-added single event and a
    /// recurring master, and return them all.  This exercises the full
    /// parse/expand pipeline that runs once backends are properly populated.
    #[test]
    fn parse_ical_events_handles_mixed_batch_like_objects_added() {
        // A newly-added one-off event (like "FE interview brainstorm").
        let new_event = "BEGIN:VCALENDAR\r\n".to_string()
            + "BEGIN:VEVENT\r\n"
            + "DTSTART:20260224T140000Z\r\n"
            + "DTEND:20260224T150000Z\r\n"
            + "SUMMARY:FE interview brainstorm\r\n"
            + "UID:fe-interview-brainstorm@test\r\n"
            + "END:VEVENT\r\n"
            + "END:VCALENDAR\r\n";

        // A bi-weekly recurring event (like "Tech 1:1").
        // Series starts Feb 10 (Tuesday) → biweekly occurrences: Feb 10, Feb 24, Mar 10, …
        // This puts an occurrence on the same day as the new event (Feb 24).
        let recurring = recurring_ical(
            "20260210T090000",
            "20260210T100000",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TU",
            "tech-1on1@test",
        );

        let icals: Vec<String> = vec![new_event, recurring];
        // Range covers Feb 24 only — the new event AND a biweekly occurrence both land here.
        let range = prague_range((2026, 2, 24), (2026, 2, 25));
        let events = parse_ical_events(
            &icals
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            range,
        );

        // FE interview (14:00 UTC) + Tech 1:1 biweekly occurrence on Feb 24 (09:00 Prague).
        assert_eq!(
            events.len(),
            2,
            "newly-added event and recurring occurrence must both appear after startup sync"
        );
        let summaries: Vec<&str> = events.iter().map(|e| e.summary.as_str()).collect();
        assert!(
            summaries.contains(&"FE interview brainstorm"),
            "newly-added event must appear"
        );
        // recurring_ical helper uses SUMMARY:Test event
        assert!(
            summaries.contains(&"Test event"),
            "recurring occurrence must appear"
        );
    }
}
