// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_standpoint.py
//!
//! The standpoint / contested-claims facility: a contested fact is several
//! standpoint-indexed claims that COEXIST, none privileged. This file carries
//! both the earlier `run_shacl` fixture cases (batched parameter rows) and
//! the remaining `test_standpoint.py` guards, ported to native `#[test]` fns.
//!
//! Structural TBox facts run over `GraphStore::ontology()` (the native twin of
//! `load_merged_graph(include_imports=False)`); projection facts run each
//! generated `standpoint-*.rq` CONSTRUCT over its coverage fixture via
//! `GraphStore::construct`; SSSOM rows are read from `generated/mappings/`.
//!
//! Python-fn -> Rust-fn mapping:
//! - `test_three_axes_are_orthogonal` -> `three_axes_are_orthogonal`
//! - `test_vantage_semantically_subsumes_according_to` -> `vantage_semantically_subsumes_according_to`
//! - `test_vantage_recognises_observer_as_standpoint` -> `vantage_recognises_observer_as_standpoint`
//! - `test_according_to_references_vantage_as_reified_counterpart` -> `according_to_references_vantage_as_reified_counterpart`
//! - `test_no_preferred_or_primary_term_is_declared` -> `no_preferred_or_primary_term_is_declared`
//! - `test_contested_places_cannot_force_inconsistency` -> `contested_places_cannot_force_inconsistency`
//! - `test_no_frame_collapsing_projection_exists` -> `no_frame_collapsing_projection_exists`
//! - `test_standpoint_owl2_projection_emits_tool_compatible_labels` -> `standpoint_owl2_projection_emits_tool_compatible_labels`
//! - `test_crminf_projection_is_at_least_as_expressive` -> `crminf_projection_is_at_least_as_expressive`
//! - `test_prov_projection_attributes_every_standpoint` -> `prov_projection_attributes_every_standpoint`
//! - `test_oa_projection_annotates_each_claim` -> `oa_projection_annotates_each_claim`
//! - `test_schema_projection_emits_per_standpoint_claims` -> `schema_projection_emits_per_standpoint_claims`
//! - `test_standpoint_tenure_generates_claim_restriction` -> `standpoint_tenure_generates_claim_restriction`
//! - `test_standpoint_crminf_projection_from_standpoint_claim_reified` -> `standpoint_crminf_projection_from_standpoint_claim_reified`
//! - `test_standpoint_crminf_projection_from_standpoint_claim_entity` -> `standpoint_crminf_projection_from_standpoint_claim_entity`
//! - `test_standpoint_schema_projection_from_standpoint_claim_entity` -> `standpoint_schema_projection_from_standpoint_claim_entity`
//! - `test_bbc_projection_exists` -> `bbc_projection_exists`
//! - `test_bbc_projection_emits_news_event` -> `bbc_projection_emits_news_event`
//! - `test_standpoint_claim_maps_to_crminf_i5` -> `standpoint_claim_maps_to_crminf_i5`
//! - `test_standpoint_claim_maps_to_iao_assertion` -> `standpoint_claim_maps_to_iao_assertion`
//! - `test_standpoint_claim_maps_to_oa_annotation` -> `standpoint_claim_maps_to_oa_annotation`
//! - `test_standpoint_maps_to_iptc_assertor` -> `standpoint_maps_to_iptc_assertor`
//! - `test_claim_modality_maps_to_sosa_has_result` -> `claim_modality_maps_to_sosa_has_result`

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::slice::rdf_query::{Object, Subject};

// ── SHACL fixture cases (earlier `run_shacl` twins, retained) ─────────────────

#[batch_cases]
#[case::coexistence_fixture_conforms(Case::file("shapes", "standpoint-coexistence"))]
#[case::preferred_claim_is_flagged(
    Case::file("shapes", "standpoint-preferred-violation")
        .fails()
        .violations(&["preferred/primary"])
)]
#[case::withdrawn_standpoint_warning_does_not_fail(
    Case::file("shapes", "standpoint-withdrawn-warning")
        .warnings(&["displayable false"])
)]
#[case::variety_coexistence_fixture_conforms(Case::file("shapes", "variety-coexistence"))]
#[case::etymology_coexistence_fixture_conforms(Case::file("shapes", "etymology-coexistence"))]
#[case::credence_band_and_decimal_consistent_conforms(Case::file(
    "shapes",
    "standpoint-credence-consistent"
))]
#[case::credence_band_decimal_mismatch_is_flagged(
    Case::file("shapes", "standpoint-credence-band-violation")
        .fails()
        .violations(&["band and decimal are inconsistent"])
)]
#[case::credence_decimal_out_of_range_is_flagged(
    Case::file("shapes", "standpoint-credence-range-violation")
        .fails()
        .violations(&["in [0.0, 1.0]"])
)]
fn standpoint(#[case] case: Case) {
    case.run();
}

// ── Namespaces + IRI constants ────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_SP: &str = "https://blackcatinformatics.ca/gmeow/examples/standpoint/";
const EX_TEST: &str = "https://example.org/test/";

const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const SKOS_SCOPE_NOTE: &str = "http://www.w3.org/2004/02/skos/core#scopeNote";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

// Projection-target predicates / classes.
const STANDPOINT_LABEL: &str = "https://blackcatinformatics.ca/gmeow#standpointLabel";
const CRMINF: &str = "http://www.ics.forth.gr/isl/CRMinf/";
const CRM: &str = "http://www.cidoc-crm.org/cidoc-crm/";
const PROV: &str = "http://www.w3.org/ns/prov#";
const OA: &str = "http://www.w3.org/ns/oa#";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const SCHEMA: &str = "https://schema.org/";
const BBC: &str = "http://www.bbc.co.uk/ontologies/news/";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// ── Local helpers ─────────────────────────────────────────────────────────────

// `read_query` is now the shared `conformance_support::read_query` (imported via
// the glob `use conformance_support::*;`): a bare `standpoint-*.rq` name resolves
// against `generated/queries/` exactly as this file's former local copy did.

/// The literal string objects of `<subject> <pred> ?o` (bnode-aware; drops IRIs).
fn literal_objects(g: &GraphStore, subject: &str, pred: &str) -> Vec<String> {
    g.objects_h(&Subject::Named(subject.to_owned()), pred)
        .into_iter()
        .filter_map(|o| match o {
            Object::Literal { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

/// True iff some SSSOM row in `generated/mappings/*.sssom.tsv` has this
/// `subject_id` (col 1) and `object_id` (col 3). Mirrors `load_mappings()` which
/// scans every file, skipping `#`-prefixed YAML metadata lines.
fn sssom_row_exists(subject_id: &str, object_id: &str) -> bool {
    for (name, bytes) in generated_mappings() {
        if !name.ends_with(".sssom.tsv") {
            continue;
        }
        let text = std::str::from_utf8(bytes)
            .unwrap_or_else(|error| panic!("authenticated mapping {name} is not UTF-8: {error}"));
        for line in text.lines() {
            if line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() >= 3 && cols[0] == subject_id && cols[2] == object_id {
                return true;
            }
        }
    }
    false
}

/// The standpoint coverage fixture (`ex:ax-*` annotated-axiom form) as a store.
fn standpoint_fixture() -> GraphStore {
    GraphStore::parse_ttl_file(&repo_root().join("tests/fixtures/coverage/standpoint.ttl"))
}

// ── Term-level structure (the merged ontology graph) ──────────────────────────

/// Twin of `test_three_axes_are_orthogonal`: standpoint ⟂ source ⟂ confidence —
/// no subPropertyOf / equivalentProperty bridge among the three axes.
#[gmeow_test_batch_macros::batch_test]
fn three_axes_are_orthogonal() {
    let g = GraphStore::ontology();
    let axes = [
        gmeow("accordingTo"),
        gmeow("wasAttributedTo"),
        gmeow("confidence"),
    ];
    for i in 0..axes.len() {
        for j in (i + 1)..axes.len() {
            let a = &axes[i];
            let b = &axes[j];
            assert!(!g.has(Some(a), Some(RDFS_SUB_PROPERTY_OF), Some(b)));
            assert!(!g.has(Some(b), Some(RDFS_SUB_PROPERTY_OF), Some(a)));
            assert!(!g.has(Some(a), Some(OWL_EQUIVALENT_PROPERTY), Some(b)));
            assert!(!g.has(Some(b), Some(OWL_EQUIVALENT_PROPERTY), Some(a)));
        }
    }
}

/// Twin of `test_vantage_semantically_subsumes_according_to`.
#[gmeow_test_batch_macros::batch_test]
fn vantage_semantically_subsumes_according_to() {
    let g = GraphStore::ontology();
    let notes = literal_objects(&g, &gmeow("vantage"), SKOS_SCOPE_NOTE);
    assert!(!notes.is_empty(), "vantage must carry a skos:scopeNote");
    assert!(
        notes
            .iter()
            .any(|t| t.contains("gmeow:vantage ⊑ gmeow:accordingTo")),
        "vantage scopeNote must document the semantic subsumption: {notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|t| t.contains("not axiomatised") || t.contains("not axiomatized")),
        "vantage scopeNote must note the subsumption is not axiomatised: {notes:?}"
    );
}

/// Twin of `test_vantage_recognises_observer_as_standpoint`.
#[gmeow_test_batch_macros::batch_test]
fn vantage_recognises_observer_as_standpoint() {
    let g = GraphStore::ontology();
    let defs = literal_objects(&g, &gmeow("vantage"), SKOS_DEFINITION);
    assert!(
        defs.iter()
            .any(|t| t.contains("observer") && t.contains("sensor") && t.contains("perceiver")),
        "vantage definition must name observer/sensor/perceiver: {defs:?}"
    );
    assert!(
        defs.iter()
            .any(|t| t.contains("IS a standpoint") || t.contains("is a standpoint")),
        "vantage definition must assert the observer-as-standpoint doctrine: {defs:?}"
    );
}

/// Twin of `test_according_to_references_vantage_as_reified_counterpart`.
#[gmeow_test_batch_macros::batch_test]
fn according_to_references_vantage_as_reified_counterpart() {
    let g = GraphStore::ontology();
    let defs = literal_objects(&g, &gmeow("accordingTo"), SKOS_DEFINITION);
    assert!(
        defs.iter().any(|t| t.contains("vantage")),
        "accordingTo definition must reference vantage: {defs:?}"
    );
    assert!(
        defs.iter()
            .any(|t| t.contains("accordingTo becomes the gmeow:vantage")),
        "accordingTo definition must document the promotion path: {defs:?}"
    );
}

/// Twin of `test_no_preferred_or_primary_term_is_declared`: no gmeow: term local
/// name (no nested `/`) may begin with `primary`/`preferred` — no slot to win.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_term_is_declared() {
    let g = GraphStore::ontology();
    let (_, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let mut offenders: Vec<String> = Vec::new();
    for row in rows {
        let Some(Some(purrdf::TermValue::Iri(iri))) = row.into_iter().next() else {
            continue;
        };
        let Some(local) = iri.strip_prefix(GMEOW) else {
            continue;
        };
        let lower = local.to_lowercase();
        if !local.contains('/') && (lower.starts_with("primary") || lower.starts_with("preferred"))
        {
            offenders.push(iri);
        }
    }
    assert!(
        offenders.is_empty(),
        "preferred/primary terms must not exist: {offenders:?}"
    );
}

/// Twin of `test_contested_places_cannot_force_inconsistency`: coexistence is
/// reasoning-safe because containedInPlace is not functional and Place is not
/// self-disjoint.
#[gmeow_test_batch_macros::batch_test]
fn contested_places_cannot_force_inconsistency() {
    let g = GraphStore::ontology();
    assert!(!g.is_functional_carrier(&gmeow("containedInPlace")));
    assert!(g.has(Some(&gmeow("Place")), Some(RDF_TYPE), Some(OWL_CLASS)));
    assert!(!g.has(
        Some(&gmeow("Place")),
        Some(OWL_DISJOINT_WITH),
        Some(&gmeow("Place"))
    ));
}

// ── The standpoint projection — LOSSLESS only ─────────────────────────────────

/// Twin of `test_no_frame_collapsing_projection_exists`: the lossless OWL 2
/// projection ships; no winner-selecting `standpoint.rq` exists.
#[gmeow_test_batch_macros::batch_test]
fn no_frame_collapsing_projection_exists() {
    assert!(
        generated_queries().contains_key("standpoint-owl2.rq"),
        "lossless standpoint-owl2.rq projection must ship"
    );
    assert!(
        !generated_queries().contains_key("standpoint.rq"),
        "a frame-selecting projection picks a winner"
    );
}

/// Twin of `test_standpoint_owl2_projection_emits_tool_compatible_labels`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_owl2_projection_emits_tool_compatible_labels() {
    let out = standpoint_fixture().construct(&[], &read_query("standpoint-owl2.rq"));

    assert!(STANDPOINT_LABEL.ends_with("#standpointLabel")); // tool convention

    let ru_labels = literal_objects(&out, &format!("{EX_SP}ax-ru"), STANDPOINT_LABEL);
    assert!(
        ru_labels.iter().any(|l| l.contains("<Diamond>")),
        "RU claim (◊ conceivable) must be Diamond: {ru_labels:?}"
    );
    assert!(
        ru_labels.iter().any(|l| l.contains("standpoint-ru")),
        "RU label must carry the standpoint name: {ru_labels:?}"
    );
    let un_labels = literal_objects(&out, &format!("{EX_SP}ax-un"), STANDPOINT_LABEL);
    assert!(
        un_labels.iter().any(|l| l.contains("<Box>")),
        "UN claim (□ unequivocal) must be Box: {un_labels:?}"
    );
    // The base axiom is preserved alongside the standpoint label (lossless).
    assert!(out.has(
        Some(&format!("{EX_SP}crimea")),
        Some(&gmeow("containedInPlace")),
        Some(&format!("{EX_SP}russia"))
    ));
}

/// Twin of `test_crminf_projection_is_at_least_as_expressive`.
#[gmeow_test_batch_macros::batch_test]
fn crminf_projection_is_at_least_as_expressive() {
    let out = standpoint_fixture().construct(&[], &read_query("standpoint-crminf.rq"));

    assert!(out.has(
        None,
        Some(RDF_TYPE),
        Some(&format!("{CRMINF}I1_Argumentation"))
    ));
    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{CRMINF}I2_Belief"))));
    // Attributed to the standpoint actor.
    assert!(out.has(
        None,
        Some(&format!("{CRM}P14_carried_out_by")),
        Some(&format!("{EX_SP}standpoint-intl-law"))
    ));

    // Belief values span the space — the refuted claim holds the proposition FALSE.
    let j5 = format!("{CRMINF}J5_holds_to_be");
    for value in ["true", "possible", "false"] {
        assert!(
            out.ask(&[], &format!("ASK {{ ?s <{j5}> \"{value}\" }}")),
            "CRMinf belief values must include {value:?}"
        );
    }

    // The denied proposition is REFERRED TO, never asserted as a base fact.
    assert!(!out.has(
        Some(&format!("{EX_SP}crimea")),
        Some(&gmeow("containedInPlace")),
        Some(&format!("{EX_SP}russia"))
    ));
    assert!(out.has(
        None,
        Some(&format!("{CRM}P67_refers_to")),
        Some(&format!("{EX_SP}crimea"))
    ));
}

/// Twin of `test_prov_projection_attributes_every_standpoint`.
#[gmeow_test_batch_macros::batch_test]
fn prov_projection_attributes_every_standpoint() {
    let out = standpoint_fixture().construct(&[], &read_query("standpoint-prov.rq"));

    for sp in ["standpoint-ru", "standpoint-un", "standpoint-intl-law"] {
        assert!(
            out.has(
                None,
                Some(&format!("{PROV}wasAttributedTo")),
                Some(&format!("{EX_SP}{sp}"))
            ),
            "{sp} must be attributed"
        );
    }
    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{PROV}Attribution"))));
    // The proposition stays reified (owl:annotated*), never asserted.
    assert!(!out.has(
        Some(&format!("{EX_SP}crimea")),
        Some(&gmeow("containedInPlace")),
        Some(&format!("{EX_SP}russia"))
    ));
    assert!(out.has(
        None,
        Some(OWL_ANNOTATED_SOURCE),
        Some(&format!("{EX_SP}crimea"))
    ));
}

/// Twin of `test_oa_projection_annotates_each_claim`.
#[gmeow_test_batch_macros::batch_test]
fn oa_projection_annotates_each_claim() {
    let out = standpoint_fixture().construct(&[], &read_query("standpoint-oa.rq"));

    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{OA}Annotation"))));
    for sp in ["standpoint-ru", "standpoint-un", "standpoint-intl-law"] {
        assert!(
            out.has(
                None,
                Some(&format!("{DCTERMS}creator")),
                Some(&format!("{EX_SP}{sp}"))
            ),
            "{sp} must be an oa creator"
        );
    }
    assert!(out.has(
        None,
        Some(&format!("{OA}hasTarget")),
        Some(&format!("{EX_SP}crimea"))
    ));
    // Proposition kept reified, never asserted.
    assert!(!out.has(
        Some(&format!("{EX_SP}crimea")),
        Some(&gmeow("containedInPlace")),
        Some(&format!("{EX_SP}russia"))
    ));
}

/// Twin of `test_schema_projection_emits_per_standpoint_claims`.
#[gmeow_test_batch_macros::batch_test]
fn schema_projection_emits_per_standpoint_claims() {
    let out = standpoint_fixture().construct(&[], &read_query("standpoint-schema.rq"));

    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{SCHEMA}Claim"))));
    // The asserting standpoints appear; the denying one (refuted) is excluded.
    for sp in ["standpoint-ru", "standpoint-un"] {
        assert!(
            out.has(
                None,
                Some(&format!("{SCHEMA}author")),
                Some(&format!("{EX_SP}{sp}"))
            ),
            "{sp} must author a schema:Claim"
        );
    }
    assert!(
        !out.has(
            None,
            Some(&format!("{SCHEMA}author")),
            Some(&format!("{EX_SP}standpoint-intl-law"))
        ),
        "the refuted standpoint must not author a schema:Claim"
    );
    // No base triple asserted.
    assert!(!out.has(
        Some(&format!("{EX_SP}crimea")),
        Some(&gmeow("containedInPlace")),
        Some(&format!("{EX_SP}russia"))
    ));
}

// ── StandpointClaim as Observation specialization ─────────────────────────────

/// Twin of `test_standpoint_tenure_generates_claim_restriction`: StandpointTenure
/// has an EL restriction requiring at least one standpointClaim.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_tenure_generates_claim_restriction() {
    let g = GraphStore::ontology();
    let tenure = Subject::Named(gmeow("StandpointTenure"));
    let found = g
        .objects_h(&tenure, RDFS_SUBCLASS_OF)
        .iter()
        .filter_map(GraphStore::object_as_subject)
        .any(|restriction| {
            g.restriction_matches(
                &restriction,
                &gmeow("standpointClaim"),
                OWL_SOME_VALUES_FROM,
                &gmeow("StandpointClaim"),
            )
        });
    assert!(
        found,
        "StandpointTenure must have ∃ standpointClaim . StandpointClaim"
    );
}

// ── Projection competency tests — StandpointClaim individuals ──────────────────

/// Twin of `test_standpoint_crminf_projection_from_standpoint_claim_reified`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_crminf_projection_from_standpoint_claim_reified() {
    let fixture = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/coverage/standpoint-claim-reified.ttl"),
    );
    let out = fixture.construct(&[], &read_query("standpoint-crminf.rq"));

    assert!(out.has(
        None,
        Some(RDF_TYPE),
        Some(&format!("{CRMINF}I1_Argumentation"))
    ));
    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{CRMINF}I2_Belief"))));
    let j5 = format!("{CRMINF}J5_holds_to_be");
    for value in ["true", "possible", "false"] {
        assert!(
            out.ask(&[], &format!("ASK {{ ?s <{j5}> \"{value}\" }}")),
            "CRMinf belief values must include {value:?}"
        );
    }
}

/// Twin of `test_standpoint_crminf_projection_from_standpoint_claim_entity`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_crminf_projection_from_standpoint_claim_entity() {
    let fixture = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/coverage/standpoint-claim-entity.ttl"),
    );
    let out = fixture.construct(&[], &read_query("standpoint-crminf.rq"));

    assert!(out.has(
        None,
        Some(RDF_TYPE),
        Some(&format!("{CRMINF}I1_Argumentation"))
    ));
    assert!(out.has(
        None,
        Some(&format!("{CRM}P67_refers_to")),
        Some(&format!("{EX_TEST}place1"))
    ));
}

/// Twin of `test_standpoint_schema_projection_from_standpoint_claim_entity`:
/// Branch C renders the entity IRI as schema:text (a literal).
#[gmeow_test_batch_macros::batch_test]
fn standpoint_schema_projection_from_standpoint_claim_entity() {
    let fixture = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/coverage/standpoint-claim-entity.ttl"),
    );
    let out = fixture.construct(&[], &read_query("standpoint-schema.rq"));

    assert!(
        out.ask(
            &[],
            &format!("ASK {{ ?c <{SCHEMA}text> \"{EX_TEST}place1\" }}")
        ),
        "the entity IRI must render as schema:text"
    );
}

/// Twin of `test_bbc_projection_exists`.
#[gmeow_test_batch_macros::batch_test]
fn bbc_projection_exists() {
    assert!(
        generated_queries().contains_key("standpoint-bbc.rq"),
        "the BBC News Ontology projection query must ship"
    );
}

/// Twin of `test_bbc_projection_emits_news_event`.
#[gmeow_test_batch_macros::batch_test]
fn bbc_projection_emits_news_event() {
    let fixture =
        GraphStore::parse_ttl_file(&repo_root().join("tests/fixtures/coverage/standpoint-bbc.ttl"));
    let out = fixture.construct(&[], &read_query("standpoint-bbc.rq"));

    assert!(out.has(None, Some(RDF_TYPE), Some(&format!("{BBC}NewsEvent"))));
    assert!(out.has(
        None,
        Some(&format!("{BBC}about")),
        Some(&format!("{EX_TEST}event1"))
    ));
}

// ── Mapping alignment tests (SSSOM rows) ──────────────────────────────────────

/// Twin of `test_standpoint_claim_maps_to_crminf_i5`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_claim_maps_to_crminf_i5() {
    assert!(
        sssom_row_exists("gmeow:StandpointClaim", "crminf:I5_Inference_Making"),
        "StandpointClaim must map to crminf:I5_Inference_Making"
    );
}

/// Twin of `test_standpoint_claim_maps_to_iao_assertion`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_claim_maps_to_iao_assertion() {
    assert!(
        sssom_row_exists("gmeow:StandpointClaim", "iao:assertion"),
        "StandpointClaim must map to iao:assertion"
    );
}

/// Twin of `test_standpoint_claim_maps_to_oa_annotation`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_claim_maps_to_oa_annotation() {
    assert!(
        sssom_row_exists("gmeow:StandpointClaim", "oa:Annotation"),
        "StandpointClaim must map to oa:Annotation"
    );
}

/// Twin of `test_standpoint_maps_to_iptc_assertor`.
#[gmeow_test_batch_macros::batch_test]
fn standpoint_maps_to_iptc_assertor() {
    assert!(
        sssom_row_exists("gmeow:Standpoint", "iptc:Assertor"),
        "Standpoint must map to iptc:Assertor"
    );
}

/// Twin of `test_claim_modality_maps_to_sosa_has_result`.
#[gmeow_test_batch_macros::batch_test]
fn claim_modality_maps_to_sosa_has_result() {
    assert!(
        sssom_row_exists("gmeow:claimModality", "sosa:hasResult"),
        "claimModality must map to sosa:hasResult"
    );
}
