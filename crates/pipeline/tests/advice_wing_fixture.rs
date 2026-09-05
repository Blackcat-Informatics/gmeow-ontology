// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The positive proof of the advisory dual-projection: drive the REAL
//! `gmeow:BareEntitySortalAdviceConstraint` (authored in
//! `slices/grounding/logic/module.ttl`) over a small fixture A-Box, end to end —
//! `logic:Constraint` → derived `sh:SPARQLConstraint` NodeShape → SHACL `Info`-severity
//! violation → [`gmeow_validate::advisory::split_advisory_results`] → [`Advisory::project`]
//! → [`gmeow_validate::advisory::project_compliance_assessment`] N-Quads.
//!
//! This is deliberately NOT a hand-authored shape: the test consumes the exact
//! producer-authenticated SHACL projection selected by the bundle identity. It never
//! compiles the logic module itself. The assertions below pin that the projected shape
//! traces back to
//! `gmeow:BareEntitySortalAdviceConstraint`'s own `logic:formalizes gmeow:Entity` /
//! `logic:severity "Info"` / verbatim `logic:message` (== `gmeow:Entity`'s `avoidWhen` prose,
//! kept in sync by the G4 drift gate).
//!
//! The SHIPPED bundle (`generated/dist/gmeow.gts`) is deliberately TBox-only (no `examples/`
//! individuals in its base graph), so no individual matches this data-matching advisory guard
//! there and its advice wing is honestly EMPTY (see `norm_claims_bundle.rs` /
//! `norm_claims_shacl.rs` / `norm_claims_reasoning.rs`). This test supplies the missing
//! anti-pattern individual itself, as a TEST-ONLY fixture A-Box, to prove the wing fires when
//! data actually matches.

use std::path::{Path, PathBuf};

use gmeow_errors::Severity;
use gmeow_validate::advisory::{
    DEONTIC_RECOMMENDATION_IRI, project_compliance_assessment, split_advisory_results,
};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const BARE_THING: &str = "https://ex.test/bareThing";
const GOOD_THING: &str = "https://ex.test/goodThing";
/// The endurant-as-Event anti-pattern individual: typed BOTH gmeow:Event and
/// gmeow:Entity, so `gmeow:EndurantAsEventAdviceConstraint`'s guard
/// (`?this a gmeow:Event` ∧ `?this a gmeow:Entity`) fires on it.
const BAD_EVENT: &str = "https://ex.test/badEvent";
const DEMO_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-fixture-demo";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// gmeow:Entity's verbatim `avoidWhen` prose (`slices/core/kernel/module.ttl`), which
/// `gmeow:BareEntitySortalAdviceConstraint`'s `logic:message`
/// (`slices/grounding/logic/module.ttl`) is kept byte-identical to by the G4 drift gate — the
/// exact string this test's advisory message must equal.
const EXPECTED_MESSAGE: &str = "Avoid typing an instance as a bare gmeow:Entity when a more \
     specific sortal applies; reserve the unqualified type for genuinely category-neutral \
     resources, and never use it for occurrents (those are gufo:Event, not endurants).";

/// gmeow:Event's verbatim `avoidWhen` prose (`slices/core/events/module.ttl`), which
/// `gmeow:EndurantAsEventAdviceConstraint`'s `logic:message`
/// (`slices/grounding/logic/module.ttl`) is kept byte-identical to by the G4 drift gate —
/// the exact string the second advisory's message must equal.
const EXPECTED_EAE_MESSAGE: &str = "Avoid typing an endurant as an Event (a person/document/place \
     is a gmeow:Entity, not an occurrence), avoid minting an event-kind SUBCLASS (the kind is a \
     gmeow:eventType value, Principle 9), and reach for gmeow:Activity when the occurrence is an \
     agent-driven provenance act with inputs and outputs.";

/// Load the producer-authenticated SHACL projection and assert that it carries a
/// shape derived from `gmeow:BareEntitySortalAdviceConstraint` itself.
fn authenticated_advisory_shapes() -> String {
    let shapes_ttl = String::from_utf8(
        gmeow_bundle_import::load_authenticated_corpus_artifact(
            &repo_root(),
            "validate-production-shapes.ttl",
        )
        .expect("authenticated production SHACL projection; tests never compile it"),
    )
    .expect("authenticated SHACL projection is UTF-8");

    // The shape must trace back to the REAL constraint: its own `logic:formalizes gmeow:Entity`,
    // `sh:severity sh:Info`, and its own verbatim `logic:message` (the shape's `sh:message`).
    assert!(
        shapes_ttl.contains(&format!("logic:formalizes <{GMEOW}Entity>")),
        "the projected shapes must carry a shape formalizing gmeow:Entity (derived from \
         gmeow:BareEntitySortalAdviceConstraint); shapes_ttl:\n{shapes_ttl}"
    );
    assert!(
        shapes_ttl.contains("sh:severity sh:Info"),
        "the projected shapes must carry an Info-severity shape (the advisory tier); \
         shapes_ttl:\n{shapes_ttl}"
    );
    assert!(
        shapes_ttl.contains(EXPECTED_MESSAGE),
        "the projected shape's sh:message must equal gmeow:BareEntitySortalAdviceConstraint's \
         verbatim logic:message (== gmeow:Entity's avoidWhen prose); shapes_ttl:\n{shapes_ttl}"
    );
    assert!(
        shapes_ttl.contains("BareEntitySortalAdviceConstraint"),
        "the projected shape's IRI must be derived from gmeow:BareEntitySortalAdviceConstraint \
         itself (procedural_shape_iri mirrors the constraint's own IRI), not a hand-authored \
         stand-in; shapes_ttl:\n{shapes_ttl}"
    );

    shapes_ttl
}

/// The fixture A-Box (N-Triples, the format `purrdf::shapes::engine::validate_graphs` parses
/// as its data graph):
///
/// * `bareThing` — a bare `gmeow:Entity` individual (no top-sortal type — the anti-pattern
///   `gmeow:BareEntitySortalAdviceConstraint`'s guard matches);
/// * `goodThing` — a control individual typed BOTH `gmeow:Entity` AND `gmeow:Agent` (a top
///   sortal — must NOT match either guard's negated disjunction);
/// * `badEvent` — typed BOTH `gmeow:Event` AND `gmeow:Entity`, the endurant-as-Event
///   anti-pattern `gmeow:EndurantAsEventAdviceConstraint`'s guard matches (it also has no
///   top sortal, so the bare-Entity guard fires on it too — an endurant mistyped as an
///   occurrence is genuinely BOTH mistakes).
fn fixture_abox_ntriples() -> String {
    format!(
        "<{BARE_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Entity> .\n\
         <{GOOD_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Entity> .\n\
         <{GOOD_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Agent> .\n\
         <{BAD_EVENT}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Event> .\n\
         <{BAD_EVENT}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Entity> .\n"
    )
}

/// The end-to-end positive proof (G2 + F2): the REAL advisory constraints, compiled off the
/// canonical logic module, fire as data-matching advisories and produce both projection wings —
/// the graded Note finding and the `deonticRecommendation` `gmeow:ComplianceAssessment` claim.
/// `gmeow:BareEntitySortalAdviceConstraint` fires on the bare `gmeow:Entity` individual, and
/// `gmeow:EndurantAsEventAdviceConstraint` fires on the endurant-as-Event individual (typed both
/// `gmeow:Event` and `gmeow:Entity`); the control individual (a proper `gmeow:Entity`+`gmeow:Agent`)
/// fires neither.
#[test]
fn bare_entity_fixture_fires_the_real_advisory_constraint_end_to_end() {
    let shapes_ttl = authenticated_advisory_shapes();
    let data_nt = fixture_abox_ntriples();

    let report = purrdf::shapes::engine::validate_graphs(&data_nt, &shapes_ttl, None)
        .expect("SHACL validation of the fixture A-Box against the derived shapes must succeed");

    // Parse the same two graphs as RdfDatasets for `split_advisory_results`'s `shapes` /
    // `ontology` parameters (the `shapes` graph resolves each fired shape's `logic:formalizes`
    // provenance; `ontology` resolves the formalized term's howToUse/useWhen source prose).
    let shapes_dataset = purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None)
        .expect("the projected shapes_ttl must itself parse as Turtle");

    // The ontology side is the exact authenticated repository dataset. A missing or
    // wrong-identity corpus fails closed; the test has no authored-source fallback.
    let ontology_dataset = gmeow_bundle_import::load_authenticated_repository_bundle(&repo_root())
        .expect("authenticated repository corpus; tests never rebuild it")
        .dataset;

    let (retained, advisories) =
        split_advisory_results(report, shapes_dataset.as_ref(), ontology_dataset.as_ref());

    // The retained (non-advisory) report must conform: the fixture carries no hard violation,
    // only the Info-severity advisory match, which `split_advisory_results` lifts out.
    assert!(
        retained.conforms,
        "the retained (non-advisory) SHACL report must conform; results: {:#?}",
        retained.results
    );

    // ── BOTH advisory constraints fired, each on its own anti-pattern individual ────────────
    // bareThing → bare-Entity advice; badEvent → BOTH the bare-Entity advice (it too carries
    // no top sortal) AND the endurant-as-Event advice; goodThing → nothing (its gmeow:Agent
    // top sortal clears the bare-Entity guard and it is not an Event). So three advisories.
    assert_eq!(
        advisories.len(),
        3,
        "expected three advisories (bareThing bare-Entity; badEvent bare-Entity + \
         endurant-as-Event; goodThing none): {advisories:#?}"
    );

    // The bare-Entity advisory on the bare individual (the pre-existing coverage).
    let bare = advisories
        .iter()
        .find(|a| {
            a.code.contains("BareEntitySortalAdviceConstraint")
                && a.subject_iri.as_deref() == Some(BARE_THING)
        })
        .unwrap_or_else(|| {
            panic!("gmeow:BareEntitySortalAdviceConstraint must fire on bareThing: {advisories:#?}")
        });
    assert!(bare.code.starts_with("advice."));
    assert_eq!(bare.severity, Severity::Note);
    assert_eq!(
        bare.message, EXPECTED_MESSAGE,
        "the bare-Entity advisory message must equal gmeow:Entity's verbatim avoidWhen prose"
    );

    // ── the SECOND advisory constraint: endurant-as-Event, on badEvent (F2) ─────────────────
    let eae = advisories
        .iter()
        .find(|a| a.code.contains("EndurantAsEventAdviceConstraint"))
        .unwrap_or_else(|| {
            panic!(
                "gmeow:EndurantAsEventAdviceConstraint must fire on the fixture: {advisories:#?}"
            )
        });
    assert_eq!(
        eae.subject_iri.as_deref(),
        Some(BAD_EVENT),
        "the endurant-as-Event advisory's subject must be the badEvent fixture individual"
    );
    assert_eq!(eae.severity, Severity::Note);
    assert_eq!(
        eae.message, EXPECTED_EAE_MESSAGE,
        "the endurant-as-Event advisory message must equal gmeow:Event's verbatim avoidWhen prose"
    );

    // The endurant-as-Event guard must fire on badEvent ONLY — never on the bare individual
    // (not an Event) nor on the control (a proper Entity+Agent, also not an Event).
    let eae_subjects: Vec<&str> = advisories
        .iter()
        .filter(|a| a.code.contains("EndurantAsEventAdviceConstraint"))
        .filter_map(|a| a.subject_iri.as_deref())
        .collect();
    assert_eq!(
        eae_subjects,
        vec![BAD_EVENT],
        "the endurant-as-Event advisory must fire on badEvent alone: {eae_subjects:?}"
    );

    // ── the claim wings + reified ComplianceAssessment N-Quads for BOTH advisories ──────────
    let claims: Vec<_> = advisories.iter().map(|a| a.project().claim).collect();
    for claim in &claims {
        assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
    }
    let eae_claim = eae.project().claim;
    assert_eq!(eae_claim.subject_iri.as_deref(), Some(BAD_EVENT));

    let nquads = project_compliance_assessment(&claims, DEMO_GRAPH);
    purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("the emitted ComplianceAssessment N-Quads must parse cleanly");
    assert!(
        nquads.contains(&format!("{}/assessment", eae_claim.code)),
        "the endurant-as-Event ComplianceAssessment IRI must embed its advice code: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{GMEOW}deonticModality> <{DEONTIC_RECOMMENDATION_IRI}>"
        )),
        "each assessed norm must carry gmeow:deonticModality gmeow:deonticRecommendation: {nquads}"
    );
    assert!(
        nquads.contains(&format!("<{GMEOW}ComplianceAssessment>")),
        "the emitted N-Quads must type an individual gmeow:ComplianceAssessment: {nquads}"
    );

    // ── the control individual (a proper Entity+Agent) must NOT fire ANY advisory ───────────
    assert!(
        !advisories
            .iter()
            .any(|a| a.subject_iri.as_deref() == Some(GOOD_THING)),
        "the control individual (typed gmeow:Entity AND gmeow:Agent) must not trigger any \
         advisory: {advisories:#?}"
    );

    // Visible in `cargo nextest run --no-capture`: the observed advisories + claim output.
    for advisory in &advisories {
        eprintln!("=== advice_wing_fixture: observed advisory ===");
        eprintln!("code:    {}", advisory.code);
        eprintln!("subject: {:?}", advisory.subject_iri);
        eprintln!("message: {}", advisory.message);
        eprintln!("suggestions: {:?}", advisory.suggestions);
    }
    eprintln!("=== advice_wing_fixture: ComplianceAssessment N-Quads ===");
    eprintln!("{nquads}");
}
