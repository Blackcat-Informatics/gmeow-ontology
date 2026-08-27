// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_notation.py (whole file; the
//! Python file is deleted).
//!
//! Both twins run over the merged ontology (`GraphStore::ontology()`):
//!   - `value_vocabularies_not_subclasses`: a dynamic whole-graph sweep asserting
//!     nothing subclasses the SymbolicSystemKind / NotationUsageRole value vocabs.
//!   - `ambiguous_cases_co_modelable`: gmeow:originFormal / LanguageOrigin are
//!     defined in the languages module (cross-slice), and the two value vocabs
//!     must remain unbridged.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// No unexpected subclasses of the SymbolicSystemKind or NotationUsageRole value
/// vocabularies exist anywhere in the merged ontology — the kinds and roles are an
/// open individual vocabulary, never a subclass lattice (Principle 9).
#[gmeow_test_batch_macros::batch_test]
fn value_vocabularies_not_subclasses() {
    let g = GraphStore::ontology();
    for value_type in ["SymbolicSystemKind", "NotationUsageRole"] {
        let node = gm(value_type);
        let offenders: Vec<String> = g
            .subjects(RDFS_SUBCLASS_OF, &node)
            .into_iter()
            .filter(|sub| sub != &node)
            .collect();
        assert!(
            offenders.is_empty(),
            "unexpected subclass(es) of gmeow:{value_type}: {offenders:?}"
        );
    }
}

/// Ambiguous systems can be co-modeled as both FormalLanguage and NotationSystem:
/// both value vocabularies (LanguageOrigin, SymbolicSystemKind) provide seed
/// individuals, and there is no inferential subclass bridge between them.
#[gmeow_test_batch_macros::batch_test]
fn ambiguous_cases_co_modelable() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("originFormal")),
            Some(RDF_TYPE),
            Some(&gm("LanguageOrigin"))
        ),
        "gmeow:originFormal must be a gmeow:LanguageOrigin"
    );
    assert!(
        g.has(
            Some(&gm("symbolicKindMusical")),
            Some(RDF_TYPE),
            Some(&gm("SymbolicSystemKind"))
        ),
        "gmeow:symbolicKindMusical must be a gmeow:SymbolicSystemKind"
    );
    assert!(
        !g.has(
            Some(&gm("LanguageOrigin")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("SymbolicSystemKind"))
        ),
        "gmeow:LanguageOrigin must not subclass gmeow:SymbolicSystemKind"
    );
    assert!(
        !g.has(
            Some(&gm("SymbolicSystemKind")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("LanguageOrigin"))
        ),
        "gmeow:SymbolicSystemKind must not subclass gmeow:LanguageOrigin"
    );
}
