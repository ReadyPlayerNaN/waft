/// Generate markdown documentation for a widget type
pub fn generate_markdown(
    widget_name: &str,
    description: &str,
    states: &[(String, String, Option<String>)], // (name, description, optional screenshot_path)
    screenshots_enabled: bool,
) -> String {
    let mut content = String::new();

    // Title and description
    content.push_str(&format!("# {}\n\n", capitalize_widget_name(widget_name)));
    content.push_str(&format!("{}\n\n", description));

    // Note about screenshots if disabled
    if !screenshots_enabled {
        content.push_str("> **Note:** Screenshots are not available. ");
        content.push_str("To generate screenshots, run with X11 available or use Xvfb.\n\n");
    }

    // Table of contents
    content.push_str("## States\n\n");
    for (name, _, _) in states {
        let anchor = name.to_lowercase().replace(' ', "-");
        content.push_str(&format!("- [{}](#{})\n", name, anchor));
    }
    content.push_str("\n");

    // State sections
    for (name, description, screenshot_path) in states {
        content.push_str(&format!("## {}\n\n", name));
        content.push_str(&format!("{}\n\n", description));

        if let Some(path) = screenshot_path {
            content.push_str(&format!("![{}]({})\n\n", name, path));
        } else {
            content.push_str("*Screenshot not available*\n\n");
        }
    }

    content
}

/// Convert kebab-case widget name to Title Case
fn capitalize_widget_name(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize_widget_name() {
        assert_eq!(capitalize_widget_name("feature-toggle"), "Feature Toggle");
        assert_eq!(capitalize_widget_name("menu-row"), "Menu Row");
        assert_eq!(capitalize_widget_name("label"), "Label");
    }

    #[test]
    fn test_generate_markdown_basic() {
        let states = vec![
            ("State 1".to_string(), "Description 1".to_string(), Some("screenshots/state1.png".to_string())),
            ("State 2".to_string(), "Description 2".to_string(), Some("screenshots/state2.png".to_string())),
        ];

        let md = generate_markdown("test-widget", "A test widget", &states, true);

        assert!(md.contains("# Test Widget"));
        assert!(md.contains("A test widget"));
        assert!(md.contains("## State 1"));
        assert!(md.contains("Description 1"));
        assert!(md.contains("![State 1](screenshots/state1.png)"));
        assert!(md.contains("## State 2"));
    }

    #[test]
    fn test_generate_markdown_without_screenshots() {
        let states = vec![
            ("State 1".to_string(), "Description 1".to_string(), None),
            ("State 2".to_string(), "Description 2".to_string(), None),
        ];

        let md = generate_markdown("test-widget", "A test widget", &states, false);

        assert!(md.contains("# Test Widget"));
        assert!(md.contains("Note:**"));
        assert!(md.contains("*Screenshot not available*"));
    }
}
