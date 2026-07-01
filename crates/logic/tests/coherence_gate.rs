//! Whole-ontology coherence-gate teeth tests.
//!
//! `make check` already runs a whole-ontology native DL consistency pass: the
//! `reason-verify` target imports the committed `gmeow.gts` bundle and reasons over
//! it via the same [`gmeow_logic::reason::reason_closure`] the verdict-only
//! [`dl_consistency`] entry point wraps. A gate that RUNS but is never shown to CATCH
//! anything is untrustworthy, so these tests prove it has teeth: an individual forced
//! into two `owl:disjointWith` classes is derived into `owl:Nothing` and reported as an
//! [`gmeow_logic::reason::InconsistencyWitness`].
//!
//! - `dl_consistency_gate_catches_injected_disjoint_clash` — the primary, deterministic
//!   proof over a minimal dataset. It exercises the SAME engine the whole-ontology gate
//!   uses, is trivially within the per-test budget, and therefore always runs on the
//!   `make check` lane (never carved out by `default-filter`).
//! - `whole_bundle_coherence_gate_catches_injected_clash` — injects ONLY the two type
//!   assertions (an individual typed both `gmeow:Agent` and `gmeow:SocialObject`, with NO
//!   `owl:disjointWith` of its own) on top of the WHOLE committed `gmeow.gts`, so the
//!   clash is forced SOLELY by the foundational-partition disjointness the bundle itself
//!   ships — binding the PRODUCTION edge to the gate's teeth (drop the kernel assertion
//!   and this test goes green→red). It additionally asserts the shipped ontology is itself
//!   coherent (a regression guard on the bundle). This is the literal whole-ontology proof
//!   and it RUNS ON-GATE, via the dedicated `make
//!   coherence-gate-teeth` target. The full-bundle chase takes ~95 s, well over the 25 s
//!   per-test budget, so it stays carved out of the budget-gated `ci`/`default` nextest
//!   profile by `default-filter` — that exclusion is budget-exempt, not gate-exempt:
//!   `coherence-gate-teeth` invokes it explicitly with `--ignore-default-filter` and an
//!   `-E` selector, without feeding the JUnit report into the 25 s budget gate, and is
//!   wired into `make check` via `CHECK_TARGETS`. The minimal test above remains a fast,
//!   deterministic companion.

use gmeow_logic::reason::dl_consistency;
use gmeow_rdf::{
    dataset_from_bytes, import_gts_events, NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm,
};
use std::path::{Path, PathBuf};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

// A self-contained clash in a fresh world so it can never interact with the shipped
// ontology's own worlds: individual X is typed into A and B, which are disjoint.
const X: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/x";
const A: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/A";
const B: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/B";
const W: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/world";

// The NET-NEW foundational-partition edge on this branch: gmeow:Agent is disjoint
// with gmeow:SocialObject (it did NOT exist before). Using the real production IRIs
// proves the newly-asserted disjointness — not a synthetic one — has teeth: an
// individual typed as both is forced to owl:Nothing by the coherence gate.
const AGENT: &str = "https://blackcatinformatics.ca/gmeow/Agent";
const SOCIAL_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/SocialObject";

/// The injected disjoint-class clash, as world-scoped N-Quads.
fn clash_nquads() -> String {
    format!(
        "<{X}> <{RDF_TYPE}> <{A}> <{W}> .\n\
         <{X}> <{RDF_TYPE}> <{B}> <{W}> .\n\
         <{A}> <{OWL_DISJOINT_WITH}> <{B}> <{W}> .\n"
    )
}

/// The same world with only the type assertion — no disjointness, hence coherent.
fn benign_nquads() -> String {
    format!("<{X}> <{RDF_TYPE}> <{A}> <{W}> .\n")
}

/// Repo root: `crates/logic` → `../..`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn dl_consistency_gate_catches_injected_disjoint_clash() {
    // Baseline: the same world without the disjointness is coherent.
    let benign = dataset_from_bytes(benign_nquads().as_bytes(), NativeRdfFormat::NQuads)
        .expect("parse benign N-Quads");
    let v0 = dl_consistency(benign.as_ref()).expect("consistency run over benign world");
    assert!(
        v0.consistent && v0.inconsistencies.is_empty(),
        "a single typed individual is coherent: {:?}",
        v0.inconsistencies
    );

    // Inject the disjoint-class clash → the gate MUST catch it.
    let poisoned = dataset_from_bytes(clash_nquads().as_bytes(), NativeRdfFormat::NQuads)
        .expect("parse clash N-Quads");
    let v1 = dl_consistency(poisoned.as_ref()).expect("consistency run over clash world");
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
    // Load the committed bundle exactly as production `reason-verify` does.
    let gts_path = repo_root().join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&gts_path)
        .unwrap_or_else(|e| panic!("read committed bundle {}: {e}", gts_path.display()));
    let bundle = import_gts_events(&bytes).expect("import the committed gmeow.gts bundle");
    let onto = bundle.dataset;

    // The shipped ontology is coherent as-is (a regression guard on the bundle).
    let v0 = dl_consistency(onto.as_ref()).expect("consistency run over the whole bundle");
    assert!(
        v0.consistent,
        "the committed gmeow.gts must be coherent, but the gate found: {:?}",
        v0.inconsistencies
    );

    // Locate the SHIPPED gmeow:Agent ⊥ gmeow:SocialObject edge in the bundle and read the
    // world (named graph) it lives in. Finding it AT ALL proves the net-new production
    // edge is actually shipped — drop the kernel assertion and this lookup fails. We inject
    // NO owl:disjointWith of our own (a self-injected one would mask exactly that
    // regression). The DL disjointness rule is world-scoped, so the clashing individual
    // must be typed in the SAME world as the shipped edge.
    let disjoint_world = onto
        .owned_quads()
        .find_map(|q| {
            let is_edge = q.predicate == OWL_DISJOINT_WITH
                && matches!(
                    (&q.subject, &q.object),
                    (RdfTerm::Iri(s), RdfTerm::Iri(o))
                        if (s == AGENT && o == SOCIAL_OBJECT) || (s == SOCIAL_OBJECT && o == AGENT)
                );
            is_edge.then(|| q.graph_name.clone())
        })
        .expect(
            "the committed gmeow.gts must ship gmeow:Agent owl:disjointWith gmeow:SocialObject",
        );

    // Type an individual into BOTH classes in that SAME world → the world-scoped DL
    // disjointness rule fires solely from the shipped edge, forcing X to owl:Nothing.
    let individual = RdfTerm::Iri(X.to_owned());
    let mut type_agent = RdfQuad::new(individual.clone(), RDF_TYPE, RdfTerm::Iri(AGENT.to_owned()));
    let mut type_social =
        RdfQuad::new(individual, RDF_TYPE, RdfTerm::Iri(SOCIAL_OBJECT.to_owned()));
    if let Some(world) = &disjoint_world {
        type_agent = type_agent.in_graph(world.clone());
        type_social = type_social.in_graph(world.clone());
    }

    let mut builder = RdfDatasetBuilder::new();
    for quad in onto.owned_quads() {
        builder.push_owned_quad(&quad);
    }
    builder.push_owned_quad(&type_agent);
    builder.push_owned_quad(&type_social);
    let poisoned = builder.freeze().expect("freeze the poisoned bundle");

    let v1 = dl_consistency(poisoned.as_ref()).expect("consistency run over poisoned bundle");
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
