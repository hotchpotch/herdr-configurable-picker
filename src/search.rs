//! Search matching: case-insensitive substring over node labels.

/// True when `label` contains `query` ignoring case. An empty query
/// matches everything.
pub fn label_matches(label: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    label.to_lowercase().contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_case_insensitive_substrings() {
        assert!(label_matches("mothership", "ship"));
        assert!(label_matches("Mothership", "mother"));
        assert!(label_matches("mothership", "SHIP"));
        assert!(!label_matches("mothership", "shop"));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(label_matches("anything", ""));
        assert!(label_matches("", ""));
    }

    #[test]
    fn works_on_non_ascii_labels() {
        assert!(label_matches("日本語ラベル", "ラベル"));
        assert!(!label_matches("日本語ラベル", "英語"));
    }
}
