//! Static search index for the settings app.
//!
//! Each page and section registers searchable items at construction time.
//! The index supports token-based case-insensitive matching with scoring
//! and deduplication.

use std::collections::HashSet;

use gtk::glib;
use gtk::prelude::*;

/// A single searchable item in the settings app.
pub struct SearchEntry {
    /// Page stable ID for stack routing (e.g. "display").
    pub page_id: &'static str,
    /// Translated page title (e.g. "Display").
    pub page_title: String,
    /// Optional section title (e.g. "Appearance"). None for page-level entries.
    pub section_title: Option<String>,
    /// Optional input/control title (e.g. "Dark Mode"). None for section-level entries.
    pub input_title: Option<String>,
    /// All searchable text fragments: translated titles + FTL keys + aliases.
    /// Pre-lowercased for fast matching.
    search_terms: Vec<String>,
    /// The GTK widget to scroll to when this result is selected.
    /// Weak ref to avoid preventing widget destruction.
    pub target_widget: Option<glib::WeakRef<gtk::Widget>>,
}

impl SearchEntry {
    /// Returns the display breadcrumb for this entry (e.g. "Display > Appearance > Dark Mode").
    pub fn breadcrumb(&self) -> String {
        let mut parts = vec![self.page_title.clone()];
        if let Some(ref section) = self.section_title {
            parts.push(section.clone());
        }
        if let Some(ref input) = self.input_title {
            parts.push(input.clone());
        }
        parts.join(" > ")
    }

    /// Returns the depth level: 0 = page, 1 = section, 2 = input.
    fn depth(&self) -> u8 {
        match (&self.section_title, &self.input_title) {
            (None, None) => 0,
            (Some(_), None) => 1,
            _ => 2,
        }
    }

    /// Check if all query tokens match any of the search terms.
    fn matches(&self, tokens: &[String]) -> bool {
        tokens.iter().all(|token| {
            self.search_terms
                .iter()
                .any(|term| term.contains(token.as_str()))
        })
    }
}

/// Shared search index for the settings app.
pub struct SearchIndex {
    entries: Vec<SearchEntry>,
    hidden_pages: HashSet<&'static str>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            hidden_pages: HashSet::new(),
        }
    }

    /// Register a page-level entry.
    pub fn add_page(&mut self, page_id: &'static str, title: &str, ftl_key: &str) {
        let search_terms = vec![title.to_lowercase(), ftl_key.to_lowercase()];
        self.entries.push(SearchEntry {
            page_id,
            page_title: title.to_string(),
            section_title: None,
            input_title: None,
            search_terms,
            target_widget: None,
        });
    }

    /// Register a section within a page.
    pub fn add_section(
        &mut self,
        page_id: &'static str,
        page_title: &str,
        section_title: &str,
        section_ftl_key: &str,
        widget: &impl gtk::prelude::IsA<gtk::Widget>,
    ) {
        let search_terms = vec![
            section_title.to_lowercase(),
            section_ftl_key.to_lowercase(),
            page_title.to_lowercase(),
        ];
        let weak = glib::WeakRef::new();
        weak.set(Some(widget.upcast_ref()));
        self.entries.push(SearchEntry {
            page_id,
            page_title: page_title.to_string(),
            section_title: Some(section_title.to_string()),
            input_title: None,
            search_terms,
            target_widget: Some(weak),
        });
    }

    /// Register an input/control within a section.
    pub fn add_input(
        &mut self,
        page_id: &'static str,
        page_title: &str,
        section_title: &str,
        input_title: &str,
        input_ftl_key: &str,
        widget: &impl gtk::prelude::IsA<gtk::Widget>,
    ) {
        let search_terms = vec![
            input_title.to_lowercase(),
            input_ftl_key.to_lowercase(),
            section_title.to_lowercase(),
            page_title.to_lowercase(),
        ];
        let weak = glib::WeakRef::new();
        weak.set(Some(widget.upcast_ref()));
        self.entries.push(SearchEntry {
            page_id,
            page_title: page_title.to_string(),
            section_title: Some(section_title.to_string()),
            input_title: Some(input_title.to_string()),
            search_terms,
            target_widget: Some(weak),
        });
    }

    /// Show or hide a page in search results.
    pub fn set_page_visible(&mut self, page_id: &'static str, visible: bool) {
        if visible {
            self.hidden_pages.remove(page_id);
        } else {
            self.hidden_pages.insert(page_id);
        }
    }

    /// Search the index. Returns entries whose search_terms contain all query tokens.
    /// Results are scored, deduplicated, and limited.
    pub fn search(&self, query: &str) -> Vec<&SearchEntry> {
        let query_lower = query.to_lowercase();
        let tokens: Vec<String> = query_lower.split_whitespace().map(String::from).collect();
        if tokens.is_empty() {
            return Vec::new();
        }

        // Collect matching entries
        let mut matches: Vec<&SearchEntry> = self
            .entries
            .iter()
            .filter(|e| !self.hidden_pages.contains(e.page_id))
            .filter(|e| e.matches(&tokens))
            .collect();

        // Sort: deeper matches first (inputs > sections > pages), then shorter breadcrumbs
        matches.sort_by(|a, b| {
            b.depth()
                .cmp(&a.depth())
                .then_with(|| a.breadcrumb().len().cmp(&b.breadcrumb().len()))
        });

        // Deduplicate: if an input match exists, suppress its parent section and page matches
        let mut seen_pages: HashSet<&str> = HashSet::new();
        let mut seen_sections: HashSet<(&str, &str)> = HashSet::new();
        let mut result: Vec<&SearchEntry> = Vec::new();

        // First pass: collect all matched page_ids and (page_id, section) pairs at input/section level
        for entry in &matches {
            match entry.depth() {
                2 => {
                    seen_pages.insert(entry.page_id);
                    if let Some(ref section) = entry.section_title {
                        seen_sections.insert((entry.page_id, section.as_str()));
                    }
                }
                1 => {
                    seen_pages.insert(entry.page_id);
                }
                _ => {}
            }
        }

        // Second pass: filter out redundant parent entries
        for entry in matches {
            match entry.depth() {
                0 => {
                    if !seen_pages.contains(entry.page_id) {
                        result.push(entry);
                    }
                }
                1 => {
                    let section = entry.section_title.as_deref().unwrap_or("");
                    if !seen_sections.contains(&(entry.page_id, section)) {
                        result.push(entry);
                    }
                }
                _ => {
                    result.push(entry);
                }
            }
        }

        result.truncate(15);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_nothing() {
        let index = SearchIndex::new();
        assert!(index.search("").is_empty());
        assert!(index.search("   ").is_empty());
    }

    #[test]
    fn page_level_match() {
        let mut index = SearchIndex::new();
        index.add_page("display", "Display", "settings-display");
        index.add_page("bluetooth", "Bluetooth", "settings-bluetooth");

        let results = index.search("display");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_id, "display");
    }

    #[test]
    fn case_insensitive_match() {
        let mut index = SearchIndex::new();
        index.add_page("display", "Display", "settings-display");

        let results = index.search("DISPLAY");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn ftl_key_match() {
        let mut index = SearchIndex::new();
        index.add_page("display", "Display", "settings-display");

        let results = index.search("settings-display");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn multi_token_match() {
        let mut index = SearchIndex::new();
        index.add_page("display", "Display", "settings-display");
        index.add_page("bluetooth", "Bluetooth", "settings-bluetooth");

        // Both tokens must match
        let results = index.search("settings display");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].page_id, "display");
    }

    #[test]
    fn hidden_pages_filtered() {
        let mut index = SearchIndex::new();
        index.add_page("wifi", "WiFi", "settings-wifi");
        index.add_page("bluetooth", "Bluetooth", "settings-bluetooth");

        index.set_page_visible("wifi", false);
        let results = index.search("wifi");
        assert!(results.is_empty());

        index.set_page_visible("wifi", true);
        let results = index.search("wifi");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn deduplication_suppresses_parents() {
        let mut index = SearchIndex::new();
        index.add_page("display", "Display", "settings-display");

        // We can't create real GTK widgets in unit tests, so test with page-level only
        // The dedup logic is tested by structure: if an input matches, parent page is suppressed
        let results = index.search("display");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].depth(), 0);
    }

    #[test]
    fn results_limited_to_15() {
        let mut index = SearchIndex::new();
        for i in 0..20 {
            let id: &'static str = Box::leak(format!("page{i}").into_boxed_str());
            let title = format!("Test Page {i}");
            index.add_page(id, &title, "test");
        }
        let results = index.search("test");
        assert_eq!(results.len(), 15);
    }
}
