//! Reading position and printed-page numbering, stored in a note's frontmatter by Kartoteka
//! and Sputnik and in a plain JSON file by Pereplyot.

use serde::{Deserialize, Serialize};

/// Coarse reading position (§8). Pairs with `read-status: reading`.
///
/// For a PDF, `page`/`of` are the whole story. For an EPUB — which has no fixed page
/// grid — `page`/`of` hold the 1-based chapter index and chapter count instead, and
/// `chapter_percent` (0-100) adds the scroll position within that chapter, so the reader
/// can resume at "12% through Chapter 4" rather than just "Chapter 4". `None` for a PDF,
/// where `page` alone is already precise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Progress {
    pub page: u32,
    pub of: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_percent: Option<u8>,
}

/// Manual override for printed page numbering, for a PDF with no `/PageLabels` dictionary of
/// its own — common, since most scanned or older PDFs declare none, unlike what
/// `fond_doc::pdf::page_labels` reads natively from a PDF that does. Anchors one raw 1-based
/// file page to the printed number that appears on it; every later page counts up from there,
/// and every earlier page (covers, front matter) is left unlabeled, the same as a PDF-native
/// `None` label would be. Set from the reader's "Set page numbering…" action; ignored when the
/// PDF already declares its own labels, since those are authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PageLabelOverride {
    pub start_page: u32,
    pub start_label: i64,
}

impl PageLabelOverride {
    /// Apply this override across `count` raw pages, producing one label per 0-based page
    /// index (the same indexing `fond_doc::pdf::page_labels` uses). Pages before
    /// `start_page` come back `None`; `start_page` and later count up from `start_label`.
    pub fn apply(&self, count: u16) -> Vec<Option<String>> {
        (0..count)
            .map(|i| {
                let raw = i as u32 + 1;
                (raw >= self.start_page)
                    .then(|| (self.start_label + (raw - self.start_page) as i64).to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_label_override_applies_from_start_page() {
        let ov = PageLabelOverride {
            start_page: 3,
            start_label: 1,
        };
        let labels = ov.apply(5);
        assert_eq!(
            labels,
            vec![
                None,
                None,
                Some("1".into()),
                Some("2".into()),
                Some("3".into())
            ]
        );
    }

    #[test]
    fn page_label_override_can_start_above_one() {
        let ov = PageLabelOverride {
            start_page: 1,
            start_label: 2,
        };
        assert_eq!(
            ov.apply(3),
            vec![Some("2".into()), Some("3".into()), Some("4".into())]
        );
    }

    #[test]
    fn progress_serialises_kebab_case_without_empty_percent() {
        let json = serde_json::to_string(&Progress {
            page: 112,
            of: 420,
            chapter_percent: None,
        })
        .unwrap();
        assert_eq!(json, r#"{"page":112,"of":420}"#);
    }
}
