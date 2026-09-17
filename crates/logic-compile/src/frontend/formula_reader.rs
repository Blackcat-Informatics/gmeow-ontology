// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dataset-bound formula reconstruction shared by all semantic owners.
//!
//! The memo contains derived values, never source ownership or admission evidence.
//! Its native term IDs are valid only in the borrowed, immutable dataset. Modal
//! translation context is part of each key. Eviction changes work, never meaning.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

use super::{
    ACTUAL_WORLD_IRI, FormulaSource, MODAL_ACCESSIBILITY_RELATIONS, SelectedFormulaGraph,
    formula_objects, logic_iri,
};
use crate::graphutil::{
    Node, Subject, subject_id, subject_of, subject_str, term_as_subject, term_is_literal, term_str,
};
use crate::ir::{Formula, Term};

const MAX_MEMO_ENTRIES: usize = 4096;
const MAX_MEMO_BYTES: usize = 8 * 1024 * 1024;

#[cfg(not(test))]
mod reconstruction;
#[cfg(test)]
#[path = "formula_reader/test_reconstruction.rs"]
mod reconstruction;

/// A frontend failure before it is attached to an owner's diagnostic stream.
/// Keeping the typed payload avoids serializing or flattening a live `Diag`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FormulaFailure {
    focus: String,
    detail: String,
}

pub(super) type ReadResult<T> = Result<T, FormulaFailure>;

fn formula_err_for(focus: impl Into<String>, detail: impl Into<String>) -> FormulaFailure {
    FormulaFailure {
        focus: focus.into(),
        detail: detail.into(),
    }
}

pub(super) fn formula_err(node: &Subject, detail: impl Into<String>) -> FormulaFailure {
    formula_err_for(subject_str(node), detail)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Translation {
    Plain,
    AtWorld { world: Term, depth: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemoKey<Id> {
    source: Id,
    translation: Translation,
}

struct MemoEntry {
    result: ReadResult<Formula>,
    bytes: usize,
}

/// One reconstruction session over one selected source dataset.
///
/// Successful expansions may be reused at any depth. A failed root is reused
/// only at another root: cycle diagnostics depend on the active source path and
/// must never be transplanted into a different recursive walk.
pub(crate) struct FormulaReader<'a, D: DatasetView + ?Sized = RdfDataset> {
    dataset: &'a D,
    graph: GraphMatch<D::Id>,
    world_prefix: String,
    temporal_prefix: String,
    interval_prefix: String,
    /// Successful size admissions belong to this immutable dataset and graph.
    admitted_sources: HashSet<D::Id>,
    memo: HashMap<MemoKey<D::Id>, MemoEntry>,
    insertion_order: VecDeque<MemoKey<D::Id>>,
    retained_bytes: usize,
    max_bytes: usize,
    observer: reconstruction::Observer,
}

impl<'a, D: DatasetView + ?Sized> FormulaReader<'a, D> {
    pub(crate) fn new(dataset: &'a D) -> Self {
        Self::in_graph(dataset, GraphMatch::Default)
    }

    pub(super) fn in_graph(dataset: &'a D, graph: GraphMatch<D::Id>) -> Self {
        let source = SelectedFormulaGraph { dataset, graph };
        let [world_prefix, temporal_prefix, interval_prefix] = fresh_variable_prefixes(&source);
        Self {
            dataset,
            graph,
            world_prefix,
            temporal_prefix,
            interval_prefix,
            admitted_sources: HashSet::new(),
            memo: HashMap::new(),
            insertion_order: VecDeque::new(),
            retained_bytes: 0,
            max_bytes: MAX_MEMO_BYTES,
            observer: reconstruction::Observer::default(),
        }
    }

    pub(crate) fn dataset(&self) -> &'a D {
        self.dataset
    }

    pub(super) fn source(&self) -> SelectedFormulaGraph<'a, D> {
        SelectedFormulaGraph {
            dataset: self.dataset,
            graph: self.graph,
        }
    }

    pub(super) fn temporal_variable(&self, interval: bool, depth: usize) -> String {
        format!(
            "{}{depth}",
            if interval {
                &self.interval_prefix
            } else {
                &self.temporal_prefix
            }
        )
    }

    pub(super) fn read_in_context(
        &mut self,
        node: &Subject,
        world: Term,
    ) -> gmeow_errors::Result<Formula> {
        self.admit(node)?;
        self.expand(
            node,
            &Translation::AtWorld { world, depth: 0 },
            &mut Vec::new(),
        )
        .map_err(|error| super::formula_err_for(error.focus, error.detail))
    }

    pub(crate) fn read(&mut self, node: &Subject) -> gmeow_errors::Result<Formula> {
        self.admit(node)?;
        self.expand(node, &Translation::Plain, &mut Vec::new())
            .map_err(|error| super::formula_err_for(error.focus, error.detail))
    }

    fn admit(&mut self, node: &Subject) -> gmeow_errors::Result<()> {
        let id = subject_id(self.dataset, node);
        if id.is_some_and(|id| self.admitted_sources.contains(&id)) {
            return Ok(());
        }
        super::admission::admit(&self.source(), node)?;
        if let Some(id) = id
            && self.admitted_sources.len() < MAX_MEMO_ENTRIES
        {
            self.admitted_sources.insert(id);
        }
        Ok(())
    }

    pub(super) fn expand(
        &mut self,
        node: &Subject,
        context: &Translation,
        active: &mut Vec<D::Id>,
    ) -> ReadResult<Formula> {
        let context = match explicit_formula_context(&self.source(), node)? {
            Some(world) => Translation::AtWorld {
                world,
                depth: match context {
                    Translation::Plain => 0,
                    Translation::AtWorld { depth, .. } => *depth,
                },
            },
            None => context.clone(),
        };
        let context = &context;
        let Some(source) = subject_id(self.dataset, node) else {
            // A missing requested IRI has no local ID to memoize. The same strict
            // constructor reader reports the absence; no source is synthesized.
            return self.reconstruct(node, context, active);
        };
        if let Some(start) = active.iter().position(|member| *member == source) {
            let mut members: Vec<_> = active[start..]
                .iter()
                .map(|id| subject_str(&subject_of(self.dataset, *id)))
                .collect();
            members.sort();
            members.dedup();
            return Err(formula_err_for(
                members[0].clone(),
                format!(
                    "logic:Formula recursive constructor cycle among {}",
                    members.join(", ")
                ),
            ));
        }
        let is_root = active.is_empty();
        let key = MemoKey {
            source,
            translation: context.clone(),
        };
        if let Some(entry) = self.memo.get(&key)
            && (entry.result.is_ok() || is_root)
        {
            return entry.result.clone();
        }
        active.push(source);
        let result = self.reconstruct(node, context, active);
        let popped = active.pop();
        debug_assert_eq!(popped, Some(source));
        if result.is_ok() || is_root {
            self.retain(key, &result);
        }
        result
    }

    fn reconstruct(
        &mut self,
        node: &Subject,
        context: &Translation,
        active: &mut Vec<D::Id>,
    ) -> ReadResult<Formula> {
        self.observer.record();
        let selection = self.source();
        let store = &selection;
        let node_id = subject_str(node);
        let relation = formula_objects(store, node, "relation");
        let not = formula_objects(store, node, "not");
        let and = formula_objects(store, node, "and");
        let or = formula_objects(store, node, "or");
        let iff = formula_objects(store, node, "iff");
        let antecedent = formula_objects(store, node, "antecedent");
        let consequent = formula_objects(store, node, "consequent");
        let forall = formula_objects(store, node, "forall");
        let exists = formula_objects(store, node, "exists");
        let necessarily = formula_objects(store, node, "necessarily");
        let possibly = formula_objects(store, node, "possibly");
        let next = formula_objects(store, node, "next");
        let eventually = formula_objects(store, node, "eventually");
        let globally = formula_objects(store, node, "globally");
        let until = formula_objects(store, node, "until");
        let until_left = formula_objects(store, node, "untilLeft");

        // `logic:overAccessibility` is a SATELLITE of a modal node, not a constructor family:
        // it is read separately by `read_over_accessibility` and must never enter the
        // exactly-one-family guard (exactly like `logic:quantifiedVariable`/`logic:argument`,
        // which are also read as satellites of their owning family).
        let families = [
            ("relation", !relation.is_empty()),
            ("not", !not.is_empty()),
            ("and", !and.is_empty()),
            ("or", !or.is_empty()),
            ("iff", !iff.is_empty()),
            (
                "antecedent/consequent",
                !antecedent.is_empty() || !consequent.is_empty(),
            ),
            ("forall", !forall.is_empty()),
            ("exists", !exists.is_empty()),
            ("necessarily", !necessarily.is_empty()),
            ("possibly", !possibly.is_empty()),
            ("next", !next.is_empty()),
            ("eventually", !eventually.is_empty()),
            ("globally", !globally.is_empty()),
            ("until", !until.is_empty() || !until_left.is_empty()),
        ];
        let present: Vec<&str> = families
            .iter()
            .filter_map(|(name, is_present)| is_present.then_some(*name))
            .collect();
        if present.len() != 1 {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {node_id} requires exactly one constructor family; found {} ({})",
                    present.len(),
                    present.join(", ")
                ),
            ));
        }

        match present[0] {
            "relation" => {
                if relation.len() != 1 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} requires exactly one logic:relation; found {}",
                            relation.len()
                        ),
                    ));
                }
                let Node::Iri(relation_iri) = &relation[0] else {
                    return Err(formula_err(
                        node,
                        format!("logic:Formula {node_id} requires an IRI-valued logic:relation"),
                    ));
                };
                let relation =
                    Term::iri(relation_iri.clone()).map_err(|e| formula_err(node, e.message()))?;
                let mut args = parse_term_carriers(store, node, "argument", &mut Vec::new())?;
                if args.is_empty() {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} atomic predication requires at least one logic:argument"
                        ),
                    ));
                }
                if let Translation::AtWorld { world, .. } = context {
                    args.insert(0, world.clone());
                }
                Formula::atom(relation, args).map_err(|e| formula_err(node, e.message()))
            }
            "not" => {
                let child = one_child_subject(store, node, "not")?;
                Ok(Formula::Not(Box::new(
                    self.expand(&child, context, active)?,
                )))
            }
            "and" | "or" => {
                let link = present[0];
                let child_terms = if link == "and" { &and } else { &or };
                if child_terms.len() < 2 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} logic:{link} requires at least two operands; found {}",
                            child_terms.len()
                        ),
                    ));
                }
                let mut parsed = Vec::with_capacity(child_terms.len());
                for child in child_terms {
                    let child = term_as_subject(child).ok_or_else(|| {
                        formula_err(
                            node,
                            format!(
                                "logic:Formula {node_id} has a non-resource logic:{link} operand"
                            ),
                        )
                    })?;
                    parsed.push(self.expand(&child, context, active)?);
                }
                Ok(if link == "and" {
                    Formula::And(parsed)
                } else {
                    Formula::Or(parsed)
                })
            }
            "iff" => {
                if iff.len() != 2 {
                    return Err(formula_err(
                        node,
                        format!(
                            "logic:Formula {node_id} logic:iff requires exactly two operands; found {}",
                            iff.len()
                        ),
                    ));
                }
                let a = term_as_subject(&iff[0]).ok_or_else(|| {
                    formula_err(
                        node,
                        format!("logic:Formula {node_id} has a non-resource logic:iff operand"),
                    )
                })?;
                let b = term_as_subject(&iff[1]).ok_or_else(|| {
                    formula_err(
                        node,
                        format!("logic:Formula {node_id} has a non-resource logic:iff operand"),
                    )
                })?;
                Ok(Formula::Iff(
                    Box::new(self.expand(&a, context, active)?),
                    Box::new(self.expand(&b, context, active)?),
                ))
            }
            "antecedent/consequent" => {
                let a = one_child_subject(store, node, "antecedent")?;
                let c = one_child_subject(store, node, "consequent")?;
                Ok(Formula::Implies(
                    Box::new(self.expand(&a, context, active)?),
                    Box::new(self.expand(&c, context, active)?),
                ))
            }
            "forall" | "exists" => {
                let link = present[0];
                let body_node = one_child_subject(store, node, link)?;
                let vars = parse_bound_vars(store, node)?;
                let body = Box::new(self.expand(&body_node, context, active)?);
                Ok(if link == "forall" {
                    Formula::Forall { vars, body }
                } else {
                    Formula::Exists { vars, body }
                })
            }
            "necessarily" | "possibly" => self.expand_modal(node, present[0], context, active),
            "next" | "eventually" | "globally" | "until" => {
                super::temporal::expand(self, node, present[0], context, active)
            }
            _ => unreachable!("constructor family was selected from a closed local array"),
        }
    }

    fn expand_modal(
        &mut self,
        node: &Subject,
        link: &str,
        context: &Translation,
        active: &mut Vec<D::Id>,
    ) -> ReadResult<Formula> {
        let (world, depth) = match context {
            Translation::Plain => (Term::Iri(ACTUAL_WORLD_IRI.to_owned()), 0),
            Translation::AtWorld { world, depth } => (world.clone(), *depth),
        };
        let body = one_child_subject(&self.source(), node, link)?;
        let accessibility = Term::Iri(read_over_accessibility(&self.source(), node)?);
        let name = format!("{}{depth}", self.world_prefix);
        let next_world = Term::Var(name.clone());
        let guard = Formula::atom(accessibility, vec![world, next_world.clone()])
            .map_err(|e| formula_err(node, e.message()))?;
        let inner = self.expand(
            &body,
            &Translation::AtWorld {
                world: next_world,
                depth: depth + 1,
            },
            active,
        )?;
        Ok(if link == "necessarily" {
            Formula::Forall {
                vars: vec![name],
                body: Box::new(Formula::Implies(Box::new(guard), Box::new(inner))),
            }
        } else {
            Formula::Exists {
                vars: vec![name],
                body: Box::new(Formula::And(vec![guard, inner])),
            }
        })
    }

    fn retain(&mut self, key: MemoKey<D::Id>, result: &ReadResult<Formula>) {
        let context_bytes = match &key.translation {
            Translation::Plain => 0,
            Translation::AtWorld { world, .. } => term_heap_bytes(world),
        };
        let payload_bytes = match result {
            Ok(formula) => formula_heap_bytes(formula),
            Err(error) => error.focus.capacity() + error.detail.capacity(),
        };
        // Account for owned tree/string allocations and both stored keys. The
        // independent entry cap also bounds hash-table and queue bookkeeping.
        let bytes = size_of::<MemoEntry>()
            + 2 * size_of::<MemoKey<D::Id>>()
            + 2 * context_bytes
            + payload_bytes;
        if bytes > self.max_bytes || self.memo.contains_key(&key) {
            return;
        }
        while self.retained_bytes + bytes > self.max_bytes || self.memo.len() >= MAX_MEMO_ENTRIES {
            let oldest = self
                .insertion_order
                .pop_front()
                .expect("a nonempty memo has an insertion record");
            let removed = self
                .memo
                .remove(&oldest)
                .expect("insertion record belongs to the memo");
            self.retained_bytes -= removed.bytes;
        }
        self.insertion_order.push_back(key.clone());
        self.memo.insert(
            key,
            MemoEntry {
                result: result.clone(),
                bytes,
            },
        );
        self.retained_bytes += bytes;
    }
}

/// A selected context modifies this node's translation, never its source graph.
fn explicit_formula_context(
    source: &impl FormulaSource,
    node: &Subject,
) -> ReadResult<Option<Term>> {
    match formula_objects(source, node, "inContext").as_slice() {
        [] => Ok(None),
        [Node::Iri(context)] => Term::iri(context.clone())
            .map(Some)
            .map_err(|error| formula_err(node, error.message())),
        values => Err(formula_err(
            node,
            format!(
                "logic:Formula {} logic:inContext requires one IRI-valued context when selected; found {} values",
                subject_str(node),
                values.len()
            ),
        )),
    }
}

/// Reserve a deterministic namespace disjoint from every authored variable in
/// the selected source graph, including variables in enclosing quantifiers.
/// This streams native indexed terms without materializing a second dataset or
/// storing a source-wide string set. Uncontested sources retain `__w0`, `__w1`, ….
fn fresh_variable_prefixes(source: &impl FormulaSource) -> [String; 3] {
    const ROLES: [&str; 3] = ["w", "tw", "ti"];
    let dataset = source.dataset();
    let mut widths = [2_usize; 3];
    if let Some(predicate) = dataset.term_id_by_value(&TermValue::Iri(logic_iri("termVariable"))) {
        for quad in crate::graphutil::source_graph_pattern(
            dataset,
            None,
            Some(predicate),
            None,
            source.source_graph(),
        ) {
            if let TermRef::Literal { lexical, .. } = dataset.resolve(quad.o) {
                let underscores = lexical.bytes().take_while(|byte| *byte == b'_').count();
                for (role, width) in ROLES.iter().zip(&mut widths) {
                    if let Some(suffix) = lexical[underscores..].strip_prefix(*role)
                        && !suffix.is_empty()
                        && suffix.bytes().all(|byte| byte.is_ascii_digit())
                    {
                        *width = (*width).max(underscores + 1);
                    }
                }
            }
        }
    }
    std::array::from_fn(|index| format!("{}{}", "_".repeat(widths[index]), ROLES[index]))
}

fn term_heap_bytes(term: &Term) -> usize {
    match term {
        Term::Var(name) | Term::Iri(name) | Term::SequenceMarker(name) => name.capacity(),
        Term::Literal(literal) => {
            literal.lexical_form.capacity()
                + literal.datatype.as_ref().map_or(0, String::capacity)
                + literal.language.as_ref().map_or(0, String::capacity)
        }
        Term::App { symbol, args } => {
            symbol.capacity()
                + args.capacity() * size_of::<Term>()
                + args.iter().map(term_heap_bytes).sum::<usize>()
        }
    }
}

fn formula_heap_bytes(formula: &Formula) -> usize {
    match formula {
        Formula::Atom { relation, args } => {
            term_heap_bytes(relation)
                + args.capacity() * size_of::<Term>()
                + args.iter().map(term_heap_bytes).sum::<usize>()
        }
        Formula::Not(body) => size_of::<Formula>() + formula_heap_bytes(body),
        Formula::And(parts) | Formula::Or(parts) => {
            parts.capacity() * size_of::<Formula>()
                + parts.iter().map(formula_heap_bytes).sum::<usize>()
        }
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            2 * size_of::<Formula>() + formula_heap_bytes(left) + formula_heap_bytes(right)
        }
        Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
            vars.capacity() * size_of::<String>()
                + vars.iter().map(String::capacity).sum::<usize>()
                + size_of::<Formula>()
                + formula_heap_bytes(body)
        }
    }
}

pub(super) fn one_child_subject(
    store: &impl FormulaSource,
    node: &Subject,
    link: &str,
) -> ReadResult<Subject> {
    let children = formula_objects(store, node, link);
    if children.len() != 1 {
        return Err(formula_err(
            node,
            format!(
                "logic:Formula {} requires exactly one logic:{link} object; found {}",
                subject_str(node),
                children.len()
            ),
        ));
    }
    term_as_subject(&children[0]).ok_or_else(|| {
        formula_err(
            node,
            format!(
                "logic:Formula {} has a non-resource logic:{link} object",
                subject_str(node)
            ),
        )
    })
}

/// Read the single typed accessibility relation a modal node pins on `logic:overAccessibility`.
///
/// A well-formed modal node pins EXACTLY ONE IRI drawn from the six typed accessibility
/// relations ([`MODAL_ACCESSIBILITY_RELATIONS`]). An absent, plural, non-IRI, or out-of-set
/// value is malformed: in particular the bare `logic:accessibleFrom` superproperty and any
/// `gmeow:modalForce*` register IRI are rejected, because the standard translation must be taken
/// over a single typed relation, never the blurred union. Every rejection routes through
/// [`formula_err`] so the shared `MALFORMED_FORMULA` error path reports it.
fn read_over_accessibility(store: &impl FormulaSource, node: &Subject) -> ReadResult<String> {
    let values = formula_objects(store, node, "overAccessibility");
    if values.len() != 1 {
        return Err(formula_err(
            node,
            format!(
                "modal logic:Formula {} requires exactly one logic:overAccessibility typed accessibility relation; found {}",
                subject_str(node),
                values.len()
            ),
        ));
    }
    let Node::Iri(iri) = &values[0] else {
        return Err(formula_err(
            node,
            format!(
                "modal logic:Formula {} requires an IRI-valued logic:overAccessibility",
                subject_str(node)
            ),
        ));
    };
    if !MODAL_ACCESSIBILITY_RELATIONS.contains(&iri.as_str()) {
        return Err(formula_err(
            node,
            format!(
                "modal logic:Formula {} logic:overAccessibility {} is not one of the six typed accessibility relations; the bare logic:accessibleFrom superproperty, any gmeow:modalForce* register, and any other IRI are rejected",
                subject_str(node),
                iri.as_str()
            ),
        ));
    }
    Ok(iri.as_str().to_owned())
}

/// Read an ordered argument list from `node`'s `logic:<link>` term-carriers (sorted by
/// `logic:termIndex`). Duplicate or gapped ordinals are malformed: RDF order must never become a
/// hidden fallback for the IR's explicit order.
fn parse_term_carriers(
    store: &impl FormulaSource,
    node: &Subject,
    link: &str,
    active: &mut Vec<Subject>,
) -> ReadResult<Vec<Term>> {
    let mut indexed: Vec<(usize, Term)> = Vec::new();
    for carrier_term in formula_objects(store, node, link) {
        let carrier = term_as_subject(&carrier_term).ok_or_else(|| {
            formula_err(
                node,
                format!(
                    "logic:Formula {} has a non-resource logic:{link} carrier",
                    subject_str(node)
                ),
            )
        })?;
        let idx = parse_term_index(store, node, &carrier)?;
        indexed.push((idx, parse_term(store, node, &carrier, active)?));
    }
    indexed.sort_by_key(|(i, _)| *i);
    validate_contiguous_indices(&indexed, node, link)?;
    Ok(indexed.into_iter().map(|(_, t)| t).collect())
}

/// Read a quantifier's ordered bound-variable names from its `logic:quantifiedVariable`
/// term-carriers (sorted by `logic:termIndex`).
///
/// Returns an error if any carrier is malformed (unparsable `termIndex` or missing
/// `termVariable`) or if the binder is vacuous (zero bound variables) — a malformed
/// binder must surface as `MALFORMED_FORMULA`, never silently narrow `∀{x,y}` to `∀{x}`.
fn parse_bound_vars(store: &impl FormulaSource, node: &Subject) -> ReadResult<Vec<String>> {
    let mut indexed: Vec<(usize, String)> = Vec::new();
    for carrier_term in formula_objects(store, node, "quantifiedVariable") {
        let carrier = term_as_subject(&carrier_term).ok_or_else(|| {
            formula_err(
                node,
                format!(
                    "logic:Formula {} has a non-resource logic:quantifiedVariable carrier",
                    subject_str(node)
                ),
            )
        })?;
        let idx = parse_term_index(store, node, &carrier)?;
        // A bound-variable carrier must resolve to a plain variable, so it never opens a
        // function-term recursion; a fresh cycle guard suffices.
        let term = parse_term(store, node, &carrier, &mut Vec::new())?;
        let Term::Var(name) = term else {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {} bound-variable carrier {} must contain exactly one logic:termVariable",
                    subject_str(node),
                    subject_str(&carrier)
                ),
            ));
        };
        indexed.push((idx, name));
    }
    if indexed.is_empty() {
        return Err(formula_err(
            node,
            format!(
                "logic:Formula {} quantifier requires at least one logic:quantifiedVariable",
                subject_str(node)
            ),
        ));
    }
    indexed.sort_by_key(|(i, _)| *i);
    validate_contiguous_indices(&indexed, node, "quantifiedVariable")?;
    Ok(indexed.into_iter().map(|(_, n)| n).collect())
}

/// Reuse the strict typed term reader for compact rule object carriers.
/// No value properties means an ordinary RDF resource; any declared carrier is
/// mandatory, including datatype-only and conflicting-value errors.
pub(super) fn read_atomic_carrier(
    store: &RdfDataset,
    owner: &Subject,
    carrier: &Subject,
) -> gmeow_errors::Result<Option<crate::ir::AtomicTerm>> {
    if [
        "termIri",
        "termVariable",
        "termLiteral",
        "termSequenceMarker",
        "termApplication",
        "termLiteralDatatype",
    ]
    .iter()
    .all(|field| formula_objects(store, carrier, field).is_empty())
    {
        return Ok(None);
    }
    super::admission::admit(store, carrier)?;
    let term = parse_term(store, owner, carrier, &mut Vec::new())
        .map_err(|error| super::formula_err_for(error.focus, error.detail))?;
    use crate::ir::AtomicTerm;
    match term {
        Term::Iri(value) => Ok(Some(AtomicTerm::Iri(value))),
        Term::Var(value) => Ok(Some(AtomicTerm::Var(format!("?{value}")))),
        Term::Literal(value) => Ok(Some(AtomicTerm::Literal(value))),
        Term::SequenceMarker(_) | Term::App { .. } => Err(super::formula_err_for(
            subject_str(owner),
            "a structured or variadic term requires a full formula, not a compact rule atom",
        )),
    }
}

/// Reconstruct a [`Term`] from a term-carrier node by its single term-value property.
fn parse_term(
    store: &impl FormulaSource,
    formula: &Subject,
    carrier: &Subject,
    active: &mut Vec<Subject>,
) -> ReadResult<Term> {
    let fields = [
        "termIri",
        "termVariable",
        "termLiteral",
        "termSequenceMarker",
        "termApplication",
    ];
    let present: Vec<(&str, Vec<Node>)> = fields
        .iter()
        .map(|field| (*field, formula_objects(store, carrier, field)))
        .filter(|(_, values)| !values.is_empty())
        .collect();
    if present.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one term-value property; found {} ({})",
                subject_str(formula),
                subject_str(carrier),
                present.len(),
                present
                    .iter()
                    .map(|(field, _)| format!("logic:{field}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let (field, values) = &present[0];
    if values.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one logic:{field} value; found {}",
                subject_str(formula),
                subject_str(carrier),
                values.len()
            ),
        ));
    }
    let datatype_values = formula_objects(store, carrier, "termLiteralDatatype");
    if *field != "termLiteral" && !datatype_values.is_empty() {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} may carry logic:termLiteralDatatype only with logic:termLiteral",
                subject_str(formula),
                subject_str(carrier)
            ),
        ));
    }

    let value = &values[0];
    match *field {
        "termIri" => {
            let Node::Iri(iri) = value else {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires an IRI-valued logic:termIri",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            };
            Term::iri(iri.clone()).map_err(|e| formula_err(formula, e.message()))
        }
        "termVariable" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termVariable name",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            Term::var(term_str(value)).map_err(|e| formula_err(formula, e.message()))
        }
        "termLiteral" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termLiteral value",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            if datatype_values.len() > 1 {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} permits at most one logic:termLiteralDatatype; found {}",
                        subject_str(formula),
                        subject_str(carrier),
                        datatype_values.len()
                    ),
                ));
            }
            let datatype = match datatype_values.first() {
                Some(Node::Iri(iri)) => Some(iri.clone()),
                Some(_) => {
                    return Err(formula_err(
                        formula,
                        format!(
                            "logic:Formula {} logic:TermCarrier {} requires an IRI-valued logic:termLiteralDatatype",
                            subject_str(formula),
                            subject_str(carrier)
                        ),
                    ));
                }
                None => None,
            };
            let Node::Lit(mut literal) = value.clone() else {
                unreachable!("termLiteral value was checked above")
            };
            if let Some(datatype) = datatype {
                // The separate datatype property is an explicit authoring form, not
                // permission to replace an already typed or language-tagged value.
                if (literal.datatype.is_some() || literal.language.is_some())
                    && literal.datatype_iri() != datatype
                {
                    return Err(formula_err(
                        formula,
                        "logic:termLiteralDatatype disagrees with the native literal",
                    ));
                }
                literal.datatype = Some(datatype);
            }
            Term::rdf_literal(literal).map_err(|e| formula_err(formula, e.message()))
        }
        "termSequenceMarker" => {
            if !term_is_literal(value) {
                return Err(formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a literal logic:termSequenceMarker name",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                ));
            }
            Term::sequence_marker(term_str(value)).map_err(|e| formula_err(formula, e.message()))
        }
        "termApplication" => {
            let function_term = term_as_subject(value).ok_or_else(|| {
                formula_err(
                    formula,
                    format!(
                        "logic:Formula {} logic:TermCarrier {} requires a resource-valued logic:termApplication (a logic:FunctionTerm node)",
                        subject_str(formula),
                        subject_str(carrier)
                    ),
                )
            })?;
            parse_function_term(store, formula, &function_term, active)
        }
        _ => unreachable!("term value property was selected from a closed local array"),
    }
}

/// Reconstruct a [`Term::App`] from the `logic:FunctionTerm` node a `logic:termApplication`
/// carrier points at: its single reified `logic:functionSymbol` (an IRI-named `logic:Type`
/// individual, never a variable — keeping the object level first-order) applied to its ordered
/// `logic:argument` term-carriers. The argument carriers are read with the same
/// [`parse_term_carriers`] machinery the atomic-predication arguments use, so an argument may
/// itself be a `logic:termApplication` and a nested term like `cons(H, cons(1, nil))`
/// round-trips. `active` is the path of function-term nodes currently being expanded: a node
/// reached from its own expansion is a cycle (`cons` whose argument is `cons`) and is rejected
/// rather than recursed into forever.
fn parse_function_term(
    store: &impl FormulaSource,
    formula: &Subject,
    function_term: &Subject,
    active: &mut Vec<Subject>,
) -> ReadResult<Term> {
    let node_id = subject_str(function_term);
    if active.contains(function_term) {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} is cyclic: it appears within its own logic:argument expansion",
                subject_str(formula),
                node_id
            ),
        ));
    }

    let symbols = formula_objects(store, function_term, "functionSymbol");
    if symbols.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} requires exactly one logic:functionSymbol; found {}",
                subject_str(formula),
                node_id,
                symbols.len()
            ),
        ));
    }
    let Node::Iri(symbol) = &symbols[0] else {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:FunctionTerm {} requires an IRI-valued logic:functionSymbol (the reified function symbol, never a variable)",
                subject_str(formula),
                node_id
            ),
        ));
    };

    active.push(function_term.clone());
    let args = parse_term_carriers(store, function_term, "argument", active);
    let popped = active.pop();
    debug_assert_eq!(popped.as_ref(), Some(function_term));
    let args = args?;

    // `Term::app` rejects a nullary application (a 0-ary function symbol is a constant and
    // must be a logic:termIri), so a logic:FunctionTerm with no logic:argument fails here
    // rather than minting a second spelling for a constant.
    Term::app(symbol.clone(), args).map_err(|e| formula_err(formula, e.message()))
}

fn parse_term_index(
    store: &impl FormulaSource,
    formula: &Subject,
    carrier: &Subject,
) -> ReadResult<usize> {
    let values = formula_objects(store, carrier, "termIndex");
    if values.len() != 1 {
        return Err(formula_err(
            formula,
            format!(
                "logic:Formula {} logic:TermCarrier {} requires exactly one logic:termIndex; found {}",
                subject_str(formula),
                subject_str(carrier),
                values.len()
            ),
        ));
    }
    term_str(&values[0]).parse::<usize>().map_err(|_| {
        formula_err(formula, format!(
            "logic:Formula {} logic:TermCarrier {} has an invalid non-negative integer logic:termIndex {:?}",
            subject_str(formula),
            subject_str(carrier),
            term_str(&values[0])
        ))
    })
}

fn validate_contiguous_indices<T>(
    indexed: &[(usize, T)],
    node: &Subject,
    link: &str,
) -> ReadResult<()> {
    for (expected, (actual, _)) in indexed.iter().enumerate() {
        if *actual != expected {
            return Err(formula_err(
                node,
                format!(
                    "logic:Formula {} logic:{link} indices must be unique and contiguous from zero; expected {expected}, found {actual}",
                    subject_str(node)
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
