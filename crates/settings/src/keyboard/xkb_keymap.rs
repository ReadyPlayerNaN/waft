//! XKB symbols file parser for keyboard layout visualization.
//!
//! Parses XKB symbols files from `/usr/share/X11/xkb/symbols/` to extract
//! key code to character mappings. Used by the keyboard grid visualization
//! widget to display what each physical key produces in a given layout.

use std::collections::HashMap;
use std::path::PathBuf;

/// A grid of key labels organized by physical keyboard rows.
#[derive(Debug, Clone, Default)]
pub struct KeymapGrid {
    /// Top row keys (Q-row): AD01..AD12
    pub top_row: Vec<String>,
    /// Home row keys (A-row): AC01..AC11
    pub home_row: Vec<String>,
    /// Bottom row keys (Z-row): AB01..AB10
    pub bottom_row: Vec<String>,
}

/// Standard key codes for the three main letter rows on a QWERTY keyboard.
const TOP_ROW_KEYS: &[&str] = &[
    "AD01", "AD02", "AD03", "AD04", "AD05", "AD06", "AD07", "AD08", "AD09", "AD10", "AD11",
    "AD12",
];

const HOME_ROW_KEYS: &[&str] = &[
    "AC01", "AC02", "AC03", "AC04", "AC05", "AC06", "AC07", "AC08", "AC09", "AC10", "AC11",
];

const BOTTOM_ROW_KEYS: &[&str] = &[
    "AB01", "AB02", "AB03", "AB04", "AB05", "AB06", "AB07", "AB08", "AB09", "AB10",
];

/// Convert an XKB keysym name to its corresponding character.
///
/// Handles ASCII keysyms, Latin-1 supplement, and common Latin Extended characters.
/// Returns `None` for unknown or non-printable keysyms.
pub fn keysym_to_char(keysym: &str) -> Option<char> {
    // Single lowercase letter keysyms map directly (a-z)
    if keysym.len() == 1 {
        let ch = keysym.chars().next()?;
        if ch.is_ascii_lowercase() {
            return Some(ch);
        }
    }

    // XKB uses Unicode code points prefixed with U or 0x for some keysyms
    if let Some(hex) = keysym.strip_prefix("U+").or_else(|| keysym.strip_prefix("U")) {
        if let Ok(code) = u32::from_str_radix(hex, 16) {
            return char::from_u32(code);
        }
    }

    KEYSYM_TABLE.get(keysym).copied()
}

/// Parse an xkb_symbols block from file content.
///
/// Extracts key code to first-level keysym mappings from a specific variant
/// (or the default block if `variant` is empty).
///
/// The parser handles lines like:
/// ```text
/// key <AD01> { [ q, Q ] };
/// key <AC01> { [ a, A, aacute, Aacute ] };
/// ```
pub fn parse_symbols_block(content: &str, variant: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // Find the right xkb_symbols block
    let block_content = match find_symbols_block(content, variant) {
        Some(c) => c,
        None => return result,
    };

    for line in block_content.lines() {
        let trimmed = line.trim();

        // Match: key <CODE> { [ keysym, ... ] };
        if let Some(rest) = trimmed.strip_prefix("key") {
            let rest = rest.trim();
            if let Some(code_start) = rest.find('<') {
                if let Some(code_end) = rest.find('>') {
                    let code = &rest[code_start + 1..code_end];

                    // Find the bracket contents
                    if let Some(bracket_start) = rest.find('[') {
                        if let Some(bracket_end) = rest.find(']') {
                            let symbols_str = &rest[bracket_start + 1..bracket_end];
                            let first_keysym = symbols_str
                                .split(',')
                                .next()
                                .map(|s| s.trim().to_string());

                            if let Some(ks) = first_keysym {
                                if !ks.is_empty() {
                                    result.insert(code.to_string(), ks);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    result
}

/// Resolve include directives in an XKB symbols block.
///
/// Processes `include "layout(variant)"` directives recursively up to `depth` levels.
/// Merges all key mappings, with later definitions overriding earlier ones.
pub fn resolve_includes(
    content: &str,
    variant: &str,
    depth: u8,
    base_dir: &std::path::Path,
) -> HashMap<String, String> {
    if depth == 0 {
        return HashMap::new();
    }

    let mut result = HashMap::new();

    // Find the right xkb_symbols block
    let block_content = match find_symbols_block(content, variant) {
        Some(c) => c,
        None => return result,
    };

    for line in block_content.lines() {
        let trimmed = line.trim();

        // Handle include directives: include "layout(variant)"
        if let Some(rest) = trimmed.strip_prefix("include") {
            let rest = rest.trim();
            if let Some(include_ref) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                let (inc_layout, inc_variant) = parse_include_ref(include_ref);

                let inc_path = base_dir.join(&inc_layout);
                if let Ok(inc_content) = std::fs::read_to_string(&inc_path) {
                    let inc_map =
                        resolve_includes(&inc_content, &inc_variant, depth - 1, base_dir);
                    result.extend(inc_map);
                } else {
                    log::debug!(
                        "[xkb_keymap] Could not read include file: {}",
                        inc_path.display()
                    );
                }
            }
        }
    }

    // Apply direct key definitions (override includes)
    let direct = parse_symbols_block(content, variant);
    result.extend(direct);

    result
}

/// Load a keymap grid for a given XKB layout and variant.
///
/// Reads the symbols file from `/usr/share/X11/xkb/symbols/`, resolves includes,
/// and builds a `KeymapGrid` with character labels for each physical key.
pub fn load_keymap_grid(layout: &str, variant: &str) -> Option<KeymapGrid> {
    let base_dir = xkb_symbols_dir();
    let path = base_dir.join(layout);
    let content = std::fs::read_to_string(&path).ok()?;

    let key_map = resolve_includes(&content, variant, 8, &base_dir);

    Some(build_grid(&key_map))
}

// --- Internal helpers ---

/// Find the xkb_symbols block for a given variant within file content.
fn find_symbols_block<'a>(content: &'a str, variant: &str) -> Option<&'a str> {
    let target = if variant.is_empty() {
        // Find the first/default xkb_symbols block
        None
    } else {
        Some(variant)
    };

    let mut search_from = 0;
    while search_from < content.len() {
        let remaining = &content[search_from..];

        // Find next xkb_symbols declaration
        let decl_pos = remaining.find("xkb_symbols")?;
        let decl_start = search_from + decl_pos;

        // Find the opening brace
        let after_decl = &content[decl_start..];
        let brace_offset = after_decl.find('{')?;
        let header = &after_decl[..brace_offset];

        let matches = match target {
            None => true, // Accept first block
            Some(v) => {
                // Check if the header contains the variant name in quotes
                header.contains(&format!("\"{}\"", v))
            }
        };

        if matches {
            // Find the matching closing brace
            let block_start = decl_start + brace_offset + 1;
            if let Some(end) = find_matching_brace(content, block_start) {
                return Some(&content[block_start..end]);
            }
        }

        search_from = decl_start + brace_offset + 1;
    }

    None
}

/// Find the position of the matching closing brace, handling nesting.
fn find_matching_brace(content: &str, start: usize) -> Option<usize> {
    let mut depth = 1;
    for (i, ch) in content[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse an include reference like `"layout(variant)"` or `"layout"`.
fn parse_include_ref(reference: &str) -> (String, String) {
    if let Some(paren_start) = reference.find('(') {
        if let Some(paren_end) = reference.find(')') {
            let layout = reference[..paren_start].to_string();
            let variant = reference[paren_start + 1..paren_end].to_string();
            return (layout, variant);
        }
    }
    (reference.to_string(), String::new())
}

/// Build a KeymapGrid from a key code -> keysym map.
fn build_grid(key_map: &HashMap<String, String>) -> KeymapGrid {
    let resolve_key = |code: &str| -> String {
        key_map
            .get(code)
            .and_then(|ks| keysym_to_char(ks))
            .map(|ch| ch.to_string())
            .unwrap_or_default()
    };

    KeymapGrid {
        top_row: TOP_ROW_KEYS.iter().map(|k| resolve_key(k)).collect(),
        home_row: HOME_ROW_KEYS.iter().map(|k| resolve_key(k)).collect(),
        bottom_row: BOTTOM_ROW_KEYS.iter().map(|k| resolve_key(k)).collect(),
    }
}

/// Return the XKB symbols directory path.
fn xkb_symbols_dir() -> PathBuf {
    PathBuf::from("/usr/share/X11/xkb/symbols")
}

/// Static keysym name to character lookup table.
static KEYSYM_TABLE: std::sync::LazyLock<HashMap<&'static str, char>> =
    std::sync::LazyLock::new(|| {
        let mut m = HashMap::new();

        // ASCII uppercase letter keysyms: A-Z
        for (i, ch) in ('A'..='Z').enumerate() {
            let name: &'static str = [
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ][i];
            m.insert(name, ch);
        }

        // Digits
        for (i, ch) in ('0'..='9').enumerate() {
            let name: &'static str =
                ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"][i];
            m.insert(name, ch);
        }

        // Common punctuation keysyms
        m.insert("space", ' ');
        m.insert("exclam", '!');
        m.insert("at", '@');
        m.insert("numbersign", '#');
        m.insert("dollar", '$');
        m.insert("percent", '%');
        m.insert("asciicircum", '^');
        m.insert("ampersand", '&');
        m.insert("asterisk", '*');
        m.insert("parenleft", '(');
        m.insert("parenright", ')');
        m.insert("minus", '-');
        m.insert("underscore", '_');
        m.insert("equal", '=');
        m.insert("plus", '+');
        m.insert("bracketleft", '[');
        m.insert("bracketright", ']');
        m.insert("braceleft", '{');
        m.insert("braceright", '}');
        m.insert("backslash", '\\');
        m.insert("bar", '|');
        m.insert("semicolon", ';');
        m.insert("colon", ':');
        m.insert("apostrophe", '\'');
        m.insert("quotedbl", '"');
        m.insert("grave", '`');
        m.insert("asciitilde", '~');
        m.insert("comma", ',');
        m.insert("period", '.');
        m.insert("slash", '/');
        m.insert("less", '<');
        m.insert("greater", '>');
        m.insert("question", '?');

        // Latin-1 accented characters
        m.insert("agrave", 'à');
        m.insert("aacute", 'á');
        m.insert("acircumflex", 'â');
        m.insert("atilde", 'ã');
        m.insert("adiaeresis", 'ä');
        m.insert("aring", 'å');
        m.insert("ae", 'æ');
        m.insert("ccedilla", 'ç');
        m.insert("egrave", 'è');
        m.insert("eacute", 'é');
        m.insert("ecircumflex", 'ê');
        m.insert("ediaeresis", 'ë');
        m.insert("igrave", 'ì');
        m.insert("iacute", 'í');
        m.insert("icircumflex", 'î');
        m.insert("idiaeresis", 'ï');
        m.insert("eth", 'ð');
        m.insert("ntilde", 'ñ');
        m.insert("ograve", 'ò');
        m.insert("oacute", 'ó');
        m.insert("ocircumflex", 'ô');
        m.insert("otilde", 'õ');
        m.insert("odiaeresis", 'ö');
        m.insert("oslash", 'ø');
        m.insert("ugrave", 'ù');
        m.insert("uacute", 'ú');
        m.insert("ucircumflex", 'û');
        m.insert("udiaeresis", 'ü');
        m.insert("yacute", 'ý');
        m.insert("thorn", 'þ');
        m.insert("ssharp", 'ß');

        // Uppercase Latin-1
        m.insert("Agrave", 'À');
        m.insert("Aacute", 'Á');
        m.insert("Acircumflex", 'Â');
        m.insert("Atilde", 'Ã');
        m.insert("Adiaeresis", 'Ä');
        m.insert("Aring", 'Å');
        m.insert("AE", 'Æ');
        m.insert("Ccedilla", 'Ç');
        m.insert("Egrave", 'È');
        m.insert("Eacute", 'É');
        m.insert("Ecircumflex", 'Ê');
        m.insert("Ediaeresis", 'Ë');
        m.insert("Igrave", 'Ì');
        m.insert("Iacute", 'Í');
        m.insert("Icircumflex", 'Î');
        m.insert("Idiaeresis", 'Ï');
        m.insert("ETH", 'Ð');
        m.insert("Ntilde", 'Ñ');
        m.insert("Ograve", 'Ò');
        m.insert("Oacute", 'Ó');
        m.insert("Ocircumflex", 'Ô');
        m.insert("Otilde", 'Õ');
        m.insert("Odiaeresis", 'Ö');
        m.insert("Oslash", 'Ø');
        m.insert("Ugrave", 'Ù');
        m.insert("Uacute", 'Ú');
        m.insert("Ucircumflex", 'Û');
        m.insert("Udiaeresis", 'Ü');
        m.insert("Yacute", 'Ý');
        m.insert("THORN", 'Þ');

        // Latin Extended-A (Czech, Polish, etc.)
        m.insert("cacute", 'ć');
        m.insert("Cacute", 'Ć');
        m.insert("ccaron", 'č');
        m.insert("Ccaron", 'Č');
        m.insert("dcaron", 'ď');
        m.insert("Dcaron", 'Ď');
        m.insert("ecaron", 'ě');
        m.insert("Ecaron", 'Ě');
        m.insert("eogonek", 'ę');
        m.insert("Eogonek", 'Ę');
        m.insert("lcaron", 'ľ');
        m.insert("Lcaron", 'Ľ');
        m.insert("lstroke", 'ł');
        m.insert("Lstroke", 'Ł');
        m.insert("nacute", 'ń');
        m.insert("Nacute", 'Ń');
        m.insert("ncaron", 'ň');
        m.insert("Ncaron", 'Ň');
        m.insert("odoubleacute", 'ő');
        m.insert("Odoubleacute", 'Ő');
        m.insert("rcaron", 'ř');
        m.insert("Rcaron", 'Ř');
        m.insert("sacute", 'ś');
        m.insert("Sacute", 'Ś');
        m.insert("scaron", 'š');
        m.insert("Scaron", 'Š');
        m.insert("scedilla", 'ş');
        m.insert("Scedilla", 'Ş');
        m.insert("tcaron", 'ť');
        m.insert("Tcaron", 'Ť');
        m.insert("udoubleacute", 'ű');
        m.insert("Udoubleacute", 'Ű');
        m.insert("uring", 'ů');
        m.insert("Uring", 'Ů');
        m.insert("zacute", 'ź');
        m.insert("Zacute", 'Ź');
        m.insert("zcaron", 'ž');
        m.insert("Zcaron", 'Ž');
        m.insert("zdot", 'ż');
        m.insert("Zdot", 'Ż');
        m.insert("aogonek", 'ą');
        m.insert("Aogonek", 'Ą');

        // Nordic / Scandinavian
        m.insert("oe", 'œ');
        m.insert("OE", 'Œ');

        // Common symbols
        m.insert("multiply", '×');
        m.insert("division", '÷');
        m.insert("section", '§');
        m.insert("degree", '°');
        m.insert("mu", 'µ');
        m.insert("paragraph", '¶');
        m.insert("sterling", '£');
        m.insert("EuroSign", '€');
        m.insert("yen", '¥');
        m.insert("cent", '¢');
        m.insert("copyright", '©');
        m.insert("registered", '®');
        m.insert("notsign", '¬');
        m.insert("brokenbar", '¦');
        m.insert("guillemotleft", '«');
        m.insert("guillemotright", '»');

        // Dead keys: map to their base character for display
        m.insert("dead_acute", '´');
        m.insert("dead_grave", '`');
        m.insert("dead_circumflex", '^');
        m.insert("dead_tilde", '~');
        m.insert("dead_diaeresis", '¨');
        m.insert("dead_caron", 'ˇ');
        m.insert("dead_cedilla", '¸');
        m.insert("dead_ring_above", '°');

        m
    });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysym_single_letter() {
        assert_eq!(keysym_to_char("a"), Some('a'));
        assert_eq!(keysym_to_char("z"), Some('z'));
    }

    #[test]
    fn keysym_uppercase() {
        assert_eq!(keysym_to_char("A"), Some('A'));
        assert_eq!(keysym_to_char("Z"), Some('Z'));
    }

    #[test]
    fn keysym_digit() {
        assert_eq!(keysym_to_char("1"), Some('1'));
        assert_eq!(keysym_to_char("0"), Some('0'));
    }

    #[test]
    fn keysym_punctuation() {
        assert_eq!(keysym_to_char("semicolon"), Some(';'));
        assert_eq!(keysym_to_char("bracketleft"), Some('['));
        assert_eq!(keysym_to_char("slash"), Some('/'));
    }

    #[test]
    fn keysym_accented() {
        assert_eq!(keysym_to_char("eacute"), Some('é'));
        assert_eq!(keysym_to_char("ccaron"), Some('č'));
        assert_eq!(keysym_to_char("scaron"), Some('š'));
        assert_eq!(keysym_to_char("zcaron"), Some('ž'));
        assert_eq!(keysym_to_char("rcaron"), Some('ř'));
    }

    #[test]
    fn keysym_unicode_notation() {
        assert_eq!(keysym_to_char("U0041"), Some('A'));
        assert_eq!(keysym_to_char("U+0041"), Some('A'));
    }

    #[test]
    fn keysym_unknown() {
        assert_eq!(keysym_to_char("NoSymbol"), None);
        assert_eq!(keysym_to_char("nonexistent_keysym"), None);
    }

    #[test]
    fn parse_simple_symbols_block() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [ q, Q ] };
    key <AD02> { [ w, W ] };
    key <AC01> { [ a, A ] };
    key <AB01> { [ z, Z ] };
};
"#;
        let map = parse_symbols_block(content, "basic");
        assert_eq!(map.get("AD01"), Some(&"q".to_string()));
        assert_eq!(map.get("AD02"), Some(&"w".to_string()));
        assert_eq!(map.get("AC01"), Some(&"a".to_string()));
        assert_eq!(map.get("AB01"), Some(&"z".to_string()));
    }

    #[test]
    fn parse_default_block() {
        let content = r#"
default xkb_symbols "basic" {
    key <AD01> { [ q, Q ] };
};
"#;
        // Empty variant should match the first block
        let map = parse_symbols_block(content, "");
        assert_eq!(map.get("AD01"), Some(&"q".to_string()));
    }

    #[test]
    fn parse_variant_block() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [ q, Q ] };
};

xkb_symbols "qwerty" {
    key <AD01> { [ e, E ] };
};
"#;
        let map = parse_symbols_block(content, "qwerty");
        assert_eq!(map.get("AD01"), Some(&"e".to_string()));
    }

    #[test]
    fn parse_accented_keysyms() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [ eacute, Eacute ] };
    key <AD02> { [ ccaron, Ccaron ] };
};
"#;
        let map = parse_symbols_block(content, "basic");
        assert_eq!(map.get("AD01"), Some(&"eacute".to_string()));
        assert_eq!(map.get("AD02"), Some(&"ccaron".to_string()));
    }

    #[test]
    fn parse_multiline_key() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [  q,  Q,  at,  Greek_OMEGA ] };
};
"#;
        let map = parse_symbols_block(content, "basic");
        assert_eq!(map.get("AD01"), Some(&"q".to_string()));
    }

    #[test]
    fn parse_empty_variant_not_found() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [ q, Q ] };
};
"#;
        let map = parse_symbols_block(content, "nonexistent");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_include_ref_simple() {
        let (layout, variant) = parse_include_ref("us");
        assert_eq!(layout, "us");
        assert_eq!(variant, "");
    }

    #[test]
    fn parse_include_ref_with_variant() {
        let (layout, variant) = parse_include_ref("latin(type2)");
        assert_eq!(layout, "latin");
        assert_eq!(variant, "type2");
    }

    #[test]
    fn build_grid_from_map() {
        let mut key_map = HashMap::new();
        key_map.insert("AD01".to_string(), "q".to_string());
        key_map.insert("AD02".to_string(), "w".to_string());
        key_map.insert("AC01".to_string(), "a".to_string());
        key_map.insert("AB01".to_string(), "z".to_string());

        let grid = build_grid(&key_map);
        assert_eq!(grid.top_row[0], "q");
        assert_eq!(grid.top_row[1], "w");
        assert_eq!(grid.home_row[0], "a");
        assert_eq!(grid.bottom_row[0], "z");
    }

    #[test]
    fn build_grid_resolves_keysyms_to_chars() {
        let mut key_map = HashMap::new();
        key_map.insert("AD01".to_string(), "eacute".to_string());
        key_map.insert("AC01".to_string(), "ccaron".to_string());

        let grid = build_grid(&key_map);
        assert_eq!(grid.top_row[0], "é");
        assert_eq!(grid.home_row[0], "č");
    }

    #[test]
    fn build_grid_empty_for_missing_keys() {
        let key_map = HashMap::new();
        let grid = build_grid(&key_map);

        assert_eq!(grid.top_row.len(), 12);
        assert_eq!(grid.home_row.len(), 11);
        assert_eq!(grid.bottom_row.len(), 10);

        // All should be empty strings
        for key in &grid.top_row {
            assert_eq!(key, "");
        }
    }

    #[test]
    fn resolve_includes_with_depth_zero() {
        let content = r#"
xkb_symbols "basic" {
    include "latin(type2)"
    key <AD01> { [ q, Q ] };
};
"#;
        // depth=0 should return empty (no recursion allowed)
        let map = resolve_includes(content, "basic", 0, std::path::Path::new("/nonexistent"));
        assert!(map.is_empty());
    }

    #[test]
    fn find_matching_brace_simple() {
        let content = "{ inner }";
        // start after first brace (index 2)
        assert_eq!(find_matching_brace(content, 2), Some(8));
    }

    #[test]
    fn find_matching_brace_nested() {
        let content = "{ { inner } }";
        assert_eq!(find_matching_brace(content, 2), Some(12));
    }

    #[test]
    fn full_grid_from_inline_content() {
        let content = r#"
xkb_symbols "basic" {
    key <AD01> { [ q, Q ] };
    key <AD02> { [ w, W ] };
    key <AD03> { [ e, E ] };
    key <AD04> { [ r, R ] };
    key <AD05> { [ t, T ] };
    key <AD06> { [ y, Y ] };
    key <AD07> { [ u, U ] };
    key <AD08> { [ i, I ] };
    key <AD09> { [ o, O ] };
    key <AD10> { [ p, P ] };
    key <AD11> { [ bracketleft, braceleft ] };
    key <AD12> { [ bracketright, braceright ] };
    key <AC01> { [ a, A ] };
    key <AC02> { [ s, S ] };
    key <AC03> { [ d, D ] };
    key <AC04> { [ f, F ] };
    key <AC05> { [ g, G ] };
    key <AC06> { [ h, H ] };
    key <AC07> { [ j, J ] };
    key <AC08> { [ k, K ] };
    key <AC09> { [ l, L ] };
    key <AC10> { [ semicolon, colon ] };
    key <AC11> { [ apostrophe, quotedbl ] };
    key <AB01> { [ z, Z ] };
    key <AB02> { [ x, X ] };
    key <AB03> { [ c, C ] };
    key <AB04> { [ v, V ] };
    key <AB05> { [ b, B ] };
    key <AB06> { [ n, N ] };
    key <AB07> { [ m, M ] };
    key <AB08> { [ comma, less ] };
    key <AB09> { [ period, greater ] };
    key <AB10> { [ slash, question ] };
};
"#;

        let map = parse_symbols_block(content, "basic");
        let grid = build_grid(&map);

        assert_eq!(
            grid.top_row,
            vec!["q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]"]
        );
        assert_eq!(
            grid.home_row,
            vec!["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"]
        );
        assert_eq!(
            grid.bottom_row,
            vec!["z", "x", "c", "v", "b", "n", "m", ",", ".", "/"]
        );
    }

    #[test]
    fn czech_qwerty_layout() {
        let content = r#"
xkb_symbols "qwerty" {
    key <AD01> { [ q, Q, backslash ] };
    key <AD02> { [ w, W, bar ] };
    key <AD03> { [ eacute, Eacute ] };
    key <AD04> { [ rcaron, Rcaron ] };
    key <AD05> { [ tcaron, Tcaron ] };
    key <AD06> { [ zcaron, Zcaron ] };
    key <AD07> { [ uacute, Uacute ] };
    key <AD08> { [ iacute, Iacute ] };
    key <AD09> { [ oacute, Oacute ] };
    key <AD10> { [ p, P ] };
    key <AC01> { [ a, A ] };
    key <AC02> { [ scaron, Scaron ] };
    key <AC03> { [ dcaron, Dcaron ] };
    key <AC04> { [ f, F ] };
    key <AC05> { [ g, G ] };
    key <AC06> { [ h, H ] };
    key <AC07> { [ j, J ] };
    key <AC08> { [ k, K ] };
    key <AC09> { [ l, L ] };
    key <AC10> { [ uring, Uring ] };
    key <AB01> { [ yacute, Yacute ] };
    key <AB02> { [ x, X ] };
    key <AB03> { [ ccaron, Ccaron ] };
    key <AB04> { [ v, V ] };
    key <AB05> { [ b, B ] };
    key <AB06> { [ ncaron, Ncaron ] };
    key <AB07> { [ m, M ] };
};
"#;

        let map = parse_symbols_block(content, "qwerty");
        let grid = build_grid(&map);

        assert_eq!(grid.top_row[2], "é");  // eacute
        assert_eq!(grid.top_row[3], "ř");  // rcaron
        assert_eq!(grid.top_row[4], "ť");  // tcaron
        assert_eq!(grid.top_row[5], "ž");  // zcaron
        assert_eq!(grid.home_row[1], "š");  // scaron
        assert_eq!(grid.home_row[2], "ď");  // dcaron
        assert_eq!(grid.home_row[9], "ů");  // uring
        assert_eq!(grid.bottom_row[0], "ý"); // yacute
        assert_eq!(grid.bottom_row[2], "č"); // ccaron
        assert_eq!(grid.bottom_row[5], "ň"); // ncaron
    }
}
