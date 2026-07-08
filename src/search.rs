//! Search matching: whitespace-split needles, ALL of which must match the
//! haystack. A needle matches either as a substring or as a fuzzy subsequence
//! for queries of at least 4 characters (`hcfg` matches
//! `herdr configurable picker`). Short queries stay substring-only to avoid
//! broad matches like `tmp` hitting most pane labels.

/// True when every whitespace-separated word of the query appears in the
/// text. Allocation-free for hot loops: both sides must already be
/// lowercase (`Row.search_text` is stored that way, and callers lowercase
/// the query once per pass). An empty query matches everything.
pub fn lowered_query_matches(lowered_text: &str, lowered_query: &str) -> bool {
    lowered_query.split_whitespace().all(|needle| {
        lowered_text.contains(needle)
            || fuzzy_allowed(needle) && fuzzy_subsequence(lowered_text, needle)
    })
}

fn fuzzy_allowed(needle: &str) -> bool {
    needle.chars().count() >= 4
}

fn fuzzy_subsequence(lowered_text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut chars = needle.chars();
    let Some(mut wanted) = chars.next() else {
        return true;
    };
    for ch in lowered_text.chars() {
        if ch == wanted {
            match chars.next() {
                Some(next) => wanted = next,
                None => return true,
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Case-insensitive convenience for the tests below.
    fn query_matches(text: &str, query: &str) -> bool {
        lowered_query_matches(&text.to_lowercase(), &query.to_lowercase())
    }

    #[test]
    fn matches_case_insensitive_substrings() {
        assert!(query_matches("picker", "ick"));
        assert!(query_matches("Picker", "pick"));
        assert!(query_matches("picker", "ICK"));
        assert!(!query_matches("picker", "pique"));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(query_matches("anything", ""));
        assert!(query_matches("", ""));
        assert!(query_matches("x", "   "));
    }

    #[test]
    fn multiple_words_all_must_match_in_any_order() {
        let text = "picker claude · working";
        assert!(query_matches(text, "pick work"));
        assert!(query_matches(text, "working claude"));
        assert!(!query_matches(text, "pick idle"));
    }

    #[test]
    fn fuzzy_matches_subsequences_like_fzf() {
        assert!(query_matches("herdr configurable picker", "hcfg"));
        assert!(query_matches("cargo test -p picker", "ctpp"));
        assert!(query_matches("feature/search-idx", "fsidx"));
        assert!(!query_matches("picker claude", "zxq"));
    }

    #[test]
    fn short_queries_do_not_fuzzy_match_everything() {
        assert!(query_matches("/var/tmp/project", "tmp"));
        assert!(!query_matches("test manual pane", "tmp"));
        assert!(!query_matches("terminal multiplexer pane", "tmp"));
    }

    #[test]
    fn fuzzy_still_requires_every_query_word() {
        let text = "feature/search-index cargo test -p picker";
        assert!(query_matches(text, "fsidx ctpp"));
        assert!(!query_matches(text, "fsidx zzq"));
    }

    #[test]
    fn works_on_non_ascii_labels() {
        assert!(query_matches("日本語ラベル claude", "ラベル claude"));
        assert!(!query_matches("日本語ラベル", "英語"));
    }
}
