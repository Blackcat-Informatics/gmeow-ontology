// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed `logic:ResultShape`: a schema-level type contract on a SPARQL
//! `SELECT` result set — the declared variables, each variable's term-kind and
//! (for literals) datatype, the per-variable binding requiredness, and the
//! row-set cardinality — plus the shape a query expects on its input.
//!
//! This module is **pure data** — no I/O, no graph parsing. It is the Rust
//! authority (Principle 17); `slices/grounding/logic/module.ttl` carries the lossy
//! ontology projection of these types, and the harness
//! (`crates/slicetest`) projects an `oxigraph::Term` into the [`ObservedTerm`]
//! surface this module checks.
//!
//! # Two operations
//!
//! 1. [`ResultShape::validate_bindings`] — **type conformance**: every binding in
//!    every row matches its declared column's term-kind / datatype / requiredness,
//!    no row carries an undeclared column, and (in `Count` mode) the row count is
//!    exact. Hard-fail, surfaced ([`ContractViolation`]); never a silent pass.
//! 2. [`ResultShape::is_satisfiable_by`] — **structural input→output
//!    compatibility**, data-free: a producer shape covers every `Required` column a
//!    consumer declares with a compatible term-kind/datatype. This is the
//!    *before-execution* composition check ([`Mismatch`]).
//!
//! The row-set **cardinality** ([`RowCardinality`]) is *recorded* on the shape (it
//! subsumes the test-DSL's `cqExactRows`/`cqExpectRowCount` tiers 1:1); the
//! exact-set / subset comparison against the declared example rows stays the
//! harness's existing concern, sourced from this single field.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ir::LOGIC_NAMESPACE;

// --------------------------------------------------------------------------- //
// Term kind — the closed value class a column's kind ranges over.
//
// Like the `result.rs` field enums, each variant exposes two string surfaces:
//   * `wire()`       — the canonical hyphenated-lowercase value (JSON/text).
//   * `local_name()` — the `module.ttl` named-individual local name (PascalCase),
//                      tied 1:1 by the Rust↔TTL cross-check.
// --------------------------------------------------------------------------- //

/// The RDF term kind of a result-set value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TermKind {
    /// An IRI (a named node).
    Iri,
    /// A literal (always datatyped; a column may further pin which datatype).
    Literal,
    /// A blank node.
    BlankNode,
    /// An RDF 1.2 triple term.
    TripleTerm,
}

impl TermKind {
    /// The canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Iri => "iri",
            Self::Literal => "literal",
            Self::BlankNode => "blank-node",
            Self::TripleTerm => "triple-term",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Iri => "TermKindIri",
            Self::Literal => "TermKindLiteral",
            Self::BlankNode => "TermKindBlankNode",
            Self::TripleTerm => "TermKindTripleTerm",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "iri" => Self::Iri,
            "literal" => Self::Literal,
            "blank-node" => Self::BlankNode,
            "triple-term" => Self::TripleTerm,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "TermKindIri" => Self::Iri,
            "TermKindLiteral" => Self::Literal,
            "TermKindBlankNode" => Self::BlankNode,
            "TermKindTripleTerm" => Self::TripleTerm,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[Self::Iri, Self::Literal, Self::BlankNode, Self::TripleTerm];
}

impl fmt::Display for TermKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// Whether a column's variable is bound in every row (`Required`) or may be
/// unbound (`Optional`, e.g. projected from a SPARQL `OPTIONAL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnBinding {
    /// The variable is bound in every result row.
    Required,
    /// The variable may be unbound in some rows.
    Optional,
}

impl ColumnBinding {
    /// The canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Required => "BindingRequired",
            Self::Optional => "BindingOptional",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "required" => Self::Required,
            "optional" => Self::Optional,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "BindingRequired" => Self::Required,
            "BindingOptional" => Self::Optional,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[Self::Required, Self::Optional];
}

impl fmt::Display for ColumnBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// The row-set cardinality contract — subsumes the test-DSL's three tiers 1:1:
/// `Exact` ↔ `cqExactRows true`, `Contains` ↔ `cqExactRows false`/absent,
/// `Count(n)` ↔ `cqExpectRowCount n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowCardinality {
    /// The declared example rows are the complete, exact result set.
    Exact,
    /// The declared example rows must all appear; extra rows are permitted.
    Contains,
    /// Only the row count is pinned (no per-row content contract).
    Count(u64),
}

impl RowCardinality {
    /// The canonical wire value (the `Count` payload is carried separately).
    pub fn wire(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Contains => "contains",
            Self::Count(_) => "count",
        }
    }
    /// The `module.ttl` named-individual local name (mode only).
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Exact => "RowsExact",
            Self::Contains => "RowsContains",
            Self::Count(_) => "RowsCount",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// The pinned row count, when this is `Count` mode.
    pub fn count(self) -> Option<u64> {
        match self {
            Self::Count(n) => Some(n),
            _ => None,
        }
    }
    /// Every mode, for the Rust↔TTL cross-check (the `Count` payload is irrelevant
    /// to the name check, so a representative `Count(0)` stands in).
    pub const ALL: &'static [Self] = &[Self::Exact, Self::Contains, Self::Count(0)];
}

impl fmt::Display for RowCardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count(n) => write!(f, "count({n})"),
            other => f.write_str(other.wire()),
        }
    }
}

// --------------------------------------------------------------------------- //
// Column + shape.
// --------------------------------------------------------------------------- //

/// The declared type of one result column. Term-kind is **mandatory** — an
/// untyped column is exactly the bag this contract exists to eliminate. A
/// datatype is meaningful only for a literal column; `None` there means "any
/// literal" (a *declared* loosening, matching the test-DSL's bare
/// `cellValueLiteral`), never a half-typed column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    /// The column binds IRIs.
    Iri,
    /// The column binds blank nodes.
    BlankNode,
    /// The column binds literals, optionally pinned to one datatype IRI.
    Literal {
        /// The required datatype IRI, or `None` for "any literal".
        datatype: Option<String>,
    },
    /// The column binds RDF 1.2 triple terms.
    TripleTerm,
}

impl ColumnKind {
    /// The term kind this column declares.
    pub fn term_kind(&self) -> TermKind {
        match self {
            Self::Iri => TermKind::Iri,
            Self::BlankNode => TermKind::BlankNode,
            Self::Literal { .. } => TermKind::Literal,
            Self::TripleTerm => TermKind::TripleTerm,
        }
    }
}

/// One typed column of a [`ResultShape`] — a declared `SELECT` variable, its
/// kind, and whether it is bound in every row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultColumn {
    /// The bare SPARQL variable name (no leading `?`).
    pub var: String,
    /// The declared term-kind (+ optional datatype) of the column's values.
    pub kind: ColumnKind,
    /// Whether the variable is bound in every row.
    pub binding: ColumnBinding,
}

impl ResultColumn {
    /// A `Required` column of the given variable and kind.
    pub fn required(var: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            var: var.into(),
            kind,
            binding: ColumnBinding::Required,
        }
    }
}

/// The typed `logic:ResultShape` — a schema-level type contract on a result set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultShape {
    /// The declared columns, canonicalised sorted-by-`var`.
    pub columns: Vec<ResultColumn>,
    /// The row-set cardinality contract.
    pub cardinality: RowCardinality,
}

/// One observed result-set value, the harness's pure-data projection of an
/// `oxigraph::Term`. A literal always carries its datatype IRI (plain literals
/// are `xsd:string`, language-tagged literals are `rdf:langString`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedTerm {
    /// An IRI value.
    Iri,
    /// A blank-node value.
    BlankNode,
    /// A literal value with its datatype IRI.
    Literal {
        /// The literal's datatype IRI.
        datatype: String,
    },
    /// An RDF 1.2 triple-term value.
    TripleTerm,
}

impl ObservedTerm {
    fn term_kind(&self) -> TermKind {
        match self {
            Self::Iri => TermKind::Iri,
            Self::BlankNode => TermKind::BlankNode,
            Self::Literal { .. } => TermKind::Literal,
            Self::TripleTerm => TermKind::TripleTerm,
        }
    }
}

/// One observed binding `(var, term)` within a result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedBinding {
    /// The bare SPARQL variable name (no leading `?`).
    pub var: String,
    /// The observed value.
    pub term: ObservedTerm,
}

impl ObservedBinding {
    /// Construct a binding.
    pub fn new(var: impl Into<String>, term: ObservedTerm) -> Self {
        Self {
            var: var.into(),
            term,
        }
    }
}

/// A type-contract violation surfaced by [`ResultShape::validate_bindings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractViolation {
    /// A `Required` column was unbound in a row.
    MissingRequired {
        /// The unbound variable.
        var: String,
    },
    /// A row carried a binding for a variable no column declares.
    UndeclaredColumn {
        /// The undeclared variable.
        var: String,
    },
    /// A binding's term-kind did not match its column's declared kind.
    TermKindMismatch {
        /// The variable.
        var: String,
        /// The declared kind.
        expected: TermKind,
        /// The observed kind.
        found: TermKind,
    },
    /// A literal binding's datatype did not match the column's pinned datatype.
    DatatypeMismatch {
        /// The variable.
        var: String,
        /// The declared datatype IRI.
        expected: String,
        /// The observed datatype IRI.
        found: String,
    },
    /// The result row count did not equal the `Count`-mode contract.
    RowCount {
        /// The pinned count.
        expected: u64,
        /// The observed count.
        found: usize,
    },
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired { var } => {
                write!(
                    f,
                    "result-shape violation: required column ?{var} is unbound"
                )
            }
            Self::UndeclaredColumn { var } => write!(
                f,
                "result-shape violation: row binds ?{var}, which the shape does not declare"
            ),
            Self::TermKindMismatch {
                var,
                expected,
                found,
            } => write!(
                f,
                "result-shape violation: ?{var} declared term-kind {expected} but bound a {found}"
            ),
            Self::DatatypeMismatch {
                var,
                expected,
                found,
            } => write!(
                f,
                "result-shape violation: ?{var} declared datatype <{expected}> but bound <{found}>"
            ),
            Self::RowCount { expected, found } => write!(
                f,
                "result-shape violation: shape pins {expected} rows but the result has {found}"
            ),
        }
    }
}

impl std::error::Error for ContractViolation {}

/// A structural incompatibility surfaced by [`ResultShape::is_satisfiable_by`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// The producer has no column for a variable the consumer requires.
    MissingColumn {
        /// The required variable.
        var: String,
    },
    /// The producer's column kind is incompatible with the consumer's.
    IncompatibleKind {
        /// The variable.
        var: String,
        /// The kind the consumer requires.
        required: TermKind,
        /// The kind the producer provides.
        provided: TermKind,
    },
    /// The producer's literal datatype is incompatible with the consumer's pin.
    IncompatibleDatatype {
        /// The variable.
        var: String,
        /// The datatype the consumer requires.
        required: String,
        /// The datatype the producer provides (`None` = unpinned/any literal).
        provided: Option<String>,
    },
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingColumn { var } => write!(
                f,
                "input-shape mismatch: required column ?{var} is not provided by the producer"
            ),
            Self::IncompatibleKind {
                var,
                required,
                provided,
            } => write!(
                f,
                "input-shape mismatch: ?{var} requires {required} but the producer provides {provided}"
            ),
            Self::IncompatibleDatatype {
                var,
                required,
                provided,
            } => write!(
                f,
                "input-shape mismatch: ?{var} requires datatype <{required}> but the producer provides {}",
                provided
                    .as_deref()
                    .map(|d| format!("<{d}>"))
                    .unwrap_or_else(|| "an unpinned literal".to_owned())
            ),
        }
    }
}

impl std::error::Error for Mismatch {}

impl ResultShape {
    /// Build a shape from its columns (canonicalising column order by `var`) and
    /// cardinality.
    pub fn new(mut columns: Vec<ResultColumn>, cardinality: RowCardinality) -> Self {
        columns.sort_by(|a, b| a.var.cmp(&b.var));
        Self {
            columns,
            cardinality,
        }
    }

    /// Look up a declared column by variable name. Columns are kept sorted by
    /// `var` (see [`Self::new`]), so this is a binary search.
    pub fn column(&self, var: &str) -> Option<&ResultColumn> {
        self.columns
            .binary_search_by(|c| c.var.as_str().cmp(var))
            .ok()
            .map(|i| &self.columns[i])
    }

    /// **Type conformance.** Check that every binding in every row matches its
    /// declared column (term-kind, datatype, requiredness), no row carries an
    /// undeclared column, and — in `Count` mode — the row count is exact.
    ///
    /// Row-set *content* comparison (exact-set vs subset) against declared example
    /// rows is the harness's concern, sourced from [`Self::cardinality`]; this
    /// method validates *types*, the contract the schema adds over the example.
    ///
    /// # Errors
    /// Returns the first [`ContractViolation`] found (hard-fail, surfaced).
    pub fn validate_bindings(
        &self,
        rows: &[Vec<ObservedBinding>],
    ) -> Result<(), ContractViolation> {
        if let RowCardinality::Count(expected) = self.cardinality
            && rows.len() as u64 != expected
        {
            return Err(ContractViolation::RowCount {
                expected,
                found: rows.len(),
            });
        }
        let declared: BTreeMap<&str, &ResultColumn> =
            self.columns.iter().map(|c| (c.var.as_str(), c)).collect();
        for row in rows {
            for binding in row {
                if !declared.contains_key(binding.var.as_str()) {
                    return Err(ContractViolation::UndeclaredColumn {
                        var: binding.var.clone(),
                    });
                }
            }
            for col in &self.columns {
                match row.iter().find(|b| b.var == col.var) {
                    None => {
                        if col.binding == ColumnBinding::Required {
                            return Err(ContractViolation::MissingRequired {
                                var: col.var.clone(),
                            });
                        }
                    }
                    Some(binding) => check_term(col, &binding.term)?,
                }
            }
        }
        Ok(())
    }

    /// **Structural input→output compatibility**, data-free. `self` is the
    /// *declared input* a query expects; `producer` is the shape that will feed it
    /// (e.g. the fixture's actual shape, or an upstream query's output shape).
    /// Compatible iff the producer covers every `Required` column of `self` with a
    /// matching term-kind and (where `self` pins one) datatype.
    ///
    /// # Errors
    /// Returns the first [`Mismatch`] found.
    pub fn is_satisfiable_by(&self, producer: &ResultShape) -> Result<(), Mismatch> {
        for col in &self.columns {
            match producer.column(&col.var) {
                None => {
                    if col.binding == ColumnBinding::Required {
                        return Err(Mismatch::MissingColumn {
                            var: col.var.clone(),
                        });
                    }
                }
                Some(prov) => {
                    let (req_kind, prov_kind) = (col.kind.term_kind(), prov.kind.term_kind());
                    if req_kind != prov_kind {
                        return Err(Mismatch::IncompatibleKind {
                            var: col.var.clone(),
                            required: req_kind,
                            provided: prov_kind,
                        });
                    }
                    if let ColumnKind::Literal {
                        datatype: Some(req),
                    } = &col.kind
                    {
                        let provided = match &prov.kind {
                            ColumnKind::Literal { datatype } => datatype.clone(),
                            _ => None,
                        };
                        if provided.as_deref() != Some(req.as_str()) {
                            return Err(Mismatch::IncompatibleDatatype {
                                var: col.var.clone(),
                                required: req.clone(),
                                provided,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Infer a shape from observed rows — the harness's "actual shape" of a fixture
    /// or result set. A variable bound in every row is `Required`, else `Optional`;
    /// each column's kind is taken from the variable's first observation; the
    /// cardinality is `Contains` (observed rows are a witnessed subset).
    pub fn from_observed(rows: &[Vec<ObservedBinding>]) -> Self {
        let mut vars: BTreeSet<&str> = BTreeSet::new();
        for row in rows {
            for binding in row {
                vars.insert(binding.var.as_str());
            }
        }
        let mut columns = Vec::with_capacity(vars.len());
        for var in vars {
            let occurrences: Vec<&ObservedTerm> = rows
                .iter()
                .filter_map(|row| row.iter().find(|b| b.var == var).map(|b| &b.term))
                .collect();
            let kind = match occurrences.first() {
                Some(ObservedTerm::Iri) => ColumnKind::Iri,
                Some(ObservedTerm::BlankNode) => ColumnKind::BlankNode,
                Some(ObservedTerm::Literal { datatype }) => ColumnKind::Literal {
                    datatype: Some(datatype.clone()),
                },
                Some(ObservedTerm::TripleTerm) => ColumnKind::TripleTerm,
                // A variable seen in `vars` always has at least one occurrence.
                None => ColumnKind::Literal { datatype: None },
            };
            let binding = if !rows.is_empty() && occurrences.len() == rows.len() {
                ColumnBinding::Required
            } else {
                ColumnBinding::Optional
            };
            columns.push(ResultColumn {
                var: var.to_owned(),
                kind,
                binding,
            });
        }
        // `vars` is a BTreeSet, so `columns` is already sorted-by-var.
        Self {
            columns,
            cardinality: RowCardinality::Contains,
        }
    }
}

/// Check one observed term against a declared column's kind.
fn check_term(col: &ResultColumn, term: &ObservedTerm) -> Result<(), ContractViolation> {
    let expected = col.kind.term_kind();
    let found = term.term_kind();
    if expected != found {
        return Err(ContractViolation::TermKindMismatch {
            var: col.var.clone(),
            expected,
            found,
        });
    }
    if let (
        ColumnKind::Literal {
            datatype: Some(want),
        },
        ObservedTerm::Literal { datatype: got },
    ) = (&col.kind, term)
        && want != got
    {
        return Err(ContractViolation::DatatypeMismatch {
            var: col.var.clone(),
            expected: want.clone(),
            found: got.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
