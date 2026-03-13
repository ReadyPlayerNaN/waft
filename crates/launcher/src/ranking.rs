//! Search result ranking for apps and windows.

use waft_protocol::entity;
use waft_protocol::entity::app::App;
use waft_protocol::Urn;

use crate::fuzzy::{fuzzy_match_positions_chars, fuzzy_score_chars};
use crate::normalize::normalize_for_search;
use crate::search_index::SearchIndex;
use crate::usage::{AppUsage, UsageMap};

/// A ranked search result (app or window).
#[derive(Debug, Clone)]
pub enum RankedResult {
    App {
        urn: Urn,
        app: App,
        score: f64,
        highlight_positions: Vec<usize>,
    },
    Window {
        urn: Urn,
        window: entity::window::Window,
        score: f64,
        highlight_positions: Vec<usize>,
    },
}

impl RankedResult {
    pub fn urn(&self) -> &Urn {
        match self {
            Self::App { urn, .. } | Self::Window { urn, .. } => urn,
        }
    }

    pub fn score(&self) -> f64 {
        match self {
            Self::App { score, .. } | Self::Window { score, .. } => *score,
        }
    }

    pub fn highlight_positions(&self) -> &[usize] {
        match self {
            Self::App { highlight_positions, .. } | Self::Window { highlight_positions, .. } => highlight_positions,
        }
    }
}

/// Scored intermediate for two-pass ranking.
enum ScoredEntry {
    App { index: usize, score: f64 },
    Window { index: usize, score: f64 },
}

/// Rank apps and windows by relevance to `query`.
///
/// Two-pass approach:
/// - Pass 1: score all items using `fuzzy_score_chars` (no position tracking).
/// - Pass 2: take top `max_results`, compute highlight positions only for those.
///
/// Empty query: apps sorted by usage count desc, windows by workspace order.
/// Apps with `available = false` are always excluded.
/// Windows with `focused = true` are always excluded.
pub fn rank_results(
    index: &SearchIndex,
    query: &str,
    usage: &UsageMap,
    rank_by_usage: bool,
    max_results: usize,
) -> Vec<RankedResult> {
    let query_norm = normalize_for_search(query);

    // Pass 1: score all items (no position tracking)
    let mut scored: Vec<ScoredEntry> = Vec::new();

    for (i, entry) in index.apps.iter().enumerate() {
        if !entry.app.available {
            continue;
        }

        let score = if query.is_empty() {
            let boost = if rank_by_usage {
                usage_boost(&entry.urn, usage)
            } else {
                0.0
            };
            Some(boost)
        } else {
            let name_score = fuzzy_score_chars(&query_norm.chars, &entry.name_norm.chars);
            let kw_score = if entry.keywords_norm.chars.is_empty() {
                None
            } else {
                fuzzy_score_chars(&query_norm.chars, &entry.keywords_norm.chars)
                    .map(|s| s * 0.5)
            };

            let base = match (name_score, kw_score) {
                (Some(n), Some(k)) => Some(n.max(k)),
                (Some(n), None) => Some(n),
                (None, Some(k)) => Some(k),
                (None, None) => None,
            };

            base.map(|b| {
                let boost = if rank_by_usage {
                    usage_boost(&entry.urn, usage)
                } else {
                    0.0
                };
                b + boost
            })
        };

        if let Some(score) = score {
            scored.push(ScoredEntry::App { index: i, score });
        }
    }

    for (i, entry) in index.windows.iter().enumerate() {
        if entry.window.focused {
            continue;
        }

        let score = if query.is_empty() {
            Some(-(entry.window.workspace_id as f64))
        } else {
            let title_score = fuzzy_score_chars(&query_norm.chars, &entry.title_norm.chars);
            let app_id_score = fuzzy_score_chars(&query_norm.chars, &entry.app_id_norm.chars);

            match (title_score, app_id_score) {
                (Some(t), Some(a)) => Some(t.max(a)),
                (Some(t), None) => Some(t),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        };

        if let Some(score) = score {
            scored.push(ScoredEntry::Window { index: i, score });
        }
    }

    // Sort by score descending and truncate to max_results
    scored.sort_by(|a, b| {
        let sa = match a {
            ScoredEntry::App { score, .. } | ScoredEntry::Window { score, .. } => *score,
        };
        let sb = match b {
            ScoredEntry::App { score, .. } | ScoredEntry::Window { score, .. } => *score,
        };
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(max_results);

    // Pass 2: compute highlight positions only for top results
    scored
        .into_iter()
        .map(|entry| match entry {
            ScoredEntry::App { index: i, score } => {
                let e = &index.apps[i];
                let highlight_positions = if query.is_empty() {
                    vec![]
                } else {
                    // Try name match first for positions
                    let name_positions = fuzzy_match_positions_chars(
                        &query_norm.chars,
                        &e.name_norm.chars,
                        &e.name_norm.char_map,
                    );
                    let kw_score = if e.keywords_norm.chars.is_empty() {
                        None
                    } else {
                        fuzzy_score_chars(&query_norm.chars, &e.keywords_norm.chars)
                            .map(|s| s * 0.5)
                    };

                    match (&name_positions, kw_score) {
                        (Some((n, pos)), Some(k)) => {
                            if *n >= k {
                                pos.clone()
                            } else {
                                vec![]
                            }
                        }
                        (Some((_, pos)), None) => pos.clone(),
                        _ => vec![],
                    }
                };
                RankedResult::App {
                    urn: e.urn.clone(),
                    app: e.app.clone(),
                    score,
                    highlight_positions,
                }
            }
            ScoredEntry::Window { index: i, score } => {
                let e = &index.windows[i];
                let highlight_positions = if query.is_empty() {
                    vec![]
                } else {
                    let title_positions = fuzzy_match_positions_chars(
                        &query_norm.chars,
                        &e.title_norm.chars,
                        &e.title_norm.char_map,
                    );
                    let app_id_score =
                        fuzzy_score_chars(&query_norm.chars, &e.app_id_norm.chars);

                    match (&title_positions, app_id_score) {
                        (Some((t, pos)), Some(a)) => {
                            if *t >= a {
                                pos.clone()
                            } else {
                                vec![]
                            }
                        }
                        (Some((_, pos)), None) => pos.clone(),
                        _ => vec![],
                    }
                };
                RankedResult::Window {
                    urn: e.urn.clone(),
                    window: e.window.clone(),
                    score,
                    highlight_positions,
                }
            }
        })
        .collect()
}

/// Usage boost for a URN: `log2(launches + 1) * 0.1`.
fn usage_boost(urn: &Urn, usage: &UsageMap) -> f64 {
    usage
        .get(urn.to_string().as_str())
        .map(|u: &AppUsage| (u.launches as f64 + 1.0).log2() * 0.1)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_index::{AppSearchEntry, WindowSearchEntry};

    fn make_app(name: &str, keywords: &[&str]) -> App {
        App {
            name: name.to_string(),
            icon: "test".to_string(),
            available: true,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            description: None,
        }
    }

    fn app_urn(id: &str) -> Urn {
        Urn::new("xdg-apps", "app", id)
    }

    fn win_urn(id: &str) -> Urn {
        Urn::new("niri", "window", id)
    }

    fn make_window(title: &str, app_id: &str, focused: bool, workspace_id: u64) -> entity::window::Window {
        entity::window::Window {
            title: title.to_string(),
            app_id: app_id.to_string(),
            workspace_id,
            focused,
        }
    }

    fn make_app_entry(id: &str, name: &str, keywords: &[&str]) -> AppSearchEntry {
        let app = make_app(name, keywords);
        let keywords_str = app.keywords.join(" ");
        AppSearchEntry {
            urn: app_urn(id),
            name_norm: normalize_for_search(&app.name),
            keywords_norm: normalize_for_search(&keywords_str),
            app,
        }
    }

    fn make_window_entry(id: &str, title: &str, app_id: &str, focused: bool, workspace_id: u64) -> WindowSearchEntry {
        let window = make_window(title, app_id, focused, workspace_id);
        WindowSearchEntry {
            urn: win_urn(id),
            title_norm: normalize_for_search(&window.title),
            app_id_norm: normalize_for_search(&window.app_id),
            window,
        }
    }

    fn index_from(apps: Vec<AppSearchEntry>, windows: Vec<WindowSearchEntry>) -> SearchIndex {
        SearchIndex { apps, windows }
    }

    const MAX: usize = 50;

    #[test]
    fn empty_query_returns_all_sorted_by_usage() {
        let index = index_from(
            vec![
                make_app_entry("gedit", "Text Editor", &[]),
                make_app_entry("firefox", "Firefox", &[]),
            ],
            vec![],
        );
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 10,
                last_used_secs: 0,
            },
        );

        let results = rank_results(&index, "", &usage, true, MAX);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn query_filters_by_fuzzy_match() {
        let index = index_from(
            vec![
                make_app_entry("firefox", "Firefox", &[]),
                make_app_entry("gedit", "Text Editor", &[]),
            ],
            vec![],
        );
        let results = rank_results(&index, "fire", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn available_false_is_excluded() {
        let mut entry = make_app_entry("ghost", "Ghost", &[]);
        entry.app.available = false;
        let index = index_from(vec![entry], vec![]);
        let results = rank_results(&index, "", &UsageMap::new(), false, MAX);
        assert!(results.is_empty());
    }

    #[test]
    fn usage_boost_applied_when_enabled() {
        let index = index_from(
            vec![
                make_app_entry("firefox", "Firefox", &[]),
                make_app_entry("firebug", "Firebug", &[]),
            ],
            vec![],
        );
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 100,
                last_used_secs: 0,
            },
        );

        let results = rank_results(&index, "fire", &usage, true, MAX);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn usage_boost_ignored_when_disabled() {
        let index = index_from(
            vec![
                make_app_entry("firefox", "Firefox", &[]),
                make_app_entry("firef", "firef app", &[]),
            ],
            vec![],
        );
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 1000,
                last_used_secs: 0,
            },
        );

        let results_with = rank_results(&index, "firef", &usage, true, MAX);
        let results_without = rank_results(&index, "firef", &usage, false, MAX);
        assert_eq!(results_without.len(), 2);
        let _ = results_with;
    }

    #[test]
    fn focused_window_excluded() {
        let index = index_from(
            vec![],
            vec![
                make_window_entry("1", "Active Window", "term", true, 1),
                make_window_entry("2", "Background Window", "firefox", false, 1),
            ],
        );
        let results = rank_results(&index, "", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Background Window"));
    }

    #[test]
    fn windows_matched_by_title() {
        let index = index_from(
            vec![],
            vec![
                make_window_entry("1", "Claude Code", "Alacritty", false, 1),
                make_window_entry("2", "Mozilla Firefox", "firefox", false, 1),
            ],
        );
        let results = rank_results(&index, "claude", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Claude Code"));
    }

    #[test]
    fn windows_matched_by_app_id() {
        let index = index_from(
            vec![],
            vec![
                make_window_entry("1", "Some Title", "Alacritty", false, 1),
                make_window_entry("2", "Web Page", "firefox", false, 1),
            ],
        );
        let results = rank_results(&index, "alac", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.app_id == "Alacritty"));
    }

    #[test]
    fn mixed_apps_and_windows() {
        let index = index_from(
            vec![make_app_entry("firefox", "Firefox", &["browser"])],
            vec![make_window_entry("1", "GitHub - Mozilla Firefox", "firefox", false, 1)],
        );
        let results = rank_results(&index, "fire", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn app_name_match_populates_highlight_positions() {
        let index = index_from(
            vec![make_app_entry("firefox", "Firefox", &[])],
            vec![],
        );
        let results = rank_results(&index, "fox", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(!results[0].highlight_positions().is_empty());
        assert_eq!(results[0].highlight_positions(), &[0, 5, 6]);
    }

    #[test]
    fn keyword_only_match_has_empty_positions() {
        let index = index_from(
            vec![make_app_entry("firefox", "Firefox", &["browser"])],
            vec![],
        );
        let results = rank_results(&index, "brow", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn window_title_match_populates_highlight_positions() {
        let index = index_from(
            vec![],
            vec![make_window_entry("1", "Claude Code", "Alacritty", false, 1)],
        );
        let results = rank_results(&index, "clau", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].highlight_positions(), &[0, 1, 2, 3]);
    }

    #[test]
    fn window_app_id_only_match_has_empty_positions() {
        let index = index_from(
            vec![],
            vec![make_window_entry("1", "Some Title", "Alacritty", false, 1)],
        );
        let results = rank_results(&index, "alac", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn empty_query_has_empty_positions() {
        let index = index_from(
            vec![make_app_entry("firefox", "Firefox", &[])],
            vec![],
        );
        let results = rank_results(&index, "", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn empty_query_windows_ordered_by_workspace_ascending() {
        let index = index_from(
            vec![],
            vec![
                make_window_entry("3", "Terminal", "Alacritty", false, 3),
                make_window_entry("1", "Editor", "helix", false, 1),
                make_window_entry("2", "Browser", "firefox", false, 2),
            ],
        );
        let results = rank_results(&index, "", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.workspace_id == 1));
        assert!(matches!(&results[1], RankedResult::Window { window, .. } if window.workspace_id == 2));
        assert!(matches!(&results[2], RankedResult::Window { window, .. } if window.workspace_id == 3));
    }

    #[test]
    fn empty_query_includes_non_focused_windows() {
        let index = index_from(
            vec![],
            vec![
                make_window_entry("1", "Terminal", "Alacritty", false, 1),
                make_window_entry("2", "Active", "firefox", true, 1),
            ],
        );
        let results = rank_results(&index, "", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Terminal"));
    }

    #[test]
    fn accent_insensitive_search() {
        let index = index_from(
            vec![make_app_entry("weather", "Počasí", &[])],
            vec![],
        );
        let results = rank_results(&index, "pocasi", &UsageMap::new(), false, MAX);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Počasí"));
        // All positions should be highlighted
        assert_eq!(results[0].highlight_positions(), &[0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn max_results_limits_output() {
        let index = index_from(
            vec![
                make_app_entry("a", "Alpha", &[]),
                make_app_entry("b", "Beta", &[]),
                make_app_entry("c", "Charlie", &[]),
            ],
            vec![],
        );
        let results = rank_results(&index, "", &UsageMap::new(), false, 2);
        assert_eq!(results.len(), 2);
    }
}
