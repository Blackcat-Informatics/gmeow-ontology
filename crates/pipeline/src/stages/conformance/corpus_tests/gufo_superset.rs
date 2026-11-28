// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Native `gmeow:logic ⊇ gUFO` coverage floor — the `meta:gate-logic-gufo-superset`
//! gate (Principle 17).
//!
//! gUFO is a generated, VALIDATION-ONLY lossy down-projection of the canonical
//! `gmeow:logic` foundation. The floor is measured over every term the vendored
//! `imports/gufo.ttl` declares in the gUFO namespace, by the kind its declaration gives
//! it, never over a class list. Each term is joined to the grounding cells that target
//! it, and each cell is judged on its native claim and its target claim independently
//! (see `gufo_floor`). The five temporary-situation reifiers are covered by the
//! `logic:Fluent` + RDF-1.2 edge-property replacement, which the worked example
//! `slices/grounding/logic/examples/criticism-fixes.ttl` exercises. The event and situation
//! types are covered by the `logic:Event` and `logic:Situation` sorts.
//!
//! The sort-to-gUFO down-projection is `gufo_for_sort` in `crates/logic-compile/src/projections/rdf.rs`;
//! this test owns the full *coverage* floor.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::LazyLock;

use gmeow_validate::grounding_observation::Observations;

use super::super::gufo_superset::{
    EXAMPLE_TTL, GUFO_TTL, MODULE_TTL, NATIVE_TTL, ObservedTerm as RdfTerm, SourceObservation,
};
use super::gufo_floor::{
    self, Correspondence, DECIDED_EXCEPTIONS, Judgment, Kind, Natives, Report, TargetClaim,
};

// --------------------------------------------------------------------------- //
// Namespaces (the vocabulary IRIs this gate joins on and the data files it reads).
// --------------------------------------------------------------------------- //
/// `logic:` foundation namespace (see `criticism-fixes.ttl` / `module.ttl` `@prefix`).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// Worked-example A-Box namespace (`@prefix ex:` in `criticism-fixes.ttl`).
const EX_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
const RDF_SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
const RDF_PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";

/// The grounding catalog and the generated view its gUFO cells belong to.
const CATALOG_TTL: &str = "slices/grounding/logic/mappings/grounding-bridges.ttl";
const GUFO_SSSOM: &str = "gmeow-logic-gufo.sssom.tsv";

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

fn logic_iri(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}
fn ex_iri(local: &str) -> String {
    format!("{EX_NS}{local}")
}
fn gufo_iri(local: &str) -> String {
    format!("{}{local}", gufo_floor::GUFO_NS)
}

/// An IRI object term (the `RdfTerm` form of a `logic:`/`owl:`/`rdf:` class).
fn iri_term(iri: &str) -> RdfTerm {
    RdfTerm::iri(iri.to_owned())
}

// The explicit producer records the import and each native module independently; the
// worked example shares the existing grounding-source native parse. Neither loader can
// run a producer.
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

fn gufo_observations() -> &'static BTreeMap<String, SourceObservation> {
    static OBSERVATIONS: std::sync::OnceLock<
        Result<
            super::source_artifact::Selected<BTreeMap<String, SourceObservation>>,
            gmeow_errors::Diag,
        >,
    > = std::sync::OnceLock::new();
    super::source_artifact::get_native(&OBSERVATIONS, super::super::gufo_superset::CHANNEL)
}

fn observed_source(path: &str) -> &'static SourceObservation {
    let source = gufo_observations()
        .get(path)
        .unwrap_or_else(|| panic!("missing source observation {path}"));
    assert_eq!(source.source_path, path);
    source
}

fn gufo_store() -> &'static SourceObservation {
    observed_source(GUFO_TTL)
}

fn logic_module() -> &'static SourceObservation {
    observed_source(MODULE_TTL)
}

/// The authored grounding cells, as the producer extracted them from the catalog.
fn catalog_cells() -> &'static Observations {
    static OBSERVATIONS: std::sync::OnceLock<
        Result<super::source_artifact::Selected<Observations>, gmeow_errors::Diag>,
    > = std::sync::OnceLock::new();
    super::source_artifact::get_native(&OBSERVATIONS, super::super::grounding_catalogs::CHANNEL)
}

fn gufo_correspondences() -> Vec<Correspondence> {
    catalog_cells()
        .catalogs
        .get(CATALOG_TTL)
        .unwrap_or_else(|| panic!("missing original catalog {CATALOG_TTL}"))
        .iter()
        .filter(|cell| cell.grounding && cell.sssom_file == GUFO_SSSOM)
        .map(|cell| Correspondence {
            source: cell
                .source_endpoint
                .clone()
                .expect("a grounding correspondence names its source endpoint"),
            target: cell
                .target_endpoint
                .clone()
                .expect("a grounding correspondence names its target endpoint"),
            predicate: cell.predicate.clone(),
            preservation: cell
                .preservation
                .clone()
                .expect("a grounding correspondence carries its preservation kind"),
            loss: cell.loss.clone(),
        })
        .collect()
}

/// Measure the floor over the produced evidence. A catalog that names a term the
/// vendored source does not declare, or a vendored declaration of unknown kind, fails here.
fn measured() -> Report {
    let terms = gufo_floor::denominator(gufo_store())
        .unwrap_or_else(|error| panic!("gUFO denominator: {error}"));
    let natives = Natives::new(NATIVE_TTL.into_iter().map(observed_source));
    gufo_floor::measure(&terms, &gufo_correspondences(), &natives)
        .unwrap_or_else(|error| panic!("gUFO floor join: {error}"))
}

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

// --------------------------------------------------------------------------- //
// The uncovered families.
//
// Each family is a coherent block of gUFO terms with no earned native counterpart and no
// native replacement. The table is the complete set of gaps: the check below requires it
// to equal the measured gaps exactly, so covering a term forces its removal here and an
// uncovered term can never be left out of it. The universal claim excludes only the
// decided exceptions in the floor module; every other row must be covered before it holds.
// --------------------------------------------------------------------------- //
const UNCOVERED_FAMILIES: &[(&str, &[&str])] = &[
    (
        "temporary-situation role properties",
        &[
            "concernsConstitutedEndurant",
            "concernsNonRigidType",
            "concernsQualityType",
            "concernsQualityValue",
            "concernsReifiedQualityValue",
            "concernsRelatedEndurant",
            "concernsRelationshipType",
            "concernsTemporaryWhole",
        ],
    ),
    (
        "qualified standing",
        &[
            "standsIn",
            "standsInQualifiedAttribution",
            "standsInQualifiedConstitution",
            "standsInQualifiedInstantiation",
            "standsInQualifiedParthood",
            "standsInQualifiedRelationship",
        ],
    ),
    (
        "parthood and constitution specializations",
        &[
            "constitutes",
            "isAspectProperPartOf",
            "isCollectionMemberOf",
            "isComponentOf",
            "isEventProperPartOf",
            "isObjectProperPartOf",
            "isSituationProperPartOf",
            "isSubCollectionOf",
            "isSubQuantityOf",
        ],
    ),
    (
        "temporal boundaries",
        &[
            "hasBeginPoint",
            "hasBeginPointInXSDDate",
            "hasBeginPointInXSDDateTimeStamp",
            "hasEndPoint",
            "hasEndPointInXSDDate",
            "hasEndPointInXSDDateTimeStamp",
        ],
    ),
    (
        "event and participation relations",
        &[
            "broughtAbout",
            "contributedToTrigger",
            "manifestedIn",
            "participatedIn",
            "wasCreatedIn",
            "wasTerminatedIn",
        ],
    ),
    (
        "quality values",
        &[
            "hasAssociatedQualityValueType",
            "hasQualityValue",
            "hasReifiedQualityValue",
            "hasValueComponent",
        ],
    ),
    ("type categorization", &["categorizes", "partitions"]),
    ("derivation", &["isDerivedFrom"]),
    ("mediation characteristics", &["mediates"]),
    ("inherence breadth", &["inheresIn"]),
    ("external dependence", &["externallyDependsOn"]),
];

/// The gUFO classes that a native term replaces rather than earns: the five temporary-situation
/// reifiers, replaced by `logic:Fluent` + RDF-1.2 edge properties, and the event and situation
/// types, replaced by the `logic:Event` and `logic:Situation` sorts. Pinned exactly so an
/// accidental over-replacement (a faithfully-coverable class judged a replacement) fails.
const EXPECTED_REPLACED_CLASSES: &[&str] = &[
    "EventType",
    "QualityValueAttributionSituation",
    "SituationType",
    "TemporaryConstitutionSituation",
    "TemporaryInstantiationSituation",
    "TemporaryParthoodSituation",
    "TemporaryRelationshipSituation",
];

// --------------------------------------------------------------------------- //
// (A) The floor, measured over the complete vendored inventory.
// --------------------------------------------------------------------------- //

/// The denominator is every vendored declaration in the gUFO namespace, by its own kind.
fn denominator_is_the_complete_vendored_inventory(report: &Report) {
    let by_kind: BTreeMap<Kind, usize> = Kind::ALL
        .into_iter()
        .map(|kind| {
            let total = report
                .outcomes
                .iter()
                .filter(|outcome| outcome.term.kind == kind)
                .count();
            (kind, total)
        })
        .collect();
    assert_eq!(
        by_kind,
        BTreeMap::from([
            (Kind::Class, 49),
            (Kind::ObjectProperty, 40),
            (Kind::DataProperty, 7),
            (Kind::AnnotationProperty, 0),
            (Kind::NamedIndividual, 0),
            (Kind::Datatype, 0),
        ]),
        "the gUFO denominator drifted from the vendored declarations"
    );
}

/// Coverage by term kind and judgment is pinned, so a change in any property's evidence
/// moves a count even when every class stays covered.
fn coverage_by_kind_is_pinned(report: &Report) {
    use Judgment::{EarnedCounterpart, NativeReplacement, Uncovered, ValidationOnlyMatch};
    assert_eq!(
        report.counts(),
        BTreeMap::from([
            ((Kind::Class, EarnedCounterpart), 42),
            ((Kind::Class, NativeReplacement), 7),
            ((Kind::ObjectProperty, EarnedCounterpart), 2),
            ((Kind::ObjectProperty, ValidationOnlyMatch), 3),
            ((Kind::ObjectProperty, Uncovered), 35),
            ((Kind::DataProperty, Uncovered), 7),
        ])
    );
}

/// The replaced classes are exactly the pinned set, and no class is left uncovered.
fn every_class_is_covered_and_the_replacements_are_pinned(report: &Report) {
    let class_gaps: Vec<&str> = report
        .gaps()
        .into_iter()
        .filter(|outcome| outcome.term.kind == Kind::Class)
        .map(|outcome| outcome.term.iri.as_str())
        .collect();
    assert!(
        class_gaps.is_empty(),
        "gUFO classes without a covering counterpart: {class_gaps:?}"
    );
    let replaced: HashSet<String> = report
        .outcomes
        .iter()
        .filter(|outcome| {
            outcome.term.kind == Kind::Class && outcome.judgment() == Judgment::NativeReplacement
        })
        .map(|outcome| outcome.term.iri.clone())
        .collect();
    let expected: HashSet<String> = EXPECTED_REPLACED_CLASSES
        .iter()
        .map(|local| gufo_iri(local))
        .collect();
    assert_eq!(replaced, expected, "replacement set drift");
}

/// Many-to-one mappings keep each source and each target individually, and a target
/// claim the view does not carry is reported apart from the native claim.
fn many_to_one_sources_and_independent_claims_are_retained(report: &Report) {
    let grouped = report.targets_by_source();
    assert_eq!(
        grouped[logic_iri("Aspect").as_str()],
        BTreeSet::from([
            gufo_iri("Aspect").as_str(),
            gufo_iri("IntrinsicAspect").as_str(),
            gufo_iri("ExtrinsicAspect").as_str(),
        ])
    );
    let mode = report
        .outcomes
        .iter()
        .find(|outcome| outcome.term.iri == gufo_iri("IntrinsicMode"))
        .expect("the denominator holds gufo:IntrinsicMode");
    assert_eq!(
        mode.cells.len(),
        2,
        "exactly the Mode and Disposition cells"
    );
    let cell_from = |source: &str| {
        mode.cells
            .iter()
            .find(|cell| cell.correspondence.source == source)
            .unwrap_or_else(|| panic!("gufo:IntrinsicMode has a cell from {source}"))
    };
    let mode_cell = cell_from(logic_iri("Mode").as_str());
    assert_eq!(mode_cell.judgment, Judgment::EarnedCounterpart);
    assert_eq!(mode_cell.target, TargetClaim::ValidationOnly);
    let disposition_cell = cell_from(logic_iri("Disposition").as_str());
    assert_eq!(disposition_cell.judgment, Judgment::ValidationOnlyMatch);
    assert_eq!(disposition_cell.target, TargetClaim::Unsupported);
    assert_eq!(
        mode.judgment(),
        Judgment::EarnedCounterpart,
        "the unsupported downprojection coexists with the earned native counterpart"
    );
}

/// The three dependence judgments are measured with their own evidence.
fn dependence_judgments_are_measured(report: &Report) {
    let judgment = |local: &str| {
        report
            .outcomes
            .iter()
            .find(|outcome| outcome.term.iri == gufo_iri(local))
            .unwrap_or_else(|| panic!("the denominator holds gufo:{local}"))
            .judgment()
    };
    assert_eq!(
        judgment("historicallyDependsOn"),
        Judgment::EarnedCounterpart
    );
    assert_eq!(
        judgment("externallyDependsOn"),
        Judgment::ValidationOnlyMatch
    );
    assert_eq!(judgment("inheresIn"), Judgment::ValidationOnlyMatch);
}

/// The gaps are exactly the declared families, and the universal claim holds only
/// when no family remains.
fn gaps_are_exactly_the_declared_families(report: &Report) {
    let mut declared: BTreeMap<String, &str> = BTreeMap::new();
    for (family, members) in UNCOVERED_FAMILIES {
        for member in *members {
            let previous = declared.insert(gufo_iri(member), *family);
            assert!(previous.is_none(), "{member} is listed in two families");
        }
    }
    let gap_terms: BTreeSet<String> = report
        .gaps()
        .into_iter()
        .map(|outcome| outcome.term.iri.clone())
        .collect();
    let listed: BTreeSet<String> = declared.keys().cloned().collect();
    assert_eq!(
        gap_terms, listed,
        "the uncovered-family table must equal the measured gaps"
    );
    for local in DECIDED_EXCEPTIONS {
        assert!(
            listed.contains(&gufo_iri(local)),
            "decided exception gufo:{local} must be a listed gap"
        );
    }
    let undecided: BTreeSet<String> = listed
        .iter()
        .filter(|iri| {
            !DECIDED_EXCEPTIONS
                .iter()
                .any(|local| **iri == gufo_iri(local))
        })
        .cloned()
        .collect();
    assert!(
        !report.every_term_is_covered_or_decided(),
        "the universal claim must not hold while {undecided:?} remain uncovered"
    );
}

// --------------------------------------------------------------------------- //

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

    let chars: HashSet<String> = objects_of(logic_module(), &LOGIC_PROPER_PART_OF, RDF_TYPE)
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
                    has_object(logic_module(), builtin_iri, RDF_TYPE, &builtin_type_term),
                    "{builtin_iri} is not declared a logic:Builtin in {MODULE_TTL}"
                );
            }
            _ => panic!("logic:invokesBuiltin target is not an IRI: {builtin:?}"),
        }
    }
}

/// INVARIANT: every gUFO cell's source is a subject a measured native module types with
/// `rdf:type`. A source with no type in any native module, such as a label-only IRI, is refused
/// here. A typed source of another kind is measured as a validation-only match by the floor,
/// not refused.
fn every_gufo_source_is_declared_by_a_native_module() {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let declared: BTreeSet<&str> = NATIVE_TTL
        .into_iter()
        .flat_map(|path| {
            observed_source(path)
                .pairs
                .get(RDF_TYPE)
                .into_iter()
                .flatten()
                .map(|(subject, _)| subject.as_str())
        })
        .collect();
    let undeclared: Vec<String> = gufo_correspondences()
        .into_iter()
        .map(|cell| cell.source)
        .filter(|source| !declared.contains(source.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "gUFO cells name sources no native module declares: {undeclared:?}"
    );
}

#[test]
fn authored_gufo_superset_contracts() {
    let report = measured();
    every_gufo_source_is_declared_by_a_native_module();
    denominator_is_the_complete_vendored_inventory(&report);
    coverage_by_kind_is_pinned(&report);
    every_class_is_covered_and_the_replacements_are_pinned(&report);
    many_to_one_sources_and_independent_claims_are_retained(&report);
    dependence_judgments_are_measured(&report);
    gaps_are_exactly_the_declared_families(&report);
    criticism_example_parses();
    criticism_example_has_native_edge_property();
    criticism_example_has_strict_partial_order();
    criticism_example_has_multilevel_instance_chain();
    criticism_example_references_builtin();
}
