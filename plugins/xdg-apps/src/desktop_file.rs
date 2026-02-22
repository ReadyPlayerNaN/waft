//! `.desktop` file parser.

/// A parsed `.desktop` file entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
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
            // Ignore locale-specific keys like Name[fr]=...
            let key = key.trim();
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
}
