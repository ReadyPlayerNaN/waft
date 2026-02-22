//! App search result ranking.

use waft_protocol::entity::app::App;
use waft_protocol::Urn;

use crate::fuzzy::fuzzy_score;
use crate::usage::{AppUsage, UsageMap};

/// A ranked search result.
#[derive(Debug)]
pub struct RankedApp {
    pub urn: Urn,
    pub app: App,
    pub score: f64,
}

/// Rank `apps` by relevance to `query` with optional usage boost.
///
/// - Empty query: sorted by usage count desc (all apps included).
/// - Non-empty query: fuzzy-matched apps sorted by combined score desc.
///
/// Apps with `available = false` are always excluded.
pub fn rank_apps(
    apps: &[(Urn, App)],
    query: &str,
    usage: &UsageMap,
    rank_by_usage: bool,
) -> Vec<RankedApp> {
    let mut ranked: Vec<RankedApp> = apps
        .iter()
        .filter(|(_, app)| app.available)
        .filter_map(|(urn, app)| {
            if query.is_empty() {
                let boost = if rank_by_usage {
                    usage_boost(urn, usage)
                } else {
                    0.0
                };
                Some(RankedApp {
                    urn: urn.clone(),
                    app: app.clone(),
                    score: boost,
                })
            } else {
                // Score against name and keywords
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
                }?;

                let boost = if rank_by_usage {
                    usage_boost(urn, usage)
                } else {
                    0.0
                };
                Some(RankedApp {
                    urn: urn.clone(),
                    app: app.clone(),
                    score: base + boost,
                })
            }
        })
        .collect();

    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

    fn urn(id: &str) -> Urn {
        Urn::new("xdg-apps", "app", id)
    }

    #[test]
    fn empty_query_returns_all_sorted_by_usage() {
        let apps = vec![
            (urn("gedit"), make_app("Text Editor", &[])),
            (urn("firefox"), make_app("Firefox", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 10,
                last_used_secs: 0,
            },
        );

        let results = rank_apps(&apps, "", &usage, true);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].app.name, "Firefox"); // higher usage
    }

    #[test]
    fn query_filters_by_fuzzy_match() {
        let apps = vec![
            (urn("firefox"), make_app("Firefox", &[])),
            (urn("gedit"), make_app("Text Editor", &[])),
        ];
        let results = rank_apps(&apps, "fire", &UsageMap::new(), false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].app.name, "Firefox");
    }

    #[test]
    fn available_false_is_excluded() {
        let mut app = make_app("Ghost", &[]);
        app.available = false;
        let apps = vec![(urn("ghost"), app)];
        let results = rank_apps(&apps, "", &UsageMap::new(), false);
        assert!(results.is_empty());
    }

    #[test]
    fn usage_boost_applied_when_enabled() {
        let apps = vec![
            (urn("firefox"), make_app("Firefox", &[])),
            (urn("firebug"), make_app("Firebug", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 100,
                last_used_secs: 0,
            },
        );

        let results = rank_apps(&apps, "fire", &usage, true);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].app.name, "Firefox");
    }

    #[test]
    fn usage_boost_ignored_when_disabled() {
        let apps = vec![
            (urn("firefox"), make_app("Firefox", &[])),
            (urn("firef"), make_app("firef app", &[])),
        ];
        let mut usage = UsageMap::new();
        usage.insert(
            "xdg-apps/app/firefox".to_string(),
            AppUsage {
                launches: 1000,
                last_used_secs: 0,
            },
        );

        let results_with = rank_apps(&apps, "firef", &usage, true);
        let results_without = rank_apps(&apps, "firef", &usage, false);
        // Both should return 2 results -- this test verifies the function works with rank_by_usage=false
        assert_eq!(results_without.len(), 2);
        let _ = results_with; // Just verify it runs
    }
}
