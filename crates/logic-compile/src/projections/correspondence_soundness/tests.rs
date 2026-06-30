// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native correspondence-soundness pass.
//!
//! Ported from the retired alignment-direction and FnO back-end lint `#[cfg(test)]`
//! blocks, re-seated on the oxigraph-free `DslView`. Each synthetic check (self-contra
//! inverse, self-inverse, domain-range, property-character, dc-refinement, dc-hand-authored,
//! Principle-5 equivalence-collapse, fno-type, fno-ref) keeps the same fixture content and
//! the same assertion so the new home carries identical coverage.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_rdf::{parse_dataset, NativeRdfFormat, RdfDataset};

use super::*;

/// Parse Turtle text into a frozen dataset (the native lenient codec, so `@x-gmeow-*`
/// tags parse). The pipeline edge does this from file bytes; tests do it from literals.
fn ds(ttl: &str) -> Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), NativeRdfFormat::Turtle.media_type(), None)
        .expect("parse fixture turtle")
}

fn mapping(subject: &str, predicate: &str, object: &str, confidence: &str) -> Mapping {
    Mapping {
        subject_id: subject.to_owned(),
        predicate_id: predicate.to_owned(),
        object_id: object.to_owned(),
        confidence: confidence.to_owned(),
        mapping_justification: "semapv:ManualMappingCuration".to_owned(),
    }
}

/// Build a `SoundnessInputs` over the given views and run the five alignment checks.
fn run_alignment(
    onto: &DslView<'_>,
    targets: &BTreeMap<String, DslView<'_>>,
    mappings: &[Mapping],
) -> Vec<ProjectionDiagnostic> {
    let empty_net: BTreeMap<String, String> = BTreeMap::new();
    let no_edoal: Vec<(String, DslView<'_>)> = Vec::new();
    let fno_ds = ds("");
    let fno_view = DslView::new(&fno_ds);
    let inputs = SoundnessInputs {
        ontology: onto,
        target_graphs: targets,
        network_failed: &empty_net,
        mappings,
        fno: &fno_view,
        edoal: &no_edoal,
    };
    lint_alignment_directions(&inputs)
}

#[test]
fn strong_property_predicates_contain_expected_curies() {
    assert!(STRONG_PROPERTY_PREDICATES.contains(&"owl:equivalentProperty"));
    assert!(STRONG_PROPERTY_PREDICATES.contains(&"skos:exactMatch"));
}

#[test]
fn dcterms_refinements_and_grandfathered_dc_are_present() {
    assert!(DCTERMS_REFINEMENTS
        .iter()
        .any(|(r, b)| *r == "dcterms:abstract" && *b == "dcterms:description"));
    assert!(GRANDFATHERED_DC.contains(&"dc:rights"));
}

#[test]
fn alignment_target_prefixes_are_expandable() {
    for prefix in ALIGNMENT_TARGETS {
        let canonical = TARGET_PREFIX_ALIASES
            .iter()
            .find(|(alias, _)| alias == prefix)
            .map(|(_, canonical)| *canonical)
            .unwrap_or(prefix);
        assert!(
            registry_iri(canonical).is_some(),
            "ALIGNMENT_TARGETS contains `{prefix}` (canonical `{canonical}`) but the registry cannot expand it"
        );
    }
}

/// A property mapped to both a term and its inverse is flagged as an ERROR.
#[test]
fn detects_self_contradicting_inverse_mapping() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:subOrganizationOf a owl:ObjectProperty .\n");
    let schema_ds = ds("@prefix schema: <https://schema.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         schema:subOrganization a owl:ObjectProperty ;\n\
         \towl:inverseOf schema:parentOrganization .\n\
         schema:parentOrganization a owl:ObjectProperty ;\n\
         \towl:inverseOf schema:subOrganization .\n");
    let onto = DslView::new(&onto_ds);
    let mut targets = BTreeMap::new();
    targets.insert("schema".to_owned(), DslView::new(&schema_ds));

    let mappings = vec![
        mapping(
            "gmeow:subOrganizationOf",
            "owl:equivalentProperty",
            "schema:parentOrganization",
            "0.9",
        ),
        mapping(
            "gmeow:subOrganizationOf",
            "skos:closeMatch",
            "schema:subOrganization",
            "0.6",
        ),
    ];

    let findings = run_alignment(&onto, &targets, &mappings);
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == "ERROR" && f.check == "inverse-direction")
        .collect();
    assert!(!errors.is_empty(), "self-contradicting inverse not flagged");
    let flagged = errors[0];
    assert_eq!(
        flagged.instance.as_deref(),
        Some("https://schema.org/subOrganization")
    );
    assert!(flagged.message.contains("schema:parentOrganization"));
    assert!(flagged
        .message
        .contains("did you mean schema:parentOrganization?"));
}

/// A symmetric target (T owl:inverseOf T) must not self-contradict.
#[test]
fn self_inverse_target_is_not_flagged() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:hasMet a owl:ObjectProperty .\n");
    let foaf_ds = ds("@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         foaf:knows a owl:ObjectProperty ; owl:inverseOf foaf:knows .\n");
    let onto = DslView::new(&onto_ds);
    let mut targets = BTreeMap::new();
    targets.insert("foaf".to_owned(), DslView::new(&foaf_ds));

    let mappings = vec![mapping(
        "gmeow:hasMet",
        "skos:closeMatch",
        "foaf:knows",
        "0.8",
    )];
    let findings = run_alignment(&onto, &targets, &mappings);
    let inverse: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "inverse-direction")
        .collect();
    assert!(
        inverse.is_empty(),
        "self-inverse wrongly flagged: {inverse:?}"
    );
}

/// Domain/range synthetic: inverted is flagged WARNING, compatible is clean, an
/// unavailable target prefix produces an INFO.
#[test]
fn domain_range_synthetic() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix schema: <https://schema.org/> .\n\
         gmeow:Child a owl:Class ; owl:equivalentClass schema:Child .\n\
         gmeow:Parent a owl:Class ; owl:equivalentClass schema:Parent .\n\
         gmeow:childOf a owl:ObjectProperty ;\n\
         \trdfs:domain gmeow:Child ;\n\
         \trdfs:range gmeow:Parent .\n");
    let schema_ds = ds("@prefix schema: <https://schema.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         schema:Child a owl:Class .\n\
         schema:Parent a owl:Class .\n\
         schema:childOf a owl:ObjectProperty ;\n\
         \trdfs:domain schema:Child ;\n\
         \trdfs:range schema:Parent .\n\
         schema:parentOf a owl:ObjectProperty ;\n\
         \trdfs:domain schema:Parent ;\n\
         \trdfs:range schema:Child .\n");
    let onto = DslView::new(&onto_ds);
    let mut targets = BTreeMap::new();
    targets.insert("schema".to_owned(), DslView::new(&schema_ds));

    let mappings = vec![
        mapping("gmeow:childOf", "skos:closeMatch", "schema:childOf", "0.6"),
        mapping("gmeow:childOf", "skos:closeMatch", "schema:parentOf", "0.6"),
        mapping("gmeow:childOf", "skos:closeMatch", "foaf:noSuchTerm", "0.6"),
    ];
    let findings = run_alignment(&onto, &targets, &mappings);

    let compatible = findings
        .iter()
        .any(|f| f.instance.as_deref() == Some("https://schema.org/childOf"));
    assert!(!compatible, "compatible mapping should not be flagged");

    let inverted: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.instance.as_deref() == Some("https://schema.org/parentOf")
                && f.check == "domain-range"
        })
        .collect();
    assert_eq!(
        inverted.len(),
        1,
        "expected one inverted domain-range finding"
    );
    assert_eq!(inverted[0].severity, "WARNING");
    assert!(inverted[0]
        .message
        .contains("domain/range are inverted relative to the target term"));

    let unavailable: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.instance.as_deref() == Some("http://xmlns.com/foaf/0.1/noSuchTerm")
                && f.check == "domain-range"
                && f.severity == "INFO"
        })
        .collect();
    assert!(
        !unavailable.is_empty(),
        "expected an INFO for unavailable axioms"
    );
    assert!(unavailable[0]
        .message
        .contains("no axioms available for target 'foaf'"));
}

/// Property-character: object-vs-datatype conflict is ERROR, characteristic mismatch is
/// WARNING, and a schema.org-like target with no OWL characteristics is skipped.
#[test]
fn property_character_mismatches_and_skips() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:dataProp a owl:DatatypeProperty .\n\
         gmeow:funcProp a owl:ObjectProperty, owl:FunctionalProperty .\n\
         gmeow:plainProp a owl:ObjectProperty .\n");
    let foaf_ds = ds("@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         foaf:objProp a owl:ObjectProperty .\n\
         foaf:plainObj a owl:ObjectProperty .\n");
    let schema_ds = ds("@prefix schema: <https://schema.org/> .\n\
         schema:someProp a schema:Property .\n");
    let onto = DslView::new(&onto_ds);
    let mut targets = BTreeMap::new();
    targets.insert("foaf".to_owned(), DslView::new(&foaf_ds));
    targets.insert("schema".to_owned(), DslView::new(&schema_ds));

    let mappings = vec![
        mapping(
            "gmeow:dataProp",
            "owl:equivalentProperty",
            "foaf:objProp",
            "0.9",
        ),
        mapping(
            "gmeow:funcProp",
            "owl:equivalentProperty",
            "foaf:plainObj",
            "0.9",
        ),
        mapping(
            "gmeow:plainProp",
            "skos:exactMatch",
            "schema:someProp",
            "0.9",
        ),
    ];
    let findings = run_alignment(&onto, &targets, &mappings);

    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "property-character" && f.severity == "ERROR")
        .collect();
    assert_eq!(errors.len(), 1, "expected one property-character ERROR");
    assert_eq!(
        errors[0].instance.as_deref(),
        Some("http://xmlns.com/foaf/0.1/objProp")
    );
    assert!(errors[0]
        .message
        .contains("GMEOW datatype property vs target object property"));

    let warnings: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "property-character" && f.severity == "WARNING")
        .collect();
    assert_eq!(warnings.len(), 1, "expected one property-character WARNING");
    assert_eq!(
        warnings[0].instance.as_deref(),
        Some("http://xmlns.com/foaf/0.1/plainObj")
    );
    assert!(warnings[0]
        .message
        .contains("GMEOW declares FunctionalProperty but the target does not"));

    let schema_character: Vec<_> = findings
        .iter()
        .filter(|f| {
            f.check == "property-character"
                && f.instance.as_deref() == Some("https://schema.org/someProp")
        })
        .collect();
    assert!(
        schema_character.is_empty(),
        "schema.org-like target with no OWL characteristics should not be flagged"
    );
}

/// A dcterms refinement aligned without its broader element is a WARNING.
#[test]
fn dc_refinement_flags_missing_broader() {
    let mappings = vec![mapping(
        "gmeow:abstract",
        "skos:closeMatch",
        "dcterms:abstract",
        "0.9",
    )];
    let findings = lint_dc_refinement(&mappings);
    let refined: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "dc-refinement")
        .collect();
    assert_eq!(refined.len(), 1, "expected one dc-refinement WARNING");
    assert_eq!(refined[0].severity, "WARNING");
    assert!(refined[0].message.contains("dcterms:abstract"));
    assert!(refined[0].message.contains("dcterms:description"));
    assert_eq!(
        refined[0].instance.as_deref(),
        Some("http://purl.org/dc/terms/description")
    );
}

/// A hand-authored dc: alignment (other than the grandfathered dc:rights) is a WARNING.
#[test]
fn dc_hand_authored_flagged() {
    let mappings = vec![
        mapping("gmeow:rights", "skos:closeMatch", "dc:rights", "0.9"),
        mapping("gmeow:creator", "skos:closeMatch", "dc:creator", "0.9"),
    ];
    let findings = lint_dc_refinement(&mappings);
    let hand: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "dc-hand-authored")
        .collect();
    assert_eq!(hand.len(), 1, "expected one dc-hand-authored WARNING");
    assert_eq!(hand[0].severity, "WARNING");
    assert_eq!(
        hand[0].instance.as_deref(),
        Some("http://purl.org/dc/elements/1.1/creator")
    );
    assert!(hand[0].message.contains("dc:creator is hand-authored"));
}

/// PRINCIPLE 5 RED TEST: a strong-equivalence chain that connects two disjoint classes
/// is a hard ERROR. This is the sole native enforcer of Constitution Principle 5.
#[test]
fn equivalence_collapse_detects_disjoint_class_chain() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:A a owl:Class .\n\
         gmeow:B a owl:Class .\n\
         gmeow:A owl:disjointWith gmeow:B .\n");
    let onto = DslView::new(&onto_ds);
    let targets: BTreeMap<String, DslView<'_>> = BTreeMap::new();

    let mappings = vec![
        mapping("gmeow:A", "skos:exactMatch", "schema:Intermediate", "0.9"),
        mapping(
            "gmeow:B",
            "owl:equivalentClass",
            "schema:Intermediate",
            "0.9",
        ),
    ];
    let findings = run_alignment(&onto, &targets, &mappings);
    let collapsed: Vec<_> = findings
        .iter()
        .filter(|f| f.check == "equivalence-collapse")
        .collect();
    assert!(
        !collapsed.is_empty(),
        "equivalence collapse not flagged (Principle 5 breach)"
    );
    let flagged = collapsed[0];
    assert_eq!(flagged.severity, "ERROR");
    assert!(flagged.message.contains("Principle 5"));
    assert!(flagged.message.contains("schema:Intermediate"));
    assert!(
        flagged.instance.as_deref() == Some("https://blackcatinformatics.ca/gmeow/A")
            || flagged.instance.as_deref() == Some("https://blackcatinformatics.ca/gmeow/B"),
        "unexpected instance {:?}",
        flagged.instance
    );
}

/// A param whose `fno:type` disagrees with its predicate's ontology `rdfs:range` is
/// flagged; an agreeing one is clean; a predicate with no range is skipped.
#[test]
fn fno_type_mismatch_is_flagged_match_is_clean() {
    let onto_ds = ds("@prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         gm:eventTime rdfs:range xsd:dateTime .\n");
    let onto = DslView::new(&onto_ds);

    let bad_ds = ds("@prefix fno: <https://w3id.org/function/ontology#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         gm:pTime a fno:Parameter ; fno:predicate gm:eventTime ; fno:type xsd:string .\n");
    let bad = DslView::new(&bad_ds);
    let probs = fno_type_mismatches(&onto, &bad);
    assert_eq!(probs.len(), 1, "expected one mismatch");
    assert_eq!(probs[0].check, "fno-type");
    assert!(probs[0].message.contains("fno:type is"));
    assert_eq!(
        probs[0].instance.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/pTime")
    );

    let good_ds = ds("@prefix fno: <https://w3id.org/function/ontology#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         gm:pTime a fno:Parameter ; fno:predicate gm:eventTime ; fno:type xsd:dateTime .\n");
    assert!(fno_type_mismatches(&onto, &DslView::new(&good_ds)).is_empty());

    let no_range_ds = ds("@prefix fno: <https://w3id.org/function/ontology#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         gm:pX a fno:Output ; fno:predicate gm:unranged ; fno:type xsd:string .\n");
    assert!(fno_type_mismatches(&onto, &DslView::new(&no_range_ds)).is_empty());
}

/// An EDOAL cell transforming via an undefined `fn*` function is flagged; a `#`-separated
/// IRI has its local name extracted correctly (split on `/` OR `#`).
#[test]
fn undefined_fno_reference_is_flagged() {
    let fno_ds = ds("@prefix fno: <https://w3id.org/function/ontology#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         gm:fnAlpha a fno:Function .\n");
    let fno = DslView::new(&fno_ds);

    let edoal_ds = ds(
        "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n\
         @prefix edoal: <http://ns.inria.org/edoal/1.0/#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         [] a align:Cell ; edoal:transformation [ rdfs:seeAlso gm:fnBeta ] .\n",
    );
    let edoal = vec![("x.edoal.ttl".to_owned(), DslView::new(&edoal_ds))];
    let probs = fno_reference_integrity(&fno, &edoal);
    assert_eq!(probs.len(), 1);
    assert_eq!(probs[0].check, "fno-ref");
    assert!(probs[0].message.contains("undefined FnO function"));
    assert!(probs[0].message.contains("fnBeta"));

    // Define fnBeta → clean.
    let fno2_ds = ds("@prefix fno: <https://w3id.org/function/ontology#> .\n\
         @prefix gm: <https://blackcatinformatics.ca/gmeow/> .\n\
         gm:fnAlpha a fno:Function .\n\
         gm:fnBeta a fno:Function .\n");
    assert!(fno_reference_integrity(&DslView::new(&fno2_ds), &edoal).is_empty());

    // A #-separated undefined function (local name fnGamma) is also caught.
    let hash_ds = ds(
        "@prefix align: <http://knowledgeweb.semanticweb.org/heterogeneity/alignment#> .\n\
         @prefix edoal: <http://ns.inria.org/edoal/1.0/#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         [] a align:Cell ; edoal:transformation \
            [ rdfs:seeAlso <https://example.org/transform#fnGamma> ] .\n",
    );
    let hash_edoal = vec![("h.edoal.ttl".to_owned(), DslView::new(&hash_ds))];
    let probs = fno_reference_integrity(&fno, &hash_edoal);
    assert_eq!(
        probs.len(),
        1,
        "expected the #-separated undefined ref flagged"
    );
    assert!(probs[0].message.contains("fnGamma"));
}

/// `run_soundness` orders FnO findings before alignment findings, then sorts by
/// severity → check → instance (parity with the retired `lint_projection`).
#[test]
fn run_soundness_combines_and_sorts() {
    let onto_ds = ds("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         gmeow:A a owl:Class .\n\
         gmeow:B a owl:Class .\n\
         gmeow:A owl:disjointWith gmeow:B .\n");
    let onto = DslView::new(&onto_ds);
    let targets: BTreeMap<String, DslView<'_>> = BTreeMap::new();
    let empty_net: BTreeMap<String, String> = BTreeMap::new();
    let no_edoal: Vec<(String, DslView<'_>)> = Vec::new();
    let fno_ds = ds("");
    let fno_view = DslView::new(&fno_ds);
    let mappings = vec![
        mapping("gmeow:A", "skos:exactMatch", "schema:Intermediate", "0.9"),
        mapping(
            "gmeow:B",
            "owl:equivalentClass",
            "schema:Intermediate",
            "0.9",
        ),
    ];
    let inputs = SoundnessInputs {
        ontology: &onto,
        target_graphs: &targets,
        network_failed: &empty_net,
        mappings: &mappings,
        fno: &fno_view,
        edoal: &no_edoal,
    };
    let out = run_soundness(&inputs);
    // Sorted: ERROR (equivalence-collapse) appears before any INFO; severity-first.
    assert!(out
        .iter()
        .any(|f| f.check == "equivalence-collapse" && f.severity == "ERROR"));
    // The list is non-decreasing under the canonical comparator.
    for w in out.windows(2) {
        assert!(
            w[0].cmp_severity_check_instance(&w[1]) != std::cmp::Ordering::Greater,
            "output not sorted: {:?} then {:?}",
            w[0],
            w[1]
        );
    }
}
