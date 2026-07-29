// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DL consistency / unsatisfiability over the structured chase.
//!
//! The native authority now closes the bundle with the predicate-as-DATA RL
//! engine ([`crate::reason::rl`]) and layers DL-specific finite consistency
//! checks over that closure. `DlGap` is no longer an accepted profile boundary:
//! constructs present in the committed bundle must be named in
//! [`DlVerdict::coverage`] as decided, and [`DlVerdict::gaps`] is reserved for a
//! hard coverage defect.
//!
//! # Distinction
//!
//! An unsatisfiable but *unpopulated* class does **not** make the ontology
//! inconsistent: it is merely a class that can have no members. Only an
//! individual actually forced into `owl:Nothing` is an inconsistency. The
//! verdict keeps both surfaces separate ([`DlVerdict::unsatisfiable_classes`]
//! vs [`DlVerdict::inconsistencies`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::facts::skolem_iri;
use crate::reason::InferredAxiom;
use purrdf::{RdfDataset, RdfLiteral, RdfQuad, RdfTerm, TermValue};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
#[allow(dead_code)]
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

// ── OWL/RDF IRI constants ──────────────────────────────────────────────────────

const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTYOF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

// ── Chase materialization backstop (sound withhold under a resource bound) ─────
//
// The DL existential chase runs a weakly/jointly-acyclic CERTIFIED program, so it is
// guaranteed to TERMINATE — but not in tractable space. A certified program can still
// mint a super-polynomial (up to double-exponential) witness model: a large exact
// cardinality (e.g. an `owl:cardinality 127` restriction feeding an inverse/equivalent-
// class cycle, W3C `WebOnt-I5.1-010`) fans out witnesses whose full materialization
// exhausts memory long before the fixpoint closes. Running such a program unbudgeted
// OOMs the process rather than deciding it.
//
// These two bounds are HARD memory ceilings on that terminating path. Exceeding EITHER
// stops the chase and records an honest INCOMPLETE withhold (a `DlGap`) — never a wrong
// `consistent`/`inconsistent` decision (the incomplete-never-wrong contract). Both are
// set FAR above the largest materialization any decided W3C OWL 2 (EL + Full) corpus
// case needs (measured max total facts is in the low hundreds; measured max chase steps
// likewise), so no currently-decided case can reach them — they only ever trip on a
// genuinely explosive input, exactly where the reasoner must withhold instead of grow.
//
// `CHASE_STEP_BACKSTOP` caps committed derivations within ONE `route_chase` invocation
// (the `StepGovernor` unit); `MAX_CHASE_FACTS` caps the cumulative fact set the outer
// alternating fixed-point accumulates across invocations. Together they bound both the
// single-call blow-up and the multi-round accumulation.
const CHASE_STEP_BACKSTOP: u64 = 20_000;
const MAX_CHASE_FACTS: usize = 500_000;

/// The largest `≥n` existential obligation the native restricted chase will lower into a
/// concrete witness-inventing rule.
///
/// A cardinality minimum `≥n` (`owl:cardinality`/`owl:minCardinality`/the qualified
/// forms) lowers to a rule that invents `n` distinct witnesses AND asserts their
/// `n·(n−1)/2` pairwise `owl:differentFrom` — a QUADRATIC head. A very large `n` (the
/// W3C `WebOnt-description-logic-907` case declares `owl:cardinality 60000`, i.e. ~1.8
/// billion difference atoms) exhausts memory just BUILDING that rule, before any chase
/// step runs. Beyond this bound the obligation is not lowered; the run withholds an
/// honest INCOMPLETE instead (the native chase cannot materialize a witness set that
/// large), never a wrong decision and never an out-of-memory abort. It is set far above
/// the largest `≥n` any decided W3C OWL 2 (EL + Full) case needs (their measured chase
/// step counts are in the low single digits), so no currently-decided case regresses.
const MAX_EXISTENTIAL_WITNESSES: usize = 512;

/// Reserved internal marker predicate recorded in the closure when the existential
/// chase stopped at [`CHASE_STEP_BACKSTOP`]/[`MAX_CHASE_FACTS`] before reaching its
/// fixpoint. It is NOT an RDF entailment; [`verdict_from_inferred`] reads its presence
/// and folds it into [`DlVerdict::gaps`] as a sound withhold, and it appears in the
/// closure ONLY on an input that actually hit the bound (never on a decided case).
const CHASE_INCOMPLETE_MARKER_PRED: &str =
    "https://blackcatinformatics.ca/gmeow/logic/reason#chaseMaterializationIncomplete";

/// The reserved subject the [`CHASE_INCOMPLETE_MARKER_PRED`] marker fact carries.
const CHASE_INCOMPLETE_MARKER_SUBJECT: &str =
    "https://blackcatinformatics.ca/gmeow/logic/reason#chaseBackstop";

// ── Out-of-fragment DL/Full constructs the native path CANNOT soundly decide ───
//
// These are inventoried in `CONSTRUCT_COVERAGE` so `scan_coverage` marks them
// `present`. The native DL consistency chase implements no datatype-value
// reasoning, so an ontology whose (in)consistency actually TURNS ON a datatype/
// facet restriction can only be soundly reported as *cannot-decide* (a non-empty
// `DlVerdict::gaps`), never as a wrong `consistent` by silently ignoring the
// axiom (the incomplete-never-wrong doctrine). The withhold is PRECISE, not
// presence-based: a facet-restricted datatype that is merely DEFINED but
// constrains no asserted/inferred literal is INERT (it cannot cause an
// inconsistency), so `classify_coverage` decides the datatype-facet families in
// that case and only withholds when a literal is actually subject to a facet
// (see `datatype_facet_has_live_obligation`). The universal top
// properties (`owl:topObjectProperty`/`owl:topDataProperty`) are NEVER decided.
//
// NOTE — property irreflexivity/asymmetry (`owl:IrreflexiveProperty`,
// `owl:AsymmetricProperty`) are NOT out-of-fragment: the DL consistency post-pass
// now DECIDES them directly with sound local clash rules (`prp-asyp`/`prp-irp`,
// see `augment_with_extra_dl_clashes`), so they are inventoried in
// `CONSTRUCT_COVERAGE` and promoted to `decided` by `classify_coverage`. The
// committed bundle asserts them (via the `logic:asymmetricProperty` /
// `logic:irreflexiveProperty` markers on `logic:properPartOf`, projected to the
// OWL characteristics); the production bundle stays consistent because it never
// asserts a `p(x,y) ∧ p(y,x)` cycle or a `p(x,x)` self-loop on such a property.
const OWL_DATATYPE_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#datatypeComplementOf";
const OWL_WITH_RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const OWL_ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const XSD_MIN_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const XSD_MAX_INCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const XSD_MIN_EXCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#minExclusive";
const XSD_MAX_EXCLUSIVE: &str = "http://www.w3.org/2001/XMLSchema#maxExclusive";
const XSD_PATTERN: &str = "http://www.w3.org/2001/XMLSchema#pattern";
const XSD_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#length";
const XSD_MIN_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#minLength";
const XSD_MAX_LENGTH: &str = "http://www.w3.org/2001/XMLSchema#maxLength";
const XSD_TOTAL_DIGITS: &str = "http://www.w3.org/2001/XMLSchema#totalDigits";
const XSD_FRACTION_DIGITS: &str = "http://www.w3.org/2001/XMLSchema#fractionDigits";
const RDF_LANG_RANGE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langRange";
// The universal (top) properties: the native chase implements no top-property
// obligation, and they are absent from both the committed bundle and the vendored
// on-gate corpus, so an ontology whose (in)consistency turns on them can only be
// soundly reported as cannot-decide. (`owl:AllDifferent`/`owl:distinctMembers` are
// deliberately NOT withheld: the native chase already decides them for the vendored
// on-gate cases — it is only incomplete on some inconsistency shapes, which is a
// per-instance deciding gap, not a wholesale unimplemented construct.)
const OWL_TOP_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#topObjectProperty";
const OWL_TOP_DATA_PROPERTY: &str = "http://www.w3.org/2002/07/owl#topDataProperty";

const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_BOTTOM_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
const OWL_BOTTOM_DATA_PROPERTY: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
const OWL_HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
// The canonical `logic:` functional-characteristic carrier: a central
// `logic:PropertyCharacteristicAssertion` record joins `logic:characterizes ?P` +
// `logic:characteristicSort logic:functionalProperty`. It is the greenfield source of the
// functional characteristic (the `owl:FunctionalProperty` type marker above is its lossy
// projection), so the functional-cardinality clash reads BOTH carriers — the marker keeps
// deciding raw external/conformance OWL inputs, the record keeps deciding the object-level
// reasoning EDB after the `owl:FunctionalProperty` slice source declarations are removed.
const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";
const LOGIC_FUNCTIONAL_PROPERTY: &str = "https://blackcatinformatics.ca/logic/functionalProperty";
// The canonical `logic:` key carrier: a central `logic:KeyAssertion` record joins
// `logic:keyClass ?C` (the identified class) + `logic:keyProperty ?P` (its single key property).
// It is the greenfield source of a datatype/single-property key (the `owl:hasKey` axiom is its lossy
// OWL-DL view), so the key-agreement clash reads BOTH carriers — the `owl:hasKey` list keeps
// deciding raw external/conformance OWL inputs, the record keeps deciding the object-level reasoning
// EDB after the `owl:hasKey` slice source declaration is migrated to the carrier.
const LOGIC_KEY_ASSERTION: &str = "https://blackcatinformatics.ca/logic/KeyAssertion";
const LOGIC_KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
const LOGIC_KEY_PROPERTY: &str = "https://blackcatinformatics.ca/logic/keyProperty";
const OWL_NEGATIVE_PROPERTY_ASSERTION: &str =
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion";
const OWL_SOURCE_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
const OWL_ASSERTION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
const OWL_TARGET_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#targetIndividual";
const OWL_TARGET_VALUE: &str = "http://www.w3.org/2002/07/owl#targetValue";

// Property-characteristic + disjointness/identity constructs the native DL
// post-pass decides via sound local clash rules (Wave A). Each derives
// `type(?i, owl:Nothing, ?w)` — the inconsistency witness the verdict reads off.
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
const OWL_PROPERTY_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_ALL_DISJOINT_PROPERTIES: &str = "http://www.w3.org/2002/07/owl#AllDisjointProperties";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const OWL_DISTINCT_MEMBERS: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
const RDF_XML_LITERAL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral";

// DL construct IRIs scanned for the native coverage inventory.
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
// `owl:InverseFunctionalProperty` needs an inverse-functional identity-merge clash
// the native forward post-pass does not perform; it is out of the OWL 2 EL profile
// and absent from the committed bundle + vendored on-gate corpus (measured). The
// Family-6a counting refutation sub-decider ([`crate::reason::refute::counting`])
// decides its pure assertional/identity fragment (the `1 = 2` collapse); outside
// that fragment its presence stays an honest cannot-decide gap, never a wrong
// `consistent`.
//
// `owl:hasSelf` is NOT inventoried here: it is IN the OWL 2 EL profile and the
// vendored EL grade decides a benign `hasSelf` self-restriction typed onto an
// individual (`new-feature-selfrestriction-001`) as consistent. Only the
// hasSelf *refutation* shape — a self-restriction in a `disjointWith`/class-constraint
// position, where the chase would have to infer self-membership (`x p x ⇒ x ∈ ∃p.Self`)
// to see the clash — is withheld, via a shape trigger in [`refutation_shape_withholds`].
const OWL_HAS_SELF: &str = "http://www.w3.org/2002/07/owl#hasSelf";
const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
// `owl:unionOf` (general class union / disjunction) has finite native coverage
// here.
//
// `owl:intersectionOf` is deliberately NOT listed in CONSTRUCT_COVERAGE and is
// therefore never placed in `DlCoverage::present`. This is intentional and
// honest: the native EL/RL-positive path already
// materialises conjunction via standard subclass-propagation rules — every
// class expression `C ≡ (A ⊓ B)` in the bundle is expressed as
// `C ⊑ A`, `C ⊑ B` (plus the EL rules for the converse), so the subsumption
// closure is genuinely complete for the intersection pattern without any
// post-pass arm. Adding `owl:intersectionOf` to the inventory would force every
// intersection instance through the classifier, which would correctly return
// `decided` — but only because the *EL/RL path* already decided it. Listing it
// here would obscure WHICH path is responsible. The coverage instrument tracks
// what THIS DL post-pass decides; EL/RL coverage is the EL engine's concern.
// Concretely: the native reasoning gate and the frozen external OWL 2 DL oracle
// gold (`tests/conformance/`) both pass with this omission, confirming the
// conjunction instances in the committed bundle are fully decided by the
// EL/RL path. If a future bundle introduces an intersection pattern the EL/RL
// path cannot handle, the frozen external oracle gold gate will catch the regression.
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";

/// The construct families this module *inventories* in the committed
/// bundle (the `(iri, qname, suffix)` triples scanned by [`scan_coverage`]).
///
/// IMPORTANT — presence in this table is **inventory, not a coverage claim.**
/// A construct's IRI appearing in the bundle places it in
/// [`DlCoverage::present`]; whether it is also [`DlCoverage::decided`] is
/// decided *empirically* by [`classify_coverage`], which inspects the actual
/// instances and the honesty of the corresponding post-pass handler. Some
/// families here (notably `owl:someValuesFrom` and the cardinality family) are
/// only *inert* in the native post-pass: they do no existential generation and
/// only fire on already-degenerate inputs, so they are reported `unsupported`
/// (Gap B) rather than silently relabelled as covered.
const CONSTRUCT_COVERAGE: &[(&str, &str, &str)] = &[
    (OWL_COMPLEMENT_OF, "owl:complementOf", "complementOf"),
    (OWL_SOME_VALUES_FROM, "owl:someValuesFrom", "someValuesFrom"),
    (OWL_ALL_VALUES_FROM, "owl:allValuesFrom", "allValuesFrom"),
    (OWL_CARDINALITY, "owl:cardinality", "cardinality"),
    (OWL_MIN_CARDINALITY, "owl:minCardinality", "minCardinality"),
    (OWL_MAX_CARDINALITY, "owl:maxCardinality", "maxCardinality"),
    (
        OWL_QUALIFIED_CARDINALITY,
        "owl:qualifiedCardinality",
        "qualifiedCardinality",
    ),
    (
        OWL_MIN_QUALIFIED_CARDINALITY,
        "owl:minQualifiedCardinality",
        "minQualifiedCardinality",
    ),
    (
        OWL_MAX_QUALIFIED_CARDINALITY,
        "owl:maxQualifiedCardinality",
        "maxQualifiedCardinality",
    ),
    (
        OWL_DISJOINT_UNION_OF,
        "owl:disjointUnionOf",
        "disjointUnionOf",
    ),
    (OWL_UNION_OF, "owl:unionOf", "unionOf"),
    (OWL_ONE_OF, "owl:oneOf", "oneOf"),
    (OWL_HAS_VALUE, "owl:hasValue", "hasValue"),
    (RDFS_DOMAIN, "rdfs:domain", "domain"),
    (RDFS_RANGE, "rdfs:range", "range"),
    (
        OWL_PROPERTY_CHAIN_AXIOM,
        "owl:propertyChainAxiom",
        "propertyChainAxiom",
    ),
    (
        OWL_BOTTOM_OBJECT_PROPERTY,
        "owl:bottomObjectProperty",
        "bottomObjectProperty",
    ),
    (
        OWL_BOTTOM_DATA_PROPERTY,
        "owl:bottomDataProperty",
        "bottomDataProperty",
    ),
    (OWL_HAS_KEY, "owl:hasKey", "hasKey"),
    // The greenfield `logic:KeyAssertion` carrier is the canonical source of a key (the `owl:hasKey`
    // marker above is its lossy OWL-DL view). It maps to the same `hasKey` family suffix, so a key
    // authored only on the carrier — as the object-level reasoning EDB carries it after the
    // `owl:hasKey` slice declaration is migrated — still counts the `hasKey` family present.
    (LOGIC_KEY_ASSERTION, "logic:KeyAssertion", "hasKey"),
    (
        OWL_NEGATIVE_PROPERTY_ASSERTION,
        "owl:NegativePropertyAssertion",
        "negativePropertyAssertion",
    ),
    (
        OWL_FUNCTIONAL_PROPERTY,
        "owl:FunctionalProperty",
        "functionalProperty",
    ),
    // ── Property-characteristic + disjointness/identity clash families (Wave A) ──
    // Each is decided by a sound local clash rule in `augment_with_extra_dl_clashes`
    // (`prp-asyp`/`prp-irp`/`prp-pdw`/`eq-diff` + the list expansions), so they are
    // promoted to `decided` unconditionally-when-present by `classify_coverage`.
    (
        OWL_ASYMMETRIC_PROPERTY,
        "owl:AsymmetricProperty",
        "asymmetricProperty",
    ),
    (
        OWL_IRREFLEXIVE_PROPERTY,
        "owl:IrreflexiveProperty",
        "irreflexiveProperty",
    ),
    (
        OWL_PROPERTY_DISJOINT_WITH,
        "owl:propertyDisjointWith",
        "propertyDisjointWith",
    ),
    (
        OWL_ALL_DISJOINT_PROPERTIES,
        "owl:AllDisjointProperties",
        "allDisjointProperties",
    ),
    (
        OWL_ALL_DISJOINT_CLASSES,
        "owl:AllDisjointClasses",
        "allDisjointClasses",
    ),
    (OWL_ALL_DIFFERENT, "owl:AllDifferent", "allDifferent"),
    // No native inverse-functional identity-merge clash exists in the forward chase
    // (out of EL, absent from the committed bundle + vendored on-gate corpus). The
    // Family-6a counting refutation sub-decider ([`crate::reason::refute::counting`])
    // wires the real inverse-functional `sameAs` propagation and its `differentFrom`
    // clash, so `classify_coverage` promotes this family to `decided` exactly when
    // that sub-decider completely decides the (pure assertional/identity) case —
    // otherwise its presence stays an honest gap, never a silently-ignored axiom.
    (
        OWL_INVERSE_FUNCTIONAL_PROPERTY,
        "owl:InverseFunctionalProperty",
        "inverseFunctionalProperty",
    ),
    // ── Out-of-fragment constructs ──
    // Datatype/facet restrictions (OWL 2 DL/Full) — the native chase carries no
    // datatype-value reasoning, so a facet that would make the ontology
    // inconsistent (e.g. a value outside a `xsd:minInclusive`/`maxInclusive`
    // range) is invisible to it. A facet is withheld (honest gap) ONLY when it
    // actually constrains an asserted/inferred literal; a facet-restricted
    // datatype that is merely DEFINED but constrains no literal is inert and
    // decided (see `datatype_facet_has_live_obligation`). The universal
    // top properties below are always `unsupported`, never `decided`.
    (
        OWL_DATATYPE_COMPLEMENT_OF,
        "owl:datatypeComplementOf",
        "datatypeComplementOf",
    ),
    (
        OWL_WITH_RESTRICTIONS,
        "owl:withRestrictions",
        "withRestrictions",
    ),
    (OWL_ON_DATATYPE, "owl:onDatatype", "onDatatype"),
    (XSD_MIN_INCLUSIVE, "xsd:minInclusive", "minInclusive"),
    (XSD_MAX_INCLUSIVE, "xsd:maxInclusive", "maxInclusive"),
    (XSD_MIN_EXCLUSIVE, "xsd:minExclusive", "minExclusive"),
    (XSD_MAX_EXCLUSIVE, "xsd:maxExclusive", "maxExclusive"),
    (XSD_PATTERN, "xsd:pattern", "pattern"),
    (XSD_LENGTH, "xsd:length", "length"),
    (XSD_MIN_LENGTH, "xsd:minLength", "minLength"),
    (XSD_MAX_LENGTH, "xsd:maxLength", "maxLength"),
    (XSD_TOTAL_DIGITS, "xsd:totalDigits", "totalDigits"),
    (XSD_FRACTION_DIGITS, "xsd:fractionDigits", "fractionDigits"),
    (RDF_LANG_RANGE, "rdf:langRange", "langRange"),
    // Universal (top) properties — 0 production usage, absent from the vendored
    // on-gate corpus, never decided by the native chase (see the constant note above).
    (
        OWL_TOP_OBJECT_PROPERTY,
        "owl:topObjectProperty",
        "topObjectProperty",
    ),
    (
        OWL_TOP_DATA_PROPERTY,
        "owl:topDataProperty",
        "topDataProperty",
    ),
];

/// The OWL 2 datatype-facet construct families (coverage suffixes). The native
/// chase carries no datatype value-space reasoning, so these are undecidable ONLY
/// when a facet actually constrains a literal; a facet-restricted datatype that is
/// merely DEFINED is inert and decided (see
/// [`datatype_facet_has_live_obligation`]). Deliberately EXCLUDES the
/// universal top properties (`owl:topObjectProperty`/`owl:topDataProperty`), which
/// are never decided.
const DATATYPE_FACET_FAMILIES: &[&str] = &[
    "datatypeComplementOf",
    "withRestrictions",
    "onDatatype",
    "minInclusive",
    "maxInclusive",
    "minExclusive",
    "maxExclusive",
    "pattern",
    "length",
    "minLength",
    "maxLength",
    "totalDigits",
    "fractionDigits",
    "langRange",
];

/// Assemble the fast native DL rule set: the fixed typed EL calculus plus
/// native clash detection. Finite DL/profile constructs are then completed by
/// [`augment_inferred_with_dl`].
pub(crate) fn structured_dl_rules() -> Vec<crate::rule_ir::EvalRule> {
    use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
    const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

    let v = EvalTerm::var;
    let n = EvalTerm::named;
    let a = EvalAtom::positive;
    let mut rules = super::el::structured_el_rules();
    rules.extend([
        EvalRule::positive(
            "dl:individual-clash",
            a(v("?i"), TYPE, n(NOTHING)),
            vec![
                a(v("?i"), TYPE, v("?c1")),
                a(v("?i"), TYPE, v("?c2")),
                a(v("?c1"), DISJOINT, v("?c2")),
            ],
        ),
        EvalRule::positive(
            "dl:unsatisfiable-class",
            a(v("?c"), SUBCLASS, n(NOTHING)),
            vec![
                a(v("?c"), SUBCLASS, v("?d")),
                a(v("?c"), SUBCLASS, v("?e")),
                a(v("?d"), DISJOINT, v("?e")),
            ],
        ),
        EvalRule::positive(
            "dl:nothing-membership",
            a(v("?i"), TYPE, n(NOTHING)),
            vec![a(v("?i"), TYPE, v("?c")), a(v("?c"), SUBCLASS, n(NOTHING))],
        ),
    ]);
    rules
}

/// A class proven unsatisfiable: it subsumes two disjoint classes, so it can
/// have no members. Unsatisfiability alone does *not* make the ontology
/// inconsistent — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsatClass {
    pub class: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// An individual forced into `owl:Nothing`: a witness that the ontology is
/// inconsistent in `world`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InconsistencyWitness {
    pub individual: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// Native DL construct-coverage inventory for one reasoning run.
///
/// `present` is the set of construct families whose IRI appears in
/// the input bundle. `decided` is the subset the native Docker-free reasoner
/// can **genuinely** decide the consistency consequences of — i.e. every
/// present instance either produced its defined consequence or is provably
/// complete by construction (see [`classify_coverage`]). `unsupported` is the
/// residual `present \ decided` — a present construct the native path cannot
/// honestly decide — UNIONED with the [`refutation_shape_withholds`]: local
/// beyond-native refutation configurations (a complement/cardinality/union in a
/// class-definition position, nominal/datatype counting, a malformed list) that
/// demote an otherwise-`decided` family, plus a few shape-only markers
/// (`malformedRdfList`, `selfDisjointClass`) that name no inventoried construct.
/// Callers surface `unsupported` through [`DlVerdict::gaps`] and gates fail on it.
///
/// Honesty doctrine: a match-arm existing for a construct is **not** sufficient
/// for `decided`. An inert or incomplete handler (e.g. `owl:someValuesFrom`
/// that does no existential generation, or a cardinality clash that only fires
/// under an explicit `owl:differentFrom`/`max 0`) leaves its construct
/// `unsupported`. Coverage tells the truth; closing those gaps for real is a
/// separate step (Gap B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlCoverage {
    pub present: Vec<String>,
    pub decided: Vec<String>,
    pub unsupported: Vec<String>,
}

/// One native DL coverage defect. This is reasoner-domain evidence, not an RDF
/// representation-conversion loss, so it is owned by GMEOW rather than PurRDF's
/// transcode [`purrdf::LossLedger`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DlGap {
    /// Stable machine-readable gap code.
    pub code: String,
    /// Human-readable explanation of the undecided construct.
    pub message: String,
}

impl DlGap {
    /// Construct a native DL coverage gap.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The verdict of a native DL consistency run.
///
/// `consistent` is `false` iff at least one [`InconsistencyWitness`] was found.
/// `unsatisfiable_classes` lists provably empty classes (which do *not* on their
/// own make the ontology inconsistent). `coverage` records the construct
/// families present and decided by the native path. `gaps` mirrors
/// `coverage.unsupported` for existing consumers and must be empty for the
/// committed bundle.
///
/// `boundary_findings` carries the fragment-certified refutation kernel's
/// FAMILY-SCOPED withholds (see [`crate::reason::refute::production_boundary_findings`]):
/// a family shape was present but its completeness bound did not close, so the kernel
/// surfaces an honest, ledger-identified "outside the certified fragment" finding
/// stamped with [`crate::reason::refute::REFUTATION_KERNEL_CATEGORY`]. It is a
/// Coherent `UnsupportedSemanticFeature` that can NEVER gate, and it is EMPTY on the
/// committed bundle and every gated corpus input (the kernel's steady state there is
/// `NoDeciderEngaged`, which emits nothing), so it changes no reasoning verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlVerdict {
    pub consistent: bool,
    pub unsatisfiable_classes: Vec<UnsatClass>,
    pub inconsistencies: Vec<InconsistencyWitness>,
    pub coverage: DlCoverage,
    pub gaps: Vec<DlGap>,
    pub boundary_findings: Vec<gmeow_errors::Finding>,
}

/// Strip a decoded object display form (`<iri>`) back to the bare IRI.
/// Non-IRI forms are returned unchanged.
fn unwrap_iri(display: &str) -> &str {
    display
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(display)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Fact {
    subject: String,
    predicate: String,
    object: String,
    world: String,
}

impl Fact {
    fn new(subject: String, predicate: String, object: String, world: String) -> Self {
        Self {
            subject,
            predicate,
            object,
            world,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Restriction {
    on_property: Option<String>,
    on_class: Option<String>,
    on_data_range: Option<String>,
    some_values_from: Option<String>,
    all_values_from: Option<String>,
    has_value: Option<String>,
    cardinality: Option<usize>,
    min_cardinality: Option<usize>,
    max_cardinality: Option<usize>,
    qualified_cardinality: Option<usize>,
    min_qualified_cardinality: Option<usize>,
    max_qualified_cardinality: Option<usize>,
}

fn fact_from_axiom(ax: &InferredAxiom) -> Option<Fact> {
    let object = unwrap_iri(&ax.object);
    if object.starts_with('"') {
        return None;
    }
    Some(Fact::new(
        unwrap_iri(&ax.subject).to_owned(),
        ax.predicate.clone(),
        object.to_owned(),
        unwrap_iri(&ax.world).to_owned(),
    ))
}

fn term_resource_key(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.clone()),
        RdfTerm::BlankNode(id) => Some(skolem_iri(id)),
        RdfTerm::Literal(_) | RdfTerm::Triple(_) => None,
    }
}

/// A comparison key for an RDF term that distinguishes resources from literals
/// and distinguishes literals by their full value (lexical form + datatype +
/// language tag). Two terms are the SAME OWL value iff their keys are equal.
///
/// Resources use a `R\u{1f}<iri>` prefix; literals use a `L\u{1f}…` prefix so a
/// literal can never collide with a resource of the same spelling. The literal
/// key folds lexical form, datatype, and language tag together because the
/// equality / negative-assertion / key checks below are literal-aware and must
/// treat `"5"^^xsd:integer` and `"5"^^xsd:string` as distinct values.
fn term_value_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => format!("R\u{1f}{iri}"),
        RdfTerm::BlankNode(id) => format!("R\u{1f}{}", skolem_iri(id)),
        RdfTerm::Literal(lit) => format!(
            "L\u{1f}{}\u{1f}{}\u{1f}{}",
            lit.lexical_form,
            lit.datatype.as_deref().unwrap_or(""),
            lit.language.as_deref().unwrap_or("")
        ),
        RdfTerm::Triple(_) => "T\u{1f}".to_owned(),
    }
}

fn graph_world_key(graph_name: &Option<RdfTerm>) -> String {
    match graph_name {
        Some(RdfTerm::Iri(iri)) => iri.clone(),
        Some(RdfTerm::BlankNode(id)) => skolem_iri(id),
        _ => crate::reason::rl::DEFAULT_WORLD.to_owned(),
    }
}

fn literal_usize(lit: &RdfLiteral) -> Option<usize> {
    match lit.datatype.as_deref() {
        Some(XSD_NON_NEGATIVE_INTEGER)
        | Some(XSD_INTEGER)
        | Some("http://www.w3.org/2001/XMLSchema#int")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedInt")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedLong")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedShort")
        | Some("http://www.w3.org/2001/XMLSchema#unsignedByte")
        | None => lit.lexical_form.parse::<usize>().ok(),
        _ => None,
    }
}

fn add_inferred_fact(
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
    fact: Fact,
    rule_name: &str,
    premises: Vec<(String, String, String)>,
) -> bool {
    if !facts.insert(fact.clone()) {
        return false;
    }
    inferred.push(InferredAxiom {
        subject: fact.subject,
        predicate: fact.predicate,
        object: format!("<{}>", fact.object),
        world: fact.world,
        is_edb: false,
        rule_name: Some(rule_name.to_owned()),
        premises,
    });
    true
}

/// Materialize a fragment-certified refutation kernel verdict into the closure.
///
/// The unified beyond-Horn kernel ([`crate::reason::refute`]) decides a
/// precisely-characterized COMPLETE fragment of the constructs this forward chase
/// withholds. An `InFragment{Inconsistent}` decision carries one
/// [`crate::reason::refute::NothingClash`] per forced-empty individual; each is
/// added as the `type(?i, owl:Nothing, ?w)` witness [`verdict_from_inferred`]
/// reads off, with the deciding rule name and clash premises preserved. An
/// `InFragment{Consistent}` decision materializes NO clash (its family is promoted
/// to `decided` by [`classify_coverage`], per family, in the later kernel tasks),
/// and an `OutOfFragment` withhold materializes nothing (the family stays an
/// honest gap).
///
/// Returns whether any witness fact was added, so the caller can fold the kernel
/// into its fixpoint if it ever decides on materialized facts. Task 2 registers no
/// family sub-decider, so on every real closure the kernel returns `OutOfFragment`
/// and this is a strict no-op — no current verdict changes — while still being
/// CALLED on the production path (no dark code).
fn materialize_refutation(
    certificate: &crate::reason::refute::RefutationCertificate,
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
) -> bool {
    use crate::reason::refute::{Decision, RefutationCertificate};
    let RefutationCertificate::InFragment { decision, witness } = certificate else {
        return false;
    };
    if *decision != Decision::Inconsistent {
        return false;
    }
    let mut added = false;
    for clash in &witness.clashes {
        added |= add_inferred_fact(
            inferred,
            facts,
            Fact::new(
                clash.individual.clone(),
                RDF_TYPE.to_owned(),
                OWL_NOTHING.to_owned(),
                clash.world.clone(),
            ),
            &clash.rule_name,
            clash.premises.clone(),
        );
    }
    added
}

fn quads_by_subject(edb: &RdfDataset) -> Vec<(String, String, RdfTerm, String)> {
    let mut rows = Vec::new();
    for quad in edb.owned_quads() {
        if let Some(subject) = term_resource_key(&quad.subject) {
            rows.push((
                subject,
                quad.predicate,
                quad.object,
                graph_world_key(&quad.graph_name),
            ));
        }
    }
    rows
}

fn read_restrictions(edb: &RdfDataset) -> BTreeMap<(String, String), Restriction> {
    let mut restrictions: BTreeMap<(String, String), Restriction> = BTreeMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            OWL_ON_PROPERTY => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.on_property = Some(value);
                }
            }
            OWL_ON_CLASS => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.on_class = Some(value);
                }
            }
            OWL_ON_DATA_RANGE => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.on_data_range = Some(value);
                }
            }
            OWL_SOME_VALUES_FROM => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.some_values_from = Some(value);
                }
            }
            OWL_ALL_VALUES_FROM => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.all_values_from = Some(value);
                }
            }
            OWL_HAS_VALUE => {
                if let Some(value) = term_resource_key(&object) {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.has_value = Some(value);
                }
            }
            OWL_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.cardinality = literal_usize(&lit);
                }
            }
            OWL_MIN_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.min_cardinality = literal_usize(&lit);
                }
            }
            OWL_MAX_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.max_cardinality = literal_usize(&lit);
                }
            }
            OWL_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.qualified_cardinality = literal_usize(&lit);
                }
            }
            OWL_MIN_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.min_qualified_cardinality = literal_usize(&lit);
                }
            }
            OWL_MAX_QUALIFIED_CARDINALITY => {
                if let RdfTerm::Literal(lit) = object {
                    let entry = restrictions
                        .entry((world.clone(), subject.clone()))
                        .or_default();
                    entry.max_qualified_cardinality = literal_usize(&lit);
                }
            }
            _ => {}
        }
    }
    restrictions
}

fn read_lists(edb: &RdfDataset) -> HashMap<(String, String), Vec<String>> {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

    let mut first: HashMap<(String, String), String> = HashMap::new();
    let mut rest: HashMap<(String, String), String> = HashMap::new();
    // Track which nodes appear as the `rdf:rest` target of another node — these
    // are interior nodes, not heads. Only nodes NOT in this set are heads.
    let mut rest_targets: HashSet<(String, String)> = HashSet::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            RDF_FIRST => {
                if let Some(value) = term_resource_key(&object) {
                    first.insert((world, subject), value);
                }
            }
            RDF_REST => {
                if let Some(value) = term_resource_key(&object) {
                    // Record this node's rest-target so we can exclude interior
                    // sublists from being mistaken for heads.
                    if value != RDF_NIL {
                        rest_targets.insert((world.clone(), value.clone()));
                    }
                    rest.insert((world, subject), value);
                }
            }
            _ => {}
        }
    }

    // Single forward pass: only walk from true list heads (nodes that have a
    // `rdf:first` but are NOT pointed to by any `rdf:rest` — i.e. are not
    // interior nodes of another list). Each node in the EDB is visited at most
    // once across the whole loop, giving O(L) total rather than O(L²).
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (key, head_value) in &first {
        let (world, head_node) = key;
        if rest_targets.contains(&(world.clone(), head_node.clone())) {
            // This node is an interior node of a longer list; its sublist will
            // be reachable when we walk from the true head.
            continue;
        }
        // Walk the chain from this head to rdf:nil, collecting members.
        let mut node = head_node.clone();
        let mut seen: HashSet<String> = HashSet::new();
        let mut members = Vec::new();
        members.push(head_value.clone());
        seen.insert(node.clone());
        while let Some(next) = rest.get(&(world.clone(), node.clone())) {
            if next == RDF_NIL || !seen.insert(next.clone()) {
                break;
            }
            if let Some(value) = first.get(&(world.clone(), next.clone())) {
                members.push(value.clone());
            }
            node = next.clone();
        }
        out.insert((world.clone(), head_node.clone()), members);
    }
    out
}

fn raw_resource_facts(edb: &RdfDataset) -> Vec<Fact> {
    let mut rows = Vec::new();
    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let (Some(subject), Some(object)) =
            (term_resource_key(&subject), term_resource_key(&object))
        else {
            continue;
        };
        rows.push(Fact::new(
            subject,
            predicate,
            object,
            graph_world_key(&graph_name),
        ));
    }
    rows
}

/// Resource facts, grouped by the exact world key the finite DL pass uses, in the
/// fixed-rule evaluator's public fact shape.
///
/// Leave-one-out batching uses this bridge so default graphs, named graphs, blank
/// graph names, and skolemized resource terms enter the incremental closure with
/// the same identity as [`augment_inferred_with_dl`].
pub(crate) fn fixed_rule_resource_facts(edb: &RdfDataset) -> Vec<(String, crate::rule_ir::Fact)> {
    raw_resource_facts(edb)
        .into_iter()
        .map(|fact| {
            (
                fact.world,
                crate::rule_ir::Fact {
                    subject: TermValue::iri(fact.subject),
                    predicate: fact.predicate,
                    object: TermValue::iri(fact.object),
                },
            )
        })
        .collect()
}

/// World keys admitted by the fixed structured-rule adapter.
///
/// The adapter intentionally evaluates only named-IRI graphs; the finite DL
/// post-pass additionally handles default and blank-node graph names. Batch
/// indexes consult this set for rule families (notably `owl:equivalentClass`)
/// implemented only by the structured adapter.
pub(crate) fn structured_rule_worlds(edb: &RdfDataset) -> BTreeSet<String> {
    edb.owned_quads()
        .filter_map(|quad| match quad.graph_name {
            Some(RdfTerm::Iri(world)) => Some(world),
            _ => None,
        })
        .collect()
}

/// Non-`owl:Nothing` subclass edges introduced by finite union expansion.
///
/// These are the only fixed-predicate subclass heads the finite DL post-pass adds
/// outside unsatisfiability. Surfacing them lets the exact leave-one-out batch run
/// transitive-reduction probes over one complete direct class graph instead of
/// invoking the whole post-pass once per candidate.
pub(crate) fn finite_dl_subclass_edges(edb: &RdfDataset) -> Vec<(String, String, String)> {
    let lists = read_lists(edb);
    let mut edges = Vec::new();
    for fact in raw_resource_facts(edb) {
        if fact.predicate != OWL_UNION_OF && fact.predicate != OWL_DISJOINT_UNION_OF {
            continue;
        }
        let Some(members) = lists.get(&(fact.world.clone(), fact.object)) else {
            continue;
        };
        edges.extend(
            members
                .iter()
                .map(|member| (fact.world.clone(), member.clone(), fact.subject.clone())),
        );
    }
    edges
}

/// Candidate `owl:disjointWith` pairs introduced by the finite DL post-pass.
///
/// `owl:members` lists are intentionally treated as disjoint-class lists even
/// before checking their owner's type. That is a safe over-approximation for the
/// leave-one-out negative filter: a pair absent here cannot be produced by the
/// complement, disjoint-union, or all-disjoint-class handlers.
pub(crate) fn finite_dl_disjoint_candidates(edb: &RdfDataset) -> Vec<(String, String, String)> {
    let lists = read_lists(edb);
    let mut pairs = BTreeSet::new();
    for fact in raw_resource_facts(edb) {
        if fact.predicate == OWL_COMPLEMENT_OF {
            pairs.insert((
                fact.world.clone(),
                fact.subject.clone(),
                fact.object.clone(),
            ));
            pairs.insert((fact.world, fact.object, fact.subject));
            continue;
        }
        if fact.predicate != OWL_DISJOINT_UNION_OF && fact.predicate != OWL_MEMBERS {
            continue;
        }
        let Some(members) = lists.get(&(fact.world.clone(), fact.object)) else {
            continue;
        };
        for (index, left) in members.iter().enumerate() {
            for (other_index, right) in members.iter().enumerate() {
                if index != other_index {
                    pairs.insert((fact.world.clone(), left.clone(), right.clone()));
                }
            }
        }
    }
    pairs.into_iter().collect()
}

fn build_index(facts: &BTreeSet<Fact>) -> HashMap<(String, String, String), BTreeSet<String>> {
    let mut index: HashMap<(String, String, String), BTreeSet<String>> = HashMap::new();
    for fact in facts {
        index
            .entry((
                fact.world.clone(),
                fact.subject.clone(),
                fact.predicate.clone(),
            ))
            .or_default()
            .insert(fact.object.clone());
    }
    index
}

fn build_predicate_index(
    facts: &BTreeSet<Fact>,
) -> HashMap<(String, String), Vec<(String, String)>> {
    let mut index: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for fact in facts {
        index
            .entry((fact.world.clone(), fact.predicate.clone()))
            .or_default()
            .push((fact.subject.clone(), fact.object.clone()));
    }
    index
}

fn objects_for<'a>(
    index: &'a HashMap<(String, String, String), BTreeSet<String>>,
    world: &str,
    subject: &str,
    predicate: &str,
) -> impl Iterator<Item = &'a String> + use<'a> {
    index
        .get(&(world.to_owned(), subject.to_owned(), predicate.to_owned()))
        .into_iter()
        .flat_map(|objects| objects.iter())
}

fn edges_for<'a>(
    index: &'a HashMap<(String, String), Vec<(String, String)>>,
    world: &str,
    predicate: &str,
) -> impl Iterator<Item = &'a (String, String)> + use<'a> {
    index
        .get(&(world.to_owned(), predicate.to_owned()))
        .into_iter()
        .flat_map(|edges| edges.iter())
}

fn has_fact(
    facts: &BTreeSet<Fact>,
    world: &str,
    subject: &str,
    predicate: &str,
    object: &str,
) -> bool {
    facts.contains(&Fact::new(
        subject.to_owned(),
        predicate.to_owned(),
        object.to_owned(),
        world.to_owned(),
    ))
}

/// True iff `a` and `b` are **provably** distinct individuals — i.e. an explicit
/// `owl:differentFrom` relates them.
///
/// Standard OWL makes **no** unique-name assumption: two named resources with
/// different IRIs may be `owl:sameAs` and are consistent absent evidence to the
/// contrary. So distinctness is asserted ONLY by an explicit `owl:differentFrom`;
/// two merely differently-spelled IRIs are NOT distinct. This is the soundness
/// floor for the functional-property and max-cardinality clash rules: firing them
/// on a UNA default would report a false `inconsistent` on a consistent ontology
/// (the OWL-2 `-inst`/`-obj-one` regression). Chase witnesses invented for a
/// `≥n` obligation are made pairwise distinct by materialising explicit
/// `owl:differentFrom` between them (their min-cardinality axiom entails it), so
/// they still satisfy this guard without a UNA shortcut.
fn distinct_individuals(facts: &BTreeSet<Fact>, world: &str, a: &str, b: &str) -> bool {
    if a == b {
        return false;
    }
    // Distinctness requires an EXPLICIT `owl:differentFrom` (either direction). A
    // co-asserted contradictory `owl:sameAs` does not suppress it — that pair is
    // itself inconsistent and is caught by the `eq-diff` clash.
    has_fact(facts, world, a, OWL_DIFFERENT_FROM, b)
        || has_fact(facts, world, b, OWL_DIFFERENT_FROM, a)
}

/// True iff the `fillers` are pairwise **provably** distinct — every pair related
/// by an explicit `owl:differentFrom` ([`distinct_individuals`], no unique-name
/// assumption). Used to decide whether `> max` fillers genuinely violate a
/// maximum, and whether existing fillers already witness a `≥n` minimum.
fn pairwise_distinct(facts: &BTreeSet<Fact>, world: &str, fillers: &[&String]) -> bool {
    for i in 0..fillers.len() {
        for j in (i + 1)..fillers.len() {
            if !distinct_individuals(facts, world, fillers[i].as_str(), fillers[j].as_str()) {
                return false;
            }
        }
    }
    true
}

fn cardinality_maxima(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut maxima = Vec::new();
    if let Some(n) = restriction.cardinality {
        maxima.push((n, None));
    }
    if let Some(n) = restriction.max_cardinality {
        maxima.push((n, None));
    }
    // A DATATYPE-qualified cardinality (`owl:onDataRange`) counts literal fillers,
    // which the IRI-individual chase does not carry — it is decided as inert (or
    // honestly withheld on a live violation) by `classify_coverage`, never counted
    // here as if unqualified. So a qualified maximum contributes to the chase only
    // for its OBJECT-qualified (`owl:onClass`) or unqualified reading.
    if restriction.on_data_range.is_none() {
        if let Some(n) = restriction.qualified_cardinality {
            maxima.push((n, restriction.on_class.as_deref()));
        }
        if let Some(n) = restriction.max_qualified_cardinality {
            maxima.push((n, restriction.on_class.as_deref()));
        }
    }
    maxima
}

fn cardinality_minima(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut minima = Vec::new();
    if let Some(n) = restriction.cardinality {
        minima.push((n, None));
    }
    if let Some(n) = restriction.min_cardinality {
        minima.push((n, None));
    }
    // A DATATYPE-qualified minimum (`owl:onDataRange`) is an existential into a
    // non-empty datatype — satisfiable without inventing an IRI witness (which
    // would be a phantom, not a literal). It is decided as inert by
    // `classify_coverage`, so it contributes no IRI-individual obligation here.
    if restriction.on_data_range.is_none() {
        if let Some(n) = restriction.qualified_cardinality {
            minima.push((n, restriction.on_class.as_deref()));
        }
        if let Some(n) = restriction.min_qualified_cardinality {
            minima.push((n, restriction.on_class.as_deref()));
        }
    }
    minima
}

/// The existential-filler obligations a restriction imposes on each of its
/// instances: `(needed, filler_class)` where `needed` distinct property fillers
/// of type `filler_class` (or `⊤` when `None`) must exist.
///
/// `someValuesFrom D` is the qualified `≥1 p.D`; the cardinality *minima*
/// (`cardinality`, `minCardinality`, `qualifiedCardinality`,
/// `minQualifiedCardinality`) contribute their `≥n` lower bound, qualified by
/// `onClass` when present. These are the obligations the chase discharges by
/// inventing scoped Skolem witnesses through the native restricted chase.
fn existential_obligations(restriction: &Restriction) -> Vec<(usize, Option<&str>)> {
    let mut obligations: Vec<(usize, Option<&str>)> = Vec::new();
    if let Some(class) = restriction.some_values_from.as_deref() {
        obligations.push((1, Some(class)));
    }
    for (n, on_class) in cardinality_minima(restriction) {
        if n > 0 {
            obligations.push((n, on_class));
        }
    }
    obligations
}

/// Namespace for authored general existential rules (the surface that lets a slice
/// express an arbitrary `∀x. φ(x) → ∃y. ψ(x, y)` rule — not just an OWL restriction —
/// and have its per-world termination class certified into `gmeow.gts`).
const LOGIC_EXISTENTIAL_NS: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";

/// The Skolem-analysis view of an authored-rule term: an IRI is a constant, a string
/// literal starting `?` is a variable.  Anything else is malformed and skipped.
fn authored_rule_term(term: &RdfTerm) -> Option<crate::rule_ir::EvalTerm> {
    use crate::rule_ir::EvalTerm;
    match term {
        RdfTerm::Iri(iri) => Some(EvalTerm::named(iri)),
        RdfTerm::Literal(lit) if lit.lexical_form.starts_with('?') => {
            Some(EvalTerm::var(&lit.lexical_form))
        }
        _ => None,
    }
}

/// Read authored general existential rules from `edb`, grouped by the reasoning world
/// (named graph) they are authored in.  Each `logicx:ExistentialRule` carries
/// `logicx:body`/`logicx:head` atom nodes, each with `logicx:s`/`logicx:p`/`logicx:o`
/// (subject/predicate/object).  Existential head variables (head vars the body does not
/// bind) are derived automatically by [`crate::physical::ExistentialRule::existentials`],
/// so no explicit `∃` marker is needed.  The produced rules are merged into the same
/// per-world map the OWL-restriction lowering feeds, so they flow through the identical
/// per-world `ChaseAdmission::certify` → shipped-certificate path.
///
/// A **declared-but-malformed** authored rule is a HARD FAIL, never a silent drop: an atom
/// node referenced by `logicx:body`/`logicx:head` whose `logicx:s`/`logicx:p`/`logicx:o` is
/// missing or malformed, or a rule that assembles to an empty body or head, returns an error
/// (no-optionality: silently dropping a body conjunct would *broaden* the rule and derive
/// facts never authored).
pub(crate) fn authored_existential_rules(
    edb: &RdfDataset,
) -> gmeow_errors::Result<BTreeMap<String, Vec<crate::physical::ExistentialRule>>> {
    use crate::rule_ir::EvalAtom;
    let ns = |local: &str| format!("{LOGIC_EXISTENTIAL_NS}{local}");
    let (rule_type, body_p, head_p, s_p, p_p, o_p) = (
        ns("ExistentialRule"),
        ns("body"),
        ns("head"),
        ns("s"),
        ns("p"),
        ns("o"),
    );

    let mut is_rule: BTreeSet<(String, String)> = BTreeSet::new();
    let mut body_nodes: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut head_nodes: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut atom_s: BTreeMap<(String, String), RdfTerm> = BTreeMap::new();
    let mut atom_p: BTreeMap<(String, String), RdfTerm> = BTreeMap::new();
    let mut atom_o: BTreeMap<(String, String), RdfTerm> = BTreeMap::new();

    // Collection-time hard-fails: a `logicx:body`/`head` that does not name a resource, or a
    // duplicate `logicx:s`/`p`/`o` on one atom node, silently alter the authored rule and are
    // rejected the moment they are seen (no-optionality — never a silent broaden/overwrite).
    let non_resource_ref = |slot: &str, world: &str, rule: &str, value: &RdfTerm| {
        reason_err(format!(
            "authored existential-rule <{rule}> in world <{world}> has a logicx:{slot} value \
             that is not a resource ({value:?}); logicx:{slot} must reference an atom-node IRI"
        ))
    };
    let duplicate_slot = |slot: &str, world: &str, node: &str| {
        reason_err(format!(
            "authored existential-rule atom <{node}> in world <{world}> has more than one \
             logicx:{slot}; each atom slot must be authored exactly once"
        ))
    };

    for (subject, predicate, object, world) in quads_by_subject(edb) {
        let key = (world.clone(), subject.clone());
        match predicate.as_str() {
            RDF_TYPE if matches!(&object, RdfTerm::Iri(iri) if *iri == rule_type) => {
                is_rule.insert(key);
            }
            p if p == body_p => match term_resource_key(&object) {
                Some(node) => {
                    body_nodes.entry(key).or_default().insert(node);
                }
                // A non-resource `logicx:body` (a literal or quoted triple) cannot name an
                // atom node. Silently dropping it would leave the rule with FEWER body
                // conjuncts than authored — a broadening (no-optionality). HARD FAIL.
                None => return Err(non_resource_ref("body", &world, &subject, &object)),
            },
            p if p == head_p => match term_resource_key(&object) {
                Some(node) => {
                    head_nodes.entry(key).or_default().insert(node);
                }
                // A non-resource `logicx:head` would silently drop a head atom — a
                // weakening of the authored rule. HARD FAIL.
                None => return Err(non_resource_ref("head", &world, &subject, &object)),
            },
            // A second `logicx:s`/`p`/`o` on one atom node would silently OVERWRITE the
            // first, reinterpreting the authored atom. A duplicate slot is malformed
            // authoring — HARD FAIL rather than pick a winner (no-optionality).
            p if p == s_p => {
                let duplicate = atom_s.insert(key, object).is_some();
                if duplicate {
                    return Err(duplicate_slot("s", &world, &subject));
                }
            }
            p if p == p_p => {
                let duplicate = atom_p.insert(key, object).is_some();
                if duplicate {
                    return Err(duplicate_slot("p", &world, &subject));
                }
            }
            p if p == o_p => {
                let duplicate = atom_o.insert(key, object).is_some();
                if duplicate {
                    return Err(duplicate_slot("o", &world, &subject));
                }
            }
            _ => {}
        }
    }

    // Assemble one atom (deterministic across authoring order) from its node key. A
    // declared atom node that cannot assemble a well-formed atom is a HARD FAIL, never a
    // silent drop (dropping a body conjunct would broaden the rule; dropping a head atom
    // would weaken it — either way silently degrades the authored semantics).
    let malformed_term = |slot: &str, world: &str, node: &str| {
        reason_err(format!(
            "authored existential-rule atom <{node}> in world <{world}> has a malformed \
             logicx:{slot} (expected an IRI constant or a \"?variable\" literal)"
        ))
    };
    let missing_slot = |slot: &str, world: &str, node: &str| {
        reason_err(format!(
            "authored existential-rule atom <{node}> in world <{world}> is missing its \
             logicx:{slot}"
        ))
    };
    let atom_of = |world: &str, node: &str| -> gmeow_errors::Result<EvalAtom> {
        let key = (world.to_owned(), node.to_owned());
        let subject = authored_rule_term(
            atom_s
                .get(&key)
                .ok_or_else(|| missing_slot("s", world, node))?,
        )
        .ok_or_else(|| malformed_term("s", world, node))?;
        let predicate = match atom_p
            .get(&key)
            .ok_or_else(|| missing_slot("p", world, node))?
        {
            RdfTerm::Iri(iri) => iri.clone(),
            _ => return Err(malformed_term("p", world, node)),
        };
        let object = authored_rule_term(
            atom_o
                .get(&key)
                .ok_or_else(|| missing_slot("o", world, node))?,
        )
        .ok_or_else(|| malformed_term("o", world, node))?;
        Ok(EvalAtom::positive(subject, &predicate, object))
    };

    let mut by_world: BTreeMap<String, Vec<crate::physical::ExistentialRule>> = BTreeMap::new();
    for (world, rule) in is_rule {
        let key = (world.clone(), rule.clone());
        let mut body = Vec::new();
        for node in body_nodes.get(&key).into_iter().flatten() {
            body.push(atom_of(&world, node)?);
        }
        let mut head = Vec::new();
        for node in head_nodes.get(&key).into_iter().flatten() {
            head.push(atom_of(&world, node)?);
        }
        if body.is_empty() || head.is_empty() {
            return Err(reason_err(format!(
                "authored existential rule <{rule}> in world <{world}> is malformed: assembled \
                 {} body atom(s) and {} head atom(s) (both must be non-empty)",
                body.len(),
                head.len()
            )));
        }
        by_world
            .entry(world)
            .or_default()
            .push(crate::physical::ExistentialRule {
                rule_iri: rule,
                body,
                head,
                distinct: vec![],
                witness_frontier: None,
                witness_policy: crate::physical::WitnessPolicy::FrontierSkolem,
            });
    }
    Ok(by_world)
}

fn structured_existential_rules(
    restrictions: &BTreeMap<(String, String), Restriction>,
    edb: &RdfDataset,
) -> (
    BTreeMap<String, Vec<crate::physical::ExistentialRule>>,
    bool,
) {
    use crate::rule_ir::{EvalAtom, EvalTerm};
    // Whether any `≥n` obligation exceeded `MAX_EXISTENTIAL_WITNESSES` and was NOT
    // lowered — the caller records an honest INCOMPLETE withhold for it.
    let mut oversized_withheld = false;

    const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    let datatype_properties = quads_by_subject(edb)
        .into_iter()
        .filter_map(|(subject, predicate, object, world)| {
            (predicate == RDF_TYPE
                && matches!(object, RdfTerm::Iri(ref iri) if iri == DATATYPE_PROPERTY))
            .then_some((world, subject))
        })
        .collect::<BTreeSet<_>>();
    let mut by_world: BTreeMap<String, Vec<crate::physical::ExistentialRule>> = BTreeMap::new();
    for ((world, restriction_iri), restriction) in restrictions {
        let Some(property) = restriction.on_property.as_deref() else {
            continue;
        };
        if restriction.on_data_range.is_some()
            || datatype_properties.contains(&(world.clone(), property.to_owned()))
        {
            continue;
        }
        for (needed, on_class) in existential_obligations(restriction) {
            // A `≥n` beyond the materialization bound is not lowered: building its
            // n·(n−1)/2-atom quadratic head would exhaust memory before the chase runs.
            // Withhold an honest INCOMPLETE instead of constructing an unbounded rule.
            if needed > MAX_EXISTENTIAL_WITNESSES {
                oversized_withheld = true;
                continue;
            }
            let witnesses = (0..needed)
                .map(|ordinal| format!("?witness{ordinal}"))
                .collect::<Vec<_>>();
            let mut head = Vec::new();
            for witness in &witnesses {
                head.push(EvalAtom::positive(
                    EvalTerm::var("?subject"),
                    property,
                    EvalTerm::var(witness),
                ));
                if let Some(class) = on_class {
                    head.push(EvalAtom::positive(
                        EvalTerm::var(witness),
                        RDF_TYPE,
                        EvalTerm::named(class),
                    ));
                }
            }
            let mut distinct = Vec::new();
            for left in 0..witnesses.len() {
                for right in (left + 1)..witnesses.len() {
                    distinct.push((witnesses[left].clone(), witnesses[right].clone()));
                    head.push(EvalAtom::positive(
                        EvalTerm::var(&witnesses[left]),
                        OWL_DIFFERENT_FROM,
                        EvalTerm::var(&witnesses[right]),
                    ));
                }
            }
            let identity = format!(
                "{world}\u{1f}{restriction_iri}\u{1f}{property}\u{1f}{needed}\u{1f}{}",
                on_class.unwrap_or(OWL_THING)
            );
            by_world
                .entry(world.clone())
                .or_default()
                .push(crate::physical::ExistentialRule {
                    rule_iri: format!(
                        "https://blackcatinformatics.ca/gmeow/logic/rule/dl-existential/{}",
                        crate::provenance::sha1_hex(&identity)
                    ),
                    body: vec![EvalAtom::positive(
                        EvalTerm::var("?subject"),
                        RDF_TYPE,
                        EvalTerm::named(restriction_iri),
                    )],
                    head,
                    distinct,
                    // Let the chase derive the frontier from the rule shape. The
                    // bound subject occurs in both body and head, so each subject's
                    // existential obligation receives its own deterministic witness.
                    witness_frontier: None,
                    witness_policy: crate::physical::WitnessPolicy::DlAncestorBlocking,
                });
        }
    }
    (by_world, oversized_withheld)
}

/// Record the sound INCOMPLETE withhold marker on the closure: the existential chase
/// stopped at [`CHASE_STEP_BACKSTOP`]/[`MAX_CHASE_FACTS`] before its fixpoint closed,
/// so the run cannot certify a `consistent`/`inconsistent` decision.
///
/// The marker is idempotent (recorded once) and lives ONLY in `inferred` — it is never
/// inserted into `facts`, so it cannot perturb any downstream rule firing;
/// [`verdict_from_inferred`] reads its presence and folds it into [`DlVerdict::gaps`].
fn record_chase_materialization_withhold(inferred: &mut Vec<InferredAxiom>) {
    if inferred
        .iter()
        .any(|ax| ax.predicate == CHASE_INCOMPLETE_MARKER_PRED)
    {
        return;
    }
    inferred.push(InferredAxiom {
        subject: CHASE_INCOMPLETE_MARKER_SUBJECT.to_owned(),
        predicate: CHASE_INCOMPLETE_MARKER_PRED.to_owned(),
        object: format!("<{CHASE_INCOMPLETE_MARKER_SUBJECT}>"),
        world: CHASE_INCOMPLETE_MARKER_SUBJECT.to_owned(),
        is_edb: false,
        rule_name: Some("dl:chase-materialization-backstop".to_owned()),
        premises: Vec::new(),
    });
}

/// Run the structured existential chase over each world's rules.
///
/// Returns the per-world chase certificates and whether the chase was cut short by the
/// materialization backstop ([`CHASE_STEP_BACKSTOP`]) in ANY world — an exhausted world
/// yields a sound partial prefix, and the caller records the withhold and stops.
fn run_structured_existential_chase(
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
    rules_by_world: &BTreeMap<String, Vec<crate::physical::ExistentialRule>>,
    witness_registries: &mut BTreeMap<String, crate::physical::SkolemRegistry>,
) -> gmeow_errors::Result<(Vec<crate::reason::ChaseCertificate>, bool)> {
    let mut certificates = Vec::new();
    let mut incomplete = false;
    for (world, rules) in rules_by_world {
        if rules.is_empty() {
            continue;
        }
        let edb = facts
            .iter()
            .filter(|fact| fact.world == *world)
            .map(|fact| crate::rule_ir::Fact {
                subject: TermValue::iri(fact.subject.clone()),
                predicate: fact.predicate.clone(),
                object: TermValue::iri(fact.object.clone()),
            })
            .collect::<Vec<_>>();
        let registry = witness_registries.entry(world.clone()).or_default();
        let (admission, outcome) = crate::physical::route_chase_with_registry_backstopped(
            world,
            &edb,
            rules,
            CHASE_STEP_BACKSTOP,
            registry,
        )?;
        let budgeted = match outcome {
            crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
            crate::physical::NativeOutcome::Unsupported(kind) => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                    detail: format!(
                        "structured DL existential chase refused {kind:?}: {:?}",
                        admission.capability_gap_rows()
                    ),
                }));
            }
        };
        // A certified program that hit the step backstop terminated INCOMPLETE: its rows
        // are a sound prefix, but the fixpoint is not closed, so the run must withhold.
        if budgeted.status == crate::seam::BudgetStatus::Exhausted {
            incomplete = true;
        }
        certificates.push(crate::reason::ChaseCertificate {
            world: world.clone(),
            admission,
        });
        for row in budgeted.rows {
            if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
                continue;
            }
            let premises = row
                .antecedents
                .iter()
                .map(|premise| {
                    let subject = match &premise.subject {
                        TermValue::Iri(iri) => iri.clone(),
                        other => {
                            return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                                detail: format!(
                                    "existential chase premise has non-IRI subject {other:?}"
                                ),
                            }));
                        }
                    };
                    Ok((
                        subject,
                        premise.predicate.clone(),
                        crate::provenance::term_display(&premise.object),
                    ))
                })
                .collect::<gmeow_errors::Result<Vec<_>>>()?;
            let subject = match row.subject {
                TermValue::Iri(iri) => iri,
                other => {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                        detail: format!("existential chase emitted non-IRI subject {other:?}"),
                    }));
                }
            };
            let object = match row.object {
                TermValue::Iri(iri) => iri,
                other => {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                        detail: format!("existential chase emitted non-IRI object {other:?}"),
                    }));
                }
            };
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(subject, row.predicate, object, row.graph),
                &row.rule_iri,
                premises,
            );
        }
    }
    Ok((certificates, incomplete))
}

/// Add DL-only finite consistency consequences to the closure.
///
/// The RL generic-triple engine owns positive entailment. This pass owns the DL
/// checks that are not natural positive RL facts: complement/disjoint-union
/// clashes, unsatisfiable restrictions, existential value-invention (the chase),
/// and cardinality clashes under the bundle's identity stance.
pub(crate) fn augment_inferred_with_dl(
    inferred: &mut Vec<InferredAxiom>,
    edb: &RdfDataset,
) -> gmeow_errors::Result<()> {
    augment_inferred_with_dl_certificates(inferred, edb).map(|_| ())
}

/// Add the finite DL consequences and retain every production existential-chase
/// termination certificate that admitted a real rule program, together with the
/// decomposable derivation of every invented null the chase minted.
///
/// The witness derivations are swept out of the per-world Skolem registries the
/// chase populates rather than dropped: each carries its firing rule, existential
/// ordinal, and frontier binding, so a downstream consumer can explain an invented
/// individual (and reconstruct its bounded proof height over the witness sub-forest)
/// entirely from the reasoning result, without re-running the chase.
pub(crate) fn augment_inferred_with_dl_certificates(
    inferred: &mut Vec<InferredAxiom>,
    edb: &RdfDataset,
) -> gmeow_errors::Result<(
    Vec<crate::reason::ChaseCertificate>,
    Vec<crate::physical::WitnessDerivation>,
)> {
    let restrictions = read_restrictions(edb);
    let (mut existential_rules, oversized_existential) =
        structured_existential_rules(&restrictions, edb);
    // An over-large `≥n` obligation was not lowered (its quadratic head would exhaust
    // memory to even build): record the honest INCOMPLETE withhold up front.
    if oversized_existential {
        record_chase_materialization_withhold(inferred);
    }
    // Merge authored general existential rules (arbitrary body/head) into the same
    // per-world map, so they are certified per-world and shipped alongside the
    // OWL-restriction certificates.
    for (world, rules) in authored_existential_rules(edb)? {
        existential_rules.entry(world).or_default().extend(rules);
    }
    let lists = read_lists(edb);

    let mut facts: BTreeSet<Fact> = raw_resource_facts(edb).into_iter().collect();
    let mut certificates = Vec::new();
    let mut witness_registries = BTreeMap::new();
    for ax in inferred.iter() {
        if let Some(fact) = fact_from_axiom(ax) {
            facts.insert(fact);
        }
    }

    // Structural schema consequences from finite DL constructs.
    for fact in facts.clone() {
        match fact.predicate.as_str() {
            OWL_COMPLEMENT_OF => {
                for (s, o) in [
                    (fact.subject.as_str(), fact.object.as_str()),
                    (fact.object.as_str(), fact.subject.as_str()),
                ] {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            s.to_owned(),
                            OWL_DISJOINT_WITH.to_owned(),
                            o.to_owned(),
                            fact.world.clone(),
                        ),
                        "dl:complement-disjoint",
                        vec![(
                            fact.subject.clone(),
                            OWL_COMPLEMENT_OF.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }
            }
            OWL_UNION_OF | OWL_DISJOINT_UNION_OF | OWL_ONE_OF => {
                let Some(members) = lists.get(&(fact.world.clone(), fact.object.clone())) else {
                    continue;
                };
                if fact.predicate == OWL_ONE_OF {
                    for member in members {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                member.clone(),
                                RDF_TYPE.to_owned(),
                                fact.subject.clone(),
                                fact.world.clone(),
                            ),
                            "dl:oneOf-member",
                            vec![(
                                fact.subject.clone(),
                                OWL_ONE_OF.to_owned(),
                                fact.object.clone(),
                            )],
                        );
                    }
                    // Nominal closure: `C ≡ oneOf(m1..mk)` means every instance of
                    // `C` is one of the members. So an individual typed `C` that is
                    // asserted **explicitly distinct** (`owl:differentFrom`) from
                    // *every* member can be no member at all and is forced into
                    // `owl:Nothing` — an inconsistency. This is the closure half of
                    // `oneOf` (the member→type direction above is the easy half); it
                    // terminates because the enumeration is finite and no witnesses
                    // are invented. (Gap G: the frozen external OWL 2 DL oracle gold
                    // caught this clash; native must too — native ⊇ oracle.)
                    //
                    // Distinctness here is the **explicit** stance only (asserted
                    // `owl:differentFrom`), NOT the UNA default the cardinality
                    // anti-merge uses: standard OWL does not assume unique names, so
                    // an instance of a `oneOf` class merely *named* differently from
                    // the members can still be `owl:sameAs` one of them and is
                    // consistent. Requiring explicit `differentFrom` keeps native
                    // faithful to the oracle (no false inconsistency on a
                    // non-UNA-consistent ontology) — soundness over an over-eager
                    // superset.
                    let one_of_class = fact.subject.clone();
                    let instances: Vec<String> = facts
                        .iter()
                        .filter(|f| {
                            f.predicate == RDF_TYPE
                                && f.object == one_of_class
                                && f.world == fact.world
                        })
                        .map(|f| f.subject.clone())
                        .collect();
                    for instance in instances {
                        // Skip the members themselves: a member IS in the class.
                        if members.iter().any(|m| m == &instance) {
                            continue;
                        }
                        let distinct_from_all = members.iter().all(|m| {
                            has_fact(&facts, &fact.world, &instance, OWL_DIFFERENT_FROM, m)
                                || has_fact(&facts, &fact.world, m, OWL_DIFFERENT_FROM, &instance)
                        });
                        if !distinct_from_all {
                            continue;
                        }
                        let mut premises: Vec<(String, String, String)> = vec![
                            (
                                one_of_class.clone(),
                                OWL_ONE_OF.to_owned(),
                                fact.object.clone(),
                            ),
                            (instance.clone(), RDF_TYPE.to_owned(), one_of_class.clone()),
                        ];
                        for m in members {
                            premises.push((
                                instance.clone(),
                                OWL_DIFFERENT_FROM.to_owned(),
                                m.clone(),
                            ));
                        }
                        premises.sort();
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                instance.clone(),
                                RDF_TYPE.to_owned(),
                                OWL_NOTHING.to_owned(),
                                fact.world.clone(),
                            ),
                            "dl:oneOf-closure-clash",
                            premises,
                        );
                    }
                    continue;
                }
                for member in members {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            member.clone(),
                            RDFS_SUBCLASSOF.to_owned(),
                            fact.subject.clone(),
                            fact.world.clone(),
                        ),
                        if fact.predicate == OWL_UNION_OF {
                            "dl:union-member"
                        } else {
                            "dl:disjointUnion-member"
                        },
                        vec![(
                            fact.subject.clone(),
                            fact.predicate.clone(),
                            fact.object.clone(),
                        )],
                    );
                }
                if fact.predicate == OWL_DISJOINT_UNION_OF {
                    for i in 0..members.len() {
                        for j in 0..members.len() {
                            if i == j {
                                continue;
                            }
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    members[i].clone(),
                                    OWL_DISJOINT_WITH.to_owned(),
                                    members[j].clone(),
                                    fact.world.clone(),
                                ),
                                "dl:disjointUnion-disjoint",
                                vec![(
                                    fact.subject.clone(),
                                    OWL_DISJOINT_UNION_OF.to_owned(),
                                    fact.object.clone(),
                                )],
                            );
                        }
                    }
                }
            }
            OWL_EQUIVALENT_PROPERTY => {
                // `p ≡ q` ⟺ mutual `subPropertyOf`, so the fixpoint's
                // subPropertyOf-propagation copies every `p` assertion onto `q`
                // (and vice versa). This is what lets `prp-pdw` fire on a
                // `p ≡ q ⊓ p disjointWith q` bundle: the copied assertions make
                // both properties carry the same `(x, y)` value.
                for (a, b) in [
                    (fact.subject.as_str(), fact.object.as_str()),
                    (fact.object.as_str(), fact.subject.as_str()),
                ] {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            a.to_owned(),
                            RDFS_SUBPROPERTYOF.to_owned(),
                            b.to_owned(),
                            fact.world.clone(),
                        ),
                        "dl:equivalentProperty-subproperty",
                        vec![(
                            fact.subject.clone(),
                            OWL_EQUIVALENT_PROPERTY.to_owned(),
                            fact.object.clone(),
                        )],
                    );
                }
            }
            _ => {}
        }
    }

    // ── owl:AllDisjointProperties / owl:AllDisjointClasses / owl:AllDifferent ──
    // Expand each list axiom to the pairwise binary form the local clash rules
    // read: `owl:members` on AllDisjointProperties → `owl:propertyDisjointWith`;
    // on AllDisjointClasses → `owl:disjointWith` (then `dl:individual-clash`
    // fires); `owl:members`/`owl:distinctMembers` on AllDifferent →
    // `owl:differentFrom` (then `eq-diff` fires against any `owl:sameAs`). Reuses
    // the shared RDF-list reader.
    for fact in facts.clone() {
        if fact.predicate != OWL_MEMBERS && fact.predicate != OWL_DISTINCT_MEMBERS {
            continue;
        }
        let Some(members) = lists.get(&(fact.world.clone(), fact.object.clone())) else {
            continue;
        };
        let node = fact.subject.as_str();
        let (relation, rule) = if fact.predicate == OWL_MEMBERS
            && has_fact(
                &facts,
                &fact.world,
                node,
                RDF_TYPE,
                OWL_ALL_DISJOINT_PROPERTIES,
            ) {
            (
                OWL_PROPERTY_DISJOINT_WITH,
                "dl:allDisjointProperties-pairwise",
            )
        } else if fact.predicate == OWL_MEMBERS
            && has_fact(
                &facts,
                &fact.world,
                node,
                RDF_TYPE,
                OWL_ALL_DISJOINT_CLASSES,
            )
        {
            (OWL_DISJOINT_WITH, "dl:allDisjointClasses-pairwise")
        } else if has_fact(&facts, &fact.world, node, RDF_TYPE, OWL_ALL_DIFFERENT) {
            // AllDifferent accepts either `owl:members` or `owl:distinctMembers`.
            (OWL_DIFFERENT_FROM, "dl:allDifferent-pairwise")
        } else {
            continue;
        };
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                add_inferred_fact(
                    inferred,
                    &mut facts,
                    Fact::new(
                        members[i].clone(),
                        relation.to_owned(),
                        members[j].clone(),
                        fact.world.clone(),
                    ),
                    rule,
                    vec![(node.to_owned(), fact.predicate.clone(), fact.object.clone())],
                );
            }
        }
    }

    loop {
        let before = facts.len();
        // perf: index rebuilt each fixpoint iter; incremental update tracked separately
        let index = build_index(&facts);
        let predicate_index = build_predicate_index(&facts);
        let mut subjects_by_world: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for fact in &facts {
            subjects_by_world
                .entry(fact.world.clone())
                .or_default()
                .insert(fact.subject.clone());
        }

        for (world, subjects) in &subjects_by_world {
            let world_facts: Vec<Fact> = facts
                .iter()
                .filter(|fact| fact.world == *world)
                .cloned()
                .collect();

            for fact in &world_facts {
                for super_property in
                    objects_for(&index, world, &fact.predicate, RDFS_SUBPROPERTYOF)
                {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            super_property.clone(),
                            fact.object.clone(),
                            world.clone(),
                        ),
                        "dl:subPropertyOf-propagation",
                        vec![
                            // schema axiom: predicate ⊑ super_property
                            (
                                fact.predicate.clone(),
                                RDFS_SUBPROPERTYOF.to_owned(),
                                super_property.clone(),
                            ),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }

                for domain in objects_for(&index, world, &fact.predicate, RDFS_DOMAIN) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            RDF_TYPE.to_owned(),
                            domain.clone(),
                            world.clone(),
                        ),
                        "dl:domain",
                        vec![
                            // schema axiom: predicate rdfs:domain domain
                            (
                                fact.predicate.clone(),
                                RDFS_DOMAIN.to_owned(),
                                domain.clone(),
                            ),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }

                for range in objects_for(&index, world, &fact.predicate, RDFS_RANGE) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            RDF_TYPE.to_owned(),
                            range.clone(),
                            world.clone(),
                        ),
                        "dl:range",
                        vec![
                            // schema axiom: predicate rdfs:range range
                            (fact.predicate.clone(), RDFS_RANGE.to_owned(), range.clone()),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }

                if has_fact(
                    &facts,
                    world,
                    &fact.predicate,
                    RDF_TYPE,
                    OWL_SYMMETRIC_PROPERTY,
                ) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            fact.predicate.clone(),
                            fact.subject.clone(),
                            world.clone(),
                        ),
                        "dl:symmetric-property",
                        vec![(
                            fact.predicate.clone(),
                            RDF_TYPE.to_owned(),
                            OWL_SYMMETRIC_PROPERTY.to_owned(),
                        )],
                    );
                }

                for inverse in objects_for(&index, world, &fact.predicate, OWL_INVERSE_OF) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.object.clone(),
                            inverse.clone(),
                            fact.subject.clone(),
                            world.clone(),
                        ),
                        "dl:inverseOf",
                        vec![
                            // schema axiom: predicate owl:inverseOf inverse
                            (
                                fact.predicate.clone(),
                                OWL_INVERSE_OF.to_owned(),
                                inverse.clone(),
                            ),
                            // source assertion: s predicate o
                            (
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                fact.object.clone(),
                            ),
                        ],
                    );
                }
            }

            for fact in &world_facts {
                if has_fact(
                    &facts,
                    world,
                    &fact.predicate,
                    RDF_TYPE,
                    OWL_TRANSITIVE_PROPERTY,
                ) {
                    for edge in edges_for(&predicate_index, world, &fact.predicate) {
                        if edge.0.as_str() != fact.object.as_str() {
                            continue;
                        }
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                fact.subject.clone(),
                                fact.predicate.clone(),
                                edge.1.clone(),
                                world.clone(),
                            ),
                            "dl:transitive-property",
                            vec![(
                                fact.predicate.clone(),
                                RDF_TYPE.to_owned(),
                                OWL_TRANSITIVE_PROPERTY.to_owned(),
                            )],
                        );
                    }
                }
            }

            for chain in edges_for(&predicate_index, world, OWL_PROPERTY_CHAIN_AXIOM) {
                let property = &chain.0;
                let list_root = &chain.1;
                let Some(members) = lists.get(&(world.clone(), list_root.clone())) else {
                    continue;
                };
                if members.len() != 2 {
                    continue;
                }
                let first = &members[0];
                let second = &members[1];
                for first_edge in edges_for(&predicate_index, world, first) {
                    let x = &first_edge.0;
                    let y = &first_edge.1;
                    for second_edge in edges_for(&predicate_index, world, second) {
                        if second_edge.0.as_str() != y.as_str() {
                            continue;
                        }
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                x.clone(),
                                property.clone(),
                                second_edge.1.clone(),
                                world.clone(),
                            ),
                            "dl:property-chain",
                            vec![(
                                property.clone(),
                                OWL_PROPERTY_CHAIN_AXIOM.to_owned(),
                                first.clone(),
                            )],
                        );
                    }
                }
            }

            for ((restriction_world, restriction_node), restriction) in &restrictions {
                if restriction_world != world {
                    continue;
                }
                let Some(property) = restriction.on_property.as_deref() else {
                    continue;
                };

                if let Some(class) = restriction.some_values_from.as_deref() {
                    for edge in edges_for(&predicate_index, world, property) {
                        let subject = &edge.0;
                        let object = &edge.1;
                        if has_fact(&facts, world, object, RDF_TYPE, class) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    restriction_node.clone(),
                                    world.clone(),
                                ),
                                "dl:someValuesFrom",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_SOME_VALUES_FROM.to_owned(),
                                    class.to_owned(),
                                )],
                            );
                        }
                    }
                }

                if let Some(class) = restriction.all_values_from.as_deref() {
                    for subject in subjects {
                        if !has_fact(&facts, world, subject, RDF_TYPE, restriction_node) {
                            continue;
                        }
                        for filler in objects_for(&index, world, subject, property) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    filler.clone(),
                                    RDF_TYPE.to_owned(),
                                    class.to_owned(),
                                    world.clone(),
                                ),
                                "dl:allValuesFrom",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_ALL_VALUES_FROM.to_owned(),
                                    class.to_owned(),
                                )],
                            );
                        }
                    }
                }

                if let Some(value) = restriction.has_value.as_deref() {
                    for subject in subjects {
                        if has_fact(&facts, world, subject, RDF_TYPE, restriction_node) {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    property.to_owned(),
                                    value.to_owned(),
                                    world.clone(),
                                ),
                                "dl:hasValue-assertion",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_HAS_VALUE.to_owned(),
                                    value.to_owned(),
                                )],
                            );
                        }
                    }
                    for edge in edges_for(&predicate_index, world, property) {
                        let subject = &edge.0;
                        let object = &edge.1;
                        if object == value {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    restriction_node.clone(),
                                    world.clone(),
                                ),
                                "dl:hasValue-recognition",
                                vec![(
                                    restriction_node.clone(),
                                    OWL_HAS_VALUE.to_owned(),
                                    value.to_owned(),
                                )],
                            );
                        }
                    }
                }
            }

            for fact in &world_facts {
                if fact.predicate != RDFS_SUBCLASSOF && fact.predicate != RDFS_SUBPROPERTYOF {
                    continue;
                }
                for transitive_target in objects_for(&index, world, &fact.object, &fact.predicate) {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            fact.subject.clone(),
                            fact.predicate.clone(),
                            transitive_target.clone(),
                            world.clone(),
                        ),
                        if fact.predicate == RDFS_SUBCLASSOF {
                            "dl:subClassOf-transitive"
                        } else {
                            "dl:subPropertyOf-transitive"
                        },
                        vec![],
                    );
                }
            }

            for subject in subjects {
                let types: Vec<&String> = objects_for(&index, world, subject, RDF_TYPE).collect();
                for &class in &types {
                    for superclass in objects_for(&index, world, class, RDFS_SUBCLASSOF) {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                subject.clone(),
                                RDF_TYPE.to_owned(),
                                superclass.clone(),
                                world.clone(),
                            ),
                            "dl:type-propagation",
                            vec![],
                        );
                    }
                }

                for (world_key, restriction_key) in restrictions.keys() {
                    if world_key != world {
                        continue;
                    }
                    if !types
                        .iter()
                        .any(|class| class.as_str() == restriction_key.as_str())
                    {
                        continue;
                    }
                    let restriction = &restrictions[&(world_key.clone(), restriction_key.clone())];
                    let Some(property) = restriction.on_property.as_deref() else {
                        continue;
                    };
                    let fillers: Vec<&String> =
                        objects_for(&index, world, subject, property).collect();

                    // ── max-cardinality / exact clash (soundness: no UNA) ────────
                    // A clash needs `> max` fillers that are PROVABLY distinct —
                    // explicit `owl:differentFrom` (no unique-name assumption:
                    // named fillers merely spelled differently may be `owl:sameAs`
                    // and are consistent). `max == 0` clashes on a single filler
                    // regardless. Chase witnesses invented for a co-present `≥n`
                    // obligation carry materialised pairwise `owl:differentFrom`
                    // (below), so a genuine `≥2 p ⊓ ≤1 p` still clashes.
                    for (max, on_class) in cardinality_maxima(restriction) {
                        let counted: Vec<&String> = match on_class {
                            Some(class) => fillers
                                .iter()
                                .copied()
                                .filter(|filler| {
                                    has_fact(&facts, world, filler.as_str(), RDF_TYPE, class)
                                })
                                .collect(),
                            None => fillers.clone(),
                        };
                        if counted.len() > max
                            && (max == 0 || pairwise_distinct(&facts, world, &counted))
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:max-cardinality-clash",
                                vec![],
                            );
                        }
                    }
                }

                for i in 0..types.len() {
                    for j in i..types.len() {
                        let c1 = types[i];
                        let c2 = types[j];
                        if has_fact(&facts, world, c1, OWL_DISJOINT_WITH, c2)
                            || has_fact(&facts, world, c2, OWL_DISJOINT_WITH, c1)
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    subject.clone(),
                                    RDF_TYPE.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:individual-clash",
                                vec![
                                    (subject.clone(), RDF_TYPE.to_owned(), c1.clone()),
                                    (subject.clone(), RDF_TYPE.to_owned(), c2.clone()),
                                    (c1.clone(), OWL_DISJOINT_WITH.to_owned(), c2.clone()),
                                ],
                            );
                        }
                    }
                }
            }

            let classes: BTreeSet<String> = facts
                .iter()
                .filter(|f| f.world == *world && f.predicate == RDFS_SUBCLASSOF)
                .map(|f| f.subject.clone())
                .collect();
            for class in &classes {
                let mut supers: BTreeSet<String> =
                    objects_for(&index, world, class, RDFS_SUBCLASSOF)
                        .cloned()
                        .collect();
                supers.insert(class.clone());
                let super_vec: Vec<&String> = supers.iter().collect();
                for i in 0..super_vec.len() {
                    for j in i..super_vec.len() {
                        let c1 = super_vec[i];
                        let c2 = super_vec[j];
                        if has_fact(&facts, world, c1, OWL_DISJOINT_WITH, c2)
                            || has_fact(&facts, world, c2, OWL_DISJOINT_WITH, c1)
                        {
                            add_inferred_fact(
                                inferred,
                                &mut facts,
                                Fact::new(
                                    class.clone(),
                                    RDFS_SUBCLASSOF.to_owned(),
                                    OWL_NOTHING.to_owned(),
                                    world.clone(),
                                ),
                                "dl:unsatisfiable-class",
                                vec![
                                    (class.clone(), RDFS_SUBCLASSOF.to_owned(), c1.clone()),
                                    (class.clone(), RDFS_SUBCLASSOF.to_owned(), c2.clone()),
                                    (c1.clone(), OWL_DISJOINT_WITH.to_owned(), c2.clone()),
                                ],
                            );
                        }
                    }
                }
            }

            for ((restriction_world, restriction_node), restriction) in &restrictions {
                if restriction_world != world {
                    continue;
                }
                if let Some(filler) = restriction.some_values_from.as_deref()
                    && has_fact(&facts, world, filler, RDFS_SUBCLASSOF, OWL_NOTHING)
                {
                    add_inferred_fact(
                        inferred,
                        &mut facts,
                        Fact::new(
                            restriction_node.clone(),
                            RDFS_SUBCLASSOF.to_owned(),
                            OWL_NOTHING.to_owned(),
                            world.clone(),
                        ),
                        "dl:someValuesFrom-unsat-filler",
                        vec![],
                    );
                }
                for (min, on_class) in cardinality_minima(restriction) {
                    if min == 0 {
                        continue;
                    }
                    if let Some(class) = on_class
                        && has_fact(&facts, world, class, RDFS_SUBCLASSOF, OWL_NOTHING)
                    {
                        add_inferred_fact(
                            inferred,
                            &mut facts,
                            Fact::new(
                                restriction_node.clone(),
                                RDFS_SUBCLASSOF.to_owned(),
                                OWL_NOTHING.to_owned(),
                                world.clone(),
                            ),
                            "dl:min-cardinality-unsat-filler",
                            vec![],
                        );
                    }
                }
            }
        }

        let (chase_certificates, chase_incomplete) = run_structured_existential_chase(
            inferred,
            &mut facts,
            &existential_rules,
            &mut witness_registries,
        )?;
        certificates.extend(chase_certificates);

        // Sound withhold under the resource bound: a single-invocation blow-up trips the
        // step backstop (`chase_incomplete`); a slow multi-round accumulation trips the
        // cumulative fact ceiling. Either way the fixpoint is not closed, so record the
        // INCOMPLETE marker and stop rather than growing the closure toward OOM.
        if chase_incomplete || facts.len() > MAX_CHASE_FACTS {
            record_chase_materialization_withhold(inferred);
            break;
        }

        if facts.len() == before {
            break;
        }
    }

    augment_with_extra_dl_clashes(
        inferred,
        &mut facts,
        &restrictions,
        edb,
        &mut certificates,
        &mut witness_registries,
    )?;

    // ── Fragment-certified refutation kernel (unified beyond-Horn decider) ──────
    // Decide the precisely-characterized COMPLETE fragment of the beyond-Horn
    // constructs the forward chase above withholds (datatype value-space,
    // counting, case-split/complement), and honestly withhold outside it. The
    // kernel is CALLED on every production closure — so its wiring is exercised,
    // not dark — but Task 2 registers no family sub-decider yet, so it returns
    // `OutOfFragment` and `materialize_refutation` adds nothing: a strict no-op on
    // real inputs (no current verdict changes; the drift-pinned withholds stay
    // `incomplete`). Tasks 3/4/5 register the per-family sub-deciders whose
    // `InFragment{Inconsistent}` clashes are materialized here as
    // `type(?i, owl:Nothing)` witnesses `verdict_from_inferred` reads off.
    materialize_refutation(&crate::reason::refute::refute(edb), inferred, &mut facts);

    certificates.sort_by(|left, right| {
        let left_finding = left.admission.to_finding();
        let right_finding = right.admission.to_finding();
        (&left.world, &left_finding.code, &left_finding.message).cmp(&(
            &right.world,
            &right_finding.code,
            &right_finding.message,
        ))
    });
    certificates
        .dedup_by(|left, right| left.world == right.world && left.admission == right.admission);

    // Sweep the decomposable derivation of every invented null out of the per-world
    // Skolem registries the chase populated (they would otherwise be dropped here).
    // The witness IRI is content-addressed, so the same recipe minted in two worlds
    // collapses to one derivation; sort+dedup by IRI keeps the set deterministic.
    let mut witness_derivations: Vec<crate::physical::WitnessDerivation> = witness_registries
        .values()
        .flat_map(|registry| {
            registry
                .witnesses()
                .filter_map(|iri| registry.explain(iri))
                .collect::<Vec<_>>()
        })
        .collect();
    witness_derivations.sort_by(|left, right| left.witness.cmp(&right.witness));
    witness_derivations.dedup_by(|left, right| left.witness == right.witness);

    Ok((certificates, witness_derivations))
}

/// The predicate-quantifying / literal-aware DL clashes the resource-only
/// [`Fact`] closure and the fixed ternary rule text cannot express.
///
/// Direct, sound consistency contradictions are layered here, each of which
/// asserts `type(x, owl:Nothing)` (the inconsistency witness the verdict reads
/// off). All run after the main closure so they observe every propagated
/// `rdf:type` / assertion / `owl:differentFrom` fact. Blocks 1–5 are the original
/// Thing/bottom/NPA/functional/key clashes; blocks 6–9 add the Wave A families —
/// asymmetric-property cycles (`prp-asyp`), irreflexive self-loops (`prp-irp`),
/// property-disjointness value collisions (`prp-pdw`, literal-aware), and the
/// `owl:sameAs` ⊓ `owl:differentFrom` contradiction (`eq-diff`, over the sameAs
/// closure). The `owl:AllDisjoint*`/`owl:AllDifferent`/`owl:equivalentProperty`
/// list/equivalence expansions that feed blocks 6–9 run in the structural
/// pre-phase of [`augment_inferred_with_dl`]:
///
/// 1. **`owl:Thing` forced empty** — `owl:Thing ⊑ owl:Nothing` or
///    `owl:Thing ≡ owl:Nothing`. The extension of `owl:Thing` is never empty, so
///    forcing it into `owl:Nothing` is inconsistent. A scoped Skolem witness
///    (`owl:Thing`'s mandatory inhabitant) is forced into `owl:Nothing`.
/// 2. **Empty bottom property has a value** — any individual typed a restriction
///    `∃p.X` (or `≥1 p`) where `p` is `owl:bottomObjectProperty` /
///    `owl:bottomDataProperty`. The bottom property's extension is empty, so an
///    obligation to have a value on it is unsatisfiable.
/// 3. **Negative property assertion contradicted** —
///    `NegativePropertyAssertion(source, p, target)` co-present with the positive
///    `p(source, target)` (object via `owl:targetIndividual`, data via
///    `owl:targetValue`). A direct contradiction.
/// 4. **Functional property with two distinct values** — `p` a
///    `owl:FunctionalProperty` with `p(x, v1)` and `p(x, v2)` for provably
///    distinct `v1`, `v2` (distinct literals, or distinct individuals under the
///    identity stance). The two values are forced equal yet provably distinct.
/// 5. **Key axiom collision** — `owl:hasKey(C, [k1..kn])` with two instances of
///    `C` agreeing on every key value yet asserted `owl:differentFrom`. The key
///    forces them equal; the explicit distinctness clashes.
fn augment_with_extra_dl_clashes(
    inferred: &mut Vec<InferredAxiom>,
    facts: &mut BTreeSet<Fact>,
    restrictions: &BTreeMap<(String, String), Restriction>,
    edb: &RdfDataset,
    certificates: &mut Vec<crate::reason::ChaseCertificate>,
    witness_registries: &mut BTreeMap<String, crate::physical::SkolemRegistry>,
) -> gmeow_errors::Result<()> {
    // ── 1. owl:Thing forced into owl:Nothing ──────────────────────────────────
    // owl:Thing ⊑ owl:Nothing (asserted or via equivalentClass) makes the always-
    // populated top class empty — inconsistent. We materialise the mandatory
    // owl:Thing inhabitant as a scoped witness and force it into owl:Nothing.
    let thing_empty_worlds: BTreeSet<String> = facts
        .iter()
        .filter(|f| {
            f.subject == OWL_THING
                && f.object == OWL_NOTHING
                && (f.predicate == RDFS_SUBCLASSOF || f.predicate == OWL_EQUIVALENT_CLASS)
        })
        .map(|f| f.world.clone())
        .collect();
    let mut thing_rules = BTreeMap::new();
    for world in thing_empty_worlds {
        use crate::rule_ir::{EvalAtom, EvalTerm};
        let rules = [RDFS_SUBCLASSOF, OWL_EQUIVALENT_CLASS]
            .into_iter()
            .map(|predicate| crate::physical::ExistentialRule {
                rule_iri: format!(
                    "https://blackcatinformatics.ca/gmeow/logic/rule/dl-thing-nonempty/{}",
                    crate::provenance::sha1_hex(&format!("{world}\u{1f}{predicate}"))
                ),
                body: vec![EvalAtom::positive(
                    EvalTerm::named(OWL_THING),
                    predicate,
                    EvalTerm::named(OWL_NOTHING),
                )],
                head: vec![EvalAtom::positive(
                    EvalTerm::var("?domainWitness"),
                    RDF_TYPE,
                    EvalTerm::named(OWL_NOTHING),
                )],
                distinct: Vec::new(),
                witness_frontier: Some(Vec::new()),
                witness_policy: crate::physical::WitnessPolicy::DlAncestorBlocking,
            })
            .collect();
        thing_rules.insert(world, rules);
    }
    let (thing_certificates, thing_incomplete) =
        run_structured_existential_chase(inferred, facts, &thing_rules, witness_registries)?;
    certificates.extend(thing_certificates);
    if thing_incomplete {
        record_chase_materialization_withhold(inferred);
    }

    // ── 2. Empty bottom property forced to have a value ───────────────────────
    // An individual typed a restriction on owl:bottomObjectProperty /
    // owl:bottomDataProperty that obligates ≥1 value (someValuesFrom, or a
    // cardinality / qualified minimum ≥ 1) cannot be satisfied: the bottom
    // property's extension is empty.
    for ((restriction_world, restriction_node), restriction) in restrictions {
        let Some(property) = restriction.on_property.as_deref() else {
            continue;
        };
        if property != OWL_BOTTOM_OBJECT_PROPERTY && property != OWL_BOTTOM_DATA_PROPERTY {
            continue;
        }
        let obligates_value = restriction.some_values_from.is_some()
            || restriction.has_value.is_some()
            || existential_obligations(restriction)
                .iter()
                .any(|(n, _)| *n >= 1);
        if !obligates_value {
            continue;
        }
        let instances: Vec<String> = facts
            .iter()
            .filter(|f| {
                f.world == *restriction_world
                    && f.predicate == RDF_TYPE
                    && f.object == *restriction_node
            })
            .map(|f| f.subject.clone())
            .collect();
        for instance in instances {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    instance,
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    restriction_world.clone(),
                ),
                "dl:bottom-property-clash",
                vec![(
                    restriction_node.clone(),
                    OWL_ON_PROPERTY.to_owned(),
                    property.to_owned(),
                )],
            );
        }
    }

    // ── 3. Negative property assertion contradicted by its positive ───────────
    // Reify owl:NegativePropertyAssertion(source, p, target) from its RDF shape
    // and clash it against the literal-aware positive p(source, target).
    let value_index = build_value_index(edb);
    for npa in read_negative_property_assertions(edb) {
        let positive = value_index
            .get(&(npa.world.clone(), npa.source.clone(), npa.property.clone()))
            .map(|s| s.contains_key(&npa.target))
            .unwrap_or(false);
        if positive {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    npa.source.clone(),
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    npa.world.clone(),
                ),
                "dl:negative-property-assertion-clash",
                vec![(
                    npa.source.clone(),
                    npa.property.clone(),
                    OWL_NEGATIVE_PROPERTY_ASSERTION.to_owned(),
                )],
            );
        }
    }

    // ── 4. Functional property with two provably-distinct values ──────────────
    // For every functional property p and subject x, if x has two values on p that
    // are provably distinct (distinct literals, or distinct named individuals under
    // the identity stance), x is forced into owl:Nothing. Functionality is read from
    // BOTH carriers (the `owl:FunctionalProperty` marker AND the canonical
    // `logic:PropertyCharacteristicAssertion` record), keyed per-world with the
    // provenance premise that declares it — see `functional_property_sources`.
    let functional_sources = functional_property_sources(facts);
    for ((world, property), premises) in &functional_sources {
        // Group this property's (subject -> distinct value-keys) in this world.
        let mut by_subject: BTreeMap<String, BTreeMap<String, RdfTerm>> = BTreeMap::new();
        for (key, terms) in &value_index {
            let (w, subject, pred) = key;
            if w != world || pred != property {
                continue;
            }
            by_subject
                .entry(subject.clone())
                .or_default()
                .extend(terms.iter().map(|(k, t)| (k.clone(), t.clone())));
        }
        for (subject, values) in by_subject {
            if values.len() < 2 {
                continue;
            }
            if functional_values_clash(facts, world, &values) {
                add_inferred_fact(
                    inferred,
                    facts,
                    Fact::new(
                        subject,
                        RDF_TYPE.to_owned(),
                        OWL_NOTHING.to_owned(),
                        world.clone(),
                    ),
                    "dl:functional-property-clash",
                    premises.clone(),
                );
            }
        }
    }

    // ── 5. Key-axiom collision ────────────────────────────────────────────────
    // owl:hasKey(C, [k1..kn]); two instances of C agreeing on every key value yet
    // explicitly owl:differentFrom are forced into owl:Nothing.
    let lists = read_lists(edb);
    for (world, key_class, key_props) in read_key_axioms(edb, &lists) {
        // Members of C in this world. owl:Thing has every individual as a member,
        // so for a key on owl:Thing every individual that bears the key props
        // counts (the trivial-inconsistency case from the OWL primer).
        let instances: Vec<String> = collect_key_subjects(facts, &value_index, &world, &key_class);
        for i in 0..instances.len() {
            for j in (i + 1)..instances.len() {
                let a = &instances[i];
                let b = &instances[j];
                // Explicit distinctness is required: standard OWL does not assume
                // unique names, so two key-agreeing instances merely named
                // differently are owl:sameAs and consistent. Only an explicit
                // owl:differentFrom makes the key collision a contradiction.
                if !has_fact(facts, &world, a, OWL_DIFFERENT_FROM, b)
                    && !has_fact(facts, &world, b, OWL_DIFFERENT_FROM, a)
                {
                    continue;
                }
                if key_values_agree(&value_index, &world, a, b, &key_props) {
                    let premises = vec![
                        (key_class.clone(), OWL_HAS_KEY.to_owned(), key_class.clone()),
                        (a.clone(), OWL_DIFFERENT_FROM.to_owned(), b.clone()),
                    ];
                    add_inferred_fact(
                        inferred,
                        facts,
                        Fact::new(
                            a.clone(),
                            RDF_TYPE.to_owned(),
                            OWL_NOTHING.to_owned(),
                            world.clone(),
                        ),
                        "dl:has-key-clash",
                        premises,
                    );
                }
            }
        }
    }

    // ── 6. Asymmetric-property cycle (prp-asyp) ───────────────────────────────
    // `AsymmetricProperty(p) ∧ p(x, y) ∧ p(y, x) ⇒ Nothing(x)`. Runs over the
    // closed `facts` set, so the SymmetricProperty-derived reverse edge (the
    // symmetric+asymmetric `-term` shape) is already present.
    let asymmetric_props: BTreeSet<(String, String)> = facts
        .iter()
        .filter(|f| f.predicate == RDF_TYPE && f.object == OWL_ASYMMETRIC_PROPERTY)
        .map(|f| (f.world.clone(), f.subject.clone()))
        .collect();
    for (world, property) in &asymmetric_props {
        let edges: Vec<(String, String)> = facts
            .iter()
            .filter(|f| f.world == *world && f.predicate == *property)
            .map(|f| (f.subject.clone(), f.object.clone()))
            .collect();
        for (x, y) in &edges {
            if x == y {
                // An asymmetric property is also irreflexive: p(x, x) is itself a
                // clash (handled below by the shared self-loop check too).
                continue;
            }
            if has_fact(facts, world, y, property, x) {
                add_inferred_fact(
                    inferred,
                    facts,
                    Fact::new(
                        x.clone(),
                        RDF_TYPE.to_owned(),
                        OWL_NOTHING.to_owned(),
                        world.clone(),
                    ),
                    "dl:asymmetric-property-clash",
                    vec![(
                        property.clone(),
                        RDF_TYPE.to_owned(),
                        OWL_ASYMMETRIC_PROPERTY.to_owned(),
                    )],
                );
            }
        }
    }

    // ── 7. Irreflexive-property self-loop (prp-irp) ───────────────────────────
    // `IrreflexiveProperty(p) ∧ p(x, x) ⇒ Nothing(x)`. An `AsymmetricProperty`
    // self-loop is likewise irreflexive, so both markers are honoured here.
    let irreflexive_props: BTreeSet<(String, String)> = facts
        .iter()
        .filter(|f| {
            f.predicate == RDF_TYPE
                && (f.object == OWL_IRREFLEXIVE_PROPERTY || f.object == OWL_ASYMMETRIC_PROPERTY)
        })
        .map(|f| (f.world.clone(), f.subject.clone()))
        .collect();
    for (world, property) in &irreflexive_props {
        let self_loops: Vec<String> = facts
            .iter()
            .filter(|f| f.world == *world && f.predicate == *property && f.subject == f.object)
            .map(|f| f.subject.clone())
            .collect();
        for x in self_loops {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    x,
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    world.clone(),
                ),
                "dl:irreflexive-property-clash",
                vec![(
                    property.clone(),
                    RDF_TYPE.to_owned(),
                    OWL_IRREFLEXIVE_PROPERTY.to_owned(),
                )],
            );
        }
    }

    // ── 8. Disjoint-property value collision (prp-pdw) ────────────────────────
    // `propertyDisjointWith(p, q) ∧ p(x, v) ∧ q(x, v) ⇒ Nothing(x)`, literal-aware
    // (data-property disjointness compares literal VALUES). The per-property value
    // map merges the closed resource facts (so equivalentProperty-propagated
    // assertions count) with the EDB literal index. A self-disjoint `p` (`p ⊥ p`)
    // clashes on any single asserted value.
    let disjoint_props: Vec<(String, String, String)> = facts
        .iter()
        .filter(|f| f.predicate == OWL_PROPERTY_DISJOINT_WITH)
        .map(|f| (f.world.clone(), f.subject.clone(), f.object.clone()))
        .collect();
    if !disjoint_props.is_empty() {
        let prop_values = build_property_value_map(facts, &value_index);
        for (world, p, q) in &disjoint_props {
            let empty: HashMap<String, BTreeSet<String>> = HashMap::new();
            let p_map = prop_values
                .get(&(world.clone(), p.clone()))
                .unwrap_or(&empty);
            let q_map = prop_values
                .get(&(world.clone(), q.clone()))
                .unwrap_or(&empty);
            for (subject, p_vals) in p_map {
                let Some(q_vals) = q_map.get(subject) else {
                    continue;
                };
                if p_vals.iter().any(|v| q_vals.contains(v)) {
                    add_inferred_fact(
                        inferred,
                        facts,
                        Fact::new(
                            subject.clone(),
                            RDF_TYPE.to_owned(),
                            OWL_NOTHING.to_owned(),
                            world.clone(),
                        ),
                        "dl:property-disjoint-clash",
                        vec![(p.clone(), OWL_PROPERTY_DISJOINT_WITH.to_owned(), q.clone())],
                    );
                }
            }
        }
    }

    // ── 9. sameAs / differentFrom contradiction (eq-diff) ─────────────────────
    // `differentFrom(a, b) ∧ a ≈ b ⇒ Nothing(a)`, where `≈` is the reflexive/
    // symmetric/transitive `owl:sameAs` closure. A reflexive `differentFrom(a, a)`
    // is the degenerate case (everything is `sameAs` itself).
    let different_from: Vec<(String, String, String)> = facts
        .iter()
        .filter(|f| f.predicate == OWL_DIFFERENT_FROM)
        .map(|f| (f.world.clone(), f.subject.clone(), f.object.clone()))
        .collect();
    // No `owl:differentFrom` ⇒ no `eq-diff` clash possible; skip the sameAs
    // closure entirely (the common case on every consistency run).
    let same_as = if different_from.is_empty() {
        SameAsClosure::empty()
    } else {
        SameAsClosure::build(facts)
    };
    for (world, a, b) in &different_from {
        if a == b || same_as.same(world, a, b) {
            add_inferred_fact(
                inferred,
                facts,
                Fact::new(
                    a.clone(),
                    RDF_TYPE.to_owned(),
                    OWL_NOTHING.to_owned(),
                    world.clone(),
                ),
                "dl:same-different-clash",
                vec![(a.clone(), OWL_DIFFERENT_FROM.to_owned(), b.clone())],
            );
        }
    }
    Ok(())
}

/// A per-`(world, property)` map `subject → {value_key}` merging the closed
/// resource [`Fact`] set (so `equivalentProperty`-propagated and other derived
/// assertions count) with the literal-aware EDB [`build_value_index`]. Resource
/// objects use the `R\u{1f}<iri>` value key so they never collide with a literal.
#[allow(clippy::type_complexity)]
fn build_property_value_map(
    facts: &BTreeSet<Fact>,
    value_index: &HashMap<(String, String, String), BTreeMap<String, RdfTerm>>,
) -> HashMap<(String, String), HashMap<String, BTreeSet<String>>> {
    let mut map: HashMap<(String, String), HashMap<String, BTreeSet<String>>> = HashMap::new();
    for fact in facts {
        map.entry((fact.world.clone(), fact.predicate.clone()))
            .or_default()
            .entry(fact.subject.clone())
            .or_default()
            .insert(format!("R\u{1f}{}", fact.object));
    }
    for ((world, subject, predicate), terms) in value_index {
        let entry = map
            .entry((world.clone(), predicate.clone()))
            .or_default()
            .entry(subject.clone())
            .or_default();
        for key in terms.keys() {
            entry.insert(key.clone());
        }
    }
    map
}

/// The reflexive/symmetric/transitive `owl:sameAs` closure, per world, as a
/// union-find over the individuals that appear in a `sameAs` fact.
struct SameAsClosure {
    /// `(world, individual) → representative individual`.
    parent: HashMap<(String, String), String>,
}

impl SameAsClosure {
    fn empty() -> Self {
        SameAsClosure {
            parent: HashMap::new(),
        }
    }

    fn build(facts: &BTreeSet<Fact>) -> Self {
        let mut c = SameAsClosure {
            parent: HashMap::new(),
        };
        for fact in facts {
            if fact.predicate != OWL_SAME_AS {
                continue;
            }
            c.union(&fact.world, &fact.subject, &fact.object);
        }
        c
    }

    fn find(&mut self, world: &str, x: &str) -> String {
        let key = (world.to_owned(), x.to_owned());
        match self.parent.get(&key).cloned() {
            None => {
                self.parent.insert(key, x.to_owned());
                x.to_owned()
            }
            Some(p) if p == x => x.to_owned(),
            Some(p) => {
                let root = self.find(world, &p);
                self.parent
                    .insert((world.to_owned(), x.to_owned()), root.clone());
                root
            }
        }
    }

    fn union(&mut self, world: &str, a: &str, b: &str) {
        let ra = self.find(world, a);
        let rb = self.find(world, b);
        if ra != rb {
            self.parent.insert((world.to_owned(), ra), rb);
        }
    }

    /// True iff `a` and `b` are in the same `owl:sameAs` class (reflexive:
    /// `a ≈ a` always holds).
    fn same(&self, world: &str, a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        let ra = self.find_readonly(world, a);
        let rb = self.find_readonly(world, b);
        ra == rb
    }

    /// Non-mutating root lookup (the closure is fully built before any query).
    fn find_readonly(&self, world: &str, x: &str) -> String {
        let mut cur = x.to_owned();
        loop {
            match self.parent.get(&(world.to_owned(), cur.clone())) {
                Some(p) if *p != cur => cur = p.clone(),
                _ => return cur,
            }
        }
    }
}

/// A reified `owl:NegativePropertyAssertion`.
struct NegativeAssertion {
    world: String,
    source: String,
    property: String,
    /// The target's value key ([`term_value_key`]), object or literal.
    target: String,
}

/// Reify every `owl:NegativePropertyAssertion` from its RDF shape
/// (`owl:sourceIndividual` / `owl:assertionProperty` /
/// `owl:targetIndividual` | `owl:targetValue`). Incomplete assertions (missing a
/// component) are skipped — a malformed NPA asserts nothing.
fn read_negative_property_assertions(edb: &RdfDataset) -> Vec<NegativeAssertion> {
    // (world, npa_node) → partial components.
    let mut source: HashMap<(String, String), String> = HashMap::new();
    let mut property: HashMap<(String, String), String> = HashMap::new();
    let mut target: HashMap<(String, String), String> = HashMap::new();
    let mut is_npa: HashSet<(String, String)> = HashSet::new();

    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let Some(subject) = term_resource_key(&subject) else {
            continue;
        };
        let world = graph_world_key(&graph_name);
        let key = (world, subject);
        match predicate.as_str() {
            RDF_TYPE => {
                if matches!(&object, RdfTerm::Iri(o) if o == OWL_NEGATIVE_PROPERTY_ASSERTION) {
                    is_npa.insert(key);
                }
            }
            OWL_SOURCE_INDIVIDUAL => {
                if let Some(v) = term_resource_key(&object) {
                    source.insert(key, v);
                }
            }
            OWL_ASSERTION_PROPERTY => {
                if let Some(v) = term_resource_key(&object) {
                    property.insert(key, v);
                }
            }
            OWL_TARGET_INDIVIDUAL | OWL_TARGET_VALUE => {
                target.insert(key, term_value_key(&object));
            }
            _ => {}
        }
    }

    // A node is a negative property assertion when it EITHER carries the explicit
    // `rdf:type owl:NegativePropertyAssertion` OR structurally bears all three
    // defining properties (`owl:sourceIndividual` / `owl:assertionProperty` /
    // `owl:targetIndividual|targetValue`). The three properties are NPA-specific
    // vocabulary, so their co-occurrence IS the NPA shape — the `-fw` OWL 2 cases
    // omit the type triple and rely on this structural recognition.
    let mut candidates: BTreeSet<(String, String)> = is_npa.into_iter().collect();
    for key in source.keys() {
        if property.contains_key(key) && target.contains_key(key) {
            candidates.insert(key.clone());
        }
    }
    let mut out = Vec::new();
    for key in candidates {
        let (Some(s), Some(p), Some(t)) = (source.get(&key), property.get(&key), target.get(&key))
        else {
            continue;
        };
        out.push(NegativeAssertion {
            world: key.0.clone(),
            source: s.clone(),
            property: p.clone(),
            target: t.clone(),
        });
    }
    out
}

/// A literal-aware value index `(world, subject, predicate) → {value_key → term}`.
/// Unlike the resource-only [`Fact`] index, this retains literal objects so the
/// negative-assertion, functional-property, and key checks can compare data
/// values. The value key is [`term_value_key`].
#[allow(clippy::type_complexity)]
fn build_value_index(
    edb: &RdfDataset,
) -> HashMap<(String, String, String), BTreeMap<String, RdfTerm>> {
    let mut index: HashMap<(String, String, String), BTreeMap<String, RdfTerm>> = HashMap::new();
    for RdfQuad {
        subject,
        predicate,
        object,
        graph_name,
        ..
    } in edb.owned_quads()
    {
        let Some(subject) = term_resource_key(&subject) else {
            continue;
        };
        let world = graph_world_key(&graph_name);
        index
            .entry((world, subject, predicate))
            .or_default()
            .insert(term_value_key(&object), object);
    }
    index
}

/// True iff the functional-property value set contains two values that are
/// **provably** distinct: two literals with different value keys (excluding the
/// `rdf:XMLLiteral` shape, whose equality is XML canonicalization the chase does
/// not perform — withheld via coverage instead of guessed), or two named
/// resources related by an explicit `owl:differentFrom`. Named resources spelled
/// differently are NOT assumed distinct (no unique-name assumption), so a
/// functional property with two merely-named fillers is consistent.
/// Every property that is functional in a world, keyed `(world, property)`, mapped to the
/// provenance premise triples that declare it so — the UNION of both carriers:
///
/// * the deprecated `owl:FunctionalProperty` type marker (`?P rdf:type owl:FunctionalProperty`),
///   still authored on raw external/conformance OWL inputs and re-emitted by the OWL grounding
///   VIEW; and
/// * the canonical central `logic:PropertyCharacteristicAssertion` record
///   (`?rec logic:characterizes ?P`, `?rec logic:characteristicSort logic:functionalProperty`),
///   the greenfield source that survives removal of the `owl:FunctionalProperty` slice
///   declarations from the object-level reasoning EDB.
///
/// Unioning the two carriers here mirrors the foundation `collect_characteristics` pass, so
/// functional-cardinality enforcement does not regress after the source removal. Both carriers
/// are joined per-world (a marker/record and the clashing values must share a world, exactly as
/// the OWL-only reader required). When BOTH declare a property functional in a world the OWL
/// marker premise is preferred, so provenance is byte-identical to the pre-migration derivation.
///
/// `(world, property)` → the provenance premise triples that declare it functional.
type FunctionalSources = BTreeMap<(String, String), Vec<(String, String, String)>>;

fn functional_property_sources(facts: &BTreeSet<Fact>) -> FunctionalSources {
    let mut out: FunctionalSources = BTreeMap::new();

    // Carrier records: join `characterizes` + `characteristicSort=logic:functionalProperty` on
    // the record IRI within one world. Recorded first so a later OWL marker overwrites the
    // premise (marker preferred, keeping legacy provenance stable).
    let mut rec_prop: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut functional_recs: BTreeSet<(String, String)> = BTreeSet::new();
    for f in facts {
        if f.predicate == LOGIC_CHARACTERIZES {
            rec_prop.insert((f.world.clone(), f.subject.clone()), f.object.clone());
        } else if f.predicate == LOGIC_CHARACTERISTIC_SORT && f.object == LOGIC_FUNCTIONAL_PROPERTY
        {
            functional_recs.insert((f.world.clone(), f.subject.clone()));
        }
    }
    for (world, rec) in &functional_recs {
        let Some(property) = rec_prop.get(&(world.clone(), rec.clone())) else {
            continue;
        };
        out.entry((world.clone(), property.clone()))
            .or_insert_with(|| {
                vec![
                    (
                        rec.clone(),
                        LOGIC_CHARACTERIZES.to_owned(),
                        property.clone(),
                    ),
                    (
                        rec.clone(),
                        LOGIC_CHARACTERISTIC_SORT.to_owned(),
                        LOGIC_FUNCTIONAL_PROPERTY.to_owned(),
                    ),
                ]
            });
    }

    // OWL type markers (preferred provenance): overwrite any carrier premise for the same key.
    for f in facts {
        if f.predicate == RDF_TYPE && f.object == OWL_FUNCTIONAL_PROPERTY {
            out.insert(
                (f.world.clone(), f.subject.clone()),
                vec![(
                    f.subject.clone(),
                    RDF_TYPE.to_owned(),
                    OWL_FUNCTIONAL_PROPERTY.to_owned(),
                )],
            );
        }
    }

    out
}

fn functional_values_clash(
    facts: &BTreeSet<Fact>,
    world: &str,
    values: &BTreeMap<String, RdfTerm>,
) -> bool {
    let is_xml_literal = |t: &RdfTerm| matches!(t, RdfTerm::Literal(l) if l.datatype.as_deref() == Some(RDF_XML_LITERAL));
    let entries: Vec<&RdfTerm> = values.values().collect();
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (a, b) = (entries[i], entries[j]);
            // XMLLiteral value equality needs XML-C14N the chase does not do; two
            // lexically-different XMLLiterals may canonicalize equal. Never clash
            // on such a pair — the `functionalProperty` coverage is withheld for
            // this shape so the case surfaces as an honest gap, not a wrong answer.
            if is_xml_literal(a) || is_xml_literal(b) {
                continue;
            }
            match (a, b) {
                // Two literals (or a literal and a resource) with different value
                // keys denote genuinely distinct OWL values.
                (RdfTerm::Literal(_), _) | (_, RdfTerm::Literal(_)) => return true,
                _ => {
                    let (Some(ak), Some(bk)) = (term_resource_key(a), term_resource_key(b)) else {
                        continue;
                    };
                    if distinct_individuals(facts, world, &ak, &bk) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Reify every key axiom into `(world, class, key_props)` — the UNION of both carriers:
///
/// * the deprecated `owl:hasKey(C, list)` axiom, resolving the key-property RDF list (a
///   dangling/empty list yields no axiom); still authored on raw external/conformance OWL
///   inputs and re-projected onto the OWL grounding view; and
/// * the canonical central `logic:KeyAssertion` record (`?rec logic:keyClass ?C`,
///   `?rec logic:keyProperty ?P`), the greenfield source that survives removal of the
///   `owl:hasKey` slice declaration from the object-level reasoning EDB.
///
/// Unioning the two here mirrors [`functional_property_sources`] for the functional
/// characteristic, so key-agreement enforcement does not regress after the source migration.
fn read_key_axioms(
    edb: &RdfDataset,
    lists: &HashMap<(String, String), Vec<String>>,
) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != OWL_HAS_KEY {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            continue;
        };
        let Some(members) = lists.get(&(world.clone(), root)) else {
            continue;
        };
        if members.is_empty() {
            continue;
        }
        out.push((world, subject, members.clone()));
    }
    for ((world, _rec), resolved) in key_assertion_records(edb) {
        if let Some((class, key_props)) = resolved {
            out.push((world, class, key_props));
        }
    }
    out
}

/// Every `logic:KeyAssertion` record keyed `(world, record)` → the resolved `(class, key_props)`
/// when well-formed, or `None` when the record is present but malformed.
type KeyAssertionRecords = BTreeMap<(String, String), Option<(String, Vec<String>)>>;

/// Every `logic:KeyAssertion` record in `edb`, keyed `(world, record)`, mapped to the resolved
/// `Some((class, key_props))` when well-formed — a `logic:keyClass` naming the identified class
/// and at least one `logic:keyProperty` — or `None` when a record is present but malformed
/// (no class, or no key property). Recording malformed records as `None` (rather than dropping
/// them) lets the coverage guard withhold `hasKey` honestly instead of silently deciding it.
fn key_assertion_records(edb: &RdfDataset) -> KeyAssertionRecords {
    let mut records: BTreeSet<(String, String)> = BTreeSet::new();
    let mut class_of: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut props_of: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        let key = (world, subject);
        if predicate == RDF_TYPE
            && term_resource_key(&object).as_deref() == Some(LOGIC_KEY_ASSERTION)
        {
            records.insert(key);
        } else if predicate == LOGIC_KEY_CLASS
            && let Some(class) = term_resource_key(&object)
        {
            class_of.insert(key, class);
        } else if predicate == LOGIC_KEY_PROPERTY
            && let Some(prop) = term_resource_key(&object)
        {
            props_of.entry(key).or_default().push(prop);
        }
    }
    let mut out: KeyAssertionRecords = BTreeMap::new();
    for rec in records {
        let resolved = match (class_of.get(&rec), props_of.get(&rec)) {
            (Some(class), Some(props)) if !props.is_empty() => Some((class.clone(), props.clone())),
            _ => None,
        };
        out.insert(rec, resolved);
    }
    out
}

/// True iff at least one key axiom is present — an `owl:hasKey` list OR a `logic:KeyAssertion`
/// carrier record — and every one resolves (a non-empty `owl:hasKey` list; a `logic:KeyAssertion`
/// naming a class and ≥1 key property). Unioning both carriers mirrors
/// [`functional_property_sources`], so `hasKey` stays decided over the object-level reasoning EDB
/// after the `owl:hasKey` slice declaration is migrated to the carrier. A present-but-malformed
/// axiom of either carrier withholds the family (returns `false`), exactly as
/// [`all_list_instances_resolve`] does for the `owl:hasKey` list alone.
fn key_axioms_all_resolve(
    edb: &RdfDataset,
    lists: &HashMap<(String, String), Vec<String>>,
) -> bool {
    let mut saw_instance = false;
    for (_subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != OWL_HAS_KEY {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            return false;
        };
        saw_instance = true;
        match lists.get(&(world, root)) {
            Some(members) if !members.is_empty() => {}
            _ => return false,
        }
    }
    for (_rec, resolved) in key_assertion_records(edb) {
        saw_instance = true;
        if resolved.is_none() {
            return false;
        }
    }
    saw_instance
}

/// Collect the candidate key subjects for a key on `class` in `world`: the
/// individuals asserted `rdf:type class`. `owl:Thing` keys every individual, so
/// for a key on `owl:Thing` we take every subject that appears in the value index
/// (every resource that bears at least one property) — the trivial-inconsistency
/// case from the OWL specification.
fn collect_key_subjects(
    facts: &BTreeSet<Fact>,
    value_index: &HashMap<(String, String, String), BTreeMap<String, RdfTerm>>,
    world: &str,
    class: &str,
) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    if class == OWL_THING {
        for (w, subject, _pred) in value_index.keys() {
            if w == world {
                set.insert(subject.clone());
            }
        }
    } else {
        for fact in facts {
            if fact.world == *world && fact.predicate == RDF_TYPE && fact.object == *class {
                set.insert(fact.subject.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// True iff `a` and `b` agree on every key property: for each key property each
/// must bear at least one value and share at least one common value. (OWL key
/// semantics: two instances with the same value for every key property are the
/// same individual; agreement requires a shared value per key property.)
fn key_values_agree(
    value_index: &HashMap<(String, String, String), BTreeMap<String, RdfTerm>>,
    world: &str,
    a: &str,
    b: &str,
    key_props: &[String],
) -> bool {
    for prop in key_props {
        let av = value_index.get(&(world.to_owned(), a.to_owned(), prop.clone()));
        let bv = value_index.get(&(world.to_owned(), b.to_owned(), prop.clone()));
        let (Some(av), Some(bv)) = (av, bv) else {
            return false;
        };
        if av.keys().all(|k| !bv.contains_key(k)) {
            return false;
        }
    }
    true
}

/// Decide native DL consistency / unsatisfiability of `edb`.
///
/// Runs the full [`structured_dl_rules`] set through the shared native structured
/// chase, then reads off the clash facts:
/// every `type(?i, owl:Nothing, ?w)` is an [`InconsistencyWitness`]; every
/// `subClassOf(?c, owl:Nothing, ?w)` (with `?c` not `owl:Nothing` itself) is an
/// [`UnsatClass`]. The verdict is consistent iff no inconsistency witness was
/// derived and no unsupported construct is present in the coverage inventory.
///
/// # Errors
///
/// Returns `Err(String)` if the source store cannot be loaded or native
/// chase/post-pass evaluation fails.
pub fn dl_consistency(edb: &RdfDataset) -> gmeow_errors::Result<DlVerdict> {
    // This is the verdict-only entry point. The single-chase pipeline
    // (run_reasoning → augment_inferred_with_dl → sort → verdict_from_inferred)
    // lives in [`crate::reason::reason_closure`]; we run it and keep only the
    // verdict, dropping the closure callers of this function do not need. Sharing
    // the one pipeline guarantees the verdict here is bit-for-bit the verdict the
    // typed `reason_all` result is folded from (the sort is closure-ordering only
    // and does not change which clash facts are derived).
    Ok(crate::reason::reason_closure(edb)?.1)
}

/// Read off the [`DlVerdict`] from an already-computed native closure.
///
/// Pure over `inferred` for the clash scan (every `type(?i, owl:Nothing, ?w)` is
/// an [`InconsistencyWitness`]; every `subClassOf(?c, owl:Nothing, ?w)` with `?c`
/// not `owl:Nothing` is an [`UnsatClass`]); the coverage scan still walks `edb`
/// because construct presence is an input property, not a derived one. The
/// verdict is consistent iff no inconsistency witness was derived and
/// `gaps` remains empty unless the native coverage inventory contains an
/// unsupported construct.
///
/// Factored out so the single-chase [`crate::reason::reason_all`] can reuse the
/// same `Vec<InferredAxiom>` it derives for the closure. [`dl_consistency`] is
/// the thin wrapper that runs the chase/post-pass then calls this.
///
/// # Errors
///
/// Returns `Err(String)` if a quad cannot be read from `edb` during the coverage scan.
pub(crate) fn verdict_from_inferred(
    inferred: &[InferredAxiom],
    edb: &RdfDataset,
) -> gmeow_errors::Result<DlVerdict> {
    let mut inconsistencies: Vec<InconsistencyWitness> = Vec::new();
    for ax in inferred {
        let object_iri = unwrap_iri(&ax.object);
        // An individual forced into owl:Nothing — an inconsistency witness.
        if ax.predicate == RDF_TYPE && object_iri == OWL_NOTHING {
            inconsistencies.push(InconsistencyWitness {
                individual: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
    }
    // The unsatisfiable (empty, unpopulated) classes are a separate scan, shared
    // with the typed-result fold via [`unsatisfiable_from_inferred`].
    let unsatisfiable_classes = unsatisfiable_from_inferred(inferred);

    // Only a populated clash (an individual in owl:Nothing) makes the ontology
    // inconsistent; an unsatisfiable-but-unpopulated class does not.
    let consistent = inconsistencies.is_empty();

    let coverage = scan_coverage(edb)?;
    let mut gaps = gaps_from_unsupported(&coverage.unsupported);
    // Fold the existential-chase materialization backstop into the verdict as a sound
    // INCOMPLETE withhold: the chase stopped before its fixpoint closed (a certified-but-
    // super-polynomial materialization), so the run cannot certify consistency. The
    // marker rides `inferred` (never `edb`), so this is the only place a runtime resource
    // bound can reach the verdict — it forces `gaps` non-empty (incomplete-never-wrong),
    // never a wrong decided answer.
    if inferred
        .iter()
        .any(|ax| ax.predicate == CHASE_INCOMPLETE_MARKER_PRED)
    {
        gaps.push(DlGap::new(
            "reason.dl-gap.chase-materialization-bound",
            "the DL existential chase reached its materialization backstop before closing \
             its fixpoint; the consistency verdict is withheld as incomplete rather than \
             decided under an unbounded materialization",
        ));
    }
    // Fold the fragment-certified refutation kernel's FAMILY-SCOPED withhold (a
    // present family shape whose completeness bound did not close) into the verdict as
    // an honest, ledger-identified boundary finding. Empty on the committed bundle and
    // every gated input (the kernel's steady state there is `NoDeciderEngaged`, which
    // emits nothing), so this changes no reasoning verdict.
    let boundary_findings = crate::reason::refute::production_boundary_findings(edb);

    Ok(DlVerdict {
        consistent,
        unsatisfiable_classes,
        inconsistencies,
        coverage,
        gaps,
        boundary_findings,
    })
}

/// Scan a native closure for the unsatisfiable (provably-empty, unpopulated)
/// classes: every `subClassOf(?c, owl:Nothing, ?w)` with `?c` not `owl:Nothing`
/// itself. An unsatisfiable class does **not** by itself make the ontology
/// inconsistent (the module distinction).
///
/// Factored out so the typed `logic:ReasoningResult` fold can recover the
/// same DL diagnostic from the shared closure payload without re-running the
/// chase, byte-identically with [`verdict_from_inferred`].
pub fn unsatisfiable_from_inferred(inferred: &[InferredAxiom]) -> Vec<UnsatClass> {
    let mut unsatisfiable_classes: Vec<UnsatClass> = Vec::new();
    for ax in inferred {
        let object_iri = unwrap_iri(&ax.object);
        if ax.predicate == RDFS_SUBCLASSOF
            && object_iri == OWL_NOTHING
            && unwrap_iri(&ax.subject) != OWL_NOTHING
            && ax.subject != OWL_NOTHING
        {
            unsatisfiable_classes.push(UnsatClass {
                class: ax.subject.clone(),
                world: ax.world.clone(),
                premises: ax.premises.clone(),
            });
        }
    }
    unsatisfiable_classes
}

/// Build the DL coverage-gap losses from the unsupported-construct names.
///
/// The single recipe `verdict_from_inferred`, the artifact ledger, and `verify`
/// all share, so the gap `code` (`reason.dl-gap.{name}`) and `message` stay
/// byte-identical whether a consumer reads `DlVerdict::gaps` directly or
/// reconstructs them from a typed result's
/// `preservation.unsupported_constructs`.
pub fn gaps_from_unsupported<I, S>(unsupported: I) -> Vec<DlGap>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    unsupported
        .into_iter()
        .map(|name| {
            let name = name.as_ref();
            DlGap::new(
                format!("reason.dl-gap.{name}"),
                format!(
                    "{name} is present in the bundle but was not decided by the native DL path"
                ),
            )
        })
        .collect()
}

/// Scan the input `edb` for the construct families and report **honest**
/// native coverage.
///
/// `present` lists the families whose IRI appears in the bundle (as a predicate
/// or as an IRI object — restriction fillers ride the object position).
/// `decided` is the subset [`classify_coverage`] proves the native post-pass
/// genuinely decides over the *actual* instances; `unsupported` is the residual
/// `present \ decided`. A present-but-undecided construct is **not** relabelled
/// as covered — it surfaces through [`DlVerdict::gaps`] and gates fail on it.
///
/// # Errors
///
/// Returns `Err` if a quad cannot be read from the source store.
pub fn scan_coverage(edb: &RdfDataset) -> gmeow_errors::Result<DlCoverage> {
    // Materialize the predicate IRIs and object IRIs once; a quad-read error is
    // a hard failure (no-optionality doctrine — silently dropping a quad could
    // miss a construct that must be counted).
    let mut present_iris: std::collections::HashSet<String> = std::collections::HashSet::new();
    for quad in edb.owned_quads() {
        present_iris.insert(quad.predicate);
        if let RdfTerm::Iri(o) = quad.object {
            present_iris.insert(o);
        }
    }

    let mut present: Vec<String> = Vec::new();
    for &(iri, _name, suffix) in CONSTRUCT_COVERAGE {
        if !present_iris.contains(iri) {
            continue;
        }
        present.push(suffix.to_owned());
    }
    present.sort();
    present.dedup();

    // Empirically classify which of the present families the native post-pass
    // can genuinely decide over the real instances.
    let decided_set = classify_coverage(edb, &present);

    // Refutation-shape withholds: local structural configurations whose
    // (in)consistency turns on reasoning-by-contradiction (disjunction case-splits,
    // complement/nominal/arithmetic/datatype counting) the native Horn/EL chase
    // cannot decide. Each is keyed on a shape ABSENT from the committed bundle and
    // the vendored on-gate corpus (measured), so it fires on the W3C DL/Full
    // refutation cases without withholding production. A withheld family is
    // demoted from `decided` and surfaced as an honest `unsupported` gap.
    let withholds = refutation_shape_withholds(edb);

    let mut decided: Vec<String> = present
        .iter()
        .filter(|name| decided_set.contains(name.as_str()))
        .filter(|name| !withholds.contains(name.as_str()))
        .cloned()
        .collect();
    decided.sort();
    let mut unsupported: Vec<String> = present
        .iter()
        .filter(|name| !decided_set.contains(name.as_str()) || withholds.contains(name.as_str()))
        .cloned()
        .collect();
    // Refutation withholds that name a construct NOT in the inventoried `present`
    // set (e.g. a malformed `rdf:List`) still surface as honest gaps.
    for w in &withholds {
        if !unsupported.iter().any(|u| u == w) {
            unsupported.push(w.clone());
        }
    }
    unsupported.sort();
    unsupported.dedup();

    Ok(DlCoverage {
        present,
        decided,
        unsupported,
    })
}

/// Local beyond-native **refutation shapes**: structural configurations whose
/// (in)consistency can be decided only by reasoning-by-contradiction — the
/// disjunction case-splits, complement refutation, cardinality/nominal/arithmetic
/// counting, and datatype value-space counting the native Horn/EL forward chase
/// does not perform. Each returned family name is demoted from `decided` to an
/// honest `unsupported` gap by [`scan_coverage`].
///
/// Every trigger keys on a shape that is **absent from the committed bundle and
/// the vendored on-gate corpus** (verified by direct measurement of the
/// production reasoning EDB and the goldens), so it fires on the W3C OWL 2 DL/Full
/// missed-inconsistency cases WITHOUT withholding the production verdict. The
/// narrow, shape-specific keying is deliberate: mere *presence* of `owl:complementOf`
/// / `owl:cardinality` / `owl:unionOf` is NOT enough — the committed bundle asserts
/// all three in benign, natively-decided positions (`gUFO` uses a complement inside
/// a property `rdfs:domain`, exact `cardinality 1`, and a single `unionOf`
/// superclass). Only the refutation *configuration* triggers a withhold.
fn refutation_shape_withholds(edb: &RdfDataset) -> BTreeSet<String> {
    let mut withholds: BTreeSet<String> = BTreeSet::new();

    // Family 1/3/6b (+ entangled Family 4) — the bounded case-split / complement /
    // union-disjoint / malformed-list sub-decider ([`crate::reason::refute::casesplit`])
    // now COMPLETELY decides a precisely-characterized propositional-plus-nominal
    // fragment of exactly the beyond-Horn refutation shapes withheld below. When it
    // decides the whole case (an `Inconsistent` decision materializes `owl:Nothing`;
    // a `Consistent` decision is certified in-fragment), the complement / union /
    // oneOf / malformed-list families it accounts for stay `decided` rather than
    // being demoted to an honest gap — so each shape-specific withhold below is
    // narrowed by `&& !casesplit_decides`. A case outside its fragment (an
    // existential/cardinality/property-characteristic construct it refuses, or a
    // budget-exceeded search) keeps `casesplit_decides` FALSE, so the withhold
    // still fires. Computed once (the bounded case-split is re-run only here).
    let casesplit_decides = crate::reason::refute::casesplit::decides(edb);

    // H1 — complement used in a positive class-constraint position. Native decides
    // the complement *clash* (`x:A ∧ x:¬A ⇒ Nothing`, via `dl:complement-disjoint`)
    // but NOT complement *refutation* (a class defined/constrained by a negated
    // class expression forced unsatisfiable-yet-nonempty). The refutation shape is a
    // complement node reachable — directly or through `intersectionOf`/`unionOf`
    // nesting — from a class-DEFINITION position (`rdfs:subClassOf`/`owl:equivalentClass`
    // superclass, or a `someValuesFrom`/`allValuesFrom` filler). The committed
    // bundle's complement is referenced ONLY by `rdfs:domain` (a property-scoping
    // position, never reached here), so this never fires on production.
    if (complement_in_class_constraint_position(edb)
        || complement_typed_individual_needs_derivation(edb))
        && !casesplit_decides
    {
        withholds.insert("complementOf".to_owned());
    }

    // H2 — number-cardinality *satisfiability* counting on a class DEFINITION. The
    // native chase decides cardinality clashes on an INDIVIDUAL (an `rdf:type`
    // restriction whose distinct asserted/generated fillers exceed the bound — the
    // Wave-A ABox path), but NOT TBox counting: a named class defined
    // (`owl:equivalentClass`/`rdfs:subClassOf` superclass, or a `some`/`allValuesFrom`
    // filler) as a cardinality restriction whose bounds are contradictory
    // (`min N > max M`), unsatisfiable-yet-forced-nonempty. We therefore withhold a
    // plain `min`/`max` (or exact bound ≥ 2) cardinality restriction ONLY when it
    // sits in a class-definition position — never when it is `rdf:type`d onto an
    // individual (the Wave-A decided case). Every class-definition restriction in the
    // committed bundle is a value restriction (`some`/`all`/`hasValue`) or a QUALIFIED
    // cardinality; the only plain cardinality restrictions it asserts are
    // `math:compilesToLogicFormula`'s two `owl:minCardinality "1"` domain companions,
    // and an `rdfs:domain` filler is a property-scoping position that
    // `nodes_in_class_constraint_position` never collects. Nothing in the bundle is
    // therefore in the withheld shape.
    // Family 2 — the counting sub-decider ([`crate::reason::refute::counting`]) now
    // COMPLETELY decides the pure class-definition cardinality fragment (a collapsed
    // `min > max` bound on a populated class materializes `owl:Nothing`; an
    // uncollapsed one is certified consistent). When it decides the case, the
    // cardinality families it accounts for stay `decided` rather than being demoted
    // to an honest gap; the withhold is narrowed to exactly the residual (a case
    // mixing cardinality with an existential/nominal/identity construct the
    // sub-decider refuses) it cannot decide.
    if !crate::reason::refute::counting::decides_cardinality(edb) {
        let restrictions = read_restrictions(edb);
        let constraint_nodes = nodes_in_class_constraint_position(edb);
        for ((world, node), r) in &restrictions {
            if !constraint_nodes.contains(&(world.clone(), node.clone())) {
                continue;
            }
            if r.min_cardinality.is_some() {
                withholds.insert("minCardinality".to_owned());
            }
            if r.max_cardinality.is_some() {
                withholds.insert("maxCardinality".to_owned());
            }
            if matches!(r.cardinality, Some(n) if n >= 2) {
                withholds.insert("cardinality".to_owned());
            }
        }
    }

    // G8 — datatype value-space counting: a cardinality restriction on a
    // `owl:DatatypeProperty` whose bound can exceed the property's datatype
    // value-space (e.g. `cardinality 257` distinct `xsd:byte` values). The chase
    // carries no datatype value-space reasoning, so it cannot refute the count. No
    // cardinality restriction in the committed bundle targets a datatype property.
    //
    // Family 5 — the datatype value-space refutation sub-decider — now COMPLETELY
    // decides this counting fragment (deriving the finite value-space cardinality
    // from the `math:`-grounded facts). The withhold is narrowed to exactly the
    // residual it cannot decide: when the subsolver decides the case, the family
    // stays `decided` (an inconsistency materializes `owl:Nothing`; a consistent
    // count is certified) rather than being demoted to an honest gap.
    if cardinality_on_datatype_property(edb) && !crate::reason::refute::datatype::decided(edb) {
        withholds.insert("cardinality".to_owned());
    }

    // G8 — a class disjoint with ITSELF (`C owl:disjointWith C`) is empty; forcing it
    // non-empty (a `min`-cardinality/`someValuesFrom`/membership obligation) is an
    // inconsistency whose refutation the chase misses when no individual is directly
    // typed `C`. No class is self-disjoint in the committed bundle.
    if class_disjoint_with_itself(edb) {
        withholds.insert("selfDisjointClass".to_owned());
    }

    // H3 — nominal counting across enumerations. An individual typed to two or more
    // distinct `owl:oneOf` enumeration classes forces a nominal pigeonhole the chase
    // cannot count. (The single-enumeration closure clash — an instance asserted
    // `owl:differentFrom` every member — is still decided by the augment handler and
    // is NOT withheld here.) The committed bundle has no individual typed to ≥2
    // enumerations.
    if individual_in_multiple_enumerations(edb) && !casesplit_decides {
        withholds.insert("oneOf".to_owned());
    }

    // H4 — union propositional refutation. A single class bearing two or more
    // `owl:unionOf` superclasses (`C ⊑ (…∪…)` repeated) is the multi-disjunction
    // propositional-SAT shape whose refutation needs joint case-splitting. The
    // committed bundle has at most ONE union superclass on any class, so this never
    // fires on production; a single disjunctive superclass stays decided.
    if class_with_multiple_union_superclasses(edb) && !casesplit_decides {
        withholds.insert("unionOf".to_owned());
    }

    // G8 — hasSelf membership refutation. A benign `owl:hasSelf` self-restriction
    // typed onto an individual (`x rdf:type ∃p.Self`) is decided consistent (and is
    // an OWL 2 EL construct), but a self-restriction in a `owl:disjointWith` /
    // class-constraint position needs the self-membership inference (`x p x ⇒ x ∈ ∃p.Self`)
    // the chase does not perform to see the clash. Withheld only in that refutation
    // position; the committed bundle asserts no `owl:hasSelf`.
    // Family 7 — the counting sub-decider now decides the `owl:hasSelf` membership
    // refutation: a self-edge (`x p x`) inhabiting a `∃p.Self` restriction that is
    // `owl:disjointWith` a class `x` also holds materializes `owl:Nothing`. When the
    // sub-decider decides the case (a clash, or a certified-consistent benign
    // position), the withhold is dropped; it stays for the residual the sub-decider
    // refuses (a self-restriction entangled with an existential/property-chain
    // construct it does not fold in).
    if has_self_restriction_in_refutation_position(edb)
        && !crate::reason::refute::counting::decides_has_self(edb)
    {
        withholds.insert("hasSelf".to_owned());
    }

    // G8 — malformed `rdf:List`: `rdf:nil` bearing `rdf:first`/`rdf:rest`. A
    // structurally-broken list makes the enclosing axiom's meaning turn on
    // list-well-formedness the chase does not adjudicate. `rdf:nil` never bears a
    // list edge in the committed bundle.
    if nil_bears_list_edge(edb) && !casesplit_decides {
        withholds.insert("malformedRdfList".to_owned());
    }

    withholds
}

/// True iff some `owl:complementOf` class node sits in a positive class-DEFINITION
/// position (see [`nodes_in_class_constraint_position`]). The complement *clash* on
/// a typed individual (`rdf:type`) and the bundle's `rdfs:domain` complement are
/// deliberately excluded — both are natively decided.
fn complement_in_class_constraint_position(edb: &RdfDataset) -> bool {
    let constraint_nodes = nodes_in_class_constraint_position(edb);
    quads_by_subject(edb)
        .into_iter()
        .any(|(subject, predicate, _, world)| {
            predicate == OWL_COMPLEMENT_OF && constraint_nodes.contains(&(world, subject))
        })
}

/// True iff some individual is `rdf:type`d to a complement class `¬M`
/// (`i rdf:type N`, `N owl:complementOf M`) WITHOUT being EDB-asserted `i rdf:type M`.
/// Native decides the *direct* complement clash (`i:N ∧ i:M ⇒ Nothing`) when both
/// memberships are present, but here `i:M` must be DERIVED (via an enumeration,
/// restriction, or subclass) for the refutation to fire — beyond the chase. When
/// `i:M` IS asserted the clash is decided, so that case is NOT withheld (preserving
/// the sound complement-clash decision). The committed bundle types no individual to
/// a complement class.
fn complement_typed_individual_needs_derivation(edb: &RdfDataset) -> bool {
    // (world, complement-node) → complemented class M.
    let mut complement_of: HashMap<(String, String), String> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate == OWL_COMPLEMENT_OF
            && let Some(m) = term_resource_key(&object)
        {
            complement_of.insert((world, subject), m);
        }
    }
    if complement_of.is_empty() {
        return false;
    }
    // (world, individual) → set of asserted types.
    let mut types: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate == RDF_TYPE
            && let Some(class) = term_resource_key(&object)
        {
            types.entry((world, subject)).or_default().insert(class);
        }
    }
    for ((world, _individual), asserted) in &types {
        for class in asserted {
            let Some(m) = complement_of.get(&(world.clone(), class.clone())) else {
                continue;
            };
            // `i : ¬M` present; the clash needs `i : M`, which is NOT asserted.
            if !asserted.contains(m) {
                return true;
            }
        }
    }
    false
}

/// The set of `(world, node)` class-expression nodes reachable — directly or
/// through `owl:intersectionOf`/`owl:unionOf` list nesting — from a positive
/// class-DEFINITION position: the object of `rdfs:subClassOf`/`owl:equivalentClass`,
/// or a `someValuesFrom`/`allValuesFrom` restriction filler.
///
/// `rdf:type` (an individual's membership) and `rdfs:domain`/`rdfs:range`
/// (property scoping) are deliberately NOT definition positions: a construct
/// reached only through them is the ABox/property-scoping case the native chase
/// decides, not the TBox-satisfiability case it cannot refute. This is the shared
/// TBox/ABox boundary the complement (H1) and cardinality (H2) withholds key on.
fn nodes_in_class_constraint_position(edb: &RdfDataset) -> BTreeSet<(String, String)> {
    let lists = read_lists(edb);
    let mut expr_members: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut worklist: Vec<(String, String)> = Vec::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        match predicate.as_str() {
            OWL_INTERSECTION_OF | OWL_UNION_OF => {
                if let Some(root) = term_resource_key(&object)
                    && let Some(members) = lists.get(&(world.clone(), root))
                {
                    expr_members.insert((world.clone(), subject.clone()), members.clone());
                }
            }
            RDFS_SUBCLASSOF | OWL_EQUIVALENT_CLASS | OWL_SOME_VALUES_FROM | OWL_ALL_VALUES_FROM => {
                if let Some(target) = term_resource_key(&object) {
                    worklist.push((world.clone(), target));
                }
            }
            _ => {}
        }
    }
    let mut constraint_used: BTreeSet<(String, String)> = BTreeSet::new();
    while let Some(node) = worklist.pop() {
        if !constraint_used.insert(node.clone()) {
            continue;
        }
        if let Some(members) = expr_members.get(&node) {
            for m in members {
                worklist.push((node.0.clone(), m.clone()));
            }
        }
    }
    constraint_used
}

/// True iff some *plain* (unqualified) cardinality restriction that can require two
/// or more distinct values — an exact `owl:cardinality` bound ≥ 2, or any plain
/// `owl:min`/`maxCardinality` — is `owl:onProperty` a property typed
/// `owl:DatatypeProperty`. That is the datatype value-space counting shape (G8):
/// `cardinality 257` distinct `xsd:byte` values is unsatisfiable, but the chase
/// carries no datatype value-space reasoning to refute it. Qualified cardinalities and
/// the exact `cardinality 1` (functional) case are deliberately NOT withheld, and the
/// committed bundle's only plain cardinality restrictions —
/// `math:compilesToLogicFormula`'s two `owl:minCardinality "1"` domain companions —
/// are `owl:onProperty` an `owl:ObjectProperty`, so this never fires on production.
fn cardinality_on_datatype_property(edb: &RdfDataset) -> bool {
    const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    let datatype_props: BTreeSet<(String, String)> = quads_by_subject(edb)
        .into_iter()
        .filter(|(_, predicate, object, _)| {
            predicate == RDF_TYPE && matches!(object, RdfTerm::Iri(o) if o == OWL_DATATYPE_PROPERTY)
        })
        .map(|(subject, _, _, world)| (world, subject))
        .collect();
    if datatype_props.is_empty() {
        return false;
    }
    let restrictions = read_restrictions(edb);
    restrictions.iter().any(|((world, _node), r)| {
        let counting = matches!(r.cardinality, Some(n) if n >= 2)
            || r.min_cardinality.is_some()
            || r.max_cardinality.is_some();
        counting
            && r.on_property
                .as_ref()
                .is_some_and(|p| datatype_props.contains(&(world.clone(), p.clone())))
    })
}

/// True iff some class is asserted `owl:disjointWith` itself (`C owl:disjointWith C`)
/// — a self-emptying class whose forced-nonempty inconsistency the chase misses
/// (G8 in [`refutation_shape_withholds`]).
fn class_disjoint_with_itself(edb: &RdfDataset) -> bool {
    quads_by_subject(edb)
        .into_iter()
        .any(|(subject, predicate, object, _)| {
            predicate == OWL_DISJOINT_WITH
                && matches!(term_resource_key(&object), Some(o) if o == subject)
        })
}

/// True iff some individual is `rdf:type`d to two or more distinct
/// `owl:oneOf` enumeration classes in one world — the multi-nominal counting shape
/// (H3 in [`refutation_shape_withholds`]).
fn individual_in_multiple_enumerations(edb: &RdfDataset) -> bool {
    let enumeration_classes: BTreeSet<(String, String)> = quads_by_subject(edb)
        .into_iter()
        .filter(|(_, predicate, _, _)| predicate == OWL_ONE_OF)
        .map(|(subject, _, _, world)| (world, subject))
        .collect();
    if enumeration_classes.len() < 2 {
        return false;
    }
    let mut per_individual: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != RDF_TYPE {
            continue;
        }
        let Some(class) = term_resource_key(&object) else {
            continue;
        };
        if enumeration_classes.contains(&(world.clone(), class.clone())) {
            per_individual
                .entry((world, subject))
                .or_default()
                .insert(class);
        }
    }
    per_individual.values().any(|classes| classes.len() >= 2)
}

/// True iff some class is the subject of two or more `rdfs:subClassOf` axioms whose
/// object is an `owl:unionOf` node — the multi-disjunction propositional-refutation
/// shape (H4 in [`refutation_shape_withholds`]).
fn class_with_multiple_union_superclasses(edb: &RdfDataset) -> bool {
    let union_nodes: BTreeSet<(String, String)> = quads_by_subject(edb)
        .into_iter()
        .filter(|(_, predicate, _, _)| predicate == OWL_UNION_OF)
        .map(|(subject, _, _, world)| (world, subject))
        .collect();
    let mut per_subject: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    for (subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != RDFS_SUBCLASSOF {
            continue;
        }
        let Some(target) = term_resource_key(&object) else {
            continue;
        };
        if union_nodes.contains(&(world.clone(), target.clone())) {
            per_subject
                .entry((world, subject))
                .or_default()
                .insert(target);
        }
    }
    per_subject.values().any(|unions| unions.len() >= 2)
}

/// True iff some `owl:hasSelf` self-restriction node sits in a refutation position:
/// a positive class-constraint position (see [`nodes_in_class_constraint_position`])
/// or a subject/object of `owl:disjointWith`. A self-restriction merely `rdf:type`d
/// onto an individual (the OWL 2 EL decided case) is NOT withheld. See
/// [`refutation_shape_withholds`] (G8).
fn has_self_restriction_in_refutation_position(edb: &RdfDataset) -> bool {
    let self_nodes: BTreeSet<(String, String)> = quads_by_subject(edb)
        .into_iter()
        .filter(|(_, predicate, _, _)| predicate == OWL_HAS_SELF)
        .map(|(subject, _, _, world)| (world, subject))
        .collect();
    if self_nodes.is_empty() {
        return false;
    }
    let constraint_nodes = nodes_in_class_constraint_position(edb);
    if self_nodes.iter().any(|n| constraint_nodes.contains(n)) {
        return true;
    }
    quads_by_subject(edb)
        .into_iter()
        .any(|(subject, predicate, object, world)| {
            if predicate != OWL_DISJOINT_WITH {
                return false;
            }
            self_nodes.contains(&(world.clone(), subject))
                || term_resource_key(&object)
                    .is_some_and(|o| self_nodes.contains(&(world.clone(), o)))
        })
}

/// True iff `rdf:nil` appears as the subject of an `rdf:first` or `rdf:rest`
/// triple — a malformed `rdf:List` (G8 in [`refutation_shape_withholds`]).
fn nil_bears_list_edge(edb: &RdfDataset) -> bool {
    const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    quads_by_subject(edb)
        .into_iter()
        .any(|(subject, predicate, _, _)| {
            subject == RDF_NIL && (predicate == RDF_FIRST || predicate == RDF_REST)
        })
}

/// Honestly classify which present construct families the native DL post-pass
/// genuinely decides, returning the set of decided family suffixes.
///
/// "Genuinely decided" means the native path can determine the construct's
/// consistency consequences over **every** present instance — not merely that a
/// match-arm exists. The classifier inspects the actual restrictions/lists in
/// the bundle (the same data [`augment_inferred_with_dl`] reads) and applies a
/// per-family honesty rule:
///
/// - `complementOf`, `domain`, `range`, `allValuesFrom`, `hasValue` — the
///   handler emits its defined consequence for every instance with no missing
///   sub-case (∀-restrictions and hasValue need no fresh individuals; domain/
///   range/complement are unconditional). Decided when present.
/// - `unionOf`, `disjointUnionOf`, `oneOf` — list-backed; decided iff **every**
///   instance resolves to a non-empty RDF list (a dangling/empty list is not
///   handled and so is not decided).
/// - `propertyChainAxiom` — the handler only composes chains of **exactly
///   length 2**; decided iff every present chain is a resolvable 2-list.
/// - `someValuesFrom` — the chase invents a scoped Skolem witness for `∃p.D`
///   (value invention through the native restricted chase), types it `D`, saturates it through the
///   EL/DL rules, and surfaces `∃p.C ⊓ ∀p.D` clashes. Decided iff every present
///   `∃` restriction has a resolvable `onProperty` and filler class so the
///   witness can be generated (Gap B).
/// - the cardinality family (`cardinality`, `min`/`maxCardinality`,
///   `qualifiedCardinality`, `min`/`maxQualifiedCardinality`) — minima generate
///   the required distinct witnesses (the same chase); maxima clash via counting
///   plus the **identity-stance anti-merge** (UNA with `owl:sameAs` merges and
///   `owl:differentFrom`), no longer requiring an explicit `owl:differentFrom`
///   between every pair. Decided iff every present cardinality instance has a
///   parseable bound and a resolvable `onProperty` (and `onClass` for the
///   qualified families).
fn classify_coverage(edb: &RdfDataset, present: &[String]) -> BTreeSet<String> {
    let present_set: BTreeSet<&str> = present.iter().map(String::as_str).collect();
    let restrictions = read_restrictions(edb);
    let lists = read_lists(edb);
    let mut decided: BTreeSet<String> = BTreeSet::new();

    // Unconditionally-complete families: the handler emits its consequence for
    // every instance and has no missing sub-case.
    for family in [
        "complementOf",
        "domain",
        "range",
        "allValuesFrom",
        "hasValue",
    ] {
        if present_set.contains(family) {
            decided.insert(family.to_owned());
        }
    }

    // List-backed enumeration/union families: decided iff every present instance
    // resolves to a non-empty RDF list the post-pass can walk.
    for (family, iri) in [
        ("unionOf", OWL_UNION_OF),
        ("disjointUnionOf", OWL_DISJOINT_UNION_OF),
        ("oneOf", OWL_ONE_OF),
    ] {
        if !present_set.contains(family) {
            continue;
        }
        if !all_list_instances_resolve(edb, iri, &lists) {
            continue;
        }
        // `owl:Thing oneOf …` constrains the (always-fixed, universe-sized) top
        // class to a finite enumeration. Deciding the resulting consistency
        // ("the extension of owl:Thing may not be a singleton") needs the
        // universe-cardinality argument, which is NOT a sound local enumeration
        // rule — the native post-pass only types the members as `owl:Thing`. We
        // therefore leave `oneOf` UNDECIDED whenever any instance targets
        // `owl:Thing`, surfacing it as an honest gap rather than a wrong
        // `consistent` (soundness over completeness).
        if family == "oneOf" && one_of_constrains_thing(edb) {
            continue;
        }
        decided.insert(family.to_owned());
    }

    // owl:bottomObjectProperty / owl:bottomDataProperty: the empty property. The
    // native post-pass forces any individual obligated to bear a value on the
    // bottom property (someValuesFrom / hasValue / ≥1 cardinality restriction)
    // into owl:Nothing. Decided unconditionally when present (the clash is a
    // direct, local consequence).
    for family in ["bottomObjectProperty", "bottomDataProperty"] {
        if present_set.contains(family) {
            decided.insert(family.to_owned());
        }
    }

    // owl:NegativePropertyAssertion: reified from its RDF shape and clashed
    // (literal-aware) against the positive assertion. Decided unconditionally
    // when present — a malformed NPA simply asserts nothing.
    if present_set.contains("negativePropertyAssertion") {
        decided.insert("negativePropertyAssertion".to_owned());
    }

    // owl:FunctionalProperty: a subject with two provably-distinct values on a
    // functional property is forced into owl:Nothing. Decided when present
    // (literal-aware distinctness + the identity stance) UNLESS a functional
    // property carries two lexically-distinct `rdf:XMLLiteral` values: XMLLiteral
    // equality is XML-C14N, which the native chase does not perform, so such a
    // pair is neither provably distinct nor provably equal — honestly withheld
    // (an XMLLiteral value could canonicalize either way) rather than risk a wrong
    // `consistent`/`inconsistent`.
    if present_set.contains("functionalProperty")
        && !functional_property_has_unresolvable_xml_literals(edb)
    {
        decided.insert("functionalProperty".to_owned());
    }

    // Wave A clash families — each decided by a sound local clash rule (no missing
    // sub-case, no value invention): asymmetric/irreflexive property cycles,
    // property-disjointness value collisions, the AllDisjoint*/AllDifferent list
    // expansions. Decided unconditionally when present.
    for family in [
        "asymmetricProperty",
        "irreflexiveProperty",
        "propertyDisjointWith",
        "allDisjointProperties",
        "allDisjointClasses",
        "allDifferent",
    ] {
        if present_set.contains(family) {
            decided.insert(family.to_owned());
        }
    }

    // owl:hasKey / logic:KeyAssertion: two key-agreeing instances asserted owl:differentFrom clash.
    // Decided iff every key axiom (an owl:hasKey list OR a logic:KeyAssertion carrier record)
    // resolves to a non-empty key-property set — see `key_axioms_all_resolve`.
    if present_set.contains("hasKey") && key_axioms_all_resolve(edb, &lists) {
        decided.insert("hasKey".to_owned());
    }

    // owl:propertyChainAxiom: only length-2 resolvable chains are composed.
    if present_set.contains("propertyChainAxiom") && all_property_chains_are_binary(edb, &lists) {
        decided.insert("propertyChainAxiom".to_owned());
    }

    // someValuesFrom: the chase invents a witness for every well-formed `∃p.D`.
    // Decided iff every present `∃` restriction has a resolvable onProperty and
    // filler class (Gap B value-invention).
    if present_set.contains("someValuesFrom")
        && all_some_values_from_instances_decidable(&restrictions)
    {
        decided.insert("someValuesFrom".to_owned());
    }

    // Cardinality family: minima generate distinct witnesses, maxima clash via
    // counting + identity-stance anti-merge. Decided iff every present instance
    // is well-formed for the relevant generation/clash (Gap B). The QUALIFIED
    // families additionally admit a DATATYPE filler (`owl:onDataRange`), whose
    // literal-counted obligation the IRI chase does not carry: those are decided
    // only when no instance carries a live datatype-max violation (PRECISE withhold,
    // mirroring the datatype-facet family) — otherwise the family is left undecided.
    let datatype_qualified_live = datatype_qualified_cardinality_has_live_obligation(edb);
    for family in ["cardinality", "minCardinality", "maxCardinality"] {
        if present_set.contains(family)
            && all_cardinality_instances_decidable(edb, &restrictions, family)
        {
            decided.insert(family.to_owned());
        }
    }
    for family in [
        "qualifiedCardinality",
        "minQualifiedCardinality",
        "maxQualifiedCardinality",
    ] {
        if present_set.contains(family)
            && all_cardinality_instances_decidable(edb, &restrictions, family)
            && !datatype_qualified_live
        {
            decided.insert(family.to_owned());
        }
    }

    // Datatype-facet family: PRECISE withhold. A facet-restricted datatype that is
    // merely DEFINED but constrains no asserted/inferred literal is INERT — it
    // cannot cause an inconsistency, so the native path decides it and the facet
    // families stay out of `gaps`. Only when a literal is actually subject to a
    // facet-restricted datatype (a value the native chase cannot validate) is the
    // family left UNDECIDED (→ honest gap). See
    // `datatype_facet_has_live_obligation`.
    // The datatype value-space refutation sub-decider (Family 5) COMPLETELY decides
    // a precise fragment of these live obligations (facet emptiness / membership,
    // `owl:datatypeComplementOf` value-space membership). When it does, the facet
    // families it accounts for are promoted to `decided` — coverage agreeing with
    // the decider exactly. The `!live` disjunct keeps this inert on a bundle with no
    // live obligation (the subsolver result cannot widen coverage there).
    if !datatype_facet_has_live_obligation(edb) || crate::reason::refute::datatype::decided(edb) {
        for family in DATATYPE_FACET_FAMILIES {
            if present_set.contains(family) {
                decided.insert((*family).to_owned());
            }
        }
    }

    // A LITERAL `owl:oneOf` datatype enumeration is a value-space the native list
    // reader skips (it drops literal members), so it is left undecided above. The
    // datatype value-space subsolver counts its distinct values exactly; promote the
    // `oneOf` family to `decided` precisely when the subsolver decides the case AND
    // every enumeration in the EDB is a literal datatype enumeration (never an
    // object enumeration, which the native path already decides).
    if present_set.contains("oneOf")
        && crate::reason::refute::datatype::decided(edb)
        && crate::reason::refute::datatype::all_oneof_are_literal_enumerations(edb)
    {
        decided.insert("oneOf".to_owned());
    }

    // owl:InverseFunctionalProperty carries NO native identity-merge rule, so it was
    // never promoted above (the chase cannot see the `1 = 2` collapse). The Family-6a
    // counting sub-decider ([`crate::reason::refute::counting`]) now wires the real
    // inverse-functional `sameAs` propagation: it merges the subjects that share a
    // value and clashes a merged pair asserted `owl:differentFrom`. It certifies the
    // family only for the PURE assertional/identity fragment (no class-construction
    // vocabulary present); a case mixing IFP with a class definition it does not
    // fold in is refused, so `inverseFunctionalProperty` stays an honest gap there.
    if present_set.contains("inverseFunctionalProperty")
        && crate::reason::refute::counting::decides_identity(edb)
    {
        decided.insert("inverseFunctionalProperty".to_owned());
    }

    // The universal top properties (`owl:topObjectProperty`/`owl:topDataProperty`)
    // are DELIBERATELY never inserted into `decided`: the native chase implements no
    // top-property semantics, so a bundle asserting them can only be reported as
    // cannot-decide (they remain in `present \ decided` = `unsupported`, populating
    // `DlVerdict::gaps`). This is the incomplete-never-wrong contract — never a
    // silently-ignored axiom.

    decided
}

/// True iff the EDB contains a LIVE undecidable datatype-facet obligation — a
/// facet-restricted datatype the native chase would actually have to reason over
/// to decide (in)consistency. There are two live shapes.
///
/// SHAPE 1, an existential into a facet datatype: a facet-restricted datatype as
/// the filler of an `owl:someValuesFrom` restriction (`∃p.D`). The existential
/// forces a datatype value to EXIST; native cannot decide the datatype's
/// (non)emptiness (e.g. the discrete `xsd:float` range `(0.0, 1.4e-45)` is empty),
/// so an instance typed into it is undecidable — regardless of any asserted literal.
///
/// SHAPE 2, a constrained literal: an asserted (or inferred) literal value on a
/// property whose `rdfs:range` or an `owl:allValuesFrom`/`owl:someValuesFrom`
/// restriction resolves to a facet-restricted datatype, or a literal directly typed
/// to such a datatype. Native cannot validate the literal against the facet.
///
/// A facet-restricted datatype — an `rdfs:Datatype` node carrying both
/// `owl:onDatatype` and `owl:withRestrictions`, or an `owl:datatypeComplementOf`
/// datatype — that is
/// merely DEFINED, or used only in a UNIVERSAL (`owl:allValuesFrom`) position with
/// no asserted value, is INERT: `∀p.D` over an empty value set is trivially
/// satisfied, so it cannot make the ontology inconsistent and the native path
/// decides it soundly (the datatype-facet families stay out of `gaps`). Matching
/// is name-scoped (world-agnostic) — a facet definition in a TBox world still
/// withholds against an obligation in any ABox world (soundness-first).
fn datatype_facet_has_live_obligation(edb: &RdfDataset) -> bool {
    // Facet-restricted datatype node keys (world-agnostic: a datatype's identity is
    // its node, and a constrained property may reference it from any world).
    let mut facet_dt: BTreeSet<String> = BTreeSet::new();
    for (subject, predicate, _object, _world) in quads_by_subject(edb) {
        if predicate == OWL_ON_DATATYPE
            || predicate == OWL_WITH_RESTRICTIONS
            || predicate == OWL_DATATYPE_COMPLEMENT_OF
        {
            facet_dt.insert(subject);
        }
    }
    if facet_dt.is_empty() {
        return false;
    }

    // Shape 1 — an existential (`someValuesFrom`) into a facet datatype forces a
    // value to exist; the datatype's (non)emptiness is undecidable natively.
    // (`allValuesFrom` is universal and is NOT a standalone obligation — it only
    // constrains values that are actually asserted, handled by Shape 2.)
    let restrictions = read_restrictions(edb);
    for r in restrictions.values() {
        if let Some(filler) = r.some_values_from.as_ref()
            && facet_dt.contains(filler)
        {
            return true;
        }
    }

    // Shape 2 — properties whose values must lie in a facet datatype: an
    // `rdfs:range` edge, or a value restriction (`all`/`someValuesFrom`) whose
    // filler is a facet datatype.
    let mut constrained_props: BTreeSet<String> = BTreeSet::new();
    for (subject, predicate, object, _world) in quads_by_subject(edb) {
        if predicate == RDFS_RANGE
            && let Some(dt) = term_resource_key(&object)
            && facet_dt.contains(&dt)
        {
            constrained_props.insert(subject);
        }
    }
    for r in restrictions.values() {
        let Some(prop) = r.on_property.as_ref() else {
            continue;
        };
        if let Some(filler) = r.all_values_from.as_ref().or(r.some_values_from.as_ref())
            && facet_dt.contains(filler)
        {
            constrained_props.insert(prop.clone());
        }
    }

    // A live obligation: an asserted literal on a constrained property, or a literal
    // directly typed to a facet-restricted (named) datatype.
    for quad in edb.owned_quads() {
        let RdfTerm::Literal(lit) = &quad.object else {
            continue;
        };
        if constrained_props.contains(&quad.predicate) {
            return true;
        }
        if let Some(dt) = lit.datatype.as_deref()
            && facet_dt.contains(dt)
        {
            return true;
        }
    }
    false
}

/// True iff some `owl:FunctionalProperty` carries two lexically-distinct
/// `rdf:XMLLiteral` values on a single subject — a shape the native chase cannot
/// decide (XMLLiteral equality is XML canonicalization, which it does not
/// perform), so the `functionalProperty` family is honestly withheld rather than
/// risk a wrong verdict on it.
fn functional_property_has_unresolvable_xml_literals(edb: &RdfDataset) -> bool {
    let value_index = build_value_index(edb);
    let functional_props: BTreeSet<(String, String)> = value_index
        .iter()
        .filter(|((_, _, pred), _)| pred.as_str() == RDF_TYPE)
        .flat_map(|((world, subject, _), terms)| {
            terms.values().filter_map(move |t| match t {
                RdfTerm::Iri(o) if o == OWL_FUNCTIONAL_PROPERTY => {
                    Some((world.clone(), subject.clone()))
                }
                _ => None,
            })
        })
        .collect();
    for ((world, _subject, pred), terms) in &value_index {
        if !functional_props.contains(&(world.clone(), pred.clone())) {
            continue;
        }
        let mut xml_forms: BTreeSet<&str> = BTreeSet::new();
        for term in terms.values() {
            if let RdfTerm::Literal(lit) = term
                && lit.datatype.as_deref() == Some(RDF_XML_LITERAL)
            {
                xml_forms.insert(lit.lexical_form.as_str());
            }
        }
        if xml_forms.len() >= 2 {
            return true;
        }
    }
    false
}

/// True iff every `owl:someValuesFrom` restriction is well-formed enough for the
/// chase to discharge it: it carries an `onProperty` and a resolvable
/// (IRI/bnode) filler class. A literal or absent filler/property is not a shape
/// the value-invention handler generates for, so it stays undecided.
fn all_some_values_from_instances_decidable(
    restrictions: &BTreeMap<(String, String), Restriction>,
) -> bool {
    let mut saw_instance = false;
    for restriction in restrictions.values() {
        if restriction.some_values_from.is_none() {
            continue;
        }
        saw_instance = true;
        if restriction.on_property.is_none() || restriction.some_values_from.is_none() {
            return false;
        }
    }
    saw_instance
}

/// True iff any `owl:oneOf` axiom has `owl:Thing` as its subject (i.e. it
/// enumerates the top class). Such an axiom needs the universe-cardinality
/// argument the native post-pass does not perform, so its presence keeps the
/// `oneOf` family honestly undecided.
fn one_of_constrains_thing(edb: &RdfDataset) -> bool {
    for (subject, predicate, _object, _world) in quads_by_subject(edb) {
        if predicate == OWL_ONE_OF && subject == OWL_THING {
            return true;
        }
    }
    false
}

/// True iff every quad whose predicate is `list_predicate` points at a resolved,
/// non-empty RDF list (so the union/oneOf/disjointUnion handler can walk it).
fn all_list_instances_resolve(
    edb: &RdfDataset,
    list_predicate: &str,
    lists: &HashMap<(String, String), Vec<String>>,
) -> bool {
    let mut saw_instance = false;
    for (_subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != list_predicate {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            return false;
        };
        saw_instance = true;
        match lists.get(&(world, root)) {
            Some(members) if !members.is_empty() => {}
            _ => return false,
        }
    }
    saw_instance
}

/// True iff every `owl:propertyChainAxiom` instance is a resolvable list of
/// exactly two properties (the only shape the post-pass composes).
fn all_property_chains_are_binary(
    edb: &RdfDataset,
    lists: &HashMap<(String, String), Vec<String>>,
) -> bool {
    let mut saw_instance = false;
    for (_subject, predicate, object, world) in quads_by_subject(edb) {
        if predicate != OWL_PROPERTY_CHAIN_AXIOM {
            continue;
        }
        let Some(root) = term_resource_key(&object) else {
            return false;
        };
        saw_instance = true;
        match lists.get(&(world, root)) {
            Some(members) if members.len() == 2 => {}
            _ => return false,
        }
    }
    saw_instance
}

/// True iff the EDB carries a LIVE undecidable DATATYPE-qualified cardinality
/// obligation — a datatype-qualified maximum (`owl:maxQualifiedCardinality` /
/// exact `owl:qualifiedCardinality` with `owl:onDataRange`) that some instance
/// could actually violate.
///
/// A datatype-qualified cardinality counts LITERAL fillers, which the
/// IRI-individual chase does not carry. Like a merely-defined datatype facet
/// ([`datatype_facet_has_live_obligation`]), such a restriction is INERT when no
/// instance can violate it — a maximum on a datatype property can only clash when a
/// subject carries MORE value-distinct literals of that datatype than the bound
/// allows, and a datatype MINIMUM is an existential into a non-empty datatype
/// (always satisfiable). Only a genuine max overflow is a live obligation the
/// native path cannot decide; absent one, the datatype-qualified families are
/// decided soundly (incomplete-never-wrong). Matching is exact on the literal's
/// datatype, mirroring [`datatype_facet_has_live_obligation`].
fn datatype_qualified_cardinality_has_live_obligation(edb: &RdfDataset) -> bool {
    let restrictions = read_restrictions(edb);
    // Datatype-qualified maxima: `(on_property, datatype, max)`. An exact
    // `owl:qualifiedCardinality n` also bounds the maximum at `n`.
    let mut maxima: Vec<(String, String, usize)> = Vec::new();
    for r in restrictions.values() {
        let (Some(prop), Some(dt)) = (r.on_property.as_ref(), r.on_data_range.as_ref()) else {
            continue;
        };
        for bound in [r.max_qualified_cardinality, r.qualified_cardinality]
            .into_iter()
            .flatten()
        {
            maxima.push((prop.clone(), dt.clone(), bound));
        }
    }
    if maxima.is_empty() {
        return false;
    }
    // The literal-aware value index already folds value-distinct fillers (its keys
    // are `term_value_key`), so a bucket's literal count IS the distinct-value count.
    let value_index = build_value_index(edb);
    for ((_world, _subject, pred), terms) in &value_index {
        for (prop, dt, max) in &maxima {
            if pred != prop {
                continue;
            }
            let distinct = terms
                .values()
                .filter(|t| {
                    matches!(t, RdfTerm::Literal(l) if l.datatype.as_deref() == Some(dt.as_str()))
                })
                .count();
            if distinct > *max {
                return true;
            }
        }
    }
    false
}

/// True iff every restriction carrying the cardinality `family` is in the
/// genuinely-decidable sub-case for the native handler (Gap B).
///
/// A cardinality instance is decidable when the chase can act on it: it has a
/// **parseable** non-negative integer bound and a resolvable `onProperty`, and —
/// for the qualified families — a resolvable `onClass`. Given that shape:
/// - a *minimum* (`min`/exact/`qualified`/`minQualified`) discharges by inventing
///   the required distinct Skolem witnesses through the native restricted chase;
/// - a *maximum* (`max`/exact/`qualified`/`maxQualified`) clashes by counting
///   distinct fillers under the identity-stance anti-merge ([`pairwise_distinct`]).
///
/// An unparsable bound, a missing `onProperty`, or a qualified restriction with
/// neither `onClass` nor `onDataRange` is a shape the handler cannot act on, so it
/// stays undecided (honesty over green) and surfaces as a gap. A DATATYPE-qualified
/// restriction (`onDataRange`) is well-formed here; its inert-vs-live decision is
/// taken separately in [`classify_coverage`] via
/// [`datatype_qualified_cardinality_has_live_obligation`].
fn all_cardinality_instances_decidable(
    edb: &RdfDataset,
    restrictions: &BTreeMap<(String, String), Restriction>,
    family: &str,
) -> bool {
    let (predicate_iri, qualified) = match family {
        "cardinality" => (OWL_CARDINALITY, false),
        "minCardinality" => (OWL_MIN_CARDINALITY, false),
        "maxCardinality" => (OWL_MAX_CARDINALITY, false),
        "qualifiedCardinality" => (OWL_QUALIFIED_CARDINALITY, true),
        "minQualifiedCardinality" => (OWL_MIN_QUALIFIED_CARDINALITY, true),
        "maxQualifiedCardinality" => (OWL_MAX_QUALIFIED_CARDINALITY, true),
        _ => return false,
    };
    // Scan the raw quads (not the parsed restrictions) so an *unparsable* bound
    // literal is caught as undecided rather than silently skipped.
    let mut saw_instance = false;
    for (subject, predicate, _object, world) in quads_by_subject(edb) {
        if predicate != predicate_iri {
            continue;
        }
        saw_instance = true;
        let Some(restriction) = restrictions.get(&(world, subject)) else {
            return false;
        };
        let bound = match family {
            "cardinality" => restriction.cardinality,
            "minCardinality" => restriction.min_cardinality,
            "maxCardinality" => restriction.max_cardinality,
            "qualifiedCardinality" => restriction.qualified_cardinality,
            "minQualifiedCardinality" => restriction.min_qualified_cardinality,
            "maxQualifiedCardinality" => restriction.max_qualified_cardinality,
            _ => None,
        };
        // Unparsable bound, missing onProperty, or (qualified) missing onClass:
        // the handler cannot generate/count, so the instance stays undecided.
        if bound.is_none() || restriction.on_property.is_none() {
            return false;
        }
        if qualified && restriction.on_class.is_none() && restriction.on_data_range.is_none() {
            return false;
        }
    }
    saw_instance
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = RDFS_SUBCLASSOF;
    const TYPE: &str = RDF_TYPE;
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
    const MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
    const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const R: &str = "http://gmeow.example/R";
    const S: &str = "http://gmeow.example/S";
    const P: &str = "http://gmeow.example/p";
    const X: &str = "http://gmeow.example/x";
    const Y: &str = "http://gmeow.example/y";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn literal_quad(s: &str, p: &str, value: &str, datatype: &str) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri(s),
            p,
            RdfTerm::Literal(RdfLiteral::typed(value, datatype)),
        )
        .in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    #[test]
    fn authored_existential_rules_are_read_and_certified_per_world() {
        // An authored general existential rule (arbitrary body/head atoms — NOT an OWL
        // restriction) is read per-world and certified by the termination-class ladder.
        const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
        const EX_P: &str = "http://ex/p";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
        let lx = |s: &str| format!("{LX}{s}");
        // A `?var` term is encoded as a string literal; a constant as an IRI.
        let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
        // MSA swap-diagonal program: `p(x,x) → ∃y. p(x,y)` and `p(x,y) → p(y,x)`.
        let store = dataset(vec![
            quad("http://ex/demo/invent", TYPE, &lx("ExistentialRule")),
            quad("http://ex/demo/invent", &lx("body"), "http://ex/demo/b1"),
            quad("http://ex/demo/invent", &lx("head"), "http://ex/demo/h1"),
            var("http://ex/demo/b1", &lx("s"), "?x"),
            quad("http://ex/demo/b1", &lx("p"), EX_P),
            var("http://ex/demo/b1", &lx("o"), "?x"),
            var("http://ex/demo/h1", &lx("s"), "?x"),
            quad("http://ex/demo/h1", &lx("p"), EX_P),
            var("http://ex/demo/h1", &lx("o"), "?y"),
            quad("http://ex/demo/swap", TYPE, &lx("ExistentialRule")),
            quad("http://ex/demo/swap", &lx("body"), "http://ex/demo/b2"),
            quad("http://ex/demo/swap", &lx("head"), "http://ex/demo/h2"),
            var("http://ex/demo/b2", &lx("s"), "?x"),
            quad("http://ex/demo/b2", &lx("p"), EX_P),
            var("http://ex/demo/b2", &lx("o"), "?y"),
            var("http://ex/demo/h2", &lx("s"), "?y"),
            quad("http://ex/demo/h2", &lx("p"), EX_P),
            var("http://ex/demo/h2", &lx("o"), "?x"),
        ]);
        let by_world =
            authored_existential_rules(store.as_ref()).expect("well-formed authored rules");
        let rules = by_world
            .get(W)
            .expect("authored rules land in their graph's world");
        assert_eq!(rules.len(), 2, "both authored rules parsed");
        match crate::physical::ChaseAdmission::certify(rules) {
            crate::physical::ChaseAdmission::ModelSummarizingAcyclic { .. } => {}
            other => panic!("swap-diagonal must certify as ModelSummarizingAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn authored_rule_with_malformed_atom_hard_fails_not_silently_dropped() {
        // A declared body atom missing its `logicx:o` must HARD FAIL — never a silent drop.
        // Silently dropping the conjunct would leave the rule with a smaller body, firing
        // more often and deriving facts the author never wrote (no-optionality violation).
        const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
        const EX_P: &str = "http://ex/p";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
        let lx = |s: &str| format!("{LX}{s}");
        let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
        let store = dataset(vec![
            quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
            quad("http://ex/demo/r", &lx("body"), "http://ex/demo/b1"),
            quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
            // b1 declares its subject and predicate but is MISSING its logicx:o (object).
            var("http://ex/demo/b1", &lx("s"), "?x"),
            quad("http://ex/demo/b1", &lx("p"), EX_P),
            var("http://ex/demo/h1", &lx("s"), "?x"),
            quad("http://ex/demo/h1", &lx("p"), EX_P),
            var("http://ex/demo/h1", &lx("o"), "?y"),
        ]);
        let err = authored_existential_rules(store.as_ref())
            .expect_err("a declared atom missing logicx:o must hard-fail, not be dropped");
        let msg = format!("{err}");
        assert!(
            msg.contains("logicx:o") && msg.contains("b1"),
            "error must name the missing slot and the offending atom: {msg}"
        );
    }

    #[test]
    fn authored_rule_with_non_resource_body_ref_hard_fails() {
        // A `logicx:body` whose value is a literal (not a resource) cannot name an atom node.
        // Silently dropping it would leave the rule with fewer body conjuncts than authored —
        // a broadening. It must HARD FAIL at collection time, not be skipped.
        const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
        const EX_P: &str = "http://ex/p";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
        let lx = |s: &str| format!("{LX}{s}");
        let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
        let store = dataset(vec![
            quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
            // logicx:body points at a LITERAL — not a resource, so it names no atom node.
            literal_quad("http://ex/demo/r", &lx("body"), "not-a-node", XSD_STRING),
            quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
            var("http://ex/demo/h1", &lx("s"), "?x"),
            quad("http://ex/demo/h1", &lx("p"), EX_P),
            var("http://ex/demo/h1", &lx("o"), "?y"),
        ]);
        let err = authored_existential_rules(store.as_ref())
            .expect_err("a non-resource logicx:body must hard-fail, not be silently dropped");
        let msg = format!("{err}");
        assert!(
            msg.contains("logicx:body") && msg.contains("not a resource"),
            "error must name the slot and the non-resource cause: {msg}"
        );
    }

    #[test]
    fn authored_rule_with_duplicate_slot_hard_fails() {
        // Two `logicx:s` triples on one atom node would silently OVERWRITE the first,
        // reinterpreting the authored atom. A duplicate slot must HARD FAIL, not pick a winner.
        const LX: &str = "https://blackcatinformatics.ca/gmeow/logic/existential#";
        const EX_P: &str = "http://ex/p";
        const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
        let lx = |s: &str| format!("{LX}{s}");
        let var = |s: &str, p: &str, v: &str| literal_quad(s, p, v, XSD_STRING);
        let store = dataset(vec![
            quad("http://ex/demo/r", TYPE, &lx("ExistentialRule")),
            quad("http://ex/demo/r", &lx("body"), "http://ex/demo/b1"),
            quad("http://ex/demo/r", &lx("head"), "http://ex/demo/h1"),
            // b1 declares its subject TWICE — a duplicate slot.
            var("http://ex/demo/b1", &lx("s"), "?x"),
            var("http://ex/demo/b1", &lx("s"), "?z"),
            quad("http://ex/demo/b1", &lx("p"), EX_P),
            var("http://ex/demo/b1", &lx("o"), "?y"),
            var("http://ex/demo/h1", &lx("s"), "?x"),
            quad("http://ex/demo/h1", &lx("p"), EX_P),
            var("http://ex/demo/h1", &lx("o"), "?y"),
        ]);
        let err = authored_existential_rules(store.as_ref())
            .expect_err("a duplicate logicx:s must hard-fail, not silently overwrite");
        let msg = format!("{err}");
        assert!(
            msg.contains("logicx:s") && msg.contains("more than one"),
            "error must name the duplicated slot: {msg}"
        );
    }

    #[test]
    fn disjoint_superclasses_make_a_unsat_and_x_inconsistent() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A
        // ⇒ A is unsatisfiable, and x is forced into owl:Nothing (inconsistent).
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "x forced into owl:Nothing must make the ontology inconsistent"
        );
        assert!(
            verdict.unsatisfiable_classes.iter().any(|u| u.class == A),
            "A must be reported unsatisfiable: {:?}",
            verdict.unsatisfiable_classes
        );
        let witness = verdict
            .inconsistencies
            .iter()
            .find(|w| w.individual == X)
            .expect("x must be an inconsistency witness");
        assert_eq!(witness.world, W, "witness carries its world IRI");
        assert!(
            !witness.premises.is_empty(),
            "derived inconsistency must carry antecedent premises"
        );
    }

    #[test]
    fn no_disjointness_is_consistent() {
        // A ⊑ B, x : A — no disjointness ⇒ consistent, no inconsistencies.
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(X, TYPE, A)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "no clash ⇒ consistent");
        assert!(
            verdict.inconsistencies.is_empty(),
            "no individual should be forced into owl:Nothing"
        );
    }

    #[test]
    fn complement_of_is_decided_and_can_clash() {
        // A complementOf B, x : A, x : B ⇒ x : owl:Nothing. This construct is
        // decided natively, so it must NOT surface as a DlGap.
        let store = dataset(vec![
            quad(A, super::OWL_COMPLEMENT_OF, B),
            quad(X, TYPE, A),
            quad(X, TYPE, B),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(!verdict.consistent, "complement clash must be inconsistent");
        assert!(
            verdict.gaps.is_empty(),
            "owl:complementOf is decided natively, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .coverage
                .present
                .contains(&"complementOf".to_owned()),
            "coverage records complementOf as present: {:?}",
            verdict.coverage
        );
    }

    // ── Refutation-shape withholds (Wave B) ──────────────────────────────────
    // Each feeds a native-undecidable refutation shape and asserts the verdict
    // honestly WITHHOLDS: a non-empty `gaps` (the `incomplete` token) and NOT a
    // wrong decided verdict. Falsifiable: a reasoner that silently ignored the
    // axiom would report a decided `consistent` with empty `gaps` and fail here.

    const ONE_OF: &str = super::OWL_ONE_OF;
    const UNION_OF: &str = super::OWL_UNION_OF;
    const COMPLEMENT_OF: &str = super::OWL_COMPLEMENT_OF;
    const EQUIV_CLASS: &str = super::OWL_EQUIVALENT_CLASS;
    const MIN_CARDINALITY: &str = super::OWL_MIN_CARDINALITY;
    const CARDINALITY: &str = super::OWL_CARDINALITY;
    const DIFFERENT_FROM: &str = super::OWL_DIFFERENT_FROM;
    const HAS_SELF: &str = super::OWL_HAS_SELF;
    const INVERSE_FUNCTIONAL: &str = super::OWL_INVERSE_FUNCTIONAL_PROPERTY;
    const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

    fn assert_withheld(verdict: &DlVerdict, token: &str) {
        assert!(
            !verdict.gaps.is_empty(),
            "expected a non-empty gap (honest cannot-decide), got none"
        );
        assert!(
            verdict.coverage.unsupported.iter().any(|u| u == token),
            "expected withheld family {token:?} in coverage.unsupported, got {:?}",
            verdict.coverage.unsupported
        );
    }

    #[test]
    fn complement_in_class_definition_is_decided_consistent() {
        // A ⊑ ¬D — the complement node is a `rdfs:subClassOf` superclass (a class
        // definition), with NO individual forced into it. The Family-1 case-split
        // refutation sub-decider ([`crate::reason::refute::casesplit`]) now COMPLETELY
        // decides this propositional-fragment case: an individual-free complement TBox
        // is trivially satisfiable (the empty interpretation is a model), so it is
        // decided CONSISTENT with no honest gap — no longer the pre-sub-decider
        // conservative withhold.
        let store = dataset(vec![
            quad(A, SUBCLASS, "http://gmeow.example/ncomp"),
            quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "an individual-free complement TBox is satisfiable"
        );
        assert!(
            !verdict.gaps.iter().any(|g| g.code.contains("complementOf"))
                && !verdict
                    .coverage
                    .unsupported
                    .iter()
                    .any(|u| u == "complementOf"),
            "the case-split sub-decider certifies this — no complementOf gap: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn complement_filler_in_restriction_is_withheld() {
        // A ⊑ ∃p.(¬D): complement as a `someValuesFrom` filler is a class-definition
        // position ⇒ honest gap.
        let store = dataset(vec![
            quad(A, SUBCLASS, R),
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, "http://gmeow.example/ncomp"),
            quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert_withheld(&verdict, "complementOf");
    }

    #[test]
    fn complement_typed_individual_without_asserted_membership_is_decided_consistent() {
        // x : ¬D, but x is NOT asserted x : D. The Family-1 case-split sub-decider now
        // decides this: `x ∈ ¬D` with no forced `x ∈ D` saturates clash-free inside
        // the certified-complete fragment, so it is decided CONSISTENT (a model has
        // `x ∉ D`) — no longer the pre-sub-decider withhold. Contrast the decided
        // clash test where BOTH memberships are asserted (decided INCONSISTENT).
        let store = dataset(vec![
            quad(X, TYPE, "http://gmeow.example/ncomp"),
            quad("http://gmeow.example/ncomp", COMPLEMENT_OF, D),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "x ∈ ¬D with no forced x ∈ D is satisfiable"
        );
        assert!(
            !verdict.gaps.iter().any(|g| g.code.contains("complementOf"))
                && !verdict
                    .coverage
                    .unsupported
                    .iter()
                    .any(|u| u == "complementOf"),
            "the case-split sub-decider certifies this — no complementOf gap: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn min_cardinality_on_a_class_definition_is_decided_consistent() {
        // C ≡ (≥2 p): a pure TBox cardinality counting definition. The Family-2
        // counting refutation sub-decider now COMPLETELY decides the pure
        // class-definition cardinality fragment: an uncollapsed bound (no `min > max`
        // conflict) on a class is satisfiable, so this is decided CONSISTENT with no
        // honest gap — no longer the pre-sub-decider withhold.
        let store = dataset(vec![
            quad(C, EQUIV_CLASS, R),
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MIN_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "an uncollapsed ≥2 definition is satisfiable"
        );
        assert!(
            verdict.gaps.is_empty(),
            "the counting sub-decider certifies this — no honest gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .coverage
                .decided
                .iter()
                .any(|d| d == "minCardinality"),
            "the minCardinality family is promoted to decided: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn collapsed_cardinality_on_a_populated_class_is_decided_inconsistent() {
        // C ⊑ (≥2 p) ⊓ (≤1 p), i : C — the collapsed bound makes the populated class
        // unsatisfiable, so the Family-2 sub-decider materializes `owl:Nothing` on the
        // instance: decided INCONSISTENT with no honest gap.
        let r2 = "http://gmeow.example/r2";
        let store = dataset(vec![
            quad(C, SUBCLASS, R),
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MIN_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            quad(C, SUBCLASS, r2),
            quad(r2, ON_PROPERTY, P),
            literal_quad(r2, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, C),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "a collapsed min>max bound on a populated class is inconsistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "the counting sub-decider decides this — no honest gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn exact_cardinality_over_finite_datatype_range_is_decided_by_family5() {
        // Family 5 (the datatype value-space refutation sub-decider) now DECIDES the
        // datatype value-space counting the forward chase cannot refute. With p an
        // `owl:DatatypeProperty` whose `rdfs:range` is `xsd:byte` (value-space size
        // 256, derived from the `math:`-grounded facts):
        //   * `x : (=257 p)` forces 257 distinct byte values into a 256-element
        //     space ⇒ pigeonhole INCONSISTENT (an `owl:Nothing` clash, empty gaps);
        //   * `x : (=256 p)` fits exactly ⇒ CONSISTENT (empty gaps).
        const BYTE: &str = "http://www.w3.org/2001/XMLSchema#byte";
        const RANGE: &str = super::RDFS_RANGE;

        let inconsistent = dataset(vec![
            quad(X, TYPE, R),
            quad(R, ON_PROPERTY, P),
            literal_quad(R, CARDINALITY, "257", XSD_NON_NEGATIVE_INTEGER),
            quad(P, TYPE, DATATYPE_PROPERTY),
            quad(P, RANGE, BYTE),
        ]);
        let verdict = dl_consistency(inconsistent.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "257 distinct xsd:byte values overflow the 256-element value space ⇒ inconsistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "Family 5 decides this — no honest gap remains: {:?}",
            verdict.gaps
        );
        assert!(
            verdict.coverage.decided.iter().any(|d| d == "cardinality"),
            "the cardinality family is promoted to decided: {:?}",
            verdict.coverage
        );

        let consistent = dataset(vec![
            quad(X, TYPE, R),
            quad(R, ON_PROPERTY, P),
            literal_quad(R, CARDINALITY, "256", XSD_NON_NEGATIVE_INTEGER),
            quad(P, TYPE, DATATYPE_PROPERTY),
            quad(P, RANGE, BYTE),
        ]);
        let verdict = dl_consistency(consistent.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "256 distinct xsd:byte values fit the value space exactly ⇒ consistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "Family 5 certifies the consistent count — no honest gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn individual_typed_to_two_enumerations_is_withheld() {
        // x : {a,b} AND x : {c,d} — nominal counting across two enumerations ⇒ gap.
        // (The single-enumeration `differentFrom`-all-members clash stays decided.)
        let e1 = "http://gmeow.example/E1";
        let e2 = "http://gmeow.example/E2";
        let store = dataset(vec![
            quad(e1, ONE_OF, "http://gmeow.example/l0"),
            quad("http://gmeow.example/l0", FIRST, A),
            quad("http://gmeow.example/l0", REST, NIL),
            quad(e2, ONE_OF, "http://gmeow.example/l1"),
            quad("http://gmeow.example/l1", FIRST, B),
            quad("http://gmeow.example/l1", REST, NIL),
            quad(X, TYPE, e1),
            quad(X, TYPE, e2),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert_withheld(&verdict, "oneOf");
    }

    #[test]
    fn class_with_two_union_superclasses_is_decided_consistent() {
        // C ⊑ (A∪B) AND C ⊑ (X∪Y): the multi-disjunction propositional shape. With NO
        // individual forced into C, the Family-3 case-split sub-decider decides it
        // CONSISTENT (the empty model satisfies every disjunctive superclass) — no
        // longer the pre-sub-decider conservative withhold. The propositional
        // refutation (every branch closing) is exercised end-to-end on the committed
        // `webont-description-logic-503`/`504` SAT pair.
        let u1 = "http://gmeow.example/u1";
        let u2 = "http://gmeow.example/u2";
        let store = dataset(vec![
            quad(C, SUBCLASS, u1),
            quad(u1, UNION_OF, "http://gmeow.example/lu1"),
            quad("http://gmeow.example/lu1", FIRST, A),
            quad("http://gmeow.example/lu1", REST, NIL),
            quad(C, SUBCLASS, u2),
            quad(u2, UNION_OF, "http://gmeow.example/lu2"),
            quad("http://gmeow.example/lu2", FIRST, B),
            quad("http://gmeow.example/lu2", REST, NIL),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "an individual-free union TBox is satisfiable"
        );
        assert!(
            !verdict.gaps.iter().any(|g| g.code.contains("unionOf"))
                && !verdict.coverage.unsupported.iter().any(|u| u == "unionOf"),
            "the case-split sub-decider certifies this — no unionOf gap: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn hasself_disjoint_refutation_is_decided_inconsistent() {
        // C disjointWith [∃p.Self], x : C, x p x. The Family-7 counting sub-decider
        // now infers `x ∈ ∃p.Self` from the self-edge and clashes it against the
        // disjoint class x also holds: decided INCONSISTENT with no honest gap — no
        // longer the pre-sub-decider withhold.
        let store = dataset(vec![
            quad(C, DISJOINT, R),
            quad(R, TYPE, "http://www.w3.org/2002/07/owl#Restriction"),
            literal_quad(
                R,
                HAS_SELF,
                "true",
                "http://www.w3.org/2001/XMLSchema#boolean",
            ),
            quad(R, ON_PROPERTY, P),
            quad(X, TYPE, C),
            quad(X, P, X),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "a self-edge inhabiting a disjoint self-restriction is inconsistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "the hasSelf sub-decider decides this — no honest gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn hasself_typed_onto_individual_is_decided_not_a_gap() {
        // x : ∃p.Self with no disjointness — a benign OWL 2 EL self-restriction.
        // Decided consistent, NO gap (the EL grade's `selfrestriction` case).
        let store = dataset(vec![
            quad(X, TYPE, R),
            quad(R, TYPE, "http://www.w3.org/2002/07/owl#Restriction"),
            literal_quad(
                R,
                HAS_SELF,
                "true",
                "http://www.w3.org/2001/XMLSchema#boolean",
            ),
            quad(R, ON_PROPERTY, P),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(verdict.consistent, "a bare self-restriction is consistent");
        assert!(
            !verdict.coverage.unsupported.iter().any(|u| u == "hasSelf"),
            "benign hasSelf must NOT be withheld: {:?}",
            verdict.coverage.unsupported
        );
    }

    #[test]
    fn malformed_nil_list_is_decided_inconsistent() {
        // rdf:nil bearing an rdf:first edge — a malformed rdf:List. The Family-6b
        // case-split sub-decider now decides this: a structurally-broken list makes
        // the world inconsistent, materializing `owl:Nothing` (on `rdf:nil`) — decided
        // INCONSISTENT with no honest gap, no longer the pre-sub-decider withhold.
        let store = dataset(vec![quad(NIL, FIRST, A)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(!verdict.consistent, "a malformed rdf:List is inconsistent");
        assert!(
            !verdict
                .gaps
                .iter()
                .any(|g| g.code.contains("malformedRdfList"))
                && !verdict
                    .coverage
                    .unsupported
                    .iter()
                    .any(|u| u == "malformedRdfList"),
            "the case-split sub-decider decides this — no malformedRdfList gap: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn self_disjoint_class_is_withheld() {
        // C disjointWith C — an empty class whose forced-nonempty inconsistency the
        // chase misses ⇒ honest gap.
        let store = dataset(vec![quad(C, DISJOINT, C), quad(X, TYPE, A)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert_withheld(&verdict, "selfDisjointClass");
    }

    #[test]
    fn inverse_functional_property_is_decided_consistent() {
        // owl:InverseFunctionalProperty has no native identity-merge clash rule, but
        // the Family-6a counting sub-decider now wires the real inverse-functional
        // `sameAs` propagation. A single assertion with no distinctness merges
        // nothing to clash — decided CONSISTENT with no honest gap (the pure
        // assertional/identity fragment).
        let store = dataset(vec![quad(P, TYPE, INVERSE_FUNCTIONAL), quad(X, P, Y)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(verdict.consistent, "a lone IFP assertion is consistent");
        assert!(
            verdict.gaps.is_empty(),
            "the identity sub-decider certifies this — no honest gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .coverage
                .decided
                .iter()
                .any(|d| d == "inverseFunctionalProperty"),
            "the inverseFunctionalProperty family is promoted to decided: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn inverse_functional_collapse_with_differentfrom_is_decided_inconsistent() {
        // s1 p o, s2 p o, p IFP, s1 differentFrom s2 — the `1 = 2` collapse: the IFP
        // merges s1 and s2, contradicting their asserted distinctness. Decided
        // INCONSISTENT with no honest gap.
        let s1 = "http://gmeow.example/s1";
        let s2 = "http://gmeow.example/s2";
        let store = dataset(vec![
            quad(P, TYPE, INVERSE_FUNCTIONAL),
            quad(s1, P, Y),
            quad(s2, P, Y),
            quad(s1, DIFFERENT_FROM, s2),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "an IFP-merged pair asserted differentFrom is inconsistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "the identity sub-decider decides this — no honest gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn single_enumeration_differentfrom_clash_stays_decided_under_withholds() {
        // Regression: the sound single-enumeration nominal clash (x : {a,b},
        // x differentFrom a, x differentFrom b ⇒ Nothing) must NOT be withheld by the
        // multi-enumeration trigger — it has ONE enumeration.
        let e1 = "http://gmeow.example/E1";
        let store = dataset(vec![
            quad(e1, ONE_OF, "http://gmeow.example/l0"),
            quad("http://gmeow.example/l0", FIRST, A),
            quad("http://gmeow.example/l0", REST, "http://gmeow.example/l1"),
            quad("http://gmeow.example/l1", FIRST, B),
            quad("http://gmeow.example/l1", REST, NIL),
            quad(X, TYPE, e1),
            quad(X, DIFFERENT_FROM, A),
            quad(X, DIFFERENT_FROM, B),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "single-enumeration clash is decided inconsistent"
        );
        assert!(
            verdict.gaps.is_empty(),
            "single-enumeration clash must NOT be withheld: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn union_of_with_resolvable_list_is_decided_not_a_gap() {
        // owl:unionOf is a positive finite class-expression consequence in the
        // predicate-as-DATA path *when its list resolves*: A = unionOf (B C).
        // Its presence with a walkable list must not emit a DlGap.
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(A, super::OWL_UNION_OF, l0),
            quad(l0, FIRST, B),
            quad(l0, REST, l1),
            quad(l1, FIRST, C),
            quad(l1, REST, NIL),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "bare union axiom is consistent");
        assert!(
            verdict.gaps.is_empty(),
            "owl:unionOf with a resolvable list is decided natively, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict.coverage.present.contains(&"unionOf".to_owned()),
            "coverage records unionOf as present: {:?}",
            verdict.coverage
        );
        assert!(
            verdict.coverage.decided.contains(&"unionOf".to_owned()),
            "coverage records unionOf as decided: {:?}",
            verdict.coverage
        );
    }

    #[test]
    fn one_of_closure_forces_a_non_member_instance_into_nothing() {
        // Colour = oneOf (red green); x : Colour but x differentFrom both red and
        // green ⇒ x can be no member ⇒ x : owl:Nothing ⇒ INCONSISTENT. This is the
        // CLOSURE half of oneOf (the member→type direction is the easy half), the
        // beyond-EL nominal reasoning the frozen external OWL 2 DL oracle gold demands
        // native catch (native ⊇ oracle).
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let colour = "http://gmeow.example/Colour";
        let red = "http://gmeow.example/red";
        let green = "http://gmeow.example/green";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(colour, super::OWL_ONE_OF, l0),
            quad(l0, FIRST, red),
            quad(l0, REST, l1),
            quad(l1, FIRST, green),
            quad(l1, REST, NIL),
            quad(X, TYPE, colour),
            quad(X, OWL_DIFFERENT_FROM, red),
            quad(X, OWL_DIFFERENT_FROM, green),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "an enumeration instance distinct from every member must clash: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict.inconsistencies.iter().any(|w| w.individual == X),
            "x must be the inconsistency witness: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn one_of_with_a_member_instance_is_consistent() {
        // Same enumeration, but x is NOT asserted distinct from the members, so by
        // the open identity stance it may be one of them ⇒ NO clash ⇒ CONSISTENT.
        // Proves the closure clash is real (driven by differentFrom), not a blanket
        // "any instance of a oneOf class is unsat".
        const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let colour = "http://gmeow.example/Colour";
        let red = "http://gmeow.example/red";
        let green = "http://gmeow.example/green";
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(colour, super::OWL_ONE_OF, l0),
            quad(l0, FIRST, red),
            quad(l0, REST, l1),
            quad(l1, FIRST, green),
            quad(l1, REST, NIL),
            quad(X, TYPE, colour),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "an enumeration instance not asserted distinct from the members is \
             consistent: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    const SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
    const QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
    const MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
    const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
    const SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    const D: &str = "http://gmeow.example/D";
    const Z: &str = "http://gmeow.example/z";

    #[test]
    fn exists_p_c_and_all_p_d_with_disjoint_c_d_is_inconsistent() {
        // GAP B keystone — the case the old inert handler missed.
        // R = ∃p.C, S = ∀p.D, C disjointWith D, x : R, x : S, NO asserted filler.
        // The chase must invent a scoped witness w with p(x,w) and type(w,C);
        // ∀p.D then types w as D; C disjoint D ⇒ w : owl:Nothing ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(S, ON_PROPERTY, P),
            quad(S, ALL_VALUES_FROM, D),
            quad(C, DISJOINT, D),
            quad(X, TYPE, R),
            quad(X, TYPE, S),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "∃p.C ⊓ ∀p.D with C⊓D⊑⊥ must be inconsistent via an invented witness: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .inconsistencies
                .iter()
                .any(|w| w.individual.starts_with(crate::facts::SKOLEM_PREFIX)),
            "the inconsistency witness must be the invented Skolem filler: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict.gaps.is_empty(),
            "someValuesFrom is now genuinely decided — no gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn some_values_from_satisfiable_filler_is_consistent_and_terminates() {
        // R = ∃p.C, x : R, no disjointness anywhere. The chase invents w, types
        // it C, and reaches a fixed point WITHOUT regenerating witnesses
        // (content-addressed identity). If termination were broken this test
        // would hang rather than fail — its mere completion is the witness.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "a satisfiable ∃p.C is consistent");
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"someValuesFrom".to_owned()),
            "someValuesFrom is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn cyclic_some_values_from_terminates() {
        // C ⊑ ∃p.C (R = ∃p.C, C ⊑ R). x : C forces a witness w typed C, which is
        // therefore R, which needs a p-filler of type C — the SAME class-set in
        // the SAME world, so the witness pool is reused (no fresh chain). The
        // restricted-chase blocking by class-set guarantees this terminates; the
        // test completing at all is the termination proof.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(C, SUBCLASS, R),
            quad(X, TYPE, C),
        ]);
        let (closure, verdict) = crate::reason::reason_closure(store.as_ref())
            .expect("cyclic DL existential reasoning should terminate");

        assert!(verdict.consistent, "cyclic but satisfiable ∃ is consistent");
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
        let filler = closure
            .iter()
            .find(|axiom| axiom.subject == X && axiom.predicate == P)
            .map(|axiom| unwrap_iri(&axiom.object).to_owned())
            .expect("the root receives one existential filler");
        assert!(closure.iter().any(|axiom| {
            axiom.subject == filler && axiom.predicate == P && unwrap_iri(&axiom.object) == filler
        }));
        assert_eq!(
            closure.iter().filter(|axiom| axiom.predicate == P).count(),
            2,
            "ancestor blocking must close the recursive model instead of growing a witness chain"
        );
    }

    #[test]
    fn qualified_min_two_uses_two_distinct_native_chase_witnesses() {
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let (closure, verdict) = crate::reason::reason_closure(store.as_ref())
            .expect("structured native existential chase should decide >=2");
        assert!(verdict.consistent);

        let mut fillers = closure
            .iter()
            .filter(|axiom| axiom.subject == X && axiom.predicate == P)
            .map(|axiom| unwrap_iri(&axiom.object).to_owned())
            .collect::<Vec<_>>();
        fillers.sort();
        fillers.dedup();
        assert_eq!(fillers.len(), 2, ">=2 must invent exactly two witnesses");
        for filler in &fillers {
            assert!(closure.iter().any(|axiom| {
                axiom.subject == *filler
                    && axiom.predicate == TYPE
                    && unwrap_iri(&axiom.object) == C
            }));
        }
        assert!(closure.iter().any(|axiom| {
            axiom.predicate == OWL_DIFFERENT_FROM
                && ((axiom.subject == fillers[0] && unwrap_iri(&axiom.object) == fillers[1])
                    || (axiom.subject == fillers[1] && unwrap_iri(&axiom.object) == fillers[0]))
        }));
        assert!(closure.iter().any(|axiom| {
            axiom
                .rule_name
                .as_deref()
                .is_some_and(|name| name.contains("dl-existential"))
        }));
    }

    #[test]
    fn existential_witnesses_are_frontier_bound_per_subject_and_deterministic() {
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, SOME_VALUES_FROM, C),
            quad(X, TYPE, R),
            quad(Y, TYPE, R),
        ]);
        let (first, first_verdict) = crate::reason::reason_closure(store.as_ref())
            .expect("structured native existential chase should decide both obligations");
        let (second, second_verdict) = crate::reason::reason_closure(store.as_ref())
            .expect("repeated native chase should be deterministic");

        assert!(first_verdict.consistent);
        assert!(second_verdict.consistent);
        assert_eq!(first, second, "frontier-addressed witnesses must be stable");

        let fillers = |subject: &str| {
            first
                .iter()
                .filter(|axiom| axiom.subject == subject && axiom.predicate == P)
                .map(|axiom| unwrap_iri(&axiom.object).to_owned())
                .collect::<BTreeSet<_>>()
        };
        let x_fillers = fillers(X);
        let y_fillers = fillers(Y);
        assert_eq!(x_fillers.len(), 1, "x needs exactly one existential filler");
        assert_eq!(y_fillers.len(), 1, "y needs exactly one existential filler");
        assert!(
            x_fillers.is_disjoint(&y_fillers),
            "different frontier bindings must not share a rule-scoped witness"
        );
        for filler in x_fillers.iter().chain(&y_fillers) {
            assert!(first.iter().any(|axiom| {
                axiom.subject == *filler
                    && axiom.predicate == TYPE
                    && unwrap_iri(&axiom.object) == C
            }));
        }
        for subject in [X, Y] {
            let link = first
                .iter()
                .find(|axiom| axiom.subject == subject && axiom.predicate == P)
                .expect("each subject must have a derived existential link");
            assert_eq!(
                link.premises,
                vec![(subject.to_owned(), TYPE.to_owned(), format!("<{R}>"))],
                "the production explanation must cite the matched restriction membership"
            );
        }

        let certified = crate::reason::reason_all_certified(store.as_ref())
            .expect("production reasoning should retain chase certificates");
        assert_eq!(certified.result.inferred(), first.as_slice());
        assert_eq!(
            certified.chase_certificates.len(),
            1,
            "the repeated DL fixpoint must deduplicate the same world/program certificate"
        );
        let finding = certified.chase_certificates[0].to_finding();
        assert_eq!(finding.code, "chase.certificate.weakly-acyclic");
        assert!(
            finding
                .message
                .contains("existential edge(s), none in a cycle")
                && !finding.message.contains("0 existential edge(s)"),
            "frontier certification must carry non-vacuous special-edge evidence: {finding:?}"
        );

        // The witness derivations swept out of the chase carry the exact recipe an
        // explain(witness) consumer decomposes: one per invented null, each pinning
        // its firing rule, existential ordinal, and frontier binding. This is the
        // reasoning-result surface the pipeline projects into graph/diagnostics.
        let all_fillers: BTreeSet<String> = x_fillers.iter().chain(&y_fillers).cloned().collect();
        assert_eq!(
            certified.witness_derivations.len(),
            2,
            "each frontier binding mints one decomposable witness derivation"
        );
        let derived_witnesses: BTreeSet<String> = certified
            .witness_derivations
            .iter()
            .map(|derivation| derivation.witness.clone())
            .collect();
        assert_eq!(
            derived_witnesses, all_fillers,
            "every invented filler must carry a swept-out witness derivation"
        );
        for derivation in &certified.witness_derivations {
            assert_eq!(derivation.ordinal, 0, "the single ∃-head fills ordinal 0");
            assert!(
                !derivation.rule_iri.is_empty(),
                "the firing rule must be pinned on the derivation"
            );
            let frontier_iris: Vec<&str> = derivation
                .frontier
                .iter()
                .map(|term| match term {
                    TermValue::Iri(iri) => iri.as_str(),
                    other => panic!("frontier binding must be an IRI: {other:?}"),
                })
                .collect();
            assert_eq!(
                frontier_iris.len(),
                1,
                "the DL restriction has exactly one frontier subject"
            );
            assert!(
                frontier_iris[0] == X || frontier_iris[0] == Y,
                "the frontier binding must be a bound demonstrand subject: {frontier_iris:?}"
            );
        }
    }

    #[test]
    fn max_one_with_two_provably_distinct_fillers_clashes() {
        // R = ≤1 p (maxCardinality 1), x : R, x p y, x p z, y owl:differentFrom z.
        // The two fillers are PROVABLY distinct (explicit differentFrom — no UNA
        // shortcut) ⇒ the ≤1 maximum is violated ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, OWL_DIFFERENT_FROM, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "two provably-distinct fillers under ≤1 must clash: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"maxCardinality".to_owned()),
            "positive maxCardinality is now decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn max_one_with_two_named_fillers_no_differentfrom_is_consistent() {
        // SOUNDNESS FLOOR (no unique-name assumption): the SAME ≤1 p shape but the
        // two named fillers carry NO owl:differentFrom. Standard OWL does not
        // assume unique names, so y and z may be owl:sameAs ⇒ the ≤1 maximum is NOT
        // violated ⇒ CONSISTENT. (The old UNA default reported a FALSE
        // inconsistency here — the OWL-2 restrict-maxcard-inst-obj-one regression.)
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "two named fillers without differentFrom must NOT clash under ≤1: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn max_one_with_mergeable_fillers_is_consistent() {
        // Same ≤1 p, but y owl:sameAs z merges the two fillers ⇒ NOT distinct ⇒
        // no clash ⇒ CONSISTENT. Proves the anti-merge is real, not a count of
        // raw IRIs.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, SAME_AS, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "mergeable fillers (sameAs) must NOT clash under ≤1: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn min_qualified_two_generates_two_distinct_witnesses_and_terminates() {
        // R = ≥2 p.C (minQualifiedCardinality 2, onClass C), x : R, no asserted
        // filler. The chase must invent TWO distinct witnesses w0,w1 both typed C
        // and both p-fillers of x. Consistent, terminating, and the ≥2 obligation
        // is then met (so re-running invents nothing new).
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(verdict.consistent, "a satisfiable ≥2 p.C is consistent");
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"minQualifiedCardinality".to_owned()),
            "minQualifiedCardinality is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn min_two_then_max_one_qualified_clashes() {
        // R = ≥2 p.C AND ≤1 p.C on the same restriction (minQualifiedCardinality 2
        // + qualifiedCardinality... use min 2 + maxCardinality 1 unqualified for a
        // crisp clash). The min generates 2 distinct C-witnesses, the max-1 then
        // counts 2 distinct fillers ⇒ clash ⇒ INCONSISTENT. Demonstrates the
        // generation and counting interlock.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, MIN_QUALIFIED_CARDINALITY, "2", XSD_NON_NEGATIVE_INTEGER),
            literal_quad(R, MAX_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "≥2 p.C with ≤1 p must clash after witness generation: {:?}",
            verdict.inconsistencies
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn qualified_cardinality_one_with_two_distinct_c_fillers_clashes() {
        // R = =1 p.C (qualifiedCardinality 1, onClass C), x : R, x p y, x p z,
        // y:C, z:C, y owl:differentFrom z (PROVABLY distinct — no UNA) ⇒ the =1
        // maximum clashes ⇒ INCONSISTENT.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_CLASS, C),
            literal_quad(R, QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, TYPE, C),
            quad(Z, TYPE, C),
            quad(Y, OWL_DIFFERENT_FROM, Z),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "=1 p.C with two distinct C-fillers must clash: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"qualifiedCardinality".to_owned()),
            "qualifiedCardinality is decided: {:?}",
            verdict.coverage
        );
        assert!(verdict.gaps.is_empty(), "no gap: {:?}", verdict.gaps);
    }

    #[test]
    fn unparseable_cardinality_bound_stays_unsupported_so_the_gate_can_fire() {
        // The gate must still be able to fire for a genuinely-undecidable case.
        // A maxCardinality whose literal is NOT a non-negative integer cannot be
        // acted on by the handler, so it stays `unsupported` → a non-empty gaps,
        // proving the gate is not dead code.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            literal_quad(
                R,
                MAX_CARDINALITY,
                "not-a-number",
                "http://www.w3.org/2001/XMLSchema#string",
            ),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"maxCardinality".to_owned()),
            "an unparsable cardinality bound is not genuinely decided: {:?}",
            verdict.coverage
        );
        assert!(
            !verdict.gaps.is_empty(),
            "an undecidable cardinality instance must yield a gap so the gate can fire: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .gaps
                .iter()
                .any(|g| g.code == "reason.dl-gap.maxCardinality"),
            "gaps must name the undecided construct: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn datatype_qualified_max_without_violation_is_decided_inert() {
        // R = ≤1 p.decimal (maxQualifiedCardinality 1, onDataRange xsd:decimal),
        // x : R, one decimal filler. A datatype-qualified maximum counts LITERAL
        // fillers the IRI chase does not carry; with no subject exceeding the bound
        // it is INERT and the native path decides it (no gap), never withholding the
        // whole qualified family on the absent `owl:onClass`.
        const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
        const MAX_QUALIFIED_CARDINALITY: &str =
            "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
        const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_DATA_RANGE, DECIMAL),
            literal_quad(R, MAX_QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            literal_quad(X, P, "1.5", DECIMAL),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "a ≤1 p.decimal with one value is consistent: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"maxQualifiedCardinality".to_owned()),
            "an inert datatype-qualified maximum is decided: {:?}",
            verdict.coverage
        );
        assert!(
            verdict.gaps.is_empty(),
            "no gap for an inert datatype-qualified maximum: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn datatype_qualified_max_overflow_stays_unsupported_so_the_gate_can_fire() {
        // Same ≤1 p.decimal, but x carries TWO value-distinct decimal literals — a
        // live max overflow the literal-blind IRI chase cannot decide. The family is
        // honestly WITHHELD (unsupported → gap), never wrongly reported decided.
        const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
        const MAX_QUALIFIED_CARDINALITY: &str =
            "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
        const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ON_DATA_RANGE, DECIMAL),
            literal_quad(R, MAX_QUALIFIED_CARDINALITY, "1", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
            literal_quad(X, P, "1.5", DECIMAL),
            literal_quad(X, P, "2.5", DECIMAL),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"maxQualifiedCardinality".to_owned()),
            "a live datatype-max overflow is not genuinely decided: {:?}",
            verdict.coverage
        );
        assert!(
            verdict
                .gaps
                .iter()
                .any(|g| g.code == "reason.dl-gap.maxQualifiedCardinality"),
            "gaps must name the withheld construct: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn some_values_from_without_on_property_stays_unsupported() {
        // A malformed ∃ restriction (someValuesFrom but no onProperty) cannot be
        // discharged by the chase, so someValuesFrom stays unsupported → gap.
        let store = dataset(vec![quad(R, SOME_VALUES_FROM, C), quad(X, TYPE, R)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"someValuesFrom".to_owned()),
            "a someValuesFrom with no onProperty is not decidable: {:?}",
            verdict.coverage
        );
        assert!(
            !verdict.gaps.is_empty(),
            "must yield a gap: {:?}",
            verdict.gaps
        );
    }

    #[test]
    fn all_values_from_pushes_type_into_existing_fillers() {
        // R = ∀p.B, x : R, x p y, y : C, B disjoint C ⇒ y : owl:Nothing.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, ALL_VALUES_FROM, B),
            quad(B, DISJOINT, C),
            quad(X, TYPE, R),
            quad(X, P, Y),
            quad(Y, TYPE, C),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "allValuesFrom must type y as B and clash with y : C"
        );
        assert!(verdict.gaps.is_empty(), "allValuesFrom is decided");
    }

    #[test]
    fn has_value_emits_required_property_and_can_clash_with_max_zero() {
        // R = (= p y) and R maxCardinality 0 on p. x : R forces x p y, then
        // max 0 detects the contradiction.
        let store = dataset(vec![
            quad(R, ON_PROPERTY, P),
            quad(R, HAS_VALUE, Y),
            literal_quad(R, MAX_CARDINALITY, "0", XSD_NON_NEGATIVE_INTEGER),
            quad(X, TYPE, R),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.consistent,
            "hasValue plus maxCardinality 0 must be inconsistent"
        );
        assert!(verdict.gaps.is_empty(), "hasValue/cardinality are decided");
        assert!(
            verdict.coverage.present.contains(&"hasValue".to_owned())
                && verdict
                    .coverage
                    .present
                    .contains(&"maxCardinality".to_owned()),
            "coverage records hasValue and maxCardinality: {:?}",
            verdict.coverage
        );
    }

    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

    fn bnode_quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    // ── owl:bottomObjectProperty / owl:bottomDataProperty ─────────────────────

    /// `i : ∃ owl:bottomObjectProperty . owl:Thing` is unsatisfiable — the bottom
    /// property is empty, so an obligation to bear a value on it forces
    /// owl:Nothing.
    #[test]
    fn bottom_object_property_some_values_from_is_inconsistent() {
        let restriction = "http://gmeow.example/r";
        let store = dataset(vec![
            quad(restriction, ON_PROPERTY, OWL_BOTTOM_OBJECT_PROPERTY),
            quad(restriction, super::OWL_SOME_VALUES_FROM, OWL_THING),
            quad(X, TYPE, restriction),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "bottom-object-property obligation clashes"
        );
        assert!(
            verdict.gaps.is_empty(),
            "bottomObjectProperty is decided, not a gap: {:?}",
            verdict.gaps
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"bottomObjectProperty".to_owned())
        );
    }

    /// The data-property analog: `i : ∃ owl:bottomDataProperty . rdfs:Literal`.
    #[test]
    fn bottom_data_property_some_values_from_is_inconsistent() {
        let restriction = "http://gmeow.example/r";
        let literal_class = "http://www.w3.org/2000/01/rdf-schema#Literal";
        let store = dataset(vec![
            quad(restriction, ON_PROPERTY, OWL_BOTTOM_DATA_PROPERTY),
            quad(restriction, super::OWL_SOME_VALUES_FROM, literal_class),
            quad(X, TYPE, restriction),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "bottom-data-property obligation clashes"
        );
        assert!(verdict.gaps.is_empty(), "bottomDataProperty is decided");
    }

    // ── owl:NegativePropertyAssertion (object + data) ─────────────────────────

    /// NPA(Peter, hasSon, Meg) co-present with hasSon(Peter, Meg) ⇒ inconsistent.
    #[test]
    fn negative_object_property_assertion_clashes_with_positive() {
        let peter = "http://gmeow.example/Peter";
        let meg = "http://gmeow.example/Meg";
        let has_son = "http://gmeow.example/hasSon";
        let npa = "npa";
        let store = dataset(vec![
            quad(peter, has_son, meg),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, peter),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_son),
            bnode_quad(npa, OWL_TARGET_INDIVIDUAL, meg),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(!verdict.consistent, "NPA contradicted by its positive");
        assert!(verdict.gaps.is_empty(), "NPA is decided");
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"negativePropertyAssertion".to_owned())
        );
    }

    /// The data analog: NPA(Meg, hasAge, "5") + hasAge(Meg, "5") ⇒ inconsistent,
    /// and a DIFFERENT literal value must NOT clash (literal-aware target match).
    #[test]
    fn negative_data_property_assertion_is_literal_aware() {
        let meg = "http://gmeow.example/Meg";
        let has_age = "http://gmeow.example/hasAge";
        let int_ty = "http://www.w3.org/2001/XMLSchema#integer";
        let npa = "npa";
        // Positive value "5" matches the negated target "5" — clash.
        let store = dataset(vec![
            literal_quad(meg, has_age, "5", int_ty),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
            RdfQuad::new(
                RdfTerm::blank_node(npa),
                OWL_TARGET_VALUE,
                RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "matching literal value clashes the NPA"
        );

        // A DIFFERENT positive value ("6") does not match the negated "5".
        let store_ok = dataset(vec![
            literal_quad(meg, has_age, "6", int_ty),
            bnode_quad(npa, TYPE, OWL_NEGATIVE_PROPERTY_ASSERTION),
            bnode_quad(npa, OWL_SOURCE_INDIVIDUAL, meg),
            bnode_quad(npa, OWL_ASSERTION_PROPERTY, has_age),
            RdfQuad::new(
                RdfTerm::blank_node(npa),
                OWL_TARGET_VALUE,
                RdfTerm::Literal(RdfLiteral::typed("5", int_ty)),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict_ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict_ok.consistent,
            "a non-matching literal value must NOT clash the NPA"
        );
    }

    // ── owl:FunctionalProperty (data values) ──────────────────────────────────

    /// A functional data property with two distinct literal values forces
    /// owl:Nothing; a single value is consistent.
    #[test]
    fn functional_data_property_two_literals_clash() {
        let peter = "http://gmeow.example/Peter";
        let has_name = "http://gmeow.example/hasName";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let store = dataset(vec![
            quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
            literal_quad(peter, has_name, "Kichwa-Tembo", str_ty),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "two distinct literal values on a functional property clash"
        );
        assert!(
            verdict
                .coverage
                .decided
                .contains(&"functionalProperty".to_owned())
        );

        let store_ok = dataset(vec![
            quad(has_name, TYPE, OWL_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
        ]);
        let verdict_ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(verdict_ok.consistent, "a single value is consistent");
    }

    /// Functionality declared ONLY by the canonical `logic:PropertyCharacteristicAssertion`
    /// carrier record (no `owl:FunctionalProperty` marker) still forces owl:Nothing on a subject
    /// with two distinct literal values — the derivation source the object-level reasoning EDB
    /// relies on once the `owl:FunctionalProperty` slice source declarations are removed. Coverage
    /// stays honest: with no OWL functional construct in the EDB, `functionalProperty` is not
    /// reported present, yet the clash is still decided from the carrier.
    #[test]
    fn functional_data_property_carrier_record_two_literals_clash() {
        let peter = "http://gmeow.example/Peter";
        let has_name = "http://gmeow.example/hasName";
        let rec = "http://gmeow.example/hasName-functional-record";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let store = dataset(vec![
            quad(rec, LOGIC_CHARACTERIZES, has_name),
            quad(rec, LOGIC_CHARACTERISTIC_SORT, LOGIC_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
            literal_quad(peter, has_name, "Kichwa-Tembo", str_ty),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "two distinct literal values on a carrier-declared functional property clash"
        );

        let store_ok = dataset(vec![
            quad(rec, LOGIC_CHARACTERIZES, has_name),
            quad(rec, LOGIC_CHARACTERISTIC_SORT, LOGIC_FUNCTIONAL_PROPERTY),
            literal_quad(peter, has_name, "Peter", str_ty),
        ]);
        let verdict_ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict_ok.consistent,
            "a single value under the carrier record is consistent"
        );
    }

    // ── owl:hasKey ────────────────────────────────────────────────────────────

    /// hasKey(owl:Thing, [hasSSN]); two differentFrom individuals sharing the key
    /// literal are forced into owl:Nothing.
    #[test]
    fn has_key_collision_with_explicit_distinctness_clashes() {
        let peter = "http://gmeow.example/Peter";
        let pg = "http://gmeow.example/Peter_Griffin";
        let has_ssn = "http://gmeow.example/hasSSN";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let key_list = "keylist";
        let store = dataset(vec![
            bnode_quad(key_list, FIRST, has_ssn),
            bnode_quad(key_list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                OWL_HAS_KEY,
                RdfTerm::blank_node(key_list),
            )
            .in_graph(RdfTerm::iri(W)),
            literal_quad(peter, has_ssn, "123-45-6789", str_ty),
            literal_quad(pg, has_ssn, "123-45-6789", str_ty),
            quad(peter, OWL_DIFFERENT_FROM, pg),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "key-agreeing differentFrom individuals clash"
        );
        assert!(verdict.gaps.is_empty(), "hasKey is decided");
        assert!(verdict.coverage.decided.contains(&"hasKey".to_owned()));
    }

    /// WITHOUT explicit owl:differentFrom, two key-agreeing individuals are
    /// merely owl:sameAs (no UNA in standard OWL) — consistent, NOT a false clash.
    #[test]
    fn has_key_collision_without_distinctness_is_consistent() {
        let peter = "http://gmeow.example/Peter";
        let pg = "http://gmeow.example/Peter_Griffin";
        let has_ssn = "http://gmeow.example/hasSSN";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let key_list = "keylist";
        let store = dataset(vec![
            bnode_quad(key_list, FIRST, has_ssn),
            bnode_quad(key_list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                OWL_HAS_KEY,
                RdfTerm::blank_node(key_list),
            )
            .in_graph(RdfTerm::iri(W)),
            literal_quad(peter, has_ssn, "123-45-6789", str_ty),
            literal_quad(pg, has_ssn, "123-45-6789", str_ty),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.consistent,
            "without differentFrom, key agreement just merges the two (sameAs) — consistent"
        );
    }

    /// The gtsHeadId / GTSSegment key expressed ONLY through the greenfield `logic:KeyAssertion`
    /// carrier (no `owl:hasKey`) still DECIDES the DL verdict: two GTSSegment individuals asserted
    /// `owl:differentFrom` yet sharing one gtsHeadId content-id are forced into owl:Nothing, and the
    /// `hasKey` family is reported present + decided from the carrier. This is the falsifiable
    /// no-regression guard after the `owl:hasKey ( gmeow:gtsHeadId )` slice declaration is migrated
    /// to `logic:gtsSegmentHeadKey` — the object-level reasoning EDB carries the key on the carrier,
    /// not on an `owl:hasKey` triple, exactly as the shipped gts slice now authors it.
    #[test]
    fn gts_segment_head_key_carrier_decides_and_clashes() {
        let seg_a = "https://blackcatinformatics.ca/gmeow/segA";
        let seg_b = "https://blackcatinformatics.ca/gmeow/segB";
        let gts_segment = "https://blackcatinformatics.ca/gmeow/GTSSegment";
        let gts_head_id = "https://blackcatinformatics.ca/gmeow/gtsHeadId";
        let key_rec = "https://blackcatinformatics.ca/logic/gtsSegmentHeadKey";
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let head = "blake3:9f2c";
        let store = dataset(vec![
            // logic:KeyAssertion carrier: a GTSSegment is keyed by its gtsHeadId.
            quad(key_rec, TYPE, LOGIC_KEY_ASSERTION),
            quad(key_rec, LOGIC_KEY_CLASS, gts_segment),
            quad(key_rec, LOGIC_KEY_PROPERTY, gts_head_id),
            // Two distinct segments sharing one content-id head — a full-history BLAKE3 collision.
            quad(seg_a, TYPE, gts_segment),
            quad(seg_b, TYPE, gts_segment),
            literal_quad(seg_a, gts_head_id, head, str_ty),
            literal_quad(seg_b, gts_head_id, head, str_ty),
            quad(seg_a, OWL_DIFFERENT_FROM, seg_b),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "two differentFrom segments sharing a gtsHeadId key clash under the carrier"
        );
        assert!(
            verdict.coverage.present.contains(&"hasKey".to_owned()),
            "the logic:KeyAssertion carrier makes the hasKey family present"
        );
        assert!(
            verdict.coverage.decided.contains(&"hasKey".to_owned()),
            "the carrier keeps hasKey decided after the owl:hasKey source is migrated"
        );
        assert!(
            verdict.gaps.is_empty(),
            "hasKey is decided from the carrier, not a gap"
        );

        // Consistency guard: WITHOUT the owl:differentFrom, the shared key merely merges the two
        // (owl:sameAs, no unique-name assumption) — consistent, yet still decided from the carrier.
        let store_ok = dataset(vec![
            quad(key_rec, TYPE, LOGIC_KEY_ASSERTION),
            quad(key_rec, LOGIC_KEY_CLASS, gts_segment),
            quad(key_rec, LOGIC_KEY_PROPERTY, gts_head_id),
            quad(seg_a, TYPE, gts_segment),
            quad(seg_b, TYPE, gts_segment),
            literal_quad(seg_a, gts_head_id, head, str_ty),
            literal_quad(seg_b, gts_head_id, head, str_ty),
        ]);
        let ok = dl_consistency(store_ok.as_ref()).expect("dl consistency should succeed");
        assert!(
            ok.consistent,
            "without owl:differentFrom the shared gtsHeadId key merges the segments — consistent"
        );
        assert!(
            ok.coverage.decided.contains(&"hasKey".to_owned()),
            "hasKey stays decided from the carrier even with no clash"
        );
    }

    // ── owl:Thing forced empty / constrained ──────────────────────────────────

    /// owl:Thing ≡ owl:Nothing (or ⊑) makes the always-populated top class empty
    /// — inconsistent.
    #[test]
    fn thing_equivalent_to_nothing_is_inconsistent() {
        let store = dataset(vec![quad(
            OWL_THING,
            super::OWL_EQUIVALENT_CLASS,
            OWL_NOTHING,
        )]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !verdict.consistent,
            "owl:Thing forced empty must be inconsistent"
        );
        assert!(verdict.gaps.is_empty(), "Thing≡Nothing is decided");
    }

    /// owl:Thing oneOf {s} is the DL/Full-divergent singleton-universe case. The
    /// native path does NOT perform the universe-cardinality argument, so it must
    /// stay an HONEST gap (incomplete) — NEVER a wrong `consistent` decided answer.
    #[test]
    fn one_of_on_thing_is_an_honest_gap_not_a_decided_answer() {
        let s = "http://gmeow.example/s";
        let list = "onelist";
        let store = dataset(vec![
            bnode_quad(list, FIRST, s),
            bnode_quad(list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(OWL_THING),
                super::OWL_ONE_OF,
                RdfTerm::blank_node(list),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        // Honesty over a wrong decided answer: oneOf-on-Thing is undecided, so it
        // surfaces as a gap and the case grades DlGap, never CorpusOnly.
        assert!(
            !verdict.gaps.is_empty(),
            "oneOf-on-owl:Thing must surface as an honest gap"
        );
        assert!(
            verdict.coverage.unsupported.contains(&"oneOf".to_owned()),
            "oneOf is undecided when it constrains owl:Thing: {:?}",
            verdict.coverage
        );
    }

    /// A normal `owl:oneOf` on an ordinary class (not owl:Thing) stays DECIDED —
    /// the Thing carve-out must not regress the general enumeration handling.
    #[test]
    fn one_of_on_ordinary_class_stays_decided() {
        let list = "onelist";
        let store = dataset(vec![
            bnode_quad(list, FIRST, X),
            bnode_quad(list, REST, NIL),
            RdfQuad::new(
                RdfTerm::iri(A),
                super::OWL_ONE_OF,
                RdfTerm::blank_node(list),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            verdict.coverage.decided.contains(&"oneOf".to_owned()),
            "ordinary oneOf stays decided: {:?}",
            verdict.coverage
        );
    }

    // ── Out-of-fragment soundness: honest cannot-decide, never a wrong consistent ──

    /// `owl:topObjectProperty` (the universal property) is not implemented by the
    /// native chase: an ontology whose (in)consistency turns on the universal
    /// property obligation must be reported as an honest cannot-decide (non-empty
    /// `gaps`, the construct `unsupported`), NEVER a wrong `consistent` by ignoring
    /// the axiom.
    #[test]
    fn top_object_property_is_an_honest_gap_never_a_wrong_consistent() {
        const TOP: &str = super::OWL_TOP_OBJECT_PROPERTY;
        let store = dataset(vec![quad(X, TOP, Y), quad(A, SUBCLASS, B)]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.gaps.is_empty(),
            "the universal property is out of fragment ⇒ honest gap, not a silent ignore: {:?}",
            verdict.coverage
        );
        assert!(
            verdict
                .coverage
                .unsupported
                .contains(&"topObjectProperty".to_owned()),
            "owl:topObjectProperty is unsupported (never decided): {:?}",
            verdict.coverage
        );
        assert!(
            !verdict
                .coverage
                .decided
                .contains(&"topObjectProperty".to_owned()),
            "owl:topObjectProperty must NOT be reported decided"
        );
    }

    /// A datatype facet restriction that WOULD hide an inconsistency — `p`'s range
    /// is restricted to `xsd:integer[<= 5]` while `x` carries the value `10` on `p`
    /// — is invisible to the native chase (no datatype-value reasoning). Because a
    /// literal is ACTUALLY subject to the facet-restricted datatype, its presence
    /// must force an honest cannot-decide (non-empty `gaps`) rather than a wrong
    /// `consistent`. (Falsifiable case (b): a facet WITH a constrained literal is
    /// withheld.)
    #[test]
    fn datatype_facet_restriction_with_constrained_literal_is_an_honest_gap() {
        const WITH_RESTRICTIONS: &str = super::OWL_WITH_RESTRICTIONS;
        const ON_DATATYPE: &str = super::OWL_ON_DATATYPE;
        const MAX_INCLUSIVE: &str = super::XSD_MAX_INCLUSIVE;
        const RANGE: &str = super::RDFS_RANGE;
        let dt = "http://gmeow.example/SmallInt";
        let facet = "http://gmeow.example/facet0";
        let store = dataset(vec![
            // dt = xsd:integer restricted by [ xsd:maxInclusive "5" ]
            quad(dt, ON_DATATYPE, super::XSD_INTEGER),
            quad(dt, WITH_RESTRICTIONS, facet),
            literal_quad(facet, MAX_INCLUSIVE, "5", super::XSD_INTEGER),
            // p ranges into the facet-restricted datatype, and x carries the
            // out-of-range value 10 on p — a LIVE obligation the native path
            // cannot validate.
            quad(P, RANGE, dt),
            literal_quad(X, P, "10", super::XSD_INTEGER),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.gaps.is_empty(),
            "a constrained literal under a facet restriction is out of fragment ⇒ honest gap: {:?}",
            verdict.coverage
        );
        for family in ["onDatatype", "withRestrictions", "maxInclusive"] {
            assert!(
                verdict.coverage.unsupported.contains(&family.to_owned()),
                "{family} must be unsupported when a literal is constrained: {:?}",
                verdict.coverage
            );
        }
    }

    /// A facet-restricted datatype that is merely DEFINED but constrains no
    /// asserted/inferred literal is INERT — it cannot cause an inconsistency, so
    /// the native path DECIDES it: the datatype-facet families do NOT appear in the
    /// gaps, and the verdict stays decided consistent. This mirrors the production
    /// `gmeow:bic` datatype (a TBox-only facet definition with no ABox literal
    /// subject to it). (Falsifiable case (a): a facet WITHOUT a constrained literal
    /// is decided.)
    #[test]
    fn datatype_facet_definition_without_constrained_literal_is_decided_inert() {
        const WITH_RESTRICTIONS: &str = super::OWL_WITH_RESTRICTIONS;
        const ON_DATATYPE: &str = super::OWL_ON_DATATYPE;
        const MIN_LENGTH: &str = super::XSD_MIN_LENGTH;
        const ALL_VALUES_FROM: &str = super::OWL_ALL_VALUES_FROM;
        const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
        let dt = "dt"; // blank-node facet datatype (as in the production bundle)
        let facet = "facet0";
        let restriction = "http://gmeow.example/BicClass";
        let store = dataset(vec![
            // A facet-restricted datatype: onDatatype xsd:string, minLength 8.
            RdfQuad::new(
                RdfTerm::blank_node(dt),
                ON_DATATYPE,
                RdfTerm::iri("http://www.w3.org/2001/XMLSchema#string"),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(
                RdfTerm::blank_node(dt),
                WITH_RESTRICTIONS,
                RdfTerm::blank_node(facet),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(
                RdfTerm::blank_node(facet),
                MIN_LENGTH,
                RdfTerm::Literal(RdfLiteral::typed("8", super::XSD_NON_NEGATIVE_INTEGER)),
            )
            .in_graph(RdfTerm::iri(W)),
            // A class restricts property p's values to the facet datatype — but NO
            // individual asserts any literal on p, so the facet is inert.
            quad(restriction, ON_PROPERTY, P),
            RdfQuad::new(
                RdfTerm::iri(restriction),
                ALL_VALUES_FROM,
                RdfTerm::blank_node(dt),
            )
            .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            verdict.consistent,
            "an inert (defined-but-unused) facet datatype is consistent: {:?}",
            verdict.inconsistencies
        );
        assert!(
            verdict.gaps.is_empty(),
            "an inert facet definition must NOT surface as a gap: {:?}",
            verdict.gaps
        );
        for family in ["onDatatype", "withRestrictions", "minLength"] {
            assert!(
                !verdict.coverage.unsupported.contains(&family.to_owned()),
                "{family} must NOT be unsupported for an inert facet: {:?}",
                verdict.coverage
            );
            assert!(
                verdict.coverage.decided.contains(&family.to_owned()),
                "{family} must be decided (inert) for an unused facet: {:?}",
                verdict.coverage
            );
        }
    }

    /// A facet-restricted datatype used as the filler of an `owl:someValuesFrom`
    /// existential (`∃p.D`) is a LIVE obligation even with NO asserted literal: the
    /// existential forces a datatype value to exist, and native cannot decide the
    /// datatype's (non)emptiness (the W3C `Datatype-Float-Discrete-001` shape — the
    /// discrete `xsd:float` range `(0.0, 1.4e-45)` is empty). It must stay withheld
    /// (facet families NOT decided ⇒ honest gap), never a wrong `consistent`.
    #[test]
    fn datatype_facet_somevaluesfrom_existential_is_withheld_without_a_literal() {
        const WITH_RESTRICTIONS: &str = super::OWL_WITH_RESTRICTIONS;
        const ON_DATATYPE: &str = super::OWL_ON_DATATYPE;
        const MIN_EXCLUSIVE: &str = super::XSD_MIN_EXCLUSIVE;
        const SOME_VALUES_FROM: &str = super::OWL_SOME_VALUES_FROM;
        const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
        let dt = "dt";
        let facet = "facet0";
        let restriction = "restriction";
        let store = dataset(vec![
            // dt = xsd:float restricted by [ xsd:minExclusive "0.0" ] (a facet datatype)
            RdfQuad::new(
                RdfTerm::blank_node(dt),
                ON_DATATYPE,
                RdfTerm::iri("http://www.w3.org/2001/XMLSchema#float"),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(
                RdfTerm::blank_node(dt),
                WITH_RESTRICTIONS,
                RdfTerm::blank_node(facet),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(
                RdfTerm::blank_node(facet),
                MIN_EXCLUSIVE,
                RdfTerm::Literal(RdfLiteral::typed(
                    "0.0",
                    "http://www.w3.org/2001/XMLSchema#float",
                )),
            )
            .in_graph(RdfTerm::iri(W)),
            // a rdf:type [ ∃dp.dt ] — the existential obligation, no asserted literal.
            quad(restriction, ON_PROPERTY, P),
            RdfQuad::new(
                RdfTerm::blank_node(restriction),
                SOME_VALUES_FROM,
                RdfTerm::blank_node(dt),
            )
            .in_graph(RdfTerm::iri(W)),
            RdfQuad::new(RdfTerm::iri(X), TYPE, RdfTerm::blank_node(restriction))
                .in_graph(RdfTerm::iri(W)),
        ]);
        let verdict = dl_consistency(store.as_ref()).expect("dl consistency should succeed");

        assert!(
            !verdict.gaps.is_empty(),
            "an existential into a facet datatype is a live obligation ⇒ honest gap: {:?}",
            verdict.coverage
        );
        for family in ["onDatatype", "withRestrictions", "minExclusive"] {
            assert!(
                verdict.coverage.unsupported.contains(&family.to_owned()),
                "{family} must be unsupported for an existential into a facet datatype: {:?}",
                verdict.coverage
            );
        }
    }

    /// The typed `ReasoningResult` fold: an out-of-fragment bundle with no derived
    /// contradiction is `information=undetermined` (honest cannot-decide) — it is
    /// NOT `is_decided_consistent()`, and its completeness drops to `Incomplete`.
    /// This pins the API-level withholding of the positive consistency verdict.
    #[test]
    fn out_of_fragment_reasoning_result_is_undetermined_not_decided_consistent() {
        use crate::result::InformationState;
        const TOP: &str = super::OWL_TOP_OBJECT_PROPERTY;
        let store = dataset(vec![quad(X, TOP, Y), quad(A, SUBCLASS, B)]);
        let result =
            crate::reason::reason_all(store.as_ref()).expect("native reason_all must decide");

        assert_eq!(
            result.information,
            InformationState::Undetermined,
            "out-of-fragment consistency is undetermined, never a positive verdict"
        );
        assert!(
            !result.is_decided_consistent(),
            "cannot-decide is NOT a decided-consistent verdict"
        );
        assert!(
            !result.preservation.unsupported_constructs.is_empty(),
            "the undecided construct is disclosed in the preservation set"
        );
    }

    // ── Wave A: property-characteristic + disjointness/identity clash families ──

    fn bn_iri(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }
    fn bn_bn(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::blank_node(s), p, RdfTerm::blank_node(o)).in_graph(RdfTerm::iri(W))
    }
    /// A three-member RDF list `[a b c]` rooted at blank node `root`.
    fn list3(root: &str, a: &str, b: &str, c: &str) -> Vec<RdfQuad> {
        const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
        let n1 = root.to_string();
        let n2 = format!("{root}-2");
        let n3 = format!("{root}-3");
        vec![
            bn_iri(&n1, RDF_FIRST, a),
            bn_bn(&n1, RDF_REST, &n2),
            bn_iri(&n2, RDF_FIRST, b),
            bn_bn(&n2, RDF_REST, &n3),
            bn_iri(&n3, RDF_FIRST, c),
            bn_iri(&n3, RDF_REST, RDF_NIL),
        ]
    }

    const P1: &str = "http://gmeow.example/p1";
    const P2: &str = "http://gmeow.example/p2";
    const P3: &str = "http://gmeow.example/p3";
    const O: &str = "http://gmeow.example/o";

    #[test]
    fn asymmetric_property_cycle_is_inconsistent() {
        // p AsymmetricProperty, x p y, y p x ⇒ Nothing(x).
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_ASYMMETRIC_PROPERTY),
            quad(X, P, Y),
            quad(Y, P, X),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "asymmetric-property cycle clashes: {:?}",
            v.inconsistencies
        );
        assert!(v.gaps.is_empty(), "no gap: {:?}", v.gaps);
        assert!(
            v.coverage
                .decided
                .contains(&"asymmetricProperty".to_owned())
        );
    }

    #[test]
    fn asymmetric_property_without_cycle_is_consistent() {
        // Falsifiable: a single directed edge on an asymmetric property is fine.
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_ASYMMETRIC_PROPERTY),
            quad(X, P, Y),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            v.consistent,
            "no reverse edge ⇒ consistent: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn symmetric_plus_asymmetric_edge_is_inconsistent() {
        // p is BOTH Symmetric and Asymmetric (the OWL-2 `-term` shape). x p y ⇒
        // (symmetric) y p x ⇒ (asymmetric) Nothing(x).
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_SYMMETRIC_PROPERTY),
            quad(P, TYPE, super::OWL_ASYMMETRIC_PROPERTY),
            quad(X, P, Y),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "symmetric+asymmetric edge clashes: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn irreflexive_property_self_loop_is_inconsistent() {
        // p IrreflexiveProperty, x p x ⇒ Nothing(x).
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_IRREFLEXIVE_PROPERTY),
            quad(X, P, X),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "irreflexive self-loop clashes: {:?}",
            v.inconsistencies
        );
        assert!(
            v.coverage
                .decided
                .contains(&"irreflexiveProperty".to_owned())
        );
    }

    #[test]
    fn property_disjoint_with_shared_object_is_inconsistent() {
        // p1 propertyDisjointWith p2, s p1 o, s p2 o ⇒ Nothing(s).
        let store = dataset(vec![
            quad(P1, super::OWL_PROPERTY_DISJOINT_WITH, P2),
            quad(X, P1, O),
            quad(X, P2, O),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "disjoint properties sharing a value clash: {:?}",
            v.inconsistencies
        );
        assert!(
            v.coverage
                .decided
                .contains(&"propertyDisjointWith".to_owned())
        );
    }

    #[test]
    fn property_disjoint_with_shared_literal_is_inconsistent() {
        // Data-property disjointness is literal-aware: same literal VALUE on two
        // disjoint data properties clashes (the disjointdataproperties shape).
        let str_ty = "http://www.w3.org/2001/XMLSchema#string";
        let store = dataset(vec![
            quad(P1, super::OWL_PROPERTY_DISJOINT_WITH, P2),
            literal_quad(X, P1, "Peter Griffin", str_ty),
            literal_quad(X, P2, "Peter Griffin", str_ty),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "shared literal on disjoint data properties clashes: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn self_disjoint_property_with_a_value_is_inconsistent() {
        // p propertyDisjointWith p (irreflexive shape): any single asserted value
        // is a value on both `p` and `p` ⇒ clash.
        let store = dataset(vec![
            quad(P, super::OWL_PROPERTY_DISJOINT_WITH, P),
            quad(X, P, O),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "self-disjoint property with a value clashes: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn equivalent_disjoint_properties_clash_via_propagation() {
        // p1 ≡ p2 AND p1 disjointWith p2, s1 p1 o1, s2 p2 o2. Equivalence copies
        // each assertion onto the other property, so s1 carries p1(o1) and p2(o1)
        // ⇒ clash.
        let o1 = "http://gmeow.example/o1";
        let s1 = "http://gmeow.example/s1";
        let store = dataset(vec![
            quad(P1, super::OWL_EQUIVALENT_PROPERTY, P2),
            quad(P1, super::OWL_PROPERTY_DISJOINT_WITH, P2),
            quad(s1, P1, o1),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "equivalent+disjoint properties clash: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn all_disjoint_properties_expands_and_clashes() {
        // AllDisjointProperties [p1 p2 p3], s p1 o, s p2 o ⇒ Nothing(s).
        let mut quads = vec![
            quad(
                "http://gmeow.example/adp",
                TYPE,
                super::OWL_ALL_DISJOINT_PROPERTIES,
            ),
            RdfQuad::new(
                RdfTerm::iri("http://gmeow.example/adp"),
                super::OWL_MEMBERS,
                RdfTerm::blank_node("adplist"),
            )
            .in_graph(RdfTerm::iri(W)),
            quad(X, P1, O),
            quad(X, P2, O),
        ];
        quads.extend(list3("adplist", P1, P2, P3));
        let store = dataset(quads);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "AllDisjointProperties collision clashes: {:?}",
            v.inconsistencies
        );
        assert!(
            v.coverage
                .decided
                .contains(&"allDisjointProperties".to_owned())
        );
    }

    #[test]
    fn same_as_and_different_from_is_inconsistent() {
        // x sameAs y AND x differentFrom y ⇒ Nothing(x).
        let store = dataset(vec![
            quad(X, super::OWL_SAME_AS, Y),
            quad(X, super::OWL_DIFFERENT_FROM, Y),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "sameAs ⊓ differentFrom clashes: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn reflexive_different_from_is_inconsistent() {
        // x differentFrom x ⇒ Nothing(x) (everything is sameAs itself).
        let store = dataset(vec![quad(X, super::OWL_DIFFERENT_FROM, X)]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "reflexive differentFrom clashes: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn all_different_with_sameas_member_is_inconsistent() {
        // AllDifferent [w1 w2 w3] with w1 sameAs w2 ⇒ Nothing (the expanded
        // differentFrom(w1,w2) contradicts sameAs).
        let w1 = "http://gmeow.example/w1";
        let w2 = "http://gmeow.example/w2";
        let w3 = "http://gmeow.example/w3";
        let mut quads = vec![
            quad("http://gmeow.example/ad", TYPE, super::OWL_ALL_DIFFERENT),
            RdfQuad::new(
                RdfTerm::iri("http://gmeow.example/ad"),
                super::OWL_MEMBERS,
                RdfTerm::blank_node("adlist"),
            )
            .in_graph(RdfTerm::iri(W)),
            quad(w1, super::OWL_SAME_AS, w2),
        ];
        quads.extend(list3("adlist", w1, w2, w3));
        let store = dataset(quads);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "AllDifferent with a sameAs member clashes: {:?}",
            v.inconsistencies
        );
        assert!(v.coverage.decided.contains(&"allDifferent".to_owned()));
    }

    #[test]
    fn all_disjoint_classes_expands_and_clashes() {
        // AllDisjointClasses [c1 c2 c3], w a c1, w a c2 ⇒ Nothing(w) (via the
        // expanded disjointWith + existing individual-clash).
        let c1 = "http://gmeow.example/c1";
        let c2 = "http://gmeow.example/c2";
        let c3 = "http://gmeow.example/c3";
        let w = "http://gmeow.example/w-ind";
        let mut quads = vec![
            quad(
                "http://gmeow.example/adc",
                TYPE,
                super::OWL_ALL_DISJOINT_CLASSES,
            ),
            RdfQuad::new(
                RdfTerm::iri("http://gmeow.example/adc"),
                super::OWL_MEMBERS,
                RdfTerm::blank_node("adclist"),
            )
            .in_graph(RdfTerm::iri(W)),
            quad(w, TYPE, c1),
            quad(w, TYPE, c2),
        ];
        quads.extend(list3("adclist", c1, c2, c3));
        let store = dataset(quads);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "AllDisjointClasses membership clashes: {:?}",
            v.inconsistencies
        );
        assert!(
            v.coverage
                .decided
                .contains(&"allDisjointClasses".to_owned())
        );
    }

    #[test]
    fn negative_property_assertion_by_shape_without_type_is_inconsistent() {
        // The OWL-2 `-fw` NPA shape: the NPA node carries source/property/target
        // but NO `rdf:type owl:NegativePropertyAssertion`; native infers NPA-hood
        // structurally and clashes it against the positive assertion.
        let s = "http://gmeow.example/s";
        let store = dataset(vec![
            bn_iri("npa", super::OWL_SOURCE_INDIVIDUAL, s),
            bn_iri("npa", super::OWL_ASSERTION_PROPERTY, P),
            bn_iri("npa", super::OWL_TARGET_INDIVIDUAL, O),
            quad(s, P, O),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "structural NPA clashes its positive: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn functional_property_two_named_fillers_no_differentfrom_is_consistent() {
        // SOUNDNESS FLOOR (no UNA): a functional property with two merely-named
        // fillers and NO owl:differentFrom is CONSISTENT — they may be owl:sameAs.
        // (The old UNA default reported a FALSE inconsistency here: the OWL-2
        // char-functional-inst / WebOnt-FunctionalProperty-00{1,2} regression.)
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_FUNCTIONAL_PROPERTY),
            quad(X, P, Y),
            quad(X, P, Z),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            v.consistent,
            "two named functional-property fillers without differentFrom must NOT clash: {:?}",
            v.inconsistencies
        );
        assert!(v.gaps.is_empty(), "no gap: {:?}", v.gaps);
    }

    #[test]
    fn functional_property_two_provably_distinct_fillers_clash() {
        // With explicit owl:differentFrom the two fillers ARE provably distinct ⇒
        // the functional property clashes (soundly).
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_FUNCTIONAL_PROPERTY),
            quad(X, P, Y),
            quad(X, P, Z),
            quad(Y, super::OWL_DIFFERENT_FROM, Z),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            !v.consistent,
            "differentFrom fillers on a functional property clash: {:?}",
            v.inconsistencies
        );
    }

    #[test]
    fn functional_property_distinct_xml_literals_are_withheld() {
        // Two lexically-distinct rdf:XMLLiteral values on a functional property:
        // the chase cannot canonicalize XML, so the family is honestly WITHHELD (a
        // gap), never a wrong clash. (OWL-2 WebOnt-miscellaneous-202.)
        let store = dataset(vec![
            quad(P, TYPE, super::OWL_FUNCTIONAL_PROPERTY),
            literal_quad(X, P, "<a></a>", super::RDF_XML_LITERAL),
            literal_quad(X, P, "<a/>", super::RDF_XML_LITERAL),
        ]);
        let v = dl_consistency(store.as_ref()).expect("dl consistency should succeed");
        assert!(
            v.coverage
                .unsupported
                .contains(&"functionalProperty".to_owned()),
            "unresolvable XMLLiteral functional values are withheld: {:?}",
            v.coverage
        );
        assert!(
            !v.gaps.is_empty(),
            "the withheld XMLLiteral shape surfaces as a gap: {:?}",
            v.gaps
        );
    }

    // The refutation-kernel materialization seam: an `InFragment{Inconsistent}`
    // certificate materializes its `type(?i, owl:Nothing)` clash witness into the
    // closure, which `verdict_from_inferred` then reads off as an inconsistency —
    // while an `OutOfFragment` withhold (the Task-2 production steady state) and an
    // `InFragment{Consistent}` decision materialize nothing. This exercises the
    // decide-path seam the per-family deciders (Tasks 3/4/5) plug into.
    #[test]
    fn materialize_refutation_seam_forces_owl_nothing_only_on_inconsistent() {
        use crate::reason::refute::{
            Decision, FragmentBoundary, FragmentFamily, NothingClash, RefutationCertificate,
            Witness, WitnessEvidence,
        };

        let inconsistent = RefutationCertificate::InFragment {
            decision: Decision::Inconsistent,
            witness: Witness {
                family: FragmentFamily::Counting,
                clashes: [NothingClash {
                    individual: "http://ex/i".to_owned(),
                    world: String::new(),
                    rule_name: "refute:counting".to_owned(),
                    premises: vec![(
                        "http://ex/i".to_owned(),
                        RDF_TYPE.to_owned(),
                        "http://ex/A".to_owned(),
                    )],
                }]
                .into_iter()
                .collect(),
                evidence: WitnessEvidence::default(),
            },
        };
        let mut inferred: Vec<InferredAxiom> = Vec::new();
        let mut facts: BTreeSet<Fact> = BTreeSet::new();
        assert!(
            materialize_refutation(&inconsistent, &mut inferred, &mut facts),
            "an inconsistent certificate materializes its clash"
        );
        let store = dataset(Vec::new());
        let verdict = verdict_from_inferred(&inferred, store.as_ref()).expect("verdict");
        assert!(
            !verdict.consistent,
            "the materialized owl:Nothing witness makes the closure inconsistent"
        );
        assert_eq!(verdict.inconsistencies.len(), 1);
        assert_eq!(verdict.inconsistencies[0].individual, "http://ex/i");

        // A consistent decision and an out-of-fragment withhold are both no-ops.
        for benign in [
            RefutationCertificate::InFragment {
                decision: Decision::Consistent,
                witness: Witness {
                    family: FragmentFamily::Counting,
                    clashes: BTreeSet::new(),
                    evidence: WitnessEvidence::default(),
                },
            },
            RefutationCertificate::OutOfFragment {
                reason: FragmentBoundary::NoDeciderEngaged,
            },
        ] {
            let mut inferred2: Vec<InferredAxiom> = Vec::new();
            let mut facts2: BTreeSet<Fact> = BTreeSet::new();
            assert!(
                !materialize_refutation(&benign, &mut inferred2, &mut facts2),
                "a consistent decision / withhold materializes nothing: {benign:?}"
            );
            assert!(inferred2.is_empty(), "no witness axiom added: {benign:?}");
        }
    }
}
