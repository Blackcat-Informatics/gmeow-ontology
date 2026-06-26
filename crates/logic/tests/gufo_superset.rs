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

use oxigraph::model::{GraphNameRef, NamedNode, NamedOrBlankNode, Term};
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

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

const GRAPHBOXROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";

fn gufo(local: &str) -> String {
    format!("{GUFO_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}
fn ex(local: &str) -> String {
    format!("{EX_NS}{local}")
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
            Logic(local) => Some(logic(local)),
            Superseded => None,
        })
        .collect()
}

// --------------------------------------------------------------------------- //
// Fixtures: load a repo-relative Turtle file into an oxigraph Store. HARD-FAIL
// (panic) on missing/unparsable input — NO-OPTIONALITY: a missing source file is
// a build error, not a silently-skipped test (mirrors `ontology_entailments.rs`).
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

fn iri(s: &str) -> NamedNode {
    NamedNode::new(s).unwrap_or_else(|e| panic!("invalid IRI {s:?}: {e}"))
}

fn iri_term(s: &str) -> Term {
    Term::NamedNode(iri(s))
}

fn subject_iri(s: &NamedOrBlankNode) -> Option<String> {
    match s {
        NamedOrBlankNode::NamedNode(nn) => Some(nn.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

fn term_iri(t: &Term) -> Option<String> {
    match t {
        Term::NamedNode(nn) => Some(nn.as_str().to_owned()),
        _ => None,
    }
}

/// All subjects of `(*, predicate, object)` (named subjects only).
fn subjects_with(store: &Store, predicate: &str, object: &Term) -> Vec<String> {
    let p = iri(predicate);
    store
        .quads_for_pattern(
            None,
            Some(p.as_ref()),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .filter_map(|q| subject_iri(&q.subject))
        .collect()
}

/// All objects of `(subject, predicate, *)`.
fn objects_of(store: &Store, subject: &str, predicate: &str) -> Vec<Term> {
    let s = NamedOrBlankNode::NamedNode(iri(subject));
    let p = iri(predicate);
    store
        .quads_for_pattern(
            Some(s.as_ref()),
            Some(p.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .map(|q| q.object)
        .collect()
}

/// All `(subject_iri, object)` pairs for predicate `p`.
fn pairs_of(store: &Store, predicate: &str) -> Vec<(String, Term)> {
    let p = iri(predicate);
    store
        .quads_for_pattern(
            None,
            Some(p.as_ref()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .filter_map(Result::ok)
        .filter_map(|q| subject_iri(&q.subject).map(|s| (s, q.object)))
        .collect()
}

/// Whether `(subject, predicate, object)` is present.
fn contains(store: &Store, subject: &str, predicate: &str, object: &Term) -> bool {
    let s = NamedOrBlankNode::NamedNode(iri(subject));
    let p = iri(predicate);
    store
        .quads_for_pattern(
            Some(s.as_ref()),
            Some(p.as_ref()),
            Some(object.as_ref()),
            Some(GraphNameRef::DefaultGraph),
        )
        .next()
        .is_some()
}

/// Every `owl:Class` IRI in the gUFO namespace declared in `imports/gufo.ttl`.
fn gufo_classes(store: &Store) -> Vec<String> {
    let mut classes: Vec<String> = subjects_with(store, RDF_TYPE, &iri_term(OWL_CLASS))
        .into_iter()
        .filter(|s| s.starts_with(GUFO_NS))
        .collect();
    classes.sort();
    classes.dedup();
    classes
}

/// All distinct named subjects in a store.
fn all_subjects(store: &Store) -> HashSet<String> {
    store
        .quads_for_pattern(None, None, None, Some(GraphNameRef::DefaultGraph))
        .filter_map(Result::ok)
        .filter_map(|q| subject_iri(&q.subject))
        .collect()
}

const GUFO_TTL: &str = "imports/gufo.ttl";
const MODULE_TTL: &str = "slices/core/logic/module.ttl";
const EXAMPLE_TTL: &str = "slices/core/logic/examples/criticism-fixes.ttl";

// --------------------------------------------------------------------------- //
// (A1) Every gUFO class has a correspondence — the minimum-baseline floor.
// --------------------------------------------------------------------------- //

#[test]
fn every_gufo_class_has_logic_correspondence() {
    let gufo_store = load_store(GUFO_TTL);
    let classes = gufo_classes(&gufo_store);
    assert!(
        !classes.is_empty(),
        "No gUFO owl:Class declarations found in {GUFO_TTL}"
    );

    let keys: HashSet<String> = GUFO_CLASS_TO_LOGIC.iter().map(|(k, _)| gufo(k)).collect();
    let mut missing: Vec<&String> = classes.iter().filter(|c| !keys.contains(*c)).collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "gmeow:logic ⊇ gUFO floor BREACHED — these gUFO classes have NO entry in the \
         GUFO_CLASS_TO_LOGIC map (add a faithful logic: target or Superseded):\n  {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// --------------------------------------------------------------------------- //
// (A2) Correspondence targets actually exist in the module.
// --------------------------------------------------------------------------- //

#[test]
fn correspondence_targets_exist() {
    let module = load_store(MODULE_TTL);
    let subjects = all_subjects(&module);
    let mut missing: Vec<String> = non_superseded_targets()
        .into_iter()
        .filter(|t| !subjects.contains(t))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "These GUFO_CLASS_TO_LOGIC targets are NOT declared as subjects in {MODULE_TTL} — \
         the correspondence is dangling:\n  {}",
        missing.join("\n  ")
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
    let module = load_store(MODULE_TTL);
    let mut no_role: Vec<String> = non_superseded_targets()
        .into_iter()
        .filter(|t| objects_of(&module, t, GRAPHBOXROLE).is_empty())
        .collect();
    no_role.sort();
    assert!(
        no_role.is_empty(),
        "These GUFO_CLASS_TO_LOGIC targets lack a gmeow:graphBoxRole annotation in \
         {MODULE_TTL} — add one rather than weakening the gate:\n  {}",
        no_role.join("\n  ")
    );
}

// --------------------------------------------------------------------------- //
// (B) Worked example — criticism-fixes.ttl parses and shows the four patterns.
// --------------------------------------------------------------------------- //

#[test]
fn criticism_example_parses() {
    let example = load_store(EXAMPLE_TTL);
    assert!(
        example
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
    let example = load_store(EXAMPLE_TTL);
    let fluents: HashSet<String> = subjects_with(&example, RDF_TYPE, &iri_term(&logic("Fluent")))
        .into_iter()
        .collect();
    let statements: HashSet<String> = subjects_with(&example, RDF_TYPE, &iri_term(RDF_STATEMENT))
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
            !objects_of(&example, reifier, pred).is_empty(),
            "reifier {reifier} is missing a {pred} quoted-triple component"
        );
    }
    // Carries LITERAL validFrom/validTo edge metadata (isinstance(o, Literal) parity).
    for (pred, name) in [(ex("validFrom"), "validFrom"), (ex("validTo"), "validTo")] {
        let has_literal = objects_of(&example, reifier, &pred)
            .iter()
            .any(|o| matches!(o, Term::Literal(_)));
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
    let example = load_store(EXAMPLE_TTL);
    let chain = pairs_of(&example, &logic("properPartOf"));
    assert!(
        chain.len() >= 2,
        "expected a logic:properPartOf chain (>= 2 edges) in {EXAMPLE_TTL}, found {}",
        chain.len()
    );

    let module = load_store(MODULE_TTL);
    let chars: HashSet<String> = objects_of(&module, &logic("properPartOf"), RDF_TYPE)
        .iter()
        .filter_map(term_iri)
        .collect();
    for required in [
        "transitiveProperty",
        "asymmetricProperty",
        "irreflexiveProperty",
    ] {
        assert!(
            chars.contains(&logic(required)),
            "logic:properPartOf is not typed logic:{required} in the module — the \
             strict-partial-order characteristic is missing"
        );
    }
}

#[test]
fn criticism_example_has_multilevel_instance_chain() {
    // §3 no-punning fix: a logic:instanceOf chain where a type is itself an instance of
    // a higher-order type, with logic:orderedType levels.
    let example = load_store(EXAMPLE_TTL);
    let inst = pairs_of(&example, &logic("instanceOf"));
    let subjects: HashSet<String> = inst.iter().map(|(s, _)| s.clone()).collect();
    // A two-step chain: an object that is itself a subject (marv -> goldenEagle -> species).
    let has_bridge = inst
        .iter()
        .filter_map(|(_, o)| term_iri(o))
        .any(|o| subjects.contains(&o));
    assert!(
        has_bridge,
        "no multi-level chain: need x logic:instanceOf y and y logic:instanceOf z"
    );
    // logic:orderedType levels are recorded.
    let has_levels = example
        .quads_for_pattern(
            None,
            Some(iri(&logic("orderedType")).as_ref()),
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
    let example = load_store(EXAMPLE_TTL);
    let invocations = pairs_of(&example, &logic("invokesBuiltin"));
    assert!(
        !invocations.is_empty(),
        "no logic:invokesBuiltin edge found in {EXAMPLE_TTL}"
    );

    let module = load_store(MODULE_TTL);
    let builtin_type = iri_term(&logic("Builtin"));
    for (_subj, builtin) in &invocations {
        let builtin_iri = term_iri(builtin)
            .unwrap_or_else(|| panic!("logic:invokesBuiltin target is not an IRI: {builtin:?}"));
        assert!(
            contains(&module, &builtin_iri, RDF_TYPE, &builtin_type),
            "{builtin_iri} is not declared a logic:Builtin in {MODULE_TTL}"
        );
    }
}
