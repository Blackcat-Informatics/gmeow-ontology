// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end proof: import a sample **external** Common Logic
//! (CLIF) knowledge base and reason over it — `parse_clif_str` → `LogicProgram`
//! IR → native materialization.
//!
//! The fixtures live under `conformance/logic/cl-ingest/`:
//!
//! - `sample-kb.logic.ttl` — the canonical `logic:` authoring source: a minimal
//!   genealogy theory (two Horn rules — an `ancestor` base case and its
//!   transitive step — over a `parent` EDB predicate).
//! - `sample-kb.clif` — the CLIF export of that program, produced by
//!   [`gmeow_logic_compile::clif::writer::project_clif`]. `parse_clif_str` only
//!   ever reconstructs the IR from the `;; @@gmeow-rdf-meta@@` carrier channel
//!   of a gmeow-dialect CLIF file (the idiomatic FOL sentences before the
//!   sentinel are a validated-only human view), so this fixture is produced by
//!   an explicit maintainer producer, never by this test.
//! - `sample-kb.edb.nq` — the EDB: `parent(alice, bob)` and `parent(bob, carol)`
//!   in the `.../genealogy/world-main` graph.
//!
//! The load-bearing assertion is the transitively derived `ancestor(alice,
//! carol)` edge — it cannot be produced by an EDB echo; only rule application
//! (the transitive-step Horn rule, applied twice) derives it.

use gmeow_logic_compile::clif::parse_clif_str;
use gmeow_logic_compile::frontend::Severity;

const GENEALOGY_NS: &str = "https://example.org/cl-ingest/genealogy/";

fn fixture_path(name: &str) -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/logic/cl-ingest/"
    )
    .to_owned()
        + name
}

fn read_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// The end-to-end ingest + reason proof: `parse_clif_str` on the committed
/// external CLIF KB → canonical structured materialization derives the
/// `ancestor` transitive closure over the `parent` EDB.
#[test]
fn sample_kb_clif_ingest_and_reason_derives_ancestor_closure() {
    // ── 1. Ingest: parse the committed external CLIF KB back into IR. ──────────
    let clif = read_fixture("sample-kb.clif");
    let source_iri = format!("{GENEALOGY_NS}kb");
    let (program, diags) =
        parse_clif_str(&clif, Some(source_iri)).expect("parse_clif_str(sample-kb.clif)");
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "CLIF ingest raised Error diagnostics on a project_clif-produced fixture: {diags:?}"
    );

    // ── 2. Reason directly from the canonical program over the committed EDB. ──
    let edb = read_fixture("sample-kb.edb.nq");
    let dataset = purrdf::parse_dataset(edb.as_bytes(), "application/n-quads", None)
        .expect("parse sample-kb.edb.nq");
    let materialized = gmeow_logic::materialize::materialize_program(
        &program,
        dataset.as_ref(),
        gmeow_logic::materialize::MaterializationLimits::default(),
        None,
    )
    .expect("materialize canonical CLIF program");
    let ancestor_pred = format!("{GENEALOGY_NS}ancestor");
    let want_alice = format!("<{GENEALOGY_NS}alice>");
    let want_bob = format!("<{GENEALOGY_NS}bob>");
    let want_carol = format!("<{GENEALOGY_NS}carol>");

    let derived_last: Vec<(String, String, String)> = materialized
        .quads
        .iter()
        .map(|q| {
            (
                gmeow_logic::provenance::term_display(&q.subject),
                q.predicate.clone(),
                gmeow_logic::provenance::term_display(&q.object),
            )
        })
        .collect();
    let found = derived_last
        .iter()
        .any(|(s, p, o)| p == &ancestor_pred && s == &want_alice && o == &want_carol);

    assert!(
        found,
        "ancestor(alice, carol) was NOT derived — the transitive-step \
         Horn rule must fire twice over the ingested CLIF program's rules; derived quads: \
         {derived_last:?}"
    );

    // The two base facts must also be present (asserted-EDB directly, and the
    // base Horn rule's direct application over `parent`).
    for (want_s, want_o) in [(&want_alice, &want_bob), (&want_bob, &want_carol)] {
        assert!(
            derived_last
                .iter()
                .any(|(s, p, o)| p == &ancestor_pred && s == want_s && o == want_o),
            "expected base ancestor({want_s}, {want_o}) to be derived; derived quads: \
             {derived_last:?}"
        );
    }
}

#[test]
fn annotated_materialization_carries_scores_through_canonical_ir() {
    let clif = read_fixture("sample-kb.clif");
    let source_iri = format!("{GENEALOGY_NS}kb");
    let (program, diags) =
        parse_clif_str(&clif, Some(source_iri)).expect("parse_clif_str(sample-kb.clif)");
    assert!(diags.iter().all(|diag| diag.severity != Severity::Error));
    let edb = read_fixture("sample-kb.edb.nq");
    let dataset = purrdf::parse_dataset(edb.as_bytes(), "application/n-quads", None)
        .expect("parse sample-kb.edb.nq");
    let parent = format!("{GENEALOGY_NS}parent");
    let alice = format!("<{GENEALOGY_NS}alice>");
    let bob = format!("<{GENEALOGY_NS}bob>");
    let carol = format!("<{GENEALOGY_NS}carol>");
    let annotated = gmeow_logic::materialize::materialize_program_annotated(
        &program,
        dataset.as_ref(),
        gmeow_logic::materialize::MaterializationLimits::default(),
        None,
        gmeow_logic::annotation::AnnotationRequest::new(
            &gmeow_logic::provenance::ZWeightSemiring,
            &gmeow_logic::annotation::AnnotationContract::exact(),
            |fact: gmeow_logic::annotation::AnnotationFactRef<'_>| {
                if fact.predicate != parent {
                    return None;
                }
                match (
                    gmeow_logic::provenance::term_display(fact.subject),
                    gmeow_logic::provenance::term_display(fact.object),
                ) {
                    (s, o) if s == alice && o == bob => Some(2),
                    (s, o) if s == bob && o == carol => Some(3),
                    _ => None,
                }
            },
        ),
    )
    .expect("annotated canonical materialization");

    assert_eq!(
        annotated.certification.query_class,
        gmeow_logic::annotation::AnnotationQueryClass::PositiveRecursive
    );
    let ancestor = format!("{GENEALOGY_NS}ancestor");
    let transitive = annotated
        .quads
        .iter()
        .find(|row| {
            row.quad.predicate == ancestor
                && gmeow_logic::provenance::term_display(&row.quad.subject) == alice
                && gmeow_logic::provenance::term_display(&row.quad.object) == carol
        })
        .expect("ancestor(alice, carol) annotated row");
    assert_eq!(transitive.annotation, 6);
    assert!(
        transitive
            .derivations
            .iter()
            .any(|derivation| derivation.annotation == 6 && derivation.sources.len() == 2),
        "{:#?}",
        transitive.derivations
    );
}
