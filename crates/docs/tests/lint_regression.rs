// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anchor/link/coverage lint regression suite.
//!
//! The in-module `lint.rs` tests already cover each finding code in isolation
//! over synthetic models. This integration suite adds three things those cannot:
//! (1) the **live-model doctrine gate** — the real discovered docs must lint with
//! ZERO errors (what the `make check` doc-lint gate enforces); (2) a
//! deterministic-ordering regression over a site that triggers EVERY finding code
//! at once; and (3) the **recorded coverage-ratchet baseline** — an insta snapshot
//! of the live per-code coverage-warning counts that burns down as source prose
//! and alignments land. Findings are documented as deterministically sorted; this
//! pins it.

// Rich colored line-diffs on assert_eq! failure; shadows the std macro for this
// file. Identical behaviour on pass; insta snapshots are unaffected.
use pretty_assertions::assert_eq;
use std::collections::{BTreeMap, BTreeSet};

use gmeow_docs::lint::lint;
use gmeow_docs::model::{DocTerm, DocTermCategory};
use gmeow_docs::render::Site;
use gmeow_docs::DocsModel;

mod common;

#[test]
fn live_docs_lint_is_clean() {
    // The doctrine guarantee: the real rendered docs carry NO lint errors
    // (dangling links / broken anchors). This is the gate `make check` depends on.
    let model = common::cached_model();
    let site = common::cached_site();
    let report = lint(&model, &site);
    assert_eq!(
        report.error_count(),
        0,
        "live docs lint must be error-free; got: {:?}",
        report.legacy_errors()
    );
}

#[test]
fn live_docs_lint_is_deterministic() {
    // Two lint passes over the same site must yield byte-identical finding
    // sequences (code + severity + message), proving the documented sort order.
    let model = common::cached_model();
    let site = common::cached_site();
    let a = lint(&model, &site);
    let b = lint(&model, &site);
    let key = |r: &gmeow_diagnostics::Report| {
        r.findings
            .iter()
            .map(|f| (f.severity, f.code.clone(), f.message.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(key(&a), key(&b), "lint findings must be deterministic");
}

/// Minimal vocabulary term (mirrors the `lint.rs` in-module `term` helper).
fn term(local: &str, definition: Option<&str>, label: Option<&str>) -> DocTerm {
    DocTerm {
        iri: format!("https://blackcatinformatics.ca/gmeow/{local}"),
        curie: format!("gmeow:{local}"),
        label: label.map(str::to_string),
        definition: definition.map(str::to_string),
        category: DocTermCategory::Class,
        owner_slice: "https://blackcatinformatics.ca/gmeow/slices/core/test".to_string(),
        parents: Vec::new(),
        domain: Vec::new(),
        range: Vec::new(),
        scope_notes: Vec::new(),
        examples: Vec::new(),
        use_when: Vec::new(),
        avoid_when: Vec::new(),
        how_to_use: Vec::new(),
        use_for_consumer: Vec::new(),
        avoid_for_consumer: Vec::new(),
        ..Default::default()
    }
}

/// A synthetic model carrying exactly the given terms (everything else empty).
fn model_with_terms(terms: Vec<DocTerm>) -> DocsModel {
    DocsModel {
        title: "T".to_string(),
        version: "2".to_string(),
        slices: Vec::new(),
        terms,
        dependency_edges: Vec::new(),
        mapping_sets: Vec::new(),
        linkages: Vec::new(),
        examples: Vec::new(),
        shapes: Vec::new(),
        competencies: Vec::new(),
        concerns: Vec::new(),
        external_terms: Vec::new(),
        recipes: Vec::new(),
        learning_paths: Vec::new(),
        four_boxes: None,
        concept_doi: None,
        available_languages: vec!["english".to_string()],
        translations: Default::default(),
        ui_catalog: Default::default(),
    }
}

#[test]
fn site_triggering_all_codes_emits_each_and_is_deterministic() {
    // A bare term with no linkage trips every coverage warning (missing
    // definition, label, usage-advice, example, scope-note, alignment); the
    // hand-built site adds a dangling internal link (→ docs/dangling-link) and a
    // broken in-page anchor (→ docs/broken-anchor). Together: all eight codes.
    let model = model_with_terms(vec![term("Bare", None, None)]);
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "index.html".to_string(),
        br#"<a href="missing/index.html">x</a>"#.to_vec(),
    );
    files.insert(
        "other/index.html".to_string(),
        br##"<a href="#nope">y</a><h2 id="real">y</h2>"##.to_vec(),
    );
    let site = Site { files };

    let report = lint(&model, &site);
    let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
    for code in [
        "docs/dangling-link",
        "docs/broken-anchor",
        "docs/missing-definition",
        "docs/missing-label",
        "docs/missing-usage-advice",
        "docs/missing-example",
        "docs/missing-scope-note",
        "docs/missing-alignment",
    ] {
        assert!(
            codes.contains(code),
            "expected finding code `{code}`; got {codes:?}"
        );
    }

    // Deterministic ordering across two passes.
    let seq = |r: &gmeow_diagnostics::Report| {
        r.findings
            .iter()
            .map(|f| (f.severity, f.code.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(seq(&report), seq(&lint(&model, &site)));
}

#[test]
fn coverage_ratchet_baseline_is_recorded() {
    // The recorded report-only ratchet: a snapshot of the live per-code coverage
    // WARNING counts. These are warnings (the gate stays green); this golden is the
    // committed baseline, and the counts are EXPECTED to fall over time as source
    // prose, examples, scope notes, and alignments land. When they change, run
    // `cargo insta review` and accept the lower numbers — the diff is the burn-down
    // ledger. This golden legitimately drifts with slice content; do not chase it in
    // unrelated PRs.
    let model = common::cached_model();
    let site = common::cached_site();
    let report = lint(&model, &site);

    let coverage: BTreeMap<String, usize> = report
        .counts_by_code()
        .into_iter()
        .filter(|(code, _)| code.starts_with("docs/missing-"))
        .collect();

    insta::assert_json_snapshot!("coverage_ratchet_baseline", coverage);
}
