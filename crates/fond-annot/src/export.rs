//! Notes export: Markdown, Typst and LaTeX, optionally grouped by what each highlight colour
//! means and cited against a bibliography key. Pure string rendering — the dialog that asks
//! how to export lives in the app.

use crate::annotation::{AnnotationKind, AnnotationSidecar};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Typst,
    Latex,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Typst, Format::Markdown, Format::Latex];

    pub fn label(self) -> &'static str {
        match self {
            Format::Markdown => "Markdown (Pandoc citations)",
            Format::Typst => "Typst",
            Format::Latex => "LaTeX (biblatex)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Markdown => "md",
            Format::Typst => "typ",
            Format::Latex => "tex",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    /// Printed page ("42", "xii") for a PDF, or a chapter ("3") for an EPUB.
    pub locator: String,
    pub is_chapter: bool,
    pub kind: AnnotationKind,
    pub color: Option<String>,
    pub quote: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourGroup {
    pub hex: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub format: Format,
    /// Group items under these colour headings, in this order, with anything else under
    /// "Other". Empty means document order with no grouping.
    pub colour_groups: Vec<ColourGroup>,
    /// Bibliography key to cite with; without one, the locator is shown as plain text.
    pub cite_key: Option<String>,
}

impl Item {
    fn locator_text(&self) -> String {
        if self.is_chapter {
            format!("ch. {}", self.locator)
        } else {
            format!("p. {}", self.locator)
        }
    }

    fn kind_tag(&self) -> Option<&'static str> {
        match self.kind {
            AnnotationKind::Underline => Some("underlined"),
            AnnotationKind::Strikeout => Some("struck out"),
            AnnotationKind::Note => Some("note"),
            AnnotationKind::Highlight | AnnotationKind::Unknown => None,
        }
    }
}

/// `(heading, items)` groups in `groups` order; ungrouped output is one headingless group.
fn groups<'a>(items: &'a [Item], groups: &[ColourGroup]) -> Vec<(Option<String>, Vec<&'a Item>)> {
    if groups.is_empty() {
        return vec![(None, items.iter().collect())];
    }
    let in_colour = |it: &Item, hex: &str| {
        it.color
            .as_deref()
            .is_some_and(|c| c.eq_ignore_ascii_case(hex))
    };
    let mut out: Vec<(Option<String>, Vec<&Item>)> = Vec::new();
    for g in groups {
        let hits: Vec<&Item> = items.iter().filter(|it| in_colour(it, &g.hex)).collect();
        if !hits.is_empty() {
            out.push((Some(g.label.clone()), hits));
        }
    }
    let rest: Vec<&Item> = items
        .iter()
        .filter(|it| !groups.iter().any(|g| in_colour(it, &g.hex)))
        .collect();
    if !rest.is_empty() {
        out.push((Some("Other".to_string()), rest));
    }
    out
}

pub fn render(title: &str, items: &[Item], bookmarks: &[String], opts: &Options) -> String {
    match opts.format {
        Format::Markdown => markdown(title, items, bookmarks, opts),
        Format::Typst => typst(title, items, bookmarks, opts),
        Format::Latex => latex(title, items, bookmarks, opts),
    }
}

// ---------------------------------------------------------------- Markdown

fn markdown(title: &str, items: &[Item], bookmarks: &[String], opts: &Options) -> String {
    let mut out = format!("# {title}\n\n");
    if !bookmarks.is_empty() {
        out.push_str("## Bookmarks\n\n");
        for b in bookmarks {
            out.push_str(&format!("- {b}\n"));
        }
        out.push('\n');
    }
    for (heading, group) in groups(items, &opts.colour_groups) {
        match heading {
            Some(h) => out.push_str(&format!("## {h}\n\n")),
            None if !items.is_empty() => out.push_str("## Notes & highlights\n\n"),
            None => {}
        }
        for it in group {
            if let Some(q) = it.quote.as_deref().filter(|q| !q.trim().is_empty()) {
                for line in q.lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                let cite = match &opts.cite_key {
                    Some(k) => format!("[@{k}, {}]", it.locator_text()),
                    None => format!("({})", it.locator_text()),
                };
                out.push_str(&format!(">\n> — {cite}"));
                if let Some(tag) = it.kind_tag() {
                    out.push_str(&format!(" *({tag})*"));
                }
                out.push_str("\n\n");
            } else {
                out.push_str(&format!("**{}**\n\n", it.locator_text()));
            }
            if let Some(n) = it.note.as_deref().filter(|n| !n.trim().is_empty()) {
                out.push_str(&format!("{n}\n\n"));
            }
        }
    }
    out
}

// ------------------------------------------------------------------ Typst

/// Escape text for Typst *markup* mode so it renders literally.
pub fn typst_escape(s: &str) -> String {
    s.split('\n')
        .map(typst_escape_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn typst_escape_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 4);
    let lead = chars.iter().take_while(|c| c.is_whitespace()).count();
    out.extend(&chars[..lead]);
    let mut i = lead;
    // A line-leading `=`, `+`, `-` or `1.` would otherwise start a heading or list.
    match chars.get(i) {
        Some(&c @ ('=' | '+' | '-')) => {
            out.push('\\');
            out.push(c);
            i += 1;
        }
        Some(c) if c.is_ascii_digit() => {
            let digits = chars[i..].iter().take_while(|d| d.is_ascii_digit()).count();
            if chars.get(i + digits) == Some(&'.') {
                out.extend(&chars[i..i + digits]);
                out.push_str("\\.");
                i += digits + 1;
            }
        }
        _ => {}
    }
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' | '#' | '$' | '*' | '_' | '`' | '~' | '@' | '<' | '>' | '[' | ']' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '/' if matches!(chars.get(i + 1), Some('/') | Some('*')) => out.push_str("\\/"),
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

fn typst_paragraphs(s: &str) -> String {
    s.split("\n\n")
        .map(|p| typst_escape(p.trim()))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn typst_attribution(it: &Item, cite_key: &Option<String>) -> String {
    match cite_key {
        Some(k) => crate::cite::typst_citation(k, Some(&it.locator_text())),
        None => typst_escape(&it.locator_text()),
    }
}

fn typst(title: &str, items: &[Item], bookmarks: &[String], opts: &Options) -> String {
    let mut out = format!("= {}\n\n", typst_escape(title));
    if !bookmarks.is_empty() {
        out.push_str("== Bookmarks\n\n");
        for b in bookmarks {
            out.push_str(&format!("- {}\n", typst_escape(b)));
        }
        out.push('\n');
    }
    for (heading, group) in groups(items, &opts.colour_groups) {
        match heading {
            Some(h) => out.push_str(&format!("== {}\n\n", typst_escape(&h))),
            None if !items.is_empty() => out.push_str("== Notes and highlights\n\n"),
            None => {}
        }
        for it in group {
            let attribution = typst_attribution(it, &opts.cite_key);
            let tag = it
                .kind_tag()
                .map(|t| format!(" _({t})_"))
                .unwrap_or_default();
            if let Some(q) = it.quote.as_deref().filter(|q| !q.trim().is_empty()) {
                out.push_str(&format!(
                    "#quote(block: true, attribution: [{attribution}{tag}])[{}]\n\n",
                    typst_paragraphs(q)
                ));
            } else {
                out.push_str(&format!("*{}*{tag}\n\n", typst_escape(&it.locator_text())));
            }
            if let Some(n) = it.note.as_deref().filter(|n| !n.trim().is_empty()) {
                out.push_str(&format!("{}\n\n", typst_paragraphs(n)));
            }
        }
    }
    out
}

// ------------------------------------------------------------------ LaTeX

pub fn latex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' | '}' | '$' | '&' | '#' | '_' | '%' => {
                out.push('\\');
                out.push(c);
            }
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            _ => out.push(c),
        }
    }
    out
}

fn latex(title: &str, items: &[Item], bookmarks: &[String], opts: &Options) -> String {
    let mut out = String::new();
    out.push_str("\\section*{");
    out.push_str(&latex_escape(title));
    out.push_str("}\n\n");
    if !bookmarks.is_empty() {
        out.push_str("\\subsection*{Bookmarks}\n\\begin{itemize}\n");
        for b in bookmarks {
            out.push_str(&format!("  \\item {}\n", latex_escape(b)));
        }
        out.push_str("\\end{itemize}\n\n");
    }
    for (heading, group) in groups(items, &opts.colour_groups) {
        match heading {
            Some(h) => out.push_str(&format!("\\subsection*{{{}}}\n\n", latex_escape(&h))),
            None if !items.is_empty() => out.push_str("\\subsection*{Notes and highlights}\n\n"),
            None => {}
        }
        for it in group {
            let cite = match &opts.cite_key {
                Some(k) => {
                    let sup = if it.is_chapter { "ch.~" } else { "p.~" };
                    format!("\\autocite[{sup}{}]{{{k}}}", latex_escape(&it.locator))
                }
                None => format!("({})", latex_escape(&it.locator_text())),
            };
            let tag = it
                .kind_tag()
                .map(|t| format!(" \\emph{{({t})}}"))
                .unwrap_or_default();
            if let Some(q) = it.quote.as_deref().filter(|q| !q.trim().is_empty()) {
                out.push_str(&format!(
                    "\\begin{{quote}}\n{}\n{cite}{tag}\n\\end{{quote}}\n\n",
                    latex_escape(q.trim())
                ));
            } else {
                out.push_str(&format!(
                    "\\textbf{{{}}}{tag}\n\n",
                    latex_escape(&it.locator_text())
                ));
            }
            if let Some(n) = it.note.as_deref().filter(|n| !n.trim().is_empty()) {
                out.push_str(&format!("{}\n\n", latex_escape(n.trim())));
            }
        }
    }
    out
}

/// Annotations of a sidecar as export items, in reading order. PDF annotations are located by
/// printed page label (falling back to the file page); EPUB ones by `chapter_number` (spine
/// position), falling back to the chapter's file name.
pub fn items_from_sidecar(
    sidecar: &AnnotationSidecar,
    page_labels: &[Option<String>],
    chapter_number: &dyn Fn(&str) -> Option<usize>,
) -> Vec<Item> {
    type SortKey = (u8, usize, Option<String>);
    let mut keyed: Vec<(SortKey, Item)> = sidecar
        .annotations
        .iter()
        .map(|a| {
            let (order, locator, is_chapter) = match (a.page, a.chapter.as_deref()) {
                (Some(p), _) => {
                    let label = page_labels
                        .get((p as usize).saturating_sub(1))
                        .and_then(|l| l.clone())
                        .unwrap_or_else(|| p.to_string());
                    ((0u8, p as usize), label, false)
                }
                (None, Some(c)) => {
                    let n = chapter_number(c);
                    let label = n.map(|n| n.to_string()).unwrap_or_else(|| {
                        std::path::Path::new(c)
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_else(|| c.to_string())
                    });
                    ((1u8, n.unwrap_or(usize::MAX)), label, true)
                }
                (None, None) => ((2u8, 0), String::from("?"), false),
            };
            (
                (order.0, order.1, a.created.clone()),
                Item {
                    locator,
                    is_chapter,
                    kind: a.kind,
                    color: a.color.clone(),
                    quote: a.snippet.clone(),
                    note: a.note.clone(),
                },
            )
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, i)| i).collect()
}

/// A quotation plus its citation, ready to paste into a document in `format`.
pub fn cite_snippet(
    format: Format,
    quote: &str,
    locator: &str,
    is_chapter: bool,
    cite_key: Option<&str>,
) -> String {
    let item = Item {
        locator: locator.to_string(),
        is_chapter,
        kind: AnnotationKind::Highlight,
        color: None,
        quote: None,
        note: None,
    };
    let quote = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    match (format, cite_key) {
        (Format::Typst, Some(k)) => format!(
            "#quote(block: true, attribution: [{}])[{}]",
            crate::cite::typst_citation(k, Some(&item.locator_text())),
            typst_escape(&quote)
        ),
        (Format::Typst, None) => format!(
            "#quote(block: true, attribution: [{}])[{}]",
            typst_escape(&item.locator_text()),
            typst_escape(&quote)
        ),
        (Format::Latex, Some(k)) => format!(
            "\\enquote{{{}}}\\autocite[{}{}]{{{k}}}",
            latex_escape(&quote),
            if is_chapter { "ch.~" } else { "p.~" },
            latex_escape(locator)
        ),
        (Format::Latex, None) => format!(
            "\\enquote{{{}}} ({})",
            latex_escape(&quote),
            latex_escape(&item.locator_text())
        ),
        (Format::Markdown, Some(k)) => {
            format!("\"{quote}\" [@{k}, {}]", item.locator_text())
        }
        (Format::Markdown, None) => format!("\"{quote}\" ({})", item.locator_text()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(page: &str, color: &str, quote: &str, note: Option<&str>) -> Item {
        Item {
            locator: page.into(),
            is_chapter: false,
            kind: AnnotationKind::Highlight,
            color: Some(color.into()),
            quote: Some(quote.into()),
            note: note.map(str::to_string),
        }
    }

    fn opts(format: Format, group: bool, key: Option<&str>) -> Options {
        let g = |hex: &str, label: &str| ColourGroup {
            hex: hex.into(),
            label: label.into(),
        };
        Options {
            format,
            colour_groups: if group {
                vec![
                    g("#D6B86A", "Key idea / thesis"),
                    g("#91A9B8", "Evidence / support"),
                    g("#9CAF88", "Connection / implication"),
                    g("#C98F78", "Problem / question"),
                ]
            } else {
                Vec::new()
            },
            cite_key: key.map(str::to_string),
        }
    }

    #[test]
    fn typst_escapes_markup() {
        assert_eq!(typst_escape("a #b $c [d] @e"), "a \\#b \\$c \\[d\\] \\@e");
        assert_eq!(typst_escape("= not a heading"), "\\= not a heading");
        assert_eq!(typst_escape("3. not a list"), "3\\. not a list");
        assert_eq!(typst_escape("see http://x"), "see http:\\//x");
        assert_eq!(typst_escape("line\n- item"), "line\n\\- item");
    }

    #[test]
    fn typst_cites_with_supplement() {
        let it = item(
            "42",
            "#D6B86A",
            "Being is becoming.",
            Some("Whitehead's claim"),
        );
        let out = render(
            "Process",
            &[it],
            &[],
            &opts(Format::Typst, false, Some("whitehead1929")),
        );
        assert!(out.contains("attribution: [@whitehead1929[p. 42]]"));
        assert!(out.contains("Whitehead's claim"));
    }

    #[test]
    fn grouping_follows_palette_order_and_uses_labels() {
        let a = item("1", "#C98F78", "problem text", None);
        let b = item("2", "#D6B86A", "idea text", None);
        let c = item("3", "#123456", "other text", None);
        let out = render("T", &[a, b, c], &[], &opts(Format::Markdown, true, None));
        let idea = out.find("Key idea").unwrap();
        let problem = out.find("Problem").unwrap();
        let other = out.find("## Other").unwrap();
        assert!(idea < problem && problem < other);
    }

    #[test]
    fn latex_escapes_and_cites() {
        let it = item("7", "#D6B86A", "50% of $x_i$ & more", None);
        let out = render("T_1", &[it], &[], &opts(Format::Latex, false, Some("k")));
        assert!(out.contains("50\\% of \\$x\\_i\\$ \\& more"));
        assert!(out.contains("\\autocite[p.~7]{k}"));
        assert!(out.contains("\\section*{T\\_1}"));
    }

    #[test]
    fn cite_snippets() {
        let t = cite_snippet(Format::Typst, "a  b\nc #", "42", false, Some("k"));
        assert_eq!(
            t,
            "#quote(block: true, attribution: [@k[p. 42]])[a b c \\#]"
        );
        let l = cite_snippet(Format::Latex, "x", "3", true, Some("k"));
        assert_eq!(l, "\\enquote{x}\\autocite[ch.~3]{k}");
        assert_eq!(
            cite_snippet(Format::Markdown, "x", "7", false, None),
            "\"x\" (p. 7)"
        );
    }

    #[test]
    fn markdown_uses_pandoc_citation() {
        let it = item("5", "#D6B86A", "q", None);
        let out = render(
            "T",
            &[it],
            &[],
            &opts(Format::Markdown, false, Some("smith2020")),
        );
        assert!(out.contains("[@smith2020, p. 5]"));
    }

    #[test]
    fn dump_samples_for_external_compilers() {
        let Ok(dir) = std::env::var("EXPORT_DUMP_DIR") else {
            return;
        };
        let nasty = "Line one with #hash, $money$, *stars*, _under_, [brackets], @at, `tick`, a // slash, <angle>.\n= fake heading\n- fake item\n3. fake numbered\nAnd \"quotes\" -- dashes... ~tilde";
        let items = vec![
            item(
                "12",
                "#D6B86A",
                nasty,
                Some("My note: 50% of it & more.\n\nSecond paragraph #1"),
            ),
            item("3", "#C98F78", "Plain problem.", None),
            Item {
                kind: AnnotationKind::Underline,
                ..item("xii", "#91A9B8", "evidence", None)
            },
            Item {
                is_chapter: true,
                ..item("4", "#9CAF88", "connection", Some("see ch"))
            },
        ];
        for (fmt, ext) in [
            (Format::Typst, "typ"),
            (Format::Latex, "tex"),
            (Format::Markdown, "md"),
        ] {
            for (name, key) in [("cited", Some("whitehead1929")), ("plain", None)] {
                let out = render(
                    "Process & Reality: A_Study #1",
                    &items,
                    &["p. 3".into()],
                    &opts(fmt, true, key),
                );
                std::fs::write(format!("{dir}/{name}.{ext}"), out).unwrap();
            }
        }
    }
}
