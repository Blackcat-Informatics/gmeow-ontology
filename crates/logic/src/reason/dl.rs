// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DL diagnostics are projections of one native execution. This module does not
//! read an RDF dataset, mint facts or dispatch another refutation evaluation.
//! Empty classes, supported local clashes, contextual conflicts, source refusal
//! and incomplete execution remain independent observations.

use crate::physical::{LogicalGraph, WitnessStatement};
use crate::reason::InferredAxiom;
use crate::reason::refute::{
    NativeFamilyCompletion, NativeFamilyLedger, NativeProofId, NativeRefutationFamily,
};
use purrdf::TermValue;
use std::collections::{BTreeMap, BTreeSet};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

fn invalid(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

/// The native construct inventory interpreted only in its declared operator role.
/// Presence never supplies model completion or rewrite authority.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum DlConstructFamily {
    /// The `complementOf` source construct.
    ComplementOf,
    /// The `someValuesFrom` source construct.
    SomeValuesFrom,
    /// The `allValuesFrom` source construct.
    AllValuesFrom,
    /// The `cardinality` source construct.
    Cardinality,
    /// The `minCardinality` source construct.
    MinCardinality,
    /// The `maxCardinality` source construct.
    MaxCardinality,
    /// The `qualifiedCardinality` source construct.
    QualifiedCardinality,
    /// The `minQualifiedCardinality` source construct.
    MinQualifiedCardinality,
    /// The `maxQualifiedCardinality` source construct.
    MaxQualifiedCardinality,
    /// The `disjointUnionOf` source construct.
    DisjointUnionOf,
    /// The `unionOf` source construct.
    UnionOf,
    /// The `oneOf` source construct.
    OneOf,
    /// The `hasValue` source construct.
    HasValue,
    /// The `domain` source construct.
    Domain,
    /// The `range` source construct.
    Range,
    /// The `propertyChainAxiom` source construct.
    PropertyChainAxiom,
    /// The `bottomObjectProperty` source construct.
    BottomObjectProperty,
    /// The `bottomDataProperty` source construct.
    BottomDataProperty,
    /// The `hasKey` source construct.
    HasKey,
    /// The `negativePropertyAssertion` source construct.
    NegativePropertyAssertion,
    /// The `functionalProperty` source construct.
    FunctionalProperty,
    /// The `asymmetricProperty` source construct.
    AsymmetricProperty,
    /// The `irreflexiveProperty` source construct.
    IrreflexiveProperty,
    /// The `propertyDisjointWith` source construct.
    PropertyDisjointWith,
    /// The `allDisjointProperties` source construct.
    AllDisjointProperties,
    /// The `allDisjointClasses` source construct.
    AllDisjointClasses,
    /// The `allDifferent` source construct.
    AllDifferent,
    /// The `inverseFunctionalProperty` source construct.
    InverseFunctionalProperty,
    /// The `datatypeComplementOf` source construct.
    DatatypeComplementOf,
    /// The `withRestrictions` source construct.
    WithRestrictions,
    /// The `onDatatype` source construct.
    OnDatatype,
    /// The `minInclusive` source construct.
    MinInclusive,
    /// The `maxInclusive` source construct.
    MaxInclusive,
    /// The `minExclusive` source construct.
    MinExclusive,
    /// The `maxExclusive` source construct.
    MaxExclusive,
    /// The `pattern` source construct.
    Pattern,
    /// The `length` source construct.
    Length,
    /// The `minLength` source construct.
    MinLength,
    /// The `maxLength` source construct.
    MaxLength,
    /// The `totalDigits` source construct.
    TotalDigits,
    /// The `fractionDigits` source construct.
    FractionDigits,
    /// The `langRange` source construct.
    LangRange,
    /// The `topObjectProperty` source construct.
    TopObjectProperty,
    /// The `topDataProperty` source construct.
    TopDataProperty,
    /// The `hasSelf` source construct.
    HasSelf,
    /// The `intersectionOf` source construct.
    IntersectionOf,
    /// The datatype whitespace facet source construct.
    WhiteSpace,
    /// The datatype timezone facet source construct.
    ExplicitTimezone,
}

impl DlConstructFamily {
    /// Stable coverage diagnostic name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ComplementOf => "complementOf",
            Self::SomeValuesFrom => "someValuesFrom",
            Self::AllValuesFrom => "allValuesFrom",
            Self::Cardinality => "cardinality",
            Self::MinCardinality => "minCardinality",
            Self::MaxCardinality => "maxCardinality",
            Self::QualifiedCardinality => "qualifiedCardinality",
            Self::MinQualifiedCardinality => "minQualifiedCardinality",
            Self::MaxQualifiedCardinality => "maxQualifiedCardinality",
            Self::DisjointUnionOf => "disjointUnionOf",
            Self::UnionOf => "unionOf",
            Self::OneOf => "oneOf",
            Self::HasValue => "hasValue",
            Self::Domain => "domain",
            Self::Range => "range",
            Self::PropertyChainAxiom => "propertyChainAxiom",
            Self::BottomObjectProperty => "bottomObjectProperty",
            Self::BottomDataProperty => "bottomDataProperty",
            Self::HasKey => "hasKey",
            Self::NegativePropertyAssertion => "negativePropertyAssertion",
            Self::FunctionalProperty => "functionalProperty",
            Self::AsymmetricProperty => "asymmetricProperty",
            Self::IrreflexiveProperty => "irreflexiveProperty",
            Self::PropertyDisjointWith => "propertyDisjointWith",
            Self::AllDisjointProperties => "allDisjointProperties",
            Self::AllDisjointClasses => "allDisjointClasses",
            Self::AllDifferent => "allDifferent",
            Self::InverseFunctionalProperty => "inverseFunctionalProperty",
            Self::DatatypeComplementOf => "datatypeComplementOf",
            Self::WithRestrictions => "withRestrictions",
            Self::OnDatatype => "onDatatype",
            Self::MinInclusive => "minInclusive",
            Self::MaxInclusive => "maxInclusive",
            Self::MinExclusive => "minExclusive",
            Self::MaxExclusive => "maxExclusive",
            Self::Pattern => "pattern",
            Self::Length => "length",
            Self::MinLength => "minLength",
            Self::MaxLength => "maxLength",
            Self::TotalDigits => "totalDigits",
            Self::FractionDigits => "fractionDigits",
            Self::LangRange => "langRange",
            Self::TopObjectProperty => "topObjectProperty",
            Self::TopDataProperty => "topDataProperty",
            Self::HasSelf => "hasSelf",
            Self::IntersectionOf => "intersectionOf",
            Self::WhiteSpace => "whiteSpace",
            Self::ExplicitTimezone => "explicitTimezone",
        }
    }
}

#[derive(Clone, Copy)]
enum ConstructRole {
    Predicate,
    Marker,
    Property,
}

const CONSTRUCTS: &[(DlConstructFamily, &str, ConstructRole)] = &[
    (
        DlConstructFamily::WhiteSpace,
        "http://www.w3.org/2001/XMLSchema#whiteSpace",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::ExplicitTimezone,
        "http://www.w3.org/2001/XMLSchema#explicitTimezone",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::ComplementOf,
        "http://www.w3.org/2002/07/owl#complementOf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::SomeValuesFrom,
        "http://www.w3.org/2002/07/owl#someValuesFrom",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::AllValuesFrom,
        "http://www.w3.org/2002/07/owl#allValuesFrom",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::Cardinality,
        "http://www.w3.org/2002/07/owl#cardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MinCardinality,
        "http://www.w3.org/2002/07/owl#minCardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MaxCardinality,
        "http://www.w3.org/2002/07/owl#maxCardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::QualifiedCardinality,
        "http://www.w3.org/2002/07/owl#qualifiedCardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MinQualifiedCardinality,
        "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MaxQualifiedCardinality,
        "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::DisjointUnionOf,
        "http://www.w3.org/2002/07/owl#disjointUnionOf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::UnionOf,
        "http://www.w3.org/2002/07/owl#unionOf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::OneOf,
        "http://www.w3.org/2002/07/owl#oneOf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::HasValue,
        "http://www.w3.org/2002/07/owl#hasValue",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::Domain,
        "http://www.w3.org/2000/01/rdf-schema#domain",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::Range,
        "http://www.w3.org/2000/01/rdf-schema#range",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::PropertyChainAxiom,
        "http://www.w3.org/2002/07/owl#propertyChainAxiom",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::BottomObjectProperty,
        "http://www.w3.org/2002/07/owl#bottomObjectProperty",
        ConstructRole::Property,
    ),
    (
        DlConstructFamily::BottomDataProperty,
        "http://www.w3.org/2002/07/owl#bottomDataProperty",
        ConstructRole::Property,
    ),
    (
        DlConstructFamily::HasKey,
        "http://www.w3.org/2002/07/owl#hasKey",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::NegativePropertyAssertion,
        "http://www.w3.org/2002/07/owl#NegativePropertyAssertion",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::FunctionalProperty,
        "http://www.w3.org/2002/07/owl#FunctionalProperty",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::AsymmetricProperty,
        "http://www.w3.org/2002/07/owl#AsymmetricProperty",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::IrreflexiveProperty,
        "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::PropertyDisjointWith,
        "http://www.w3.org/2002/07/owl#propertyDisjointWith",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::AllDisjointProperties,
        "http://www.w3.org/2002/07/owl#AllDisjointProperties",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::AllDisjointClasses,
        "http://www.w3.org/2002/07/owl#AllDisjointClasses",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::AllDifferent,
        "http://www.w3.org/2002/07/owl#AllDifferent",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::InverseFunctionalProperty,
        "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
        ConstructRole::Marker,
    ),
    (
        DlConstructFamily::DatatypeComplementOf,
        "http://www.w3.org/2002/07/owl#datatypeComplementOf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::WithRestrictions,
        "http://www.w3.org/2002/07/owl#withRestrictions",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::OnDatatype,
        "http://www.w3.org/2002/07/owl#onDatatype",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MinInclusive,
        "http://www.w3.org/2001/XMLSchema#minInclusive",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MaxInclusive,
        "http://www.w3.org/2001/XMLSchema#maxInclusive",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MinExclusive,
        "http://www.w3.org/2001/XMLSchema#minExclusive",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MaxExclusive,
        "http://www.w3.org/2001/XMLSchema#maxExclusive",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::Pattern,
        "http://www.w3.org/2001/XMLSchema#pattern",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::Length,
        "http://www.w3.org/2001/XMLSchema#length",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MinLength,
        "http://www.w3.org/2001/XMLSchema#minLength",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::MaxLength,
        "http://www.w3.org/2001/XMLSchema#maxLength",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::TotalDigits,
        "http://www.w3.org/2001/XMLSchema#totalDigits",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::FractionDigits,
        "http://www.w3.org/2001/XMLSchema#fractionDigits",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::LangRange,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#langRange",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::TopObjectProperty,
        "http://www.w3.org/2002/07/owl#topObjectProperty",
        ConstructRole::Property,
    ),
    (
        DlConstructFamily::TopDataProperty,
        "http://www.w3.org/2002/07/owl#topDataProperty",
        ConstructRole::Property,
    ),
    (
        DlConstructFamily::HasSelf,
        "http://www.w3.org/2002/07/owl#hasSelf",
        ConstructRole::Predicate,
    ),
    (
        DlConstructFamily::IntersectionOf,
        "http://www.w3.org/2002/07/owl#intersectionOf",
        ConstructRole::Predicate,
    ),
];

/// Shared fixed native DL rules. The joint engine owns their execution.
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

/// A proved empty class, without an assertion that it is populated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnsatClass {
    pub class: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// An individual forced into `owl:Nothing`: a witness that the ontology is
/// inconsistent in `world`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InconsistencyWitness {
    pub individual: String,
    pub world: String,
    pub premises: Vec<(String, String, String)>,
}

/// Construct-level completion folded from every selected native world.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DlCoverage {
    pub present: Vec<String>,
    pub decided: Vec<String>,
    pub unsupported: Vec<String>,
}

/// One native DL coverage defect. This is reasoner-domain evidence, not an RDF
/// representation-conversion loss, so it is owned by GMEOW rather than PurRDF's
/// transcode [`purrdf::LossLedger`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

/// Diagnostic projection of the retained native result. `consistent` means no
/// supported local or contextual contradiction; gaps independently prevent a
/// completed consistency claim. Empty unpopulated classes are not contradictions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DlVerdict {
    pub consistent: bool,
    pub unsatisfiable_classes: Vec<UnsatClass>,
    pub inconsistencies: Vec<InconsistencyWitness>,
    pub coverage: DlCoverage,
    pub gaps: Vec<DlGap>,
    pub boundary_findings: Vec<gmeow_errors::Finding>,
}

impl DlVerdict {
    /// Classify the consistency claim without discarding independent coverage gaps.
    /// A proved conflict remains `Both` even if another obligation is unsupported.
    /// Only a consistent verdict with no gap or unsupported construct is `Supported`;
    /// every other verdict is `Undetermined`.
    #[must_use]
    pub fn information_state(&self) -> crate::result::InformationState {
        use crate::result::InformationState;
        if !self.consistent {
            InformationState::Both
        } else if self.gaps.is_empty() && self.coverage.unsupported.is_empty() {
            InformationState::Supported
        } else {
            InformationState::Undetermined
        }
    }
}

fn unwrap_iri(display: &str) -> &str {
    display
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(display)
}

/// Project proved class emptiness from exact committed native rows.
pub fn unsatisfiable_from_inferred(inferred: &[InferredAxiom]) -> Vec<UnsatClass> {
    let mut unsatisfiable_classes: Vec<UnsatClass> = Vec::new();
    for ax in inferred {
        if ax
            .object
            .as_iri()
            .and_then(|iri| EmptyClassAssertion::classify(&ax.predicate, iri))
            == Some(EmptyClassAssertion::Subsumption)
            && !empty_class_marker(&ax.predicate, unwrap_iri(&ax.subject))
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

/// The two empty-class assertion roles consumed by native verdicts and their
/// explanation projections. Class emptiness and an individual clash are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyClassAssertion {
    /// An individual is a member of the empty class: a populated contradiction.
    Membership,
    /// A class is a subclass of the empty class: no population is implied.
    Subsumption,
}

impl EmptyClassAssertion {
    /// Interpret an asserted predicate and an IRI object under the declared
    /// grounding vocabulary. Ordinary data predicates never acquire a class role.
    pub fn classify(predicate: &str, object_iri: &str) -> Option<Self> {
        if !empty_class_marker(predicate, object_iri) {
            return None;
        }
        let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
        let operator = semantics.predicate(predicate);
        if operator == semantics.predicate(RDF_TYPE) {
            Some(Self::Membership)
        } else if operator == semantics.predicate(RDFS_SUBCLASSOF) {
            Some(Self::Subsumption)
        } else {
            None
        }
    }
}

/// Recognize the empty class only in an admitted class-marker position. This
/// interprets a declared spelling pair without rewriting an RDF term or premise.
fn empty_class_marker(predicate: &str, iri: &str) -> bool {
    iri == OWL_NOTHING
        || crate::native_semantics::SemanticVocabulary::GroundedLogicV1
            .alternate_marker(predicate, iri)
            == Some(OWL_NOTHING)
}

#[cfg(test)]
#[path = "dl_verdict_tests.rs"]
mod verdict_tests;

/// Convert retained unsupported family names to stable diagnostic gaps.
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

/// One selected native construct, linked to its actual assertion or derivation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeConstructOccurrence {
    /// The operator family selected in its semantic role.
    pub family: DlConstructFamily,
    /// Exact committed native statement, without vocabulary rewriting.
    pub statement: WitnessStatement,
    /// The shared proof node retaining original source or actual derivation.
    pub support: NativeProofId,
}

/// Construct occurrences retained during a world's native fact insertion pass.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceCoverageWorld {
    /// Original graph identity, independent of its execution key.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<TermValue>,
    /// Every selected original or committed construct occurrence.
    pub constructs: Vec<NativeConstructOccurrence>,
    /// Completed-grammar admission for every selected owner, from shared caches.
    pub admissions: Vec<NativeConstructAdmission>,
}

/// Eligibility of one source construct owner after its grammar writers finish.
/// This is source/capability evidence, never an inferred RDF contradiction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeConstructAdmission {
    /// The exact selected operator family.
    pub family: DlConstructFamily,
    /// Native owner identity shared by this family's selected field assertions.
    #[serde(with = "crate::term_serde")]
    pub owner: TermValue,
    /// All independently asserted or derived selections of this owner/family.
    pub selectors: Vec<NativeProofId>,
    /// Exact selected fields and available traversed list evidence.
    pub support: Vec<NativeProofId>,
    /// Actual completion of the required grammar and value checks.
    pub completion: NativeFamilyCompletion,
    /// Every observed refusal, independently of positive semantic conclusions.
    pub obstructions: Vec<super::refute::NativeFamilyObstruction>,
}

/// Source-role inventory observed by the single native execution, never by a
/// terminal dataset scanner. Empty selected graphs retain explicit entries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceCoverageObservation {
    /// Exact native input, selection and semantic recipe commitment.
    pub input_contract: [u8; 32],
    /// Every admitted native world, including worlds with no construct occurrence.
    pub worlds: BTreeMap<String, SourceCoverageWorld>,
}

impl SourceCoverageObservation {
    /// Validate scope and exact proof-backed roles of an authenticated observation.
    /// The source/action commitment authenticates completeness of the captured
    /// inventory; these intrinsic checks do not authenticate arbitrary input bytes.
    ///
    /// # Errors
    /// Refuses absent worlds, duplicate observations, foreign proof nodes and
    /// a vocabulary label attached to a statement outside its operator role.
    pub fn validate(
        &self,
        input_contract: [u8; 32],
        worlds: &BTreeMap<String, LogicalGraph>,
        ledgers: &[NativeFamilyLedger],
    ) -> gmeow_errors::Result<()> {
        if self.input_contract != input_contract || self.worlds.len() != worlds.len() {
            return Err(invalid(
                "native construct inventory changes the selected execution scope",
            ));
        }
        let mut indexed = BTreeMap::new();
        for ledger in ledgers {
            if indexed.insert(ledger.world.as_str(), ledger).is_some() {
                return Err(invalid(
                    "native construct inventory has duplicate world proof ledgers",
                ));
            }
        }
        for (world, observations) in &self.worlds {
            if worlds.get(world).map(LogicalGraph::graph) != Some(observations.graph.as_ref()) {
                return Err(invalid(
                    "native construct inventory changes original graph ownership",
                ));
            }
            let ledger = indexed
                .get(world.as_str())
                .ok_or_else(|| invalid("native construct world has no proof ledger"))?;
            if ledger.input_contract != input_contract || ledger.graph != observations.graph {
                return Err(invalid(
                    "native construct evidence belongs to a different input or graph",
                ));
            }
            let proofs: BTreeMap<_, _> = ledger
                .proofs
                .iter()
                .map(|proof| (proof.id, &proof.statement))
                .collect();
            if observations
                .admissions
                .windows(2)
                .any(|pair| (pair[0].family, &pair[0].owner) >= (pair[1].family, &pair[1].owner))
            {
                return Err(invalid(
                    "native source admissions repeat an owner or lose canonical lookup order",
                ));
            }
            let mut seen = BTreeSet::new();
            let mut expected =
                BTreeMap::<(DlConstructFamily, &TermValue), BTreeSet<NativeProofId>>::new();
            for occurrence in &observations.constructs {
                if proofs.get(&occurrence.support).copied() != Some(&occurrence.statement)
                    || !seen.insert((occurrence.family, occurrence.support))
                    || !construct_families(
                        &occurrence.statement.predicate,
                        &occurrence.statement.subject,
                        &occurrence.statement.object,
                    )
                    .contains(&occurrence.family)
                {
                    return Err(invalid(
                        "native construct occurrence has missing, repeated or role-incompatible proof support",
                    ));
                }
                expected
                    .entry((occurrence.family, &occurrence.statement.subject))
                    .or_default()
                    .insert(occurrence.support);
            }
            for admission in &observations.admissions {
                let selectors = admission.selectors.iter().copied().collect::<BTreeSet<_>>();
                if expected.remove(&(admission.family, &admission.owner)) != Some(selectors.clone())
                    || selectors.len() != admission.selectors.len()
                    || admission
                        .selectors
                        .windows(2)
                        .any(|pair| pair[0] >= pair[1])
                    || admission.support.windows(2).any(|pair| pair[0] >= pair[1])
                    || admission.support.iter().any(|id| !proofs.contains_key(id))
                    || !selectors.is_subset(&admission.support.iter().copied().collect())
                    || admission.completion == NativeFamilyCompletion::NotEngaged
                    || (admission.completion == NativeFamilyCompletion::Complete
                        && !admission.obstructions.is_empty())
                    || matches!(&admission.completion, NativeFamilyCompletion::Awaiting { reads } if reads.is_empty())
                    || (admission.completion == NativeFamilyCompletion::Obstructed
                        && admission.obstructions.is_empty())
                    || admission.obstructions.iter().any(|obstruction| {
                        obstruction.detail.is_empty()
                            || obstruction.support.is_empty()
                            || obstruction
                                .support
                                .iter()
                                .any(|id| !admission.support.contains(id))
                    })
                {
                    return Err(invalid(
                        "native source eligibility omits its selected owner or actual field evidence",
                    ));
                }
            }
            if !expected.is_empty() {
                return Err(invalid(
                    "native construct inventory omits a selected source eligibility observation",
                ));
            }
        }
        Ok(())
    }
}

struct ConstructIndex {
    predicates: BTreeMap<&'static str, DlConstructFamily>,
    markers: BTreeMap<&'static str, DlConstructFamily>,
    properties: BTreeMap<&'static str, DlConstructFamily>,
    property_edges: BTreeSet<&'static str>,
}

fn construct_index() -> &'static ConstructIndex {
    static INDEX: std::sync::LazyLock<ConstructIndex> = std::sync::LazyLock::new(|| {
        let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
        let mut index = ConstructIndex {
            predicates: BTreeMap::new(),
            markers: BTreeMap::new(),
            properties: BTreeMap::new(),
            property_edges: BTreeSet::new(),
        };
        for &(family, iri, role) in CONSTRUCTS {
            match role {
                ConstructRole::Predicate => {
                    index.predicates.insert(semantics.predicate(iri), family);
                }
                ConstructRole::Marker => {
                    index.markers.insert(iri, family);
                    if let Some(canonical) = semantics.alternate_marker(RDF_TYPE, iri) {
                        index.markers.insert(canonical, family);
                    }
                }
                ConstructRole::Property => {
                    index.properties.insert(iri, family);
                }
            }
        }
        index.markers.insert(
            "https://blackcatinformatics.ca/logic/KeyAssertion",
            DlConstructFamily::HasKey,
        );
        for predicate in [
            SOURCE_INDIVIDUAL,
            ASSERTION_PROPERTY,
            TARGET_INDIVIDUAL,
            TARGET_VALUE,
        ] {
            index.predicates.insert(
                semantics.predicate(predicate),
                DlConstructFamily::NegativePropertyAssertion,
            );
        }
        for predicate in [
            "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
            "http://www.w3.org/2002/07/owl#equivalentProperty",
            "http://www.w3.org/2002/07/owl#inverseOf",
            "http://www.w3.org/2002/07/owl#propertyDisjointWith",
        ] {
            index.property_edges.insert(semantics.predicate(predicate));
        }
        index
    });
    &INDEX
}

/// Select source-owner roles from a prepared rule body's fixed predicate and
/// optional fixed object. Variable objects never pretend to be marker constants.
/// The same index owns runtime observation and native producer eligibility.
pub(crate) fn source_selector_families(
    predicate: &str,
    object: Option<&TermValue>,
) -> Vec<DlConstructFamily> {
    let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
    let operator = semantics.predicate(predicate);
    let index = construct_index();
    let mut selected = Vec::new();
    if let Some(family) = index.predicates.get(operator) {
        selected.push(*family);
    }
    if let Some(family) = index.properties.get(predicate) {
        selected.push(*family);
    }
    if let Some(iri) = object.and_then(TermValue::as_iri) {
        if operator == semantics.predicate(RDF_TYPE)
            && let Some(family) = index.markers.get(iri)
        {
            selected.push(*family);
        }
        if (index.property_edges.contains(operator) || operator == semantics.predicate(ON_PROPERTY))
            && let Some(family) = index.properties.get(iri)
        {
            selected.push(*family);
        }
        if predicate == "https://blackcatinformatics.ca/logic/characteristicSort"
            && iri == "https://blackcatinformatics.ca/logic/functionalProperty"
        {
            selected.push(DlConstructFamily::FunctionalProperty);
        }
    }
    selected.sort();
    selected.dedup();
    selected
}

/// A structured operator must await all of its selected owner fields before
/// execution. Atomic relation propagation validates only the actual bound row;
/// unrelated completion reads must not create a self-cycle on that propagation.
pub(crate) fn source_execution_requires_admission(
    family: DlConstructFamily,
    predicate: &str,
) -> bool {
    use DlConstructFamily as F;
    match family {
        F::AllValuesFrom
        | F::SomeValuesFrom
        | F::HasValue
        | F::HasSelf
        | F::Cardinality
        | F::MinCardinality
        | F::MaxCardinality
        | F::QualifiedCardinality
        | F::MinQualifiedCardinality
        | F::MaxQualifiedCardinality
        | F::PropertyChainAxiom
        | F::HasKey
        | F::NegativePropertyAssertion
        | F::AllDifferent
        | F::AllDisjointClasses
        | F::AllDisjointProperties
        | F::UnionOf
        | F::IntersectionOf
        | F::OneOf
        | F::DisjointUnionOf
        | F::WithRestrictions => true,
        F::FunctionalProperty => {
            predicate == "https://blackcatinformatics.ca/logic/characteristicSort"
        }
        _ => false,
    }
}

/// Validate a selected atom's native operand roles without waiting for other
/// writers. The same check governs source admission and positive rule candidates;
/// an invalid source operand never becomes an inferred RDF contradiction.
pub(crate) fn source_operand_issue(
    family: DlConstructFamily,
    predicate: &str,
    subject: &TermValue,
    object: &TermValue,
) -> Option<&'static str> {
    use DlConstructFamily as F;
    if matches!(
        family,
        F::SomeValuesFrom
            | F::AllValuesFrom
            | F::ComplementOf
            | F::Domain
            | F::Range
            | F::DatatypeComplementOf
            | F::OnDatatype
    ) && !resource(object)
    {
        return Some("the selected class or datatype operand must be a native resource");
    }
    if matches!(family, F::Domain | F::Range | F::PropertyDisjointWith)
        && subject.as_iri().is_none()
    {
        return Some("a selected property operator requires a native property IRI");
    }
    if family == F::PropertyDisjointWith && object.as_iri().is_none() {
        return Some("a selected property-disjoint operand must be a native property IRI");
    }
    if matches!(
        family,
        F::FunctionalProperty
            | F::InverseFunctionalProperty
            | F::AsymmetricProperty
            | F::IrreflexiveProperty
    ) && predicate != "https://blackcatinformatics.ca/logic/characteristicSort"
        && subject.as_iri().is_none()
    {
        return Some("a selected property characteristic requires a native property IRI");
    }
    if matches!(
        family,
        F::MinInclusive
            | F::MaxInclusive
            | F::MinExclusive
            | F::MaxExclusive
            | F::Pattern
            | F::Length
            | F::MinLength
            | F::MaxLength
            | F::TotalDigits
            | F::FractionDigits
            | F::LangRange
            | F::WhiteSpace
            | F::ExplicitTimezone
    ) && !matches!(object, TermValue::Literal { .. })
    {
        return Some("a selected datatype facet requires its native literal operand");
    }
    None
}

/// Interpret operator and marker positions only; neither data mentions nor
/// embedded triple terms select a native construct family. The immutable role
/// index is shared; each native row performs only its applicable indexed lookups.
fn construct_families(
    predicate: &str,
    subject: &TermValue,
    object: &TermValue,
) -> Vec<DlConstructFamily> {
    let mut selected = source_selector_families(predicate, Some(object));
    let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
    let operator = semantics.predicate(predicate);
    let index = construct_index();
    if let Some(family) = subject.as_iri().and_then(|iri| index.properties.get(iri)) {
        let property_declaration = operator == semantics.predicate(RDF_TYPE)
            && object.as_iri().is_some_and(|marker| {
                [
                    "http://www.w3.org/2002/07/owl#ObjectProperty",
                    "http://www.w3.org/2002/07/owl#DatatypeProperty",
                ]
                .iter()
                .any(|role| {
                    marker == *role || semantics.alternate_marker(predicate, marker) == Some(*role)
                })
            });
        if property_declaration || index.property_edges.contains(operator) {
            selected.push(*family);
        }
    }
    selected.sort();
    selected.dedup();
    selected
}

/// Record one row already visited by the native insertion suffix. The caller
/// visits each committed row once, including inferred schema and initial rows.
/// This does not read, freeze or reconstruct a dataset.
pub(crate) fn observe_construct(
    fact: &crate::rule_ir::Fact,
    input: &super::refute::native::NativeFamilyInput<'_>,
    ledger: &mut NativeFamilyLedger,
    world: &mut SourceCoverageWorld,
) -> gmeow_errors::Result<()> {
    if world.graph.as_ref() != input.graph() {
        return Err(invalid(
            "native construct observer changes its source graph",
        ));
    }
    let families = construct_families(&fact.predicate, &fact.subject, &fact.object);
    if families.is_empty() {
        return Ok(());
    }
    let support = input.support(std::slice::from_ref(fact), ledger)?;
    let [support] = support.as_slice() else {
        return Err(invalid(
            "one native construct statement must have exactly one proof",
        ));
    };
    for family in families {
        world.constructs.push(NativeConstructOccurrence {
            family,
            statement: WitnessStatement {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
            },
            support: *support,
        });
    }
    Ok(())
}

const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
const KEY_PROPERTY: &str = "https://blackcatinformatics.ca/logic/keyProperty";
const MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const DISTINCT_MEMBERS: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
const SOURCE_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
const ASSERTION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
const TARGET_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#targetIndividual";
const TARGET_VALUE: &str = "http://www.w3.org/2002/07/owl#targetValue";
const CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";

fn admission_fields(family: DlConstructFamily) -> &'static [&'static str] {
    use DlConstructFamily as F;
    match family {
        F::AllValuesFrom
        | F::SomeValuesFrom
        | F::HasValue
        | F::HasSelf
        | F::Cardinality
        | F::MinCardinality
        | F::MaxCardinality => &[ON_PROPERTY],
        F::QualifiedCardinality | F::MinQualifiedCardinality | F::MaxQualifiedCardinality => {
            &[ON_PROPERTY, ON_CLASS, ON_DATA_RANGE]
        }
        F::HasKey => &[KEY_CLASS, KEY_PROPERTY, FIRST, REST],
        F::AllDifferent => &[MEMBERS, DISTINCT_MEMBERS, FIRST, REST],
        F::AllDisjointClasses | F::AllDisjointProperties => &[MEMBERS, FIRST, REST],
        F::UnionOf
        | F::OneOf
        | F::IntersectionOf
        | F::DisjointUnionOf
        | F::PropertyChainAxiom
        | F::WithRestrictions => &[FIRST, REST],
        F::NegativePropertyAssertion => &[
            SOURCE_INDIVIDUAL,
            ASSERTION_PROPERTY,
            TARGET_INDIVIDUAL,
            TARGET_VALUE,
        ],
        F::FunctionalProperty => &[CHARACTERIZES],
        _ => &[],
    }
}

/// All grammar readers of the fixed source-admission operation. Specific type
/// markers prevent unrelated class-membership heads creating a false cycle.
/// Equality and other positive grammar writers may finish before this gate;
/// completion-dependent consumers require its actual result afterwards.
pub(crate) fn source_admission_reads() -> Vec<super::refute::NativeRead> {
    CONSTRUCTS
        .iter()
        .flat_map(|(family, _, _)| source_admission_reads_for(*family))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Exact grammar-writer dependencies of one source family. Unrelated metadata
/// never delays a property producer whose own owner has been admitted.
pub(crate) fn source_admission_reads_for(
    family: DlConstructFamily,
) -> Vec<super::refute::NativeRead> {
    use super::refute::{NativeRead, NativeReadKind};
    let completed = |predicate: &str| NativeRead {
        predicate: Some(predicate.to_owned()),
        marker: None,
        kind: NativeReadKind::Completed,
    };
    let mut reads = BTreeSet::new();
    for &(selected, iri, role) in CONSTRUCTS {
        if selected != family {
            continue;
        }
        match role {
            ConstructRole::Marker => {
                reads.insert(NativeRead::declaration(RDF_TYPE, iri));
            }
            ConstructRole::Predicate => {
                reads.insert(completed(iri));
            }
            ConstructRole::Property => {
                reads.insert(completed(iri));
                for marker in [
                    "http://www.w3.org/2002/07/owl#ObjectProperty",
                    "http://www.w3.org/2002/07/owl#DatatypeProperty",
                ] {
                    reads.insert(NativeRead::declaration(RDF_TYPE, marker));
                }
                for predicate in [
                    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
                    "http://www.w3.org/2002/07/owl#equivalentProperty",
                    "http://www.w3.org/2002/07/owl#inverseOf",
                    "http://www.w3.org/2002/07/owl#propertyDisjointWith",
                    ON_PROPERTY,
                ] {
                    reads.insert(completed(predicate));
                }
            }
        }
    }
    for predicate in admission_fields(family) {
        reads.insert(completed(predicate));
    }
    if family == DlConstructFamily::HasKey {
        reads.insert(NativeRead::declaration(
            RDF_TYPE,
            "https://blackcatinformatics.ca/logic/KeyAssertion",
        ));
    }
    if family == DlConstructFamily::FunctionalProperty {
        reads.insert(NativeRead::declaration(
            "https://blackcatinformatics.ca/logic/characteristicSort",
            "https://blackcatinformatics.ca/logic/functionalProperty",
        ));
    }
    reads.into_iter().collect()
}

impl SourceCoverageWorld {
    /// Borrow one exact owner admission from the deterministic sorted inventory.
    /// Missing and pending owners never authorize a property producer.
    pub(crate) fn admission(
        &self,
        family: DlConstructFamily,
        owner: &TermValue,
    ) -> Option<&NativeConstructAdmission> {
        self.admissions
            .binary_search_by(|admission| {
                (admission.family, &admission.owner).cmp(&(family, owner))
            })
            .ok()
            .map(|index| &self.admissions[index])
    }
}

fn selected_fields(
    input: &super::refute::native::NativeFamilyInput<'_>,
    owner: &TermValue,
    predicate: &str,
) -> Vec<crate::rule_ir::Fact> {
    let Some(owner) = input.rel.term_id(owner) else {
        return Vec::new();
    };
    let mut cursor = input
        .rel
        .select_semantic(predicate, crate::physical::Bound::Subject(owner));
    let mut fields = Vec::new();
    while let Some((subject, object, _, spelling)) = cursor.next() {
        fields.push(crate::rule_ir::Fact {
            subject: input.rel.interner().resolve(subject).clone(),
            predicate: spelling.to_owned(),
            object: input.rel.interner().resolve(object).clone(),
        });
    }
    fields
}

fn resource(term: &TermValue) -> bool {
    matches!(term, TermValue::Iri(_) | TermValue::Blank { .. })
}

/// Admit every selected owner using the same indexed native state and bounded
/// list/value caches as the actual producers. No dataset or alternate parser is
/// constructed. The caller retains the recorded outcome before publishing any
/// completed extension dependent on these definitions.
pub(crate) fn admit_source_constructs(
    input: &super::refute::native::NativeFamilyInput<'_>,
    values: &mut crate::physical::SchemaValues,
    lists: &mut crate::physical::LogicalListCache,
    ledger: &mut NativeFamilyLedger,
    world: &mut SourceCoverageWorld,
) -> gmeow_errors::Result<()> {
    if world.graph.as_ref() != input.graph() || ledger.world != input.world() {
        return Err(invalid(
            "source admission belongs to a different native world",
        ));
    }
    if world
        .admissions
        .iter()
        .map(|admission| admission.selectors.len())
        .sum::<usize>()
        == world.constructs.len()
        && !world.admissions.is_empty()
        && world.admissions.iter().all(|admission| {
            !matches!(
                admission.completion,
                NativeFamilyCompletion::Awaiting { .. }
            )
        })
    {
        return Ok(());
    }
    let mut pending_by_family = BTreeMap::new();
    let mut groups =
        BTreeMap::<(DlConstructFamily, TermValue), Vec<&NativeConstructOccurrence>>::new();
    for occurrence in &world.constructs {
        groups
            .entry((occurrence.family, occurrence.statement.subject.clone()))
            .or_default()
            .push(occurrence);
    }
    let mut admissions = Vec::with_capacity(groups.len());
    for ((family, owner), occurrences) in groups {
        let pending = pending_by_family.entry(family).or_insert_with(|| {
            source_admission_reads_for(family)
                .into_iter()
                .filter(|read| !input.completed(read))
                .collect::<Vec<_>>()
        });
        let selectors: Vec<_> = occurrences
            .iter()
            .map(|occurrence| occurrence.support)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if let Some(previous) = world.admission(family, &owner)
            && previous.selectors == selectors
            && !matches!(previous.completion, NativeFamilyCompletion::Awaiting { .. })
        {
            admissions.push(previous.clone());
            continue;
        }
        let mut admission = NativeConstructAdmission {
            family,
            owner: owner.clone(),
            selectors: selectors.clone(),
            support: selectors,
            completion: if pending.is_empty() {
                NativeFamilyCompletion::Complete
            } else {
                NativeFamilyCompletion::Awaiting {
                    reads: pending.clone(),
                }
            },
            obstructions: Vec::new(),
        };
        if !pending.is_empty() {
            admissions.push(admission);
            continue;
        }
        if !ledger.charge(1) {
            admission.completion = NativeFamilyCompletion::Exhausted;
            admissions.push(admission);
            continue;
        }
        let mut fields = BTreeMap::new();
        for predicate in admission_fields(family) {
            // List cells are read by the shared list cache, not re-traversed here.
            if [FIRST, REST].contains(predicate) {
                continue;
            }
            fields.insert(*predicate, selected_fields(input, &owner, predicate));
        }
        let mut support = occurrences
            .iter()
            .map(|occurrence| crate::rule_ir::Fact {
                subject: occurrence.statement.subject.clone(),
                predicate: occurrence.statement.predicate.clone(),
                object: occurrence.statement.object.clone(),
            })
            .chain(fields.values().flatten().cloned())
            .collect::<Vec<_>>();
        let objects = |predicate| {
            fields
                .get(predicate)
                .map_or_else(BTreeSet::new, |rows: &Vec<crate::rule_ir::Fact>| {
                    rows.iter().map(|row| &row.object).collect()
                })
        };
        let mut failures = Vec::<(super::refute::NativeObstructionKind, String)>::new();
        let mut invalid_shape = |message: &str| {
            failures.push((
                super::refute::NativeObstructionKind::SourceShape,
                message.to_owned(),
            ))
        };
        use DlConstructFamily as F;
        let restrictions = matches!(
            family,
            F::AllValuesFrom
                | F::SomeValuesFrom
                | F::HasValue
                | F::HasSelf
                | F::Cardinality
                | F::MinCardinality
                | F::MaxCardinality
                | F::QualifiedCardinality
                | F::MinQualifiedCardinality
                | F::MaxQualifiedCardinality
        );
        if restrictions {
            let properties = objects(ON_PROPERTY);
            if properties.len() != 1 || !properties.iter().all(|term| term.as_iri().is_some()) {
                invalid_shape("a selected restriction requires exactly one property IRI");
            }
        }
        if matches!(
            family,
            F::QualifiedCardinality | F::MinQualifiedCardinality | F::MaxQualifiedCardinality
        ) {
            let qualifiers = objects(ON_CLASS)
                .into_iter()
                .chain(objects(ON_DATA_RANGE))
                .collect::<Vec<_>>();
            if qualifiers.len() != 1 || !qualifiers.iter().all(|term| resource(term)) {
                invalid_shape(
                    "a qualified count requires exactly one resource class or datatype qualifier",
                );
            }
        }
        for occurrence in &occurrences {
            if let Some(message) = source_operand_issue(
                family,
                &occurrence.statement.predicate,
                &occurrence.statement.subject,
                &occurrence.statement.object,
            ) {
                invalid_shape(message);
            }
        }
        if family == F::NegativePropertyAssertion {
            let subjects = objects(SOURCE_INDIVIDUAL);
            let predicates = objects(ASSERTION_PROPERTY);
            let individuals = objects(TARGET_INDIVIDUAL);
            let literals = objects(TARGET_VALUE);
            if subjects.len() != 1 || !subjects.iter().all(|term| resource(term)) {
                invalid_shape("a negative assertion requires one source individual");
            }
            if predicates.len() != 1 || !predicates.iter().all(|term| term.as_iri().is_some()) {
                invalid_shape("a negative assertion requires one assertion-property IRI");
            }
            if individuals.len() + literals.len() != 1
                || !individuals.iter().all(|term| resource(term))
                || !literals
                    .iter()
                    .all(|term| matches!(term, TermValue::Literal { .. }))
            {
                invalid_shape(
                    "a negative assertion requires exactly one correctly typed individual or literal target",
                );
            }
        }
        if family == F::FunctionalProperty
            && occurrences.iter().any(|occurrence| {
                occurrence.statement.predicate
                    == "https://blackcatinformatics.ca/logic/characteristicSort"
            })
        {
            let properties = objects(CHARACTERIZES);
            if properties.len() != 1 || !properties.iter().all(|term| term.as_iri().is_some()) {
                invalid_shape("a characteristic record requires exactly one property IRI");
            }
        }
        let semantics = crate::native_semantics::SemanticVocabulary::GroundedLogicV1;
        let canonical_key = family == F::HasKey
            && occurrences.iter().any(|occurrence| {
                semantics.predicate(&occurrence.statement.predicate)
                    == semantics.predicate(RDF_TYPE)
                    && occurrence.statement.object.as_iri()
                        == Some("https://blackcatinformatics.ca/logic/KeyAssertion")
            });
        let mut roots = Vec::new();
        if matches!(
            family,
            F::PropertyChainAxiom
                | F::UnionOf
                | F::OneOf
                | F::IntersectionOf
                | F::DisjointUnionOf
                | F::WithRestrictions
        ) {
            roots.extend(
                occurrences
                    .iter()
                    .map(|occurrence| &occurrence.statement.object),
            );
        }
        if family == F::HasKey {
            roots.extend(
                occurrences
                    .iter()
                    .filter(|occurrence| {
                        semantics.predicate(&occurrence.statement.predicate)
                            == semantics.predicate("http://www.w3.org/2002/07/owl#hasKey")
                    })
                    .map(|occurrence| &occurrence.statement.object),
            );
        }
        if matches!(
            family,
            F::AllDifferent | F::AllDisjointClasses | F::AllDisjointProperties
        ) {
            roots.extend(objects(MEMBERS));
            if family == F::AllDifferent {
                roots.extend(objects(DISTINCT_MEMBERS));
            }
            if roots.is_empty() {
                invalid_shape("a selected pairwise construct requires a member list");
            }
        }
        // Release the mutation-only helper before recording typed traversal results.
        if matches!(
            family,
            F::Cardinality
                | F::MinCardinality
                | F::MaxCardinality
                | F::QualifiedCardinality
                | F::MinQualifiedCardinality
                | F::MaxQualifiedCardinality
        ) {
            let (native_values, _) = values.parts();
            for occurrence in &occurrences {
                if native_values
                    .cardinality(&occurrence.statement.object)
                    .is_none()
                {
                    failures.push((
                        super::refute::NativeObstructionKind::UnsupportedValue,
                        "cardinality requires an admitted non-negative native count".to_owned(),
                    ));
                }
            }
        }
        if family == F::HasSelf {
            let (native_values, _) = values.parts();
            for occurrence in &occurrences {
                if !matches!(&occurrence.statement.object, TermValue::Literal { .. })
                    || !matches!(
                        &native_values.literal(&occurrence.statement.object).meaning,
                        crate::reason::value::LiteralMeaning::Native(
                            purrdf::xsd::XsdValue::Boolean(true)
                        )
                    )
                {
                    failures.push((
                        super::refute::NativeObstructionKind::UnsupportedValue,
                        "hasSelf requires an admitted true native boolean".to_owned(),
                    ));
                }
            }
        }
        roots.sort();
        roots.dedup();
        // A canonical record and projected RDF list can share an owner or root;
        // each representation is an independently selected grammar obligation.
        let selected_lists = roots
            .into_iter()
            .map(|root| (root, false))
            .chain(canonical_key.then_some((&owner, true)));
        for (root, key_record) in selected_lists {
            let read = if key_record {
                lists.read_key_record(input.rel, root)
            } else {
                lists.read(input.rel, root)
            };
            match read {
                Ok(Some(list)) => {
                    support.extend(list.premises(input.rel));
                    let property_list = matches!(
                        family,
                        F::PropertyChainAxiom | F::HasKey | F::AllDisjointProperties
                    );
                    if family == F::PropertyChainAxiom && list.members.len() < 2 {
                        failures.push((
                            super::refute::NativeObstructionKind::SourceShape,
                            "a selected property chain requires at least two properties".to_owned(),
                        ));
                    }
                    if property_list
                        && (list.members.is_empty()
                            || list.members.iter().any(|member| {
                                input.rel.interner().resolve(*member).as_iri().is_none()
                            }))
                    {
                        failures.push((super::refute::NativeObstructionKind::SourceShape, "a selected property list must be nonempty and contain only native property IRIs".to_owned()));
                    } else if family != F::OneOf
                        && list
                            .members
                            .iter()
                            .any(|member| !resource(input.rel.interner().resolve(*member)))
                    {
                        failures.push((
                            super::refute::NativeObstructionKind::SourceShape,
                            "a selected structural list requires resource members".to_owned(),
                        ));
                    }
                }
                Ok(None) => {
                    support.extend_from_slice(lists.pending_premises());
                    failures.push((
                        super::refute::NativeObstructionKind::IncompleteDefinition,
                        "selected list writers completed without a complete logical list"
                            .to_owned(),
                    ));
                }
                Err(error) => {
                    let message = error.message().to_owned();
                    support.extend(error.premises);
                    failures.push((super::refute::NativeObstructionKind::SourceShape, message));
                }
            }
        }
        support.sort_by_key(crate::rule_ir::Fact::key);
        support.dedup();
        admission.support = input.support(&support, ledger)?;
        for (kind, detail) in failures {
            admission
                .obstructions
                .push(super::refute::NativeFamilyObstruction {
                    kind,
                    detail,
                    support: admission.support.clone(),
                });
        }
        if !admission.obstructions.is_empty() {
            admission.completion = NativeFamilyCompletion::Obstructed;
        }
        admissions.push(admission);
    }
    world.admissions = admissions;
    Ok(())
}

fn complete(completion: &NativeFamilyCompletion) -> bool {
    matches!(
        completion,
        NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
    )
}

fn family_complete(ledger: &NativeFamilyLedger, family: NativeRefutationFamily) -> bool {
    !ledger.work.exhausted
        && ledger.outcomes.iter().any(|outcome| {
            outcome.family == family
                && outcome.obligation == super::refute::NativeObligationScope::World
        })
        && ledger
            .outcomes
            .iter()
            .filter(|outcome| outcome.family == family)
            .all(|outcome| complete(&outcome.completion) && outcome.obstructions.is_empty())
}

/// The coverage owner of each selected construct is explicit. Dependencies are
/// conjunctive: one complete model never overrides a different unfinished one.
fn construct_complete(
    construct: DlConstructFamily,
    ledger: &NativeFamilyLedger,
    class: &super::refute::ClassExecutionOutcome,
    frontier: &crate::query_ir::CompletionFrontier,
) -> bool {
    use DlConstructFamily as F;
    use NativeRefutationFamily as N;
    if frontier.completed != frontier.total {
        return false;
    }
    match construct {
        F::TopObjectProperty | F::TopDataProperty => false,
        F::ComplementOf | F::UnionOf | F::DisjointUnionOf | F::OneOf => {
            class.completion == NativeFamilyCompletion::Complete && class.obstructions.is_empty()
        }
        F::IntersectionOf | F::AllDisjointClasses => {
            complete(&class.completion) && class.obstructions.is_empty()
        }
        F::Cardinality
        | F::MinCardinality
        | F::MaxCardinality
        | F::QualifiedCardinality
        | F::MinQualifiedCardinality
        | F::MaxQualifiedCardinality
        | F::SomeValuesFrom => {
            family_complete(ledger, N::Cardinality) && family_complete(ledger, N::Datatype)
        }
        F::DatatypeComplementOf
        | F::WithRestrictions
        | F::OnDatatype
        | F::MinInclusive
        | F::MaxInclusive
        | F::MinExclusive
        | F::MaxExclusive
        | F::Pattern
        | F::Length
        | F::MinLength
        | F::MaxLength
        | F::TotalDigits
        | F::FractionDigits
        | F::LangRange
        | F::WhiteSpace
        | F::ExplicitTimezone => family_complete(ledger, N::Datatype),
        F::FunctionalProperty
        | F::InverseFunctionalProperty
        | F::HasKey
        | F::NegativePropertyAssertion
        | F::PropertyDisjointWith
        | F::AllDisjointProperties => {
            family_complete(ledger, N::Identity) && family_complete(ledger, N::Datatype)
        }
        F::AllDifferent => family_complete(ledger, N::Identity),
        F::HasSelf => family_complete(ledger, N::HasSelf),
        F::AllValuesFrom
        | F::HasValue
        | F::Domain
        | F::Range
        | F::PropertyChainAxiom
        | F::BottomObjectProperty
        | F::BottomDataProperty
        | F::AsymmetricProperty
        | F::IrreflexiveProperty => true,
    }
}

/// Fold only retained native observations. Completion across worlds is a meet;
/// one fully decided world cannot conceal an obstructed instance in another.
fn coverage_from_native(execution: &crate::result::NativeExecutionEvidence) -> DlCoverage {
    let ledgers: BTreeMap<_, _> = execution
        .families
        .iter()
        .map(|ledger| (ledger.world.as_str(), ledger))
        .collect();
    let classes: BTreeMap<_, _> = execution
        .classes
        .iter()
        .map(|class| (class.world.as_str(), class))
        .collect();
    let mut families = BTreeMap::<DlConstructFamily, bool>::new();
    for (world, observation) in &execution.source_coverage.worlds {
        let admissions: BTreeMap<_, _> = observation
            .admissions
            .iter()
            .map(|admission| ((admission.family, &admission.owner), admission))
            .collect();
        for occurrence in &observation.constructs {
            let admission = admissions[&(occurrence.family, &occurrence.statement.subject)];
            let decided = admission.completion == NativeFamilyCompletion::Complete
                && admission.obstructions.is_empty()
                && execution.status
                    == crate::reason::refute::native::NativeClosureStatus::Completed
                && construct_complete(
                    occurrence.family,
                    ledgers[world.as_str()],
                    classes[world.as_str()],
                    &execution.frontier,
                );
            families
                .entry(occurrence.family)
                .and_modify(|all| *all &= decided)
                .or_insert(decided);
        }
    }
    let mut coverage = DlCoverage {
        present: Vec::new(),
        decided: Vec::new(),
        unsupported: Vec::new(),
    };
    for (family, decided) in families {
        let name = family.name().to_owned();
        coverage.present.push(name.clone());
        if decided {
            coverage.decided.push(name);
        } else {
            coverage.unsupported.push(name);
        }
    }
    coverage.present.sort();
    coverage.decided.sort();
    coverage.unsupported.sort();
    coverage
}

/// Read local contradictions and class emptiness from exact native output rows.
/// Context-wide contradictions remain in the execution evidence rather than
/// inventing a subject on which to assert membership in the empty class.
fn local_verdict(inferred: &[InferredAxiom]) -> (Vec<UnsatClass>, Vec<InconsistencyWitness>) {
    let inconsistencies = inferred
        .iter()
        .filter(|ax| {
            ax.object
                .as_iri()
                .and_then(|iri| EmptyClassAssertion::classify(&ax.predicate, iri))
                == Some(EmptyClassAssertion::Membership)
        })
        .map(|ax| InconsistencyWitness {
            individual: ax.subject.clone(),
            world: ax.world.clone(),
            premises: ax.premises.clone(),
        })
        .collect();
    (unsatisfiable_from_inferred(inferred), inconsistencies)
}

/// Project an execution already admitted by the owning result constructor.
/// The owning result constructor validates the proof DAG once before this fold.
pub(crate) fn verdict_from_validated_native(
    inferred: &[InferredAxiom],
    execution: &crate::result::NativeExecutionEvidence,
) -> DlVerdict {
    let coverage = coverage_from_native(execution);
    let mut gaps = gaps_from_unsupported(&coverage.unsupported);
    let mut boundaries = BTreeSet::new();
    let mut record = |code: String, world: &str, detail: String, family| {
        gaps.push(DlGap::new(
            format!("reason.dl-gap.{code}"),
            format!("{world}: {detail}"),
        ));
        boundaries.insert(super::refute::FragmentBoundary::Uncertified {
            family,
            obstructions: BTreeSet::from([format!("{world}: {detail}")]),
        });
    };
    for ledger in &execution.families {
        for outcome in &ledger.outcomes {
            if !complete(&outcome.completion) || !outcome.obstructions.is_empty() {
                let detail = format!(
                    "{:?} {:?}: {:?}; {:?}",
                    outcome.family, outcome.obligation, outcome.completion, outcome.obstructions
                );
                let family = if outcome.family == NativeRefutationFamily::Datatype {
                    super::refute::FragmentFamily::DatatypeValueSpace
                } else {
                    super::refute::FragmentFamily::Counting
                };
                record(
                    format!("native-{:?}", outcome.family).to_lowercase(),
                    &ledger.world,
                    detail,
                    family,
                );
            }
        }
        if ledger.work.exhausted {
            record(
                "native-analysis-budget".to_owned(),
                &ledger.world,
                "the shared native family analysis allowance was exhausted".to_owned(),
                super::refute::FragmentFamily::Counting,
            );
        }
    }
    for class in &execution.classes {
        if !complete(&class.completion) || !class.obstructions.is_empty() {
            record(
                "native-class-model".to_owned(),
                &class.world,
                format!("{:?}; {:?}", class.completion, class.obstructions),
                super::refute::FragmentFamily::CaseSplit,
            );
        }
    }
    for (world, source) in &execution.source_coverage.worlds {
        for admission in &source.admissions {
            if admission.completion != NativeFamilyCompletion::Complete
                || !admission.obstructions.is_empty()
            {
                record(
                    format!("source-{}", admission.family.name()),
                    world,
                    format!(
                        "{} owner {}: {:?}; {:?}",
                        admission.family.name(),
                        crate::provenance::term_display(&admission.owner),
                        admission.completion,
                        admission.obstructions
                    ),
                    super::refute::FragmentFamily::CaseSplit,
                );
            }
        }
    }
    for (world, source) in &execution.class_admission.selected_worlds {
        if let Some(refusal) = &source.refusal {
            gaps.push(DlGap::new(
                "reason.dl-gap.class-source-admission",
                format!("{world}: {refusal:?}"),
            ));
            boundaries.insert(refusal.clone());
        }
    }
    if execution.frontier.completed != execution.frontier.total {
        gaps.push(DlGap::new(
            "reason.dl-gap.native-frontier",
            "the shared native execution ended before every selected producer stratum completed",
        ));
    }
    if execution.status != crate::reason::refute::native::NativeClosureStatus::Completed {
        gaps.push(DlGap::new(
            "reason.dl-gap.native-completion",
            format!(
                "the shared native closure did not complete: {:?}",
                execution.status
            ),
        ));
    }
    gaps.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    gaps.dedup();
    let boundary_findings = boundaries
        .iter()
        .flat_map(|boundary| super::refute::boundary_diag_ledger(boundary).findings("reason"))
        .collect();
    let (unsatisfiable_classes, inconsistencies) = local_verdict(inferred);
    DlVerdict {
        consistent: inconsistencies.is_empty() && !execution.has_conflict(),
        unsatisfiable_classes,
        inconsistencies,
        coverage,
        gaps,
        boundary_findings,
    }
}

/// Run the native engine once and retain its DL diagnostic projection.
///
/// # Errors
/// Returns the actual source-admission or execution refusal.
pub fn dl_consistency(
    input: crate::reason::PreparedReasoningInput,
    domains: &crate::reason::SelectedDomains,
) -> gmeow_errors::Result<DlVerdict> {
    crate::reason::reason_all(input, domains)?.native_verdict()
}

#[path = "dl.tests.rs"]
#[cfg(test)]
mod tests;
