// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from
//! `slices/extensions/dreaming/tests/test_dreaming.py`.
//!
//! The competency guard for the dreaming extension examples: the merged ontology
//! closed with every `slices/extensions/dreaming/examples/*.ttl` composes dreams,
//! dream reports, dream elements, lucid-dream awareness modes, and
//! memory-consolidation replay with analogical transfer.
//!
//! Store composition: the originals ran `load_merged_graph(include_imports=False)`
//! then `graph.parse` for each sorted `examples/*.ttl`. The native twin builds the
//! same graph via `GraphStore::ontology_plus_ttl_dir(examples/)`. The queries are
//! the same inline SPARQL strings the originals used (rdflib set-of-`str(row[0])`
//! semantics reproduced as distinct-value counts / membership).

use crate::conformance_support::*;
use purrdf::TermValue;
use std::collections::BTreeSet;

/// The merged ontology closed with every dreaming example, mirroring the Python
/// `_load_dreaming_graph()`.
fn dreaming_store() -> GraphStore {
    GraphStore::ontology_plus_ttl_dir(&repo_root().join("slices/extensions/dreaming/examples"))
}

/// Collect the distinct IRI values of the first projected column (the native twin
/// of the Python `{str(row[0]) for row in results}` set).
fn distinct_col0_iris(rows: &[Vec<Option<TermValue>>]) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|r| match r.first() {
            Some(Some(TermValue::Iri(iri))) => Some(iri.to_string()),
            _ => None,
        })
        .collect()
}

/// Twin of `test_dream_experience_composition`: dreams are composed Experiences with
/// a dreaming process, imagined origin, and an awareness mode drawn from the
/// `FILTER (?mode IN (…))` set.
#[gmeow_test_batch_macros::batch_test]
fn dream_experience_composition() {
    let g = dreaming_store();
    let (_, rows) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?dream
         WHERE {
             ?dream a gmeow:Experience ;
                    gmeow:mentalProcessType gmeow:processDreaming ;
                    gmeow:contentOrigin gmeow:originImagined ;
                    gmeow:awarenessMode ?mode .
             FILTER (?mode IN (
                 gmeow:modeDreaming,
                 gmeow:modeREM,
                 gmeow:modeLucidDreaming
             ))
         }",
    );
    assert!(
        !distinct_col0_iris(&rows).is_empty(),
        "Expected at least one composed dream Experience."
    );
}

/// Twin of `test_dream_report_composition`: dream reports are `DreamReport`
/// recollections with imagined content origin.
#[gmeow_test_batch_macros::batch_test]
fn dream_report_composition() {
    let g = dreaming_store();
    let (_, rows) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?report
         WHERE {
             ?report a gmeow:DreamReport ;
                     gmeow:mentalProcessType gmeow:processRecollection ;
                     gmeow:contentOrigin gmeow:originImagined .
         }",
    );
    assert!(
        !distinct_col0_iris(&rows).is_empty(),
        "Expected at least one composed DreamReport."
    );
}

/// Twin of `test_dream_element_links`: dream experiences link to imagined
/// constituents via `gmeow:dreamElement`.
#[gmeow_test_batch_macros::batch_test]
fn dream_element_links() {
    let g = dreaming_store();
    let (_, rows) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?dream ?element
         WHERE {
             ?dream gmeow:dreamElement ?element .
         }",
    );
    assert!(
        !distinct_col0_iris(&rows).is_empty(),
        "Expected at least one dream-to-element link."
    );
}

/// Twin of `test_lucid_dream_uses_mode_lucid_dreaming`: exactly one example
/// experience is a dream with the lucid-dreaming mode.
#[gmeow_test_batch_macros::batch_test]
fn lucid_dream_uses_mode_lucid_dreaming() {
    let g = dreaming_store();
    let (_, rows) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?dream
         WHERE {
             ?dream a gmeow:Experience ;
                    gmeow:mentalProcessType gmeow:processDreaming ;
                    gmeow:awarenessMode gmeow:modeLucidDreaming .
         }",
    );
    let dreams = distinct_col0_iris(&rows);
    assert_eq!(
        dreams.len(),
        1,
        "Expected exactly one lucid dreaming Experience, found {}: {dreams:?}",
        dreams.len()
    );
}

/// Twin of `test_memory_consolidation_replay`: AI replay is a
/// consolidation/concept-formation `LearningEvent` with an `Analogy` reached via
/// `gmeow:learnedFrom`. Reproduces the original's TWO-STAGE runtime-templated
/// `VALUES` block: stage 1 selects the `LearningEvent`s, stage 2 embeds them as a
/// `VALUES ?replay { … }` block and finds the analogies.
#[gmeow_test_batch_macros::batch_test]
fn memory_consolidation_replay() {
    let g = dreaming_store();

    let (_, replay_rows) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?replay
         WHERE {
             ?replay a gmeow:LearningEvent ;
                     gmeow:learningType gmeow:learningConsolidation ;
                     gmeow:learningType gmeow:learningConceptFormation .
         }",
    );
    let replays = distinct_col0_iris(&replay_rows);
    assert!(
        !replays.is_empty(),
        "Expected at least one LearningEvent with consolidation and concept formation."
    );

    // Runtime-templated VALUES block, exactly as the Python `" ".join(f"<{uri}>" …)`.
    let values_block = replays
        .iter()
        .map(|uri| format!("<{uri}>"))
        .collect::<Vec<_>>()
        .join(" ");
    let analogy_query = format!(
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         SELECT ?analogy
         WHERE {{
             VALUES ?replay {{ {values_block} }}
             ?replay gmeow:learnedFrom ?analogy .
             ?analogy a gmeow:Analogy .
         }}"
    );
    let (_, analogy_rows) = g.select(&[], &analogy_query);
    assert!(
        !distinct_col0_iris(&analogy_rows).is_empty(),
        "Expected at least one Analogy linked to the consolidation/concept-formation \
         LearningEvent via gmeow:learnedFrom."
    );
}
