//! Simple fuzzy matching with scoring.

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
    if query.is_empty() {
        return Some(0.0);
    }

    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    let mut qi = 0;
    let mut last_match: Option<usize> = None;
    let mut contiguous_bonus = 0.0;
    let mut matched = 0usize;
    let mut first_match: Option<usize> = None;

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
            qi += 1;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    let base_score = matched as f64 / target_lower.len().max(1) as f64;
    let prefix_bonus = if first_match == Some(0) { 0.5 } else { 0.0 };

    Some(base_score + contiguous_bonus + prefix_bonus)
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
}
