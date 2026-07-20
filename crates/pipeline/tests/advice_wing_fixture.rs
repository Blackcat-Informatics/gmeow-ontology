// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The positive proof of the advisory dual-projection: drive the REAL
//! `gmeow:BareEntitySortalAdviceConstraint` (authored in
//! `slices/grounding/logic/module.ttl`) over a small fixture A-Box, end to end —
//! `logic:Constraint` → derived `sh:SPARQLConstraint` NodeShape → SHACL `Info`-severity
//! violation → [`gmeow_validate::advisory::split_advisory_results`] → [`Advisory::project`]
//! → [`gmeow_validate::advisory::project_compliance_assessment`] N-Quads.
//!
//! This is deliberately NOT a hand-authored shape: step 1 reads the canonical logic module
//! text, step 2 compiles it with the same `gmeow_logic_compile::frontend::parse_logic_str` +
//! `gmeow_logic_compile::projections::shapes::project_procedural_constraints` pipeline the
//! real `compile_logic` pipeline stage uses (`crates/pipeline/src/stages/compile_logic.rs`),
//! and the assertions below pin that the projected shape traces back to
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

/// Read the real logic module text, compile it with the exact frontend + projection APIs the
/// pipeline's `compile_logic` stage uses, and assert the projected `shapes_ttl` carries a shape
/// derived from `gmeow:BareEntitySortalAdviceConstraint` itself (not a stand-in).
fn compile_real_advisory_shapes() -> String {
    let module_path = repo_root().join("slices/grounding/logic/module.ttl");
    let module_text = std::fs::read_to_string(&module_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", module_path.display()));

    let (program, diags) = gmeow_logic_compile::frontend::parse_logic_str(&module_text, None)
        .expect("the canonical logic: module must parse as a logic: program");
    // MALFORMED_CONSTRAINT/MALFORMED_FORMULA diagnostics would mean the real constraint failed
    // to extract — hard-fail rather than silently validate against a truncated program.
    let hard_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
        .collect();
    assert!(
        hard_diags.is_empty(),
        "parsing slices/grounding/logic/module.ttl must not raise error-grade diagnostics: {hard_diags:?}"
    );

    let shapes_ttl = gmeow_logic_compile::projections::shapes::project_procedural_constraints(&program);

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
/// as its data graph): one bare `gmeow:Entity` individual (no top-sortal type — the anti-pattern
/// the real constraint's guard matches) and one control individual typed BOTH `gmeow:Entity`
/// AND `gmeow:Agent` (a top sortal — must NOT match the guard's negated disjunction).
fn fixture_abox_ntriples() -> String {
    format!(
        "<{BARE_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Entity> .\n\
         <{GOOD_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Entity> .\n\
         <{GOOD_THING}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}Agent> .\n"
    )
}

/// The end-to-end positive proof (G2): the REAL `gmeow:BareEntitySortalAdviceConstraint`,
/// compiled off the canonical logic module, fires as a data-matching advisory on the bare
/// `gmeow:Entity` fixture individual and produces both projection wings — the graded Note
/// finding and the `deonticRecommendation` `gmeow:ComplianceAssessment` claim — while the
/// control individual (typed a top sortal too) does not fire at all.
#[test]
fn bare_entity_fixture_fires_the_real_advisory_constraint_end_to_end() {
    let shapes_ttl = compile_real_advisory_shapes();
    let data_nt = fixture_abox_ntriples();

    let report = purrdf::shapes::engine::validate_graphs(&data_nt, &shapes_ttl)
        .expect("SHACL validation of the fixture A-Box against the derived shapes must succeed");

    // Parse the same two graphs as RdfDatasets for `split_advisory_results`'s `shapes` /
    // `ontology` parameters (the `shapes` graph resolves each fired shape's `logic:formalizes`
    // provenance; `ontology` resolves the formalized term's howToUse/useWhen source prose).
    let shapes_dataset = purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None)
        .expect("the projected shapes_ttl must itself parse as Turtle");

    // Union the logic module (carries the constraint + its `logic:formalizes gmeow:Entity`
    // back-reference — though that provenance is read from `shapes_dataset`, not this one) with
    // the kernel module (carries gmeow:Entity's own howToUse/useWhen source-language prose) so
    // the advisory's suggestion/guidance surfacing actually resolves.
    let module_path = repo_root().join("slices/grounding/logic/module.ttl");
    let kernel_path = repo_root().join("slices/core/kernel/module.ttl");
    let mut ontology_bytes =
        std::fs::read(&module_path).unwrap_or_else(|e| panic!("read {}: {e}", module_path.display()));
    ontology_bytes.push(b'\n');
    ontology_bytes.extend(
        std::fs::read(&kernel_path).unwrap_or_else(|e| panic!("read {}: {e}", kernel_path.display())),
    );
    let ontology_dataset = purrdf::parse_dataset(&ontology_bytes, "text/turtle", None)
        .expect("the union of the logic + kernel modules must parse as Turtle");

    let (retained, advisories) =
        split_advisory_results(report, shapes_dataset.as_ref(), ontology_dataset.as_ref());

    // The retained (non-advisory) report must conform: the fixture carries no hard violation,
    // only the Info-severity advisory match, which `split_advisory_results` lifts out.
    assert!(
        retained.conforms,
        "the retained (non-advisory) SHACL report must conform; results: {:#?}",
        retained.results
    );

    // ── exactly one advisory fired, on the bare-Entity individual ───────────────────────────
    assert_eq!(
        advisories.len(),
        1,
        "expected exactly one advisory (bareThing only; goodThing must not fire): {advisories:#?}"
    );
    let advisory = &advisories[0];

    assert_eq!(
        advisory.subject_iri.as_deref(),
        Some(BARE_THING),
        "the advisory's subject must be the bare-Entity fixture individual"
    );
    assert!(
        advisory.code.starts_with("advice."),
        "advisory code must carry the advice. family prefix: {}",
        advisory.code
    );
    assert!(
        advisory.code.contains("BareEntitySortalAdviceConstraint"),
        "advisory code must embed the governing shape's local name: {}",
        advisory.code
    );
    assert_eq!(advisory.severity, Severity::Note);
    assert_eq!(
        advisory.message, EXPECTED_MESSAGE,
        "the advisory message must equal gmeow:Entity's verbatim avoidWhen prose"
    );

    // ── the claim wing ───────────────────────────────────────────────────────────────────────
    let projection = advisory.project();
    let claim = &projection.claim;
    assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
    assert_eq!(claim.subject_iri.as_deref(), Some(BARE_THING));

    // ── the reified ComplianceAssessment N-Quads ─────────────────────────────────────────────
    let nquads = project_compliance_assessment(std::slice::from_ref(claim), DEMO_GRAPH);
    purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("the emitted ComplianceAssessment N-Quads must parse cleanly");
    assert!(
        nquads.contains(&format!("{}/assessment", claim.code)),
        "the ComplianceAssessment IRI must embed the advice code: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{GMEOW}deonticModality> <{DEONTIC_RECOMMENDATION_IRI}>"
        )),
        "the assessed norm must carry gmeow:deonticModality gmeow:deonticRecommendation: {nquads}"
    );
    assert!(
        nquads.contains(&format!("<{GMEOW}ComplianceAssessment>")),
        "the emitted N-Quads must type an individual gmeow:ComplianceAssessment: {nquads}"
    );

    // ── the control individual (typed a top sortal too) must NOT fire ───────────────────────
    assert!(
        !advisories
            .iter()
            .any(|a| a.subject_iri.as_deref() == Some(GOOD_THING)),
        "the control individual (typed gmeow:Entity AND gmeow:Agent) must not trigger the \
         advisory: {advisories:#?}"
    );

    // Visible in `cargo nextest run --no-capture`: the observed advisory + claim output.
    eprintln!("=== advice_wing_fixture: observed advisory ===");
    eprintln!("code:    {}", advisory.code);
    eprintln!("message: {}", advisory.message);
    eprintln!("suggestions: {:?}", advisory.suggestions);
    eprintln!("=== advice_wing_fixture: ComplianceAssessment N-Quads ===");
    eprintln!("{nquads}");
}
