//! `.desktop` file parser.

use std::collections::HashMap;

/// A parsed `.desktop` file entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub localized_names: HashMap<String, String>,
}

impl DesktopEntry {
    pub fn resolve_name(&self, locale: &str) -> &str {
        // Normalise BCP47 separators to POSIX: "pt-BR" → "pt_BR"
        // .desktop files always use POSIX underscore format per XDG spec.
        let posix_locale: std::borrow::Cow<str> = if locale.contains('-') {
            locale.replace('-', "_").into()
        } else {
            locale.into()
        };

        // 1. Exact match (now works for both "cs_CZ" and "cs-CZ" → "cs_CZ")
        if let Some(name) = self.localized_names.get(posix_locale.as_ref()) {
            return name;
        }
        // 2. Language-only prefix (split on '-' or '_')
        let lang = posix_locale.split(['-', '_']).next().unwrap_or("");
        if !lang.is_empty() && let Some(name) = self.localized_names.get(lang) {
            return name;
        }
        // 3. Fallback
        &self.name
    }
}

/// Parse a `.desktop` file text. Returns `None` if the file should be skipped
/// (wrong Type, NoDisplay=true, Hidden=true, or missing required fields).
pub fn parse_desktop_entry(content: &str) -> Option<DesktopEntry> {
    let mut in_desktop_entry = false;
    let mut entry_type = String::new();
    let mut name = String::new();
    let mut icon = String::new();
    let mut exec = String::new();
    let mut description = None;
    let mut keywords = Vec::new();
    let mut no_display = false;
    let mut hidden = false;
    let mut localized_names: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if let Some(locale_key) = key.strip_prefix("Name[").and_then(|s| s.strip_suffix(']')) {
                localized_names.insert(locale_key.to_string(), value.trim().to_string());
                continue;
            }
            // Ignore other locale-specific keys like Comment[fr]=...
            if key.contains('[') {
                continue;
            }
            let value = value.trim();
            match key {
                "Type" => entry_type = value.to_string(),
                "Name" => name = value.to_string(),
                "Icon" => icon = value.to_string(),
                "Exec" => exec = value.to_string(),
                "Comment" => description = Some(value.to_string()),
                "Keywords" => {
                    keywords = value
                        .split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
                "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }

    if entry_type != "Application" || no_display || hidden || name.is_empty() || exec.is_empty() {
        return None;
    }

    Some(DesktopEntry {
        name,
        icon: if icon.is_empty() {
            "application-x-executable".to_string()
        } else {
            icon
        },
        exec,
        description,
        keywords,
        localized_names,
    })
}

/// Strip field codes from an Exec= value (e.g. %f, %u, %U, %F, %i, %c, %k).
pub fn strip_exec_field_codes(exec: &str) -> String {
    let field_codes = [
        "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k", "%v", "%m",
    ];
    let mut result = exec.to_string();
    for code in field_codes {
        result = result.replace(code, "");
    }
    // Collapse multiple spaces
    let parts: Vec<&str> = result.split_whitespace().collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX_DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=Firefox Web Browser
Icon=firefox
Exec=firefox %u
Comment=Browse the World Wide Web
Keywords=web;browser;internet;
"#;

    const NODISPLAY_DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=Hidden App
Icon=hidden
Exec=hidden
NoDisplay=true
"#;

    const NON_APP_DESKTOP: &str = r#"[Desktop Entry]
Type=Link
Name=Some Link
"#;

    const MINIMAL_DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=MinApp
Icon=minapp
Exec=minapp
"#;

    const LOCALIZED_DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=Firefox Web Browser
Name[cs]=Webový prohlížeč Firefox
Name[de]=Firefox Webbrowser
Icon=firefox
Exec=firefox %u
"#;

    #[test]
    fn parses_full_entry() {
        let entry = parse_desktop_entry(FIREFOX_DESKTOP).unwrap();
        assert_eq!(entry.name, "Firefox Web Browser");
        assert_eq!(entry.icon, "firefox");
        assert_eq!(entry.exec, "firefox %u");
        assert_eq!(
            entry.description,
            Some("Browse the World Wide Web".to_string())
        );
        assert_eq!(entry.keywords, vec!["web", "browser", "internet"]);
    }

    #[test]
    fn skips_nodisplay() {
        assert!(parse_desktop_entry(NODISPLAY_DESKTOP).is_none());
    }

    #[test]
    fn skips_non_application_type() {
        assert!(parse_desktop_entry(NON_APP_DESKTOP).is_none());
    }

    #[test]
    fn parses_minimal_entry() {
        let entry = parse_desktop_entry(MINIMAL_DESKTOP).unwrap();
        assert_eq!(entry.name, "MinApp");
        assert_eq!(entry.description, None);
        assert!(entry.keywords.is_empty());
    }

    #[test]
    fn strips_exec_field_codes() {
        assert_eq!(strip_exec_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_exec_field_codes("app %f --flag"), "app --flag");
        assert_eq!(strip_exec_field_codes("app"), "app");
        assert_eq!(strip_exec_field_codes("app %U %F"), "app");
    }

    #[test]
    fn collects_localized_names() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.name, "Firefox Web Browser");
        assert_eq!(
            entry.localized_names.get("cs").map(String::as_str),
            Some("Webový prohlížeč Firefox")
        );
        assert_eq!(
            entry.localized_names.get("de").map(String::as_str),
            Some("Firefox Webbrowser")
        );
    }

    #[test]
    fn localized_names_empty_for_unlocalized_entry() {
        let entry = parse_desktop_entry(FIREFOX_DESKTOP).unwrap();
        assert!(entry.localized_names.is_empty());
    }

    #[test]
    fn resolve_name_exact_locale_match() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.resolve_name("cs"), "Webový prohlížeč Firefox");
    }

    #[test]
    fn resolve_name_language_only_match() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.resolve_name("cs_CZ"), "Webový prohlížeč Firefox");
    }

    #[test]
    fn resolve_name_bcp47_language_only_match() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.resolve_name("cs-CZ"), "Webový prohlížeč Firefox");
    }

    #[test]
    fn resolve_name_falls_back_to_base_name() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.resolve_name("ja"), "Firefox Web Browser");
    }

    #[test]
    fn resolve_name_empty_locale_falls_back() {
        let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
        assert_eq!(entry.resolve_name(""), "Firefox Web Browser");
    }

    #[test]
    fn resolve_name_posix_region_match() {
        let content = r#"[Desktop Entry]
Type=Application
Name=Some App
Name[pt_BR]=Aplicativo
Icon=app
Exec=app
"#;
        let entry = parse_desktop_entry(content).unwrap();
        // BCP47 input "pt-BR" should match POSIX key "pt_BR"
        assert_eq!(entry.resolve_name("pt-BR"), "Aplicativo");
        // POSIX input "pt_BR" should also match
        assert_eq!(entry.resolve_name("pt_BR"), "Aplicativo");
        // Language-only fallback (no bare "pt" key) → falls back to base name
        assert_eq!(entry.resolve_name("pt"), "Some App");
    }
}
