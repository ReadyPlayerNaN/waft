//! Formatting utilities for agenda display.

/// Format a time range as "HH:MM – HH:MM" using glib::DateTime.
pub fn format_time_range(start: i64, end: i64) -> String {
    let start_str = format_timestamp(start);
    let end_str = format_timestamp(end);
    format!("{start_str} \u{2013} {end_str}")
}

/// Format a unix timestamp as "HH:MM" in local time using glib::DateTime.
pub fn format_timestamp(ts: i64) -> String {
    glib::DateTime::from_unix_local(ts)
        .and_then(|dt| dt.format("%H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "--:--".to_string())
}

/// Strip HTML tags from a string (simple `<tag>` stripper).
pub fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}
