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

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use purrdf::TermValue;
use rayon::prelude::*;

use super::{ProducerEffect, StatementPattern, constant};
use crate::native_semantics::SemanticVocabulary;
use crate::rule_ir::{EvalTerm, Fact};

/// Avoid Rayon coordination for the small refinements that dominate ordinary
/// rules. Large conditioned domains are evaluated in bounded batches so each
/// worker retains only one batch of temporary variable bitsets.
const PARALLEL_HEAD_REFINEMENT_MIN_CANDIDATES: usize = 1_024;
const PARALLEL_HEAD_REFINEMENT_CHUNK_SIZE: usize = 256;

/// A finite set of abstract values. These are value-domain bits, not row IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Domain(Vec<u64>);

impl Domain {
    fn empty(size: usize) -> Self {
        Self(vec![0; size.div_ceil(64)])
    }
    fn all(size: usize) -> Self {
        let mut words = vec![u64::MAX; size.div_ceil(64)];
        if let Some(last) = words.last_mut()
            && size % 64 != 0
        {
            *last = (1u64 << (size % 64)) - 1;
        }
        Self(words)
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
    fn intersect_singleton(&mut self, index: usize) -> bool {
        let present = self.contains(index);
        let word = index / 64;
        let bit = 1u64 << (index % 64);
        let mut changed = false;
        for (position, value) in self.0.iter_mut().enumerate() {
            let next = if present && position == word { bit } else { 0 };
            changed |= *value != next;
            *value = next;
        }
        changed
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
    fn singleton(&self) -> Option<usize> {
        let mut values = self.indices();
        let value = values.next()?;
        values.next().is_none().then_some(value)
    }
    fn union_intersection(&mut self, left: &Self, right: &Self) -> bool {
        let mut present = false;
        for ((output, left), right) in self.0.iter_mut().zip(&left.0).zip(&right.0) {
            let intersection = left & right;
            present |= intersection != 0;
            *output |= intersection;
        }
        present
    }
    fn hash_nonzero_words(&self, hash: &mut blake3::Hasher) {
        hash.update(&(self.0.len() as u64).to_le_bytes());
        hash.update(&(self.0.iter().filter(|word| **word != 0).count() as u64).to_le_bytes());
        for (index, word) in self.0.iter().enumerate().filter(|(_, word)| **word != 0) {
            hash.update(&(index as u64).to_le_bytes());
            hash.update(&word.to_le_bytes());
        }
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
    /// Pure native cardinality equalities with one statically interpreted side.
    /// These restrict values without adding a statement dependency.
    pub(crate) cardinality_guards: Vec<FlowCardinalityGuard>,
    /// Wildcard reads whose predicate is selected from one RDF list. The read is
    /// narrowed only after the selected source lists are known and their selector
    /// and cell predicates are proven immutable for this input shape.
    pub(crate) list_reads: Vec<FlowListRead>,
    /// Wildcard reads whose predicate is the object of one source selector.
    /// These cover native cardinality probes and restricted witness checks.
    pub(crate) selected_reads: Vec<FlowSelectedRead>,
}

#[derive(Debug, Clone)]
pub(crate) struct FlowCardinalityGuard {
    pub(crate) term: EvalTerm,
    pub(crate) count: u128,
}

#[derive(Debug, Clone)]
pub(crate) struct FlowListRead {
    pub(crate) selector_predicate: String,
    pub(crate) read_index: usize,
    pub(crate) read_column: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct FlowSelectedRead {
    pub(crate) selector_predicate: String,
    pub(crate) read_index: usize,
    pub(crate) read_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    name: String,
    body: Vec<[Slot; 3]>,
    heads: Vec<[Slot; 3]>,
    reads: Vec<Option<[Slot; 3]>>,
    variables: usize,
    native_witnesses: Vec<usize>,
    cardinality_guards: Vec<(Slot, u128)>,
    list_reads: Vec<ListRead>,
    selected_reads: Vec<SelectedRead>,
    names: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct ListRead {
    selector_predicate: String,
    read_index: usize,
    read_column: usize,
    selected_members: Option<Domain>,
}

#[derive(Debug, Clone)]
struct SelectedRead {
    selector_predicate: String,
    read_index: usize,
    read_column: usize,
    selected_values: Option<Domain>,
}

#[derive(Debug, Clone)]
struct RuleRefinement {
    bindings: Vec<Domain>,
    heads: Vec<[Domain; 3]>,
}

/// One nonempty seeded head join, reduced to what publication reads. The
/// opposite column stays sparse until it would outgrow its bitset, so a whole
/// refinement's results stay resident at no more than one domain per class.
#[derive(Debug)]
struct HeadSupport {
    predicates: Vec<usize>,
    opposite: AdaptiveDomain,
}

/// A support row compared by the value set it denotes, whichever
/// representation stores it.
struct SupportContent<'a>(&'a AdaptiveDomain);

impl PartialEq for SupportContent<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.indices().eq(other.0.indices())
    }
}

impl Eq for SupportContent<'_> {}

impl std::hash::Hash for SupportContent<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for value in self.0.indices() {
            value.hash(state);
        }
    }
}

/// Exact partition refinement over candidate positions. Each split separates,
/// inside every class it touches, the members it names from the rest, in time
/// linear in the members named.
struct CandidatePartition {
    class: Vec<usize>,
    sizes: Vec<usize>,
    touched: Vec<usize>,
    fresh: Vec<usize>,
    touched_classes: Vec<usize>,
    members: Vec<usize>,
}

impl CandidatePartition {
    /// One class of every unmarked position, and a singleton per marked one.
    fn new(singletons: &[bool]) -> Self {
        let shared = usize::from(singletons.iter().any(|single| !single));
        let mut sizes = vec![0; shared];
        let class = singletons
            .iter()
            .map(|&single| {
                if single {
                    sizes.push(1);
                    sizes.len() - 1
                } else {
                    sizes[0] += 1;
                    0
                }
            })
            .collect();
        let classes = sizes.len();
        Self {
            class,
            sizes,
            touched: vec![0; classes],
            fresh: vec![usize::MAX; classes],
            touched_classes: Vec::new(),
            members: Vec::new(),
        }
    }

    fn is_discrete(&self) -> bool {
        self.sizes.len() == self.class.len()
    }

    /// Split every class by membership in a set of distinct positions.
    fn split(&mut self, members: impl IntoIterator<Item = usize>) {
        if self.is_discrete() {
            return;
        }
        self.members.clear();
        self.members.extend(members);
        for &member in &self.members {
            let class = self.class[member];
            if self.touched[class] == 0 {
                self.touched_classes.push(class);
            }
            self.touched[class] += 1;
        }
        for &class in &self.touched_classes {
            let named = self.touched[class];
            if named < self.sizes[class] {
                self.fresh[class] = self.sizes.len();
                self.sizes[class] -= named;
                self.sizes.push(named);
                self.touched.push(0);
                self.fresh.push(usize::MAX);
            }
        }
        for &member in &self.members {
            let fresh = self.fresh[self.class[member]];
            if fresh != usize::MAX {
                self.class[member] = fresh;
            }
        }
        for &class in &self.touched_classes {
            self.touched[class] = 0;
            self.fresh[class] = usize::MAX;
        }
        self.touched_classes.clear();
    }

    /// Split every class by an exact label; unlabelled positions stay together.
    fn split_labelled(&mut self, mut labelled: Vec<(usize, u64)>) {
        if self.is_discrete() || labelled.is_empty() {
            return;
        }
        labelled.sort_unstable_by_key(|&(member, label)| (self.class[member], label));
        let mut start = 0;
        while start < labelled.len() {
            let class = self.class[labelled[start].0];
            let end = start
                + labelled[start..]
                    .iter()
                    .take_while(|(member, _)| self.class[*member] == class)
                    .count();
            let mut groups: Vec<&[(usize, u64)]> = labelled[start..end]
                .chunk_by(|left, right| left.1 == right.1)
                .collect();
            // A class wholly covered by one label keeps its id for that group.
            if end - start == self.sizes[class] {
                groups.remove(0);
            }
            for group in groups {
                let fresh = self.sizes.len();
                self.sizes[class] -= group.len();
                self.sizes.push(group.len());
                self.touched.push(0);
                self.fresh.push(usize::MAX);
                for &(member, _) in group {
                    self.class[member] = fresh;
                }
            }
            start = end;
        }
    }

    /// Classes as candidate values, ordered by their first candidate.
    fn classes(self, candidates: &[usize]) -> Vec<Vec<usize>> {
        let mut output = vec![usize::MAX; self.sizes.len()];
        let mut classes: Vec<Vec<usize>> = Vec::with_capacity(self.sizes.len());
        for (&class, &value) in self.class.iter().zip(candidates) {
            if output[class] == usize::MAX {
                output[class] = classes.len();
                classes.push(Vec::with_capacity(self.sizes[class]));
            }
            classes[output[class]].push(value);
        }
        classes
    }
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

fn cardinality_inputs(rules: &[FlowRule], semantics: SemanticVocabulary) -> Vec<OperatorInput> {
    let mut inputs = BTreeSet::new();
    for rule in rules {
        for guard in &rule.cardinality_guards {
            let Some(name) = variable(&guard.term) else {
                continue;
            };
            for atom in &rule.body {
                for column in [0usize, 2] {
                    if variable(&atom[column]) == Some(name) {
                        inputs.insert(OperatorInput {
                            predicate: atom_predicate(atom, semantics),
                            column,
                        });
                    }
                }
            }
        }
    }
    inputs.into_iter().collect()
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

fn cardinality_domains(universe: &Universe, rules: &[Rule]) -> BTreeMap<u128, Domain> {
    let counts: BTreeSet<_> = rules
        .iter()
        .flat_map(|rule| rule.cardinality_guards.iter().map(|(_, count)| *count))
        .collect();
    counts
        .into_iter()
        .map(|count| {
            let mut domain = Domain::one(universe.size(), universe.other());
            for (index, value) in universe.values.iter().enumerate() {
                if crate::reason::value::NativeValues::parse_cardinality(value) == Some(count) {
                    domain.insert(index);
                }
            }
            (count, domain)
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RelationSummary {
    columns: [AdaptiveDomain; 2],
    /// Opposite-column supports keyed only by conditioned constants. This is a
    /// bounded grammar relation, not retained source rows.
    by_subject: BTreeMap<usize, AdaptiveDomain>,
    by_object: BTreeMap<usize, AdaptiveDomain>,
    /// Producer and native-witness supports can be Cartesian products. Retaining
    /// each product once avoids expanding a wildcard or generated-value side into
    /// one universe-width support per conditioned value.
    rectangles: Vec<[Domain; 2]>,
}

/// An input column normally contains only a small fraction of the selected value
/// universe. Keep that case sparse; promote to the fixed-width domain once the
/// sorted indices would occupy at least as much memory as its bitset. Both
/// representations denote the same abstract value set.
#[derive(Debug, Clone)]
enum AdaptiveDomain {
    Sparse { size: usize, values: Vec<usize> },
    Dense(Domain),
}

impl AdaptiveDomain {
    fn empty(size: usize) -> Self {
        Self::Sparse {
            size,
            values: Vec::new(),
        }
    }

    fn union(&mut self, other: &Domain) -> bool {
        match self {
            Self::Dense(domain) => domain.union(other),
            Self::Sparse { size, values } => {
                let dense_words = size.div_ceil(64);
                let incoming = other.indices().count();
                if values.len().saturating_add(incoming) >= dense_words {
                    let mut domain = Domain::empty(*size);
                    for &value in values.iter() {
                        domain.insert(value);
                    }
                    let changed = domain.union(other);
                    *self = Self::Dense(domain);
                    return changed;
                }
                let mut changed = false;
                for value in other.indices() {
                    if let Err(index) = values.binary_search(&value) {
                        values.insert(index, value);
                        changed = true;
                    }
                }
                changed
            }
        }
    }

    fn insert(&mut self, value: usize) -> bool {
        match self {
            Self::Dense(domain) => {
                if domain.contains(value) {
                    return false;
                }
                domain.insert(value);
                true
            }
            Self::Sparse { size, values } => {
                let Err(index) = values.binary_search(&value) else {
                    return false;
                };
                values.insert(index, value);
                if values.len() >= size.div_ceil(64) {
                    let mut domain = Domain::empty(*size);
                    for &value in values.iter() {
                        domain.insert(value);
                    }
                    *self = Self::Dense(domain);
                }
                true
            }
        }
    }

    fn to_domain(&self) -> Domain {
        match self {
            Self::Dense(domain) => domain.clone(),
            Self::Sparse { size, values } => {
                let mut domain = Domain::empty(*size);
                for &value in values {
                    domain.insert(value);
                }
                domain
            }
        }
    }

    fn intersect_union_into(&self, filter: &Domain, output: &mut Domain) -> bool {
        match self {
            Self::Dense(domain) => {
                let mut intersection = domain.clone();
                intersection.intersect(filter);
                let present = !intersection.is_empty();
                output.union(&intersection);
                present
            }
            Self::Sparse { values, .. } => {
                let mut present = false;
                for &value in values {
                    if filter.contains(value) {
                        output.insert(value);
                        present = true;
                    }
                }
                present
            }
        }
    }

    fn hash_nonzero_words(&self, hash: &mut blake3::Hasher) {
        match self {
            Self::Dense(domain) => domain.hash_nonzero_words(hash),
            Self::Sparse { size, values } => {
                hash.update(&(size.div_ceil(64) as u64).to_le_bytes());
                let groups = values
                    .iter()
                    .map(|value| value / 64)
                    .scan(None, |previous, word| {
                        let distinct = previous.is_none_or(|last| last != word);
                        *previous = Some(word);
                        Some(distinct)
                    })
                    .filter(|distinct| *distinct)
                    .count();
                hash.update(&(groups as u64).to_le_bytes());
                let mut values = values.iter().copied().peekable();
                while let Some(value) = values.next() {
                    let word_index = value / 64;
                    let mut word = 1u64 << (value % 64);
                    while values.peek().is_some_and(|value| *value / 64 == word_index) {
                        let value = values.next().expect("peeked support value");
                        word |= 1 << (value % 64);
                    }
                    hash.update(&(word_index as u64).to_le_bytes());
                    hash.update(&word.to_le_bytes());
                }
            }
        }
    }

    fn is_subset_of(&self, other: &Domain) -> bool {
        match self {
            Self::Dense(domain) => domain.is_subset_of(other),
            Self::Sparse { values, .. } => values.iter().all(|value| other.contains(*value)),
        }
    }

    fn indices(&self) -> Box<dyn Iterator<Item = usize> + '_> {
        match self {
            Self::Dense(domain) => Box::new(domain.indices()),
            Self::Sparse { values, .. } => Box::new(values.iter().copied()),
        }
    }
}

impl RelationSummary {
    fn empty(size: usize) -> Self {
        Self {
            columns: std::array::from_fn(|_| AdaptiveDomain::empty(size)),
            by_subject: BTreeMap::new(),
            by_object: BTreeMap::new(),
            rectangles: Vec::new(),
        }
    }

    fn rectangle_covers_pair(&self, subject: usize, object: usize) -> bool {
        self.rectangles
            .iter()
            .any(|rectangle| rectangle[0].contains(subject) && rectangle[1].contains(object))
    }

    fn rectangle_covers(&self, side: usize, value: usize, opposite: &Domain) -> bool {
        self.rectangles.iter().any(|rectangle| {
            rectangle[side].contains(value) && opposite.is_subset_of(&rectangle[1 - side])
        })
    }

    /// Union into `output` the `filter`ed opposite-column support of every value in
    /// `values` on `side`, and return the values that have any such support.
    ///
    /// A rectangle contributes the same `rectangle[1 - side] ∩ filter` to each of its
    /// values, so that intersection is formed once per rectangle rather than once per
    /// value: the result is identical to probing each value separately.
    fn supports_into(
        &self,
        side: usize,
        values: &Domain,
        filter: &Domain,
        output: &mut Domain,
        size: usize,
    ) -> Domain {
        let supports = if side == 0 {
            &self.by_subject
        } else {
            &self.by_object
        };
        let mut present = Domain::empty(size);
        for value in values.indices() {
            if supports
                .get(&value)
                .is_some_and(|support| support.intersect_union_into(filter, output))
            {
                present.insert(value);
            }
        }
        for rectangle in &self.rectangles {
            if !rectangle[side].overlaps(values) {
                continue;
            }
            let mut contribution = rectangle[1 - side].clone();
            contribution.intersect(filter);
            if !contribution.is_empty() {
                output.union(&contribution);
                present.union_intersection(&rectangle[side], values);
            }
        }
        present
    }

    fn publish_rectangle(&mut self, rectangle: [Domain; 2]) -> bool {
        if self.rectangles.iter().any(|existing| {
            rectangle[0].is_subset_of(&existing[0]) && rectangle[1].is_subset_of(&existing[1])
        }) {
            return false;
        }
        self.rectangles.retain(|existing| {
            !(existing[0].is_subset_of(&rectangle[0]) && existing[1].is_subset_of(&rectangle[1]))
        });
        self.by_subject.retain(|subject, support| {
            !rectangle[0].contains(*subject) || !support.is_subset_of(&rectangle[1])
        });
        self.by_object.retain(|object, support| {
            !rectangle[1].contains(*object) || !support.is_subset_of(&rectangle[0])
        });
        self.rectangles.push(rectangle);
        true
    }
}

/// Full-input abstract columns plus grammar-constant conditional supports. Size
/// depends on the selected operator vocabulary, never the number of corpus rows.
/// The flow vocabulary identity fixes every bit meaning used by a cached summary.
#[derive(Debug, Clone)]
pub(crate) struct FlowSummary {
    relations: BTreeMap<usize, RelationSummary>,
    /// Monotone candidate lookup for exact conditioned supports. A
    /// singleton-constrained dynamic predicate join can use this instead of
    /// scanning every predicate; rectangle subsumption may leave harmless stale
    /// candidates that the ordinary relation support check rejects.
    predicates_by_subject: BTreeMap<usize, AdaptiveDomain>,
    predicates_by_object: BTreeMap<usize, AdaptiveDomain>,
    /// Rectangles deliberately remain compact, so their predicates are checked
    /// separately rather than expanding a wildcard side into this exact index.
    rectangle_relations: BTreeSet<usize>,
}

impl FlowSummary {
    fn index_condition(&mut self, predicate: usize, side: usize, value: usize, size: usize) {
        let predicates = if side == 0 {
            &mut self.predicates_by_subject
        } else {
            &mut self.predicates_by_object
        };
        predicates
            .entry(value)
            .or_insert_with(|| AdaptiveDomain::empty(size))
            .insert(predicate);
    }

    /// Restrict a dynamic predicate search when a conditioned value column is a
    /// singleton. Exact supports use the derived reverse index; compact producer
    /// rectangles are consulted without materializing their Cartesian products.
    fn predicates_for_singleton(
        &self,
        required: &[Domain; 3],
        conditioned: &Domain,
        size: usize,
    ) -> Option<Domain> {
        let mut selected: Option<Domain> = None;
        for (side, column) in [(0usize, 0usize), (1, 2)] {
            if !required[column].is_subset_of(conditioned) {
                continue;
            }
            let Some(value) = required[column].singleton() else {
                continue;
            };
            let index = if side == 0 {
                &self.predicates_by_subject
            } else {
                &self.predicates_by_object
            };
            let mut predicates = index
                .get(&value)
                .map_or_else(|| Domain::empty(size), AdaptiveDomain::to_domain);
            for predicate in &self.rectangle_relations {
                let relation = self
                    .relations
                    .get(predicate)
                    .expect("rectangle relation index remains synchronized");
                if relation
                    .rectangles
                    .iter()
                    .any(|rectangle| rectangle[side].contains(value))
                {
                    predicates.insert(*predicate);
                }
            }
            predicates.intersect(&required[1]);
            if let Some(existing) = &mut selected {
                existing.intersect(&predicates);
            } else {
                selected = Some(predicates);
            }
        }
        selected
    }

    pub(crate) fn identity(&self, template: &[u8; 32]) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"gmeow-native-input-columns-v3\0");
        hash.update(template);
        hash.update(&(self.relations.len() as u64).to_le_bytes());
        for (predicate, relation) in &self.relations {
            hash.update(&(*predicate as u64).to_le_bytes());
            for column in &relation.columns {
                column.hash_nonzero_words(&mut hash);
            }
            for supports in [&relation.by_subject, &relation.by_object] {
                hash.update(&(supports.len() as u64).to_le_bytes());
                for (value, domain) in supports {
                    hash.update(&(*value as u64).to_le_bytes());
                    domain.hash_nonzero_words(&mut hash);
                }
            }
            hash.update(&(relation.rectangles.len() as u64).to_le_bytes());
            for rectangle in &relation.rectangles {
                for domain in rectangle {
                    domain.hash_nonzero_words(&mut hash);
                }
            }
        }
        *hash.finalize().as_bytes()
    }
    fn publish_columns_tracking(
        &mut self,
        head: &[Domain; 3],
        universe: &Universe,
        changed_predicates: &mut BTreeSet<usize>,
    ) -> bool {
        if head.iter().any(Domain::is_empty) {
            return false;
        }
        let mut changed = false;
        for predicate in universe.iri_values(&head[1]).indices() {
            let relation = self
                .relations
                .entry(predicate)
                .or_insert_with(|| RelationSummary::empty(universe.size()));
            let subject_changed = relation.columns[0].union(&head[0]);
            let object_changed = relation.columns[1].union(&head[2]);
            let relation_changed = subject_changed || object_changed;
            if relation_changed {
                changed_predicates.insert(predicate);
                changed = true;
            }
        }
        changed
    }

    fn publish_columns(&mut self, head: &[Domain; 3], universe: &Universe) -> bool {
        self.publish_columns_tracking(head, universe, &mut BTreeSet::new())
    }

    fn publish_rectangle(&mut self, head: &[Domain; 3], universe: &Universe) -> bool {
        let mut changed = self.publish_columns(head, universe);
        changed |= self.publish_support_rectangle(head, universe, &mut BTreeSet::new());
        changed
    }

    /// Retain an exact Cartesian support without eagerly expanding either side
    /// into one conditional domain per value. Columns may be published by the
    /// caller after all head refinements, preserving the existing transfer order.
    fn publish_support_rectangle(
        &mut self,
        head: &[Domain; 3],
        universe: &Universe,
        changed_predicates: &mut BTreeSet<usize>,
    ) -> bool {
        if head.iter().any(Domain::is_empty) {
            return false;
        }
        let mut changed = false;
        for predicate in universe.iri_values(&head[1]).indices() {
            let relation = self
                .relations
                .entry(predicate)
                .or_insert_with(|| RelationSummary::empty(universe.size()));
            if relation.publish_rectangle([head[0].clone(), head[2].clone()]) {
                self.rectangle_relations.insert(predicate);
                changed_predicates.insert(predicate);
                changed = true;
            }
        }
        changed
    }

    /// Publish one exact source row without allocating three universe-width
    /// temporary domains. This is equivalent to a rectangle whose subject,
    /// predicate and object columns are singletons.
    fn publish_fact(
        &mut self,
        subject: usize,
        predicate: usize,
        object: usize,
        universe_size: usize,
        conditioned: &Domain,
    ) {
        let (indexed_subject, indexed_object) = {
            let relation = self
                .relations
                .entry(predicate)
                .or_insert_with(|| RelationSummary::empty(universe_size));
            relation.columns[0].insert(subject);
            relation.columns[1].insert(object);
            let covered = relation.rectangle_covers_pair(subject, object);
            let indexed_subject = conditioned.contains(subject) && !covered;
            let indexed_object = conditioned.contains(object) && !covered;
            if indexed_subject {
                relation
                    .by_subject
                    .entry(subject)
                    .or_insert_with(|| AdaptiveDomain::empty(universe_size))
                    .insert(object);
            }
            if indexed_object {
                relation
                    .by_object
                    .entry(object)
                    .or_insert_with(|| AdaptiveDomain::empty(universe_size))
                    .insert(subject);
            }
            (indexed_subject, indexed_object)
        };
        if indexed_subject {
            self.index_condition(predicate, 0, subject, universe_size);
        }
        if indexed_object {
            self.index_condition(predicate, 1, object, universe_size);
        }
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
        if relation.rectangle_covers(side, value, opposite) {
            return false;
        }
        let supports = if side == 0 {
            &mut relation.by_subject
        } else {
            &mut relation.by_object
        };
        let changed = supports
            .entry(value)
            .or_insert_with(|| AdaptiveDomain::empty(size))
            .union(opposite);
        if changed {
            self.index_condition(predicate, side, value, size);
        }
        changed
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
    cardinality_inputs: Vec<OperatorInput>,
    cardinality_domains: BTreeMap<u128, Domain>,
    /// External output envelopes participate in list-structure immutability.
    /// They remain compact patterns and are never expanded into synthetic facts.
    possible_writes: Vec<StatementPattern>,
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
        assert_eq!(rules.len(), effects.len());
        let mut universe = Universe::new(semantics);
        let (operator_inputs, operator_conditions) = operator_inputs(rules, semantics);
        let cardinality_inputs = cardinality_inputs(rules, semantics);
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
            for guard in &rule.cardinality_guards {
                universe.observe(&guard.term);
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
        let rules: Vec<_> = rules
            .iter()
            .zip(effects)
            .map(|(source, effect)| {
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
                let cardinality_guards = source
                    .cardinality_guards
                    .iter()
                    .map(|guard| {
                        let slot = match &guard.term {
                            EvalTerm::Var(name) => {
                                let next = variables.len();
                                Slot::Variable(*variables.entry(name.clone()).or_insert(next))
                            }
                            term => Slot::Constant(
                                universe.value(&constant(term).expect("constant guard term")),
                            ),
                        };
                        (slot, guard.count)
                    })
                    .collect();
                let list_reads = source
                    .list_reads
                    .iter()
                    .map(|read| {
                        assert!(
                            read.read_index < source.reads.len(),
                            "list-selected read index must name a declared producer read"
                        );
                        assert!(read.read_column < 3, "statement read columns are ternary");
                        ListRead {
                            selector_predicate: semantics
                                .predicate(&read.selector_predicate)
                                .to_owned(),
                            read_index: read.read_index,
                            read_column: read.read_column,
                            selected_members: None,
                        }
                    })
                    .collect();
                let selected_reads = source
                    .selected_reads
                    .iter()
                    .map(|read| {
                        assert!(
                            read.read_index < source.reads.len(),
                            "source-selected read index must name a declared producer read"
                        );
                        assert!(read.read_column < 3, "statement read columns are ternary");
                        SelectedRead {
                            selector_predicate: semantics
                                .predicate(&read.selector_predicate)
                                .to_owned(),
                            read_index: read.read_index,
                            read_column: read.read_column,
                            selected_values: None,
                        }
                    })
                    .collect();
                Rule {
                    name: effect.name().to_owned(),
                    body,
                    heads,
                    reads,
                    variables: variables.len(),
                    native_witnesses,
                    cardinality_guards,
                    list_reads,
                    selected_reads,
                    names: variables,
                }
            })
            .collect();
        let native_witness_domain = universe.native_witness_domain();
        let cardinality_domains = cardinality_domains(&universe, &rules);
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
            cardinality_inputs,
            cardinality_domains,
            possible_writes: Vec::new(),
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
        let observed: Vec<_> = observed
            .cloned()
            .map(|mut pattern| {
                // A caller's ranges belong to its own value universe. The output
                // envelope is deliberately conservative in this enriched one.
                pattern.ranges = None;
                pattern
            })
            .collect();
        let mut universe = self.universe.clone();
        for pattern in &observed {
            for value in [&pattern.subject, &pattern.object].into_iter().flatten() {
                universe.observe(&EvalTerm::ConstLit(value.clone()));
            }
            if let Some(predicate) = &pattern.predicate {
                universe.observe(&EvalTerm::named(predicate));
            }
        }
        let native_witness_domain = universe.native_witness_domain();
        let cardinality_domains = cardinality_domains(&universe, &self.rules);
        let conditioned = self.conditioned.resized(universe.size());
        let mut possible_writes = self.possible_writes.clone();
        possible_writes.extend(observed);
        Self {
            universe,
            rules: self.rules.clone(),
            native_witness_domain,
            conditioned,
            operator_inputs: self.operator_inputs.clone(),
            cardinality_inputs: self.cardinality_inputs.clone(),
            cardinality_domains,
            possible_writes,
        }
    }

    /// Retain the sparse operator identities selected by this source. Every RDF
    /// predicate and only those resource-valued schema selectors that backward role
    /// analysis can place in predicate position receive exact cells. Folding those
    /// IRIs into `Other` would splice unrelated property rows at a variable-predicate
    /// join; ordinary resources, literals and blank nodes stay in the conservative
    /// cell rather than widening every relation bitset to the whole corpus.
    pub(crate) fn with_source_operators<'a, I>(&self, facts: I) -> Self
    where
        I: Iterator<Item = &'a Fact> + Clone,
    {
        self.with_source_metadata(facts, true)
    }

    /// Retain only source-selected metadata cells needed by finite termination
    /// specialization. Unlike execution scheduling, this does not intern every
    /// observed predicate or unrelated operator operand from the corpus.
    pub(crate) fn with_source_selections<'a, I>(&self, facts: I) -> Self
    where
        I: Iterator<Item = &'a Fact> + Clone,
    {
        self.with_source_metadata(facts, false)
    }

    fn with_source_metadata<'a, I>(&self, facts: I, all_operator_values: bool) -> Self
    where
        I: Iterator<Item = &'a Fact> + Clone,
    {
        const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

        let mut universe = self.universe.clone();
        let mut values = BTreeSet::new();
        let mut conditioned_values = BTreeSet::new();
        let selectors: BTreeSet<_> = self
            .rules
            .iter()
            .flat_map(|rule| {
                rule.list_reads
                    .iter()
                    .map(|read| read.selector_predicate.as_str())
                    .chain(
                        rule.selected_reads
                            .iter()
                            .map(|read| read.selector_predicate.as_str()),
                    )
            })
            .map(str::to_owned)
            .collect();
        let mut roots: BTreeMap<String, BTreeSet<TermValue>> = BTreeMap::new();
        let mut selector_owners: BTreeMap<String, BTreeSet<TermValue>> = BTreeMap::new();
        let mut first: BTreeMap<TermValue, BTreeSet<TermValue>> = BTreeMap::new();
        let mut rest: BTreeMap<TermValue, BTreeSet<TermValue>> = BTreeMap::new();
        for fact in facts.clone() {
            if all_operator_values {
                values.insert(TermValue::iri(&fact.predicate));
            }
            let predicate = self.universe.semantics.predicate(&fact.predicate);
            if selectors.contains(predicate) {
                selector_owners
                    .entry(predicate.to_owned())
                    .or_default()
                    .insert(fact.subject.clone());
                roots
                    .entry(predicate.to_owned())
                    .or_default()
                    .insert(fact.object.clone());
            }
            match predicate {
                RDF_FIRST => {
                    first
                        .entry(fact.subject.clone())
                        .or_default()
                        .insert(fact.object.clone());
                }
                RDF_REST => {
                    rest.entry(fact.subject.clone())
                        .or_default()
                        .insert(fact.object.clone());
                }
                _ => {}
            }
            for input in self.operator_inputs.iter().filter(|_| all_operator_values) {
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
                        conditioned_values.insert(value.clone());
                        if input
                            .predicate
                            .as_deref()
                            .is_some_and(|predicate| !matches!(predicate, RDF_FIRST | RDF_REST))
                        {
                            let carrier = if input.column == 0 {
                                &fact.object
                            } else {
                                &fact.subject
                            };
                            values.insert(carrier.clone());
                            conditioned_values.insert(carrier.clone());
                        }
                    }
                }
            }
            for input in self
                .cardinality_inputs
                .iter()
                .filter(|_| all_operator_values)
            {
                if input
                    .predicate
                    .as_deref()
                    .is_none_or(|required| required == predicate)
                {
                    let value = if input.column == 0 {
                        fact.subject.clone()
                    } else {
                        fact.object.clone()
                    };
                    values.insert(value.clone());
                    conditioned_values.insert(value);
                }
            }
        }
        let nil = TermValue::iri(RDF_NIL);
        let selected_members: BTreeMap<_, _> = selectors
            .into_iter()
            .map(|selector| {
                let mut members = BTreeSet::new();
                let mut pending: VecDeque<_> = roots
                    .get(&selector)
                    .into_iter()
                    .flat_map(|roots| roots.iter().cloned())
                    .collect();
                let mut visited = BTreeSet::new();
                while let Some(node) = pending.pop_front() {
                    if node == nil || !visited.insert(node.clone()) {
                        continue;
                    }
                    if let Some(values) = first.get(&node) {
                        members.extend(values.iter().cloned());
                    }
                    if let Some(tails) = rest.get(&node) {
                        pending.extend(tails.iter().cloned());
                    }
                }
                (selector, members)
            })
            .collect();
        for members in selected_members.values() {
            for value in members {
                values.insert(value.clone());
                conditioned_values.insert(value.clone());
            }
        }
        for rule in &self.rules {
            for read in &rule.list_reads {
                if let Some(owners) = selector_owners.get(&read.selector_predicate) {
                    for value in owners {
                        values.insert(value.clone());
                        conditioned_values.insert(value.clone());
                    }
                }
            }
            for read in &rule.selected_reads {
                if let Some(owners) = selector_owners.get(&read.selector_predicate) {
                    for value in owners {
                        values.insert(value.clone());
                        conditioned_values.insert(value.clone());
                    }
                }
                if let Some(selected) = roots.get(&read.selector_predicate) {
                    for value in selected {
                        if read.read_column != 1 || matches!(value, TermValue::Iri(_)) {
                            values.insert(value.clone());
                            conditioned_values.insert(value.clone());
                        }
                    }
                }
            }
        }
        if !all_operator_values {
            let anchors = conditioned_values.clone();
            for fact in facts {
                let predicate = self.universe.semantics.predicate(&fact.predicate);
                for input in &self.operator_inputs {
                    if !input
                        .predicate
                        .as_deref()
                        .is_some_and(|required| required == predicate)
                    {
                        continue;
                    }
                    let value = if input.column == 0 {
                        &fact.subject
                    } else {
                        &fact.object
                    };
                    let carrier = if input.column == 0 {
                        &fact.object
                    } else {
                        &fact.subject
                    };
                    if matches!(value, TermValue::Iri(_))
                        && (anchors.contains(value) || anchors.contains(carrier))
                    {
                        values.insert(value.clone());
                        values.insert(carrier.clone());
                        conditioned_values.insert(value.clone());
                        conditioned_values.insert(carrier.clone());
                    }
                }
                for input in &self.cardinality_inputs {
                    if !input
                        .predicate
                        .as_deref()
                        .is_some_and(|required| required == predicate)
                    {
                        continue;
                    }
                    let value = if input.column == 0 {
                        &fact.subject
                    } else {
                        &fact.object
                    };
                    let carrier = if input.column == 0 {
                        &fact.object
                    } else {
                        &fact.subject
                    };
                    if anchors.contains(value) || anchors.contains(carrier) {
                        values.insert(value.clone());
                        values.insert(carrier.clone());
                        conditioned_values.insert(value.clone());
                        conditioned_values.insert(carrier.clone());
                    }
                }
            }
        }
        for value in &values {
            universe.register(value.clone());
        }
        let native_witness_domain = universe.native_witness_domain();
        let cardinality_domains = cardinality_domains(&universe, &self.rules);
        let mut conditioned = self.conditioned.resized(universe.size());
        for value in conditioned_values {
            conditioned.insert(universe.value(&value));
        }
        let mut rules = self.rules.clone();
        for rule in &mut rules {
            for read in &mut rule.list_reads {
                let mut members = Domain::empty(universe.size());
                if let Some(values) = selected_members.get(&read.selector_predicate) {
                    for value in values {
                        if read.read_column != 1 || matches!(value, TermValue::Iri(_)) {
                            members.insert(universe.value(value));
                        }
                    }
                }
                read.selected_members = Some(members);
            }
            for read in &mut rule.selected_reads {
                let mut selected = Domain::empty(universe.size());
                if let Some(values) = roots.get(&read.selector_predicate) {
                    for value in values {
                        if read.read_column != 1 || matches!(value, TermValue::Iri(_)) {
                            selected.insert(universe.value(value));
                        }
                    }
                }
                read.selected_values = Some(selected);
            }
        }
        Self {
            conditioned,
            universe,
            rules,
            native_witness_domain,
            operator_inputs: self.operator_inputs.clone(),
            cardinality_inputs: self.cardinality_inputs.clone(),
            cardinality_domains,
            possible_writes: self.possible_writes.clone(),
        }
    }

    pub(crate) fn vocabulary_identity(&self) -> [u8; 32] {
        crate::physical::metadata_identity("gmeow-native-flow-vocabulary-v1", &self.universe.values)
    }

    pub(crate) fn summarize<'a>(&self, facts: impl Iterator<Item = &'a Fact>) -> FlowSummary {
        let mut result = FlowSummary {
            relations: BTreeMap::new(),
            predicates_by_subject: BTreeMap::new(),
            predicates_by_object: BTreeMap::new(),
            rectangle_relations: BTreeSet::new(),
        };
        for fact in facts {
            result.publish_fact(
                self.universe.value(&fact.subject),
                self.universe.iri(&fact.predicate),
                self.universe.value(&fact.object),
                self.universe.size(),
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
            summary.publish_rectangle(&self.pattern_domains(pattern), &self.universe);
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
        for (slot, count) in &rule.cardinality_guards {
            let allowed = self
                .cardinality_domains
                .get(count)
                .expect("cardinality guard domain was prepared");
            match slot {
                Slot::Variable(variable) => {
                    bindings[*variable].intersect(allowed);
                }
                Slot::Constant(value) if !allowed.contains(*value) => return None,
                Slot::Constant(_) => {}
            }
        }
        for &(variable, value) in seeds {
            bindings[variable].intersect_singleton(value);
        }
        if bindings.iter().any(Domain::is_empty) {
            return None;
        }
        self.stabilize_bindings(rule, state, bindings)
    }

    /// Refine a seeded join from the rule's already-stable unseeded upper bound.
    ///
    /// Constraint propagation only removes values. The greatest fixed point under
    /// an additional singleton seed is therefore contained in the unseeded fixed
    /// point, so restarting every candidate from the full universe repeats broad
    /// scans without changing the answer. Recursive support published during this
    /// visit still reschedules the rule; that later visit supplies a new upper
    /// bound from the enlarged state.
    fn bindings_seeded_from(
        &self,
        rule: &Rule,
        state: &FlowSummary,
        upper: &[Domain],
        seeds: &[(usize, usize)],
    ) -> Option<Vec<Domain>> {
        let mut bindings = upper.to_vec();
        for &(variable, value) in seeds {
            bindings[variable].intersect_singleton(value);
        }
        if bindings.iter().any(Domain::is_empty) {
            return None;
        }
        self.stabilize_bindings(rule, state, bindings)
    }

    fn stabilize_bindings(
        &self,
        rule: &Rule,
        state: &FlowSummary,
        mut bindings: Vec<Domain>,
    ) -> Option<Vec<Domain>> {
        let size = self.universe.size();
        loop {
            let mut changed = false;
            for atom in &rule.body {
                let required = self.universe.domains(atom, &bindings, true);
                let mut possible: [Domain; 3] = std::array::from_fn(|_| Domain::empty(size));
                let indexed_predicates =
                    state.predicates_for_singleton(&required, &self.conditioned, size);
                let predicates = indexed_predicates.as_ref().unwrap_or(&required[1]);
                for predicate in predicates.indices() {
                    let Some(relation) = state.relations.get(&predicate) else {
                        continue;
                    };
                    let mut row = [
                        relation.columns[0].to_domain(),
                        self.universe.predicates(&Domain::one(size, predicate)),
                        relation.columns[1].to_domain(),
                    ];
                    for (values, wanted) in row.iter_mut().zip(&required) {
                        values.intersect(wanted);
                    }
                    if !row[0].is_empty() && row[0].is_subset_of(&self.conditioned) {
                        let mut objects = Domain::empty(size);
                        let subjects =
                            relation.supports_into(0, &row[0], &row[2], &mut objects, size);
                        row[0] = subjects;
                        row[2] = objects;
                    }
                    if !row[2].is_empty() && row[2].is_subset_of(&self.conditioned) {
                        let mut subjects = Domain::empty(size);
                        let objects =
                            relation.supports_into(1, &row[2], &row[0], &mut subjects, size);
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
        upper: &[Domain],
    ) -> Option<Vec<Domain>> {
        match head[slot] {
            Slot::Constant(constant) => (constant == value).then(|| upper.to_vec()),
            Slot::Variable(variable) => {
                self.bindings_seeded_from(rule, state, upper, &[(variable, value)])
            }
        }
    }

    /// Whether exchanging this head's subject and object variables is an
    /// automorphism of the positive body. The constrained support computed for
    /// one side is then exactly the support needed for the same value on the
    /// other side. Recursive and dynamic-predicate reads remain safe: publishing
    /// a changed head schedules every affected reader for another fixed-point
    /// visit, including the current rule.
    fn has_symmetric_value_sides(&self, rule: &Rule, head: &[Slot; 3]) -> bool {
        let [
            Slot::Variable(subject),
            Slot::Constant(_),
            Slot::Variable(object),
        ] = head
        else {
            return false;
        };
        if subject == object
            || rule.native_witnesses.contains(subject) != rule.native_witnesses.contains(object)
        {
            return false;
        }
        let exchange = |slot: &Slot| match slot {
            Slot::Variable(variable) if variable == subject => Slot::Variable(*object),
            Slot::Variable(variable) if variable == object => Slot::Variable(*subject),
            slot => slot.clone(),
        };
        let mut unmatched_guards = rule.cardinality_guards.clone();
        for (slot, count) in &rule.cardinality_guards {
            let exchanged = (exchange(slot), *count);
            let Some(index) = unmatched_guards
                .iter()
                .position(|candidate| candidate == &exchanged)
            else {
                return false;
            };
            unmatched_guards.swap_remove(index);
        }
        if !unmatched_guards.is_empty() {
            return false;
        }
        let mut unmatched = rule.body.clone();
        for atom in &rule.body {
            let exchanged = atom.each_ref().map(exchange);
            let Some(index) = unmatched
                .iter()
                .position(|candidate| candidate == &exchanged)
            else {
                return false;
            };
            unmatched.swap_remove(index);
        }
        unmatched.is_empty()
    }

    /// The part of one seeded head join that publication consumes: `None` when
    /// the join is empty, otherwise the head predicates and the opposite value
    /// column. A predicate-slot candidate publishes nothing.
    fn head_value_support(
        &self,
        head: &[Slot; 3],
        slot: usize,
        value_bindings: Option<Vec<Domain>>,
    ) -> Option<HeadSupport> {
        let value_bindings = value_bindings?;
        let mut compact = AdaptiveDomain::empty(self.universe.size());
        if !matches!(slot, 0 | 2) {
            return Some(HeadSupport {
                predicates: Vec::new(),
                opposite: compact,
            });
        }
        let constrained = self.universe.domains(head, &value_bindings, false);
        let opposite = if slot == 0 {
            &constrained[2]
        } else {
            &constrained[0]
        };
        compact.union(opposite);
        Some(HeadSupport {
            predicates: self
                .universe
                .iri_values(&constrained[1])
                .indices()
                .collect(),
            opposite: compact,
        })
    }

    /// Publish one candidate's support. `exchange` names the class
    /// representative whose support was computed; exchanging the two values is an
    /// automorphism of every input of that join, so the member's exact support is
    /// the representative's with the pair transposed.
    #[allow(clippy::too_many_arguments)]
    fn publish_head_support(
        &self,
        state: &mut FlowSummary,
        slot: usize,
        value: usize,
        support: Option<&HeadSupport>,
        exchange: usize,
        scratch: &mut Domain,
        domains: &mut [Domain; 3],
        symmetric_value_sides: bool,
        changed_predicates: &mut BTreeSet<usize>,
    ) {
        let Some(support) = support else {
            domains[slot].remove(value);
            if slot == 0 && symmetric_value_sides {
                domains[2].remove(value);
            }
            return;
        };
        let transpose = |index: usize| {
            if index == exchange {
                value
            } else if index == value {
                exchange
            } else {
                index
            }
        };
        for index in support.opposite.indices() {
            scratch.insert(transpose(index));
        }
        for predicate in support.predicates.iter().map(|&index| transpose(index)) {
            if state.publish_condition(predicate, slot / 2, value, scratch, self.universe.size()) {
                changed_predicates.insert(predicate);
            }
            if slot == 0
                && symmetric_value_sides
                && state.publish_condition(predicate, 1, value, scratch, self.universe.size())
            {
                changed_predicates.insert(predicate);
            }
        }
        for index in support.opposite.indices() {
            scratch.remove(transpose(index));
        }
    }

    /// Partition head candidates into classes of mutually exchangeable values.
    ///
    /// Two candidates share a class only when transposing them preserves every
    /// input of the seeded join `stabilize_bindings` evaluates: the stable upper
    /// bound, each read relation's columns, rectangles, exact supports keyed by
    /// either value, membership in every other key's support, the singleton
    /// predicate index, and the IRI kind that selects head predicates. Splitting by
    /// every support row separates two values unless each row names both or
    /// neither, so self and mutual edges among members compare exactly. Rule
    /// constants, their semantic alternates, alternate-bearing predicates and
    /// predicate-keyed values stay singletons. The join is a deterministic function
    /// of those inputs, so it commutes with the transposition: one representative
    /// per class yields the exact support of every member.
    ///
    /// Partition refinement keeps this linear in the summary: each distinct set
    /// splits the classes it touches, and no per-value signature is materialized.
    /// Classes are ordered by their first candidate.
    fn value_classes(
        &self,
        rule: &Rule,
        state: &FlowSummary,
        upper: &[Domain],
        candidates: &[usize],
    ) -> Vec<Vec<usize>> {
        let size = self.universe.size();
        let mut position = vec![usize::MAX; size];
        for (index, &value) in candidates.iter().enumerate() {
            position[value] = index;
        }
        let candidate = |value: usize| (position[value] != usize::MAX).then(|| position[value]);
        let mut singletons = vec![false; candidates.len()];

        let mut scope = Domain::empty(size);
        for atom in &rule.body {
            let required = self.universe.domains(atom, upper, true);
            for (slot, values) in atom.iter().zip(&required) {
                if matches!(slot, Slot::Constant(_)) {
                    for value in values.indices().filter_map(candidate) {
                        singletons[value] = true;
                    }
                }
            }
            scope.union(&required[1]);
        }
        for slot in rule
            .heads
            .iter()
            .flatten()
            .chain(rule.cardinality_guards.iter().map(|(slot, _)| slot))
        {
            if let Slot::Constant(value) = slot
                && let Some(index) = candidate(*value)
            {
                singletons[index] = true;
            }
        }
        for predicate in scope.indices() {
            if let Some(TermValue::Iri(iri)) = self.universe.values.get(predicate)
                && let Some(alternate) = self.universe.semantics.alternate_predicate(iri)
            {
                for value in [predicate, self.universe.iri(alternate)] {
                    if let Some(index) = candidate(value) {
                        singletons[index] = true;
                    }
                }
            }
        }
        let mut iris = Vec::new();
        for (index, &value) in candidates.iter().enumerate() {
            match self.universe.values.get(value) {
                None => singletons[index] = true,
                Some(TermValue::Iri(iri)) => {
                    if state.relations.contains_key(&value)
                        || self.universe.semantics.alternate_predicate(iri).is_some()
                    {
                        singletons[index] = true;
                    }
                    iris.push(index);
                }
                Some(_) => {}
            }
        }

        let mut partition = CandidatePartition::new(&singletons);
        partition.split(iris);
        for domain in upper {
            if partition.is_discrete() {
                break;
            }
            partition.split(domain.indices().filter_map(candidate));
        }
        let mut content_ids: HashMap<SupportContent<'_>, u64> = HashMap::new();
        for (&predicate, relation) in &state.relations {
            if !scope.contains(predicate) || partition.is_discrete() {
                continue;
            }
            for column in &relation.columns {
                partition.split(column.indices().filter_map(candidate));
            }
            for rectangle in &relation.rectangles {
                for side in rectangle {
                    partition.split(side.indices().filter_map(candidate));
                }
            }
            for supports in [&relation.by_subject, &relation.by_object] {
                let mut labelled = Vec::new();
                let mut rows = Vec::new();
                for (&key, support) in supports {
                    let next = content_ids.len() as u64;
                    let id = *content_ids.entry(SupportContent(support)).or_insert(next);
                    if let Some(index) = candidate(key) {
                        labelled.push((index, id));
                    }
                    // An identical row splits identically wherever it recurs.
                    if id == next {
                        rows.push(support);
                    }
                }
                partition.split_labelled(labelled);
                for support in rows {
                    partition.split(support.indices().filter_map(candidate));
                }
            }
        }
        for index_by_value in [&state.predicates_by_subject, &state.predicates_by_object] {
            let mut by_predicate: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for (&value, predicates) in index_by_value {
                if let Some(index) = candidate(value) {
                    for predicate in predicates.indices().filter(|p| scope.contains(*p)) {
                        by_predicate.entry(predicate).or_default().push(index);
                    }
                }
            }
            for members in by_predicate.into_values() {
                partition.split(members);
            }
        }
        partition.classes(candidates)
    }

    /// Remove only a conditioned value whose constrained abstract join is empty,
    /// and publish that same join's opposite-column support without recomputing
    /// it. Every unconditioned value keeps the ordinary conservative column bound.
    fn refine_head(
        &self,
        state: &mut FlowSummary,
        rule_index: usize,
        visit: usize,
        rule: &Rule,
        head: &[Slot; 3],
        bindings: &[Domain],
        changed_predicates: &mut BTreeSet<usize>,
    ) -> [Domain; 3] {
        let mut domains = self.universe.domains(head, bindings, false);
        let symmetric_value_sides =
            domains[0] == domains[2] && self.has_symmetric_value_sides(rule, head);
        let mut native_witness_rectangle_published = false;
        for slot in 0..3 {
            if slot == 2 && symmetric_value_sides {
                continue;
            }
            let native_witness_slot = matches!(head[slot], Slot::Variable(variable) if rule.native_witnesses.contains(&variable));
            if native_witness_slot {
                // A native witness is forbidden from the body when the Rule is
                // lowered. Its seeded join is therefore identical for every
                // candidate witness: the exact head support is one Cartesian
                // rectangle, not one universe-width domain per candidate. Count
                // the selected cells for diagnostics without retaining them.
                let candidates = domains[slot]
                    .indices()
                    .filter(|value| self.conditioned.contains(*value))
                    .count();
                let started = std::time::Instant::now();
                if candidates >= 1_000 {
                    tracing::info!(
                        target: "pipeline_reasoning_detail",
                        phase = "refine-publish-native-head",
                        event = "start",
                        slot,
                        candidates,
                        rule_index,
                        visit,
                        producer = rule.name.as_str(),
                        variables = rule.variables,
                        body_atoms = rule.body.len(),
                        "large native value-flow refinement",
                    );
                }
                if !native_witness_rectangle_published {
                    state.publish_support_rectangle(&domains, &self.universe, changed_predicates);
                    native_witness_rectangle_published = true;
                }
                if candidates >= 1_000 {
                    tracing::info!(
                        target: "pipeline_reasoning_detail",
                        phase = "refine-publish-native-head",
                        event = "end",
                        slot,
                        candidates,
                        rule_index,
                        visit,
                        producer = rule.name.as_str(),
                        elapsed_ms = started.elapsed().as_millis(),
                        representation = "native-witness-rectangle",
                        "large native value-flow refinement",
                    );
                }
                continue;
            }
            let candidates: Vec<_> = domains[slot]
                .indices()
                .filter(|value| self.conditioned.contains(*value))
                .collect();
            let started = std::time::Instant::now();
            if candidates.len() >= 1_000 {
                tracing::info!(
                    target: "pipeline_reasoning_detail",
                    phase = "refine-publish-native-head",
                    event = "start",
                    slot,
                    candidates = candidates.len(),
                    rule_index,
                    visit,
                    producer = rule.name.as_str(),
                    variables = rule.variables,
                    body_atoms = rule.body.len(),
                    "large native value-flow refinement",
                );
            }
            let mut scratch = Domain::empty(self.universe.size());
            let mut classes_evaluated = candidates.len();
            if candidates.len() >= PARALLEL_HEAD_REFINEMENT_MIN_CANDIDATES
                && matches!(head[slot], Slot::Variable(_))
            {
                // Every class representative observes one immutable monotone
                // snapshot; members then publish in candidate order. A publication
                // reschedules every affected reader, including recursive and
                // dynamic readers, so a later fixed-point visit sees support added
                // by sibling candidates.
                let classes = self.value_classes(rule, state, bindings, &candidates);
                classes_evaluated = classes.len();
                let mut supports = Vec::with_capacity(classes.len());
                for batch in classes.chunks(PARALLEL_HEAD_REFINEMENT_CHUNK_SIZE) {
                    supports.par_extend(batch.par_iter().map(|members| {
                        let refinement = self
                            .bindings_for_head_value(rule, head, slot, members[0], state, bindings);
                        self.head_value_support(head, slot, refinement)
                    }));
                }
                let mut publications: Vec<(usize, usize)> = classes
                    .iter()
                    .enumerate()
                    .flat_map(|(class, members)| members.iter().map(move |&value| (value, class)))
                    .collect();
                publications.sort_unstable();
                for (value, class) in publications {
                    self.publish_head_support(
                        state,
                        slot,
                        value,
                        supports[class].as_ref(),
                        classes[class][0],
                        &mut scratch,
                        &mut domains,
                        symmetric_value_sides,
                        changed_predicates,
                    );
                }
            } else {
                for value in candidates.iter().copied() {
                    let refinement =
                        self.bindings_for_head_value(rule, head, slot, value, state, bindings);
                    let support = self.head_value_support(head, slot, refinement);
                    self.publish_head_support(
                        state,
                        slot,
                        value,
                        support.as_ref(),
                        value,
                        &mut scratch,
                        &mut domains,
                        symmetric_value_sides,
                        changed_predicates,
                    );
                }
            }
            if candidates.len() >= 1_000 {
                tracing::info!(
                    target: "pipeline_reasoning_detail",
                    phase = "refine-publish-native-head",
                    event = "end",
                    slot,
                    candidates = candidates.len(),
                    classes = classes_evaluated,
                    rule_index,
                    visit,
                    producer = rule.name.as_str(),
                    elapsed_ms = started.elapsed().as_millis(),
                    "large native value-flow refinement",
                );
            }
        }
        domains
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
        const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

        assert_eq!(enabled.len(), self.rules.len());
        let (_, refinements) = self.closure_enabled(input, enabled);
        let mut refined: Vec<_> = effects
            .iter()
            .zip(&self.rules)
            .zip(enabled)
            .zip(refinements)
            .map(|(((effect, rule), enabled), refinement)| {
                let mut effect = effect.clone();
                let Some(refinement) = enabled.then_some(refinement).flatten() else {
                    effect.writes.clear();
                    effect.completion_for.clear();
                    effect.reads.clear();
                    return effect;
                };
                for (pattern, domains) in effect.writes.iter_mut().zip(refinement.heads) {
                    self.restrict(pattern, domains);
                }
                for ((pattern, _), read) in effect.reads.iter_mut().zip(&rule.reads) {
                    let domains = if let Some(read) = read {
                        self.universe.domains(read, &refinement.bindings, true)
                    } else {
                        self.pattern_domains(pattern)
                    };
                    self.restrict(pattern, domains);
                }
                effect
            })
            .collect();
        for (index, rule) in self.rules.iter().enumerate() {
            if !enabled[index] {
                continue;
            }
            for list_read in &rule.list_reads {
                let Some(members) = &list_read.selected_members else {
                    continue;
                };
                let structure_is_mutable =
                    [list_read.selector_predicate.as_str(), RDF_FIRST, RDF_REST]
                        .into_iter()
                        .any(|predicate| {
                            let read = self
                                .ranged_pattern(StatementPattern::relation(Some(predicate), None));
                            refined
                                .iter()
                                .flat_map(|effect| &effect.writes)
                                .chain(&self.possible_writes)
                                .any(|write| read.reads_write(write, self.universe.semantics))
                        });
                if structure_is_mutable {
                    continue;
                }
                let Some((pattern, _)) = refined[index].reads.get_mut(list_read.read_index) else {
                    // An unreachable producer has every read erased above.
                    continue;
                };
                let mut domains = pattern
                    .ranges
                    .clone()
                    .unwrap_or_else(|| self.pattern_domains(pattern));
                let allowed = match list_read.read_column {
                    0 => members.clone(),
                    1 => self.universe.predicates(members),
                    2 => {
                        let mut markers = Domain::empty(self.universe.size());
                        for value in members.indices() {
                            markers.union(&self.universe.marker(value, &domains[1]));
                        }
                        markers
                    }
                    _ => unreachable!("validated statement read column"),
                };
                domains[list_read.read_column].intersect(&allowed);
                self.restrict(pattern, domains);
            }
            for selected_read in &rule.selected_reads {
                let Some(values) = &selected_read.selected_values else {
                    continue;
                };
                let selector = self.ranged_pattern(StatementPattern::relation(
                    Some(&selected_read.selector_predicate),
                    None,
                ));
                let selector_is_mutable = refined
                    .iter()
                    .flat_map(|effect| &effect.writes)
                    .chain(&self.possible_writes)
                    .any(|write| selector.reads_write(write, self.universe.semantics));
                if selector_is_mutable {
                    continue;
                }
                let Some((pattern, _)) = refined[index].reads.get_mut(selected_read.read_index)
                else {
                    continue;
                };
                let mut domains = pattern
                    .ranges
                    .clone()
                    .unwrap_or_else(|| self.pattern_domains(pattern));
                let allowed = match selected_read.read_column {
                    0 => values.clone(),
                    1 => self.universe.predicates(values),
                    2 => {
                        let mut markers = Domain::empty(self.universe.size());
                        for value in values.indices() {
                            markers.union(&self.universe.marker(value, &domains[1]));
                        }
                        markers
                    }
                    _ => unreachable!("validated statement read column"),
                };
                domains[selected_read.read_column].intersect(&allowed);
                self.restrict(pattern, domains);
            }
        }
        refined
    }

    fn closure(&self, input: &FlowSummary) -> FlowSummary {
        self.closure_enabled(input, &vec![true; self.rules.len()]).0
    }

    fn closure_enabled(
        &self,
        input: &FlowSummary,
        enabled: &[bool],
    ) -> (FlowSummary, Vec<Option<RuleRefinement>>) {
        let mut state = input.clone();
        let mut readers: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        let mut dynamic_readers = BTreeSet::new();
        for (index, (rule, enabled)) in self.rules.iter().zip(enabled).enumerate() {
            if !enabled {
                continue;
            }
            for atom in &rule.body {
                match atom[1] {
                    Slot::Constant(predicate) => {
                        let predicates = self
                            .universe
                            .predicates(&Domain::one(self.universe.size(), predicate));
                        for predicate in predicates.indices() {
                            readers.entry(predicate).or_default().insert(index);
                        }
                    }
                    Slot::Variable(_) => {
                        dynamic_readers.insert(index);
                    }
                }
            }
        }
        // The monotone closure is the same under any fair schedule, so visit writers
        // before their readers: strongly connected components of the publish graph in
        // topological order, each closed before the next starts. A producer outside a
        // recursive component runs once, after every input it reads is final. The edges
        // are exactly those the worklist below schedules along: a head publishes its
        // own predicate (any predicate when its predicate is a variable) and every
        // dynamic reader reads all of them.
        let active: Vec<usize> = enabled
            .iter()
            .enumerate()
            .filter_map(|(index, enabled)| enabled.then_some(index))
            .collect();
        let mut node_of = vec![usize::MAX; self.rules.len()];
        for (node, &index) in active.iter().enumerate() {
            node_of[index] = node;
        }
        let adjacency: Vec<Vec<usize>> = active
            .iter()
            .map(|&writer| {
                let mut dependents = BTreeSet::<usize>::new();
                for head in &self.rules[writer].heads {
                    match head[1] {
                        Slot::Constant(predicate) => {
                            dependents.extend(readers.get(&predicate).into_iter().flatten());
                            dependents.extend(&dynamic_readers);
                        }
                        Slot::Variable(_) => dependents.extend(&active),
                    }
                }
                dependents
                    .into_iter()
                    .map(|dependent| node_of[dependent])
                    .collect()
            })
            .collect();
        // Tarjan yields components in reverse topological order.
        let mut components = purrdf_core::graph::tarjan_scc(&adjacency);
        components.reverse();
        let mut component_of = vec![usize::MAX; self.rules.len()];
        for (component, members) in components.iter_mut().enumerate() {
            for node in members.iter_mut() {
                *node = active[*node];
                component_of[*node] = component;
            }
            members.sort_unstable();
        }
        let largest = components.iter().map(Vec::len).max().unwrap_or(0);
        tracing::info!(
            target: "pipeline_reasoning_detail",
            phase = "refine-closure-components",
            rules = active.len(),
            components = components.len(),
            recursive = components.iter().filter(|members| members.len() > 1).count(),
            largest,
            "value-flow closure schedule",
        );
        let mut queued = vec![false; self.rules.len()];
        let mut visits = vec![0usize; self.rules.len()];
        let mut refinements = vec![None; self.rules.len()];
        for (component, members) in components.into_iter().enumerate() {
            // A FIFO worklist keeps the transfer order deterministic while putting a
            // self-rescheduled rule behind peers that were already pending. Ordered
            // minimum extraction can starve those peers during recursive closure.
            let mut pending: VecDeque<_> = members.into_iter().collect();
            for &index in &pending {
                queued[index] = true;
            }
            while let Some(index) = pending.pop_front() {
                queued[index] = false;
                visits[index] += 1;
                let rule = &self.rules[index];
                let Some(bindings) = self.bindings(rule, &state) else {
                    refinements[index] = None;
                    continue;
                };
                let mut changed_predicates = BTreeSet::new();
                let mut heads = Vec::with_capacity(rule.heads.len());
                for head in &rule.heads {
                    let domains = self.refine_head(
                        &mut state,
                        index,
                        visits[index],
                        rule,
                        head,
                        &bindings,
                        &mut changed_predicates,
                    );
                    state.publish_columns_tracking(
                        &domains,
                        &self.universe,
                        &mut changed_predicates,
                    );
                    heads.push(domains);
                }
                refinements[index] = Some(RuleRefinement { bindings, heads });
                if !changed_predicates.is_empty() {
                    // A later component runs every member once its inputs are final; an
                    // earlier one cannot read this publication (it would share a cycle).
                    let mut schedule = |dependent: usize| {
                        debug_assert!(component_of[dependent] >= component);
                        if component_of[dependent] == component && !queued[dependent] {
                            queued[dependent] = true;
                            pending.push_back(dependent);
                        }
                    };
                    for dependent in &dynamic_readers {
                        schedule(*dependent);
                    }
                    for predicate in changed_predicates {
                        if let Some(dependents) = readers.get(&predicate) {
                            for dependent in dependents {
                                schedule(*dependent);
                            }
                        }
                    }
                }
            }
        }
        (state, refinements)
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
        let mut rules = self.rules.clone();
        for rule in &mut rules {
            for read in &mut rule.list_reads {
                if let Some(members) = &mut read.selected_members {
                    *members = members.resized(universe.size());
                }
            }
            for read in &mut rule.selected_reads {
                if let Some(values) = &mut read.selected_values {
                    *values = values.resized(universe.size());
                }
            }
        }
        Some(Self {
            native_witness_domain: universe.native_witness_domain(),
            cardinality_domains: cardinality_domains(&universe, &rules),
            conditioned: self.conditioned.resized(universe.size()),
            universe,
            rules,
            operator_inputs: self.operator_inputs.clone(),
            cardinality_inputs: self.cardinality_inputs.clone(),
            possible_writes: self.possible_writes.clone(),
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
