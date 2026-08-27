// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_software.py
//!
//! All fixture-only and whole-ontology graph-membership assertions are migrated
//! here; the structural TBox cells live in slices/extensions/software/tests/structural.ttl.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::TermValue;
use std::collections::BTreeSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://example.org/software/";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";

const FACETS: [&str; 6] = [
    "Project",
    "SoftwareProduct",
    "SourceTree",
    "Repository",
    "Commit",
    "Release",
];

// ── SHACL conformance cases ───────────────────────────────────────────────────

#[batch_cases]
#[case::facet_orthogonality_shacl_rejects_two_facets(
    Case::inline(format!(
        "@prefix gmeow: <{GMEOW}> .\n\
         @prefix ex:    <{EX}> .\n\
         @prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         ex:x rdf:type gmeow:Project .\n\
         ex:x rdf:type gmeow:SoftwareProduct .\n"
    ))
        .fails()
        .violations(&["may fill at most one of these mutually disjoint classes"])
)]
#[case::fixture_parses_and_shacl_passes(Case::repo_path("tests/fixtures/software.ttl"))]
fn software(#[case] case: Case) {
    case.run();
}

// ── Five-facet orthogonality guard (Principle 9) ──────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn no_subclass_bridge_between_facets() {
    let g = GraphStore::ontology();
    for (i, a) in FACETS.iter().enumerate() {
        for b in &FACETS[i + 1..] {
            let a_iri = format!("{GMEOW}{a}");
            let b_iri = format!("{GMEOW}{b}");
            assert!(
                !g.has(Some(&a_iri), Some(RDFS_SUBCLASS_OF), Some(&b_iri)),
                "{a} must not be a subclass of {b}"
            );
            assert!(
                !g.has(Some(&b_iri), Some(RDFS_SUBCLASS_OF), Some(&a_iri)),
                "{b} must not be a subclass of {a}"
            );
            assert!(
                !g.has(Some(&a_iri), Some(OWL_EQUIVALENT_CLASS), Some(&b_iri)),
                "{a} must not be equivalent to {b}"
            );
            assert!(
                !g.has(Some(&b_iri), Some(OWL_EQUIVALENT_CLASS), Some(&a_iri)),
                "{b} must not be equivalent to {a}"
            );
        }
    }
}

// ── Fixture: MeowGraph ────────────────────────────────────────────────────────

fn fixture_store() -> GraphStore {
    GraphStore::parse_ttl_file(&repo_root().join("tests/fixtures/software.ttl"))
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_all_five_facets() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}meowgraph")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}SoftwareProject"))
    ));
    assert!(g.has(
        Some(&format!("{EX}meowgraphProduct")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}SoftwareProduct"))
    ));
    assert!(g.has(
        Some(&format!("{EX}treeInitial")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}SourceTree"))
    ));
    assert!(g.has(
        Some(&format!("{EX}repo")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Repository"))
    ));
    assert!(g.has(
        Some(&format!("{EX}commitInitial")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Commit"))
    ));
    assert!(g.has(
        Some(&format!("{EX}v1_0_0")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Release"))
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_commit_has_content_digest() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}commitInitial")),
        Some(&format!("{GMEOW}contentDigest")),
        None,
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_ai_contributor_is_first_class() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}copilot")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}SoftwareAgent")),
    ));
    let contributions: BTreeSet<String> =
        g.subjects(&format!("{GMEOW}contributor"), &format!("{EX}copilot"));
    assert!(!contributions.is_empty(), "copilot must have contributions");
    for contrib in contributions {
        assert!(
            g.has(
                Some(&contrib),
                Some(&format!("{GMEOW}contributionRole")),
                Some(&format!("{GMEOW}roleAIAssistant"))
            ),
            "{contrib} must carry roleAIAssistant"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_contribution_reifies_role_and_degree() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}contribAlice")),
        Some(&format!("{GMEOW}contributionRole")),
        Some(&format!("{GMEOW}roleSoftwareMaintainer")),
    ));
    assert!(g.has(
        Some(&format!("{EX}contribAlice")),
        Some(&format!("{GMEOW}contributionDegree")),
        Some(&format!("{GMEOW}degreeLead")),
    ));
    assert!(g.has(
        Some(&format!("{EX}contribAlice")),
        Some(&format!("{GMEOW}contributionTarget")),
        Some(&format!("{EX}meowgraphProduct")),
    ));
}

// ── Software-specific seeds (dynamic subset sweeps) ───────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn software_contribution_roles_seeded() {
    let g = GraphStore::ontology();
    let expected: BTreeSet<String> = [
        "roleSoftwareMaintainer",
        "roleSoftwareDeveloper",
        "roleCodeReviewer",
        "roleReleaser",
        "roleSecurityContact",
        "roleBotContributor",
        "roleAIAssistant",
    ]
    .into_iter()
    .map(|r| format!("{GMEOW}{r}"))
    .collect();
    let actual: BTreeSet<String> = g.subjects_of_type(&format!("{GMEOW}ContributionRole"));
    let missing: Vec<_> = expected.difference(&actual).cloned().collect();
    assert!(
        missing.is_empty(),
        "Missing software ContributionRole seeds: {missing:?}"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn software_event_types_seeded() {
    let g = GraphStore::ontology();
    let expected: BTreeSet<String> = [
        "eventTypeCommit",
        "eventTypeRelease",
        "eventTypePush",
        "eventTypeMerge",
        "eventTypeCodeReview",
    ]
    .into_iter()
    .map(|e| format!("{GMEOW}{e}"))
    .collect();
    let actual: BTreeSet<String> = g.subjects_of_type(&format!("{GMEOW}EventType"));
    let missing: Vec<_> = expected.difference(&actual).cloned().collect();
    assert!(
        missing.is_empty(),
        "Missing software EventType seeds: {missing:?}"
    );
}

// ── Fixture: MeowGraph Phase B — 3-commit DAG, blobs, tree entries, events ────

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_three_commit_dag() {
    let g = fixture_store();
    assert!(!g.has(
        Some(&format!("{EX}commitInitial")),
        Some(&format!("{GMEOW}parentCommit")),
        None,
    ));
    assert!(g.has(
        Some(&format!("{EX}commitFeature")),
        Some(&format!("{GMEOW}parentCommit")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitMerge")),
        Some(&format!("{GMEOW}parentCommit")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitMerge")),
        Some(&format!("{GMEOW}parentCommit")),
        Some(&format!("{EX}commitFeature")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_commit_ancestor_closure() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}commitFeature")),
        Some(&format!("{GMEOW}commitAncestor")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitMerge")),
        Some(&format!("{GMEOW}commitAncestor")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}commitMerge")),
        Some(&format!("{GMEOW}commitAncestor")),
        Some(&format!("{EX}commitFeature")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_blobs_and_tree_entries() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}readmeBlob")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Blob"))
    ));
    assert!(g.has(
        Some(&format!("{EX}mainPyBlob")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Blob"))
    ));
    assert!(g.has(
        Some(&format!("{EX}readmeEntry")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}TreeEntry"))
    ));
    assert!(g.has(
        Some(&format!("{EX}readmeEntry")),
        Some(&format!("{GMEOW}treeEntryName")),
        None,
    ));
    assert!(g.has(
        Some(&format!("{EX}readmeEntry")),
        Some(&format!("{GMEOW}treeEntryMode")),
        None,
    ));
    assert!(g.has(
        Some(&format!("{EX}readmeEntry")),
        Some(&format!("{GMEOW}treeEntryObject")),
        Some(&format!("{EX}readmeBlob")),
    ));
    assert!(g.has(
        Some(&format!("{EX}mainPyEntry")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}TreeEntry"))
    ));
    assert!(g.has(
        Some(&format!("{EX}mainPyEntry")),
        Some(&format!("{GMEOW}treeEntryObject")),
        Some(&format!("{EX}mainPyBlob")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_push_event() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}pushFeature")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Push")),
    ));
    assert!(g.has(
        Some(&format!("{EX}pushFeature")),
        Some(&format!("{GMEOW}pushTarget")),
        Some(&format!("{EX}repo")),
    ));
    assert!(g.has(
        Some(&format!("{EX}pushFeature")),
        Some(&format!("{GMEOW}eventType")),
        Some(&format!("{GMEOW}eventTypePush")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_merge_event() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}mergeFeature")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Merge")),
    ));
    assert!(g.has(
        Some(&format!("{EX}mergeFeature")),
        Some(&format!("{GMEOW}mergeBase")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}mergeFeature")),
        Some(&format!("{GMEOW}mergeSource")),
        Some(&format!("{EX}featureBranch")),
    ));
    assert!(g.has(
        Some(&format!("{EX}mergeFeature")),
        Some(&format!("{GMEOW}mergeTarget")),
        Some(&format!("{EX}mainBranch")),
    ));
    assert!(g.has(
        Some(&format!("{EX}mergeFeature")),
        Some(&format!("{GMEOW}eventType")),
        Some(&format!("{GMEOW}eventTypeMerge")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_code_review_event() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}reviewFeature")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}CodeReview")),
    ));
    assert!(g.has(
        Some(&format!("{EX}reviewFeature")),
        Some(&format!("{GMEOW}reviewOf")),
        Some(&format!("{EX}mrFeature")),
    ));
    assert!(g.has(
        Some(&format!("{EX}reviewFeature")),
        Some(&format!("{GMEOW}reviewCommit")),
        Some(&format!("{EX}commitFeature")),
    ));
    assert!(g.has(
        Some(&format!("{EX}reviewFeature")),
        Some(&format!("{GMEOW}eventType")),
        Some(&format!("{GMEOW}eventTypeCodeReview")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_has_diff() {
    let g = fixture_store();
    assert!(g.has(
        Some(&format!("{EX}diffInitialFeature")),
        Some(RDF_TYPE),
        Some(&format!("{GMEOW}Diff")),
    ));
    assert!(g.has(
        Some(&format!("{EX}diffInitialFeature")),
        Some(&format!("{GMEOW}diffFrom")),
        Some(&format!("{EX}commitInitial")),
    ));
    assert!(g.has(
        Some(&format!("{EX}diffInitialFeature")),
        Some(&format!("{GMEOW}diffTo")),
        Some(&format!("{EX}commitFeature")),
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn fixture_repository_has_materialization_depth() {
    let g = fixture_store();
    let (vars, rows) = g.select(
        &[],
        &format!(
            "PREFIX ex: <{EX}>\n\
         PREFIX gmeow: <{GMEOW}>\n\
         PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\
         SELECT ?v WHERE {{ ex:repo gmeow:materializationDepth ?v }}"
        ),
    );
    assert_eq!(
        rows.len(),
        1,
        "repo must have exactly one materializationDepth"
    );
    let v_idx = vars.iter().position(|v| v == "v").expect("?v projected");
    let term = rows[0][v_idx].as_ref().expect("?v bound");
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            ..
        } => {
            assert_eq!(lexical_form, "2");
            assert_eq!(
                datatype,
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
            );
        }
        other => panic!("expected xsd:nonNegativeInteger literal, got {other:?}"),
    }
}
