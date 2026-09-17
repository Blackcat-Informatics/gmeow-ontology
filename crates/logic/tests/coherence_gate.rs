// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only grading of producer-owned whole-bundle coherence observations.
//! Tiny synthetic controls independently exercise the native admission boundary.

use gmeow_logic::coherence_observations::{
    CHARACTERISTIC_ARTIFACT, CharacteristicObservation, DISJOINT_ARTIFACT, DisjointObservation,
    IriQuad, RELCOMP_ARTIFACT, RelcompObservation, evaluate_characteristic_facts,
    project_characteristic_facts,
};
use gmeow_logic::foundation::FoundationQuad;
use gmeow_logic::reason::dl_consistency;
use purrdf::{NativeRdfFormat, dataset_from_bytes};
use std::path::{Path, PathBuf};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

const LOGIC_VIOLATION: &str = "https://blackcatinformatics.ca/logic/violation";

const LOGIC_TRANSITIVE_PROPERTY: &str = "https://blackcatinformatics.ca/logic/transitiveProperty";

const LOGIC_SYMMETRIC_PROPERTY: &str = "https://blackcatinformatics.ca/logic/symmetricProperty";

const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";

const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";

const LOGIC_OVER_ACCESSIBILITY: &str = "https://blackcatinformatics.ca/logic/overAccessibility";

const LOGIC_IRREFLEXIVITY_VIOLATION: &str =
    "https://blackcatinformatics.ca/logic/IrreflexivityViolation";

const LOGIC_ASYMMETRY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/AsymmetryViolation";

const CHAR_WORLD: &str = "https://blackcatinformatics.ca/gmeow/test/characteristic/world";

const GMEOW_SUB_EVENT_OF: &str = "https://blackcatinformatics.ca/gmeow/subEventOf";

const GMEOW_COUNTER_GOAL: &str = "https://blackcatinformatics.ca/gmeow/counterGoal";

const GMEOW_COUNTERPART_OF: &str = "https://blackcatinformatics.ca/gmeow/counterpartOf";

const GMEOW_COARSER_THAN: &str = "https://blackcatinformatics.ca/gmeow/coarserThan";

const GMEOW_SHARPENS: &str = "https://blackcatinformatics.ca/gmeow/sharpens";

const GMEOW_PART_OF: &str = "https://blackcatinformatics.ca/gmeow/partOf";

const GMEOW_VERSION_OF: &str = "https://blackcatinformatics.ca/gmeow/versionOf";

const GMEOW_EDITION_OF: &str = "https://blackcatinformatics.ca/gmeow/editionOf";

const LOGIC_CARRIER_DISAGREEMENT: &str =
    "https://blackcatinformatics.ca/logic/CharacteristicCarrierDisagreement";

const X: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/x";

const A: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/A";

const B: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/B";

const W: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/world";

fn clash_nquads() -> String {
    format!(
        "<{X}> <{RDF_TYPE}> <{A}> <{W}> .\n\
         <{X}> <{RDF_TYPE}> <{B}> <{W}> .\n\
         <{A}> <{OWL_DISJOINT_WITH}> <{B}> <{W}> .\n"
    )
}

fn benign_nquads() -> String {
    format!("<{X}> <{RDF_TYPE}> <{A}> <{W}> .\n")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn observation<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(&repo_root(), name).expect(
        "load the exact authenticated producer coherence observation; tests never produce it",
    );
    let result: Result<T, gmeow_errors::RecordedDiag> =
        serde_json::from_slice(&bytes).expect("decode the selected typed coherence observation");
    result.expect("the explicit coherence producer must complete the selected observation")
}

fn synthetic_verdict(
    dataset: &purrdf::RdfDataset,
) -> gmeow_errors::Result<gmeow_logic::reason::DlVerdict> {
    use gmeow_logic::reason::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    let input = gmeow_logic::reason::prepare_reasoning_input(dataset)?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri(W)),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.coherence.synthetic.v1".to_owned(),
        *input.ingress_contract(),
    )?])?;
    dl_consistency(input, &domains)
}

#[test]
fn dl_consistency_gate_catches_injected_disjoint_clash() {
    // Baseline: the same world without the disjointness is coherent.
    let benign = dataset_from_bytes(benign_nquads().as_bytes(), NativeRdfFormat::NQuads)
        .expect("parse benign N-Quads");
    let v0 = synthetic_verdict(&benign).expect("consistency run over benign world");
    assert!(
        v0.consistent && v0.inconsistencies.is_empty(),
        "a single typed individual is coherent: {:?}",
        v0.inconsistencies
    );

    // Inject the disjoint-class clash → the gate MUST catch it.
    let poisoned = dataset_from_bytes(clash_nquads().as_bytes(), NativeRdfFormat::NQuads)
        .expect("parse clash N-Quads");
    let v1 = synthetic_verdict(&poisoned).expect("consistency run over clash world");
    assert!(
        !v1.consistent,
        "an individual forced into two disjoint classes must be inconsistent"
    );
    assert!(
        v1.inconsistencies
            .iter()
            .any(|w| w.individual.contains("coherence/x")),
        "the inconsistency witness must name the injected individual: {:?}",
        v1.inconsistencies
    );
}

#[test]
fn whole_bundle_coherence_gate_catches_injected_clash() {
    let observed: DisjointObservation = observation(DISJOINT_ARTIFACT);
    let v1 = observed.verdict;
    assert!(
        !v1.consistent,
        "typing an individual both gmeow:Agent and gmeow:SocialObject in the shipped edge's \
         world must be caught by the SHIPPED foundational-partition disjointness (no \
         self-injected owl:disjointWith)"
    );
    assert!(
        v1.inconsistencies
            .iter()
            .any(|w| w.individual.contains("coherence/x")),
        "the inconsistency witness must name the injected individual: {:?}",
        v1.inconsistencies
    );
}

#[test]
fn whole_bundle_relcomp_gate_holds_and_has_teeth() {
    let observed: RelcompObservation = observation(RELCOMP_ARTIFACT);

    // The shipped ontology satisfies relator mediation: zero RelComp violations.
    let offenders = observed.clean_offenders;
    assert!(
        offenders.is_empty(),
        "the committed gmeow.gts must satisfy relator mediation, but these concrete relators \
         reach fewer than two entities (add a distinct mediation role, or make an existing \
         role non-functional): {offenders:#?}"
    );

    // Teeth: a concrete subclass relator mediating a single FUNCTIONAL role reaches one
    // entity → RelComp. Inject it on top of the real bundle and require the gate to fire.
    let bad = "https://blackcatinformatics.ca/gmeow/test/relcomp/DegenerateRelator";
    let offenders = observed.poisoned_offenders;
    assert!(
        offenders.iter().any(|s| s == bad),
        "an injected concrete relator with a single functional role must fire RelComp: \
         {offenders:#?}"
    );
}

#[test]
fn characteristic_projection_retains_contextual_ownership_and_refuses_orphan_modal_nodes() {
    let source = r#"@prefix l: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <urn:projection:> .
        l:overAccessibility a l:functionalProperty .
        ex:request a l:ContextualEvaluationRequest ; l:queryFormula ex:outer .
        ex:outer l:not ex:inner .
        ex:inner l:necessarily ex:body ; l:overAccessibility l:epistemicallyPossible ."#;
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let projected = project_characteristic_facts(&dataset);
    assert!(projected.contains(&IriQuad::new(
        "urn:projection:inner",
        LOGIC_OVER_ACCESSIBILITY,
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        CHAR_WORLD
    )));
    assert!(
        characteristic_violations(&evaluate_characteristic_facts(&projected).unwrap()).is_empty()
    );

    let orphan = source.replace("a l:ContextualEvaluationRequest ; ", "");
    let dataset = purrdf::parse_dataset(orphan.as_bytes(), "text/turtle", None).unwrap();
    let projected = project_characteristic_facts(&dataset);
    let error = evaluate_characteristic_facts(&projected).unwrap_err();
    assert!(error.message().contains("modalEvalWorld"), "{error}");
}

fn characteristic_violations(quads: &[FoundationQuad]) -> Vec<(String, String)> {
    let irr = format!("<{LOGIC_IRREFLEXIVITY_VIOLATION}>");
    let asym = format!("<{LOGIC_ASYMMETRY_VIOLATION}>");
    quads
        .iter()
        .filter(|q| q.predicate == LOGIC_VIOLATION && (q.object == irr || q.object == asym))
        .map(|q| (q.subject.clone(), q.object.clone()))
        .collect()
}

#[test]
fn whole_bundle_characteristic_gate_holds_and_has_teeth() {
    let observed: CharacteristicObservation = observation(CHARACTERISTIC_ARTIFACT);
    let facts = &observed.carrier_facts;

    // Bind to production: every DL-projectable H4 target carries BOTH its canonical logic:
    // marker and its canonical logic: record in the shipped bundle. Drop either carrier of
    // any of them and this test goes red — closing the dual-carrier silent-drift hole.
    let marker_fact = |prop: &str, marker: &str| IriQuad::new(prop, RDF_TYPE, marker, CHAR_WORLD);
    let characterizes_fact =
        |rec: &str, prop: &str| IriQuad::new(rec, LOGIC_CHARACTERIZES, prop, CHAR_WORLD);
    let sort_fact = |rec: &str, sort_local: &str| {
        IriQuad::new(
            rec,
            LOGIC_CHARACTERISTIC_SORT,
            &format!("{LOGIC_NS}{sort_local}"),
            CHAR_WORLD,
        )
    };
    // (property, canonical logic: marker, logic: record local name, characteristic-sort local
    // name). These characteristics are DUAL-carried in the bundle — the property's canonical
    // `?P a logic:{sort}` marker AND its logic: record — but the OWL characteristic view is now
    // generated-view-only (a projection) for transitivity and symmetry too: the authoring
    // vocabulary was retired to logic:, so `owl:TransitiveProperty` / `owl:SymmetricProperty` are
    // ABSENT from the bundle, exactly as functionality was moved to a logic:-only carrier + an
    // owl:-view-only projection for functionality. This loop therefore binds the logic: marker, not
    // the owl: one. gmeow:versionOf / gmeow:editionOf go further (carrier-record only, no direct
    // marker) and are asserted below via the same logic:-only treatment as counterGoal
    // irreflexivity, not through this dual-carrier loop.
    let production: [(&str, &str, &str, &str); 6] = [
        (
            GMEOW_SUB_EVENT_OF,
            LOGIC_TRANSITIVE_PROPERTY,
            "subEventOfTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_COARSER_THAN,
            LOGIC_TRANSITIVE_PROPERTY,
            "coarserThanTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_SHARPENS,
            LOGIC_TRANSITIVE_PROPERTY,
            "sharpensTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_PART_OF,
            LOGIC_TRANSITIVE_PROPERTY,
            "partOfTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_COUNTER_GOAL,
            LOGIC_SYMMETRIC_PROPERTY,
            "counterGoalSymmetry",
            "symmetricProperty",
        ),
        (
            GMEOW_COUNTERPART_OF,
            LOGIC_SYMMETRIC_PROPERTY,
            "counterpartOfSymmetry",
            "symmetricProperty",
        ),
    ];
    for (prop, marker, rec_local, sort_local) in production {
        let rec = format!("{GMEOW_NS}{rec_local}");
        assert!(
            facts.contains(&marker_fact(prop, marker)),
            "the committed gmeow.gts must declare {prop} with canonical logic: characteristic \
             {marker} (the owl: spelling is generated-view-only)"
        );
        assert!(
            facts.contains(&characterizes_fact(&rec, prop)),
            "the committed gmeow.gts must carry the carrier record {rec} characterizing {prop}"
        );
        assert!(
            facts.contains(&sort_fact(&rec, sort_local)),
            "the carrier record {rec} must assert characteristic sort logic:{sort_local}"
        );
    }
    // counterGoal irreflexivity is a logic:-only carrier (no OWL projection, DL-clean).
    let cg_irr = format!("{GMEOW_NS}counterGoalIrreflexivity");
    assert!(
        facts.contains(&characterizes_fact(&cg_irr, GMEOW_COUNTER_GOAL)),
        "the committed gmeow.gts must carry the counterGoal irreflexivity carrier record"
    );
    assert!(
        facts.contains(&sort_fact(&cg_irr, "irreflexiveProperty")),
        "the counterGoal irreflexivity record must assert logic:irreflexiveProperty"
    );

    // Functionality is a logic:-ONLY carrier: the source owl:FunctionalProperty
    // marker was deprecated and now exists only in the generated OWL view, so the bundle
    // carries NO marker triple for these — only the logic: record. The pair (record →
    // characterizes property, record → characteristicSort functionalProperty) is the single
    // carrier, so dropping either half goes red exactly as the dual-carrier loop does above.
    let functional_only: [(&str, &str); 2] = [
        (GMEOW_VERSION_OF, "versionOfFunctionality"),
        (GMEOW_EDITION_OF, "editionOfFunctionality"),
    ];
    for (prop, rec_local) in functional_only {
        let rec = format!("{GMEOW_NS}{rec_local}");
        assert!(
            facts.contains(&characterizes_fact(&rec, prop)),
            "the committed gmeow.gts must carry the carrier-only record {rec} characterizing {prop}"
        );
        assert!(
            facts.contains(&sort_fact(&rec, "functionalProperty")),
            "the carrier record {rec} must assert characteristic sort logic:functionalProperty"
        );
    }

    // HOLDS: the shipped ontology satisfies its property characteristics.
    let clean_violations = observed.clean_violations;
    assert!(
        clean_violations.is_empty(),
        "the committed gmeow.gts must satisfy its property characteristics, but the gate \
         found these irreflexivity/asymmetry violations: {clean_violations:#?}"
    );
    // HOLDS: no dual-carrier drift — every DL-projectable logic: characteristic record in
    // the shipped bundle has its OWL projection, so the agreement gate fires nothing.
    let clean_disagreements = observed.clean_disagreements;
    assert!(
        clean_disagreements.is_empty(),
        "the committed gmeow.gts must have zero characteristic-carrier disagreements, but \
         these properties carry a logic: record whose OWL projection is missing: \
         {clean_disagreements:#?}"
    );

    // TEETH: inject over the shipped declarations + two fresh violating properties.
    let t = "https://blackcatinformatics.ca/gmeow/test/characteristic";
    let out = observed.poisoned;
    let has_edge = |s: &str, p: &str, o: &str| {
        let obj = format!("<{o}>");
        out.iter()
            .any(|q| q.subject == s && q.predicate == p && q.object == obj)
    };
    let fires = |subject: &str, discipline: &str| {
        let obj = format!("<{discipline}>");
        out.iter()
            .any(|q| q.subject == subject && q.predicate == LOGIC_VIOLATION && q.object == obj)
    };

    // Transitivity closure over the shipped transitive property.
    assert!(
        has_edge(&format!("{t}/A"), GMEOW_SUB_EVENT_OF, &format!("{t}/C")),
        "the gate must close A→C over shipped-transitive gmeow:subEventOf"
    );
    // Symmetric mirror over the shipped symmetric property.
    assert!(
        has_edge(&format!("{t}/N"), GMEOW_COUNTER_GOAL, &format!("{t}/M")),
        "the gate must mirror N→M over shipped-symmetric gmeow:counterGoal"
    );
    // counterpartOf is symmetric (mirrored) but deliberately NOT transitive (not closed).
    assert!(
        has_edge(&format!("{t}/Y"), GMEOW_COUNTERPART_OF, &format!("{t}/X")),
        "gmeow:counterpartOf is symmetric, so Y→X must be mirrored"
    );
    assert!(
        !has_edge(&format!("{t}/X"), GMEOW_COUNTERPART_OF, &format!("{t}/Z")),
        "gmeow:counterpartOf is NOT transitive, so X→Z must never be derived"
    );
    // Violation teeth.
    assert!(
        fires(&format!("{t}/self"), LOGIC_IRREFLEXIVITY_VIOLATION),
        "an irreflexive property holding of a self-pair must fire IrreflexivityViolation"
    );
    assert!(
        fires(&format!("{t}/P"), LOGIC_ASYMMETRY_VIOLATION)
            || fires(&format!("{t}/Q"), LOGIC_ASYMMETRY_VIOLATION),
        "an asymmetric property holding both ways must fire AsymmetryViolation"
    );
    // Carrier-agreement teeth: a DL-projectable logic: record injected without its OWL
    // marker must fire CharacteristicCarrierDisagreement on the drifted property.
    assert!(
        fires(&format!("{t}/driftProp"), LOGIC_CARRIER_DISAGREEMENT),
        "a logic: transitive record with no owl:TransitiveProperty projection must fire \
         CharacteristicCarrierDisagreement"
    );
}
