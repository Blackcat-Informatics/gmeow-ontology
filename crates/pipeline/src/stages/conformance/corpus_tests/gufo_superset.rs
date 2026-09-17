// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Native `gmeow:logic ⊇ gUFO` coverage floor — the `meta:gate-logic-gufo-superset`
//! gate (Principle 17), ported from the retired Python fixture
//! `tests/test_logic_gufo_superset.py`.
//!
//! gUFO is a generated, VALIDATION-ONLY lossy down-projection of the canonical
//! `gmeow:logic` foundation: every gUFO `owl:Class` must therefore be covered by a
//! richer `logic:` term, or be explicitly SUPERSEDED by the `logic:Fluent` + RDF-1.2
//! edge-property pattern (the five temporary-situation reifiers). This is the honest
//! floor that enforces it, checked against authenticated producer observations of
//! `imports/gufo.ttl` + `slices/grounding/logic/module.ttl` + the worked example
//! `slices/grounding/logic/examples/criticism-fixes.ttl`.
//!
//! Replaces the Python `_GUFO_CLASS_TO_LOGIC` fixture inlined when the Python
//! compiler was deleted. The 11-stereotype *runtime* sort map stays in
//! `crates/logic/src/compile/adapter.rs`; this test owns the full *coverage* floor.

use std::collections::HashSet;
use std::sync::LazyLock;

use super::super::gufo_superset::{
    EXAMPLE_TTL, GUFO_TTL, MODULE_TTL, ObservedTerm as RdfTerm, SourceObservation,
};

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
// declared in `slices/grounding/logic/module.ttl`.
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

// The explicit producer records the import independently; module/example observations
// share the existing grounding-source native parse. Neither loader can run a producer.
fn grounding_source(path: &str) -> &'static SourceObservation {
    let observed = super::gmn_grounding::observations();
    let source = observed
        .sources
        .get(path)
        .unwrap_or_else(|| panic!("missing source observation {path}"));
    let summary = source
        .gufo_superset
        .as_ref()
        .unwrap_or_else(|| panic!("missing gUFO product observation {path}"));
    assert_eq!(summary.source_path, path);
    summary
}

fn gufo_store() -> &'static SourceObservation {
    static OBSERVATIONS: std::sync::OnceLock<
        Result<super::source_artifact::Selected<SourceObservation>, gmeow_errors::Diag>,
    > = std::sync::OnceLock::new();
    let observed = super::source_artifact::get(&OBSERVATIONS, super::super::gufo_superset::CHANNEL);
    assert_eq!(observed.source_path, GUFO_TTL);
    observed
}

// --------------------------------------------------------------------------- //
// Typed helper functions.
//
// All sources are graph-flat (default graph): a quad with `graph_name == None`.
// The helpers filter the snapshot vec, mirroring the prior oxigraph
// `quads_for_pattern(..., DefaultGraph)` queries.
// --------------------------------------------------------------------------- //

/// All subjects of a selected predicate/object observation. The producer already
/// enforced the original default-graph and named-subject selection.
fn subjects_with(store: &SourceObservation, predicate: &str, object: &RdfTerm) -> Vec<String> {
    store
        .pairs
        .get(predicate)
        .into_iter()
        .flatten()
        .filter(|(_, o)| o == object)
        .map(|(s, _)| s.clone())
        .collect()
}

fn objects_of(store: &SourceObservation, subject: &str, predicate: &str) -> Vec<RdfTerm> {
    store
        .pairs
        .get(predicate)
        .into_iter()
        .flatten()
        .filter(|(s, _)| s == subject)
        .map(|(_, o)| o.clone())
        .collect()
}

fn pairs_of(store: &SourceObservation, predicate: &str) -> Vec<(String, RdfTerm)> {
    store.pairs.get(predicate).cloned().unwrap_or_default()
}

fn has_object(store: &SourceObservation, subject: &str, predicate: &str, object: &RdfTerm) -> bool {
    store
        .pairs
        .get(predicate)
        .into_iter()
        .flatten()
        .any(|(s, o)| s == subject && o == object)
}

fn has_any_object(store: &SourceObservation, subject: &str, predicate: &str) -> bool {
    store
        .pairs
        .get(predicate)
        .into_iter()
        .flatten()
        .any(|(s, _)| s == subject)
}

/// Every `owl:Class` IRI in the gUFO namespace declared in `imports/gufo.ttl`.
fn gufo_classes(store: &SourceObservation) -> Vec<String> {
    let mut classes: Vec<String> = subjects_with(store, RDF_TYPE, &iri_term(OWL_CLASS))
        .into_iter()
        .filter(|n| n.starts_with(GUFO_NS))
        .collect();
    classes.sort();
    classes.dedup();
    classes
}

/// All distinct named subjects in a store.
fn all_subjects(store: &SourceObservation) -> HashSet<String> {
    store.subjects.iter().cloned().collect()
}

// --------------------------------------------------------------------------- //
// (A1) Every gUFO class has a correspondence — the minimum-baseline floor.
// --------------------------------------------------------------------------- //

fn every_gufo_class_has_logic_correspondence() {
    let classes = gufo_classes(gufo_store());
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

fn correspondence_targets_exist() {
    let subjects = all_subjects(grounding_source(MODULE_TTL));
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

fn new_logic_terms_carry_graphbox_role() {
    let mut no_role: Vec<String> = NON_SUPERSEDED
        .iter()
        .filter(|t| !has_any_object(grounding_source(MODULE_TTL), t, GRAPHBOXROLE))
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

fn criticism_example_parses() {
    assert!(
        grounding_source(EXAMPLE_TTL).default_quad_count > 0,
        "worked example {EXAMPLE_TTL} parsed empty"
    );
}

fn criticism_example_has_native_edge_property() {
    // §1 triple-bloat fix: an RDF-1.2 reifier typed logic:Fluent carrying the quoted
    // (subject, predicate, object) and validFrom/validTo edge metadata.
    let fluents: HashSet<String> = subjects_with(
        grounding_source(EXAMPLE_TTL),
        RDF_TYPE,
        &iri_term(&LOGIC_FLUENT),
    )
    .into_iter()
    .collect();
    let statements: HashSet<String> = subjects_with(
        grounding_source(EXAMPLE_TTL),
        RDF_TYPE,
        &iri_term(RDF_STATEMENT),
    )
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
            !objects_of(grounding_source(EXAMPLE_TTL), reifier, pred).is_empty(),
            "reifier {reifier} is missing a {pred} quoted-triple component"
        );
    }
    // Carries LITERAL validFrom/validTo edge metadata (isinstance(o, Literal) parity).
    for (pred_iri, name) in [
        (ex_iri("validFrom"), "validFrom"),
        (ex_iri("validTo"), "validTo"),
    ] {
        let has_literal = objects_of(grounding_source(EXAMPLE_TTL), reifier, &pred_iri)
            .iter()
            .any(|o| matches!(o, RdfTerm::Literal(_)));
        assert!(
            has_literal,
            "reifier {reifier} carries no literal {name} edge metadata"
        );
    }
}

fn criticism_example_has_strict_partial_order() {
    // §2 OWL-2 global-restriction fix: logic:properPartOf used in the example, and the
    // module types it transitive ∧ asymmetric ∧ irreflexive at once (illegal in OWL 2).
    let chain = pairs_of(grounding_source(EXAMPLE_TTL), &LOGIC_PROPER_PART_OF);
    assert!(
        chain.len() >= 2,
        "expected a logic:properPartOf chain (>= 2 edges) in {EXAMPLE_TTL}, found {}",
        chain.len()
    );

    let chars: HashSet<String> = objects_of(
        grounding_source(MODULE_TTL),
        &LOGIC_PROPER_PART_OF,
        RDF_TYPE,
    )
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

fn criticism_example_has_multilevel_instance_chain() {
    // §3 no-punning fix: a logic:instanceOf chain where a type is itself an instance of
    // a higher-order type, with logic:orderedType levels.
    let inst = pairs_of(grounding_source(EXAMPLE_TTL), &LOGIC_INSTANCE_OF);
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
    let has_levels = grounding_source(EXAMPLE_TTL)
        .pairs
        .get(&*LOGIC_ORDERED_TYPE)
        .is_some_and(|rows| !rows.is_empty());
    assert!(has_levels, "no logic:orderedType levels recorded");
}

fn criticism_example_references_builtin() {
    // §4 builtin-derived value: a derivation references a logic:Builtin individual via
    // logic:invokesBuiltin; the target must be declared a logic:Builtin in the module.
    let invocations = pairs_of(grounding_source(EXAMPLE_TTL), &LOGIC_INVOKES_BUILTIN);
    assert!(
        !invocations.is_empty(),
        "no logic:invokesBuiltin edge found in {EXAMPLE_TTL}"
    );

    let builtin_type_term = iri_term(&LOGIC_BUILTIN);
    for (_subj, builtin) in &invocations {
        match builtin {
            RdfTerm::Iri(builtin_iri) => {
                assert!(
                    has_object(
                        grounding_source(MODULE_TTL),
                        builtin_iri,
                        RDF_TYPE,
                        &builtin_type_term
                    ),
                    "{builtin_iri} is not declared a logic:Builtin in {MODULE_TTL}"
                );
            }
            _ => panic!("logic:invokesBuiltin target is not an IRI: {builtin:?}"),
        }
    }
}

#[test]
fn authored_gufo_superset_contracts() {
    every_gufo_class_has_logic_correspondence();
    correspondence_targets_exist();
    superseded_set_is_the_five_reifiers();
    new_logic_terms_carry_graphbox_role();
    criticism_example_parses();
    criticism_example_has_native_edge_property();
    criticism_example_has_strict_partial_order();
    criticism_example_has_multilevel_instance_chain();
    criticism_example_references_builtin();
}
