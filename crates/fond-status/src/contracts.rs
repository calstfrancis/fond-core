//! Example replies for every helper command and the status file, as the apps print them and
//! Porog reads them. Plain JSON files rather than shared Rust types, so an app can move to a new
//! `fond-core` revision on its own schedule and a test on each side still catches a field that
//! changed shape. An app's own test parses what it prints and compares its fields with these; Porog
//! parses these and checks what it makes of them.

/// `sputnik-now` with an action, a later list and pieces.
pub const NOW_CARD: &str = include_str!("../contracts/now_card.json");
/// `sputnik-now` when Sputnik has nothing to show and says why.
pub const NOW_ERROR: &str = include_str!("../contracts/now_error.json");
/// An `<app>-search` reply: one hit by key, one with its own arguments.
pub const SEARCH_HITS: &str = include_str!("../contracts/search_hits.json");
/// An `<app>-whats-new` reply.
pub const WHATS_NEW: &str = include_str!("../contracts/whats_new.json");
/// A status file with a resume entry carrying a context line.
pub const STATUS: &str = include_str!("../contracts/status.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_is_json_with_schema_1() {
        for text in [NOW_CARD, NOW_ERROR, SEARCH_HITS, WHATS_NEW, STATUS] {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(value["schema"], 1, "{text}");
        }
    }

    #[test]
    fn the_status_and_whats_new_examples_are_the_types_these_crates_write() {
        let status: crate::Status = serde_json::from_str(STATUS).unwrap();
        assert_eq!(status.resume[0].context.as_deref().map(str::len) > Some(0), true);
        let news: crate::whats_new::WhatsNew = serde_json::from_str(WHATS_NEW).unwrap();
        assert_eq!(news.items.len(), 2);
    }
}
