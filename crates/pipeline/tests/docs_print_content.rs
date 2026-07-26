// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content acceptance over the source-rendered print-documentation Typst source.
//!
//! The print PDF is compiled from a deterministic `gmeow.typ` that carries the
//! academic sections behind stable `// <<section:NAME>>` markers. These tests
//! render the plain-text `.typ` directly from the canonical documentation model
//! (NO GTS round-trip and no PDF parser — the `.typ` is the auditable source),
//! and assert two things:
//!
//! * **F1** — every academic section marker is present, the metrics section names
//!   a live term count backed by the bundle's own per-term corpus, each framework
//!   comparison carries at least one table row, and the FAIR section cites the
//!   exact FAIR-metadata gate literal `docs-print` emits.
//! * **F2** — the loss-appendix's dropped-capability set matches the single shared
//!   loss source `gmeow_docs::formats::format_capabilities(DocFormat::Pdf)`, the
//!   same table that feeds the `graph/docs-format-rendering` graph and the
//!   LossLedger. The appendix cannot drift from the graph because both read it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use docs_print::FAIR_GATE;
use gmeow_docs::formats::{DocFormat, format_capabilities};
/// The repository root, resolved off the crate manifest.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Render once from canonical sources. The second tuple member is the live model term count.
fn print_projection() -> &'static (String, usize) {
    static PROJECTION: OnceLock<(String, usize)> = OnceLock::new();
    PROJECTION.get_or_init(|| {
        let model = gmeow_docs::DocsModel::discover(&repo_root()).expect("discover docs model");
        let losses = [
            DocFormat::Site,
            DocFormat::Mdbook,
            DocFormat::Pdf,
            DocFormat::Snippets,
        ]
        .into_iter()
        .map(format_capabilities)
        .collect::<Vec<_>>();
        let typ = docs_print::render_typ(&model, &BTreeMap::new(), &[], &losses);
        (typ, model.terms.len())
    })
}

/// The section marker line for `name`.
fn marker(name: &str) -> String {
    format!("// <<section:{name}>>")
}

/// The text of the section introduced by `// <<section:name>>`, spanning from its
/// marker up to (but not including) the next `// <<section:` marker (or EOF).
fn section_text<'a>(typ: &'a str, name: &str) -> &'a str {
    let m = marker(name);
    let start = typ
        .find(&m)
        .unwrap_or_else(|| panic!("section marker {m:?} missing from gmeow.typ"));
    let after = &typ[start + m.len()..];
    let end = after
        .find("// <<section:")
        .map(|i| start + m.len() + i)
        .unwrap_or(typ.len());
    &typ[start..end]
}

#[test]
fn real_model_typ_compiles_to_pdf() {
    // The print projection inlines EVERY real slice's guide + child documents
    // (`docs.md`, `design/*.md`, …) as Typst. Their authored markdown — tables,
    // fenced code, lists, block-quotes, cross-document links, and hostile term
    // text — must lower to a document that actually COMPILES, not merely render to
    // a plausible-looking source. This is the whole-corpus compile gate: if the
    // Markdown→Typst inliner ever emits invalid Typst for any real document, this
    // reds here rather than only when the build stage compiles the PDF.
    let typ = &print_projection().0;
    let pdf = docs_print::compile_pdf(typ, &[]).expect("the real-model print Typst must compile");
    assert!(pdf.starts_with(b"%PDF"), "compile_pdf must produce a PDF");
}

#[test]
fn print_typ_carries_every_section_marker() {
    let typ = &print_projection().0;
    for name in [
        "metrics",
        "methodology",
        "fair",
        "loss-appendix",
        "comparison-gufo",
        "comparison-bfo",
        "comparison-dolce",
        "pipeline-dag",
    ] {
        assert!(
            typ.contains(&marker(name)),
            "gmeow.typ is missing the {} marker",
            marker(name)
        );
    }
}

#[test]
fn metrics_section_names_a_live_term_count() {
    let (typ, term_count) = print_projection();
    let metrics = section_text(typ, "metrics");

    // The metrics section is a `#table(...)` block.
    assert!(
        metrics.contains("#table("),
        "metrics section must render a #table block, got:\n{metrics}"
    );

    // Pull the printed `"Terms", "<N>"` metric row and assert it equals the
    // canonical documentation model's live term count.
    let printed = printed_terms_metric(metrics);
    assert_eq!(printed, *term_count);
}

/// Parse the `"Terms", "<N>",` metric row's integer out of the metrics section.
fn printed_terms_metric(metrics: &str) -> usize {
    // The row is `  "Terms", "<N>",` (tstr-quoted cells). Find the `"Terms",`
    // cell, then read the integer inside the next quoted cell.
    let anchor = metrics
        .find("\"Terms\"")
        .unwrap_or_else(|| panic!("metrics section has no \"Terms\" row:\n{metrics}"));
    let rest = &metrics[anchor + "\"Terms\"".len()..];
    let open = rest
        .find('"')
        .unwrap_or_else(|| panic!("no value cell after the Terms label:\n{metrics}"));
    let after = &rest[open + 1..];
    let close = after
        .find('"')
        .unwrap_or_else(|| panic!("unterminated Terms value cell:\n{metrics}"));
    after[..close]
        .parse::<usize>()
        .unwrap_or_else(|e| panic!("Terms metric {:?} is not an integer: {e}", &after[..close]))
}

#[test]
fn each_framework_comparison_has_at_least_one_row() {
    let typ = &print_projection().0;
    for name in ["comparison-gufo", "comparison-bfo", "comparison-dolce"] {
        let section = section_text(typ, name);
        assert!(
            section.contains("#table("),
            "{name} must render a #table block"
        );
        assert!(
            section.contains("table.header("),
            "{name} table must carry a header row"
        );
        // A data row is a line whose first non-space char is a quote (a tstr cell);
        // the `table.header(...)` line starts with `table`, so it is excluded.
        let data_rows = section
            .lines()
            .filter(|l| l.trim_start().starts_with('"'))
            .count();
        assert!(
            data_rows >= 1,
            "{name} table must carry at least one data row beyond the header, section:\n{section}"
        );
    }
}

#[test]
fn fair_section_cites_the_exact_fair_gate() {
    let typ = &print_projection().0;
    let fair = section_text(typ, "fair");
    assert!(
        fair.contains(FAIR_GATE),
        "FAIR section must cite the exact FAIR-metadata gate literal {FAIR_GATE:?}, section:\n{fair}"
    );
}

#[test]
fn loss_appendix_matches_the_shared_pdf_loss_source() {
    let typ = &print_projection().0;
    let appendix = section_text(typ, "loss-appendix");

    // The single source of truth for the PDF's declared losses — the same table
    // that grounds `graph/docs-format-rendering` and the LossLedger.
    let caps = format_capabilities(DocFormat::Pdf);
    assert!(
        !caps.dropped.is_empty(),
        "the PDF format is expected to declare capability losses"
    );
    for cap in &caps.dropped {
        assert!(
            appendix.contains(cap.slug()),
            "loss-appendix must enumerate the dropped capability slug {:?} from the shared \
             loss source, section:\n{appendix}",
            cap.slug()
        );
    }
}
