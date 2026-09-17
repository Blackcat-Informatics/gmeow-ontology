// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Finite abstract interpretation of native statement value flow.
//!
//! Every program constant has its own cell; Other represents ALL remaining native
//! values. Input abstraction visits every fact. Relation columns retain conditional
//! supports for constants that participate in rule or protected-definition grammar,
//! preventing unrelated rows from being spliced at a variable join. Positive
//! bindings narrow head domains; dropped guards and generated variables still
//! over-approximate production. The monotone fixed point therefore covers every
//! concrete reachable statement, including effects of later strata. No input row,
//! rendered RDF or cumulative concrete closure is retained here.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use purrdf::TermValue;

use super::{ProducerEffect, StatementPattern, constant};
use crate::native_semantics::SemanticVocabulary;
use crate::rule_ir::{EvalTerm, Fact};

/// A finite set of abstract values. These are value-domain bits, not row IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Domain(Vec<u64>);

impl Domain {
    fn empty(size: usize) -> Self {
        Self(vec![0; size.div_ceil(64)])
    }
    fn all(size: usize) -> Self {
        let mut result = Self::empty(size);
        for index in 0..size {
            result.insert(index);
        }
        result
    }
    fn one(size: usize, index: usize) -> Self {
        let mut result = Self::empty(size);
        result.insert(index);
        result
    }
    fn resized(&self, size: usize) -> Self {
        let mut result = Self::empty(size);
        for index in self.indices() {
            result.insert(index);
        }
        result
    }
    fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }
    fn remove(&mut self, index: usize) {
        self.0[index / 64] &= !(1 << (index % 64));
    }
    fn contains(&self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }
    fn union(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (left, right) in self.0.iter_mut().zip(&other.0) {
            let next = *left | right;
            changed |= *left != next;
            *left = next;
        }
        changed
    }
    fn intersect(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (left, right) in self.0.iter_mut().zip(&other.0) {
            let next = *left & right;
            changed |= *left != next;
            *left = next;
        }
        changed
    }
    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(&other.0)
            .any(|(left, right)| left & right != 0)
    }
    fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }
    fn is_subset_of(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(&other.0)
            .all(|(left, right)| left & !right == 0)
    }
    fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(word, &bits)| {
            let mut remaining = bits;
            std::iter::from_fn(move || {
                if remaining == 0 {
                    return None;
                }
                let bit = remaining.trailing_zeros() as usize;
                remaining &= remaining - 1;
                Some(word * 64 + bit)
            })
        })
    }
}

/// Positive body over-approximation and the atomic heads of one native producer.
#[derive(Debug, Clone)]
pub(crate) struct FlowRule {
    pub(crate) body: Vec<[EvalTerm; 3]>,
    pub(crate) heads: Vec<[EvalTerm; 3]>,
    /// Only variables created by the native existential witness constructors.
    /// Body-bound and numeric output variables are never included.
    pub(crate) native_witnesses: Vec<String>,
    /// Explicit reads parallel to ProducerEffect::reads; None denotes an implicit
    /// structural read whose conservative pattern is already declared by the engine.
    pub(crate) reads: Vec<Option<[EvalTerm; 3]>>,
}

#[derive(Debug, Clone)]
enum Slot {
    Constant(usize),
    Variable(usize),
}

impl Slot {
    fn domain(&self, bindings: &[Domain], size: usize) -> Domain {
        match self {
            Self::Constant(index) => Domain::one(size, *index),
            Self::Variable(index) => bindings[*index].clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct Rule {
    body: Vec<[Slot; 3]>,
    heads: Vec<[Slot; 3]>,
    reads: Vec<Option<[Slot; 3]>>,
    variables: usize,
    native_witnesses: Vec<usize>,
    names: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct OperatorInput {
    predicate: Option<String>,
    column: usize,
}

fn variable(term: &EvalTerm) -> Option<&str> {
    match term {
        EvalTerm::Var(name) => Some(name),
        _ => None,
    }
}

fn atom_predicate(atom: &[EvalTerm; 3], semantics: SemanticVocabulary) -> Option<String> {
    match &atom[1] {
        EvalTerm::Var(_) => None,
        term => constant(term)
            .and_then(|value| value.as_iri().map(str::to_owned))
            .map(|predicate| semantics.predicate(&predicate).to_owned()),
    }
}

/// Backward role analysis for values that can become RDF predicates. Exact cells
/// are needed only at these source columns and for predicates actually present;
/// ordinary entity/class IRIs remain in `Other` and do not widen every bitset.
fn operator_inputs(
    rules: &[FlowRule],
    semantics: SemanticVocabulary,
) -> (Vec<OperatorInput>, Vec<StatementPattern>) {
    let mut operators: Vec<BTreeSet<String>> = rules
        .iter()
        .map(|rule| {
            rule.body
                .iter()
                .chain(&rule.heads)
                .chain(rule.reads.iter().flatten())
                .filter_map(|atom| variable(&atom[1]).map(str::to_owned))
                .collect()
        })
        .collect();
    let mut inputs = BTreeSet::new();
    loop {
        let before = (
            inputs.len(),
            operators.iter().map(BTreeSet::len).sum::<usize>(),
        );
        for (rule, variables) in rules.iter().zip(&operators) {
            for atom in &rule.body {
                for column in [0usize, 2] {
                    if variable(&atom[column]).is_some_and(|name| variables.contains(name)) {
                        inputs.insert(OperatorInput {
                            predicate: atom_predicate(atom, semantics),
                            column,
                        });
                    }
                }
            }
        }
        for (rule, variables) in rules.iter().zip(&mut operators) {
            for head in &rule.heads {
                let Some(predicate) = atom_predicate(head, semantics) else {
                    // A dynamic head can conservatively remain `Other`; expanding
                    // its arbitrary data columns into operator cells would scale
                    // every bitset to the whole corpus. Fixed selector heads carry
                    // precise operator demand backward without that materialization.
                    continue;
                };
                for input in &inputs {
                    if input
                        .predicate
                        .as_ref()
                        .is_none_or(|required| required == &predicate)
                    {
                        if let Some(name) = variable(&head[input.column]) {
                            variables.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        let after = (
            inputs.len(),
            operators.iter().map(BTreeSet::len).sum::<usize>(),
        );
        if before == after {
            break;
        }
    }
    let mut conditions = Vec::new();
    for (rule, variables) in rules.iter().zip(&operators) {
        for atom in &rule.body {
            if [0usize, 2]
                .into_iter()
                .any(|column| variable(&atom[column]).is_some_and(|name| variables.contains(name)))
            {
                conditions.push(StatementPattern::statement(atom));
            }
        }
    }
    (inputs.into_iter().collect(), conditions)
}

#[derive(Debug, Clone)]
struct Universe {
    values: Vec<TermValue>,
    ids: HashMap<TermValue, usize>,
    iris: HashMap<String, usize>,
    semantics: SemanticVocabulary,
}

impl Universe {
    fn new(semantics: SemanticVocabulary) -> Self {
        Self {
            values: Vec::new(),
            ids: HashMap::new(),
            iris: HashMap::new(),
            semantics,
        }
    }
    fn register(&mut self, value: TermValue) {
        if self.ids.contains_key(&value) {
            return;
        }
        let index = self.values.len();
        if let TermValue::Iri(iri) = &value {
            self.iris.insert(iri.clone(), index);
        }
        self.values.push(value.clone());
        self.ids.insert(value, index);
    }
    fn observe(&mut self, term: &EvalTerm) {
        if let Some(value) = constant(term) {
            if let TermValue::Iri(iri) = &value {
                let spellings: Vec<_> = self
                    .semantics
                    .possible_spellings(iri)
                    .map(TermValue::iri)
                    .collect();
                for spelling in spellings {
                    self.register(spelling);
                }
            } else {
                self.register(value);
            }
        }
    }
    fn size(&self) -> usize {
        self.values.len() + 1
    }
    fn other(&self) -> usize {
        self.values.len()
    }
    fn value(&self, value: &TermValue) -> usize {
        self.ids.get(value).copied().unwrap_or(self.other())
    }
    fn iri(&self, iri: &str) -> usize {
        self.iris.get(iri).copied().unwrap_or(self.other())
    }
    /// Every concrete existential output is a scoped Skolem or an n-ary tuple
    /// reifier. Keep Other and every known value in either constructor namespace:
    /// source-authored witness-looking IRIs can coincide with generated outputs.
    /// This restricts producer outputs only; it never equates or rewrites data.
    fn native_witness_domain(&self) -> Domain {
        let mut domain = Domain::one(self.size(), self.other());
        for (index, value) in self.values.iter().enumerate() {
            if let TermValue::Iri(iri) = value
                && (iri.starts_with(crate::facts::SKOLEM_PREFIX)
                    || iri.starts_with(crate::provenance::NARY_REIFIER_PREFIX))
            {
                domain.insert(index);
            }
        }
        domain
    }

    fn iri_values(&self, domain: &Domain) -> Domain {
        let mut result = Domain::empty(self.size());
        for index in domain.indices() {
            if index == self.other() || matches!(self.values[index], TermValue::Iri(_)) {
                result.insert(index);
            }
        }
        result
    }
    fn predicates(&self, domain: &Domain) -> Domain {
        let mut result = self.iri_values(domain);
        for index in domain.indices() {
            if let Some(TermValue::Iri(iri)) = self.values.get(index)
                && let Some(alternate) = self.semantics.alternate_predicate(iri)
            {
                result.insert(self.iri(alternate));
            }
        }
        result
    }
    fn marker(&self, value: usize, predicates: &Domain) -> Domain {
        let mut result = Domain::one(self.size(), value);
        let Some(TermValue::Iri(iri)) = self.values.get(value) else {
            return result;
        };
        for predicate in predicates.indices() {
            let operator = match self.values.get(predicate) {
                Some(TermValue::Iri(predicate)) => predicate.as_str(),
                // An unknown operator might put this constant in a marker role.
                // Broaden that possibility; never rename a variable binding.
                _ => "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            };
            if let Some(alternate) = self.semantics.alternate_marker(operator, iri) {
                result.insert(self.iri(alternate));
            }
        }
        result
    }
    fn domains(&self, atom: &[Slot; 3], bindings: &[Domain], read: bool) -> [Domain; 3] {
        let mut domains = atom
            .each_ref()
            .map(|slot| slot.domain(bindings, self.size()));
        if read {
            domains[1] = self.predicates(&domains[1]);
            if let Slot::Constant(value) = atom[2] {
                domains[2] = self.marker(value, &domains[1]);
            }
        }
        domains
    }
}

#[derive(Debug, Clone)]
struct RelationSummary {
    columns: [Domain; 2],
    /// Opposite-column supports keyed only by conditioned constants. This is a
    /// bounded grammar relation, not retained source rows.
    by_subject: BTreeMap<usize, Domain>,
    by_object: BTreeMap<usize, Domain>,
}

impl RelationSummary {
    fn empty(size: usize) -> Self {
        Self {
            columns: std::array::from_fn(|_| Domain::empty(size)),
            by_subject: BTreeMap::new(),
            by_object: BTreeMap::new(),
        }
    }
}

/// Full-input abstract columns plus grammar-constant conditional supports. Size
/// depends on the selected operator vocabulary, never the number of corpus rows.
/// The flow vocabulary identity fixes every bit meaning used by a cached summary.
#[derive(Debug, Clone)]
pub(crate) struct FlowSummary {
    relations: BTreeMap<usize, RelationSummary>,
}

impl FlowSummary {
    pub(crate) fn identity(&self, template: &[u8; 32]) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"gmeow-native-input-columns-v2\0");
        hash.update(template);
        hash.update(&(self.relations.len() as u64).to_le_bytes());
        for (predicate, relation) in &self.relations {
            hash.update(&(*predicate as u64).to_le_bytes());
            for column in &relation.columns {
                hash.update(&(column.0.len() as u64).to_le_bytes());
                for word in &column.0 {
                    hash.update(&word.to_le_bytes());
                }
            }
            for supports in [&relation.by_subject, &relation.by_object] {
                hash.update(&(supports.len() as u64).to_le_bytes());
                for (value, domain) in supports {
                    hash.update(&(*value as u64).to_le_bytes());
                    for word in &domain.0 {
                        hash.update(&word.to_le_bytes());
                    }
                }
            }
        }
        *hash.finalize().as_bytes()
    }
    fn publish_columns(&mut self, head: &[Domain; 3], universe: &Universe) -> bool {
        if head.iter().any(Domain::is_empty) {
            return false;
        }
        let mut changed = false;
        for predicate in universe.iri_values(&head[1]).indices() {
            let relation = self
                .relations
                .entry(predicate)
                .or_insert_with(|| RelationSummary::empty(universe.size()));
            changed |= relation.columns[0].union(&head[0]);
            changed |= relation.columns[1].union(&head[2]);
        }
        changed
    }

    fn publish_rectangle(
        &mut self,
        head: &[Domain; 3],
        universe: &Universe,
        conditioned: &Domain,
    ) -> bool {
        let mut changed = self.publish_columns(head, universe);
        if head.iter().any(Domain::is_empty) {
            return changed;
        }
        for predicate in universe.iri_values(&head[1]).indices() {
            let relation = self
                .relations
                .get_mut(&predicate)
                .expect("columns were published first");
            for subject in head[0]
                .indices()
                .filter(|index| conditioned.contains(*index))
            {
                changed |= relation
                    .by_subject
                    .entry(subject)
                    .or_insert_with(|| Domain::empty(universe.size()))
                    .union(&head[2]);
            }
            for object in head[2]
                .indices()
                .filter(|index| conditioned.contains(*index))
            {
                changed |= relation
                    .by_object
                    .entry(object)
                    .or_insert_with(|| Domain::empty(universe.size()))
                    .union(&head[0]);
            }
        }
        changed
    }

    fn publish_condition(
        &mut self,
        predicate: usize,
        side: usize,
        value: usize,
        opposite: &Domain,
        size: usize,
    ) -> bool {
        let relation = self
            .relations
            .entry(predicate)
            .or_insert_with(|| RelationSummary::empty(size));
        let supports = if side == 0 {
            &mut relation.by_subject
        } else {
            &mut relation.by_object
        };
        supports
            .entry(value)
            .or_insert_with(|| Domain::empty(size))
            .union(opposite)
    }
}

/// Immutable variable layouts and native value cells, shared across input shapes.
#[derive(Debug, Clone)]
pub(crate) struct ValueFlow {
    universe: Universe,
    rules: Vec<Rule>,
    native_witness_domain: Domain,
    conditioned: Domain,
    operator_inputs: Vec<OperatorInput>,
}

impl ValueFlow {
    pub(crate) fn new(
        rules: &[FlowRule],
        effects: &[ProducerEffect],
        semantics: SemanticVocabulary,
    ) -> Self {
        Self::with_observations(rules, effects, semantics, &[])
    }

    /// Retain exact protected source grammar cells in the same abstract domain;
    /// these are observations, never fake rules or asserted facts.
    pub(crate) fn with_observations(
        rules: &[FlowRule],
        effects: &[ProducerEffect],
        semantics: SemanticVocabulary,
        observed: &[StatementPattern],
    ) -> Self {
        Self::with_conditioned_observations(rules, effects, semantics, observed, observed)
    }

    /// Register every comparison constant, while retaining tuple supports only
    /// for the exact protected grammar cells supplied by the caller.
    pub(crate) fn with_conditioned_observations(
        rules: &[FlowRule],
        effects: &[ProducerEffect],
        semantics: SemanticVocabulary,
        observed: &[StatementPattern],
        conditioned_patterns: &[StatementPattern],
    ) -> Self {
        let mut universe = Universe::new(semantics);
        let (operator_inputs, operator_conditions) = operator_inputs(rules, semantics);
        for pattern in observed {
            for value in [&pattern.subject, &pattern.object].into_iter().flatten() {
                universe.observe(&EvalTerm::ConstLit(value.clone()));
            }
            if let Some(predicate) = &pattern.predicate {
                universe.observe(&EvalTerm::named(predicate));
            }
        }
        for rule in rules {
            for term in rule
                .body
                .iter()
                .chain(&rule.heads)
                .chain(rule.reads.iter().flatten())
                .flatten()
            {
                universe.observe(term);
            }
        }
        for effect in effects {
            for pattern in effect
                .writes
                .iter()
                .chain(effect.reads.iter().map(|(read, _)| read))
            {
                for value in [&pattern.subject, &pattern.object].into_iter().flatten() {
                    universe.observe(&EvalTerm::ConstLit(value.clone()));
                }
                if let Some(predicate) = &pattern.predicate {
                    universe.observe(&EvalTerm::named(predicate));
                }
            }
        }
        let rules = rules
            .iter()
            .map(|source| {
                let mut variables = BTreeMap::new();
                let mut lower = |atom: &[EvalTerm; 3]| {
                    atom.each_ref().map(|term| match term {
                        EvalTerm::Var(name) => {
                            let next = variables.len();
                            Slot::Variable(*variables.entry(name.clone()).or_insert(next))
                        }
                        _ => Slot::Constant(universe.value(&constant(term).expect("constant"))),
                    })
                };
                let body = source.body.iter().map(&mut lower).collect();
                let heads = source.heads.iter().map(&mut lower).collect();
                let reads = source
                    .reads
                    .iter()
                    .map(|read| read.as_ref().map(&mut lower))
                    .collect();
                let native_witnesses = source
                    .native_witnesses
                    .iter()
                    .map(|name| {
                        let index = *variables
                            .get(name)
                            .expect("native witness names a head variable");
                        assert!(
                            source.body.iter().flatten().all(|term| {
                                !matches!(term, EvalTerm::Var(variable) if variable == name)
                            }),
                            "a body-bound variable cannot have a generated-value domain"
                        );
                        index
                    })
                    .collect();
                Rule {
                    body,
                    heads,
                    reads,
                    variables: variables.len(),
                    native_witnesses,
                    names: variables,
                }
            })
            .collect();
        let native_witness_domain = universe.native_witness_domain();
        let mut conditioned = Domain::empty(universe.size());
        for pattern in conditioned_patterns.iter().chain(&operator_conditions) {
            if let Some(subject) = &pattern.subject {
                conditioned.insert(universe.value(subject));
            }
            let predicates = pattern.predicate.as_ref().map(|predicate| {
                universe.predicates(&Domain::one(universe.size(), universe.iri(predicate)))
            });
            if let Some(predicates) = &predicates {
                conditioned.union(predicates);
            }
            if let Some(object) = &pattern.object {
                let object = universe.value(object);
                conditioned.union(&predicates.map_or_else(
                    || Domain::one(universe.size(), object),
                    |predicates| universe.marker(object, &predicates),
                ));
            }
        }
        conditioned.remove(universe.other());
        Self {
            universe,
            rules,
            native_witness_domain,
            conditioned,
            operator_inputs,
        }
    }

    /// Extend the immutable template universe with selected producer envelopes.
    /// Existing rule slots remain valid because new cells are appended; the old
    /// `Other` cell is never stored in a rule slot. This keeps one abstract
    /// analysis precise without lowering or retaining source rows again.
    pub(crate) fn with_additional_observations<'a>(
        &self,
        observed: impl Iterator<Item = &'a StatementPattern>,
    ) -> Self {
        let mut universe = self.universe.clone();
        for pattern in observed {
            for value in [&pattern.subject, &pattern.object].into_iter().flatten() {
                universe.observe(&EvalTerm::ConstLit(value.clone()));
            }
            if let Some(predicate) = &pattern.predicate {
                universe.observe(&EvalTerm::named(predicate));
            }
        }
        let native_witness_domain = universe.native_witness_domain();
        let conditioned = self.conditioned.resized(universe.size());
        Self {
            universe,
            rules: self.rules.clone(),
            native_witness_domain,
            conditioned,
            operator_inputs: self.operator_inputs.clone(),
        }
    }

    /// Retain the sparse operator identities selected by this source. Every RDF
    /// predicate and only those resource-valued schema selectors that backward role
    /// analysis can place in predicate position receive exact cells. Folding those
    /// IRIs into `Other` would splice unrelated property rows at a variable-predicate
    /// join; ordinary resources, literals and blank nodes stay in the conservative
    /// cell rather than widening every relation bitset to the whole corpus.
    pub(crate) fn with_source_operators<'a>(&self, facts: impl Iterator<Item = &'a Fact>) -> Self {
        let mut universe = self.universe.clone();
        let mut values = BTreeSet::new();
        for fact in facts {
            values.insert(TermValue::iri(&fact.predicate));
            let predicate = self.universe.semantics.predicate(&fact.predicate);
            for input in &self.operator_inputs {
                if input
                    .predicate
                    .as_deref()
                    .is_none_or(|required| required == predicate)
                {
                    let value = if input.column == 0 {
                        &fact.subject
                    } else {
                        &fact.object
                    };
                    if matches!(value, TermValue::Iri(_)) {
                        values.insert(value.clone());
                    }
                }
            }
        }
        for value in &values {
            universe.register(value.clone());
        }
        let native_witness_domain = universe.native_witness_domain();
        let mut conditioned = self.conditioned.resized(universe.size());
        for value in values {
            conditioned.insert(universe.value(&value));
        }
        Self {
            conditioned,
            universe,
            rules: self.rules.clone(),
            native_witness_domain,
            operator_inputs: self.operator_inputs.clone(),
        }
    }

    pub(crate) fn vocabulary_identity(&self) -> [u8; 32] {
        crate::physical::metadata_identity("gmeow-native-flow-vocabulary-v1", &self.universe.values)
    }

    pub(crate) fn summarize<'a>(&self, facts: impl Iterator<Item = &'a Fact>) -> FlowSummary {
        let mut result = FlowSummary {
            relations: BTreeMap::new(),
        };
        for fact in facts {
            result.publish_rectangle(
                &[
                    Domain::one(self.universe.size(), self.universe.value(&fact.subject)),
                    Domain::one(self.universe.size(), self.universe.iri(&fact.predicate)),
                    Domain::one(self.universe.size(), self.universe.value(&fact.object)),
                ],
                &self.universe,
                &self.conditioned,
            );
        }
        result
    }

    /// Admit every possible external producer output before reachability
    /// refinement. Unknown positions cover the entire abstract universe; exact
    /// predicates and markers retain their declared roles. These are possible
    /// columns, never synthesized concrete input rows.
    pub(crate) fn seed_patterns<'a>(
        &self,
        summary: &mut FlowSummary,
        patterns: impl Iterator<Item = &'a StatementPattern>,
    ) {
        for pattern in patterns {
            summary.publish_rectangle(
                &self.pattern_domains(pattern),
                &self.universe,
                &self.conditioned,
            );
        }
    }

    /// One abstract join. Conditional supports preserve tuple correlation whenever
    /// a rule or protected-definition constant constrains either value column.
    /// Unconditioned values retain the conservative column product.
    fn bindings(&self, rule: &Rule, state: &FlowSummary) -> Option<Vec<Domain>> {
        self.bindings_seeded(rule, state, &[])
    }

    fn bindings_seeded(
        &self,
        rule: &Rule,
        state: &FlowSummary,
        seeds: &[(usize, usize)],
    ) -> Option<Vec<Domain>> {
        let size = self.universe.size();
        let mut bindings = vec![Domain::all(size); rule.variables];
        if !rule.native_witnesses.is_empty() {
            for &variable in &rule.native_witnesses {
                bindings[variable] = self.native_witness_domain.clone();
            }
        }
        for &(variable, value) in seeds {
            bindings[variable].intersect(&Domain::one(size, value));
        }
        if bindings.iter().any(Domain::is_empty) {
            return None;
        }
        loop {
            let mut changed = false;
            for atom in &rule.body {
                let required = self.universe.domains(atom, &bindings, true);
                let mut possible: [Domain; 3] = std::array::from_fn(|_| Domain::empty(size));
                for predicate in required[1].indices() {
                    let Some(relation) = state.relations.get(&predicate) else {
                        continue;
                    };
                    let mut row = [
                        relation.columns[0].clone(),
                        self.universe.predicates(&Domain::one(size, predicate)),
                        relation.columns[1].clone(),
                    ];
                    for (values, wanted) in row.iter_mut().zip(&required) {
                        values.intersect(wanted);
                    }
                    if !row[0].is_empty() && required[0].is_subset_of(&self.conditioned) {
                        let mut subjects = Domain::empty(size);
                        let mut objects = Domain::empty(size);
                        for subject in row[0].indices() {
                            let Some(support) = relation.by_subject.get(&subject) else {
                                continue;
                            };
                            let mut support = support.clone();
                            support.intersect(&row[2]);
                            if !support.is_empty() {
                                subjects.insert(subject);
                                objects.union(&support);
                            }
                        }
                        row[0] = subjects;
                        row[2] = objects;
                    }
                    if !row[2].is_empty() && required[2].is_subset_of(&self.conditioned) {
                        let mut subjects = Domain::empty(size);
                        let mut objects = Domain::empty(size);
                        for object in row[2].indices() {
                            let Some(support) = relation.by_object.get(&object) else {
                                continue;
                            };
                            let mut support = support.clone();
                            support.intersect(&row[0]);
                            if !support.is_empty() {
                                subjects.union(&support);
                                objects.insert(object);
                            }
                        }
                        row[0] = subjects;
                        row[2] = objects;
                    }
                    // Repeated variables in a single atom still denote one value.
                    for left in 0..3 {
                        for right in 0..left {
                            if let (Slot::Variable(a), Slot::Variable(b)) =
                                (&atom[left], &atom[right])
                                && a == b
                            {
                                let mut common = row[left].clone();
                                common.intersect(&row[right]);
                                row[left] = common.clone();
                                row[right] = common;
                            }
                        }
                    }
                    if row.iter().any(Domain::is_empty) {
                        continue;
                    }
                    for (possible, values) in possible.iter_mut().zip(&row) {
                        possible.union(values);
                    }
                }
                if possible.iter().any(Domain::is_empty) {
                    return None;
                }
                for (slot, values) in atom.iter().zip(&possible) {
                    if let Slot::Variable(variable) = slot {
                        changed |= bindings[*variable].intersect(values);
                    }
                }
                if bindings.iter().any(Domain::is_empty) {
                    return None;
                }
            }
            if !changed {
                return Some(bindings);
            }
        }
    }

    fn bindings_for_head_value(
        &self,
        rule: &Rule,
        head: &[Slot; 3],
        slot: usize,
        value: usize,
        state: &FlowSummary,
    ) -> Option<Vec<Domain>> {
        match head[slot] {
            Slot::Constant(constant) => (constant == value)
                .then(|| self.bindings(rule, state))
                .flatten(),
            Slot::Variable(variable) => self.bindings_seeded(rule, state, &[(variable, value)]),
        }
    }

    /// Remove only a conditioned value whose constrained abstract join is empty.
    /// Every unconditioned value keeps the ordinary conservative column bound.
    fn head_domains(
        &self,
        rule: &Rule,
        head: &[Slot; 3],
        bindings: &[Domain],
        state: &FlowSummary,
    ) -> [Domain; 3] {
        let mut domains = self.universe.domains(head, bindings, false);
        for slot in 0..3 {
            let candidates: Vec<_> = domains[slot]
                .indices()
                .filter(|value| self.conditioned.contains(*value))
                .collect();
            for value in candidates {
                if self
                    .bindings_for_head_value(rule, head, slot, value, state)
                    .is_none()
                {
                    domains[slot].remove(value);
                }
            }
        }
        domains
    }

    fn publish_head_conditions(
        &self,
        state: &mut FlowSummary,
        rule: &Rule,
        head: &[Slot; 3],
        domains: &[Domain; 3],
    ) -> bool {
        let mut changed = false;
        for side in [0usize, 2] {
            let candidates: Vec<_> = domains[side]
                .indices()
                .filter(|value| self.conditioned.contains(*value))
                .collect();
            for value in candidates {
                let Some(bindings) = self.bindings_for_head_value(rule, head, side, value, state)
                else {
                    continue;
                };
                let constrained = self.universe.domains(head, &bindings, false);
                let opposite = if side == 0 {
                    &constrained[2]
                } else {
                    &constrained[0]
                };
                for predicate in self.universe.iri_values(&constrained[1]).indices() {
                    changed |= state.publish_condition(
                        predicate,
                        side / 2,
                        value,
                        opposite,
                        self.universe.size(),
                    );
                }
            }
        }
        changed
    }

    pub(crate) fn refine(
        &self,
        effects: &[ProducerEffect],
        input: &FlowSummary,
    ) -> Vec<ProducerEffect> {
        self.refine_enabled(effects, input, &vec![true; self.rules.len()])
    }

    /// Scope producer reachability before evaluating abstract bindings. A source
    /// rule owned by another world cannot seed a local writer or dependency.
    pub(crate) fn refine_enabled(
        &self,
        effects: &[ProducerEffect],
        input: &FlowSummary,
        enabled: &[bool],
    ) -> Vec<ProducerEffect> {
        assert_eq!(enabled.len(), self.rules.len());
        let state = self.closure_enabled(input, enabled);
        effects
            .iter()
            .zip(&self.rules)
            .zip(enabled)
            .map(|((effect, rule), enabled)| {
                let mut effect = effect.clone();
                let Some(bindings) = enabled.then(|| self.bindings(rule, &state)).flatten() else {
                    effect.writes.clear();
                    effect.completion_for.clear();
                    effect.reads.clear();
                    return effect;
                };
                for (pattern, head) in effect.writes.iter_mut().zip(&rule.heads) {
                    self.restrict(pattern, self.head_domains(rule, head, &bindings, &state));
                }
                for ((pattern, _), read) in effect.reads.iter_mut().zip(&rule.reads) {
                    let domains = if let Some(read) = read {
                        self.universe.domains(read, &bindings, true)
                    } else {
                        self.pattern_domains(pattern)
                    };
                    self.restrict(pattern, domains);
                }
                effect
            })
            .collect()
    }

    fn closure(&self, input: &FlowSummary) -> FlowSummary {
        self.closure_enabled(input, &vec![true; self.rules.len()])
    }

    fn closure_enabled(&self, input: &FlowSummary, enabled: &[bool]) -> FlowSummary {
        let mut state = input.clone();
        loop {
            let mut changed = false;
            for (rule, enabled) in self.rules.iter().zip(enabled) {
                if !enabled {
                    continue;
                }
                if let Some(bindings) = self.bindings(rule, &state) {
                    for head in &rule.heads {
                        let domains = self.head_domains(rule, head, &bindings, &state);
                        changed |= state.publish_columns(&domains, &self.universe);
                        changed |= self.publish_head_conditions(&mut state, rule, head, &domains);
                    }
                }
            }
            if !changed {
                break;
            }
        }
        state
    }

    /// Add bounded exact selector cells without re-lowering a native rule. The
    /// existing constant indices remain stable; Other still covers every value
    /// outside the enriched vocabulary, including arbitrary future witnesses.
    pub(crate) fn with_source_constants<'a>(
        &self,
        facts: impl Iterator<Item = &'a Fact>,
        limit: usize,
    ) -> Option<Self> {
        let mut universe = self.universe.clone();
        if universe.size() > limit {
            return None;
        }
        for fact in facts {
            for term in [
                EvalTerm::ConstLit(fact.subject.clone()),
                EvalTerm::named(&fact.predicate),
                EvalTerm::ConstLit(fact.object.clone()),
            ] {
                universe.observe(&term);
                if universe.size() > limit {
                    return None;
                }
            }
        }
        Some(Self {
            native_witness_domain: universe.native_witness_domain(),
            conditioned: self.conditioned.resized(universe.size()),
            universe,
            rules: self.rules.clone(),
            operator_inputs: self.operator_inputs.clone(),
        })
    }

    /// Complete abstract reachability bounds each body variable. A finite domain
    /// excludes Other; variables that may carry future witnesses stay unbound in
    /// the proof. Cartesian enumeration can add matches but cannot lose a firing.
    pub(crate) fn finite_bindings<'a>(
        &self,
        facts: impl Iterator<Item = &'a Fact>,
    ) -> Vec<Option<BTreeMap<String, Vec<&TermValue>>>> {
        self.finite_bindings_with_patterns(facts, std::iter::empty())
    }

    /// Bound every producer after admitting typed external output envelopes.
    /// The envelopes are abstract possible columns, never synthesized facts.
    pub(crate) fn finite_bindings_with_patterns<'a, 'b>(
        &'a self,
        facts: impl Iterator<Item = &'b Fact>,
        patterns: impl Iterator<Item = &'b StatementPattern>,
    ) -> Vec<Option<BTreeMap<String, Vec<&'a TermValue>>>> {
        let mut input = self.summarize(facts);
        self.seed_patterns(&mut input, patterns);
        let state = self.closure(&input);
        self.rules
            .iter()
            .map(|rule| {
                let bindings = self.bindings(rule, &state)?;
                Some(
                    rule.names
                        .iter()
                        .filter_map(|(name, &slot)| {
                            let domain = &bindings[slot];
                            (!domain.contains(self.universe.other())).then(|| {
                                (
                                    name.clone(),
                                    domain
                                        .indices()
                                        .map(|index| &self.universe.values[index])
                                        .collect(),
                                )
                            })
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn pattern_domains(&self, pattern: &StatementPattern) -> [Domain; 3] {
        let value = |term: &Option<TermValue>| {
            term.as_ref().map_or_else(
                || Domain::all(self.universe.size()),
                |value| Domain::one(self.universe.size(), self.universe.value(value)),
            )
        };
        let predicates = pattern.predicate.as_ref().map_or_else(
            || Domain::all(self.universe.size()),
            |predicate| Domain::one(self.universe.size(), self.universe.iri(predicate)),
        );
        let object = pattern.object.as_ref().map_or_else(
            || value(&None),
            |term| self.universe.marker(self.universe.value(term), &predicates),
        );
        [
            value(&pattern.subject),
            self.universe.predicates(&predicates),
            object,
        ]
    }

    /// Prove absence of every reachable writer using the same complete abstract
    /// closure and value cells as scheduling. A current absence of source rows is
    /// never evidence of immutability.
    pub(crate) fn immutable_predicate<'a>(
        &self,
        predicate: &str,
        effects: impl Iterator<Item = &'a ProducerEffect>,
    ) -> bool {
        let mut read = StatementPattern::relation(Some(predicate), None);
        read.ranges = Some(self.pattern_domains(&read));
        effects
            .flat_map(|effect| &effect.writes)
            .all(|write| !read.reads_write(write, self.universe.semantics))
    }

    /// Name the first reachable producer that may write this exact protected
    /// pattern. Unknown effects stay overlapping; source absence is not a proof.
    pub(crate) fn overlapping_writer<'a>(
        &self,
        pattern: &StatementPattern,
        effects: &'a [ProducerEffect],
    ) -> Option<&'a str> {
        let mut read = pattern.clone();
        read.ranges = Some(self.pattern_domains(&read));
        effects
            .iter()
            .find(|effect| {
                effect
                    .writes
                    .iter()
                    .any(|write| read.reads_write(write, self.universe.semantics))
            })
            .map(|effect| effect.name.as_str())
    }

    /// Compare one protected read with one source-refined write in this exact
    /// abstract universe. Both sides receive compatible range cells; dropping
    /// the read range would turn every variable head back into a wildcard.
    pub(crate) fn overlaps_write(
        &self,
        pattern: &StatementPattern,
        write: &StatementPattern,
    ) -> bool {
        let mut read = pattern.clone();
        read.ranges = Some(self.pattern_domains(&read));
        read.reads_write(write, self.universe.semantics)
    }

    /// Bind an external completion observation to this analysis universe. Fixed
    /// grammar constants then remain disjoint from the `Other` cell used for
    /// data-selected predicates, while genuinely unknown observations stay broad.
    pub(crate) fn ranged_pattern(&self, mut pattern: StatementPattern) -> StatementPattern {
        pattern.ranges = Some(self.pattern_domains(&pattern));
        pattern
    }

    fn restrict(&self, pattern: &mut StatementPattern, domains: [Domain; 3]) {
        if pattern.predicate.is_none() {
            let mut predicates = domains[1].indices();
            if let Some(first) = predicates.next()
                && predicates.next().is_none()
                && let Some(TermValue::Iri(iri)) = self.universe.values.get(first)
            {
                pattern.predicate = Some(iri.clone());
            }
        }
        pattern.ranges = Some(domains);
    }
}

#[cfg(test)]
mod tests;
