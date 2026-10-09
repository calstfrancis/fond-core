//! Marks as other PDF readers keep them: read the highlights, underlines, strike-outs and
//! sticky notes a PDF carries (Acrobat, Zotero, Preview, Okular), and write a set of marks into
//! a copy of a PDF so they show in those readers. Colours and sticky-note positions come
//! across, which [`crate::annotation`] (the older, highlight-only pair) does not carry.

use pdfium_render::prelude::*;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Highlight,
    Underline,
    Strikeout,
    /// A sticky note: an icon at a point, with its text as the contents.
    Note,
    /// A rectangle (written as a square annotation), for a clipped figure.
    Area,
}

/// A mark read from a PDF.
#[derive(Debug, Clone)]
pub struct ReadMark {
    pub kind: MarkKind,
    /// 1-based page.
    pub page: u16,
    /// One quad per line (8 floats each, PDF user space); empty for a sticky note.
    pub quads: Vec<[f32; 8]>,
    pub contents: Option<String>,
    /// The marked text, from the page's text layer.
    pub snippet: Option<String>,
    pub color: Option<[u8; 3]>,
    /// Top-left of a sticky note's icon, PDF user space.
    pub position: Option<[f32; 2]>,
}

/// A mark to write into a PDF.
#[derive(Debug, Clone)]
pub struct WriteMark {
    pub kind: MarkKind,
    pub page: u16,
    pub quads: Vec<[f32; 8]>,
    /// `[left, bottom, right, top]` for an [`MarkKind::Area`].
    pub rect: Option<[f32; 4]>,
    /// Top-left of a [`MarkKind::Note`] icon.
    pub position: Option<[f32; 2]>,
    pub contents: Option<String>,
    pub color: [u8; 3],
}

const STICKY_SIDE: f32 = 18.0;

fn bounds_of(quads: &[[f32; 8]]) -> Option<[f32; 4]> {
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for q in quads {
        for (i, v) in q.iter().enumerate() {
            if i % 2 == 0 {
                b[0] = b[0].min(*v);
                b[2] = b[2].max(*v);
            } else {
                b[1] = b[1].min(*v);
                b[3] = b[3].max(*v);
            }
        }
    }
    (!quads.is_empty()).then_some(b)
}

fn rect_of(left: f32, bottom: f32, right: f32, top: f32) -> PdfRect {
    PdfRect::new(
        PdfPoints::new(bottom),
        PdfPoints::new(left),
        PdfPoints::new(top),
        PdfPoints::new(right),
    )
}

fn colour_of(annotation: &impl PdfPageAnnotationCommon) -> Option<[u8; 3]> {
    let c = annotation
        .stroke_color()
        .or_else(|_| annotation.fill_color())
        .ok()?;
    Some([c.red(), c.green(), c.blue()])
}

/// Read every highlight, underline, strike-out and sticky note of the PDF.
pub fn read_marks(pdfium: &Pdfium, bytes: &[u8]) -> Result<Vec<ReadMark>> {
    let document = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let mut out = Vec::new();
    for (index, page) in document.pages().iter().enumerate() {
        let text = page.text().ok();
        for annotation in page.annotations().iter() {
            let kind = match annotation.annotation_type() {
                PdfPageAnnotationType::Highlight => MarkKind::Highlight,
                PdfPageAnnotationType::Underline => MarkKind::Underline,
                PdfPageAnnotationType::Strikeout => MarkKind::Strikeout,
                PdfPageAnnotationType::Text => MarkKind::Note,
                _ => continue,
            };
            let contents = annotation.contents().filter(|s| !s.trim().is_empty());
            let color = colour_of(&annotation);
            if kind == MarkKind::Note {
                let Ok(b) = annotation.bounds() else { continue };
                if contents.is_none() {
                    continue;
                }
                out.push(ReadMark {
                    kind,
                    page: (index + 1) as u16,
                    quads: Vec::new(),
                    contents,
                    snippet: None,
                    color,
                    position: Some([b.left().value, b.top().value]),
                });
                continue;
            }
            let mut quads: Vec<[f32; 8]> = annotation
                .attachment_points()
                .iter()
                .map(|q| {
                    [
                        q.x1.value, q.y1.value, q.x2.value, q.y2.value, q.x3.value, q.y3.value,
                        q.x4.value, q.y4.value,
                    ]
                })
                .collect();
            if quads.is_empty() {
                if let Ok(r) = annotation.bounds() {
                    let (l, rt, t, b) = (
                        r.left().value,
                        r.right().value,
                        r.top().value,
                        r.bottom().value,
                    );
                    quads.push([l, t, rt, t, l, b, rt, b]);
                }
            }
            let snippet = text
                .as_ref()
                .and_then(|t| t.for_annotation(&annotation).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            out.push(ReadMark {
                kind,
                page: (index + 1) as u16,
                quads,
                contents,
                snippet,
                color,
                position: None,
            });
        }
    }
    Ok(out)
}

/// Write `marks` into the PDF and return the new bytes. The input is left alone.
pub fn write_marks(pdfium: &Pdfium, bytes: &[u8], marks: &[WriteMark]) -> Result<Vec<u8>> {
    let document = pdfium.load_pdf_from_byte_slice(bytes, None)?;
    let pages = document.pages().len();
    for mark in marks {
        let index = mark.page.saturating_sub(1);
        if index >= pages {
            continue;
        }
        let mut page = document.pages().get(index)?;
        let colour = PdfColor::new(mark.color[0], mark.color[1], mark.color[2], 255);
        match mark.kind {
            MarkKind::Highlight | MarkKind::Underline | MarkKind::Strikeout => {
                let Some(b) = bounds_of(&mark.quads) else {
                    continue;
                };
                macro_rules! markup {
                    ($a:expr) => {{
                        let mut a = $a;
                        a.set_bounds(rect_of(b[0], b[1], b[2], b[3]))?;
                        for q in &mark.quads {
                            a.attachment_points_mut().create_attachment_point_at_end(
                                PdfQuadPoints::new_from_values(
                                    q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7],
                                ),
                            )?;
                        }
                        a.set_stroke_color(colour)?;
                        if let Some(c) = &mark.contents {
                            a.set_contents(c)?;
                        }
                    }};
                }
                match mark.kind {
                    MarkKind::Highlight => {
                        markup!(page.annotations_mut().create_highlight_annotation()?)
                    }
                    MarkKind::Underline => {
                        markup!(page.annotations_mut().create_underline_annotation()?)
                    }
                    _ => markup!(page.annotations_mut().create_strikeout_annotation()?),
                }
            }
            MarkKind::Note => {
                let Some([x, y]) = mark.position else {
                    continue;
                };
                let mut a = page
                    .annotations_mut()
                    .create_text_annotation(mark.contents.as_deref().unwrap_or(""))?;
                a.set_bounds(rect_of(x, y - STICKY_SIDE, x + STICKY_SIDE, y))?;
                a.set_stroke_color(colour)?;
            }
            MarkKind::Area => {
                let Some([l, b, r, t]) = mark.rect else {
                    continue;
                };
                let mut a = page.annotations_mut().create_square_annotation()?;
                a.set_bounds(rect_of(l, b, r, t))?;
                a.set_stroke_color(colour)?;
                if let Some(c) = &mark.contents {
                    a.set_contents(c)?;
                }
            }
        }
    }
    Ok(document.save_to_bytes()?)
}
