// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Datatype obligations over completed world-local native stores. Definitions,
//! lists and values use the same compiler as the authored-rule fixed point.
//! Universal restrictions constrain existing values; each existential requests
//! its own witness. Undefined capacity never becomes a consistency certificate.

use super::BoundKind;
use super::native::{
    NativeBoundEvidence, NativeFamilyCompletion, NativeFamilyInput, NativeFamilyLedger,
    NativeFamilyOutcome, NativeObligationScope, NativeObstructionKind, NativeRead, NativeReadKind,
    NativeRefutationFamily, NativeSupportedClash,
};
use crate::facts::TermId;
use crate::physical::SchemaValues;
use crate::physical::{Bound, DatatypeCache, DatatypePlan, LogicalListCache, RelationStore};
use crate::reason::value::{LiteralMeaning, NativeValues};
use crate::rule_ir::Fact;
use purrdf::TermValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const SOME: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const ALL: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const QUALIFIER: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const EXACT: &str = "http://www.w3.org/2002/07/owl#cardinality";
const MIN: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const MAX: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const QUALIFIED_EXACT: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const QUALIFIED_MIN: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const QUALIFIED_MAX: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const RULE_NAME: &str = "refute:datatype-value-space";
const DEFINITION_FIELDS: &[&str] = &[
    "http://www.w3.org/2002/07/owl#onDatatype",
    "http://www.w3.org/2002/07/owl#withRestrictions",
    "http://www.w3.org/2002/07/owl#datatypeComplementOf",
    ONE_OF,
    "http://www.w3.org/2002/07/owl#intersectionOf",
    "http://www.w3.org/2002/07/owl#unionOf",
];
const RESTRICTION_FIELDS: &[&str] = &[
    PROPERTY,
    SOME,
    ALL,
    QUALIFIER,
    EXACT,
    MIN,
    MAX,
    QUALIFIED_EXACT,
    QUALIFIED_MIN,
    QUALIFIED_MAX,
];

struct Model<'a> {
    input: &'a NativeFamilyInput<'a>,
    rel: &'a RelationStore,
    datatypes: &'a mut DatatypeCache,
    lists: &'a mut LogicalListCache,
    values: &'a mut NativeValues,
    types: BTreeMap<TermId, BTreeSet<TermId>>,
    properties: BTreeSet<TermId>,
    restrictions: BTreeSet<TermId>,
    definitions: BTreeSet<TermId>,
    enumerations: BTreeSet<TermId>,
}

impl<'a> Model<'a> {
    fn new(
        input: &'a NativeFamilyInput<'a>,
        values: &'a mut SchemaValues,
        lists: &'a mut LogicalListCache,
    ) -> Self {
        let semantics = input.rel.semantics;
        let (values, datatypes) = values.parts();
        let mut model = Self {
            input,
            rel: input.rel,
            datatypes,
            lists,
            values,
            types: BTreeMap::new(),
            properties: BTreeSet::new(),
            restrictions: BTreeSet::new(),
            definitions: BTreeSet::new(),
            enumerations: BTreeSet::new(),
        };
        for fact in input.store.facts() {
            let subject = model
                .rel
                .term_id(&fact.subject)
                .expect("shared native subject");
            let object = model
                .rel
                .term_id(&fact.object)
                .expect("shared native object");
            let predicate = semantics.predicate(&fact.predicate);
            if predicate == semantics.predicate(TYPE) {
                model.types.entry(subject).or_default().insert(object);
                if let TermValue::Iri(class) = &fact.object {
                    let marker = |expected| {
                        class == expected
                            || semantics.alternate_marker(&fact.predicate, class) == Some(expected)
                    };
                    if marker(DATATYPE_PROPERTY) {
                        model.properties.insert(subject);
                    }
                }
            }
            if RESTRICTION_FIELDS
                .iter()
                .any(|field| semantics.predicate(field) == predicate)
            {
                model.restrictions.insert(subject);
            }
            if predicate == semantics.predicate(ONE_OF) {
                model.enumerations.insert(subject);
            }
            if DEFINITION_FIELDS
                .iter()
                .any(|field| semantics.predicate(field) == predicate)
            {
                model.definitions.insert(subject);
            }
        }
        model
    }

    fn objects(&self, subject: TermId, predicate: &str) -> BTreeSet<TermId> {
        let mut result = BTreeSet::new();
        let mut rows = self.rel.select_semantic(predicate, Bound::Subject(subject));
        while let Some((_, object, _, _)) = rows.next() {
            result.insert(object);
        }
        result
    }

    fn facts(&self, subject: TermId, predicate: &str) -> Vec<Fact> {
        let mut result = Vec::new();
        let mut rows = self.rel.select_semantic(predicate, Bound::Subject(subject));
        while let Some((subject, object, _, predicate)) = rows.next() {
            result.push(Fact {
                subject: self.term(subject).clone(),
                predicate: predicate.to_owned(),
                object: self.term(object).clone(),
            });
        }
        result
    }

    fn term(&self, id: TermId) -> &TermValue {
        self.rel.interner().resolve(id)
    }

    fn name(&self, id: TermId) -> String {
        match self.term(id) {
            TermValue::Iri(iri) => iri.clone(),
            other => crate::provenance::term_display(other),
        }
    }

    fn prepare(&mut self, root: TermId) -> gmeow_errors::Result<Arc<DatatypePlan>> {
        let term = self.rel.interner().resolve(root);
        let plan = self
            .datatypes
            .prepare(self.rel, term, self.lists, self.values)?;
        if !plan.model_values_admitted() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: "a datatype model contains an uninterpreted enumeration value".to_owned(),
            }));
        }
        Ok(plan)
    }

    fn inherited_restrictions(&self, seed: TermId) -> Arc<[(TermId, TermId)]> {
        let mut found = BTreeSet::new();
        let mut pending = vec![seed];
        let mut seen = BTreeSet::new();
        while let Some(class) = pending.pop() {
            if !seen.insert(class) {
                continue;
            }
            if self.restrictions.contains(&class) {
                for property in self.objects(class, PROPERTY).intersection(&self.properties) {
                    found.insert((class, *property));
                }
            }
            pending.extend(self.objects(class, SUBCLASS));
        }
        found.into_iter().collect()
    }

    fn obligations(&self) -> Vec<Obligation> {
        let mut keys = BTreeMap::<(TermId, TermId), BTreeSet<TermId>>::new();
        // Share the compact inherited restriction inventory across individuals.
        // At most 64 classes / 256 KiB of pairs survive; misses still compute the
        // same traversal. Paths are recovered only when a clash needs evidence.
        let mut inherited = BTreeMap::<TermId, Arc<[(TermId, TermId)]>>::new();
        let mut bytes = 0usize;
        for (individual, classes) in &self.types {
            for class in classes {
                let restrictions = if let Some(found) = inherited.get(class) {
                    Arc::clone(found)
                } else {
                    let found = self.inherited_restrictions(*class);
                    let payload = std::mem::size_of_val(found.as_ref());
                    if inherited.len() < 64 && payload <= 256 * 1024 - bytes {
                        bytes += payload;
                        inherited.insert(*class, Arc::clone(&found));
                    }
                    found
                };
                for (class, property) in restrictions.iter() {
                    keys.entry((*individual, *property))
                        .or_default()
                        .insert(*class);
                }
            }
        }
        for property in &self.properties {
            // Every asserted datatype-property value is an obligation, including
            // bare named ranges and the property's literal-only contract itself.
            let name = self.name(*property);
            let mut rows = self.rel.select_semantic(&name, Bound::Any);
            while let Some((subject, _, _, _)) = rows.next() {
                keys.entry((subject, *property)).or_default();
            }
        }
        keys.into_iter()
            .map(|((individual, property), restrictions)| Obligation {
                individual,
                property,
                restrictions,
            })
            .collect()
    }

    fn path(&self, individual: TermId, target: TermId) -> Vec<Fact> {
        let mut paths = BTreeMap::new();
        let mut queue = VecDeque::from([(individual, TYPE, None)]);
        while let Some((subject, predicate, parent)) = queue.pop_front() {
            let mut rows = self.rel.select_semantic(predicate, Bound::Subject(subject));
            while let Some((subject, object, _, predicate)) = rows.next() {
                if let std::collections::btree_map::Entry::Vacant(entry) = paths.entry(object) {
                    entry.insert((parent, subject, predicate));
                    if object == target {
                        let mut result = Vec::new();
                        let mut current = Some(object);
                        while let Some(object) = current {
                            let (parent, subject, predicate) = paths[&object];
                            result.push(Fact {
                                subject: self.term(subject).clone(),
                                predicate: predicate.to_owned(),
                                object: self.term(object).clone(),
                            });
                            current = parent;
                        }
                        return result;
                    }
                    queue.push_back((object, SUBCLASS, Some(object)));
                }
            }
        }
        Vec::new()
    }
}

struct Obligation {
    individual: TermId,
    property: TermId,
    restrictions: BTreeSet<TermId>,
}

impl Obligation {
    fn scope(&self, model: &Model<'_>) -> NativeObligationScope {
        NativeObligationScope::Property {
            individual: model.term(self.individual).clone(),
            property: model.term(self.property).clone(),
            restrictions: self
                .restrictions
                .iter()
                .map(|id| model.term(*id).clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }

    fn premises(
        &self,
        model: &Model<'_>,
        plans: &[Arc<DatatypePlan>],
        value: Option<&Fact>,
    ) -> Vec<Fact> {
        let mut facts = model.facts(self.property, RANGE);
        facts.extend(model.facts(self.property, TYPE));
        for node in &self.restrictions {
            facts.extend(model.path(self.individual, *node));
            for field in RESTRICTION_FIELDS {
                facts.extend(model.facts(*node, field));
            }
        }
        for plan in plans {
            facts.extend(plan.premises.iter().cloned());
        }
        facts.extend(value.cloned());
        facts.sort_by_key(Fact::key);
        facts.dedup();
        facts
    }

    fn clash(
        &self,
        model: &Model<'_>,
        plans: &[Arc<DatatypePlan>],
        value: Option<&Fact>,
        output: &mut NativeFamilyOutcome,
        ledger: &mut NativeFamilyLedger,
    ) -> gmeow_errors::Result<()> {
        output.conclusions.push(NativeSupportedClash {
            committed: None,
            subject: model.term(self.individual).clone(),
            rule: RULE_NAME.to_owned(),
            support: model
                .input
                .support(&self.premises(model, plans, value), ledger)?,
        });
        Ok(())
    }

    fn evaluate(
        &self,
        model: &mut Model<'_>,
        ledger: &mut NativeFamilyLedger,
        definitions_ready: bool,
    ) -> gmeow_errors::Result<NativeFamilyOutcome> {
        let mut output =
            NativeFamilyOutcome::new(NativeRefutationFamily::Datatype, self.scope(model));
        let mut universal = BTreeSet::new();
        for field in model.facts(self.property, RANGE) {
            if model.input.admit_selector(&field, &mut output, ledger)? {
                universal.insert(model.rel.term_id(&field.object).expect("native range term"));
            }
        }
        let mut requests = Vec::<(u128, Option<TermId>, BoundKind)>::new();
        let mut upper = false;
        let mut exact_limits = Vec::new();
        for node in &self.restrictions {
            let fields: Vec<_> = RESTRICTION_FIELDS
                .iter()
                .flat_map(|field| model.facts(*node, field))
                .collect();
            if model.objects(*node, PROPERTY).len() != 1 {
                output.obstruct(
                    NativeObstructionKind::SourceShape,
                    "a datatype restriction requires one explicit property",
                    model.input.support(&fields, ledger)?,
                );
            }
            let mut admitted = true;
            for field in &fields {
                admitted &= model.input.admit_selector(field, &mut output, ledger)?;
            }
            if !admitted {
                continue;
            }
            universal.extend(model.objects(*node, ALL));
            for space in model.objects(*node, SOME) {
                requests.push((1, Some(space), BoundKind::Min));
            }
            let qualifiers = model.objects(*node, QUALIFIER);
            for (predicate, qualified, kind) in [
                (EXACT, false, BoundKind::Exact),
                (MIN, false, BoundKind::Min),
                (MAX, false, BoundKind::Max),
                (QUALIFIED_EXACT, true, BoundKind::Exact),
                (QUALIFIED_MIN, true, BoundKind::Min),
                (QUALIFIED_MAX, true, BoundKind::Max),
            ] {
                for fact in model.facts(*node, predicate) {
                    let count = model.values.cardinality(&fact.object);
                    let qualifier = if qualified && qualifiers.len() == 1 {
                        qualifiers.first().copied()
                    } else {
                        None
                    };
                    let support = model.input.support(std::slice::from_ref(&fact), ledger)?;
                    output.bounds.push(NativeBoundEvidence {
                        owner: fact.subject.clone(),
                        predicate: fact.predicate.clone(),
                        value: fact.object.clone(),
                        kind,
                        interpreted: count,
                        qualifier: qualifier.map(|id| model.term(id).clone()),
                        capacity: None,
                        support: support.clone(),
                    });
                    let Some(count) = count else {
                        output.obstruct(
                            NativeObstructionKind::UnsupportedValue,
                            "a datatype bound requires an admitted non-negative integer",
                            support,
                        );
                        continue;
                    };
                    if qualified && qualifiers.len() != 1 {
                        output.obstruct(
                            NativeObstructionKind::SourceShape,
                            "a qualified datatype bound requires one explicit onDataRange",
                            model.input.support(&fields, ledger)?,
                        );
                        continue;
                    }
                    if kind == BoundKind::Max {
                        upper = true;
                        continue;
                    }
                    if kind == BoundKind::Exact {
                        exact_limits.push((count, qualifier));
                    }
                    requests.push((count, qualifier, kind));
                }
            }
        }
        if !definitions_ready {
            output.finish(model.input, reads(), ledger);
            return Ok(output);
        }
        let mut plans = Vec::new();
        for root in &universal {
            let fields = DEFINITION_FIELDS
                .iter()
                .flat_map(|field| model.facts(*root, field))
                .collect::<Vec<_>>();
            let mut admitted = true;
            for field in &fields {
                admitted &= model.input.admit_selector(field, &mut output, ledger)?;
            }
            if !admitted {
                continue;
            }
            if !ledger.charge(1) {
                break;
            }
            match model.prepare(*root) {
                Ok(plan) => plans.push(plan),
                Err(error) => output.obstruct(
                    NativeObstructionKind::IncompleteDefinition,
                    error.message(),
                    model.input.support(
                        &DEFINITION_FIELDS
                            .iter()
                            .flat_map(|field| model.facts(*root, field))
                            .collect::<Vec<_>>(),
                        ledger,
                    )?,
                ),
            }
        }
        let property = model.name(self.property);
        let values = model.facts(self.individual, &property);
        for fact in &values {
            if !ledger.charge(1) {
                break;
            }
            if !matches!(fact.object, TermValue::Literal { .. }) {
                self.clash(model, &plans, Some(fact), &mut output, ledger)?;
                continue;
            }
            let interpreted = model.values.literal(&fact.object);
            if matches!(interpreted.meaning, LiteralMeaning::Opaque(_)) {
                output.obstruct(
                    NativeObstructionKind::UnsupportedValue,
                    "an asserted datatype value has no admitted interpretation",
                    model.input.support(std::slice::from_ref(fact), ledger)?,
                );
            }
            for plan in &plans {
                if !ledger.charge(1) {
                    break;
                }
                match plan.contains_value(&interpreted) {
                    Some(true) => {}
                    Some(false) => self.clash(
                        model,
                        std::slice::from_ref(plan),
                        Some(fact),
                        &mut output,
                        ledger,
                    )?,
                    None => output.obstruct(
                        NativeObstructionKind::UnsupportedValue,
                        "literal membership in the selected value space is undecidable",
                        model.input.support(&plan.premises, ledger)?,
                    ),
                }
            }
        }
        for (count, qualifier, _) in &requests {
            for (limit, limited) in &exact_limits {
                if !ledger.charge(1) {
                    break;
                }
                if count > limit
                    && (limited.is_none()
                        || limited == qualifier
                        || limited.is_some_and(|root| universal.contains(&root)))
                {
                    self.clash(model, &plans, None, &mut output, ledger)?;
                }
            }
        }
        if !exact_limits.is_empty()
            && requests
                .iter()
                .map(|(_, qualifier, _)| *qualifier)
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        {
            output.obstruct(NativeObstructionKind::UnsupportedCombination, "interacting qualified existence and exact bounds require a joint value-space model",
                model.input.support(&self.premises(model, &plans, None), ledger)?);
        }
        for (count, qualifier, _) in requests {
            if !ledger.charge(1) {
                break;
            }
            if count == 0 {
                continue;
            }
            let mut selected = plans.clone();
            if let Some(root) = qualifier
                && !universal.contains(&root)
            {
                let fields = DEFINITION_FIELDS
                    .iter()
                    .flat_map(|field| model.facts(root, field))
                    .collect::<Vec<_>>();
                let mut admitted = true;
                for field in &fields {
                    admitted &= model.input.admit_selector(field, &mut output, ledger)?;
                }
                if !admitted {
                    continue;
                }
                match model.prepare(root) {
                    Ok(plan) => selected.push(plan),
                    Err(error) => {
                        output.obstruct(
                            NativeObstructionKind::IncompleteDefinition,
                            error.message(),
                            model
                                .input
                                .support(&self.premises(model, &plans, None), ledger)?,
                        );
                        continue;
                    }
                }
            }
            let mut capacity = selected.is_empty().then_some(true);
            for plan in &selected {
                match plan.admits_count(count) {
                    Some(false) => capacity = Some(false),
                    Some(true) if selected.len() == 1 => capacity = Some(true),
                    _ => {}
                }
            }
            match capacity {
                Some(false) => self.clash(model, &selected, None, &mut output, ledger)?,
                Some(true) => {}
                None => output.obstruct(
                    NativeObstructionKind::UnsupportedCombination,
                    "capacity of the selected datatype intersection is undecidable",
                    model
                        .input
                        .support(&self.premises(model, &selected, None), ledger)?,
                ),
            }
        }
        if upper || (!exact_limits.is_empty() && !values.is_empty()) {
            output.obstruct(NativeObstructionKind::UnsupportedCombination, "datatype upper-bound consistency requires the complete native cardinality obligation",
                model.input.support(&self.premises(model, &plans, None), ledger)?);
        }
        output.finish(model.input, reads(), ledger);
        Ok(output)
    }
}

/// Cached datatype meanings require completed constructor/facet/list writers.
/// Final model admission additionally waits for every possible property writer;
/// current source presence cannot trim that dynamic dependency.
pub(crate) fn preparation_reads() -> Vec<NativeRead> {
    SchemaValues::datatype_reads()
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

pub(crate) fn analyze(
    input: &NativeFamilyInput<'_>,
    values: &mut SchemaValues,
    lists: &mut LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    let definition_reads = preparation_reads();
    let ready = definition_reads.iter().all(|read| input.completed(read));
    let mut model = Model::new(input, values, lists);
    let obligations = model.obligations();
    let mut world = NativeFamilyOutcome::new(
        NativeRefutationFamily::Datatype,
        NativeObligationScope::World,
    );
    if obligations.is_empty() && model.definitions.is_empty() && model.enumerations.is_empty() {
        world.completion = NativeFamilyCompletion::NotEngaged;
    }
    world.finish(input, reads(), ledger);
    ledger.outcomes.push(world);
    for root in model.definitions.clone() {
        let mut outcome = NativeFamilyOutcome::new(
            NativeRefutationFamily::Datatype,
            NativeObligationScope::Definition {
                owner: model.term(root).clone(),
            },
        );
        let fields = DEFINITION_FIELDS
            .iter()
            .flat_map(|field| model.facts(root, field))
            .collect::<Vec<_>>();
        let mut admitted = true;
        for field in &fields {
            admitted &= input.admit_selector(field, &mut outcome, ledger)?;
        }
        if ready && admitted && ledger.charge(1) {
            if let Err(error) = model.prepare(root) {
                let fields = DEFINITION_FIELDS
                    .iter()
                    .flat_map(|field| model.facts(root, field))
                    .collect::<Vec<_>>();
                outcome.obstruct(
                    NativeObstructionKind::IncompleteDefinition,
                    error.message(),
                    input.support(&fields, ledger)?,
                );
            }
        }
        outcome.finish(input, definition_reads.clone(), ledger);
        ledger.outcomes.push(outcome);
    }
    for obligation in obligations {
        let outcome = obligation.evaluate(&mut model, ledger, ready)?;
        ledger.outcomes.push(outcome);
    }
    Ok(())
}

#[cfg(test)]
#[path = "datatype/tests.rs"]
mod tests;
