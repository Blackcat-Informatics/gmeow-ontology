// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native cardinality, identity and self-restriction obligations. Equality and
//! inverse/functional consequences come exclusively from shared schema producers.
//! Every positive conflict retains actual source or committed native support.

use super::BoundKind;
use super::native::{
    NativeBoundEvidence, NativeFamilyCompletion, NativeFamilyInput, NativeFamilyLedger,
    NativeFamilyOutcome, NativeObligationScope, NativeObstructionKind, NativeRead, NativeReadKind,
    NativeRefutationFamily, NativeSupportedClash,
};
use crate::facts::TermId;
use crate::physical::SchemaValues;
use crate::physical::{Bound, LogicalListCache};
use crate::rule_ir::Fact;
use purrdf::TermValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_HAS_SELF: &str = "http://www.w3.org/2002/07/owl#hasSelf";

const OWL_INVERSE_FUNCTIONAL_PROPERTY: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const OWL_DIFFERENT_FROM: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const OWL_ALL_DIFFERENT: &str = "http://www.w3.org/2002/07/owl#AllDifferent";

const RULE_CARDINALITY: &str = "refute:counting-cardinality";
const RULE_IDENTITY: &str = "refute:counting-identity";
const RULE_HAS_SELF: &str = "refute:counting-has-self";

const QUALIFIED: &[(&str, BoundKind)] = &[
    (
        "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
        BoundKind::Min,
    ),
    (
        "http://www.w3.org/2002/07/owl#maxQualifiedCardinality",
        BoundKind::Max,
    ),
    (
        "http://www.w3.org/2002/07/owl#qualifiedCardinality",
        BoundKind::Exact,
    ),
];

struct Model<'a> {
    input: &'a NativeFamilyInput<'a>,
}
impl Model<'_> {
    fn term(&self, id: TermId) -> &TermValue {
        self.input.rel.interner().resolve(id)
    }
    fn facts(&self, predicate: &str, bound: Bound) -> Vec<Fact> {
        let mut rows = self.input.rel.select_semantic(predicate, bound);
        let mut found = Vec::new();
        while let Some((subject, object, _, predicate)) = rows.next() {
            found.push(Fact {
                subject: self.term(subject).clone(),
                predicate: predicate.to_owned(),
                object: self.term(object).clone(),
            });
        }
        found.sort_by_key(Fact::key);
        found.dedup();
        found
    }
    fn id(&self, term: &TermValue) -> TermId {
        self.input.rel.term_id(term).expect("stored native term")
    }
    fn objects(&self, subject: TermId, predicate: &str) -> BTreeSet<TermId> {
        self.facts(predicate, Bound::Subject(subject))
            .iter()
            .map(|fact| self.id(&fact.object))
            .collect()
    }
    fn marker(&self, fact: &Fact, marker: &str) -> bool {
        fact.object.as_iri().is_some_and(|iri| {
            iri == marker
                || self
                    .input
                    .rel
                    .semantics
                    .alternate_marker(&fact.predicate, iri)
                    == Some(marker)
        })
    }
    fn restrictions(&self) -> BTreeSet<TermId> {
        [OWL_MIN_CARDINALITY, OWL_MAX_CARDINALITY, OWL_CARDINALITY]
            .into_iter()
            .chain(QUALIFIED.iter().map(|(p, _)| *p))
            .flat_map(|predicate| self.facts(predicate, Bound::Any))
            .map(|fact| self.id(&fact.subject))
            .collect()
    }
    fn source_fields(&self, node: TermId) -> Vec<Fact> {
        [
            OWL_ON_PROPERTY,
            OWL_ON_CLASS,
            OWL_ON_DATA_RANGE,
            OWL_MIN_CARDINALITY,
            OWL_MAX_CARDINALITY,
            OWL_CARDINALITY,
        ]
        .into_iter()
        .chain(QUALIFIED.iter().map(|(p, _)| *p))
        .flat_map(|p| self.facts(p, Bound::Subject(node)))
        .collect()
    }
    fn reach(
        &self,
        seed: TermId,
        lists: &mut LogicalListCache,
        output: &mut NativeFamilyOutcome,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<BTreeMap<TermId, Vec<Fact>>> {
        let mut paths = BTreeMap::new();
        let mut queue = VecDeque::from([(seed, Vec::new())]);
        while let Some((node, path)) = queue.pop_front() {
            if paths.contains_key(&node) {
                continue;
            }
            if !ledger.charge(1) {
                break;
            }
            paths.insert(node, path.clone());
            for fact in self
                .facts(RDFS_SUBCLASSOF, Bound::Subject(node))
                .into_iter()
                .chain(self.facts(OWL_EQUIVALENT_CLASS, Bound::Subject(node)))
            {
                let mut next = path.clone();
                next.push(fact.clone());
                queue.push_back((self.id(&fact.object), next));
            }
            for fact in self.facts(OWL_EQUIVALENT_CLASS, Bound::Object(node)) {
                let mut next = path.clone();
                next.push(fact.clone());
                queue.push_back((self.id(&fact.subject), next));
            }
            for fact in self.facts(OWL_INTERSECTION_OF, Bound::Subject(node)) {
                if !self.input.admit_selector(&fact, output, ledger)? {
                    continue;
                }
                let complete = preparation_reads()
                    .iter()
                    .all(|read| self.input.completed(read));
                if !complete {
                    continue;
                }
                match lists.read(self.input.rel, &fact.object) {
                    Ok(Some(list)) => {
                        let mut next = path.clone();
                        next.push(fact);
                        next.extend(list.premises(self.input.rel));
                        for member in &list.members {
                            queue.push_back((*member, next.clone()));
                        }
                    }
                    Ok(None) => output.obstruct(
                        NativeObstructionKind::IncompleteDefinition,
                        "a selected intersection has an incomplete native list",
                        self.input.support(&[fact], ledger)?,
                    ),
                    Err(error) => output.obstruct(
                        NativeObstructionKind::SourceShape,
                        error.message(),
                        self.input.support(&[fact], ledger)?,
                    ),
                }
            }
        }
        Ok(paths)
    }
}

/// Fixed point completion includes late unknown-property and equality writers.
/// Positive conflicts only read present support; cached list traversal additionally
/// requires completed list definitions, declared even if no list exists yet.
pub(crate) fn preparation_reads() -> Vec<NativeRead> {
    [RDF_FIRST, RDF_REST]
        .into_iter()
        .map(|predicate| NativeRead {
            marker: None,
            predicate: Some(predicate.to_owned()),
            kind: NativeReadKind::Completed,
        })
        .collect()
}

pub(crate) fn reads() -> Vec<NativeRead> {
    vec![NativeRead {
        marker: None,
        predicate: None,
        kind: NativeReadKind::Completed,
    }]
}

struct BoundRow {
    node: TermId,
    property: TermId,
    qualifier: Option<TermId>,
    kind: BoundKind,
    count: u128,
    facts: Vec<Fact>,
}
fn bounds(
    model: &Model<'_>,
    nodes: &BTreeMap<TermId, Vec<Fact>>,
    values: &mut SchemaValues,
    output: &mut NativeFamilyOutcome,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<Vec<BoundRow>> {
    let mut rows = Vec::new();
    let (values, _) = values.parts();
    for (node, path) in nodes {
        let properties = model.objects(*node, OWL_ON_PROPERTY);
        let class = model.objects(*node, OWL_ON_CLASS);
        let data = model.objects(*node, OWL_ON_DATA_RANGE);
        for (predicate, kind, qualified) in [
            (OWL_MIN_CARDINALITY, BoundKind::Min, false),
            (OWL_MAX_CARDINALITY, BoundKind::Max, false),
            (OWL_CARDINALITY, BoundKind::Exact, false),
        ]
        .into_iter()
        .chain(QUALIFIED.iter().map(|(p, k)| (*p, *k, true)))
        {
            for fact in model.facts(predicate, Bound::Subject(*node)) {
                let count = values.cardinality(&fact.object);
                let qualifier = if class.len() + data.len() == 1 {
                    class.first().or(data.first()).copied()
                } else {
                    None
                };
                let mut facts = path.clone();
                facts.extend(model.source_fields(*node));
                let support = model.input.support(&facts, ledger)?;
                output.bounds.push(NativeBoundEvidence {
                    owner: fact.subject.clone(),
                    predicate: fact.predicate.clone(),
                    value: fact.object.clone(),
                    kind,
                    interpreted: count,
                    qualifier: qualified
                        .then_some(qualifier)
                        .flatten()
                        .map(|id| model.term(id).clone()),
                    capacity: None,
                    support: support.clone(),
                });
                if !model.input.admit_selector(&fact, output, ledger)? {
                    continue;
                }
                let Some(count) = count else {
                    output.obstruct(
                        NativeObstructionKind::UnsupportedValue,
                        "cardinality requires an admitted non-negative integer",
                        support,
                    );
                    continue;
                };
                if properties.len() != 1
                    || !properties
                        .iter()
                        .all(|id| model.term(*id).as_iri().is_some())
                {
                    output.obstruct(
                        NativeObstructionKind::SourceShape,
                        "a cardinality restriction requires one native property IRI",
                        support,
                    );
                    continue;
                }
                if qualified && qualifier.is_none() {
                    output.obstruct(
                        NativeObstructionKind::SourceShape,
                        "a qualified cardinality requires exactly one class or datatype qualifier",
                        support,
                    );
                    continue;
                }
                let qualifier = if qualified { qualifier } else { None };
                rows.push(BoundRow {
                    node: *node,
                    property: *properties.first().expect("one property"),
                    qualifier,
                    kind,
                    count,
                    facts,
                });
            }
        }
    }
    Ok(rows)
}

fn cardinality(
    model: &Model<'_>,
    values: &mut SchemaValues,
    lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let restrictions = model.restrictions();
    let mut world = NativeFamilyOutcome::new(
        NativeRefutationFamily::Cardinality,
        NativeObligationScope::World,
    );
    if restrictions.is_empty() {
        world.completion = NativeFamilyCompletion::NotEngaged;
    }

    world.finish(model.input, reads(), ledger);
    ledger.outcomes.push(world);
    if restrictions.is_empty() {
        return Ok(());
    }
    // Every bound owner is inventoried independently of current inhabitants.
    for node in &restrictions {
        let mut output = NativeFamilyOutcome::new(
            NativeRefutationFamily::Cardinality,
            NativeObligationScope::Definition {
                owner: model.term(*node).clone(),
            },
        );
        bounds(
            model,
            &BTreeMap::from([(*node, Vec::new())]),
            values,
            &mut output,
            ledger,
        )?;
        output.finish(model.input, reads(), ledger);
        ledger.outcomes.push(output);
    }
    for assertion in model.facts(RDF_TYPE, Bound::Any) {
        let mut discovery = NativeFamilyOutcome::new(
            NativeRefutationFamily::Cardinality,
            NativeObligationScope::Definition {
                owner: assertion.object.clone(),
            },
        );
        let paths = model.reach(model.id(&assertion.object), lists, &mut discovery, ledger)?;
        if !discovery.obstructions.is_empty() {
            discovery.finish(model.input, reads(), ledger);
            ledger.outcomes.push(discovery);
        }
        let nodes: BTreeMap<_, _> = paths
            .into_iter()
            .filter(|(node, _)| restrictions.contains(node))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        let mut output = NativeFamilyOutcome::new(
            NativeRefutationFamily::Cardinality,
            NativeObligationScope::World,
        );
        let rows = bounds(model, &nodes, values, &mut output, ledger)?;
        let properties: BTreeSet<_> = rows.iter().map(|row| row.property).collect();
        for property in properties {
            let mut obligation = NativeFamilyOutcome::new(
                NativeRefutationFamily::Cardinality,
                NativeObligationScope::Property {
                    individual: assertion.subject.clone(),
                    property: model.term(property).clone(),
                    restrictions: rows
                        .iter()
                        .filter(|row| row.property == property)
                        .map(|row| model.term(row.node).clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                },
            );
            obligation.bounds = output
                .bounds
                .iter()
                .filter(|bound| {
                    nodes.keys().any(|node| {
                        model.term(*node) == &bound.owner
                            && model.objects(*node, OWL_ON_PROPERTY).contains(&property)
                    })
                })
                .cloned()
                .collect();
            // Invalid definition owners already have their exact own records.
            // A different property's malformed bound is not this property's source.

            for lower in rows
                .iter()
                .filter(|row| row.property == property && row.kind != BoundKind::Max)
            {
                for upper in rows
                    .iter()
                    .filter(|row| row.property == property && row.kind != BoundKind::Min)
                {
                    if !ledger.charge(1) {
                        break;
                    }
                    if lower.count > upper.count
                        && (upper.qualifier.is_none() || upper.qualifier == lower.qualifier)
                    {
                        let mut facts = vec![assertion.clone()];
                        facts.extend(lower.facts.iter().cloned());
                        facts.extend(upper.facts.iter().cloned());
                        obligation.conclusions.push(NativeSupportedClash {
                            committed: None,
                            subject: assertion.subject.clone(),
                            rule: RULE_CARDINALITY.to_owned(),
                            support: model.input.support(&facts, ledger)?,
                        });
                    }
                }
            }
            obligation.finish(model.input, reads(), ledger);
            ledger.outcomes.push(obligation);
        }
    }
    Ok(())
}

fn identity(
    model: &Model<'_>,
    values: &mut SchemaValues,
    _lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let mut output = NativeFamilyOutcome::new(
        NativeRefutationFamily::Identity,
        NativeObligationScope::World,
    );
    let markers = model.facts(RDF_TYPE, Bound::Any);
    let engaged = markers.iter().any(|fact| {
        [
            OWL_INVERSE_FUNCTIONAL_PROPERTY,
            OWL_FUNCTIONAL_PROPERTY,
            OWL_ALL_DIFFERENT,
        ]
        .iter()
        .any(|marker| model.marker(fact, marker))
    }) || !model.facts(OWL_SAME_AS, Bound::Any).is_empty()
        || !model.facts(OWL_DIFFERENT_FROM, Bound::Any).is_empty();
    if !engaged {
        output.completion = NativeFamilyCompletion::NotEngaged;
    } else {
        // The shared native writer owns all merges. Only existing equality rows
        // justify an equality clash here; syntactic reflexivity needs no invented row.
        for distinct in model.facts(OWL_DIFFERENT_FROM, Bound::Any) {
            if !ledger.charge(1) {
                break;
            }
            let same = model
                .facts(OWL_SAME_AS, Bound::Subject(model.id(&distinct.subject)))
                .into_iter()
                .filter(|fact| fact.object == distinct.object)
                .collect::<Vec<_>>();
            if distinct.subject == distinct.object || !same.is_empty() {
                let mut facts = same;
                facts.push(distinct.clone());
                let support = model.input.support(&facts, ledger)?;
                for subject in [&distinct.subject, &distinct.object]
                    .into_iter()
                    .collect::<BTreeSet<_>>()
                {
                    if matches!(subject, TermValue::Iri(_) | TermValue::Blank { .. }) {
                        output.conclusions.push(NativeSupportedClash {
                            committed: None,
                            subject: subject.clone(),
                            rule: RULE_IDENTITY.to_owned(),
                            support: support.clone(),
                        });
                    } else {
                        output.obstruct(
                            NativeObstructionKind::UnsupportedValue,
                            "literal identity contradiction has no resource-local membership head",
                            support.clone(),
                        );
                    }
                }
            }
        }
        // Shared list/source preparation retains each AllDifferent owner and
        // shared schema producers own its actual pairwise consequences.
        for declaration in markers
            .iter()
            .filter(|fact| model.marker(fact, OWL_ALL_DIFFERENT))
        {
            model
                .input
                .admit_selector(declaration, &mut output, ledger)?;
        }
        let (values, _) = values.parts();
        for fact in model
            .facts(OWL_SAME_AS, Bound::Any)
            .into_iter()
            .chain(model.facts(OWL_DIFFERENT_FROM, Bound::Any))
        {
            if matches!(fact.object, TermValue::Literal { .. })
                && matches!(
                    values.literal(&fact.object).meaning,
                    crate::reason::value::LiteralMeaning::Opaque(_)
                )
            {
                output.obstruct(
                    NativeObstructionKind::UnsupportedValue,
                    "identity model contains an unadmitted literal value",
                    model.input.support(std::slice::from_ref(&fact), ledger)?,
                );
            }
        }
    }
    output.finish(model.input, reads(), ledger);
    ledger.outcomes.push(output);
    Ok(())
}

fn has_self(
    model: &Model<'_>,
    values: &mut SchemaValues,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let fields = model.facts(OWL_HAS_SELF, Bound::Any);
    let mut world = NativeFamilyOutcome::new(
        NativeRefutationFamily::HasSelf,
        NativeObligationScope::World,
    );
    if fields.is_empty() {
        world.completion = NativeFamilyCompletion::NotEngaged;
    }

    world.finish(model.input, reads(), ledger);
    ledger.outcomes.push(world);
    let (values, _) = values.parts();
    for field in fields {
        let owner = model.id(&field.subject);
        let mut output = NativeFamilyOutcome::new(
            NativeRefutationFamily::HasSelf,
            NativeObligationScope::Definition {
                owner: field.subject.clone(),
            },
        );
        if !model.input.admit_selector(&field, &mut output, ledger)? {
            output.finish(model.input, reads(), ledger);
            ledger.outcomes.push(output);
            continue;
        }
        let admitted = matches!(&field.object, TermValue::Literal { .. })
            && matches!(
                values.literal(&field.object).meaning,
                crate::reason::value::LiteralMeaning::Native(purrdf::xsd::XsdValue::Boolean(true))
            );
        if !admitted {
            output.obstruct(
                NativeObstructionKind::UnsupportedValue,
                "hasSelf requires an admitted true boolean",
                model.input.support(std::slice::from_ref(&field), ledger)?,
            );
        } else {
            let properties = model.facts(OWL_ON_PROPERTY, Bound::Subject(owner));
            if properties.len() != 1 || properties[0].object.as_iri().is_none() {
                let mut support = properties.clone();
                support.push(field.clone());
                output.obstruct(
                    NativeObstructionKind::SourceShape,
                    "hasSelf requires one native property IRI",
                    model.input.support(&support, ledger)?,
                );
            } else {
                let property = &properties[0];
                let disjoints = model
                    .facts(OWL_DISJOINT_WITH, Bound::Subject(owner))
                    .into_iter()
                    .chain(model.facts(OWL_DISJOINT_WITH, Bound::Object(owner)))
                    .collect::<Vec<_>>();
                for edge in model.facts(
                    property.object.as_iri().expect("native property"),
                    Bound::Any,
                ) {
                    if edge.subject != edge.object {
                        continue;
                    }
                    for disjoint in &disjoints {
                        if !ledger.charge(1) {
                            break;
                        }
                        let class = if disjoint.subject == field.subject {
                            &disjoint.object
                        } else {
                            &disjoint.subject
                        };
                        for membership in model
                            .facts(RDF_TYPE, Bound::Subject(model.id(&edge.subject)))
                            .into_iter()
                            .filter(|fact| &fact.object == class)
                        {
                            let facts = [
                                field.clone(),
                                property.clone(),
                                disjoint.clone(),
                                edge.clone(),
                                membership,
                            ];
                            output.conclusions.push(NativeSupportedClash {
                                committed: None,
                                subject: edge.subject.clone(),
                                rule: RULE_HAS_SELF.to_owned(),
                                support: model.input.support(&facts, ledger)?,
                            });
                        }
                    }
                }
            }
        }
        output.finish(model.input, reads(), ledger);
        ledger.outcomes.push(output);
    }
    Ok(())
}

pub(crate) fn analyze_family(
    family: NativeRefutationFamily,
    input: &NativeFamilyInput<'_>,
    values: &mut SchemaValues,
    lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let model = Model { input };
    match family {
        NativeRefutationFamily::Cardinality => cardinality(&model, values, lists, ledger),
        NativeRefutationFamily::Identity => identity(&model, values, lists, ledger),
        NativeRefutationFamily::HasSelf => has_self(&model, values, ledger),
        NativeRefutationFamily::Datatype => unreachable!("datatype has its own typed producer"),
    }
}

#[path = "counting.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "counting_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::analyze;
#[cfg(test)]
use test_support::{OWL_CLASS, OWL_OBJECT_PROPERTY, OWL_RESTRICTION, XSD_BOOLEAN, XSD_STRING};
