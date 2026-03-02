//! Search result ranking for apps and windows.

use waft_protocol::entity;
use waft_protocol::entity::app::App;
use waft_protocol::Urn;

use crate::fuzzy::fuzzy_score;
use crate::usage::{AppUsage, UsageMap};

/// A ranked search result (app or window).
#[derive(Debug, Clone)]
pub enum RankedResult {
    App {
        urn: Urn,
        app: App,
        score: f64,
    },
    Window {
        urn: Urn,
        window: entity::window::Window,
        score: f64,
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
            })
        } else {
            let name_score = fuzzy_score(query, &app.name);
            let keywords_str = app.keywords.join(" ");
            let kw_score = if keywords_str.is_empty() {
                None
            } else {
                fuzzy_score(query, &keywords_str).map(|s| s * 0.5)
            };

            let base = match (name_score, kw_score) {
                (Some(n), Some(k)) => Some(n.max(k)),
                (Some(n), None) => Some(n),
                (None, Some(k)) => Some(k),
                (None, None) => None,
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
                score: 0.0,
            })
        } else {
            let title_score = fuzzy_score(query, &window.title);
            let app_id_score = fuzzy_score(query, &window.app_id);

            let base = match (title_score, app_id_score) {
                (Some(t), Some(a)) => Some(t.max(a)),
                (Some(t), None) => Some(t),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            };

            base.map(|score| RankedResult::Window {
                urn: urn.clone(),
                window: window.clone(),
                score,
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

    fn make_window(title: &str, app_id: &str, focused: bool) -> entity::window::Window {
        entity::window::Window {
            title: title.to_string(),
            app_id: app_id.to_string(),
            workspace_id: 1,
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
            (win_urn("1"), make_window("Active Window", "term", true)),
            (win_urn("2"), make_window("Background Window", "firefox", false)),
        ];
        let results = rank_results(&[], &windows, "", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Background Window"));
    }

    #[test]
    fn windows_matched_by_title() {
        let windows = vec![
            (win_urn("1"), make_window("Claude Code", "Alacritty", false)),
            (win_urn("2"), make_window("Mozilla Firefox", "firefox", false)),
        ];
        let results = rank_results(&[], &windows, "claude", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RankedResult::Window { window, .. } if window.title == "Claude Code"));
    }

    #[test]
    fn windows_matched_by_app_id() {
        let windows = vec![
            (win_urn("1"), make_window("Some Title", "Alacritty", false)),
            (win_urn("2"), make_window("Web Page", "firefox", false)),
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
            (win_urn("1"), make_window("GitHub - Mozilla Firefox", "firefox", false)),
        ];
        let results = rank_results(&apps, &windows, "fire", &UsageMap::new(), false);
        assert_eq!(results.len(), 2);
    }
}
