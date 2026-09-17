// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated end-to-end proof: the producer imports a sample **external** Common Logic
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

use gmeow_logic_compile::frontend::Severity;

const GENEALOGY_NS: &str = "https://example.org/cl-ingest/genealogy/";

fn observation() -> &'static super::super::cl_ingest::Observation {
    static OBSERVATION: std::sync::OnceLock<
        Result<
            super::source_artifact::Selected<
                Result<super::super::cl_ingest::Observation, gmeow_errors::RecordedDiag>,
            >,
            gmeow_errors::Diag,
        >,
    > = std::sync::OnceLock::new();
    super::source_artifact::get(&OBSERVATION, super::super::cl_ingest::CHANNEL)
        .as_ref()
        .expect("producer-recorded CLIF ingestion")
}

/// The producer-recorded ingest + reason proof: `parse_clif_str` on the committed
/// external CLIF KB → canonical structured materialization derives the
/// `ancestor` transitive closure over the `parent` EDB.
#[test]
fn sample_kb_clif_ingest_and_reason_derives_ancestor_closure() {
    let observed = observation();
    let diags = &observed.diagnostics;
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "CLIF ingest raised Error diagnostics: {diags:?}"
    );
    let quads = observed
        .plain
        .as_ref()
        .expect("native CLIF materialization");
    let ancestor_pred = format!("{GENEALOGY_NS}ancestor");
    let want_alice = format!("<{GENEALOGY_NS}alice>");
    let want_bob = format!("<{GENEALOGY_NS}bob>");
    let want_carol = format!("<{GENEALOGY_NS}carol>");

    let derived_last: Vec<(String, String, String)> = quads
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
    let observed = observation();
    assert!(
        observed
            .diagnostics
            .iter()
            .all(|diag| diag.severity != Severity::Error)
    );
    let annotated = observed
        .annotated
        .as_ref()
        .expect("annotated canonical materialization");
    let alice = format!("<{GENEALOGY_NS}alice>");
    let carol = format!("<{GENEALOGY_NS}carol>");

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
