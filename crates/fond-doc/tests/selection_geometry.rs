//! Selection and text extraction on pages whose geometry isn't the simple upright, origin-at-
//! (0,0) case: an offset crop box and a `/Rotate 90` page whose text reads upright once
//! rotated (a landscape scan stored on a portrait sheet). Also the punctuation a horizontal
//! drag must not lose. Requires PDFium (skips otherwise).

use fond_doc::{bind_pdfium, extract_text, select_text_in_rect, select_text_range};

static PDFIUM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const LINES: [&str; 4] = [
    "Page 1",
    "The quick brown fox jumps over the lazy dog.",
    "Pack my box with five dozen liquor jugs.",
    "Sphinx of black quartz, judge my vow.",
];

fn pdf(page_keys: &str, text_start: &str) -> Vec<u8> {
    let mut ops = format!("BT /F1 14 Tf {text_start} 20 TL ");
    for (i, line) in LINES.iter().enumerate() {
        if i == 0 {
            ops.push_str(&format!("({line}) Tj "));
        } else {
            ops.push_str(&format!("T* ({line}) Tj "));
        }
    }
    ops.push_str("ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] {page_keys} /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
        ),
        format!("<< /Length {} >>\nstream\n{}\nendstream", ops.len(), ops),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, obj));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for off in &offsets {
        pdf.push_str(&format!("{off:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
        objects.len() + 1,
        xref
    ));
    pdf.into_bytes()
}

fn plain() -> Vec<u8> {
    pdf("", "1 0 0 1 72 700 Tm")
}

fn cropped() -> Vec<u8> {
    pdf("/CropBox [50 60 562 732]", "1 0 0 1 72 700 Tm")
}

/// Text drawn 90° anticlockwise on the page so it reads upright after `/Rotate 90`.
fn rotated() -> Vec<u8> {
    pdf("/Rotate 90", "0 1 -1 0 72 72 Tm")
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

const PACK: &str = "Packmyboxwithfivedozenliquorjugs.";

macro_rules! pdfium_or_skip {
    () => {
        match bind_pdfium() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("SKIP: PDFium not available ({e})");
                return;
            }
        }
    };
}

#[test]
fn extract_text_reads_every_line_of_cropped_and_rotated_pages() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    for (name, bytes) in [
        ("plain", plain()),
        ("cropped", cropped()),
        ("rotated", rotated()),
    ] {
        let text = extract_text(pdfium, &bytes).unwrap().full_text();
        for line in LINES {
            assert!(text.contains(line), "{name}: missing {line:?} in {text:?}");
        }
    }
}

#[test]
fn a_horizontal_drag_through_a_line_keeps_its_punctuation() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    // The "Pack" line's baseline is y=660; a pointer at mid x-height (y=665) never touches the
    // full stop's ink, which sits on the baseline.
    for (name, bytes) in [("plain", plain()), ("cropped", cropped())] {
        let sel = select_text_range(pdfium, &bytes, 0, 0.0, 665.0, 612.0, 665.0)
            .unwrap()
            .unwrap();
        assert_eq!(sel.quads.len(), 1, "{name}: {sel:?}");
        assert_eq!(squash(&sel.text), PACK, "{name}");
    }
}

#[test]
fn a_rect_selection_keeps_punctuation_too() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    let sel = select_text_in_rect(pdfium, &plain(), 0, 0.0, 664.0, 612.0, 666.0)
        .unwrap()
        .unwrap();
    assert_eq!(squash(&sel.text), PACK);
}

#[test]
fn an_adjacent_line_does_not_leak_into_the_selection() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    let sel = select_text_range(pdfium, &plain(), 0, 0.0, 665.0, 612.0, 665.0)
        .unwrap()
        .unwrap();
    assert!(!sel.text.contains("fox") && !sel.text.contains("Sphinx"));
}

#[test]
fn a_rotated_page_selects_the_line_the_reader_sees() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    let bytes = rotated();
    // Displayed line "Pack…" sits 504pt up the displayed page; a drag along it from the
    // displayed left edge to x=700 is, in user space, a vertical run at x = 612 - 504.
    let sel = select_text_range(pdfium, &bytes, 0, 108.0, 0.0, 108.0, 700.0)
        .unwrap()
        .unwrap();
    assert_eq!(sel.quads.len(), 1, "one displayed line only: {sel:?}");
    assert_eq!(squash(&sel.text), PACK);
    let q = sel.quads[0];
    let (x0, x1) = (q[0].min(q[2]), q[0].max(q[2]));
    let (y0, y1) = (q[1].min(q[5]), q[1].max(q[5]));
    assert!(
        (y1 - y0) > (x1 - x0) * 5.0,
        "a displayed line is a tall strip in user space: {q:?}"
    );
    assert!(x0 > 95.0 && x1 < 125.0, "{q:?}");
}

#[test]
fn a_rotated_page_trims_the_first_and_last_displayed_lines() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    let bytes = rotated();
    // From the middle of the "fox" line down to the middle of the "Sphinx" line, displayed.
    // Displayed y of line k is 612 - (72 + 20k) + ~4 for mid-glyph: fox 524, Pack 504, Sphinx 484.
    // User space for displayed (x, y) is (612 - y, x).
    let sel = select_text_range(
        pdfium,
        &bytes,
        0,
        612.0 - 524.0,
        200.0,
        612.0 - 484.0,
        120.0,
    )
    .unwrap()
    .unwrap();
    assert_eq!(sel.quads.len(), 3, "{sel:?}");
    let text = squash(&sel.text);
    assert!(text.contains(PACK), "{text}");
    assert!(
        !text.contains("Page1"),
        "the first displayed line is not part of the drag: {text}"
    );
}

#[test]
fn a_rect_selection_works_on_a_rotated_page() {
    let _g = PDFIUM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdfium = pdfium_or_skip!();
    let sel = select_text_in_rect(pdfium, &rotated(), 0, 100.0, 0.0, 112.0, 700.0)
        .unwrap()
        .unwrap();
    assert_eq!(squash(&sel.text), PACK);
}
