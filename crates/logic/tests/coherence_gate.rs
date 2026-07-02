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

use gmeow_logic::foundation::{evaluate as foundation_evaluate, AntiRigidityPolicy};
use gmeow_logic::reason::dl_consistency;
use gmeow_logic::store::WorldStore;
use purrdf::{
    dataset_from_bytes, import_gts_events, NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const LOGIC_SUBCLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";
const LOGIC_MEDIATES: &str = "https://blackcatinformatics.ca/logic/mediates";
const LOGIC_VIOLATION: &str = "https://blackcatinformatics.ca/logic/violation";
const LOGIC_RELCOMP: &str = "https://blackcatinformatics.ca/logic/RelComp";
const LOGIC_KIND: &str = "https://blackcatinformatics.ca/logic/Kind";
const LOGIC_RELATOR: &str = "https://blackcatinformatics.ca/logic/Relator";
// One synthetic world holding the whole bundle's relator schema — RelComp is a
// class-level (TBox) discipline, so a single world is the correct scope.
const BUNDLE_WORLD: &str = "https://blackcatinformatics.ca/gmeow/test/relcomp/world";

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

/// Project the committed bundle to the IRI-only fact set the foundation reasoner needs
/// for the relator-mediation discipline, as world-scoped N-Quads in [`BUNDLE_WORLD`]:
///
/// - `rdf:type` triples whose object is a `logic:` stereotype or `owl:FunctionalProperty`
///   (the stereotype puns + the functional-role markers),
/// - every `rdfs:subClassOf` / `logic:subClassOf` edge (mapped to `logic:subClassOf`, so
///   `subClassOfT` reaches `logic:Relator` and `hasLogicSubclass` distinguishes leaves),
/// - every `logic:mediates` edge (the mediation roles).
///
/// The foundation chase is all-IRI, so literal- and blank-object triples are dropped; none
/// bear on relator mediation.
fn project_relator_facts(onto: &purrdf::RdfDataset) -> BTreeSet<String> {
    let mut lines: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        let mapped_predicate = match q.predicate.as_str() {
            RDF_TYPE if o.starts_with(LOGIC_NS) || o == OWL_FUNCTIONAL_PROPERTY => RDF_TYPE,
            RDFS_SUBCLASS_OF | LOGIC_SUBCLASS_OF => LOGIC_SUBCLASS_OF,
            LOGIC_MEDIATES => LOGIC_MEDIATES,
            _ => continue,
        };
        lines.insert(format!(
            "<{s}> <{mapped_predicate}> <{o}> <{BUNDLE_WORLD}> .\n"
        ));
    }
    lines
}

/// Run the native foundation discipline over projected N-Quads and return the subject IRIs
/// that fire `logic:violation logic:RelComp`.
fn relcomp_offenders(nquads: &str) -> Vec<String> {
    let store = WorldStore::new();
    store
        .load_nquads(nquads)
        .expect("load the projected relator facts");
    let quads = foundation_evaluate(&store, AntiRigidityPolicy::WitnessObligation)
        .expect("foundation evaluate over the projected relator facts");
    let relcomp_obj = format!("<{LOGIC_RELCOMP}>");
    quads
        .into_iter()
        .filter(|q| q.predicate == LOGIC_VIOLATION && q.object == relcomp_obj)
        .map(|q| q.subject)
        .collect()
}

/// Whole-ontology relator-mediation gate. Projects the committed `gmeow.gts` to its
/// relator schema and runs the native foundation discipline over the whole bundle — the
/// canonical `logic:` enforcement mechanism, no longer confined to conformance fixtures.
/// Every concrete production relator must reach at least two entities (two distinct roles,
/// or one non-functional role), so the shipped ontology must produce ZERO RelComp
/// violations. A degenerate relator injected on top (a single functional role) must fire,
/// proving the gate has teeth.
///
/// Named `whole_bundle_..._gate` and matched by the `coherence-gate-teeth` selector; the
/// whole-bundle chase is over the 25 s budget, so it is carved out of the budget-gated
/// nextest profile by `default-filter` (budget-exempt, not gate-exempt).
#[test]
fn whole_bundle_relcomp_gate_holds_and_has_teeth() {
    let gts_path = repo_root().join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&gts_path)
        .unwrap_or_else(|e| panic!("read committed bundle {}: {e}", gts_path.display()));
    let bundle = import_gts_events(&bytes).expect("import the committed gmeow.gts bundle");
    let facts = project_relator_facts(bundle.dataset.as_ref());
    let projection: String = facts.iter().cloned().collect();

    // The shipped ontology satisfies relator mediation: zero RelComp violations.
    let offenders = relcomp_offenders(&projection);
    assert!(
        offenders.is_empty(),
        "the committed gmeow.gts must satisfy relator mediation, but these concrete relators \
         reach fewer than two entities (add a distinct mediation role, or make an existing \
         role non-functional): {offenders:#?}"
    );

    // Teeth: a concrete subclass relator mediating a single FUNCTIONAL role reaches one
    // entity → RelComp. Inject it on top of the real bundle and require the gate to fire.
    let bad = "https://blackcatinformatics.ca/gmeow/test/relcomp/DegenerateRelator";
    let role = "https://blackcatinformatics.ca/gmeow/test/relcomp/soleRole";
    let poisoned = format!(
        "{projection}\
         <{bad}> <{RDF_TYPE}> <{LOGIC_KIND}> <{BUNDLE_WORLD}> .\n\
         <{bad}> <{LOGIC_SUBCLASS_OF}> <{LOGIC_RELATOR}> <{BUNDLE_WORLD}> .\n\
         <{bad}> <{LOGIC_MEDIATES}> <{role}> <{BUNDLE_WORLD}> .\n\
         <{role}> <{RDF_TYPE}> <{OWL_FUNCTIONAL_PROPERTY}> <{BUNDLE_WORLD}> .\n"
    );
    let offenders = relcomp_offenders(&poisoned);
    assert!(
        offenders.iter().any(|s| s == bad),
        "an injected concrete relator with a single functional role must fire RelComp: \
         {offenders:#?}"
    );
}
