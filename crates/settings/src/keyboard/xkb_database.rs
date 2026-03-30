//! XKB database parser for variant information.
//!
//! Parses `/usr/share/X11/xkb/rules/base.lst` to extract available
//! variants for each keyboard layout.

/// A variant available for a specific keyboard layout.
#[derive(Debug, Clone)]
pub struct XkbVariant {
    /// Variant code (e.g. "qwerty", "dvorak").
    pub code: String,
    /// Description (e.g. "Czech (QWERTY)").
    pub description: String,
}

const BASE_LST_PATH: &str = "/usr/share/X11/xkb/rules/base.lst";

/// Get available variants for a specific layout code.
///
/// Parses the `! variant` section of `base.lst` and returns variants
/// that belong to the given layout. Returns an empty vec if the file
/// is missing or the layout has no variants.
pub fn get_variants_for_layout(layout: &str) -> Vec<XkbVariant> {
    match std::fs::read_to_string(BASE_LST_PATH) {
        Ok(content) => parse_variants_for_layout(&content, layout),
        Err(e) => {
            log::debug!("[keyboard] Failed to read {BASE_LST_PATH}: {e}");
            Vec::new()
        }
    }
}

/// Parse variants for a specific layout from base.lst content.
fn parse_variants_for_layout(content: &str, layout: &str) -> Vec<XkbVariant> {
    let mut variants = Vec::new();
    let mut in_variant_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "! variant" {
            in_variant_section = true;
            continue;
        }

        if trimmed.starts_with('!') {
            if in_variant_section {
                break;
            }
            continue;
        }

        if in_variant_section && !trimmed.is_empty() {
            // Format: "  variant_code    layout_code: Description"
            if let Some(variant) = parse_variant_line(trimmed, layout) {
                variants.push(variant);
            }
        }
    }

    variants
}

/// Parse a single variant line, returning Some if it belongs to the target layout.
///
/// Line format: `  qwerty          cz: Czech (QWERTY)`
fn parse_variant_line(line: &str, target_layout: &str) -> Option<XkbVariant> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let code = parts.next()?;
    let rest = parts.next()?.trim();

    // rest = "cz: Czech (QWERTY)"
    let colon_pos = rest.find(':')?;
    let layout_code = rest[..colon_pos].trim();

    if layout_code != target_layout {
        return None;
    }

    let description = rest[colon_pos + 1..].trim().to_string();

    Some(XkbVariant {
        code: code.to_string(),
        description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BASE_LST: &str = r#"! model
  pc104           Generic 104-key PC
  pc105           Generic 105-key PC

! layout
  us              English (US)
  cz              Czech
  de              German

! variant
  chr             us: Cherokee
  dvorak          us: English (Dvorak)
  colemak         us: English (Colemak)
  bksl            cz: Czech (with <\|>)
  qwerty          cz: Czech (QWERTY)
  ucw             cz: Czech (UCW, only strstrokes)
  nodeadkeys      de: German (no dead keys)
  dvorak          de: German (Dvorak)

! option
  grp             Switching to another layout
  grp:win_space_toggle Both Win keys together
"#;

    #[test]
    fn parse_us_variants() {
        let variants = parse_variants_for_layout(SAMPLE_BASE_LST, "us");
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0].code, "chr");
        assert_eq!(variants[0].description, "Cherokee");
        assert_eq!(variants[1].code, "dvorak");
        assert_eq!(variants[1].description, "English (Dvorak)");
        assert_eq!(variants[2].code, "colemak");
        assert_eq!(variants[2].description, "English (Colemak)");
    }

    #[test]
    fn parse_cz_variants() {
        let variants = parse_variants_for_layout(SAMPLE_BASE_LST, "cz");
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0].code, "bksl");
        assert_eq!(variants[1].code, "qwerty");
        assert_eq!(variants[2].code, "ucw");
    }

    #[test]
    fn parse_de_variants() {
        let variants = parse_variants_for_layout(SAMPLE_BASE_LST, "de");
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].code, "nodeadkeys");
        assert_eq!(variants[1].code, "dvorak");
    }

    #[test]
    fn parse_unknown_layout_returns_empty() {
        let variants = parse_variants_for_layout(SAMPLE_BASE_LST, "zz");
        assert!(variants.is_empty());
    }

    #[test]
    fn parse_empty_content_returns_empty() {
        let variants = parse_variants_for_layout("", "us");
        assert!(variants.is_empty());
    }

    #[test]
    fn parse_no_variant_section_returns_empty() {
        let content = "! layout\n  us    English (US)\n";
        let variants = parse_variants_for_layout(content, "us");
        assert!(variants.is_empty());
    }
}
