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

    // Scan for Zoom: find all https:// occurrences and check for zoom.us/
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find("https://") {
        let abs_pos = search_from + pos;
        let url = extract_url_at(text, abs_pos);
        if url.contains("zoom.us/") && !links.iter().any(|l| l.url == url) {
            links.push(MeetingLink {
                url,
                provider: MeetingProvider::Zoom,
            });
        }
        search_from = abs_pos + "https://".len();
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
