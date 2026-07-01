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
//! - `whole_bundle_coherence_gate_catches_injected_clash` — the same clash injected on
//!   top of the WHOLE committed `gmeow.gts`, additionally asserting the shipped ontology
//!   is itself coherent (a regression guard on the bundle). This is the literal
//!   whole-ontology proof and it RUNS ON-GATE, via the dedicated `make
//!   coherence-gate-teeth` target. The full-bundle chase takes ~95 s, well over the 25 s
//!   per-test budget, so it stays carved out of the budget-gated `ci`/`default` nextest
//!   profile by `default-filter` — that exclusion is budget-exempt, not gate-exempt:
//!   `coherence-gate-teeth` invokes it explicitly with `--ignore-default-filter` and an
//!   `-E` selector, without feeding the JUnit report into the 25 s budget gate, and is
//!   wired into `make check` via `CHECK_TARGETS`. The minimal test above remains a fast,
//!   deterministic companion.

use gmeow_logic::reason::dl_consistency;
use gmeow_rdf::{dataset_from_bytes, import_gts_events, NativeRdfFormat, RdfDatasetBuilder};
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

/// The net-new top-sortal clash: individual X is typed both gmeow:Agent and
/// gmeow:SocialObject, which this branch asserts owl:disjointWith. World-scoped.
fn top_sortal_clash_nquads() -> String {
    format!(
        "<{X}> <{RDF_TYPE}> <{AGENT}> <{W}> .\n\
         <{X}> <{RDF_TYPE}> <{SOCIAL_OBJECT}> <{W}> .\n\
         <{AGENT}> <{OWL_DISJOINT_WITH}> <{SOCIAL_OBJECT}> <{W}> .\n"
    )
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
fn dl_consistency_gate_catches_net_new_agent_social_object_clash() {
    // The foundational partition's NET-NEW edge (gmeow:Agent ⊥ gmeow:SocialObject,
    // which did not exist before this branch) must have teeth: an individual typed as
    // both a gmeow:Agent and a gmeow:SocialObject is forced to owl:Nothing.
    let poisoned = dataset_from_bytes(
        top_sortal_clash_nquads().as_bytes(),
        NativeRdfFormat::NQuads,
    )
    .expect("parse top-sortal clash N-Quads");
    let v = dl_consistency(poisoned.as_ref()).expect("consistency run over top-sortal clash");
    assert!(
        !v.consistent,
        "an individual typed both gmeow:Agent and gmeow:SocialObject must be inconsistent \
         under the net-new top-sortal disjointness"
    );
    assert!(
        v.inconsistencies
            .iter()
            .any(|w| w.individual.contains("coherence/x")),
        "the inconsistency witness must name the injected individual: {:?}",
        v.inconsistencies
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

    // Inject the self-contained clash on top of the whole ontology → the gate must catch it.
    let clash = dataset_from_bytes(clash_nquads().as_bytes(), NativeRdfFormat::NQuads)
        .expect("parse clash N-Quads");
    let mut builder = RdfDatasetBuilder::new();
    for quad in onto.owned_quads() {
        builder.push_owned_quad(&quad);
    }
    for quad in clash.owned_quads() {
        builder.push_owned_quad(&quad);
    }
    let poisoned = builder.freeze().expect("freeze the poisoned bundle");

    let v1 = dl_consistency(poisoned.as_ref()).expect("consistency run over poisoned bundle");
    assert!(
        !v1.consistent,
        "the whole-ontology gate must catch a disjoint-class clash injected into the bundle"
    );
    assert!(
        v1.inconsistencies
            .iter()
            .any(|w| w.individual.contains("coherence/x")),
        "the inconsistency witness must name the injected individual: {:?}",
        v1.inconsistencies
    );
}
