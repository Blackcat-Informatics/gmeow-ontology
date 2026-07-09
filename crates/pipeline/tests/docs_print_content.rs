// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content acceptance over the committed print-documentation Typst source.
//!
//! The print PDF is compiled from a deterministic `gmeow.typ` that carries the
//! academic sections behind stable `// <<section:NAME>>` markers. These tests
//! fold the SHIPPED `generated/dist/gmeow.gts`, pull the plain-text `.typ` out of
//! the `docs-print` archive (NO PDF parser — the `.typ` is the auditable source),
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

use std::collections::BTreeSet;
use std::path::PathBuf;

use docs_print::FAIR_GATE;
use gmeow_docs::formats::{DocFormat, format_capabilities};
use gmeow_pipeline::bundle_blobs::Bundle;

/// The committed bundle path, resolved off the crate manifest.
fn committed_gts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("generated/dist/gmeow.gts")
}

/// Fold the committed bundle once.
fn committed_bundle() -> Bundle {
    let bytes = std::fs::read(committed_gts_path()).expect("read committed gmeow.gts");
    Bundle::from_snapshot(&bytes).expect("fold committed gmeow.gts")
}

/// The plain-text `x-gmeow-english/gmeow.typ` from the `docs-print` archive.
fn print_typ_source(bundle: &Bundle) -> String {
    let archive = bundle.docs_print().expect("resolve docs-print archive");
    let bytes = archive
        .get("x-gmeow-english/gmeow.typ")
        .expect("docs-print carries x-gmeow-english/gmeow.typ");
    String::from_utf8(bytes.clone()).expect("gmeow.typ is UTF-8")
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
fn print_typ_carries_every_section_marker() {
    let bundle = committed_bundle();
    let typ = print_typ_source(&bundle);
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
    let bundle = committed_bundle();
    let typ = print_typ_source(&bundle);
    let metrics = section_text(&typ, "metrics");

    // The metrics section is a `#table(...)` block.
    assert!(
        metrics.contains("#table("),
        "metrics section must render a #table block, got:\n{metrics}"
    );

    // Live datum: the bundle's own per-term corpus. Each documented term emits a
    // `x-gmeow-english/terms/<slug>/card.md`; distinct `<slug>` values are a lower
    // bound on the model's term count (several IRIs can share one page slug, so
    // the printed count is ≥ the distinct-slug count, never below it). Deriving the
    // floor from the bundle proves the metric is real live data, not a placeholder.
    let docs = bundle
        .ontology_docs()
        .expect("resolve ontology-docs archive");
    let card_slugs: BTreeSet<&str> = docs
        .keys()
        .filter_map(|k| k.strip_prefix("x-gmeow-english/terms/"))
        .filter_map(|rest| rest.strip_suffix("/card.md"))
        .collect();
    let term_floor = card_slugs.len();
    assert!(
        term_floor > 0,
        "the committed ontology-docs must carry per-term cards"
    );

    // Pull the printed `"Terms", "<N>"` metric row and assert N is a positive
    // integer at least the live floor.
    let printed = printed_terms_metric(metrics);
    assert!(
        printed >= term_floor,
        "metrics section term count {printed} is below the live per-term-card floor \
         {term_floor} — the printed metric is not backed by the bundle's own corpus"
    );
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
    let bundle = committed_bundle();
    let typ = print_typ_source(&bundle);
    for name in ["comparison-gufo", "comparison-bfo", "comparison-dolce"] {
        let section = section_text(&typ, name);
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
    let bundle = committed_bundle();
    let typ = print_typ_source(&bundle);
    let fair = section_text(&typ, "fair");
    assert!(
        fair.contains(FAIR_GATE),
        "FAIR section must cite the exact FAIR-metadata gate literal {FAIR_GATE:?}, section:\n{fair}"
    );
}

#[test]
fn loss_appendix_matches_the_shared_pdf_loss_source() {
    let bundle = committed_bundle();
    let typ = print_typ_source(&bundle);
    let appendix = section_text(&typ, "loss-appendix");

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
