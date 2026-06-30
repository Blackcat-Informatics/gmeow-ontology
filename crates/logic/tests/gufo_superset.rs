// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Native `gmeow:logic ⊇ gUFO` coverage floor — the `meta:gate-logic-gufo-superset`
//! gate (Principle 17), ported from the retired Python fixture
//! `tests/test_logic_gufo_superset.py` (#731).
//!
//! gUFO is a generated, VALIDATION-ONLY lossy down-projection of the canonical
//! `gmeow:logic` foundation: every gUFO `owl:Class` must therefore be covered by a
//! richer `logic:` term, or be explicitly SUPERSEDED by the `logic:Fluent` + RDF-1.2
//! edge-property pattern (the five temporary-situation reifiers). This is the honest
//! floor that enforces it, checked against the live sources
//! `imports/gufo.ttl` + `slices/core/logic/module.ttl` + the worked example
//! `slices/core/logic/examples/criticism-fixes.ttl`.
//!
//! Replaces the Python `_GUFO_CLASS_TO_LOGIC` fixture inlined when the Python
//! compiler was deleted in #727. The 11-stereotype *runtime* sort map stays in
//! `crates/logic/src/compile/adapter.rs`; this test owns the full *coverage* floor.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;

use gmeow_rdf::{parse_dataset, RdfQuad, RdfTerm};

// --------------------------------------------------------------------------- //
// Namespaces (mirror `adapter.rs` constants + the data files this gate reads).
// --------------------------------------------------------------------------- //
const GUFO_NS: &str = "http://purl.org/nemo/gufo#";
/// `logic:` foundation namespace (see `criticism-fixes.ttl` / `module.ttl` `@prefix`).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// Worked-example A-Box namespace (`@prefix ex:` in `criticism-fixes.ttl`).
const EX_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";

// Constant IRI caches for frequently-used predicates and classes.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
const GRAPHBOXROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";

// logic: term IRIs
static LOGIC_FLUENT: LazyLock<String> = LazyLock::new(|| logic_iri("Fluent"));
static LOGIC_PROPER_PART_OF: LazyLock<String> = LazyLock::new(|| logic_iri("properPartOf"));
static LOGIC_INSTANCE_OF: LazyLock<String> = LazyLock::new(|| logic_iri("instanceOf"));
static LOGIC_ORDERED_TYPE: LazyLock<String> = LazyLock::new(|| logic_iri("orderedType"));
static LOGIC_INVOKES_BUILTIN: LazyLock<String> = LazyLock::new(|| logic_iri("invokesBuiltin"));
static LOGIC_BUILTIN: LazyLock<String> = LazyLock::new(|| logic_iri("Builtin"));
static LOGIC_TRANSITIVE_PROPERTY: LazyLock<String> =
    LazyLock::new(|| logic_iri("transitiveProperty"));
static LOGIC_ASYMMETRIC_PROPERTY: LazyLock<String> =
    LazyLock::new(|| logic_iri("asymmetricProperty"));
static LOGIC_IRREFLEXIVE_PROPERTY: LazyLock<String> =
    LazyLock::new(|| logic_iri("irreflexiveProperty"));

fn gufo_iri(local: &str) -> String {
    format!("{GUFO_NS}{local}")
}
fn logic_iri(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}
fn ex_iri(local: &str) -> String {
    format!("{EX_NS}{local}")
}

/// An IRI object term (the `RdfTerm` form of a `logic:`/`owl:`/`rdf:` class).
fn iri_term(iri: &str) -> RdfTerm {
    RdfTerm::iri(iri.to_owned())
}

// --------------------------------------------------------------------------- //
// The `gmeow:logic ⊇ gUFO` coverage map.
//
// Every gUFO `owl:Class` (local name) → the richer `logic:` term that subsumes it
// (`Target::Logic`), or `Target::Superseded` for the five temporary-situation
// reifiers replaced by the `logic:Fluent` + RDF-1.2 edge-property pattern.
//
// Provenance: Principle 17 (`gmeow:logic` is canonical; gUFO is its lossy
// down-projection). Keys are the gUFO classes of `imports/gufo.ttl` PLUS the
// map-only extra `Disposition` (not a stock gUFO class — gUFO models it as an
// IntrinsicMode; `logic:` carries it first-class). Targets are `logic:` terms
// declared in `slices/core/logic/module.ttl`.
//
// The map is MANY-TO-ONE: 50 entries = 45 `Logic` + 5 `Superseded`; the 45 `Logic`
// targets dedupe to 40 DISTINCT `logic:` IRIs (e.g. `Aspect`/`IntrinsicAspect`/
// `ExtrinsicAspect` → `Aspect`; `IntrinsicMode`/`ExtrinsicMode` → `Mode`;
// `EventType` → `Event`; `SituationType` → `Situation`). A2/A4 dedupe before
// checking — never assume one key = one target.
// --------------------------------------------------------------------------- //
enum Target {
    /// Faithful correspondence — the gUFO class is subsumed by this `logic:` local name.
    Logic(&'static str),
    /// Deliberately replaced by `logic:Fluent` + RDF-1.2 edge properties (not a 1:1 target).
    Superseded,
}

use Target::{Logic, Superseded};

const GUFO_CLASS_TO_LOGIC: &[(&str, Target)] = &[
    // --- Top of the individual taxonomy ---
    ("Individual", Logic("Individual")),
    ("ConcreteIndividual", Logic("ConcreteIndividual")),
    ("AbstractIndividual", Logic("AbstractIndividual")),
    // --- Endurants / perdurants / situations ---
    ("Endurant", Logic("Endurant")),
    ("Event", Logic("Event")),
    ("Situation", Logic("Situation")),
    ("Participation", Logic("Participation")),
    // --- Endurant subkinds: objects vs aspects ---
    ("Object", Logic("Object")),
    ("Aspect", Logic("Aspect")),
    ("IntrinsicAspect", Logic("Aspect")),
    ("ExtrinsicAspect", Logic("Aspect")),
    ("IntrinsicMode", Logic("Mode")),
    ("ExtrinsicMode", Logic("Mode")),
    // Disposition: map-only extra (not in stock gUFO; logic: carries it first-class).
    ("Disposition", Logic("Disposition")),
    ("Quality", Logic("Quality")),
    ("QualityValue", Logic("QualityValue")),
    ("Relator", Logic("Relator")),
    // --- Object aggregation kinds ---
    ("Collection", Logic("Collection")),
    ("FixedCollection", Logic("FixedCollection")),
    ("VariableCollection", Logic("VariableCollection")),
    ("Quantity", Logic("Quantity")),
    ("FunctionalComplex", Logic("FunctionalComplex")),
    // --- Type level (higher-order) ---
    ("Type", Logic("Type")),
    ("EndurantType", Logic("EndurantType")),
    ("RelationshipType", Logic("RelationshipType")),
    (
        "MaterialRelationshipType",
        Logic("MaterialRelationshipType"),
    ),
    (
        "ComparativeRelationshipType",
        Logic("ComparativeRelationshipType"),
    ),
    ("AbstractIndividualType", Logic("AbstractIndividualType")),
    ("ConcreteIndividualType", Logic("ConcreteIndividualType")),
    ("EventType", Logic("Event")),
    ("SituationType", Logic("Situation")),
    // --- Endurant-type meta axes (sortality / rigidity) ---
    ("Sortal", Logic("Sortal")),
    ("NonSortal", Logic("NonSortal")),
    ("RigidType", Logic("RigidType")),
    ("AntiRigidType", Logic("AntiRigidType")),
    ("SemiRigidType", Logic("SemiRigidType")),
    ("NonRigidType", Logic("NonRigidType")),
    // --- The OntoUML stereotypes ---
    ("Kind", Logic("Kind")),
    ("SubKind", Logic("SubKind")),
    ("Phase", Logic("Phase")),
    ("Role", Logic("Role")),
    ("Category", Logic("Category")),
    ("Mixin", Logic("Mixin")),
    ("RoleMixin", Logic("RoleMixin")),
    ("PhaseMixin", Logic("PhaseMixin")),
    // --- Superseded temporary-situation reifiers (logic:Fluent + RDF-1.2) ---
    ("QualityValueAttributionSituation", Superseded),
    ("TemporaryConstitutionSituation", Superseded),
    ("TemporaryInstantiationSituation", Superseded),
    ("TemporaryParthoodSituation", Superseded),
    ("TemporaryRelationshipSituation", Superseded),
];

/// The five gUFO temporary-situation reifiers `gmeow:logic` deliberately replaces
/// with `logic:Fluent` + RDF-1.2 edge properties. Pinned exactly so an accidental
/// over-supersession (mapping a faithfully-coverable class to SUPERSEDED) fails.
const EXPECTED_SUPERSEDED: &[&str] = &[
    "QualityValueAttributionSituation",
    "TemporaryConstitutionSituation",
    "TemporaryInstantiationSituation",
    "TemporaryParthoodSituation",
    "TemporaryRelationshipSituation",
];

/// The distinct, non-SUPERSEDED `logic:` target IRIs the map covers (deduped —
/// mirrors the Python `_non_superseded_targets()` set).
fn non_superseded_targets() -> HashSet<String> {
    GUFO_CLASS_TO_LOGIC
        .iter()
        .filter_map(|(_, t)| match t {
            Logic(local) => Some(logic_iri(local)),
            Superseded => None,
        })
        .collect()
}

/// Pre-built deduped non-superseded target set (avoids re-building per test).
static NON_SUPERSEDED: LazyLock<HashSet<String>> = LazyLock::new(non_superseded_targets);

// --------------------------------------------------------------------------- //
// Fixtures: load a repo-relative Turtle file into a flat quad list. HARD-FAIL
// (panic) on missing/unparsable input — NO-OPTIONALITY: a missing source file is
// a build error, not a silently-skipped test (mirrors `ontology_entailments.rs`).
// Three sources are pure Turtle (default-graph triples).
// --------------------------------------------------------------------------- //

/// Repo root (`CARGO_MANIFEST_DIR` = `<repo>/crates/logic`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Parse a repo-relative Turtle file natively and snapshot its quads (graph-flat).
fn load_store(rel: &str) -> Vec<RdfQuad> {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("missing ontology source {}: {e}", path.display()));
    let dataset = parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
    dataset.owned_quads().collect()
}

const GUFO_TTL: &str = "imports/gufo.ttl";
const MODULE_TTL: &str = "slices/core/logic/module.ttl";
const EXAMPLE_TTL: &str = "slices/core/logic/examples/criticism-fixes.ttl";

/// Lazily-loaded, shared stores — parsed exactly once per test binary run.
static GUFO_STORE: LazyLock<Vec<RdfQuad>> = LazyLock::new(|| load_store(GUFO_TTL));
static MODULE_STORE: LazyLock<Vec<RdfQuad>> = LazyLock::new(|| load_store(MODULE_TTL));
static EXAMPLE_STORE: LazyLock<Vec<RdfQuad>> = LazyLock::new(|| load_store(EXAMPLE_TTL));

// --------------------------------------------------------------------------- //
// Typed helper functions.
//
// All sources are graph-flat (default graph): a quad with `graph_name == None`.
// The helpers filter the snapshot vec, mirroring the prior oxigraph
// `quads_for_pattern(..., DefaultGraph)` queries.
// --------------------------------------------------------------------------- //

/// The IRI string of a term, if it is an IRI.
fn term_iri(t: &RdfTerm) -> Option<&str> {
    match t {
        RdfTerm::Iri(iri) => Some(iri.as_str()),
        _ => None,
    }
}

/// Whether a quad lives in the default graph (the only graph these sources use).
fn in_default_graph(q: &RdfQuad) -> bool {
    q.graph_name.is_none()
}

/// All subjects of `(*, predicate, object)` (named subjects only; skip blank nodes).
fn subjects_with(store: &[RdfQuad], predicate: &str, object: &RdfTerm) -> Vec<String> {
    store
        .iter()
        .filter(|q| in_default_graph(q) && q.predicate == predicate && &q.object == object)
        .filter_map(|q| term_iri(&q.subject).map(str::to_owned))
        .collect()
}

/// All objects of `(subject, predicate, *)`.
fn objects_of(store: &[RdfQuad], subject: &str, predicate: &str) -> Vec<RdfTerm> {
    store
        .iter()
        .filter(|q| {
            in_default_graph(q) && q.predicate == predicate && term_iri(&q.subject) == Some(subject)
        })
        .map(|q| q.object.clone())
        .collect()
}

/// All `(subject_iri, object)` pairs for predicate `p` (named subjects only).
fn pairs_of(store: &[RdfQuad], predicate: &str) -> Vec<(String, RdfTerm)> {
    store
        .iter()
        .filter(|q| in_default_graph(q) && q.predicate == predicate)
        .filter_map(|q| term_iri(&q.subject).map(|s| (s.to_owned(), q.object.clone())))
        .collect()
}

/// Whether `(subject, predicate, object)` is present.
fn has_object(store: &[RdfQuad], subject: &str, predicate: &str, object: &RdfTerm) -> bool {
    store.iter().any(|q| {
        in_default_graph(q)
            && q.predicate == predicate
            && term_iri(&q.subject) == Some(subject)
            && &q.object == object
    })
}

/// Whether subject has ANY object for the given predicate (existence probe).
fn has_any_object(store: &[RdfQuad], subject: &str, predicate: &str) -> bool {
    store.iter().any(|q| {
        in_default_graph(q) && q.predicate == predicate && term_iri(&q.subject) == Some(subject)
    })
}

/// Every `owl:Class` IRI in the gUFO namespace declared in `imports/gufo.ttl`.
fn gufo_classes(store: &[RdfQuad]) -> Vec<String> {
    let mut classes: Vec<String> = subjects_with(store, RDF_TYPE, &iri_term(OWL_CLASS))
        .into_iter()
        .filter(|n| n.starts_with(GUFO_NS))
        .collect();
    classes.sort();
    classes.dedup();
    classes
}

/// All distinct named subjects in a store.
fn all_subjects(store: &[RdfQuad]) -> HashSet<String> {
    store
        .iter()
        .filter(|q| in_default_graph(q))
        .filter_map(|q| term_iri(&q.subject).map(str::to_owned))
        .collect()
}

// --------------------------------------------------------------------------- //
// (A1) Every gUFO class has a correspondence — the minimum-baseline floor.
// --------------------------------------------------------------------------- //

#[test]
fn every_gufo_class_has_logic_correspondence() {
    let classes = gufo_classes(&GUFO_STORE);
    assert!(
        !classes.is_empty(),
        "No gUFO owl:Class declarations found in {GUFO_TTL}"
    );

    let keys: HashSet<String> = GUFO_CLASS_TO_LOGIC
        .iter()
        .map(|(k, _)| gufo_iri(k))
        .collect();
    let mut missing: Vec<&String> = classes.iter().filter(|c| !keys.contains(*c)).collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "gmeow:logic ⊇ gUFO floor BREACHED — these gUFO classes have NO entry in the \
         GUFO_CLASS_TO_LOGIC map (add a faithful logic: target or Superseded):\n  {}",
        missing
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// --------------------------------------------------------------------------- //
// (A2) Correspondence targets actually exist in the module.
// --------------------------------------------------------------------------- //

#[test]
fn correspondence_targets_exist() {
    let subjects = all_subjects(&MODULE_STORE);
    let mut missing: Vec<String> = NON_SUPERSEDED
        .iter()
        .filter(|t| !subjects.contains(*t))
        .cloned()
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "These GUFO_CLASS_TO_LOGIC targets are NOT declared as subjects in {MODULE_TTL} — \
         the correspondence is dangling:\n  {}",
        missing
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// --------------------------------------------------------------------------- //
// (A3) The SUPERSEDED set is exactly the five reifiers.
// --------------------------------------------------------------------------- //

#[test]
fn superseded_set_is_the_five_reifiers() {
    let actual: HashSet<&str> = GUFO_CLASS_TO_LOGIC
        .iter()
        .filter_map(|(k, t)| matches!(t, Superseded).then_some(*k))
        .collect();
    let expected: HashSet<&str> = EXPECTED_SUPERSEDED.iter().copied().collect();

    let mut over: Vec<&&str> = actual.difference(&expected).collect();
    let mut under: Vec<&&str> = expected.difference(&actual).collect();
    over.sort();
    under.sort();
    assert!(
        actual == expected,
        "SUPERSEDED set drift.\n  unexpected (over-supersession): {over:?}\n  \
         missing (should be superseded): {under:?}"
    );
}

// --------------------------------------------------------------------------- //
// (A4) Every correspondence target carries a graphBoxRole.
// --------------------------------------------------------------------------- //

#[test]
fn new_logic_terms_carry_graphbox_role() {
    let mut no_role: Vec<String> = NON_SUPERSEDED
        .iter()
        .filter(|t| !has_any_object(&MODULE_STORE, t, GRAPHBOXROLE))
        .cloned()
        .collect();
    no_role.sort();
    assert!(
        no_role.is_empty(),
        "These GUFO_CLASS_TO_LOGIC targets lack a gmeow:graphBoxRole annotation in \
         {MODULE_TTL} — add one rather than weakening the gate:\n  {}",
        no_role
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// --------------------------------------------------------------------------- //
// (B) Worked example — criticism-fixes.ttl parses and shows the four patterns.
// --------------------------------------------------------------------------- //

#[test]
fn criticism_example_parses() {
    assert!(
        EXAMPLE_STORE.iter().any(in_default_graph),
        "worked example {EXAMPLE_TTL} parsed empty"
    );
}

#[test]
fn criticism_example_has_native_edge_property() {
    // §1 triple-bloat fix: an RDF-1.2 reifier typed logic:Fluent carrying the quoted
    // (subject, predicate, object) and validFrom/validTo edge metadata.
    let fluents: HashSet<String> =
        subjects_with(&EXAMPLE_STORE, RDF_TYPE, &iri_term(&LOGIC_FLUENT))
            .into_iter()
            .collect();
    let statements: HashSet<String> =
        subjects_with(&EXAMPLE_STORE, RDF_TYPE, &iri_term(RDF_STATEMENT))
            .into_iter()
            .collect();
    let mut reifiers: Vec<&String> = fluents.intersection(&statements).collect();
    reifiers.sort();
    assert!(
        !reifiers.is_empty(),
        "no rdf:Statement + logic:Fluent reifier found in {EXAMPLE_TTL}"
    );
    let reifier = reifiers[0];

    // Quotes a full (subject, predicate, object) triple term.
    for pred in [RDF_SUBJECT, RDF_PREDICATE, RDF_OBJECT] {
        assert!(
            !objects_of(&EXAMPLE_STORE, reifier, pred).is_empty(),
            "reifier {reifier} is missing a {pred} quoted-triple component"
        );
    }
    // Carries LITERAL validFrom/validTo edge metadata (isinstance(o, Literal) parity).
    for (pred_iri, name) in [
        (ex_iri("validFrom"), "validFrom"),
        (ex_iri("validTo"), "validTo"),
    ] {
        let has_literal = objects_of(&EXAMPLE_STORE, reifier, &pred_iri)
            .iter()
            .any(|o| matches!(o, RdfTerm::Literal(_)));
        assert!(
            has_literal,
            "reifier {reifier} carries no literal {name} edge metadata"
        );
    }
}

#[test]
fn criticism_example_has_strict_partial_order() {
    // §2 OWL-2 global-restriction fix: logic:properPartOf used in the example, and the
    // module types it transitive ∧ asymmetric ∧ irreflexive at once (illegal in OWL 2).
    let chain = pairs_of(&EXAMPLE_STORE, &LOGIC_PROPER_PART_OF);
    assert!(
        chain.len() >= 2,
        "expected a logic:properPartOf chain (>= 2 edges) in {EXAMPLE_TTL}, found {}",
        chain.len()
    );

    let chars: HashSet<String> = objects_of(&MODULE_STORE, &LOGIC_PROPER_PART_OF, RDF_TYPE)
        .into_iter()
        .filter_map(|o| match o {
            RdfTerm::Iri(n) => Some(n),
            _ => None,
        })
        .collect();
    for required in [
        &*LOGIC_TRANSITIVE_PROPERTY,
        &*LOGIC_ASYMMETRIC_PROPERTY,
        &*LOGIC_IRREFLEXIVE_PROPERTY,
    ] {
        assert!(
            chars.contains(required),
            "logic:properPartOf is not typed logic:{} in the module — the \
             strict-partial-order characteristic is missing",
            required.trim_start_matches(LOGIC_NS)
        );
    }
}

#[test]
fn criticism_example_has_multilevel_instance_chain() {
    // §3 no-punning fix: a logic:instanceOf chain where a type is itself an instance of
    // a higher-order type, with logic:orderedType levels.
    let inst = pairs_of(&EXAMPLE_STORE, &LOGIC_INSTANCE_OF);
    let subjects: HashSet<&String> = inst.iter().map(|(s, _)| s).collect();
    // A two-step chain: an object that is itself a subject (marv -> goldenEagle -> species).
    let has_bridge = inst.iter().any(|(_, o)| match o {
        RdfTerm::Iri(n) => subjects.contains(n),
        _ => false,
    });
    assert!(
        has_bridge,
        "no multi-level chain: need x logic:instanceOf y and y logic:instanceOf z"
    );
    // logic:orderedType levels are recorded.
    let has_levels = EXAMPLE_STORE
        .iter()
        .any(|q| in_default_graph(q) && q.predicate == *LOGIC_ORDERED_TYPE);
    assert!(has_levels, "no logic:orderedType levels recorded");
}

#[test]
fn criticism_example_references_builtin() {
    // §4 builtin-derived value: a derivation references a logic:Builtin individual via
    // logic:invokesBuiltin; the target must be declared a logic:Builtin in the module.
    let invocations = pairs_of(&EXAMPLE_STORE, &LOGIC_INVOKES_BUILTIN);
    assert!(
        !invocations.is_empty(),
        "no logic:invokesBuiltin edge found in {EXAMPLE_TTL}"
    );

    let builtin_type_term = iri_term(&LOGIC_BUILTIN);
    for (_subj, builtin) in &invocations {
        match builtin {
            RdfTerm::Iri(builtin_iri) => {
                assert!(
                    has_object(&MODULE_STORE, builtin_iri, RDF_TYPE, &builtin_type_term),
                    "{builtin_iri} is not declared a logic:Builtin in {MODULE_TTL}"
                );
            }
            _ => panic!("logic:invokesBuiltin target is not an IRI: {builtin:?}"),
        }
    }
}
