// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit operation-contract projection; complete generated shapes own admission.
//!
//! This is not a whole-ontology native import. It selects named callable, AST,
//! named subclass and scalar-frame facts required by the arithmetic proof. Anonymous
//! declarative class restrictions and unrelated source/annotation structures remain
//! owned by the authenticated complete generated validation products. A required
//! selected edge with an anonymous/quoted object fails; it is never dropped.

use super::{G, L, M, TYPE, failure};
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, TermRef,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Records(
    pub BTreeMap<(String, String), Vec<Term>>,
    BTreeSet<(String, String)>,
);

fn execution_predicate(predicate: &str) -> bool {
    if [
        TYPE,
        gmeow_ns::RDFS_SUB_CLASS_OF,
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
    ]
    .contains(&predicate)
    {
        return true;
    }
    [
        (
            L,
            &[
                "subClassOf",
                "inputParameters",
                "outputParameters",
                "parameterPredicate",
                "parameterValueType",
                "parameterRequired",
            ][..],
        ),
        (
            M,
            &[
                "argumentSlot",
                "slotExpression",
                "slotIndex",
                "operator",
                "hasMathematicalSymbol",
                "operatorDomain",
                "operatorCodomain",
            ][..],
        ),
        (
            G,
            &[
                "calculationExpression",
                "determinacyModel",
                "dimensionCount",
                "frameKind",
                "frameRealm",
                "hasAxis",
                "requiresHost",
                "currencyCode",
            ][..],
        ),
    ]
    .iter()
    .any(|(namespace, names)| {
        names
            .iter()
            .any(|name| predicate.strip_prefix(*namespace) == Some(*name))
    })
}

impl Records {
    pub(super) fn read(dataset: &RdfDataset, owner: bool) -> gmeow_errors::Result<Self> {
        let mut records = Self::default();
        for quad in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
            if !owner && quad.g.is_some() {
                return Err(failure(
                    "finance profile requires one explicit default-graph source",
                ));
            }
            let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
                (dataset.resolve(quad.s), dataset.resolve(quad.p))
            else {
                // Only named operation records are selected. Blank restriction
                // bodies and quoted statement metadata belong to the full owner
                // projections, not this explicitly bounded arithmetic program.
                if owner {
                    continue;
                }
                return Err(failure(
                    "finance inputs must have named subjects and predicates",
                ));
            };
            if owner && !execution_predicate(predicate) {
                continue;
            }
            let object = match dataset.resolve(quad.o) {
                TermRef::Iri(value) => Term::Iri(value.into()),
                TermRef::Literal {
                    lexical,
                    datatype,
                    language,
                    direction,
                } => {
                    let TermRef::Iri(datatype) = dataset.resolve(datatype) else {
                        unreachable!("RDF datatype is IRI")
                    };
                    Term::Literal(RdfLiteral {
                        lexical_form: lexical.into(),
                        datatype: Some(datatype.into()),
                        language: language.map(str::to_owned),
                        direction,
                    })
                }
                _ if owner => {
                    // Blank class restrictions are deliberately not named parent
                    // premises; complete owner shapes enforce their obligations.
                    if !gmeow_ns::SUB_CLASS_OF.contains(&predicate) {
                        records.1.insert((subject.into(), predicate.into()));
                    }
                    continue;
                }
                _ => {
                    return Err(failure(
                        "finance input records require named references or RDF literals",
                    ));
                }
            };
            records.insert(subject, predicate, object);
        }
        Ok(records)
    }
    pub(super) fn insert(&mut self, subject: &str, predicate: &str, value: Term) {
        let values = self
            .0
            .entry((subject.into(), predicate.into()))
            .or_default();
        if !values.contains(&value) {
            values.push(value);
            values.sort();
        }
    }
    pub(super) fn values(&self, subject: &str, predicate: &str) -> &[Term] {
        self.0
            .get(&(subject.into(), predicate.into()))
            .map_or(&[], Vec::as_slice)
    }
    pub(super) fn one(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<Term> {
        if self.1.contains(&(subject.into(), predicate.into())) {
            return Err(failure(format!(
                "required selected owner field {subject} {predicate} has unsupported anonymous or quoted object; named operation records are mandatory"
            )));
        }
        match self.values(subject, predicate) {
            [value] => Ok(value.clone()),
            values => Err(failure(format!(
                "{subject} requires exactly one {predicate}; found {}",
                values.len()
            ))),
        }
    }
    pub(super) fn reference(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<String> {
        match self.one(subject, predicate)? {
            Term::Iri(value) => Ok(value),
            _ => Err(failure(format!(
                "{subject}: {predicate} requires a named reference"
            ))),
        }
    }
    pub(super) fn typed(&self, subject: &str, class: &str) -> bool {
        self.values(subject, TYPE)
            .contains(&Term::Iri(class.into()))
    }
    pub(super) fn require_type(&self, subject: &str, class: &str) -> gmeow_errors::Result<()> {
        if !self.typed(subject, class) {
            return Err(failure(format!("{subject} requires explicit type {class}")));
        }
        Ok(())
    }
    pub(super) fn list(&self, head: &str) -> gmeow_errors::Result<Vec<String>> {
        let mut next = head.to_owned();
        let mut seen = BTreeSet::new();
        let mut values = vec![];
        while next != "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil" {
            if !seen.insert(next.clone()) {
                return Err(failure("cyclic callable parameter list"));
            }
            values.push(self.reference(&next, "http://www.w3.org/1999/02/22-rdf-syntax-ns#first")?);
            next = self.reference(&next, "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest")?;
        }
        Ok(values)
    }
    pub(super) fn admit_owner(
        &mut self,
        owner: &Self,
        roots: impl IntoIterator<Item = String>,
    ) -> gmeow_errors::Result<()> {
        let follows = [
            TYPE.to_owned(),
            format!("{L}subClassOf"),
            gmeow_ns::RDFS_SUB_CLASS_OF.into(),
            format!("{L}inputParameters"),
            format!("{L}outputParameters"),
            format!("{L}parameterPredicate"),
            format!("{L}parameterValueType"),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first".into(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest".into(),
            format!("{M}argumentSlot"),
            format!("{M}slotExpression"),
            format!("{M}operator"),
            format!("{M}hasMathematicalSymbol"),
            format!("{M}operatorDomain"),
            format!("{M}operatorCodomain"),
            format!("{G}calculationExpression"),
            format!("{G}determinacyModel"),
            format!("{G}frameKind"),
            format!("{G}frameRealm"),
            format!("{G}hasAxis"),
        ];
        let mut pending: Vec<_> = roots.into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(subject) = pending.pop() {
            if !seen.insert(subject.clone()) {
                continue;
            }
            if let Some((_, predicate)) = owner.1.iter().find(|(s, _)| s == &subject) {
                return Err(failure(format!(
                    "required selected owner record {subject} {predicate} has unsupported anonymous or quoted object"
                )));
            }
            if owner.0.keys().any(|(s, _)| s == &subject) {
                for ((s, p), values) in self.0.range((subject.clone(), String::new())..) {
                    if s != &subject {
                        break;
                    }
                    if values
                        .iter()
                        .any(|value| !owner.values(s, p).contains(value))
                    {
                        return Err(failure(format!(
                            "source changes authenticated owner record {s} {p}"
                        )));
                    }
                }
            }
            for ((s, predicate), values) in owner.0.range((subject.clone(), String::new())..) {
                if s != &subject {
                    break;
                }
                let existing = self.values(s, predicate);
                if !existing.is_empty() && existing.iter().any(|value| !values.contains(value)) {
                    return Err(failure(format!(
                        "source overrides authenticated owner record {s} {predicate}"
                    )));
                }
                for value in values {
                    if follows.contains(predicate)
                        && let Term::Iri(iri) = value
                    {
                        pending.push(iri.clone());
                    }
                    self.insert(s, predicate, value.clone());
                }
            }
        }
        Ok(())
    }
    pub(super) fn premises(&self) -> Vec<Formula> {
        self.0
            .iter()
            .flat_map(|((s, p), values)| {
                values.iter().map(move |value| Formula::Atom {
                    relation: Term::Iri(p.clone()),
                    args: vec![Term::Iri(s.clone()), value.clone()],
                })
            })
            .collect()
    }
    pub(super) fn dataset(
        &self,
        validation_view: bool,
    ) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
        let mut builder = RdfDatasetBuilder::new();
        for ((subject, predicate), values) in &self.0 {
            for value in values {
                let object = match value {
                    Term::Iri(iri) => RdfTerm::iri(iri),
                    Term::Literal(value) => RdfTerm::literal(value.clone()),
                    _ => unreachable!("record terms are atomic"),
                };
                builder.push_owned_quad(&RdfQuad::new(
                    RdfTerm::iri(subject),
                    predicate,
                    object.clone(),
                ))?;
            }
        }
        let dataset = builder.freeze()?;
        if validation_view {
            gmeow_logic_compile::projections::reader_view::with_owl_rdfs_projection(&dataset)
        } else {
            Ok(dataset)
        }
    }
}
