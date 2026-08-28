// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_expertise.py
//!
//! Retained dynamic/cross-slice TBox guards for the expertise module,
//! ported to native `#[test]` fns over `GraphStore::ontology()` (the native twin
//! of the merged `load_merged_graph(include_imports=False)` graph):
//!
//! - `test_proficiency_scale_is_generalised` → [`proficiency_scale_is_generalised`]
//! - `test_proficiency_levels_carry_scale` → [`proficiency_levels_carry_scale`]
//! - `test_no_primary_or_preferred_skill_term` → [`no_primary_or_preferred_skill_term`]
//! - `test_endorsement_uses_attestation` → [`endorsement_uses_attestation`]

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

/// The three property flavours a banned selector could be minted as.
const PROP_TYPES: [&str; 3] = [
    OWL_OBJECT_PROPERTY,
    OWL_DATATYPE_PROPERTY,
    OWL_ANNOTATION_PROPERTY,
];

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn logic(local: &str) -> String {
    format!("{LOGIC}{local}")
}

/// Twin of `test_proficiency_scale_is_generalised`.
///
/// ProficiencyScale is a logic:QualityValue and all expected scale seeds exist.
#[gmeow_test_batch_macros::batch_test]
fn proficiency_scale_is_generalised() {
    let g = GraphStore::ontology();
    let scale_class = gmeow("ProficiencyScale");
    assert!(
        g.has(
            Some(&scale_class),
            Some(RDFS_SUBCLASS_OF),
            Some(&logic("QualityValue"))
        ),
        "ProficiencyScale must be rdfs:subClassOf logic:QualityValue"
    );
    for scale in [
        "scaleCEFR",
        "scaleILR",
        "scaleACTFL",
        "scaleSelfReported",
        "scaleDreyfus",
        "scaleNIH",
        "scaleAssessed",
    ] {
        assert!(
            g.has(Some(&gmeow(scale)), Some(RDF_TYPE), Some(&scale_class)),
            "{scale} must be a gmeow:ProficiencyScale"
        );
    }
}

/// Twin of `test_proficiency_levels_carry_scale`.
///
/// Each proficiency level individual is linked to its parent scale.
#[gmeow_test_batch_macros::batch_test]
fn proficiency_levels_carry_scale() {
    let g = GraphStore::ontology();
    let level_scale = gmeow("levelScale");
    for (level, scale) in [
        ("cefrB2", "scaleCEFR"),
        ("dreyfusExpert", "scaleDreyfus"),
        ("nihExpert", "scaleNIH"),
        ("assessedCompetent", "scaleAssessed"),
    ] {
        assert!(
            g.has(Some(&gmeow(level)), Some(&level_scale), Some(&gmeow(scale))),
            "{level} must carry gmeow:levelScale {scale}"
        );
    }
}

/// Twin of `test_no_primary_or_preferred_skill_term`.
///
/// Principle 9: no single slot wins — no primary/preferred skill selector may
/// exist as a property or class.
#[gmeow_test_batch_macros::batch_test]
fn no_primary_or_preferred_skill_term() {
    let g = GraphStore::ontology();
    for banned in [
        "primarySkill",
        "preferredSkill",
        "primaryCredential",
        "preferredCredential",
        "primaryOccupation",
        "preferredOccupation",
    ] {
        let node = gmeow(banned);
        for pt in PROP_TYPES {
            assert!(
                !g.has(Some(&node), Some(RDF_TYPE), Some(pt)),
                "{banned} must not exist as {pt}"
            );
        }
        assert!(
            !g.has(Some(&node), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{banned} must not exist as owl:Class"
        );
    }
}

/// Twin of `test_endorsement_uses_attestation`.
///
/// No new skill-endorsement mechanism beyond the existing Attestation relator;
/// the trust module's `endorses` stays scoped to agent-to-agent web-of-trust.
#[gmeow_test_batch_macros::batch_test]
fn endorsement_uses_attestation() {
    let g = GraphStore::ontology();
    assert!(
        g.has(Some(&gmeow("Attestation")), Some(RDF_TYPE), Some(OWL_CLASS)),
        "gmeow:Attestation must be an owl:Class"
    );
    let endorses = gmeow("endorses");
    assert!(
        g.has(Some(&endorses), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
        "gmeow:endorses must be an owl:ObjectProperty"
    );
    assert!(
        g.has(Some(&endorses), Some(RDFS_DOMAIN), Some(&gmeow("Agent"))),
        "gmeow:endorses must have rdfs:domain gmeow:Agent"
    );
    for banned in ["endorsesSkill", "skillEndorsement", "skillEndorsedBy"] {
        let node = gmeow(banned);
        for pt in PROP_TYPES {
            assert!(
                !g.has(Some(&node), Some(RDF_TYPE), Some(pt)),
                "{banned} must not exist as {pt}"
            );
        }
    }
}
