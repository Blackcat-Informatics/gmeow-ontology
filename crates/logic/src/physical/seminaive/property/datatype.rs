// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Completed datatype definitions compiled once into a native expression DAG.
//! The store and caches belong to one asserting world. Shared subexpressions are
//! lowered once, without RDF conversion, recursive tree expansion or last-write
//! selection of conjunctive facets. Every plan retains its actual source evidence.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

mod extent;
use extent::ValueSpaceBounds;

use super::{Fact, ListCache, RelationStore, seminaive_err};
use crate::facts::TermId;
use crate::physical::store::Bound;
use crate::reason::value::{LiteralMeaning, LiteralValue, NativeValues};
use purrdf::TermValue;

/// The same intrinsic capacity authority used by compiled datatype plans.
pub(crate) fn finite_named_capacity(iri: &str) -> Option<u128> {
    extent::named(iri).exact_count()
}

const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#datatypeComplementOf";
const ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const CONSTRUCTORS: &[&str] = &[
    ONE_OF,
    INTERSECTION,
    UNION,
    COMPLEMENT,
    ON_DATATYPE,
    RESTRICTIONS,
];
const FACETS: &[&str] = &[
    "http://www.w3.org/2001/XMLSchema#minInclusive",
    "http://www.w3.org/2001/XMLSchema#maxInclusive",
    "http://www.w3.org/2001/XMLSchema#minExclusive",
    "http://www.w3.org/2001/XMLSchema#maxExclusive",
    "http://www.w3.org/2001/XMLSchema#length",
    "http://www.w3.org/2001/XMLSchema#minLength",
    "http://www.w3.org/2001/XMLSchema#maxLength",
    "http://www.w3.org/2001/XMLSchema#pattern",
    "http://www.w3.org/2001/XMLSchema#totalDigits",
    "http://www.w3.org/2001/XMLSchema#fractionDigits",
    "http://www.w3.org/2001/XMLSchema#whiteSpace",
    "http://www.w3.org/2001/XMLSchema#explicitTimezone",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#langRange",
];

/// Definitions may strengthen as facets or constructors arrive. Their complete
/// producer extensions are mandatory predecessors, not positive round snapshots.
pub(super) fn reads() -> impl Iterator<Item = &'static str> {
    CONSTRUCTORS
        .iter()
        .copied()
        .chain(FACETS.iter().copied())
        .chain([super::list::FIRST, super::list::REST])
}

#[derive(Debug)]
enum Facet {
    Ordered {
        kind: usize,
        bound: Arc<LiteralValue>,
    },
    Length {
        kind: usize,
        bound: u128,
    },
    /// Structurally valid XSD facet outside the certified native value-space
    /// fragment. It remains in the compiled definition so every consumer gets
    /// an honest undecided result instead of turning a capability boundary into
    /// an execution error or silently dropping the facet.
    Unsupported,
}

impl Facet {
    fn holds(&self, value: &LiteralValue) -> Option<bool> {
        match self {
            Self::Ordered { kind, bound } => {
                let order = value.facet_order(bound)?;
                Some(match kind {
                    0 => !order.is_lt(),
                    1 => !order.is_gt(),
                    2 => order.is_gt(),
                    3 => order.is_lt(),
                    _ => unreachable!("prepared order facet"),
                })
            }
            Self::Length { kind, bound } => {
                let length = value.facet_length()?;
                Some(match kind {
                    4 => length == *bound,
                    5 => length >= *bound,
                    6 => length <= *bound,
                    _ => unreachable!("prepared length facet"),
                })
            }
            Self::Unsupported => None,
        }
    }
}

#[derive(Debug)]
enum Node {
    Named(String),
    Enumeration {
        members: Vec<Arc<LiteralValue>>,
        bytes: usize,
    },
    Complement(usize),
    Intersection(Vec<usize>),
    Union(Vec<usize>),
    Restriction {
        base: usize,
        facets: Vec<Facet>,
    },
}

/// Node indices are execution-local handles, never serialized or proof identity.
#[derive(Debug)]
pub(crate) struct DatatypePlan {
    nodes: Vec<Node>,
    root: usize,
    pub(crate) premises: Vec<Fact>,
    bytes: usize,
    capacity: std::sync::OnceLock<ValueSpaceBounds>,
}

fn conjunction(values: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for value in values {
        match value {
            Some(false) => return Some(false),
            Some(true) => {}
            None => unknown = true,
        }
    }
    if unknown { None } else { Some(true) }
}

fn disjunction(values: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for value in values {
        match value {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    if unknown { None } else { Some(false) }
}

impl DatatypePlan {
    /// A complete datatype model requires every enumerated literal to have an
    /// admitted interpretation; positive membership alone has weaker obligations.
    pub(crate) fn model_values_admitted(&self) -> bool {
        self.nodes.iter().all(|node| match node {
            Node::Enumeration { members, .. } => members
                .iter()
                .all(|value| !matches!(value.meaning, LiteralMeaning::Opaque(_))),
            _ => true,
        })
    }

    /// Bounds come from this same admitted definition DAG, with exact native
    /// enumeration equality and PurRDF primitive range decisions.
    pub(crate) fn admits_count(&self, count: u128) -> Option<bool> {
        self.capacity
            .get_or_init(|| extent::analyze(&self.nodes, self.root))
            .admits(count)
    }

    /// One topological evaluation visits shared expression nodes once per value.
    /// Complement negates a datatype membership, never absence of an RDF fact.
    pub(crate) fn contains(&self, source: &TermValue, values: &mut NativeValues) -> Option<bool> {
        if !matches!(source, TermValue::Literal { .. }) {
            return Some(false);
        }
        self.contains_value(&values.literal(source))
    }

    /// Shared interpreted input for conjunctions of already compiled plans.
    pub(crate) fn contains_value(&self, interpretation: &LiteralValue) -> Option<bool> {
        // Leaf plans cover the common named range and enumeration probes. They
        // need no temporary vector for expression-node results.
        match &self.nodes[self.root] {
            Node::Named(iri) => return interpretation.named_datatype(iri),
            Node::Enumeration { members, .. } => {
                return disjunction(
                    members
                        .iter()
                        .map(|member| interpretation.same_value(member)),
                );
            }
            _ => {}
        }
        let mut results = Vec::<Option<bool>>::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let result = match node {
                Node::Named(iri) => interpretation.named_datatype(iri),
                Node::Enumeration { members, .. } => disjunction(
                    members
                        .iter()
                        .map(|member| interpretation.same_value(member)),
                ),
                Node::Complement(inner) => results[*inner].map(|value| !value),
                Node::Intersection(members) => {
                    conjunction(members.iter().map(|member| results[*member]))
                }
                Node::Union(members) => disjunction(members.iter().map(|member| results[*member])),
                Node::Restriction { base, facets } => conjunction(
                    std::iter::once(results[*base])
                        .chain(facets.iter().map(|facet| facet.holds(interpretation))),
                ),
            };
            results.push(result);
        }
        results[self.root]
    }
}

/// One world's completed definitions, shared across its strata and rounds. Plans
/// bypass retention beyond 64 entries / 512 KiB; misses preserve identical execution.
#[derive(Default)]
pub(crate) struct DatatypeCache {
    entries: BTreeMap<TermId, Arc<DatatypePlan>>,
    bytes: usize,
}

impl DatatypeCache {
    /// The caller's signed effect certificate must complete all `reads()` first.
    pub(crate) fn prepare(
        &mut self,
        rel: &RelationStore,
        root: &TermValue,
        lists: &mut ListCache,
        values: &mut NativeValues,
    ) -> gmeow_errors::Result<Arc<DatatypePlan>> {
        if !matches!(root, TermValue::Iri(_) | TermValue::Blank { .. }) {
            return Err(seminaive_err("a datatype expression must be a resource"));
        }
        let root = rel
            .term_id(root)
            .ok_or_else(|| seminaive_err("a datatype expression is absent"))?;
        if let Some(plan) = self.entries.get(&root) {
            return Ok(Arc::clone(plan));
        }
        let plan = Arc::new(
            Compiler {
                rel,
                lists,
                values,
                premises: Vec::new(),
            }
            .compile(root)?,
        );
        if self.entries.len() < 64 && self.bytes.saturating_add(plan.bytes) <= 512 * 1024 {
            self.bytes += plan.bytes;
            self.entries.insert(root, Arc::clone(&plan));
        }
        Ok(plan)
    }
}

/// Raw child handles are native term IDs. A topological pass resolves them into
/// local node indices, so deep definitions cannot overflow a recursive compiler.
enum RawNode {
    Named(String),
    Enumeration {
        members: Vec<Arc<LiteralValue>>,
        bytes: usize,
    },
    Complement(TermId),
    Intersection(Vec<TermId>),
    Union(Vec<TermId>),
    Restriction {
        base: TermId,
        facets: Vec<Facet>,
    },
}

impl RawNode {
    fn children(&self) -> Vec<TermId> {
        match self {
            Self::Complement(inner) | Self::Restriction { base: inner, .. } => vec![*inner],
            Self::Intersection(members) | Self::Union(members) => members.clone(),
            Self::Named(_) | Self::Enumeration { .. } => Vec::new(),
        }
    }

    fn lower(self, indices: &BTreeMap<TermId, usize>) -> Node {
        let index = |term: TermId| indices[&term];
        match self {
            Self::Named(iri) => Node::Named(iri),
            Self::Enumeration { members, bytes } => Node::Enumeration { members, bytes },
            Self::Complement(inner) => Node::Complement(index(inner)),
            Self::Intersection(members) => {
                Node::Intersection(members.into_iter().map(index).collect())
            }
            Self::Union(members) => Node::Union(members.into_iter().map(index).collect()),
            Self::Restriction { base, facets } => Node::Restriction {
                base: index(base),
                facets,
            },
        }
    }
}

struct Compiler<'a> {
    rel: &'a RelationStore,
    lists: &'a mut ListCache,
    values: &'a mut NativeValues,
    premises: Vec<Fact>,
}

impl Compiler<'_> {
    fn compile(mut self, root: TermId) -> gmeow_errors::Result<DatatypePlan> {
        let mut pending = VecDeque::from([root]);
        let mut raw = BTreeMap::new();
        while let Some(term) = pending.pop_front() {
            if raw.contains_key(&term) {
                continue;
            }
            let node = self.read_node(term)?;
            pending.extend(node.children());
            raw.insert(term, node);
        }
        let mut incoming = BTreeMap::new();
        let mut parents = BTreeMap::<TermId, Vec<TermId>>::new();
        for (term, node) in &raw {
            let children: BTreeSet<_> = node.children().into_iter().collect();
            incoming.insert(*term, children.len());
            for child in children {
                parents.entry(child).or_default().push(*term);
            }
        }
        let mut ready: BTreeSet<_> = incoming
            .iter()
            .filter_map(|(term, count)| (*count == 0).then_some(*term))
            .collect();
        let mut nodes = Vec::with_capacity(raw.len());
        let mut indices = BTreeMap::new();
        while let Some(term) = ready.pop_first() {
            let node = raw
                .remove(&term)
                .expect("uncompiled datatype node")
                .lower(&indices);
            indices.insert(term, nodes.len());
            nodes.push(node);
            for parent in parents.get(&term).into_iter().flatten() {
                let count = incoming.get_mut(parent).expect("datatype parent");
                *count -= 1;
                if *count == 0 {
                    ready.insert(*parent);
                }
            }
        }
        if !raw.is_empty() {
            return Err(seminaive_err(
                "cyclic datatype expressions cannot certify a finite definition DAG",
            ));
        }
        // The payload-bearing copies are native names, enumeration values and
        // proof facts. Numeric facet interpretations are bounded scalar values.
        let bytes = std::mem::size_of::<DatatypePlan>()
            .saturating_add(nodes.capacity() * std::mem::size_of::<Node>())
            .saturating_add(self.premises.capacity() * std::mem::size_of::<Fact>())
            .saturating_add(
                self.premises
                    .iter()
                    .map(|fact| {
                        term_bytes(&fact.subject) + fact.predicate.len() + term_bytes(&fact.object)
                    })
                    .sum::<usize>(),
            )
            .saturating_add(nodes.iter().map(node_bytes).sum::<usize>());
        Ok(DatatypePlan {
            capacity: std::sync::OnceLock::new(),
            nodes,
            root: indices[&root],
            premises: self.premises,
            bytes,
        })
    }

    fn values(&mut self, subject: TermId, predicate: &'static str) -> BTreeSet<TermId> {
        let mut values = BTreeSet::new();
        let mut cursor = self.rel.select_semantic(predicate, Bound::Subject(subject));
        while let Some((subject, object, _, predicate)) = cursor.next() {
            values.insert(object);
            self.premises.push(Fact {
                subject: self.rel.interner().resolve(subject).clone(),
                predicate: predicate.to_owned(),
                object: self.rel.interner().resolve(object).clone(),
            });
        }
        values
    }

    fn unique(
        &mut self,
        subject: TermId,
        predicate: &'static str,
    ) -> gmeow_errors::Result<Option<TermId>> {
        let values = self.values(subject, predicate);
        if values.len() > 1 {
            return Err(seminaive_err(format!(
                "datatype definition has multiple {predicate} values"
            )));
        }
        Ok(values.first().copied())
    }

    fn resource(&self, term: TermId) -> gmeow_errors::Result<TermId> {
        if matches!(
            self.rel.interner().resolve(term),
            TermValue::Iri(_) | TermValue::Blank { .. }
        ) {
            Ok(term)
        } else {
            Err(seminaive_err("a datatype operand must be a resource"))
        }
    }

    fn members(&mut self, root: TermId) -> gmeow_errors::Result<Vec<TermId>> {
        let list = self
            .lists
            .read(self.rel, self.rel.interner().resolve(root))?
            .ok_or_else(|| {
                seminaive_err("a completed datatype definition has an incomplete list")
            })?;
        self.premises.extend(list.premises(self.rel));
        Ok(list.members.clone())
    }

    fn read_node(&mut self, term: TermId) -> gmeow_errors::Result<RawNode> {
        self.resource(term)?;
        let mut fields = BTreeMap::new();
        for predicate in CONSTRUCTORS {
            if let Some(value) = self.unique(term, predicate)? {
                fields.insert(*predicate, value);
            }
        }
        if let TermValue::Iri(iri) = self.rel.interner().resolve(term)
            && NativeValues::is_named_datatype(iri)
        {
            if !fields.is_empty() {
                return Err(seminaive_err(
                    "an intrinsic datatype cannot be silently redefined by a constructor",
                ));
            }
            return Ok(RawNode::Named(iri.clone()));
        }
        if fields.len() == 2
            && fields.contains_key(ON_DATATYPE)
            && fields.contains_key(RESTRICTIONS)
        {
            let base = self.resource(fields[ON_DATATYPE])?;
            let facets = self.facets(fields[RESTRICTIONS])?;
            return Ok(RawNode::Restriction { base, facets });
        }
        if fields.len() != 1 {
            return Err(seminaive_err(
                "a datatype expression requires one complete constructor",
            ));
        }
        let (predicate, operand) = fields.pop_first().expect("one datatype constructor");
        match predicate {
            COMPLEMENT => Ok(RawNode::Complement(self.resource(operand)?)),
            ONE_OF => {
                let members = self.members(operand)?;
                let mut values = Vec::with_capacity(members.len());
                let mut bytes = 0usize;
                for member in members {
                    let value = self.rel.interner().resolve(member);
                    if !matches!(value, TermValue::Literal { .. }) {
                        return Err(seminaive_err(
                            "a datatype enumeration member must be a literal",
                        ));
                    }
                    // Interpret each enumeration operand once at compilation. Even
                    // values too large for the scalar cache are not parsed per probe.
                    bytes = bytes
                        .saturating_add(term_bytes(value))
                        .saturating_add(std::mem::size_of::<TermValue>())
                        .saturating_add(
                            std::mem::size_of::<LiteralValue>() + 2 * std::mem::size_of::<usize>(),
                        );
                    values.push(self.values.literal(value));
                }
                Ok(RawNode::Enumeration {
                    members: values,
                    bytes,
                })
            }
            INTERSECTION | UNION => {
                let members = self.members(operand)?;
                for member in &members {
                    self.resource(*member)?;
                }
                Ok(if predicate == INTERSECTION {
                    RawNode::Intersection(members)
                } else {
                    RawNode::Union(members)
                })
            }
            _ => Err(seminaive_err(
                "an onDatatype/withRestrictions pair must be complete",
            )),
        }
    }

    fn facets(&mut self, root: TermId) -> gmeow_errors::Result<Vec<Facet>> {
        let mut facets = Vec::new();
        for member in self.members(root)? {
            self.resource(member)?;
            let before = facets.len();
            for (kind, predicate) in FACETS.iter().enumerate() {
                for value in self.values(member, predicate) {
                    let source = self.rel.interner().resolve(value);
                    if !matches!(source, TermValue::Literal { .. }) {
                        return Err(seminaive_err("a datatype facet operand must be a literal"));
                    }
                    let facet = match kind {
                        0..=3 => {
                            let bound = self.values.literal(source);
                            if !matches!(
                                bound.meaning,
                                LiteralMeaning::Rational(_)
                                    | LiteralMeaning::Native(
                                        purrdf::xsd::XsdValue::Float(_)
                                            | purrdf::xsd::XsdValue::Double(_)
                                            | purrdf::xsd::XsdValue::DateTime(_)
                                            | purrdf::xsd::XsdValue::Date(_)
                                            | purrdf::xsd::XsdValue::Time(_)
                                            | purrdf::xsd::XsdValue::Duration(_)
                                            | purrdf::xsd::XsdValue::Gregorian(_)
                                    )
                            ) {
                                return Err(seminaive_err(
                                    "a datatype order facet has an unordered or uninterpreted operand",
                                ));
                            }
                            Facet::Ordered { kind, bound }
                        }
                        4..=6 => Facet::Length {
                            kind,
                            bound: self.values.cardinality(source).ok_or_else(|| {
                                seminaive_err(
                                    "a datatype length facet requires a non-negative integer field",
                                )
                            })?,
                        },
                        _ => Facet::Unsupported,
                    };
                    facets.push(facet);
                }
            }
            if facets.len() == before {
                return Err(seminaive_err(
                    "a datatype facet record carries no admitted facet",
                ));
            }
        }
        Ok(facets)
    }
}

fn term_bytes(term: &TermValue) -> usize {
    match term {
        TermValue::Iri(iri) => iri.len(),
        TermValue::Blank { label, .. } => label.len() + 64,
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => lexical_form.len() + datatype.len() + language.as_ref().map_or(0, String::len),
        _ => 0, // Constructor, enumeration and facet admission exclude quoted terms.
    }
}

fn node_bytes(node: &Node) -> usize {
    match node {
        Node::Named(iri) => iri.len(),
        Node::Enumeration { members, bytes } => {
            members.capacity() * std::mem::size_of::<Arc<LiteralValue>>() + bytes
        }
        Node::Complement(_) => 0,
        Node::Intersection(members) | Node::Union(members) => {
            members.capacity() * std::mem::size_of::<usize>()
        }
        Node::Restriction { facets, .. } => {
            facets.capacity() * (std::mem::size_of::<Facet>() + std::mem::size_of::<LiteralValue>())
        }
    }
}

#[path = "datatype.tests.rs"]
#[cfg(test)]
mod tests;
