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

use oxigraph::model::{GraphNameRef, NamedNode, Term};
use oxigraph::store::Store;

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::parse_dataset;

// --------------------------------------------------------------------------- //
// Namespaces (mirror `adapter.rs` constants + the data files this gate reads).
// --------------------------------------------------------------------------- //
const GUFO_NS: &str = "http://purl.org/nemo/gufo#";
/// `logic:` foundation namespace (see `criticism-fixes.ttl` / `module.ttl` `@prefix`).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// Worked-example A-Box namespace (`@prefix ex:` in `criticism-fixes.ttl`).
const EX_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";

// Constant NamedNode caches for frequently-used predicates and classes.
static RDF_TYPE: LazyLock<NamedNode> =
    LazyLock::new(|| nn("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"));
static OWL_CLASS: LazyLock<NamedNode> = LazyLock::new(|| nn("http://www.w3.org/2002/07/owl#Class"));
static RDF_STATEMENT: LazyLock<NamedNode> =
    LazyLock::new(|| nn("http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement"));
static RDF_SUBJECT: LazyLock<NamedNode> =
    LazyLock::new(|| nn("http://www.w3.org/1999/02/22-rdf-syntax-ns#subject"));
static RDF_PREDICATE: LazyLock<NamedNode> =
    LazyLock::new(|| nn("http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate"));
static RDF_OBJECT: LazyLock<NamedNode> =
    LazyLock::new(|| nn("http://www.w3.org/1999/02/22-rdf-syntax-ns#object"));
static GRAPHBOXROLE: LazyLock<NamedNode> =
    LazyLock::new(|| nn("https://blackcatinformatics.ca/gmeow/graphBoxRole"));

// logic: term NamedNodes
static LOGIC_FLUENT: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("Fluent"));
static LOGIC_PROPER_PART_OF: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("properPartOf"));
static LOGIC_INSTANCE_OF: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("instanceOf"));
static LOGIC_ORDERED_TYPE: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("orderedType"));
static LOGIC_INVOKES_BUILTIN: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("invokesBuiltin"));
static LOGIC_BUILTIN: LazyLock<NamedNode> = LazyLock::new(|| logic_nn("Builtin"));
static LOGIC_TRANSITIVE_PROPERTY: LazyLock<NamedNode> =
    LazyLock::new(|| logic_nn("transitiveProperty"));
static LOGIC_ASYMMETRIC_PROPERTY: LazyLock<NamedNode> =
    LazyLock::new(|| logic_nn("asymmetricProperty"));
static LOGIC_IRREFLEXIVE_PROPERTY: LazyLock<NamedNode> =
    LazyLock::new(|| logic_nn("irreflexiveProperty"));

/// Build a NamedNode; panics on invalid IRI (programming error).
fn nn(iri: &str) -> NamedNode {
    NamedNode::new(iri).unwrap_or_else(|e| panic!("invalid IRI {iri:?}: {e}"))
}

fn gufo_nn(local: &str) -> NamedNode {
    nn(&format!("{GUFO_NS}{local}"))
}
fn logic_nn(local: &str) -> NamedNode {
    nn(&format!("{LOGIC_NS}{local}"))
}
fn ex_nn(local: &str) -> NamedNode {
    nn(&format!("{EX_NS}{local}"))
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

/// The distinct, non-SUPERSEDED `logic:` target NamedNodes the map covers (deduped —
/// mirrors the Python `_non_superseded_targets()` set).
fn non_superseded_targets() -> HashSet<NamedNode> {
    GUFO_CLASS_TO_LOGIC
        .iter()
        .filter_map(|(_, t)| match t {
            Logic(local) => Some(logic_nn(local)),
            Superseded => None,
        })
        .collect()
}

/// Pre-built deduped non-superseded target set (avoids re-building per test).
static NON_SUPERSEDED: LazyLock<HashSet<NamedNode>> = LazyLock::new(non_superseded_targets);

// --------------------------------------------------------------------------- //
// Fixtures: load a repo-relative Turtle file into an oxigraph Store. HARD-FAIL
// (panic) on missing/unparsable input — NO-OPTIONALITY: a missing source file is
// a build error, not a silently-skipped test (mirrors `ontology_entailments.rs`).
// Three sources are pure Turtle.
// --------------------------------------------------------------------------- //

/// Repo root (`CARGO_MANIFEST_DIR` = `<repo>/crates/logic`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_store(rel: &str) -> Store {
    let path = repo_root().join(rel);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("missing ontology source {}: {e}", path.display()));
    let dataset = parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
    store_from_dataset(dataset.as_ref(), GraphPolicy::PreserveNamedGraphs)
        .unwrap_or_else(|e| panic!("failed to materialize {}: {e}", path.display()))
}

const GUFO_TTL: &str = "imports/gufo.ttl";
const MODULE_TTL: &str = "slices/core/logic/module.ttl";
const EXAMPLE_TTL: &str = "slices/core/logic/examples/criticism-fixes.ttl";

/// Lazily-loaded, shared stores — parsed exactly once per test binary run.
static GUFO_STORE: LazyLock<Store> = LazyLock::new(|| load_store(GUFO_TTL));
static MODULE_STORE: LazyLock<Store> = LazyLock::new(|| load_store(MODULE_TTL));
static EXAMPLE_STORE: LazyLock<Store> = LazyLock::new(|| load_store(EXAMPLE_TTL));

// --------------------------------------------------------------------------- //
// Typed helper functions.
// --------------------------------------------------------------------------- //

/// All subjects of `(*, predicate, object)` (named subjects only; skip blank nodes).
fn subjects_with(store: &Store, predicate: &NamedNode, object: &Term) -> Vec<NamedNode> {
    store
        .quads_for_pattern(
            None,
            Some(predicate.as_ref()),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .filter_map(|q| match q.subject {
            oxigraph::model::NamedOrBlankNode::NamedNode(n) => Some(n),
            oxigraph::model::NamedOrBlankNode::BlankNode(_) => None,
        })
        .collect()
}

/// All objects of `(subject, predicate, *)`.
fn objects_of(store: &Store, subject: &NamedNode, predicate: &NamedNode) -> Vec<Term> {
    store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .map(|q| q.object)
        .collect()
}

/// All `(subject_NamedNode, object)` pairs for predicate `p`.
fn pairs_of(store: &Store, predicate: &NamedNode) -> Vec<(NamedNode, Term)> {
    store
        .quads_for_pattern(
            None,
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .filter_map(|q| match q.subject {
            oxigraph::model::NamedOrBlankNode::NamedNode(n) => Some((n, q.object)),
            oxigraph::model::NamedOrBlankNode::BlankNode(_) => None,
        })
        .collect()
}

/// Whether `(subject, predicate, object)` is present (uses quads_for_pattern(...).next().is_some()).
fn has_object(store: &Store, subject: &NamedNode, predicate: &NamedNode, object: &Term) -> bool {
    store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some()
}

/// Whether subject has ANY object for the given predicate (existence probe).
fn has_any_object(store: &Store, subject: &NamedNode, predicate: &NamedNode) -> bool {
    store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some()
}

/// Every `owl:Class` NamedNode in the gUFO namespace declared in `imports/gufo.ttl`.
fn gufo_classes(store: &Store) -> Vec<NamedNode> {
    let mut classes: Vec<NamedNode> =
        subjects_with(store, &RDF_TYPE, &Term::NamedNode((*OWL_CLASS).clone()))
            .into_iter()
            .filter(|n| n.as_str().starts_with(GUFO_NS))
            .collect();
    classes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    classes.dedup();
    classes
}

/// All distinct named subjects in a store.
fn all_subjects(store: &Store) -> HashSet<NamedNode> {
    store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .filter_map(|q| match q.subject {
            oxigraph::model::NamedOrBlankNode::NamedNode(n) => Some(n),
            oxigraph::model::NamedOrBlankNode::BlankNode(_) => None,
        })
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

    let keys: HashSet<NamedNode> = GUFO_CLASS_TO_LOGIC
        .iter()
        .map(|(k, _)| gufo_nn(k))
        .collect();
    let mut missing: Vec<&NamedNode> = classes.iter().filter(|c| !keys.contains(*c)).collect();
    missing.sort_by(|a, b| a.as_str().cmp(b.as_str()));
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
    let mut missing: Vec<NamedNode> = NON_SUPERSEDED
        .iter()
        .filter(|t| !subjects.contains(*t))
        .cloned()
        .collect();
    missing.sort_by(|a, b| a.as_str().cmp(b.as_str()));
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
    let mut no_role: Vec<NamedNode> = NON_SUPERSEDED
        .iter()
        .filter(|t| !has_any_object(&MODULE_STORE, t, &GRAPHBOXROLE))
        .cloned()
        .collect();
    no_role.sort_by(|a, b| a.as_str().cmp(b.as_str()));
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
        EXAMPLE_STORE
            .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
            .next()
            .is_some(),
        "worked example {EXAMPLE_TTL} parsed empty"
    );
}

#[test]
fn criticism_example_has_native_edge_property() {
    // §1 triple-bloat fix: an RDF-1.2 reifier typed logic:Fluent carrying the quoted
    // (subject, predicate, object) and validFrom/validTo edge metadata.
    let fluents: HashSet<NamedNode> = subjects_with(
        &EXAMPLE_STORE,
        &RDF_TYPE,
        &Term::NamedNode((*LOGIC_FLUENT).clone()),
    )
    .into_iter()
    .collect();
    let statements: HashSet<NamedNode> = subjects_with(
        &EXAMPLE_STORE,
        &RDF_TYPE,
        &Term::NamedNode((*RDF_STATEMENT).clone()),
    )
    .into_iter()
    .collect();
    let mut reifiers: Vec<&NamedNode> = fluents.intersection(&statements).collect();
    reifiers.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    assert!(
        !reifiers.is_empty(),
        "no rdf:Statement + logic:Fluent reifier found in {EXAMPLE_TTL}"
    );
    let reifier = reifiers[0];

    // Quotes a full (subject, predicate, object) triple term.
    for pred in [&*RDF_SUBJECT, &*RDF_PREDICATE, &*RDF_OBJECT] {
        assert!(
            !objects_of(&EXAMPLE_STORE, reifier, pred).is_empty(),
            "reifier {} is missing a {} quoted-triple component",
            reifier.as_str(),
            pred.as_str()
        );
    }
    // Carries LITERAL validFrom/validTo edge metadata (isinstance(o, Literal) parity).
    for (pred_nn, name) in [
        (ex_nn("validFrom"), "validFrom"),
        (ex_nn("validTo"), "validTo"),
    ] {
        let has_literal = objects_of(&EXAMPLE_STORE, reifier, &pred_nn)
            .iter()
            .any(|o| matches!(o, Term::Literal(_)));
        assert!(
            has_literal,
            "reifier {} carries no literal {name} edge metadata",
            reifier.as_str()
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

    let chars: HashSet<NamedNode> = objects_of(&MODULE_STORE, &LOGIC_PROPER_PART_OF, &RDF_TYPE)
        .into_iter()
        .filter_map(|o| match o {
            Term::NamedNode(n) => Some(n),
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
            required.as_str().trim_start_matches(LOGIC_NS)
        );
    }
}

#[test]
fn criticism_example_has_multilevel_instance_chain() {
    // §3 no-punning fix: a logic:instanceOf chain where a type is itself an instance of
    // a higher-order type, with logic:orderedType levels.
    let inst = pairs_of(&EXAMPLE_STORE, &LOGIC_INSTANCE_OF);
    let subjects: HashSet<&NamedNode> = inst.iter().map(|(s, _)| s).collect();
    // A two-step chain: an object that is itself a subject (marv -> goldenEagle -> species).
    let has_bridge = inst.iter().any(|(_, o)| match o {
        Term::NamedNode(n) => subjects.contains(n),
        _ => false,
    });
    assert!(
        has_bridge,
        "no multi-level chain: need x logic:instanceOf y and y logic:instanceOf z"
    );
    // logic:orderedType levels are recorded.
    let has_levels = EXAMPLE_STORE
        .quads_for_pattern(
            None,
            Some((*LOGIC_ORDERED_TYPE).as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some();
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

    let builtin_type_term = Term::NamedNode((*LOGIC_BUILTIN).clone());
    for (_subj, builtin) in &invocations {
        match builtin {
            Term::NamedNode(builtin_nn) => {
                assert!(
                    has_object(&MODULE_STORE, builtin_nn, &RDF_TYPE, &builtin_type_term),
                    "{} is not declared a logic:Builtin in {MODULE_TTL}",
                    builtin_nn.as_str()
                );
            }
            _ => panic!("logic:invokesBuiltin target is not an IRI: {builtin:?}"),
        }
    }
}
