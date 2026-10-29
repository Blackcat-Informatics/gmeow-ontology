// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit exact-decimal interpretation, lowered into the shared native rules.
//! Symbolic application lowering remains uninterpreted outside this profile.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::{
    ir::{Formula, LogicProgram, Term},
    relational_core::NumericOperator,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfLiteral, TermRef};
use sha2::{Digest, Sha256};

use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};

const MATH: &str = "https://blackcatinformatics.ca/math/";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// Stable selected semantics: finite exact scalar arithmetic, with no rounding.
pub const PROFILE: &str = "math-exact-decimal-v1";

type Source = BTreeMap<(String, String), Vec<Term>>;

fn iri(local: &str) -> String {
    format!("{MATH}{local}")
}
fn atom(predicate: &str, left: Term, right: Term) -> Formula {
    Formula::Atom {
        relation: Term::Iri(predicate.into()),
        args: vec![left, right],
    }
}
fn named(value: &str) -> Term {
    Term::Iri(value.into())
}

fn objects<'a>(source: &'a Source, subject: &str, predicate: &str) -> &'a [Term] {
    source
        .get(&(subject.into(), predicate.into()))
        .map_or(&[], Vec::as_slice)
}
fn one(source: &Source, subject: &str, predicate: &str) -> gmeow_errors::Result<Term> {
    match objects(source, subject, predicate) {
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
fn reference(source: &Source, subject: &str, predicate: &str) -> gmeow_errors::Result<String> {
    match one(source, subject, predicate)? {
        Term::Iri(value) => Ok(value),
        _ => Err(crate::error::profile_result(
            PROFILE,
            format!("{subject} requires an IRI for {predicate}"),
        )),
    }
}
fn require_type(source: &Source, subject: &str, class: &str) -> gmeow_errors::Result<()> {
    if !objects(source, subject, TYPE).contains(&named(class)) {
        return Err(crate::error::profile_result(
            PROFILE,
            format!("{subject} must explicitly have type {class}"),
        ));
    }
    Ok(())
}

fn read_source(dataset: &RdfDataset) -> gmeow_errors::Result<Source> {
    if dataset
        .quads_for_pattern(None, None, None, GraphMatch::Any)
        .any(|quad| quad.g.is_some())
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "this profile accepts only a default-graph source; select/export one world explicitly",
        ));
    }
    let mut source = Source::new();
    for quad in dataset.quads_for_pattern(None, None, None, GraphMatch::Default) {
        let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
            (dataset.resolve(quad.s), dataset.resolve(quad.p))
        else {
            return Err(crate::error::profile_result(
                PROFILE,
                "this profile requires named source subjects and predicates",
            ));
        };
        let value = match dataset.resolve(quad.o) {
            TermRef::Iri(value) => named(value),
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
            _ => {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "this profile requires IRI or RDF literal objects; blank and quoted objects are unsupported",
                ));
            }
        };
        let values = source
            .entry((subject.into(), predicate.into()))
            .or_default();
        if !values.contains(&value) {
            values.push(value);
        }
    }
    for values in source.values_mut() {
        values.sort();
    }
    Ok(source)
}

struct Lowering<'a> {
    source: &'a Source,
    bindings: &'a BTreeMap<String, String>,
    quantity_classes: &'a BTreeSet<String>,
    used: BTreeSet<String>,
    visited: BTreeSet<String>,
    premises: Vec<Formula>,
    calls: Vec<Formula>,
    next: usize,
}
impl Lowering<'_> {
    fn witness(&mut self, subject: &str) {
        for ((s, predicate), values) in self.source.range((subject.to_owned(), String::new())..) {
            if s != subject {
                break;
            }
            for value in values {
                self.premises.push(atom(predicate, named(s), value.clone()));
            }
        }
    }
    fn expression(&mut self, node: &str, depth: usize) -> gmeow_errors::Result<Term> {
        if depth > crate::math_expression::analysis::depth_limit()
            || !self.visited.insert(node.into())
        {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("cyclic or excessively deep expression at {node}"),
            ));
        }
        // The structural owner admits the general grammar. This selected profile
        // must not reinterpret a multiply typed node through a different branch.
        // MathematicalExpression is a harmless shared base, not a concrete form.
        let forms: Vec<_> = [
            "ApplicationExpression",
            "SymbolReference",
            "BindingExpression",
            "VariableExpression",
            "NumberLiteral",
        ]
        .into_iter()
        .filter(|form| objects(self.source, node, TYPE).contains(&named(&iri(form))))
        .collect();
        if forms.len() != 1 {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("{node}: requires one unambiguous concrete AST form; found {forms:?}"),
            ));
        }
        self.witness(node);
        let value = if forms[0] == "SymbolReference" {
            self.symbol(node)?
        } else {
            self.application(node, depth)?
        };
        self.visited.remove(node);
        Ok(value)
    }
    fn symbol(&mut self, node: &str) -> gmeow_errors::Result<Term> {
        let symbol = reference(self.source, node, &iri("hasMathematicalSymbol"))?;
        require_type(self.source, &symbol, &iri("MathematicalSymbol"))?;
        self.witness(&symbol);
        let quantity = self
            .bindings
            .get(&symbol)
            .ok_or_else(|| {
                crate::error::profile_result(PROFILE, format!("missing --bind {symbol}=QUANTITY"))
            })?
            .clone();
        if !objects(self.source, &quantity, TYPE).contains(&named(&iri("Quantity"))) {
            if !self
                .quantity_classes
                .iter()
                .any(|class| objects(self.source, &quantity, TYPE).contains(&named(class)))
            {
                return Err(crate::error::profile_result(
                    PROFILE,
                    format!(
                        "{quantity} requires Quantity type or an explicitly admitted owned subclass"
                    ),
                ));
            }
            // This is a native body obligation, never an asserted input type.
            self.premises
                .push(atom(TYPE, named(&quantity), named(&iri("Quantity"))));
        }
        if reference(self.source, &quantity, &iri("hasDimension"))? != iri("dimensionless") {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "{quantity} is dimensioned; this profile supports only explicitly dimensionless quantities"
                ),
            ));
        }
        let Term::Literal(literal) = one(self.source, &quantity, &iri("quantityValue"))? else {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("{quantity} requires a numeric literal quantityValue"),
            ));
        };
        if literal.language.is_some() || literal.direction.is_some() {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("{quantity}: numeric values cannot carry language or direction"),
            ));
        }
        let value = purrdf::xsd::parse_by_iri(&literal.lexical_form, literal.datatype_iri())
            .map_err(|error| crate::error::profile_result(PROFILE, format!("{quantity}: {error}")))?
            .ok_or_else(|| {
                crate::error::profile_result(
                    PROFILE,
                    format!("{quantity}: unsupported numeric datatype"),
                )
            })?;
        if !value.is_exact_numeric() {
            return Err(crate::error::profile_result(
                PROFILE,
                format!(
                    "{quantity}: only integer and decimal inputs are admitted; float/double are unsupported"
                ),
            ));
        }
        self.used.insert(symbol);
        self.witness(&quantity);
        let value = Term::Var(format!("input{}", self.next));
        self.next += 1;
        self.premises
            .push(atom(&iri("quantityValue"), named(&quantity), value.clone()));
        Ok(value)
    }

    fn application(&mut self, node: &str, depth: usize) -> gmeow_errors::Result<Term> {
        require_type(self.source, node, &iri("ApplicationExpression"))?;
        let operator = reference(self.source, node, &iri("operator"))?;
        let operator = match operator.strip_prefix(MATH) {
            Some("Addition") => NumericOperator::Add,
            Some("Subtraction") => NumericOperator::Subtract,
            Some("Multiplication") => NumericOperator::Multiply,
            Some("Division") => NumericOperator::ExactDivide,
            _ => {
                return Err(crate::error::profile_result(
                    PROFILE,
                    format!(
                        "{node}: unsupported operator {operator}; supported operators are Addition, Subtraction, Multiplication, Division"
                    ),
                ));
            }
        };
        let ordered = self.slots(node)?;
        let left = self.expression(&ordered[&0], depth + 1)?;
        let right = self.expression(&ordered[&1], depth + 1)?;
        let result = Term::Var(format!("value{}", self.next));
        self.next += 1;
        self.calls.push(Formula::Atom {
            relation: named(operator.iri()),
            args: vec![left, right, result.clone()],
        });
        Ok(result)
    }

    fn slots(&mut self, node: &str) -> gmeow_errors::Result<BTreeMap<usize, String>> {
        let slots = objects(self.source, node, &iri("argumentSlot"));
        if slots.len() != 2 {
            return Err(crate::error::profile_result(
                PROFILE,
                format!("{node}: binary arithmetic requires exactly two argument slots"),
            ));
        }
        let mut ordered = BTreeMap::new();
        for slot in slots {
            let Term::Iri(slot) = slot else {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "argument slots must be named IRIs",
                ));
            };
            require_type(self.source, slot, &iri("ArgumentSlot"))?;
            self.witness(slot);
            let Term::Literal(index) = one(self.source, slot, &iri("slotIndex"))? else {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "slotIndex must be integer",
                ));
            };
            if index.language.is_some() || index.direction.is_some() {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "slotIndex cannot carry language or direction",
                ));
            }
            let value = purrdf::xsd::parse_by_iri(&index.lexical_form, index.datatype_iri())
                .map_err(|error| {
                    crate::error::profile_result(PROFILE, format!("invalid slotIndex: {error}"))
                })?;
            let Some(value @ purrdf::xsd::XsdValue::Integer { .. }) = value else {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "slotIndex requires an XSD integer datatype, not a string, decimal or float",
                ));
            };
            let index = value.canonical_lexical().parse::<usize>().map_err(|_| {
                crate::error::profile_result(
                    PROFILE,
                    "slotIndex must be a bounded nonnegative integer",
                )
            })?;
            let expression = reference(self.source, slot, &iri("slotExpression"))?;
            if ordered.insert(index, expression).is_some() {
                return Err(crate::error::profile_result(
                    PROFILE,
                    "duplicate argument slot index",
                ));
            }
        }
        if ordered.keys().copied().collect::<Vec<_>>() != [0, 1] {
            return Err(crate::error::profile_result(
                PROFILE,
                "binary argument slot indexes must be exactly 0 and 1",
            ));
        }
        Ok(ordered)
    }
}

/// A fresh derived quantity and its complete, normally governed native evidence.
pub struct Evaluation {
    /// Content-addressed quantity IRI, never present in the asserted source.
    pub quantity: String,
    /// Native reasoned result; values are derived rather than asserted outputs.
    pub result: crate::result::ReasoningResult,
}

fn validate_quantity_result(
    result: &crate::result::ReasoningResult,
    quantity: &str,
) -> gmeow_errors::Result<()> {
    let rows = result
        .inferred()
        .iter()
        .filter(|row| !row.is_edb && row.subject == quantity);
    let has_quantity_type = rows
        .clone()
        .any(|row| row.predicate == TYPE && row.object == purrdf::TermValue::iri(iri("Quantity")));
    let dimensions: Vec<_> = rows
        .clone()
        .filter(|row| row.predicate == iri("hasDimension"))
        .map(|row| &row.object)
        .collect();
    let values: Vec<_> = rows
        .filter(|row| row.predicate == iri("quantityValue"))
        .map(|row| &row.object)
        .collect();
    if !has_quantity_type
        || dimensions.len() != 1
        || dimensions[0] != &purrdf::TermValue::iri(iri("dimensionless"))
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "native computation must derive Quantity type and exactly one dimensionless dimension; no partial result is published",
        ));
    }
    let [
        purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        },
    ] = values.as_slice()
    else {
        return Err(crate::error::profile_result(
            PROFILE,
            "native computation must derive exactly one untagged numeric quantityValue; no partial result is published",
        ));
    };
    let value = purrdf::xsd::parse_by_iri(lexical_form, datatype).map_err(|error| {
        crate::error::profile_result(PROFILE, format!("invalid derived quantityValue: {error}"))
    })?;
    if !value.is_some_and(|value| value.is_exact_numeric()) {
        return Err(crate::error::profile_result(
            PROFILE,
            "derived quantityValue must be an exact integer or decimal; no partial result is published",
        ));
    }
    // Other native consequences, including inherited types, remain in the
    // governed result and its proof; their presence is not incompleteness.
    Ok(())
}

fn encode_identity(
    root: &str,
    bindings: &BTreeMap<String, String>,
    premises: &[Formula],
    calls: &[Formula],
    context: &EvaluationContext,
) -> gmeow_errors::Result<Vec<u8>> {
    use gmeow_errors::DiagKind;
    use purrdf_lex::json::{Format, Value, record::ToJson};

    let mut fields = vec![
        PROFILE.to_json(),
        root.to_json(),
        bindings.to_json(),
        premises.to_json(),
        calls.to_json(),
    ];
    if !context.identity.is_empty()
        || !context.premises.is_empty()
        || !context.quantity_classes.is_empty()
    {
        fields.extend([
            context.identity.to_json(),
            context.premises.to_json(),
            context.quantity_classes.to_json(),
        ]);
    }
    let mut resident = purrdf_lex::allocation::Resident;
    let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
    purrdf_lex::json::write_with_memory(&Value::Array(fields), Format::COMPACT, &mut memory)
        .map(String::into_bytes)
        .map_err(|error| {
            let kind = crate::error::Result::new(format!("{PROFILE}: {error}"));
            let mut diagnostic =
                gmeow_errors::Diag::new(kind.code(), kind.grade(), kind.to_string())
                    .with_source(error)
                    .with_context(format!("encode {PROFILE} result identity"));
            if let Some(class) = kind.failure_class() {
                diagnostic = diagnostic.with_failure_class(class);
            }
            diagnostic
        })
}

/// Prepare the explicitly selected exact-scalar profile without executing it.
///
/// # Errors
/// Rejects malformed ASTs, binding ambiguity, unsupported dimensions or values,
/// division by zero, and nonterminating decimal quotients. It never rounds.
/// Retains native storage failures when encoding the result identity.
pub fn prepare(
    dataset: &RdfDataset,
    root: &str,
    bindings: &BTreeMap<String, String>,
    context: &EvaluationContext,
) -> gmeow_errors::Result<PreparedEvaluation> {
    crate::math_expression::analysis::structural_key_at(dataset, root)
        .map_err(|error| crate::error::profile_result(PROFILE, error.to_string()))?;
    let source = read_source(dataset)?;
    let mut lowering = Lowering {
        source: &source,
        bindings,
        quantity_classes: &context.quantity_classes,
        used: BTreeSet::new(),
        visited: BTreeSet::new(),
        premises: vec![],
        calls: vec![],
        next: 0,
    };
    let value = lowering.expression(root, 0)?;
    if lowering.used.len() != bindings.len() {
        return Err(crate::error::profile_result(
            PROFILE,
            "unused binding: every selected symbol must occur in the root expression",
        ));
    }
    let identity = encode_identity(root, bindings, &lowering.premises, &lowering.calls, context)?;
    let digest = format!("{:x}", Sha256::digest(identity));
    let quantity = format!("urn:gmeow:math-evaluation:quantity:{digest}");
    if source.keys().any(|(subject, _)| subject == &quantity)
        || source
            .values()
            .flatten()
            .any(|object| object == &named(&quantity))
    {
        return Err(crate::error::profile_result(
            PROFILE,
            "derived result identity already occurs in the source; supply original inputs only",
        ));
    }
    let mut antecedent = lowering.premises;
    antecedent.extend(context.premises.clone());
    antecedent.extend(lowering.calls);
    let head = Formula::And(vec![
        atom(TYPE, named(&quantity), named(&iri("Quantity"))),
        atom(
            &iri("hasDimension"),
            named(&quantity),
            named(&iri("dimensionless")),
        ),
        atom(&iri("quantityValue"), named(&quantity), value),
    ]);
    let program = LogicProgram::new(
        vec![],
        vec![],
        vec![],
        Some(format!("urn:gmeow:{PROFILE}:{digest}")),
    )
    .with_formulas(vec![Formula::Implies(
        Box::new(Formula::And(antecedent)),
        Box::new(head),
    )]);
    Ok(PreparedEvaluation { quantity, program })
}

/// Additional admitted premises bound before the result identity is selected.
#[derive(Default)]
pub struct EvaluationContext {
    /// Selected caller contract and authenticated owner identity.
    pub identity: String,
    /// Positive source premises admitted by that caller.
    pub premises: Vec<Formula>,
    /// Caller-admitted subclasses; native Quantity membership remains a required premise.
    pub quantity_classes: BTreeSet<String>,
}

/// Native arithmetic program for a caller that adds its admitted result roles.
pub struct PreparedEvaluation {
    /// Fresh quantity identity bound to the selected source and context.
    pub quantity: String,
    /// Canonical native program; domain callers may append derived conclusions.
    pub program: LogicProgram,
}

/// Execute a prepared program once through the normal native owner.
///
/// # Errors
/// Returns source admission, arithmetic, proof and result-completeness failures.
pub fn execute_prepared(
    dataset: &RdfDataset,
    prepared: PreparedEvaluation,
) -> gmeow_errors::Result<Evaluation> {
    let input = crate::reason::prepare_reasoning_input(dataset)?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        format!("urn:gmeow:profile:{PROFILE}"),
        *input.ingress_contract(),
    )?])?;
    let result = crate::reason::reason_program(&prepared.program, input, &domains)?;
    result.validate_native_closure()?;
    validate_quantity_result(&result, &prepared.quantity)?;
    Ok(Evaluation {
        quantity: prepared.quantity,
        result,
    })
}

/// Execute the standalone exact scalar profile without domain extensions.
///
/// # Errors
/// Rejects malformed inputs, unsupported selections and non-exact results.
pub fn evaluate(
    dataset: &RdfDataset,
    root: &str,
    bindings: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Evaluation> {
    execute_prepared(
        dataset,
        prepare(dataset, root, bindings, &EvaluationContext::default())?,
    )
}

#[cfg(test)]
mod tests;
