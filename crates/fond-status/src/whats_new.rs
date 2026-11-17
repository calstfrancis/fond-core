//! What an app prints for `<app>-whats-new`, the reply Porog shows when the icon on an app's
//! row is pressed: the newest release's name and a headline with its detail for each change.
//!
//! Two ways to make one. An app with a hand-kept list builds it with [`WhatsNew::new`]; an app
//! whose release notes are its `CHANGELOG.md` parses the newest released entry with
//! [`from_changelog`], so the reply can never fall behind the release.

use serde::{Deserialize, Serialize};

/// The schema of the reply. Bumped only for a change an older Porog cannot ignore.
pub const SCHEMA: u32 = 1;

/// How many items a reply carries.
pub const MAX_ITEMS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub headline: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatsNew {
    pub schema: u32,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    pub items: Vec<Item>,
}

impl WhatsNew {
    pub fn new(version: &str, release_name: &str, items: &[(&str, &str)]) -> Self {
        let name = release_name.trim();
        WhatsNew {
            schema: SCHEMA,
            version: version.to_string(),
            release_name: (!name.is_empty()).then(|| name.to_string()),
            items: items
                .iter()
                .take(MAX_ITEMS)
                .map(|(headline, detail)| Item {
                    headline: headline.to_string(),
                    detail: detail.to_string(),
                })
                .collect(),
        }
    }

    /// The reply as one line of JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }
}

fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.trim().split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "")
}

/// A bullet's text as a headline and its detail: its bold lead if it has one
/// (`**Headline.** the rest`), otherwise its first sentence.
fn split_bullet(text: &str) -> Option<Item> {
    let (headline, detail) = match text.strip_prefix("**").and_then(|r| r.split_once("**")) {
        Some((lead, rest)) => (lead.to_string(), rest.to_string()),
        None => match text.split_once(". ") {
            Some((first, rest)) => (first.to_string(), rest.to_string()),
            None => (text.to_string(), String::new()),
        },
    };
    let headline = plain(&headline)
        .trim()
        .trim_end_matches(['.', ':'])
        .trim()
        .to_string();
    (!headline.is_empty()).then(|| Item {
        headline,
        detail: plain(&detail).split_whitespace().collect::<Vec<_>>().join(" "),
    })
}

/// The newest released entry of `changelog` (`## [0.28.0] "Name" — date — subtitle`), as a
/// reply. `[Unreleased]` and any heading that is not a plain version are skipped. `None` when
/// there is no released entry.
pub fn from_changelog(changelog: &str) -> Option<WhatsNew> {
    let mut lines = changelog.lines();
    let (version, name) = lines.by_ref().find_map(|line| {
        let rest = line.strip_prefix("## [")?;
        let (version, rest) = rest.split_once(']')?;
        is_version(version).then(|| {
            let name = rest.split('"').nth(1).unwrap_or("").to_string();
            (version.to_string(), name)
        })
    })?;
    let mut bullets: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("## ") {
            break;
        }
        if let Some(bullet) = line.strip_prefix("- ") {
            bullets.push(bullet.trim().to_string());
        } else if line.starts_with("  ") && !line.trim().is_empty() {
            if let Some(last) = bullets.last_mut() {
                last.push(' ');
                last.push_str(line.trim());
            }
        }
    }
    let items = bullets
        .iter()
        .filter_map(|b| split_bullet(b))
        .take(MAX_ITEMS)
        .collect();
    Some(WhatsNew {
        schema: SCHEMA,
        version,
        release_name: (!name.trim().is_empty()).then_some(name),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n## [Unreleased]\n\n- **Not yet.** unreleased\n\n\
## [0.4.0] \"Left Note\" — 2026-11-17 — Notes\n\n### Added\n\
- **A note for each app.** A pencil beside a row\n  keeps one line for you.\n\
- Plain bullet. With a second sentence.\n- `Code` only\n\n## [0.3.0] \"Ready Hand\" — 2026-11-15\n- **Old.** old\n";

    #[test]
    fn the_newest_released_entry_is_used_and_unreleased_is_skipped() {
        let w = from_changelog(LOG).unwrap();
        assert_eq!(w.version, "0.4.0");
        assert_eq!(w.release_name.as_deref(), Some("Left Note"));
        assert_eq!(w.items.len(), 3);
        assert_eq!(w.items[0].headline, "A note for each app");
        assert_eq!(w.items[0].detail, "A pencil beside a row keeps one line for you.");
        assert_eq!(w.items[1].headline, "Plain bullet");
        assert_eq!(w.items[1].detail, "With a second sentence.");
        assert_eq!(w.items[2].headline, "Code only");
    }

    #[test]
    fn no_released_entry_is_none() {
        assert_eq!(from_changelog("# Changelog\n\n## [Unreleased]\n- x\n"), None);
        assert_eq!(from_changelog(""), None);
    }

    #[test]
    fn a_hand_kept_list_makes_the_same_reply() {
        let w = WhatsNew::new("0.2.0", " ", &[("Headline", "Detail"), ("Alone", "")]);
        assert_eq!(w.release_name, None);
        assert_eq!(
            w.to_json(),
            r#"{"schema":1,"version":"0.2.0","items":[{"headline":"Headline","detail":"Detail"},{"headline":"Alone"}]}"#
        );
    }
}
