pub mod active_profile_section;
pub mod dnd_section;
pub mod combinator_editor;
pub mod group_form;
pub mod groups_section;
pub mod pattern_row;
pub mod profiles_section;

/// Generate a URL-safe ID from a human-readable name.
pub fn id_from_name(name: &str) -> String {
    let lowered = name.to_lowercase();
    let filtered: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    filtered
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_from_simple_name() {
        assert_eq!(id_from_name("Work Apps"), "work-apps");
    }

    #[test]
    fn id_from_name_with_punctuation() {
        assert_eq!(id_from_name("My  Test!@#Group"), "my-test-group");
    }

    #[test]
    fn id_from_name_trims_dashes() {
        assert_eq!(id_from_name("  leading trailing  "), "leading-trailing");
    }
}
