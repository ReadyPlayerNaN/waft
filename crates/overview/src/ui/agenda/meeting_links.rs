//! Meeting link detection and extraction.

use waft_protocol::entity::calendar::CalendarEvent;

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

/// Extract meeting links from an event's description and location fields.
pub fn extract_meeting_links(event: &CalendarEvent) -> Vec<MeetingLink> {
    let mut links = Vec::new();
    let fields: Vec<&str> = [event.description.as_deref(), event.location.as_deref()]
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

/// Decode HTML entities in a URL string (only `&amp;` which commonly appears in href attributes).
fn decode_html_entities(url: &str) -> String {
    url.replace("&amp;", "&")
}

/// Extract the numeric Zoom meeting ID from a URL (the number after `/j/`).
fn zoom_meeting_id(url: &str) -> Option<&str> {
    let pos = url.find("/j/")?;
    let after = &url[pos + 3..];
    let end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    if end == 0 {
        return None;
    }
    Some(&after[..end])
}

/// Scan text for meeting URLs by looking for known anchors.
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

    // Scan for Zoom: find all https:// occurrences and check for zoom.us/j/
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find("https://") {
        let abs_pos = search_from + pos;
        let raw_url = extract_url_at(text, abs_pos);
        if raw_url.contains("zoom.us/j/") {
            let url = decode_html_entities(&raw_url);
            let new_id = zoom_meeting_id(&url);
            if !links.iter().any(|l| {
                matches!(l.provider, MeetingProvider::Zoom) && zoom_meeting_id(&l.url) == new_id
            }) {
                links.push(MeetingLink {
                    url,
                    provider: MeetingProvider::Zoom,
                });
            }
        }
        search_from = abs_pos + "https://".len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(description: Option<&str>, location: Option<&str>) -> CalendarEvent {
        CalendarEvent {
            uid: "test".to_string(),
            summary: "Test".to_string(),
            start_time: 0,
            end_time: 3600,
            all_day: false,
            description: description.map(str::to_string),
            location: location.map(str::to_string),
            attendees: vec![],
        }
    }

    #[test]
    fn zoom_single_join_link() {
        let event = make_event(
            Some("Join Zoom Meeting\nhttps://us06web.zoom.us/j/12345678?pwd=abc\nMeeting ID: 123"),
            None,
        );
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://us06web.zoom.us/j/12345678?pwd=abc");
    }

    #[test]
    fn zoom_ignores_find_local_number_url() {
        // Typical Zoom invite: join link + "find your local number" link.
        let desc = "Join Zoom Meeting\n\
            https://us06web.zoom.us/j/12345678?pwd=abc\n\
            \n\
            Find your local number: https://us06web.zoom.us/u/xyzxyz";
        let event = make_event(Some(desc), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1, "only the /j/ join URL should be extracted, not /u/");
        assert_eq!(links[0].url, "https://us06web.zoom.us/j/12345678?pwd=abc");
    }

    #[test]
    fn zoom_deduplicates_same_url_in_description_and_location() {
        let url = "https://zoom.us/j/99999?pwd=zzz";
        let event = make_event(Some(url), Some(url));
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1, "same URL in both fields must be deduplicated");
    }

    #[test]
    fn zoom_deduplicates_html_href_vs_anchor_text() {
        // HTML Zoom invite: href has &amp;usp=sf_link tracking param, anchor text is clean.
        // Both refer to the same meeting — only one link should be returned.
        let desc = r#"Join Zoom Meeting<br><a href="https://us06web.zoom.us/j/12345678?pwd=ABC&amp;usp=sf_link">https://us06web.zoom.us/j/12345678?pwd=ABC</a>"#;
        let event = make_event(Some(desc), None);
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1, "HTML href with &amp; vs anchor text must be deduplicated");
        // The stored URL should have & decoded (not &amp;)
        assert!(!links[0].url.contains("&amp;"), "URL must not contain raw &amp;");
    }

    #[test]
    fn zoom_deduplicates_html_href_plus_location() {
        // HTML description with &amp;usp=sf_link in href, plus location with decoded & — same meeting.
        let desc = r#"<a href="https://zoom.us/j/99999?pwd=XYZ&amp;usp=sf_link">https://zoom.us/j/99999?pwd=XYZ</a>"#;
        let loc = "https://zoom.us/j/99999?pwd=XYZ&usp=sf_link";
        let event = make_event(Some(desc), Some(loc));
        let links = extract_meeting_links(&event);
        assert_eq!(links.len(), 1, "all URL variants of the same meeting must collapse to one link");
    }
}

/// Extract a URL starting at `start` in `text`, stopping at termination characters.
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
