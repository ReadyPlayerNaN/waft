//! Markup sanitization and URL linkification for notification text.
//!
//! Notifications arrive via D-Bus with raw text that may contain characters
//! invalid in Pango markup (e.g. bare `&`). This module provides functions to
//! sanitize text before passing it to `Label::set_markup()`.

/// Prepare a notification title for Pango markup display.
///
/// Per the freedesktop notification spec, the summary (title) does not support
/// markup regardless of advertised capabilities. Always escape the text.
pub fn prepare_title(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let escaped = glib::markup_escape_text(text);
    linkify_urls(&escaped, false)
}

/// Prepare a notification description/body for Pango markup display.
///
/// When `body-markup` is advertised, senders *may* include Pango markup.
/// We try to parse the text as markup first; if it's valid we keep it as-is
/// (and linkify bare URLs). If parsing fails, we escape the entire text.
///
/// Note: `pango::parse_markup` does not recognise `<a>` tags, but GTK labels
/// do.  We strip `<a …>` / `</a>` before validation so that incoming markup
/// with hyperlinks is still accepted as valid.
pub fn prepare_description(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // pango::parse_markup doesn't know about <a> tags, but GTK labels handle
    // them fine. Strip them before validation so we don't reject valid input.
    let stripped = strip_anchor_tags(text);
    let is_valid_markup = gtk::pango::parse_markup(&stripped, '\0').is_ok();

    if is_valid_markup {
        linkify_urls(text, true)
    } else {
        let escaped = glib::markup_escape_text(text);
        linkify_urls(&escaped, false)
    }
}

/// Scan text for bare `http://` / `https://` URLs and wrap them in `<a>` tags.
///
/// When `is_markup` is true, URLs already inside `<a>` tags are left alone
/// using a simple tag-tracking state machine.
fn linkify_urls(text: &str, is_markup: bool) -> String {
    if !text.contains("http://") && !text.contains("https://") {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    // Track whether we're between <a>...</a> (content of an anchor element)
    let mut inside_anchor = false;
    // Track whether we're inside a `<...>` tag declaration itself
    let mut inside_tag = false;

    while i < len {
        if is_markup {
            if chars[i] == '<' {
                inside_tag = true;
                let rest: String = chars[i..].iter().collect();
                if rest.starts_with("<a ") || rest.starts_with("<a>") {
                    inside_anchor = true;
                } else if rest.starts_with("</a>") || rest.starts_with("</a ") {
                    inside_anchor = false;
                }
                result.push(chars[i]);
                i += 1;
                continue;
            }
            if inside_tag {
                if chars[i] == '>' {
                    inside_tag = false;
                }
                result.push(chars[i]);
                i += 1;
                continue;
            }
        }

        // Look for URL start (only outside anchor elements and tag declarations)
        if !inside_anchor {
            let rest: String = chars[i..].iter().collect();
            if rest.starts_with("http://") || rest.starts_with("https://") {
                let url_end = find_url_end(&rest);
                let url = &rest[..url_end];
                let domain = extract_domain(url);
                result.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    url,
                    glib::markup_escape_text(domain)
                ));
                i += url.chars().count();
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Remove `<a …>` opening tags and `</a>` closing tags, keeping their content.
///
/// This is used before `pango::parse_markup` validation because Pango does not
/// recognise `<a>` tags even though GTK labels render them.
fn strip_anchor_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch == '<' {
            // Collect the potential tag
            let mut tag = String::new();
            for c in chars.by_ref() {
                tag.push(c);
                if c == '>' {
                    break;
                }
            }
            let tag_lower = tag.to_ascii_lowercase();
            // Drop <a ...> and </a> tags, keep everything else
            if tag_lower.starts_with("<a ") || tag_lower.starts_with("<a>") || tag_lower == "</a>" {
                continue;
            }
            result.push_str(&tag);
        } else {
            result.push(ch);
            chars.next();
        }
    }

    result
}

/// Find the end index (byte offset) of a URL starting at the beginning of `s`.
///
/// Stops at whitespace, `<`, `>`, or end of string. Strips trailing punctuation
/// that is unlikely to be part of the URL (`.`, `,`, `)`, `!`, `?`, `;`, `:`).
fn find_url_end(s: &str) -> usize {
    let end = s
        .find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"')
        .unwrap_or(s.len());

    let url = &s[..end];
    // Strip trailing punctuation that is commonly not part of URLs
    let trimmed =
        url.trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | '!' | '?' | ';' | ':'));
    if trimmed.is_empty() {
        end
    } else {
        trimmed.len()
    }
}

/// Extract the display domain from a URL.
///
/// Strips the scheme (`http://`, `https://`) and leading `www.`,
/// then takes everything up to the first `/`.
fn extract_domain(url: &str) -> &str {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let without_www = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);

    match without_www.find('/') {
        Some(pos) => &without_www[..pos],
        None => without_www,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- prepare_title --

    #[test]
    fn title_escapes_ampersand() {
        assert_eq!(prepare_title("Tom & Jerry"), "Tom &amp; Jerry");
    }

    #[test]
    fn title_escapes_angle_brackets() {
        assert_eq!(prepare_title("a < b > c"), "a &lt; b &gt; c");
    }

    #[test]
    fn title_empty_string() {
        assert_eq!(prepare_title(""), "");
    }

    // -- prepare_description --

    #[test]
    fn description_preserves_valid_markup() {
        let input = "<b>bold</b> and <i>italic</i>";
        let result = prepare_description(input);
        assert_eq!(result, input);
    }

    #[test]
    fn description_escapes_invalid_markup() {
        let result = prepare_description("hello & goodbye");
        assert_eq!(result, "hello &amp; goodbye");
    }

    #[test]
    fn description_slack_url_with_ampersand() {
        let input = "check https://youtube.com/watch?v=xxx&list=yyy";
        let result = prepare_description(input);
        // The & gets escaped, URL gets linkified
        assert!(result.contains("&amp;"));
        assert!(result.contains("<a href="));
        assert!(result.contains("youtube.com"));
    }

    #[test]
    fn description_empty_string() {
        assert_eq!(prepare_description(""), "");
    }

    #[test]
    fn description_preserves_existing_anchor_tags() {
        let input = "<a href=\"https://example.com\">click here</a>";
        let result = prepare_description(input);
        assert_eq!(result, input);
    }

    // -- linkify_urls --

    #[test]
    fn linkify_bare_url() {
        let result = linkify_urls("See https://example.com for details", false);
        assert_eq!(
            result,
            "See <a href=\"https://example.com\">example.com</a> for details"
        );
    }

    #[test]
    fn linkify_url_with_path() {
        let result = linkify_urls("Visit https://example.com/page", false);
        assert_eq!(
            result,
            "Visit <a href=\"https://example.com/page\">example.com</a>"
        );
    }

    #[test]
    fn linkify_skips_existing_anchor_in_markup() {
        let input = "<a href=\"https://example.com\">link</a> and https://other.com";
        let result = linkify_urls(input, true);
        assert!(result.contains("<a href=\"https://example.com\">link</a>"));
        assert!(result.contains("<a href=\"https://other.com\">other.com</a>"));
    }

    #[test]
    fn linkify_no_urls() {
        let input = "no urls here";
        assert_eq!(linkify_urls(input, false), input);
    }

    #[test]
    fn linkify_trailing_punctuation_stripped() {
        let result = linkify_urls("Go to https://example.com.", false);
        assert_eq!(
            result,
            "Go to <a href=\"https://example.com\">example.com</a>."
        );
    }

    // -- extract_domain --

    #[test]
    fn domain_https_with_www_and_path() {
        assert_eq!(extract_domain("https://www.youtube.com/path"), "youtube.com");
    }

    #[test]
    fn domain_http_no_www() {
        assert_eq!(extract_domain("http://example.com/foo"), "example.com");
    }

    #[test]
    fn domain_no_path() {
        assert_eq!(extract_domain("https://example.com"), "example.com");
    }

    #[test]
    fn domain_with_port() {
        assert_eq!(
            extract_domain("https://localhost:3000/api"),
            "localhost:3000"
        );
    }
}
