// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `docs-print` — the deterministic Typst-source renderer and byte-reproducible
//! in-memory PDF compiler for the GMEOW print documentation projection.
//!
//! [`render_typ`] turns the shared [`DocsModel`](gmeow_docs::model::DocsModel)
//! into a pure, `insta`-goldenable Typst source string, and [`compile_pdf`]
//! compiles that source in a minimal, fully in-memory [`typst::World`] (no
//! filesystem, no clock, embedded fonts only) to a PDF whose bytes are stable
//! across runs — no `/CreationDate`, a fixed document identifier. The embedded
//! font set is pinned by [`embedded_font_digest`].

mod doc_render;
mod error;
mod render;
mod world;

use gmeow_errors::Diag;
use typst::foundations::Smart;
use typst::layout::{Frame, FrameItem, PagedDocument};
use typst_pdf::{PdfOptions, PdfStandards};

pub use render::{FAIR_GATE, escape_typ, render_typ};
pub use world::{BIB_PATH, MAIN_PATH, embedded_font_digest};

/// The fixed PDF document identifier. A constant (not a hash of the mutable
/// title/author) so the identifier never drifts and the PDF stays byte-stable.
const PDF_IDENT: &str = "gmeow-print-docs";

/// Compile deterministic Typst `typ` source (with its `bib` bytes) to the
/// finished Typst frame tree — the exact [`PagedDocument`] both [`compile_pdf`]
/// hands to `typst-pdf` for serialization and [`pdf_text_layer`] walks for its
/// extracted text. Any Typst compile diagnostic is a hard failure.
fn compile_document(typ: &str, bib: &[u8]) -> Result<PagedDocument, Diag> {
    let world = world::PrintWorld::new(typ, bib);
    let compiled = typst::compile::<PagedDocument>(&world);
    compiled
        .output
        .map_err(|diags| error::from_typst("Typst compilation", &diags))
}

/// Compile deterministic Typst `typ` source (with its `bib` bytes) to a PDF.
///
/// The world carries exactly the main source, the `references.bib` bytes, and the
/// embedded [`typst_assets`] fonts; it has NO clock ([`typst::World::today`]
/// returns `None`), so any use of `datetime.today()` is a hard compile error. PDF
/// export is configured with no timestamp and a fixed identifier, so the output
/// carries no `/CreationDate` and is byte-reproducible. Any Typst compile or
/// export diagnostic is a hard failure, returned as a [`Diag`].
pub fn compile_pdf(typ: &str, bib: &[u8]) -> Result<Vec<u8>, Diag> {
    let document = compile_document(typ, bib)?;

    let options = PdfOptions {
        // A fixed identifier (never a wall-clock/environment value); its hash
        // becomes the PDF document id, so the output stays byte-stable.
        ident: Smart::Custom(PDF_IDENT),
        // No timestamp: the reproducibility contract forbids embedding time, so
        // the PDF carries no `/CreationDate`.
        timestamp: None,
        page_ranges: None,
        standards: PdfStandards::default(),
    };

    let pdf = typst_pdf::pdf(&document, &options)
        .map_err(|diags| error::from_typst("PDF export", &diags))?;
    Ok(pdf)
}

/// The PDF's TEXT LAYER: the plain text of every compiled page's frame tree, in
/// document order, one page per line.
///
/// This walks the SAME [`PagedDocument`] frame tree `compile_pdf` hands to
/// `typst-pdf` for serialization, collecting each [`FrameItem::Text`] run's
/// plain text (recursing into [`FrameItem::Group`] subframes for tables/boxes).
/// `typst-pdf` writes these exact runs into the PDF's text-showing operators and
/// their `ToUnicode` CMaps, so this is genuinely what a PDF text-extraction tool
/// (or copy-paste) recovers from the rendered document — unlike grepping the
/// Typst SOURCE, which would trivially contain markup that never actually lays
/// out (e.g. inside a comment or a broken directive). A gate that greps this
/// output only passes when content survives all the way through compilation.
pub fn pdf_text_layer(typ: &str, bib: &[u8]) -> Result<String, Diag> {
    let document = compile_document(typ, bib)?;
    let mut out = String::new();
    for page in &document.pages {
        collect_frame_text(&page.frame, &mut out);
        out.push('\n');
    }
    Ok(out)
}

/// Recursively append every text run's plain text from `frame` (and its nested
/// groups) to `out`, space-separated.
fn collect_frame_text(frame: &Frame, out: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Text(text) => {
                out.push_str(&text.text);
                out.push(' ');
            }
            FrameItem::Group(group) => collect_frame_text(&group.frame, out),
            _ => {}
        }
    }
}
