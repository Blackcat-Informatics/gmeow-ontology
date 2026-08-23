// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Single-source cross-check: every canonical construct the reasoner lowers at
//! the EDB boundary (crates/logic `reason::CALCULUS_VOCABULARY`) MUST be backed
//! by a shipped `logic:GroundingCorrespondence` in the `logic:` grounding slice.
//!
//! The reasoner's fixed-calculus lowering table pairs each canonical `logic:`
//! construct with its generated W3C OWL/RDFS projection (Principle 17). Those
//! pairs are also authored, as first-class meta-level correspondence data, in
//! `slices/grounding/logic/module.ttl` (routed into `graph/correspondence-laws`).
//! If the two ever drift — a reasoner row with no shipped law, or a renamed
//! target — a reader of the bundle can no longer justify the lowering the engine
//! performs. This test parses the shipped laws and asserts the Rust table is a
//! subset of them, so the drift is a red test rather than a silent divergence.
//!
//! The expected pairs are read DIRECTLY from the reasoner via
//! [`gmeow_logic::reason::calculus_vocabulary`] (the table was a private `static`; it is
//! now exposed) rather than re-typed here, so the two can never drift; the pairs the
//! `gmeow-ns` crate also anchors are cross-checked against those constants so the ns
//! constants, the reasoner table, and the shipped laws are pinned to one another.

use std::collections::BTreeSet;
use std::path::PathBuf;

use purrdf::slice::rdf_query::Dataset;

const GROUNDING_CORRESPONDENCE: &str =
    "https://blackcatinformatics.ca/logic/GroundingCorrespondence";
const SOURCE_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const TARGET_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root two levels above crates/validate")
        .to_path_buf()
}

fn logic_module() -> PathBuf {
    repo_root().join("slices/grounding/logic/module.ttl")
}

/// The (source, target) endpoint pairs of every shipped `logic:GroundingCorrespondence`.
fn shipped_grounding_pairs() -> BTreeSet<(String, String)> {
    let path = logic_module();
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let ds = Dataset::parse_turtle(&bytes, &path.display().to_string())
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));

    let mut pairs = BTreeSet::new();
    for law in ds
        .subjects_of_type(GROUNDING_CORRESPONDENCE)
        .expect("query logic:GroundingCorrespondence subjects")
    {
        let sources = ds
            .object_iris(&law, SOURCE_ENDPOINT)
            .expect("query logic:sourceEndpoint");
        let targets = ds
            .object_iris(&law, TARGET_ENDPOINT)
            .expect("query logic:targetEndpoint");
        // The shape pins exactly one of each; guard the invariant so a
        // malformed law cannot silently drop a pair.
        assert_eq!(
            sources.len(),
            1,
            "grounding correspondence {law} must declare exactly one logic:sourceEndpoint"
        );
        assert_eq!(
            targets.len(),
            1,
            "grounding correspondence {law} must declare exactly one logic:targetEndpoint"
        );
        pairs.insert((sources[0].clone(), targets[0].clone()));
    }
    pairs
}

/// The reasoner's `CALCULUS_VOCABULARY`, consumed DIRECTLY from the engine via
/// [`gmeow_logic::reason::calculus_vocabulary`] rather than re-typed here — each
/// `(canonical logic: IRI, projected W3C OWL/RDFS IRI)` pair as owned `String`s. A
/// hand-copied mirror could drift from the engine table silently; reading the exposed
/// table makes any drift a compile/link fact, not a stale duplicate.
fn expected_calculus_pairs() -> BTreeSet<(String, String)> {
    gmeow_logic::reason::calculus_vocabulary()
        .iter()
        .map(|(canonical, projected)| ((*canonical).to_owned(), (*projected).to_owned()))
        .collect()
}

/// Every reasoner-lowered construct is backed by a shipped grounding law.
#[test]
fn calculus_vocabulary_is_backed_by_shipped_grounding_laws() {
    let shipped = shipped_grounding_pairs();
    assert!(
        !shipped.is_empty(),
        "no logic:GroundingCorrespondence laws parsed from the logic slice — the query is vacuous"
    );

    let expected = expected_calculus_pairs();
    // Non-vacuity: the mirrored table must be the full 51-row calculus vocabulary.
    assert_eq!(
        expected.len(),
        51,
        "expected calculus vocabulary must have 51 rows, matching CALCULUS_VOCABULARY"
    );

    let missing: Vec<&(String, String)> = expected.difference(&shipped).collect();
    assert!(
        missing.is_empty(),
        "reasoner CALCULUS_VOCABULARY rows with no shipped logic:GroundingCorrespondence \
         (source, target) law:\n{missing:#?}"
    );
}

/// The `gmeow-ns` typing-marker constants agree with the shipped laws, pinning
/// the ns anchors, the reasoner table, and the correspondence corpus together.
#[test]
fn ns_typing_marker_constants_match_shipped_laws() {
    let shipped = shipped_grounding_pairs();
    for (logic_iri, owl_iri) in [
        (gmeow_ns::LOGIC_CLASS, gmeow_ns::OWL_CLASS),
        (
            gmeow_ns::LOGIC_OBJECT_PROPERTY,
            gmeow_ns::OWL_OBJECT_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_DATATYPE_PROPERTY,
            gmeow_ns::OWL_DATATYPE_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_ANNOTATION_PROPERTY,
            gmeow_ns::OWL_ANNOTATION_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_NAMED_INDIVIDUAL,
            gmeow_ns::OWL_NAMED_INDIVIDUAL,
        ),
        (gmeow_ns::LOGIC_ONTOLOGY, gmeow_ns::OWL_ONTOLOGY),
        (gmeow_ns::LOGIC_THING, gmeow_ns::OWL_THING),
        (gmeow_ns::LOGIC_NOTHING, gmeow_ns::OWL_NOTHING),
        (gmeow_ns::LOGIC_RESTRICTION, gmeow_ns::OWL_RESTRICTION),
    ] {
        let pair = (logic_iri.to_owned(), owl_iri.to_owned());
        assert!(
            shipped.contains(&pair),
            "ns constant pair {pair:?} has no backing logic:GroundingCorrespondence law"
        );
    }
}
