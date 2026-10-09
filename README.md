# fond-core

Shared, UI-agnostic primitives for the Fond suite (Kartoteka, Zerkalo, Skrizhal, Gost) and
Pereplyot. MIT licensed.

| Crate | What it is |
|---|---|
| `fond-doc` | PDF and EPUB access: rendering, text extraction, search, outline, page labels, text selection, annotation import/export. Built on PDFium. |
| `fond-annot` | The annotation sidecar format (`annots/<key>.json`), reading `Progress`, `PageLabelOverride`, and Typst citation text, and notes export (Typst, LaTeX, Markdown, grouped by highlight colour). No PDFium, no GTK, no bibliography engine. |

These used to live inside Kartoteka (`fond-doc`, and the annotation types inside `fond-bib`).
They were moved here so the reader app and the suite depend on one small, separately
versioned source of truth instead of pinning a whole Kartoteka release.

Kartoteka's `fond-bib` re-exports the `fond-annot` types at their old paths
(`fond_bib::annotation`, `fond_bib::Progress`, …), so existing code is unchanged.

## Annotation format compatibility

The sidecar is read and rewritten by several apps that are not all updated at once, so a file
written by a newer app must survive being opened and saved by an older one:

- `AnnotationKind` is `#[non_exhaustive]`; a kind this version doesn't know reads as
  `Unknown`, and its original name is written back unchanged.
- Fields this version doesn't know are collected in `Annotation::extra` /
  `AnnotationSidecar::extra` and written back untouched.
- New fields must be additive and optional. Bump `schema` only for a change old readers cannot
  safely ignore, and make sure every consumer (Kartoteka, Sputnik, Pereplyot) is on a
  `fond-annot` that has this tolerance **before** any app starts writing the new field.
