/// Result of normalizing a string for search: normalized chars + mapping to original indices.
pub struct Normalized {
    pub chars: Vec<char>,
    /// `chars[i]` came from the original char at index `char_map[i]`.
    pub char_map: Vec<usize>,
}

/// Normalize a string for accent-insensitive fuzzy search.
///
/// Lowercases, NFD-decomposes, and strips combining diacritical marks.
/// `"Počasí"` → `['p','o','c','a','s','i']` with char_map pointing back to
/// original char indices for correct highlight mapping.
pub fn normalize_for_search(s: &str) -> Normalized {
    use unicode_normalization::char::decompose_canonical;

    let mut chars = Vec::new();
    let mut char_map = Vec::new();

    for (i, ch) in s.chars().enumerate() {
        decompose_canonical(ch, |decomposed| {
            let code = decomposed as u32;
            // Skip combining diacritical marks (U+0300..U+036F)
            if !(0x0300..=0x036F).contains(&code) {
                for lower in decomposed.to_lowercase() {
                    chars.push(lower);
                    char_map.push(i);
                }
            }
        });
    }

    Normalized { chars, char_map }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_unchanged() {
        let n = normalize_for_search("Firefox");
        assert_eq!(n.chars, vec!['f', 'i', 'r', 'e', 'f', 'o', 'x']);
        assert_eq!(n.char_map, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn czech_accents_stripped() {
        let n = normalize_for_search("Počasí");
        assert_eq!(n.chars, vec!['p', 'o', 'c', 'a', 's', 'i']);
        assert_eq!(n.char_map, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn french_accents_stripped() {
        let n = normalize_for_search("Café");
        assert_eq!(n.chars, vec!['c', 'a', 'f', 'e']);
        assert_eq!(n.char_map, vec![0, 1, 2, 3]);
    }

    #[test]
    fn german_umlaut() {
        let n = normalize_for_search("München");
        assert_eq!(n.chars, vec!['m', 'u', 'n', 'c', 'h', 'e', 'n']);
    }

    #[test]
    fn empty_string() {
        let n = normalize_for_search("");
        assert!(n.chars.is_empty());
        assert!(n.char_map.is_empty());
    }

    #[test]
    fn mixed_ascii_and_accented() {
        let n = normalize_for_search("naïve");
        assert_eq!(n.chars, vec!['n', 'a', 'i', 'v', 'e']);
    }
}
