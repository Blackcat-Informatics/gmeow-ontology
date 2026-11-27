// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! The gUFO floor measure: the denominator, the join and the judgments.
//!
//! The denominator is every term the vendored gUFO source declares in the gUFO namespace,
//! with the kind its own declaration gives it (class, object property, data property,
//! annotation property, individual or datatype). Each term is joined to the correspondence
//! cells that name it as their target. Each cell is judged on the native claim and the
//! target claim independently: the native claim asks whether the source is a declared
//! native term that earns the counterpart, and the target claim records what the generated
//! gUFO view does with it. Every cell keeps its own source identity, so many-to-one
//! mappings stay individually visible.
//!
//! The measure is pure over observed data. It reads no file and produces no corpus.

use std::collections::{BTreeMap, BTreeSet};

use super::super::gufo_superset::{ObservedTerm, SourceObservation};

/// The gUFO namespace and the ontology IRI that names the vendored source itself.
pub(super) const GUFO_NS: &str = "http://purl.org/nemo/gufo#";

/// The gUFO terms the foundation charter excepts from the universal claim by decision:
/// `inheresIn` (the charter omits a generic inherence predicate) and `externallyDependsOn`
/// (its relator domain is disjoint from the gUFO domain, so the view does not emit the edge).
/// Each keeps its measured judgment and stays listed as a gap; the universal claim excludes
/// only these.
pub(super) const DECIDED_EXCEPTIONS: &[&str] = &["inheresIn", "externallyDependsOn"];
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";
const GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";
const CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
const CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";
const PROPERTY_CHARACTERISTIC_ASSERTION: &str =
    "https://blackcatinformatics.ca/logic/PropertyCharacteristicAssertion";

/// The kind a vendored declaration gives a term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Class,
    ObjectProperty,
    DataProperty,
    AnnotationProperty,
    NamedIndividual,
    Datatype,
}

impl Kind {
    pub(super) const ALL: [Self; 6] = [
        Self::Class,
        Self::ObjectProperty,
        Self::DataProperty,
        Self::AnnotationProperty,
        Self::NamedIndividual,
        Self::Datatype,
    ];

    fn from_declaration(type_iri: &str) -> Option<Self> {
        if type_iri == RDFS_DATATYPE {
            return Some(Self::Datatype);
        }
        match type_iri.strip_prefix(OWL_NS)? {
            "Class" => Some(Self::Class),
            "ObjectProperty" => Some(Self::ObjectProperty),
            "DatatypeProperty" => Some(Self::DataProperty),
            "AnnotationProperty" => Some(Self::AnnotationProperty),
            "NamedIndividual" => Some(Self::NamedIndividual),
            _ => None,
        }
    }

    /// The `logic:` type a native term of the same kind carries.
    fn native_type(self) -> Option<String> {
        let local = match self {
            Self::Class => "Class",
            Self::ObjectProperty => "ObjectProperty",
            Self::DataProperty => "DatatypeProperty",
            Self::AnnotationProperty => "AnnotationProperty",
            Self::NamedIndividual => "NamedIndividual",
            Self::Datatype => return None,
        };
        Some(format!("{LOGIC_NS}{local}"))
    }
}

/// A property characteristic gUFO declares and a native counterpart must carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Characteristic {
    Functional,
    InverseFunctional,
    Transitive,
    Symmetric,
    Asymmetric,
    Reflexive,
    Irreflexive,
}

impl Characteristic {
    const ALL: [Self; 7] = [
        Self::Functional,
        Self::InverseFunctional,
        Self::Transitive,
        Self::Symmetric,
        Self::Asymmetric,
        Self::Reflexive,
        Self::Irreflexive,
    ];

    fn stem(self) -> &'static str {
        match self {
            Self::Functional => "functional",
            Self::InverseFunctional => "inverseFunctional",
            Self::Transitive => "transitive",
            Self::Symmetric => "symmetric",
            Self::Asymmetric => "asymmetric",
            Self::Reflexive => "reflexive",
            Self::Irreflexive => "irreflexive",
        }
    }

    fn owl_type(self) -> String {
        let stem = self.stem();
        let (head, tail) = stem.split_at(1);
        format!("{OWL_NS}{}{tail}Property", head.to_uppercase())
    }

    fn logic_sort(self) -> String {
        format!("{LOGIC_NS}{}Property", self.stem())
    }
}

/// One denominator entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Term {
    pub iri: String,
    pub kind: Kind,
    pub characteristics: BTreeSet<Characteristic>,
}

/// A failure to derive the denominator from the vendored declarations.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum InventoryError {
    /// A subject in the gUFO namespace carries no recognized declaration.
    Unclassified(String),
    /// A subject in the gUFO namespace is declared with more than one kind.
    Ambiguous(String),
}

impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unclassified(term) => {
                write!(
                    f,
                    "{term} is in the gUFO namespace with no recognized declaration"
                )
            }
            Self::Ambiguous(term) => {
                write!(f, "{term} is declared with more than one kind")
            }
        }
    }
}

/// Derive the denominator from the vendored source's own declarations.
pub(super) fn denominator(vendored: &SourceObservation) -> Result<Vec<Term>, InventoryError> {
    let mut kinds: BTreeMap<&str, BTreeSet<Kind>> = BTreeMap::new();
    let mut characteristics: BTreeMap<&str, BTreeSet<Characteristic>> = BTreeMap::new();
    for (subject, object) in vendored.pairs.get(RDF_TYPE).into_iter().flatten() {
        let ObservedTerm::Iri(type_iri) = object else {
            continue;
        };
        if !subject.starts_with(GUFO_NS) {
            continue;
        }
        if let Some(kind) = Kind::from_declaration(type_iri) {
            kinds.entry(subject).or_default().insert(kind);
        } else if let Some(found) = Characteristic::ALL
            .into_iter()
            .find(|characteristic| characteristic.owl_type() == *type_iri)
        {
            characteristics.entry(subject).or_default().insert(found);
        }
    }
    let mut terms = Vec::new();
    for subject in &vendored.subjects {
        if !subject.starts_with(GUFO_NS) || subject == GUFO_NS {
            continue;
        }
        let Some(declared) = kinds.get(subject.as_str()) else {
            return Err(InventoryError::Unclassified(subject.clone()));
        };
        let mut declared = declared.iter();
        let (Some(kind), None) = (declared.next(), declared.next()) else {
            return Err(InventoryError::Ambiguous(subject.clone()));
        };
        terms.push(Term {
            iri: subject.clone(),
            kind: *kind,
            characteristics: characteristics.remove(subject.as_str()).unwrap_or_default(),
        });
    }
    Ok(terms)
}

/// One authored grounding cell that targets a gUFO term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Correspondence {
    pub source: String,
    pub target: String,
    pub predicate: String,
    pub preservation: String,
    /// The projection loss the cell states, from `gmeow:lossyDrop`, when it states one.
    pub loss: Option<String>,
}

/// The judgment on the native claim, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Judgment {
    /// An exact, close or narrow match whose source is declared with the target's kind, carries
    /// a graph box role and, for a property, every characteristic gUFO states for the target.
    EarnedCounterpart,
    /// A related match whose source carries the same declaration evidence and whose cell
    /// states the projection loss. The native term stands in for the gUFO term.
    NativeReplacement,
    /// A cell exists, but its source is a broad match or lacks a declaration, a graph box role
    /// or a characteristic. The catalog validates; the native claim is not earned.
    ValidationOnlyMatch,
    /// No cell names the term.
    Uncovered,
}

impl Judgment {
    /// Whether this judgment satisfies the promise that every gUFO term is covered.
    pub(super) fn covers(self) -> bool {
        matches!(self, Self::EarnedCounterpart | Self::NativeReplacement)
    }
}

/// What the generated gUFO view does with the correspondence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TargetClaim {
    ValidationOnly,
    Unsupported,
    Other(String),
}

impl TargetClaim {
    fn from_preservation(iri: &str) -> Self {
        match iri.strip_prefix(LOGIC_NS) {
            Some("ValidationOnly") => Self::ValidationOnly,
            Some("Unsupported") => Self::Unsupported,
            _ => Self::Other(iri.to_owned()),
        }
    }
}

/// Why a native source falls short of the evidence a counterpart needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Shortfall {
    /// The source is declared in no native module with the target's kind.
    NotDeclaredWithKind,
    /// The declaring module gives the source no `gmeow:graphBoxRole`.
    NoGraphBoxRole,
    /// gUFO states these characteristics and the native source does not carry them.
    MissingCharacteristics(BTreeSet<Characteristic>),
    /// A related match states no projection loss, so the cell does not say what the native
    /// replacement leaves out.
    NoStatedLoss,
}

/// The relation the cell's SKOS predicate states between the native source and the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relation {
    SameOrWider,
    Narrower,
    Related,
}

impl Relation {
    fn from_predicate(predicate: &str) -> Option<Self> {
        match predicate.strip_prefix(SKOS_NS)? {
            "exactMatch" | "closeMatch" | "narrowMatch" => Some(Self::SameOrWider),
            "broadMatch" => Some(Self::Narrower),
            "relatedMatch" => Some(Self::Related),
            _ => None,
        }
    }
}

/// One cell, judged with its basis. Source identity is kept per cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CellJudgment {
    pub correspondence: Correspondence,
    pub judgment: Judgment,
    pub target: TargetClaim,
    pub shortfalls: Vec<Shortfall>,
}

/// One denominator term with every cell that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TermOutcome {
    pub term: Term,
    pub cells: Vec<CellJudgment>,
}

impl TermOutcome {
    /// The strongest native judgment among the term's cells.
    pub(super) fn judgment(&self) -> Judgment {
        self.cells
            .iter()
            .map(|cell| cell.judgment)
            .min()
            .unwrap_or(Judgment::Uncovered)
    }
}

/// A failure to join the catalog to the denominator.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum JoinError {
    /// A cell targets a term the vendored source does not declare.
    UnknownTarget(String),
    /// A cell's predicate is not a SKOS match the measure can read.
    UnreadablePredicate { source: String, predicate: String },
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTarget(target) => {
                write!(
                    f,
                    "a grounding cell targets {target}, which the vendored source does not declare"
                )
            }
            Self::UnreadablePredicate { source, predicate } => {
                write!(
                    f,
                    "the cell from {source} uses {predicate}, which is not a SKOS match"
                )
            }
        }
    }
}

/// The native modules' observations, aggregated.
pub(super) struct Natives<'a> {
    modules: Vec<&'a SourceObservation>,
}

impl<'a> Natives<'a> {
    pub(super) fn new(modules: impl IntoIterator<Item = &'a SourceObservation>) -> Self {
        Self {
            modules: modules.into_iter().collect(),
        }
    }

    fn pairs<'s>(
        &'s self,
        predicate: &'s str,
    ) -> impl Iterator<Item = &'a (String, ObservedTerm)> + 's {
        self.modules
            .iter()
            .flat_map(move |module| module.pairs.get(predicate).into_iter().flatten())
    }

    fn has_type(&self, subject: &str, type_iri: &str) -> bool {
        self.pairs(RDF_TYPE)
            .any(|(s, o)| s == subject && matches!(o, ObservedTerm::Iri(iri) if iri == type_iri))
    }

    fn has_graph_box_role(&self, subject: &str) -> bool {
        self.pairs(GRAPH_BOX_ROLE).any(|(s, _)| s == subject)
    }

    /// Characteristics carried as a type of the property or by a typed
    /// `logic:PropertyCharacteristicAssertion` that characterizes it.
    fn characteristics(&self, subject: &str) -> BTreeSet<Characteristic> {
        let assertions: BTreeSet<&str> = self
            .pairs(CHARACTERIZES)
            .filter(|(_, o)| matches!(o, ObservedTerm::Iri(iri) if iri == subject))
            .map(|(s, _)| s.as_str())
            .filter(|assertion| self.has_type(assertion, PROPERTY_CHARACTERISTIC_ASSERTION))
            .collect();
        Characteristic::ALL
            .into_iter()
            .filter(|characteristic| {
                let sort = characteristic.logic_sort();
                self.has_type(subject, &sort)
                    || self.pairs(CHARACTERISTIC_SORT).any(|(s, o)| {
                        assertions.contains(s.as_str())
                            && matches!(o, ObservedTerm::Iri(iri) if *iri == sort)
                    })
            })
            .collect()
    }

    fn shortfalls(&self, source: &str, target: &Term) -> Vec<Shortfall> {
        let declared = target
            .kind
            .native_type()
            .is_some_and(|native| self.has_type(source, &native));
        if !declared {
            return vec![Shortfall::NotDeclaredWithKind];
        }
        let mut shortfalls = Vec::new();
        if !self.has_graph_box_role(source) {
            shortfalls.push(Shortfall::NoGraphBoxRole);
        }
        // Only a property carries characteristics, so a class yields no characteristic shortfall.
        let carried = self.characteristics(source);
        let missing: BTreeSet<Characteristic> = target
            .characteristics
            .difference(&carried)
            .copied()
            .collect();
        if !missing.is_empty() {
            shortfalls.push(Shortfall::MissingCharacteristics(missing));
        }
        shortfalls
    }

    fn judge(&self, cell: &Correspondence, target: &Term) -> Result<CellJudgment, JoinError> {
        let relation = Relation::from_predicate(&cell.predicate).ok_or_else(|| {
            JoinError::UnreadablePredicate {
                source: cell.source.clone(),
                predicate: cell.predicate.clone(),
            }
        })?;
        let claim = TargetClaim::from_preservation(&cell.preservation);
        let mut shortfalls = self.shortfalls(&cell.source, target);
        // A replacement must say what it leaves out. The loss is a property of the cell,
        // so it is read here and not from the target claim.
        let states_loss = cell
            .loss
            .as_deref()
            .is_some_and(|loss| !loss.trim().is_empty());
        if relation == Relation::Related && !states_loss {
            shortfalls.push(Shortfall::NoStatedLoss);
        }
        // The native judgment reads only the relation, the cell's stated loss and the native
        // evidence. The target claim is reported beside it and never changes it.
        let judgment = match (shortfalls.is_empty(), relation) {
            (true, Relation::SameOrWider) => Judgment::EarnedCounterpart,
            (true, Relation::Related) => Judgment::NativeReplacement,
            (true, Relation::Narrower) | (false, _) => Judgment::ValidationOnlyMatch,
        };
        Ok(CellJudgment {
            correspondence: cell.clone(),
            judgment,
            target: claim,
            shortfalls,
        })
    }
}

/// The measured floor.
#[derive(Debug)]
pub(super) struct Report {
    pub outcomes: Vec<TermOutcome>,
}

impl Report {
    /// Terms per kind and strongest judgment.
    pub(super) fn counts(&self) -> BTreeMap<(Kind, Judgment), usize> {
        let mut counts = BTreeMap::new();
        for outcome in &self.outcomes {
            *counts
                .entry((outcome.term.kind, outcome.judgment()))
                .or_insert(0) += 1;
        }
        counts
    }

    /// Terms whose strongest judgment does not cover them.
    pub(super) fn gaps(&self) -> Vec<&TermOutcome> {
        self.outcomes
            .iter()
            .filter(|outcome| !outcome.judgment().covers())
            .collect()
    }

    /// Whether every gap is one of the charter's decided exceptions, so every other term is
    /// covered by evidence. It does not compare subsumption, disjointness or domain and range.
    pub(super) fn every_term_is_covered_or_decided(&self) -> bool {
        self.gaps().iter().all(|outcome| {
            DECIDED_EXCEPTIONS
                .iter()
                .any(|local| outcome.term.iri == format!("{GUFO_NS}{local}"))
        })
    }

    /// Each native source with the gUFO terms its cells name.
    pub(super) fn targets_by_source(&self) -> BTreeMap<&str, BTreeSet<&str>> {
        let mut grouped: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for cell in self.outcomes.iter().flat_map(|outcome| &outcome.cells) {
            grouped
                .entry(&cell.correspondence.source)
                .or_default()
                .insert(&cell.correspondence.target);
        }
        grouped
    }
}

/// Join the catalog to the denominator and judge every cell.
pub(super) fn measure(
    terms: &[Term],
    cells: &[Correspondence],
    natives: &Natives<'_>,
) -> Result<Report, JoinError> {
    let mut outcomes: Vec<TermOutcome> = terms
        .iter()
        .map(|term| TermOutcome {
            term: term.clone(),
            cells: Vec::new(),
        })
        .collect();
    for cell in cells {
        let Some(outcome) = outcomes.iter_mut().find(|o| o.term.iri == cell.target) else {
            return Err(JoinError::UnknownTarget(cell.target.clone()));
        };
        let judged = natives.judge(cell, &outcome.term)?;
        outcome.cells.push(judged);
    }
    Ok(Report { outcomes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iri(local: &str) -> String {
        format!("{GUFO_NS}{local}")
    }

    fn logic(local: &str) -> String {
        format!("{LOGIC_NS}{local}")
    }

    fn observation(rows: &[(&str, &str, String)]) -> SourceObservation {
        let mut observed = SourceObservation {
            source_path: "synthetic.ttl".to_owned(),
            default_quad_count: rows.len(),
            subjects: BTreeSet::new(),
            pairs: BTreeMap::new(),
        };
        for (subject, predicate, object) in rows {
            observed.subjects.insert((*subject).to_owned());
            observed
                .pairs
                .entry((*predicate).to_owned())
                .or_default()
                .push(((*subject).to_owned(), ObservedTerm::iri(object.clone())));
        }
        observed
    }

    fn owl(local: &str) -> String {
        format!("{OWL_NS}{local}")
    }

    fn vendored() -> SourceObservation {
        observation(&[
            (GUFO_NS, RDF_TYPE, owl("Ontology")),
            (iri("Thing").as_str(), RDF_TYPE, owl("Class")),
            (iri("partOf").as_str(), RDF_TYPE, owl("ObjectProperty")),
            (iri("partOf").as_str(), RDF_TYPE, owl("TransitiveProperty")),
        ])
    }

    fn native(with_property: bool) -> SourceObservation {
        let mut rows = vec![
            ("urn:n:Thing", RDF_TYPE, logic("Class")),
            ("urn:n:Thing", GRAPH_BOX_ROLE, "urn:box".to_owned()),
        ];
        if with_property {
            rows.extend([
                ("urn:n:partOf", RDF_TYPE, logic("ObjectProperty")),
                ("urn:n:partOf", RDF_TYPE, logic("transitiveProperty")),
                ("urn:n:partOf", GRAPH_BOX_ROLE, "urn:box".to_owned()),
            ]);
        }
        observation(&rows)
    }

    fn cell(source: &str, target: &str, predicate: &str, preservation: &str) -> Correspondence {
        Correspondence {
            source: source.to_owned(),
            target: iri(target),
            predicate: format!("{SKOS_NS}{predicate}"),
            preservation: logic(preservation),
            loss: None,
        }
    }

    fn with_loss(cell: Correspondence, loss: &str) -> Correspondence {
        Correspondence {
            loss: Some(loss.to_owned()),
            ..cell
        }
    }

    fn catalog() -> Vec<Correspondence> {
        vec![
            cell("urn:n:Thing", "Thing", "closeMatch", "ValidationOnly"),
            cell("urn:n:partOf", "partOf", "closeMatch", "ValidationOnly"),
        ]
    }

    fn judgment_of(report: &Report, local: &str) -> Judgment {
        report
            .outcomes
            .iter()
            .find(|outcome| outcome.term.iri == iri(local))
            .expect("the synthetic denominator holds the term")
            .judgment()
    }

    /// INVARIANT: removing a property's evidence changes its result even when every class
    /// remains covered. Removing the native declaration leaves a validation-only match;
    /// removing the cell as well leaves the property uncovered.
    #[test]
    fn removing_property_evidence_changes_its_result_while_the_class_stays_covered() {
        let terms = denominator(&vendored()).expect("the synthetic source classifies every term");
        let full = native(true);
        let report = measure(&terms, &catalog(), &Natives::new([&full])).expect("joins");
        assert_eq!(judgment_of(&report, "partOf"), Judgment::EarnedCounterpart);
        assert!(report.every_term_is_covered_or_decided());

        let classes_only = native(false);
        let report = measure(&terms, &catalog(), &Natives::new([&classes_only])).expect("joins");
        assert_eq!(judgment_of(&report, "Thing"), Judgment::EarnedCounterpart);
        assert_eq!(
            judgment_of(&report, "partOf"),
            Judgment::ValidationOnlyMatch
        );
        assert!(!report.every_term_is_covered_or_decided());

        let cells = catalog();
        let report = measure(&terms, &cells[..1], &Natives::new([&full])).expect("joins");
        assert_eq!(judgment_of(&report, "partOf"), Judgment::Uncovered);
    }

    /// INVARIANT: a native property that lacks a characteristic gUFO states for the term is
    /// not an earned counterpart, and the shortfall names the characteristic.
    #[test]
    fn a_missing_characteristic_prevents_an_earned_counterpart() {
        let terms = denominator(&vendored()).expect("classifies");
        let bare = observation(&[
            ("urn:n:Thing", RDF_TYPE, logic("Class")),
            ("urn:n:Thing", GRAPH_BOX_ROLE, "urn:box".to_owned()),
            ("urn:n:partOf", RDF_TYPE, logic("ObjectProperty")),
            ("urn:n:partOf", GRAPH_BOX_ROLE, "urn:box".to_owned()),
        ]);
        let report = measure(&terms, &catalog(), &Natives::new([&bare])).expect("joins");
        let property = &report.outcomes[1];
        assert_eq!(property.judgment(), Judgment::ValidationOnlyMatch);
        assert_eq!(
            property.cells[0].shortfalls,
            vec![Shortfall::MissingCharacteristics(BTreeSet::from([
                Characteristic::Transitive
            ]))]
        );
    }

    /// INVARIANT: an unsupported target claim coexists with an earned native counterpart
    /// and is reported independently; an unsupported cell on a related native source is a
    /// native replacement; and every cell keeps its own source identity.
    #[test]
    fn native_and_target_claims_are_independent_and_sources_stay_individual() {
        let terms = denominator(&vendored()).expect("classifies");
        let full = native(true);
        let replacement = observation(&[
            ("urn:n:Fluent", RDF_TYPE, logic("Class")),
            ("urn:n:Fluent", GRAPH_BOX_ROLE, "urn:box".to_owned()),
        ]);
        let cells = vec![
            cell("urn:n:Thing", "Thing", "closeMatch", "Unsupported"),
            with_loss(
                cell("urn:n:Fluent", "Thing", "relatedMatch", "Unsupported"),
                "the gUFO structure is not emitted",
            ),
        ];
        let natives = Natives::new([&full, &replacement]);
        let report = measure(&terms, &cells, &natives).expect("joins");
        let thing = &report.outcomes[0];
        assert_eq!(thing.cells[0].judgment, Judgment::EarnedCounterpart);
        assert_eq!(thing.cells[0].target, TargetClaim::Unsupported);
        assert_eq!(thing.cells[1].judgment, Judgment::NativeReplacement);
        assert_eq!(thing.judgment(), Judgment::EarnedCounterpart);
        let grouped = report.targets_by_source();
        assert_eq!(grouped.len(), 2);
        assert!(grouped["urn:n:Fluent"].contains(iri("Thing").as_str()));
    }

    /// INVARIANT: a related match is a native replacement only when its cell states the
    /// projection loss. Without a stated loss, or with a blank one, the same native evidence
    /// is a validation-only match that names the missing loss.
    #[test]
    fn a_replacement_without_a_stated_loss_is_not_a_replacement() {
        let terms = denominator(&vendored()).expect("classifies");
        let full = native(true);
        let natives = Natives::new([&full]);
        let related = cell("urn:n:Thing", "Thing", "relatedMatch", "Unsupported");
        let judged = |cell: Correspondence| {
            let report = measure(&terms, &[cell], &natives).expect("joins");
            report.outcomes[0].cells[0].clone()
        };

        let unstated = judged(related.clone());
        assert_eq!(unstated.judgment, Judgment::ValidationOnlyMatch);
        assert_eq!(unstated.shortfalls, vec![Shortfall::NoStatedLoss]);
        assert!(!unstated.judgment.covers());

        let blank = judged(with_loss(related.clone(), "  "));
        assert_eq!(blank.judgment, Judgment::ValidationOnlyMatch);
        assert_eq!(blank.shortfalls, vec![Shortfall::NoStatedLoss]);

        let stated = judged(with_loss(related, "the gUFO structure is not emitted"));
        assert_eq!(stated.judgment, Judgment::NativeReplacement);
        assert!(stated.shortfalls.is_empty());
        assert!(stated.judgment.covers());
    }

    /// INVARIANT: the target claim is reported beside the native judgment and never changes it.
    /// A broad match with full native evidence is the same validation-only match whether the
    /// generated view validates the term or leaves it unsupported.
    #[test]
    fn the_target_claim_never_changes_the_native_judgment() {
        let terms = denominator(&vendored()).expect("classifies");
        let full = native(true);
        let natives = Natives::new([&full]);
        let validated = [cell(
            "urn:n:partOf",
            "partOf",
            "broadMatch",
            "ValidationOnly",
        )];
        let unsupported = [cell("urn:n:partOf", "partOf", "broadMatch", "Unsupported")];
        let under_validation = measure(&terms, &validated, &natives).expect("joins");
        let under_unsupported = measure(&terms, &unsupported, &natives).expect("joins");
        let a = &under_validation.outcomes[1].cells[0];
        let b = &under_unsupported.outcomes[1].cells[0];
        assert_eq!(a.judgment, Judgment::ValidationOnlyMatch);
        assert_eq!(b.judgment, Judgment::ValidationOnlyMatch);
        assert_ne!(a.target, b.target);
    }

    /// INVARIANT: a term in the gUFO namespace with no recognized declaration is refused
    /// rather than silently left out of the denominator.
    #[test]
    fn an_undeclared_term_in_the_gufo_namespace_is_refused() {
        let undeclared = observation(&[
            (iri("Thing").as_str(), RDF_TYPE, owl("Class")),
            (iri("Stray").as_str(), "urn:p", owl("Class")),
        ]);
        assert_eq!(
            denominator(&undeclared),
            Err(InventoryError::Unclassified(iri("Stray")))
        );
    }
}
