// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit operation-contract projection; complete generated shapes own admission.
//!
//! This is not a whole-ontology native import. It selects named callable, AST,
//! named subclass and scalar-frame facts required by the arithmetic proof. Anonymous
//! declarative class restrictions and unrelated source/annotation structures remain
//! owned by the authenticated complete generated validation products. A required
//! selected edge with an anonymous/quoted object fails; it is never dropped.

use super::{G, L, M, PROFILE, TYPE};
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

struct CallableList {
    members: Vec<String>,
    cells: BTreeSet<String>,
}

fn named_list_term(
    dataset: &RdfDataset,
    term: purrdf::TermId,
    head: &str,
    role: &str,
) -> gmeow_errors::Result<String> {
    match dataset.resolve(term) {
        TermRef::Iri(iri) => Ok(iri.to_owned()),
        _ => Err(crate::error::profile_result(
            PROFILE,
            format!("callable list {head} requires named {role}"),
        )),
    }
}

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
                return Err(crate::error::profile_result(
                    PROFILE,
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
                return Err(crate::error::profile_result(
                    PROFILE,
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
                    return Err(crate::error::profile_result(
                        PROFILE,
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
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "required selected owner field {subject} {predicate} has unsupported anonymous or quoted object; named operation records are mandatory"
                ),
            ));
        }
        match self.values(subject, predicate) {
            [value] => Ok(value.clone()),
            values => Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "{subject} requires exactly one {predicate}; found {}",
                    values.len()
                ),
            )),
        }
    }
    pub(super) fn reference(&self, subject: &str, predicate: &str) -> gmeow_errors::Result<String> {
        match self.one(subject, predicate)? {
            Term::Iri(value) => Ok(value),
            _ => Err(crate::error::profile_result(
                PROFILE,
                format!("{subject}: {predicate} requires a named reference"),
            )),
        }
    }
    pub(super) fn typed(&self, subject: &str, class: &str) -> bool {
        self.values(subject, TYPE)
            .contains(&Term::Iri(class.into()))
    }
    pub(super) fn require_type(&self, subject: &str, class: &str) -> gmeow_errors::Result<()> {
        if !self.typed(subject, class) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("{subject} requires explicit type {class}"),
            ));
        }
        Ok(())
    }
    pub(super) fn list(&self, head: &str) -> gmeow_errors::Result<Vec<String>> {
        Ok(self.callable_list(head)?.members)
    }
    fn callable_list(&self, head: &str) -> gmeow_errors::Result<CallableList> {
        let dataset = self.dataset(false)?;
        let head_id = dataset
            .term_id_if_ready(&purrdf::TermValue::iri(head))
            .ok_or_else(|| {
                crate::error::profile_result(PROFILE, format!("missing named callable list {head}"))
            })?;
        let first = dataset.term_id_if_ready(&purrdf::TermValue::iri(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
        ));
        let rest = dataset.term_id_if_ready(&purrdf::TermValue::iri(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
        ));
        let nil = dataset.term_id_if_ready(&purrdf::TermValue::iri(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        ));
        let mut visited = BTreeSet::new();
        // The same strict walker used by DatasetView owns both membership and
        // cell traversal. Retain its visited cells to import complete source
        // records/proof premises, without a second first/rest traversal.
        let members = purrdf_core::walk_rdf_list(
            head_id,
            nil,
            |cell| {
                visited.insert(cell);
                dataset.sole_object(cell, first, GraphMatch::Default)
            },
            |cell| dataset.sole_object(cell, rest, GraphMatch::Default),
        )
        .map_err(|error| {
            crate::error::profile_result(
                PROFILE,
                format!("malformed callable list {head}: {error}"),
            )
        })?;
        let cells = visited
            .into_iter()
            .map(|cell| named_list_term(&dataset, cell, head, "cells"))
            .collect::<gmeow_errors::Result<BTreeSet<_>>>()?;
        for cell in &cells {
            self.require_supported_record(cell)?;
        }
        Ok(CallableList {
            members: members
                .into_iter()
                .map(|member| named_list_term(&dataset, member, head, "members"))
                .collect::<gmeow_errors::Result<_>>()?,
            cells,
        })
    }
    fn require_supported_record(&self, subject: &str) -> gmeow_errors::Result<()> {
        if let Some((_, predicate)) = self.1.iter().find(|(s, _)| s == subject) {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "required selected owner record {subject} {predicate} has unsupported anonymous or quoted object"
                ),
            ));
        }
        Ok(())
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
            format!("{L}parameterPredicate"),
            format!("{L}parameterValueType"),
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
        let mut selected_lists = BTreeSet::new();
        while let Some(subject) = pending.pop() {
            if !seen.insert(subject.clone()) {
                continue;
            }
            owner.require_supported_record(&subject)?;
            if owner.0.keys().any(|(s, _)| s == &subject) {
                for ((s, p), values) in self.0.range((subject.clone(), String::new())..) {
                    if s != &subject {
                        break;
                    }
                    if values
                        .iter()
                        .any(|value| !owner.values(s, p).contains(value))
                    {
                        return Err(crate::error::profile_result(
                            PROFILE,
                            format!("source changes authenticated owner record {s} {p}"),
                        ));
                    }
                }
            }
            for ((s, predicate), values) in owner.0.range((subject.clone(), String::new())..) {
                if s != &subject {
                    break;
                }
                let existing = self.values(s, predicate);
                if !existing.is_empty() && existing.iter().any(|value| !values.contains(value)) {
                    return Err(crate::error::profile_result(
                        PROFILE,
                        format!("source overrides authenticated owner record {s} {predicate}"),
                    ));
                }
                for value in values {
                    if let Term::Iri(iri) = value {
                        if predicate == &format!("{L}inputParameters")
                            || predicate == &format!("{L}outputParameters")
                        {
                            let list = owner.callable_list(iri)?;
                            selected_lists.insert(iri.clone());
                            pending.extend(list.cells);
                            pending.extend(list.members);
                        } else if follows.contains(predicate) {
                            pending.push(iri.clone());
                        }
                    }
                    self.insert(s, predicate, value.clone());
                }
            }
        }
        // Validate the effective selected lists as well as the authenticated
        // owner lists: source assertions must not change even rdf:nil, whose
        // canonical empty record may otherwise have no owner subject row.
        for head in selected_lists {
            self.callable_list(&head)?;
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
