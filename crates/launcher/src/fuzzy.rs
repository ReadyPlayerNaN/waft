//! Simple fuzzy matching with scoring.

use gtk::glib;

/// Score how well `query` matches `target`.
///
/// Returns `None` if the query does not fuzzy-match the target at all.
/// Returns `Some(score)` where higher is better.
///
/// Algorithm:
/// - Each character in the query must appear in the target in order.
/// - Score = matched_chars / target_chars.
/// - Bonus for contiguous run: each additional contiguous character adds 0.1.
/// - Bonus for prefix match (query is a prefix of target): +0.5.
pub fn fuzzy_score(query: &str, target: &str) -> Option<f64> {
    fuzzy_match_positions(query, target).map(|(score, _)| score)
}

/// Score how well `query` matches `target`, returning matched char positions.
///
/// Returns `None` if the query does not fuzzy-match the target at all.
/// Returns `Some((score, positions))` where positions are char indices into `target`.
/// Empty query returns `Some((0.0, vec![]))`.
pub fn fuzzy_match_positions(query: &str, target: &str) -> Option<(f64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0.0, vec![]));
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    let mut qi = 0;
    let mut last_match: Option<usize> = None;
    let mut contiguous_bonus = 0.0;
    let mut matched = 0usize;
    let mut first_match: Option<usize> = None;
    let mut positions: Vec<usize> = Vec::new();

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            // Contiguous bonus
            if let Some(last) = last_match && last + 1 == ti {
                contiguous_bonus += 0.1;
            }
            last_match = Some(ti);
            matched += 1;
            positions.push(ti);
            qi += 1;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    let base_score = matched as f64 / target_lower.len().max(1) as f64;
    let prefix_bonus = if first_match == Some(0) { 0.5 } else { 0.0 };

    Some((base_score + contiguous_bonus + prefix_bonus, positions))
}

/// Score how well `query` fuzzy-matches `target` on pre-normalized char slices.
///
/// No position tracking — faster than `fuzzy_match_positions_chars`.
/// Returns `None` if the query does not match.
pub fn fuzzy_score_chars(query: &[char], target: &[char]) -> Option<f64> {
    if query.is_empty() {
        return Some(0.0);
    }

    let mut qi = 0;
    let mut last_match: Option<usize> = None;
    let mut contiguous_bonus = 0.0;
    let mut matched = 0usize;
    let mut first_match: Option<usize> = None;

    for (ti, &tc) in target.iter().enumerate() {
        if qi < query.len() && tc == query[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            if let Some(last) = last_match && last + 1 == ti {
                contiguous_bonus += 0.1;
            }
            last_match = Some(ti);
            matched += 1;
            qi += 1;
        }
    }

    if qi < query.len() {
        return None;
    }

    let base_score = matched as f64 / target.len().max(1) as f64;
    let prefix_bonus = if first_match == Some(0) { 0.5 } else { 0.0 };
    Some(base_score + contiguous_bonus + prefix_bonus)
}

/// Score and return matched positions, mapped back to original string indices via `char_map`.
///
/// Matches on pre-normalized char slices but maps positions through `char_map`
/// so highlight indices correspond to the original (un-normalized) string.
pub fn fuzzy_match_positions_chars(
    query: &[char],
    target: &[char],
    char_map: &[usize],
) -> Option<(f64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0.0, vec![]));
    }

    let mut qi = 0;
    let mut last_match: Option<usize> = None;
    let mut contiguous_bonus = 0.0;
    let mut matched = 0usize;
    let mut first_match: Option<usize> = None;
    let mut positions: Vec<usize> = Vec::new();

    for (ti, &tc) in target.iter().enumerate() {
        if qi < query.len() && tc == query[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            if let Some(last) = last_match && last + 1 == ti {
                contiguous_bonus += 0.1;
            }
            last_match = Some(ti);
            matched += 1;
            positions.push(char_map[ti]);
            qi += 1;
        }
    }

    if qi < query.len() {
        return None;
    }

    let base_score = matched as f64 / target.len().max(1) as f64;
    let prefix_bonus = if first_match == Some(0) { 0.5 } else { 0.0 };
    Some((base_score + contiguous_bonus + prefix_bonus, positions))
}

/// Build Pango markup that dims non-matched characters.
///
/// Matched characters pass through at full opacity; non-matched characters are
/// wrapped in `<span alpha="60%">` so they appear dimmed. This is theme-agnostic
/// (works with any CSS `color`, dark/light mode, and selection accent colors).
/// All text is escaped for safe use in Pango markup.
pub fn build_highlight_markup(text: &str, positions: &[usize]) -> String {
    if positions.is_empty() {
        return glib::markup_escape_text(text).into();
    }

    let pos_set: std::collections::HashSet<usize> = positions.iter().copied().collect();
    let mut result = String::new();
    let mut in_dim = false;

    for (i, ch) in text.chars().enumerate() {
        let matched = pos_set.contains(&i);
        if !matched && !in_dim {
            result.push_str("<span alpha=\"60%\">");
            in_dim = true;
        } else if matched && in_dim {
            result.push_str("</span>");
            in_dim = false;
        }
        let escaped: String = glib::markup_escape_text(&ch.to_string()).into();
        result.push_str(&escaped);
    }

    if in_dim {
        result.push_str("</span>");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores_highest() {
        let score = fuzzy_score("firefox", "firefox").unwrap();
        // Exact match: all chars matched, all contiguous, prefix match
        assert!(score > 1.0, "exact match should score > 1.0, got {score}");
    }

    #[test]
    fn prefix_match_scores_higher_than_middle_match() {
        let prefix = fuzzy_score("fir", "firefox").unwrap();
        let middle = fuzzy_score("fox", "firefox").unwrap();
        assert!(prefix > middle, "prefix match should score higher");
    }

    #[test]
    fn non_matching_query_returns_none() {
        assert!(fuzzy_score("xyz", "firefox").is_none());
    }

    #[test]
    fn subsequence_match_succeeds() {
        // 'ff' matches 'firefox' (f..f..)
        assert!(fuzzy_score("ff", "firefox").is_some());
    }

    #[test]
    fn empty_query_matches_everything_with_zero_score() {
        let score = fuzzy_score("", "firefox").unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("FF", "firefox").is_some());
        assert!(fuzzy_score("FIRE", "Firefox").is_some());
    }

    // -- fuzzy_match_positions tests --

    #[test]
    fn positions_exact_match() {
        let (score, positions) = fuzzy_match_positions("fire", "fire").unwrap();
        assert!(score > 1.0);
        assert_eq!(positions, vec![0, 1, 2, 3]);
    }

    #[test]
    fn positions_prefix() {
        let (_, positions) = fuzzy_match_positions("fir", "firefox").unwrap();
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn positions_non_contiguous() {
        let (_, positions) = fuzzy_match_positions("fx", "firefox").unwrap();
        assert_eq!(positions, vec![0, 6]);
    }

    #[test]
    fn positions_case_insensitive() {
        let (_, positions) = fuzzy_match_positions("FF", "firefox").unwrap();
        assert_eq!(positions, vec![0, 4]);
    }

    #[test]
    fn positions_empty_query() {
        let (score, positions) = fuzzy_match_positions("", "firefox").unwrap();
        assert_eq!(score, 0.0);
        assert!(positions.is_empty());
    }

    #[test]
    fn positions_non_match() {
        assert!(fuzzy_match_positions("xyz", "firefox").is_none());
    }

    // -- build_highlight_markup tests --

    #[test]
    fn markup_no_positions() {
        assert_eq!(build_highlight_markup("hello", &[]), "hello");
    }

    #[test]
    fn markup_single_char() {
        // 'h' matched, 'ello' dimmed
        assert_eq!(
            build_highlight_markup("hello", &[0]),
            "h<span alpha=\"60%\">ello</span>"
        );
    }

    #[test]
    fn markup_contiguous_run() {
        // 'hel' matched, 'lo' dimmed
        assert_eq!(
            build_highlight_markup("hello", &[0, 1, 2]),
            "hel<span alpha=\"60%\">lo</span>"
        );
    }

    #[test]
    fn markup_non_contiguous() {
        // 'h' and 'o' matched, 'ell' dimmed
        assert_eq!(
            build_highlight_markup("hello", &[0, 4]),
            "h<span alpha=\"60%\">ell</span>o"
        );
    }

    #[test]
    fn markup_special_chars() {
        // '<' matched, 'b>' and '&' dimmed
        assert_eq!(
            build_highlight_markup("<b>&", &[0]),
            "&lt;<span alpha=\"60%\">b&gt;&amp;</span>"
        );
    }

    #[test]
    fn markup_empty_positions() {
        assert_eq!(build_highlight_markup("a&b", &[]), "a&amp;b");
    }

    // -- fuzzy_score_chars tests --

    #[test]
    fn score_chars_matches_score_for_ascii() {
        let pairs = [
            ("fire", "firefox"),
            ("ff", "firefox"),
            ("fox", "firefox"),
            ("", "firefox"),
        ];
        for (q, t) in pairs {
            let original = fuzzy_score(q, t);
            let q_chars: Vec<char> = q.to_lowercase().chars().collect();
            let t_chars: Vec<char> = t.to_lowercase().chars().collect();
            let chars_result = fuzzy_score_chars(&q_chars, &t_chars);
            assert_eq!(
                original, chars_result,
                "mismatch for query={q:?} target={t:?}"
            );
        }
    }

    #[test]
    fn score_chars_no_match() {
        let q: Vec<char> = "xyz".chars().collect();
        let t: Vec<char> = "firefox".chars().collect();
        assert!(fuzzy_score_chars(&q, &t).is_none());
    }

    #[test]
    fn score_chars_accent_insensitive() {
        use crate::normalize::normalize_for_search;
        let q = normalize_for_search("pocasi");
        let t = normalize_for_search("Počasí");
        assert!(fuzzy_score_chars(&q.chars, &t.chars).is_some());
    }

    // -- fuzzy_match_positions_chars tests --

    #[test]
    fn positions_chars_maps_back_to_original() {
        use crate::normalize::normalize_for_search;
        let q = normalize_for_search("pocasi");
        let t = normalize_for_search("Počasí");
        let (_, positions) =
            fuzzy_match_positions_chars(&q.chars, &t.chars, &t.char_map).unwrap();
        // All 6 chars of "Počasí" should be highlighted
        assert_eq!(positions, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn positions_chars_partial_match() {
        use crate::normalize::normalize_for_search;
        let q = normalize_for_search("cafe");
        let t = normalize_for_search("Café Latte");
        let (_, positions) =
            fuzzy_match_positions_chars(&q.chars, &t.chars, &t.char_map).unwrap();
        assert_eq!(positions, vec![0, 1, 2, 3]);
    }

    #[test]
    fn markup_multibyte_chars() {
        // "café" has char indices 0='c', 1='a', 2='f', 3='é'
        // 'f' and 'é' matched, 'ca' dimmed
        assert_eq!(
            build_highlight_markup("café", &[2, 3]),
            "<span alpha=\"60%\">ca</span>fé"
        );
    }
}
