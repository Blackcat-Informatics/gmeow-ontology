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
//!   uses and always runs on the `make check` lane.
//! - `whole_bundle_coherence_gate_catches_injected_clash` — injects ONLY the two type
//!   assertions (an individual typed both `gmeow:Agent` and `gmeow:SocialObject`, with NO
//!   `owl:disjointWith` of its own) on top of the WHOLE committed `gmeow.gts`, so the
//!   clash is forced SOLELY by the foundational-partition disjointness the bundle itself
//!   ships — binding the PRODUCTION edge to the gate's teeth (drop the kernel assertion
//!   and this test goes green→red). The clean-bundle regression guard is the explicit
//!   `reason-verify` prerequisite of the dedicated `make coherence-gate-teeth` target, so
//!   the poisoned test does not repeat that clean chase. This is the literal
//!   whole-ontology teeth proof and it RUNS ON-GATE. It recovers the SAME object-level
//!   reasoning EDB as production before injecting the clash: documentation, mappings,
//!   correspondence, reports, and SHACL/ShEx validation-shape sidecars remain shipped
//!   but reasoner-invisible. The poisoned chase is still a whole-ontology operation, so
//!   it stays in the exhaustive architectural lane
//!   selected outside the default nextest profile. That separation is not gate exemption:
//!   `coherence-gate-teeth` invokes it explicitly with `--ignore-default-filter` and an
//!   `-E` selector and is wired into `make check` via `CHECK_TARGETS`. The minimal
//!   test above remains a fast,
//!   deterministic companion.

use gmeow_logic::foundation::{
    AntiRigidityPolicy, FoundationQuad, evaluate as foundation_evaluate,
};
use gmeow_logic::reason::dl_consistency;
use gmeow_logic::reasoning_graphs::is_object_level_named_graph;
use gmeow_logic::store::WorldStore;
use purrdf::{
    NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm, dataset_from_bytes, import_gts_events,
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

// ── Property-characteristic gate (H4) ────────────────────────────────────────────
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";
const LOGIC_IRREFLEXIVITY_VIOLATION: &str =
    "https://blackcatinformatics.ca/logic/IrreflexivityViolation";
const LOGIC_ASYMMETRY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/AsymmetryViolation";
// A synthetic world holding the whole bundle's characteristic schema + injected edges.
const CHAR_WORLD: &str = "https://blackcatinformatics.ca/gmeow/test/characteristic/world";
// Shipped properties whose characteristics the gate binds to (drop a declaration → red).
const GMEOW_SUB_EVENT_OF: &str = "https://blackcatinformatics.ca/gmeow/subEventOf";
const GMEOW_COUNTER_GOAL: &str = "https://blackcatinformatics.ca/gmeow/counterGoal";
const GMEOW_COUNTERPART_OF: &str = "https://blackcatinformatics.ca/gmeow/counterpartOf";
const GMEOW_COARSER_THAN: &str = "https://blackcatinformatics.ca/gmeow/coarserThan";
const GMEOW_SHARPENS: &str = "https://blackcatinformatics.ca/gmeow/sharpens";
const GMEOW_PART_OF: &str = "https://blackcatinformatics.ca/gmeow/partOf";
const GMEOW_VERSION_OF: &str = "https://blackcatinformatics.ca/gmeow/versionOf";
const GMEOW_EDITION_OF: &str = "https://blackcatinformatics.ca/gmeow/editionOf";
// Drift discipline: a DL-projectable logic: characteristic record whose OWL projection
// is missing (the two carriers of one characteristic have diverged).
const LOGIC_CARRIER_DISAGREEMENT: &str =
    "https://blackcatinformatics.ca/logic/CharacteristicCarrierDisagreement";

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

fn admitted_reasoning_graph(graph: &Option<RdfTerm>) -> bool {
    match graph {
        None => true,
        Some(RdfTerm::Iri(iri)) => is_object_level_named_graph(iri),
        Some(_) => false,
    }
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
    // Load the committed bundle exactly as production `reason-verify` does, then recover
    // its object-level EDB. Running DL closure over every shipped meta/report graph is
    // both semantically wrong and asymptotically tied to documentation growth.
    let gts_path = repo_root().join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&gts_path)
        .unwrap_or_else(|e| panic!("read committed bundle {}: {e}", gts_path.display()));
    let bundle = import_gts_events(&bytes).expect("import the committed gmeow.gts bundle");
    let snapshot = bundle.dataset;

    // Locate the SHIPPED gmeow:Agent ⊥ gmeow:SocialObject edge in the bundle and read the
    // world (named graph) it lives in. Finding it AT ALL proves the net-new production
    // edge is actually shipped — drop the kernel assertion and this lookup fails. We inject
    // NO owl:disjointWith of our own (a self-injected one would mask exactly that
    // regression). The DL disjointness rule is world-scoped, so the clashing individual
    // must be typed in the SAME world as the shipped edge.
    let disjoint_world = snapshot
        .owned_quads()
        .find_map(|q| {
            let is_edge = admitted_reasoning_graph(&q.graph_name)
                && q.predicate == OWL_DISJOINT_WITH
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
    for quad in snapshot.owned_quads() {
        if admitted_reasoning_graph(&quad.graph_name) {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in snapshot.owned_reifiers() {
        if admitted_reasoning_graph(&reifier.graph) {
            builder.push_owned_reifier(&reifier);
        }
    }
    for annotation in snapshot.owned_annotations() {
        if admitted_reasoning_graph(&annotation.graph) {
            builder.push_owned_annotation(&annotation);
        }
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
/// Named `whole_bundle_..._gate` and matched by the `coherence-gate-teeth` selector;
/// the whole-bundle chase is an exhaustive architectural proof selected explicitly
/// outside the default nextest profile.
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

/// Whether an IRI is a property-characteristic marker — the OWL characteristic classes
/// or their `logic:` analogues.
fn is_characteristic_marker(iri: &str) -> bool {
    matches!(
        iri,
        OWL_TRANSITIVE_PROPERTY
            | OWL_SYMMETRIC_PROPERTY
            | OWL_IRREFLEXIVE_PROPERTY
            | OWL_ASYMMETRIC_PROPERTY
            | OWL_FUNCTIONAL_PROPERTY
    ) || matches!(
        iri.strip_prefix(LOGIC_NS),
        Some("transitiveProperty")
            | Some("symmetricProperty")
            | Some("irreflexiveProperty")
            | Some("asymmetricProperty")
            | Some("functionalProperty")
    )
}

/// Project the committed bundle to the IRI-only fact set the characteristic pass needs,
/// world-scoped in [`CHAR_WORLD`]:
///
/// - each `?P rdf:type <characteristic marker>` type triple (owl: or logic:),
/// - each central record `?rec logic:characterizes ?P` / `?rec logic:characteristicSort ?sort`,
/// - every edge `?s ?P ?o` whose predicate ?P carries a characteristic, so the pass can
///   close/mirror it and detect a self- or mutual-pair clash.
///
/// The foundation chase is all-IRI, so literal- and blank-object triples are dropped.
fn project_characteristic_facts(onto: &purrdf::RdfDataset) -> BTreeSet<String> {
    // Pass 1: which predicates carry a characteristic (a marker on the property itself, or
    // a property named by a central record)?
    let mut characterized: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        if let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) {
            if q.predicate == RDF_TYPE && is_characteristic_marker(o) {
                characterized.insert(s.clone());
            } else if q.predicate == LOGIC_CHARACTERIZES {
                characterized.insert(o.clone());
            }
        }
    }
    // Pass 2: emit the markers, the record links, and the edges of characterized predicates.
    let mut lines: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        let emit = match q.predicate.as_str() {
            RDF_TYPE => is_characteristic_marker(o),
            LOGIC_CHARACTERIZES | LOGIC_CHARACTERISTIC_SORT => true,
            pred => characterized.contains(pred),
        };
        if emit {
            lines.insert(format!("<{s}> <{}> <{o}> <{CHAR_WORLD}> .\n", q.predicate));
        }
    }
    lines
}

/// Run the native foundation discipline over projected characteristic N-Quads.
fn run_characteristic_gate(nquads: &str) -> Vec<FoundationQuad> {
    let store = WorldStore::new();
    store
        .load_nquads(nquads)
        .expect("load the projected characteristic facts");
    foundation_evaluate(&store, AntiRigidityPolicy::WitnessObligation)
        .expect("foundation evaluate over the projected characteristic facts")
}

/// The subjects that fire a characteristic (irreflexivity/asymmetry) violation.
fn characteristic_violations(quads: &[FoundationQuad]) -> Vec<(String, String)> {
    let irr = format!("<{LOGIC_IRREFLEXIVITY_VIOLATION}>");
    let asym = format!("<{LOGIC_ASYMMETRY_VIOLATION}>");
    quads
        .iter()
        .filter(|q| q.predicate == LOGIC_VIOLATION && (q.object == irr || q.object == asym))
        .map(|q| (q.subject.clone(), q.object.clone()))
        .collect()
}

/// Whole-ontology property-characteristic gate. Projects the committed `gmeow.gts` to its
/// characteristic schema (the marker/record declarations + the edges of characterised
/// properties) and runs the native foundation discipline over it — the canonical `logic:`
/// enforcement of transitivity closure, symmetric mirroring, and irreflexivity/asymmetry
/// clashes, no longer confined to conformance fixtures.
///
/// HOLDS: the shipped ontology has ZERO characteristic violations. TEETH: over the shipped
/// declarations (gmeow:subEventOf transitive, gmeow:counterGoal symmetric) the gate closes
/// and mirrors injected edges; a fresh irreflexive self-loop and an asymmetric mutual pair
/// injected on top must each fire; and gmeow:counterpartOf — symmetric but deliberately not
/// transitive — is mirrored but never closed.
///
/// Named `whole_bundle_..._gate` and matched by the `coherence-gate-teeth` selector; the
/// whole-bundle chase is carved out of the budget-gated nextest profile by `default-filter`
/// (budget-exempt, not gate-exempt).
#[test]
fn whole_bundle_characteristic_gate_holds_and_has_teeth() {
    let gts_path = repo_root().join("generated/dist/gmeow.gts");
    let bytes = std::fs::read(&gts_path)
        .unwrap_or_else(|e| panic!("read committed bundle {}: {e}", gts_path.display()));
    let bundle = import_gts_events(&bytes).expect("import the committed gmeow.gts bundle");
    let facts = project_characteristic_facts(bundle.dataset.as_ref());
    let projection: String = facts.iter().cloned().collect();

    // Bind to production: every DL-projectable H4 target carries BOTH its OWL marker and
    // its canonical logic: record in the shipped bundle. Drop either carrier of any of
    // them and this test goes red — closing the dual-carrier silent-drift hole.
    let marker_fact =
        |prop: &str, marker: &str| format!("<{prop}> <{RDF_TYPE}> <{marker}> <{CHAR_WORLD}> .\n");
    let characterizes_fact = |rec: &str, prop: &str| {
        format!("<{rec}> <{LOGIC_CHARACTERIZES}> <{prop}> <{CHAR_WORLD}> .\n")
    };
    let sort_fact = |rec: &str, sort_local: &str| {
        format!("<{rec}> <{LOGIC_CHARACTERISTIC_SORT}> <{LOGIC_NS}{sort_local}> <{CHAR_WORLD}> .\n")
    };
    // (property, OWL marker, logic: record local name, characteristic-sort local name).
    let production: [(&str, &str, &str, &str); 8] = [
        (
            GMEOW_SUB_EVENT_OF,
            OWL_TRANSITIVE_PROPERTY,
            "subEventOfTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_COARSER_THAN,
            OWL_TRANSITIVE_PROPERTY,
            "coarserThanTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_SHARPENS,
            OWL_TRANSITIVE_PROPERTY,
            "sharpensTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_PART_OF,
            OWL_TRANSITIVE_PROPERTY,
            "partOfTransitivity",
            "transitiveProperty",
        ),
        (
            GMEOW_COUNTER_GOAL,
            OWL_SYMMETRIC_PROPERTY,
            "counterGoalSymmetry",
            "symmetricProperty",
        ),
        (
            GMEOW_COUNTERPART_OF,
            OWL_SYMMETRIC_PROPERTY,
            "counterpartOfSymmetry",
            "symmetricProperty",
        ),
        (
            GMEOW_VERSION_OF,
            OWL_FUNCTIONAL_PROPERTY,
            "versionOfFunctionality",
            "functionalProperty",
        ),
        (
            GMEOW_EDITION_OF,
            OWL_FUNCTIONAL_PROPERTY,
            "editionOfFunctionality",
            "functionalProperty",
        ),
    ];
    for (prop, marker, rec_local, sort_local) in production {
        let rec = format!("{LOGIC_NS}{rec_local}");
        assert!(
            facts.contains(&marker_fact(prop, marker)),
            "the committed gmeow.gts must declare {prop} with OWL characteristic {marker}"
        );
        assert!(
            facts.contains(&characterizes_fact(&rec, prop)),
            "the committed gmeow.gts must carry the logic: record {rec} characterizing {prop}"
        );
        assert!(
            facts.contains(&sort_fact(&rec, sort_local)),
            "the logic: record {rec} must assert characteristic sort logic:{sort_local}"
        );
    }
    // counterGoal irreflexivity is a logic:-only carrier (no OWL projection, DL-clean).
    let cg_irr = format!("{LOGIC_NS}counterGoalIrreflexivity");
    assert!(
        facts.contains(&characterizes_fact(&cg_irr, GMEOW_COUNTER_GOAL)),
        "the committed gmeow.gts must carry the logic:-only counterGoal irreflexivity record"
    );
    assert!(
        facts.contains(&sort_fact(&cg_irr, "irreflexiveProperty")),
        "the counterGoal irreflexivity record must assert logic:irreflexiveProperty"
    );

    // HOLDS: the shipped ontology satisfies its property characteristics.
    let clean = run_characteristic_gate(&projection);
    let clean_violations = characteristic_violations(&clean);
    assert!(
        clean_violations.is_empty(),
        "the committed gmeow.gts must satisfy its property characteristics, but the gate \
         found these irreflexivity/asymmetry violations: {clean_violations:#?}"
    );
    // HOLDS: no dual-carrier drift — every DL-projectable logic: characteristic record in
    // the shipped bundle has its OWL projection, so the agreement gate fires nothing.
    let disagreement_obj = format!("<{LOGIC_CARRIER_DISAGREEMENT}>");
    let clean_disagreements: Vec<String> = clean
        .iter()
        .filter(|q| q.predicate == LOGIC_VIOLATION && q.object == disagreement_obj)
        .map(|q| q.subject.clone())
        .collect();
    assert!(
        clean_disagreements.is_empty(),
        "the committed gmeow.gts must have zero characteristic-carrier disagreements, but \
         these properties carry a logic: record whose OWL projection is missing: \
         {clean_disagreements:#?}"
    );

    // TEETH: inject over the shipped declarations + two fresh violating properties.
    let t = "https://blackcatinformatics.ca/gmeow/test/characteristic";
    let irr_prop = format!("{t}/strictlyContains");
    let asym_prop = format!("{t}/strictlyBefore");
    let poisoned = format!(
        "{projection}\
         <{t}/A> <{GMEOW_SUB_EVENT_OF}> <{t}/B> <{CHAR_WORLD}> .\n\
         <{t}/B> <{GMEOW_SUB_EVENT_OF}> <{t}/C> <{CHAR_WORLD}> .\n\
         <{t}/M> <{GMEOW_COUNTER_GOAL}> <{t}/N> <{CHAR_WORLD}> .\n\
         <{t}/X> <{GMEOW_COUNTERPART_OF}> <{t}/Y> <{CHAR_WORLD}> .\n\
         <{t}/Y> <{GMEOW_COUNTERPART_OF}> <{t}/Z> <{CHAR_WORLD}> .\n\
         <{irr_prop}> <{RDF_TYPE}> <{OWL_IRREFLEXIVE_PROPERTY}> <{CHAR_WORLD}> .\n\
         <{t}/self> <{irr_prop}> <{t}/self> <{CHAR_WORLD}> .\n\
         <{asym_prop}> <{RDF_TYPE}> <{OWL_ASYMMETRIC_PROPERTY}> <{CHAR_WORLD}> .\n\
         <{t}/P> <{asym_prop}> <{t}/Q> <{CHAR_WORLD}> .\n\
         <{t}/Q> <{asym_prop}> <{t}/P> <{CHAR_WORLD}> .\n\
         <{t}/driftRec> <{LOGIC_CHARACTERIZES}> <{t}/driftProp> <{CHAR_WORLD}> .\n\
         <{t}/driftRec> <{LOGIC_CHARACTERISTIC_SORT}> <{LOGIC_NS}transitiveProperty> <{CHAR_WORLD}> .\n"
    );
    let out = run_characteristic_gate(&poisoned);
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
