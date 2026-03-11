//! Search result ranking for apps and windows.

use waft_protocol::entity;
use waft_protocol::entity::app::App;
use waft_protocol::Urn;

use crate::fuzzy::fuzzy_match_positions;
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

/// Rank apps and windows by relevance to `query`.
///
/// - Empty query: apps sorted by usage count desc, windows sorted by order (most recently focused first).
/// - Non-empty query: fuzzy-matched results sorted by combined score desc.
///
/// Apps with `available = false` are always excluded.
/// Windows with `focused = true` are always excluded.
pub fn rank_results(
    apps: &[(Urn, App)],
    windows: &[(Urn, entity::window::Window)],
    query: &str,
    usage: &UsageMap,
    rank_by_usage: bool,
) -> Vec<RankedResult> {
    let mut ranked: Vec<RankedResult> = Vec::new();

    // Rank apps
    for (urn, app) in apps.iter().filter(|(_, app)| app.available) {
        let result = if query.is_empty() {
            let boost = if rank_by_usage {
                usage_boost(urn, usage)
            } else {
                0.0
            };
            Some(RankedResult::App {
                urn: urn.clone(),
                app: app.clone(),
                score: boost,
                highlight_positions: vec![],
            })
        } else {
            let name_match = fuzzy_match_positions(query, &app.name);
            let keywords_str = app.keywords.join(" ");
            let kw_score = if keywords_str.is_empty() {
                None
            } else {
                fuzzy_match_positions(query, &keywords_str).map(|(s, _)| s * 0.5)
            };

            let (base, positions) = match (&name_match, kw_score) {
                (Some((n, pos)), Some(k)) => {
                    if *n >= k {
                        (Some(*n), pos.clone())
                    } else {
                        (Some(k), vec![])
                    }
                }
                (Some((n, pos)), None) => (Some(*n), pos.clone()),
                (None, Some(k)) => (Some(k), vec![]),
                (None, None) => (None, vec![]),
            };

            base.map(|base| {
                let boost = if rank_by_usage {
                    usage_boost(urn, usage)
                } else {
                    0.0
                };
                RankedResult::App {
                    urn: urn.clone(),
                    app: app.clone(),
                    score: base + boost,
                    highlight_positions: positions,
                }
            })
        };

        if let Some(r) = result {
            ranked.push(r);
        }
    }

    // Rank windows (exclude focused)
    for (urn, window) in windows.iter().filter(|(_, w)| !w.focused) {
        let result = if query.is_empty() {
            Some(RankedResult::Window {
                urn: urn.clone(),
                window: window.clone(),
                score: -(window.workspace_id as f64),
                highlight_positions: vec![],
            })
        } else {
            let title_match = fuzzy_match_positions(query, &window.title);
            let app_id_match = fuzzy_match_positions(query, &window.app_id);

            let (base, positions) = match (&title_match, &app_id_match) {
                (Some((t, t_pos)), Some((a, _))) => {
                    if t >= a {
                        (Some(*t), t_pos.clone())
                    } else {
                        (Some(*a), vec![])
                    }
                }
                (Some((t, t_pos)), None) => (Some(*t), t_pos.clone()),
                (None, Some((a, _))) => (Some(*a), vec![]),
                (None, None) => (None, vec![]),
            };

            base.map(|score| RankedResult::Window {
                urn: urn.clone(),
                window: window.clone(),
                score,
                highlight_positions: positions,
            })
        };

        if let Some(r) = result {
            ranked.push(r);
        }
    }

    ranked.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));
    ranked
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

    #[test]
    fn empty_query_returns_all_sorted_by_usage() {
        let apps = vec![
            (app_urn("gedit"), make_app("Text Editor", &[])),
            (app_urn("firefox"), make_app("Firefox", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 10,
                last_used_secs: 0,
            },
        );

        let results = rank_results(&apps, &[], "", &usage, true);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn query_filters_by_fuzzy_match() {
        let apps = vec![
            (app_urn("firefox"), make_app("Firefox", &[])),
            (app_urn("gedit"), make_app("Text Editor", &[])),
        ];
        let results = rank_results(&apps, &[], "fire", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn available_false_is_excluded() {
        let mut app = make_app("Ghost", &[]);
        app.available = false;
        let apps = vec![(app_urn("ghost"), app)];
        let results = rank_results(&apps, &[], "", &UsageMap::new(), false);
        assert!(results.is_empty());
    }

    #[test]
    fn usage_boost_applied_when_enabled() {
        let apps = vec![
            (app_urn("firefox"), make_app("Firefox", &[])),
            (app_urn("firebug"), make_app("Firebug", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 100,
                last_used_secs: 0,
            },
        );

        let results = rank_results(&apps, &[], "fire", &usage, true);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], RankedResult::App { app, .. } if app.name == "Firefox"));
    }

    #[test]
    fn usage_boost_ignored_when_disabled() {
        let apps = vec![
            (app_urn("firefox"), make_app("Firefox", &[])),
            (app_urn("firef"), make_app("firef app", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 1000,
                last_used_secs: 0,
            },
        );

        let results_with = rank_results(&apps, &[], "firef", &usage, true);
        let results_without = rank_results(&apps, &[], "firef", &usage, false);
        assert_eq!(results_without.len(), 2);
        let _ = results_with;
    }

    #[test]
    fn focused_window_excluded() {
        let windows = vec![
            (win_urn("1"), make_window("Active Window", "term", true, 1)),
            (win_urn("2"), make_window("Background Window", "firefox", false, 1)),
        ];
        let results = rank_results(&[], &windows, "", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Background Window"));
    }

    #[test]
    fn windows_matched_by_title() {
        let windows = vec![
            (win_urn("1"), make_window("Claude Code", "Alacritty", false, 1)),
            (win_urn("2"), make_window("Mozilla Firefox", "firefox", false, 1)),
        ];
        let results = rank_results(&[], &windows, "claude", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Claude Code"));
    }

    #[test]
    fn windows_matched_by_app_id() {
        let windows = vec![
            (win_urn("1"), make_window("Some Title", "Alacritty", false, 1)),
            (win_urn("2"), make_window("Web Page", "firefox", false, 1)),
        ];
        let results = rank_results(&[], &windows, "alac", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.app_id == "Alacritty"));
    }

    #[test]
    fn mixed_apps_and_windows() {
        let apps = vec![
            (app_urn("firefox"), make_app("Firefox", &["browser"])),
        ];
        let windows = vec![
            (win_urn("1"), make_window("GitHub - Mozilla Firefox", "firefox", false, 1)),
        ];
        let results = rank_results(&apps, &windows, "fire", &UsageMap::new(), false);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn app_name_match_populates_highlight_positions() {
        let apps = vec![(app_urn("firefox"), make_app("Firefox", &[]))];
        let results = rank_results(&apps, &[], "fox", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        // "fox" matches "Firefox" — greedy: 'f' at 0, 'o' at 5, 'x' at 6
        assert!(!results[0].highlight_positions().is_empty());
        assert_eq!(results[0].highlight_positions(), &[0, 5, 6]);
    }

    #[test]
    fn keyword_only_match_has_empty_positions() {
        let apps = vec![(app_urn("firefox"), make_app("Firefox", &["browser"]))];
        let results = rank_results(&apps, &[], "brow", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        // Matched by keyword only, so highlight positions should be empty
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn window_title_match_populates_highlight_positions() {
        let windows = vec![
            (win_urn("1"), make_window("Claude Code", "Alacritty", false, 1)),
        ];
        let results = rank_results(&[], &windows, "clau", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        // "clau" matches "Claude Code" at positions 0,1,2,3
        assert_eq!(results[0].highlight_positions(), &[0, 1, 2, 3]);
    }

    #[test]
    fn window_app_id_only_match_has_empty_positions() {
        let windows = vec![
            (win_urn("1"), make_window("Some Title", "Alacritty", false, 1)),
        ];
        let results = rank_results(&[], &windows, "alac", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        // Title "Some Title" doesn't match "alac", only app_id does
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn empty_query_has_empty_positions() {
        let apps = vec![(app_urn("firefox"), make_app("Firefox", &[]))];
        let results = rank_results(&apps, &[], "", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(results[0].highlight_positions().is_empty());
    }

    #[test]
    fn empty_query_windows_ordered_by_workspace_ascending() {
        let windows = vec![
            (win_urn("3"), make_window("Terminal", "Alacritty", false, 3)),
            (win_urn("1"), make_window("Editor", "helix", false, 1)),
            (win_urn("2"), make_window("Browser", "firefox", false, 2)),
        ];
        let results = rank_results(&[], &windows, "", &UsageMap::new(), false);
        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.workspace_id == 1));
        assert!(matches!(&results[1], RankedResult::Window { window, .. } if window.workspace_id == 2));
        assert!(matches!(&results[2], RankedResult::Window { window, .. } if window.workspace_id == 3));
    }

    #[test]
    fn empty_query_includes_non_focused_windows() {
        let windows = vec![
            (win_urn("1"), make_window("Terminal", "Alacritty", false, 1)),
            (win_urn("2"), make_window("Active", "firefox", true, 1)),  // focused, excluded
        ];
        let results = rank_results(&[], &windows, "", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Terminal"));
    }
}
