// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_cognition.py
//!
//! Migrated tests (SHACL fixture-based, `run_shacl(...)` calls):
//!   - `test_wellformed_knowledge_proficiency_conforms`  →  `wellformed_knowledge_proficiency_conforms`
//!   - `test_malformed_knowledge_proficiency_is_flagged` →  `malformed_knowledge_proficiency_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_mental_moment_is_category_under_intrinsic_mode`: cross-slice subject
//!     (gmeow:MentalMoment lives in slices/core/kernel); `_graph()` TBox check.
//!   - `test_intentional_mode_reparented_under_mental_moment`: cross-slice subject
//!     (gmeow:IntentionalMode lives in slices/core/teleology); `_graph()` TBox check.
//!   - `test_proficiency_vocab_relocated_to_kernel`: cross-slice subjects
//!     (ProficiencyScale/Level/Modality live in slices/core/kernel); `_graph()` check.
//!
//! Migrated to native GraphStore / SSSOM-scan twins below (not SHACL):
//!   - `test_mental_moment_has_exactly_one_gufo_metaclass`: whole-merged-graph
//!     dynamic sweep over an open gufo:/logic: metaclass set.
//!   - `test_cognition_sssom_rows_include_expected_alignments`,
//!     `test_cognition_sssom_includes_corrected_wikidata_qids`,
//!     `test_cognition_sssom_includes_opencyc_knows_about`: row scans over
//!     `generated/mappings/gmeow-cognition.sssom.tsv`.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use std::collections::BTreeSet;

// ── Tests migrated from tests/test_cognition.py ───────────────────────────────

#[batch_cases]
#[case::wellformed_knowledge_proficiency_conforms(Case::file("shapes", "cognition-wellformed"))]
#[case::malformed_knowledge_proficiency_is_flagged(
    Case::file("shapes", "cognition-malformed")
        .fails()
        .violations(&[
            "must reference exactly one subject",
            "must carry exactly one KnowledgeLevel",
            "at most one scale",
        ])
)]
fn cognition(#[case] case: Case) {
    case.run();
}

// ── GraphStore / SSSOM twins migrated from tests/test_cognition.py ────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const GUFO: &str = "http://purl.org/nemo/gufo#";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The `(subject_id, predicate_id, object_id)` CURIE rows of the cognition mapping
/// set (`generated/mappings/gmeow-cognition.sssom.tsv`), skipping `#`-prefixed YAML
/// metadata lines and the TSV header. Mirrors `_cognition_sssom_rows()`.
fn cognition_sssom_rows() -> BTreeSet<(String, String, String)> {
    let text = generated_mapping("gmeow-cognition.sssom.tsv");
    let mut rows = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("subject_id") {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 3 {
            rows.insert((cols[0].to_owned(), cols[1].to_owned(), cols[2].to_owned()));
        }
    }
    rows
}

/// Twin of `test_mental_moment_has_exactly_one_gufo_metaclass`: each of the four
/// mental-umbrella classes carries exactly one ontological metaclass from the open
/// gufo:/logic: set. A whole-merged-graph dynamic sweep.
#[gmeow_test_batch_macros::batch_test]
fn mental_moment_has_exactly_one_gufo_metaclass() {
    let g = GraphStore::ontology();
    let metaclass_locals = [
        "Kind",
        "Category",
        "Relator",
        "Mode",
        "IntrinsicMode",
        "QualityValue",
        "AbstractIndividualType",
        "Phase",
        "Role",
        "SubKind",
        "RoleMixin",
        "PhaseMixin",
        "Mixin",
    ];
    let known_meta: BTreeSet<String> = metaclass_locals
        .iter()
        .flat_map(|m| [format!("{GUFO}{m}"), format!("{LOGIC}{m}")])
        .collect();
    for cls in [
        "MentalMoment",
        "CognitiveState",
        "KnowledgeProficiency",
        "KnowledgeLevel",
    ] {
        let types = g.objects(&gm(cls), RDF_TYPE);
        let meta: Vec<&String> = types.intersection(&known_meta).collect();
        assert_eq!(
            meta.len(),
            1,
            "{cls} must carry exactly one ontological metaclass, got {meta:?}"
        );
    }
}

/// Twin of `test_cognition_sssom_rows_include_expected_alignments`: the cognition
/// SSSOM ledger contains the expected cross-ontology rows.
#[gmeow_test_batch_macros::batch_test]
fn cognition_sssom_rows_include_expected_alignments() {
    let rows = cognition_sssom_rows();
    let expected: [(&str, &str, &str); 16] = [
        ("gmeow:knowsAbout", "skos:exactMatch", "schema:knowsAbout"),
        ("gmeow:knowsAbout", "skos:relatedMatch", "sumo:knows"),
        ("gmeow:knowsAbout", "skos:relatedMatch", "wd:Q9081"),
        ("gmeow:isAwareOf", "skos:relatedMatch", "sumo:knows"),
        ("gmeow:attendsTo", "skos:closeMatch", "foaf:focus"),
        ("gmeow:interestedIn", "skos:closeMatch", "foaf:interest"),
        ("gmeow:attendsTo", "skos:relatedMatch", "wd:Q6501338"),
        ("gmeow:curiousAbout", "skos:relatedMatch", "wd:Q366791"),
        ("gmeow:hasMastered", "skos:relatedMatch", "wd:Q12770764"),
        ("gmeow:knowsAbout", "skos:relatedMatch", "cyc:knowsAbout"),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "ctdlasn:"),
        (
            "gmeow:scaleKnowledgeDepth",
            "skos:relatedMatch",
            "esco-base:",
        ),
        ("gmeow:scaleKnowledgeDepth", "skos:relatedMatch", "onet:"),
        (
            "gmeow:scaleKnowledgeDepth",
            "skos:relatedMatch",
            "wd:Q1774565",
        ),
        (
            "gmeow:scaleKnowledgeDepth",
            "skos:relatedMatch",
            "wd:Q5307365",
        ),
        (
            "gmeow:scaleKnowledgeDepth",
            "skos:relatedMatch",
            "https://en.wikipedia.org/wiki/Structure_of_observed_learning_outcome",
        ),
    ];
    let missing: Vec<&(&str, &str, &str)> = expected
        .iter()
        .filter(|(s, p, o)| !rows.contains(&((*s).to_owned(), (*p).to_owned(), (*o).to_owned())))
        .collect();
    assert!(
        missing.is_empty(),
        "missing cognition SSSOM rows: {missing:?}"
    );
}

/// Twin of `test_cognition_sssom_includes_corrected_wikidata_qids`: the verified
/// QIDs are present and the rejected issue-supplied QIDs never crept back in.
#[gmeow_test_batch_macros::batch_test]
fn cognition_sssom_includes_corrected_wikidata_qids() {
    let rows = cognition_sssom_rows();
    let qids: BTreeSet<&String> = rows
        .iter()
        .map(|(_s, _p, o)| o)
        .filter(|o| o.starts_with("wd:"))
        .collect();
    for present in ["wd:Q6501338", "wd:Q366791", "wd:Q12770764"] {
        assert!(
            qids.contains(&present.to_owned()),
            "corrected QID {present} expected"
        );
    }
    for rejected in ["wd:Q327954", "wd:Q179637", "wd:Q1016098"] {
        assert!(
            !qids.contains(&rejected.to_owned()),
            "rejected issue QID {rejected} must not have crept back in"
        );
    }
}

/// Twin of `test_cognition_sssom_includes_opencyc_knows_about`: OpenCyc knowsAbout
/// is present as a relatedMatch anchor.
#[gmeow_test_batch_macros::batch_test]
fn cognition_sssom_includes_opencyc_knows_about() {
    let rows = cognition_sssom_rows();
    assert!(rows.contains(&(
        "gmeow:knowsAbout".to_owned(),
        "skos:relatedMatch".to_owned(),
        "cyc:knowsAbout".to_owned()
    )));
}
