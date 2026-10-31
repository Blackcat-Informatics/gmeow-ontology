// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict graph-free lowering of canonical rules and caller-supplied facts.
//!
//! This produces native PurRDF clauses, not the separate Datalog text projection.
//! Inputs describe one caller-selected graph-free interpretation: no named worlds,
//! contextual annotations or implicit ontology closure are flattened into it. Negation
//! is stratified negation-as-failure over the model the caller chooses to evaluate.
//! Literal support requires a shared clause/fact rendering contract and is refused here.
//! The richer structured-term backward projection remains a separate supported target.

use std::collections::BTreeSet;
use std::fmt;

use purrdf::datalog::clause::{ClauseAtom, ClauseTerm, DlClause};
use purrdf::datalog::guard::Guard;
use purrdf::datalog::store::{Fact, ShapeFact};

use crate::ir::{AtomicTerm, ContextualScope, LogicAxiom, LogicRule, NodeKind, TermKindPredicate};
use crate::relational_core::NumericOperator;

/// Complete native output for the selected graph-free fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredDatalog {
    /// Rules in input order, preserving body signs and native inequality guards.
    pub clauses: Vec<DlClause<3>>,
    /// Ground caller facts, using the native clause term's lexical rendering.
    pub facts: Vec<ShapeFact<3>>,
}

/// The source input responsible for a refusal; all indices are zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatalogInput {
    /// A program-wide property, such as stratification.
    Program,
    /// A rule's own metadata.
    Rule(usize),
    /// A rule head.
    Head(usize),
    /// One rule body atom.
    Body { rule: usize, atom: usize },
    /// One rule inequality pair.
    Guard { rule: usize, guard: usize },
    /// One caller fact.
    Fact(usize),
}

/// Position within a source atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatalogSlot {
    /// The subject resource.
    Subject,
    /// The predicate IRI.
    Predicate,
    /// The typed object.
    Object,
}

/// Why the requested source cannot enter this exact fragment.
#[derive(Debug)]
pub enum DatalogRefusal {
    /// Context cannot be erased by a graph-free converter.
    ContextualScope,
    /// A source kind requires another interpretation.
    NodeKind(NodeKind),
    /// Load-bearing annotations cannot become ordinary relational facts.
    LoadBearing,
    /// Reductions are not plain Horn clauses.
    Aggregation,
    /// The native Datalog head is positive.
    NegativeHead,
    /// Caller facts must be positive assertions.
    NegativeFact,
    /// No admitted common literal rendering exists in this profile.
    Literal(DatalogSlot),
    /// Blank identity and existential semantics are outside this profile.
    BlankNode { slot: DatalogSlot, value: String },
    /// A constant must be a well-formed absolute IRI, retained without normalization.
    InvalidIri {
        slot: DatalogSlot,
        value: String,
        source: purrdf::iri::IriError,
    },
    /// Source variables require a nonempty name after their question-mark sigil.
    InvalidVariable(String),
    /// Canonical compact rules do not admit a variable predicate.
    VariablePredicate(String),
    /// A caller fact cannot bind variables.
    NonGroundFact(DatalogSlot),
    /// Head, negative-atom and guard variables must be bound by positive body atoms.
    UnsafeVariable(String),
    /// An interpreted relation must not be mistaken for an ordinary stored predicate.
    InterpretedPredicate(String),
    /// Native predicate dependencies contain a negative cycle.
    UnstratifiedNegation,
}

/// An actionable typed refusal with its source coordinate.
#[derive(Debug)]
pub struct DatalogLoweringError {
    /// Input that requires correction or a different explicitly supported projection.
    pub input: DatalogInput,
    /// Exact unsupported capability or malformed value.
    pub reason: DatalogRefusal,
}

impl fmt::Display for DatalogLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native Datalog lowering at {:?}: ", self.input)?;
        match &self.reason {
            DatalogRefusal::ContextualScope => {
                f.write_str("contextual scope requires an interpretation that preserves it")
            }
            DatalogRefusal::NodeKind(kind) => write!(f, "unsupported source kind {kind:?}"),
            DatalogRefusal::LoadBearing => {
                f.write_str("load-bearing annotation semantics are not ordinary relational data")
            }
            DatalogRefusal::Aggregation => {
                f.write_str("aggregation requires a reduction-capable target")
            }
            DatalogRefusal::NegativeHead => {
                f.write_str("a negative conclusion is not a positive head")
            }
            DatalogRefusal::NegativeFact => f.write_str("caller facts must be positive"),
            DatalogRefusal::Literal(slot) => {
                write!(f, "literal in {slot:?} requires a shared clause/fact codec")
            }
            DatalogRefusal::BlankNode { slot, value } => {
                write!(
                    f,
                    "blank node {value:?} in {slot:?} requires explicit identity or existential support"
                )
            }
            DatalogRefusal::InvalidIri {
                slot,
                value,
                source,
            } => {
                write!(f, "invalid absolute IRI {value:?} in {slot:?}: {source}")
            }
            DatalogRefusal::InvalidVariable(value) => write!(f, "invalid variable {value:?}"),
            DatalogRefusal::VariablePredicate(value) => {
                write!(f, "variable predicate {value:?} is outside compact Rule IR")
            }
            DatalogRefusal::NonGroundFact(slot) => {
                write!(f, "fact term in {slot:?} must be ground")
            }
            DatalogRefusal::UnsafeVariable(variable) => {
                write!(f, "variable {variable:?} needs a positive body binding")
            }
            DatalogRefusal::InterpretedPredicate(predicate) => write!(
                f,
                "interpreted predicate {predicate:?} requires its native operator lowering"
            ),
            DatalogRefusal::UnstratifiedNegation => {
                f.write_str("negative predicate dependencies are not stratified")
            }
        }
    }
}

impl std::error::Error for DatalogLoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.reason {
            DatalogRefusal::InvalidIri { source, .. } => Some(source),
            _ => None,
        }
    }
}

type Result<T> = std::result::Result<T, DatalogLoweringError>;

fn refusal(input: DatalogInput, reason: DatalogRefusal) -> DatalogLoweringError {
    DatalogLoweringError { input, reason }
}

/// Lower canonical Rule IR and the caller's own facts into native graph-free Datalog.
///
/// Variables retain their authored names, constants retain exact IRI identity, negative
/// body atoms retain their sign, and distinct pairs become native not-equal guards.
/// Every head, negative-body and guard variable needs a positive body binding. Ground
/// empty-body rules are legal. Output preserves input order and is returned atomically;
/// a refusal never returns a partially converted program.
///
/// The input slices explicitly select the graph-free facts and rules. They do not
/// certify source completeness, add ontology facts, or authorize merging named worlds.
/// The caller owns native evaluation, resource limits and the chosen model's scope.
///
/// # Errors
/// Refuses nondefault context on rules or any atom, nondefault node kinds, load-bearing
/// annotations, reductions, negative heads/facts, nonground facts, unsafe variables,
/// malformed resources, blank nodes, literals, interpreted operators and unstratified
/// negation. No source is silently dropped or approximated.
pub fn lower_rules_and_facts(rules: &[LogicRule], facts: &[LogicAxiom]) -> Result<LoweredDatalog> {
    let clauses = rules
        .iter()
        .enumerate()
        .map(|(index, rule)| lower_rule(rule, index))
        .collect::<Result<Vec<_>>>()?;
    let facts = facts
        .iter()
        .enumerate()
        .map(|(index, fact)| lower_fact(fact, index))
        .collect::<Result<Vec<_>>>()?;
    if purrdf::datalog::plan::stratify(&clauses).is_none() {
        return Err(refusal(
            DatalogInput::Program,
            DatalogRefusal::UnstratifiedNegation,
        ));
    }
    Ok(LoweredDatalog { clauses, facts })
}

fn admit_scope(scope: &ContextualScope, input: DatalogInput) -> Result<()> {
    if scope != &ContextualScope::default() {
        return Err(refusal(input, DatalogRefusal::ContextualScope));
    }
    Ok(())
}

fn admit_atom(atom: &LogicAxiom, input: DatalogInput) -> Result<()> {
    admit_scope(&atom.scope, input)?;
    if atom.node_kind != NodeKind::ObjectLevelFormula {
        return Err(refusal(input, DatalogRefusal::NodeKind(atom.node_kind)));
    }
    if atom.load_bearing {
        return Err(refusal(input, DatalogRefusal::LoadBearing));
    }
    if is_interpreted(&atom.predicate) {
        return Err(refusal(
            input,
            DatalogRefusal::InterpretedPredicate(atom.predicate.clone()),
        ));
    }
    Ok(())
}

/// Relations requiring interpreted execution rather than stored-tuple matching.
pub(crate) fn is_interpreted(predicate: &str) -> bool {
    NumericOperator::from_iri(predicate).is_some()
        || TermKindPredicate::from_relation(predicate).is_some()
        || predicate
            .strip_prefix(crate::ir::LOGIC_NAMESPACE)
            .is_some_and(|local| {
                matches!(
                    local,
                    "termEqual"
                        | "termSameComponent"
                        | "termDistinct"
                        | "termLess"
                        | "termLessEqual"
                        | "termGreater"
                        | "termGreaterEqual"
                        | "termIn"
                        | "termStrStarts"
                        | "termRegex"
                        | "termLangMatches"
                        | "termSum"
                        | "linkVia"
                        | "directType"
                        | "sparqlTarget"
                        | "transitiveReach"
                        | "contextualApplication"
                )
            })
}

fn variable(value: &str, input: DatalogInput) -> Result<ClauseTerm> {
    if !value.starts_with('?')
        || value.len() == 1
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(refusal(
            input,
            DatalogRefusal::InvalidVariable(value.to_owned()),
        ));
    }
    Ok(ClauseTerm::var(value))
}

fn iri(value: &str, slot: DatalogSlot, input: DatalogInput) -> Result<ClauseTerm> {
    let source = match purrdf::iri::is_absolute(value) {
        Ok(true) => return Ok(ClauseTerm::iri(value)),
        Ok(false) => purrdf::iri::IriError::MissingScheme,
        Err(source) => source,
    };
    Err(refusal(
        input,
        DatalogRefusal::InvalidIri {
            slot,
            value: value.to_owned(),
            source,
        },
    ))
}

fn term(term: &AtomicTerm, slot: DatalogSlot, input: DatalogInput) -> Result<ClauseTerm> {
    match term {
        AtomicTerm::Var(value) => variable(value, input),
        AtomicTerm::Iri(value) => iri(value, slot, input),
        AtomicTerm::Blank(value) => Err(refusal(
            input,
            DatalogRefusal::BlankNode {
                slot,
                value: value.clone(),
            },
        )),
        AtomicTerm::Literal(_) => Err(refusal(input, DatalogRefusal::Literal(slot))),
    }
}

fn lower_atom(atom: &LogicAxiom, input: DatalogInput) -> Result<ClauseAtom<3>> {
    admit_atom(atom, input)?;
    if atom.predicate.starts_with('?') {
        return Err(refusal(
            input,
            DatalogRefusal::VariablePredicate(atom.predicate.clone()),
        ));
    }
    let subject = term(
        &AtomicTerm::resource(atom.subject.clone()),
        DatalogSlot::Subject,
        input,
    )?;
    let predicate = iri(&atom.predicate, DatalogSlot::Predicate, input)?;
    let object = term(&atom.obj, DatalogSlot::Object, input)?;
    let lowered = ClauseAtom::triple(subject, predicate, object);
    Ok(if atom.negated {
        lowered.into_negated()
    } else {
        lowered
    })
}

fn require_bound(
    atom: &ClauseAtom<3>,
    bound: &BTreeSet<String>,
    input: DatalogInput,
) -> Result<()> {
    for term in atom.terms() {
        if let ClauseTerm::Var(name) = term
            && !bound.contains(name)
        {
            return Err(refusal(input, DatalogRefusal::UnsafeVariable(name.clone())));
        }
    }
    Ok(())
}

fn lower_rule(rule: &LogicRule, index: usize) -> Result<DlClause<3>> {
    let input = DatalogInput::Rule(index);
    admit_scope(&rule.scope, input)?;
    if rule.node_kind != NodeKind::DerivationRule {
        return Err(refusal(input, DatalogRefusal::NodeKind(rule.node_kind)));
    }
    if rule.aggregation.is_some() {
        return Err(refusal(input, DatalogRefusal::Aggregation));
    }
    if rule.head.negated {
        return Err(refusal(
            DatalogInput::Head(index),
            DatalogRefusal::NegativeHead,
        ));
    }
    let head = lower_atom(&rule.head, DatalogInput::Head(index))?;
    let body = rule
        .body
        .iter()
        .enumerate()
        .map(|(atom, value)| lower_atom(value, DatalogInput::Body { rule: index, atom }))
        .collect::<Result<Vec<_>>>()?;
    let clause = DlClause::datalog(head, body);
    let bound = clause.bound_variables();
    for head in clause.head_atoms() {
        require_bound(head, &bound, DatalogInput::Head(index))?;
    }
    for (atom, value) in clause
        .body()
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_negated())
    {
        require_bound(value, &bound, DatalogInput::Body { rule: index, atom })?;
    }
    let mut guards = Vec::with_capacity(rule.distinct_pairs.len());
    for (guard, (left, right)) in rule.distinct_pairs.iter().enumerate() {
        let input = DatalogInput::Guard { rule: index, guard };
        for name in [left, right] {
            variable(name, input)?;
            if !bound.contains(name) {
                return Err(refusal(input, DatalogRefusal::UnsafeVariable(name.clone())));
            }
        }
        guards.push(Guard::not_equal(left.as_str(), right.as_str()));
    }
    Ok(clause.with_guards(guards))
}

fn fact_surface(term: &ClauseTerm, slot: DatalogSlot, input: DatalogInput) -> Result<String> {
    term.surface()
        .ok_or_else(|| refusal(input, DatalogRefusal::NonGroundFact(slot)))
}

fn lower_fact(fact: &LogicAxiom, index: usize) -> Result<ShapeFact<3>> {
    let input = DatalogInput::Fact(index);
    if fact.negated {
        return Err(refusal(input, DatalogRefusal::NegativeFact));
    }
    let atom = lower_atom(fact, input)?;
    Ok(Fact {
        subject: fact_surface(atom.subject(), DatalogSlot::Subject, input)?,
        predicate: fact_surface(atom.predicate(), DatalogSlot::Predicate, input)?,
        object: fact_surface(atom.object(), DatalogSlot::Object, input)?,
        graph: (),
    })
}
