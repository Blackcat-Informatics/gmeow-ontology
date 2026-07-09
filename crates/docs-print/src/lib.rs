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

mod error;
mod render;
mod world;

use gmeow_errors::Diag;
use typst::foundations::Smart;
use typst::layout::PagedDocument;
use typst_pdf::{PdfOptions, PdfStandards};

pub use render::{FAIR_GATE, escape_typ, render_typ};
pub use world::{BIB_PATH, MAIN_PATH, embedded_font_digest};

/// The fixed PDF document identifier. A constant (not a hash of the mutable
/// title/author) so the identifier never drifts and the PDF stays byte-stable.
const PDF_IDENT: &str = "gmeow-print-docs";

/// Compile deterministic Typst `typ` source (with its `bib` bytes) to a PDF.
///
/// The world carries exactly the main source, the `references.bib` bytes, and the
/// embedded [`typst_assets`] fonts; it has NO clock ([`typst::World::today`]
/// returns `None`), so any use of `datetime.today()` is a hard compile error. PDF
/// export is configured with no timestamp and a fixed identifier, so the output
/// carries no `/CreationDate` and is byte-reproducible. Any Typst compile or
/// export diagnostic is a hard failure, returned as a [`Diag`].
pub fn compile_pdf(typ: &str, bib: &[u8]) -> Result<Vec<u8>, Diag> {
    let world = world::PrintWorld::new(typ, bib);

    let compiled = typst::compile::<PagedDocument>(&world);
    let document = compiled
        .output
        .map_err(|diags| error::from_typst("Typst compilation", &diags))?;

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
