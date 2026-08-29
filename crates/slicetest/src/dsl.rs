// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Load a slice-resident test-DSL spec file and introspect its cells.
//!
//! A `tests/*.ttl` spec is itself ontology data in the `gmeow:` test-DSL
//! vocabulary (`dsl/tests/vocabulary.ttl`). Rather than keep a hand-written
//! deserializer in lockstep with that vocabulary, the harness loads each spec
//! into a native [`RdfDataset`] (the canonical codec, lenient parsing, the same
//! primitive the validation path uses) and SPARQL-introspects the three cell types
//! into typed Rust structs. The nested `ExpectedRow -> rowCell -> ExpectedCell`
//! shape of a SELECT competency question is pulled out declaratively in one join
//! and grouped in Rust.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use gmeow_errors::{Diag, Result};
use gmeow_logic_compile::result_shape::{
    ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality, TermKind,
};
use purrdf::{RdfDataset, TermValue};

use crate::error::{ResultShapeParse, SparqlEval, SpecCell, SpecLoad, TypedBinding};
use crate::native_query::{self, render_term};

/// The GMEOW namespace; the test-DSL terms live directly under it.
pub const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `logic:` namespace; the result-shape terms live directly under it.
pub use gmeow_ns::LOGIC_NS;

/// The three cell collections parsed out of one `tests/*.ttl` spec file.
#[derive(Debug, Clone, Default)]
pub struct SpecFile {
    pub competency: Vec<CompetencyQuestion>,
    pub structural: Vec<StructuralAssertion>,
    pub conformance: Vec<ExampleConformance>,
}

/// A `gmeow:CompetencyQuestion` cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CompetencyQuestion {
    pub iri: String,
    /// Inline `gmeow:cqQuery` (mutually exclusive with `query_file`).
    pub query_inline: Option<String>,
    /// `gmeow:cqQueryFile` — REPO-ROOT-relative path to a `.rq` file.
    pub query_file: Option<String>,
    /// `gmeow:cqProject` — REPO-ROOT-relative path to a CONSTRUCT `.rq` file that
    /// MATERIALIZES a computed projection over the overlaid canon BEFORE the cqQuery
    /// runs. The harness runs the CONSTRUCT and unions its triples into the dataset
    /// the question is answered against, so a projection-agreement question compares
    /// the flat shortcut against a materialized collapse of the canon rather than a
    /// second hand-asserted copy of the same IRI (defeating the circular gate).
    pub project_query_file: Option<String>,
    /// `gmeow:cqExpectAsk` — expected ASK boolean (ASK questions only).
    pub expect_ask: Option<bool>,
    /// `gmeow:cqExpectRowCount` — coarse expected SELECT row count.
    pub expect_row_count: Option<u64>,
    /// `gmeow:cqExactRows` — whether the enumerated rows are the COMPLETE set.
    pub exact_rows: bool,
    /// `gmeow:cqExpectRow` — enumerated expected SELECT rows.
    pub expected_rows: Vec<ExpectedRow>,
    /// `gmeow:cqReasoning` — the entailment lane (defaults to [`ReasoningProfile::None`]).
    pub reasoning: ReasoningProfile,
    /// `gmeow:cqDataFile` — SLICE-relative ABox fixture overlaid onto the asserted
    /// merged graph for this one question (instance-classifier questions only).
    /// Honoured only in the [`ReasoningProfile::None`] lane.
    pub data_file: Option<String>,
    /// `gmeow:cqResultShape` — the typed [`ResultShape`] the SELECT result is
    /// contracted to (the OUTPUT contract). The actual bindings are validated
    /// against it after execution.
    pub result_shape: Option<ResultShape>,
    /// `gmeow:cqInputShape` — the typed [`ResultShape`] the question's input is
    /// expected to satisfy (the INPUT contract), checked before execution against
    /// the upstream producer named by `gmeow:cqConsumes`.
    pub input_shape: Option<ResultShape>,
    /// `gmeow:cqConsumes` — the IRI of the upstream [`CompetencyQuestion`] whose
    /// declared output (`gmeow:cqResultShape`) must satisfy this question's
    /// `gmeow:cqInputShape` before execution (composition pre-check).
    pub consumes: Option<String>,
    pub rationale: Option<String>,
}

/// One enumerated SELECT result row (`gmeow:ExpectedRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedRow {
    /// One cell per projected variable.
    pub cells: Vec<ExpectedCell>,
}

/// One variable-to-value binding within an [`ExpectedRow`] (`gmeow:ExpectedCell`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedCell {
    /// The SPARQL variable name, WITHOUT the leading `?`.
    pub var: String,
    /// The expected bound value — an IRI (`gmeow:cellValueIri`) or a literal
    /// (`gmeow:cellValueLiteral`), kept as a native [`TermValue`] so it compares
    /// directly (via [`render_term`]) against a query solution binding.
    pub value: TermValue,
}

/// A `gmeow:StructuralAssertion` cell.
#[derive(Debug, Clone)]
pub struct StructuralAssertion {
    pub iri: String,
    pub polarity: Polarity,
    /// `gmeow:saPattern` — a SPARQL ASK (mutually exclusive with `shape`).
    pub pattern: Option<String>,
    /// `gmeow:saShape` — an `sh:NodeShape` IRI (mutually exclusive with `pattern`).
    pub shape: Option<String>,
    /// `gmeow:saScope` — defaults to [`Scope::ModuleAndExamples`] when omitted.
    pub scope: Scope,
    /// `gmeow:saFailWitness` — OPTIONAL SLICE-relative fixture whose triples SUPPLY
    /// the banned pattern, so the assertion has demonstrable teeth. When present, the
    /// harness runs the assertion's `pattern` a SECOND time over module ∪ fixture and
    /// requires the polarity to be VIOLATED there (a `mustNot` ban must now HOLD; a
    /// `must` ban must now FAIL). A fixture that does not trip the ban is a hard fail —
    /// it proves the ban is vacuous. This is how a `scopeModule` ban (an ASK over the
    /// slice's own module, which by construction never carries the banned triple) gets
    /// a fail-witness: the fixture injects the violation the real module must never hold.
    pub fail_witness: Option<String>,
    pub rationale: Option<String>,
}

/// A `gmeow:ExampleConformance` cell.
#[derive(Debug, Clone)]
pub struct ExampleConformance {
    pub iri: String,
    /// `gmeow:exampleFile` — SLICE-relative path to the example/counter-example.
    pub file: String,
    pub outcome: Outcome,
    /// `gmeow:expectedViolationCode` — the `shacl.<LocalName>` code (violates only).
    pub violation_code: Option<String>,
    /// `gmeow:expectedSourceShape` — OPTIONAL (violates cells only). The SHACL
    /// `sh:sourceShape` IRI the matching finding must originate from. Every
    /// `logic:Constraint` projects to the SAME generic finding component
    /// (`shacl.SPARQLConstraintComponent`), so the component code alone cannot
    /// prove the SPECIFIC named rule fired. When present, the harness additionally
    /// requires the finding that matched `expectedViolationCode` to carry this
    /// source shape (exact IRI, or a local-name suffix match). Absent → the check
    /// is component-code-only, exactly as before (backward-compatible: a cell that
    /// does not set it is unaffected).
    pub expected_source_shape: Option<String>,
    /// `gmeow:expectedFailureClass` — OPTIONAL (violates cells only). The semantic
    /// `math:<Class>` (or other slice-owned) failure-class IRI the counter-example is
    /// expected to raise, and ISOLATION at the class level: when set, the harness
    /// additionally requires that EVERY finding produced across BOTH channels (native
    /// SHACL violations, resolved to a class through the generated shape's own
    /// `gmeow:enforcesFailureClass` annotation, AND native Rust findings such as
    /// [`gmeow_logic::math_expression::check_math_expression_findings`], resolved by the
    /// `math:<Class>:` message-token convention) names THIS class — never a bare
    /// component-code match that a same-coded but semantically different finding could
    /// also satisfy. This is strictly stronger than `expectedSourceShape` (which pins one
    /// derived shape but does not exclude an UNRELATED extra finding from firing
    /// alongside it) and is the only mechanism that reaches native (non-SHACL) failure
    /// classes at all.
    pub expected_failure_class: Option<String>,

    /// `gmeow:expectedSoleFinding` — OPTIONAL (violates cells only). The
    /// EXHAUSTIVENESS half, which neither of the two fields above can express: they
    /// pin that a particular law DID fire, and the harness's violates branch asks
    /// only that SOME finding match, so a rationale claiming the fixture isolates
    /// one defect is unfalsifiable while this is absent. When `Some(true)`, EVERY
    /// violation-severity result must originate from `expected_source_shape` —
    /// several rows of the SAME law still conform, one finding from another law is a
    /// hard failure. Absent → unchanged behaviour, so no cell that omits it is
    /// affected.
    ///
    /// `expected_source_shape` is REQUIRED whenever this is `Some(true)`: soleness is
    /// a claim about WHICH law is the only one, and an unnamed law cannot carry it.
    /// A `Some(true)` with no pinned shape is a cell-configuration HARD FAIL in
    /// [`crate::exec`], and `shapes/test-dsl-shapes.ttl` rejects the same pairing
    /// declaratively.
    pub expected_sole_finding: Option<bool>,
    pub rationale: Option<String>,
}

/// `gmeow:saPolarity` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Must,
    MustNot,
}

/// `gmeow:saScope` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Module,
    ModuleAndExamples,
}

/// `gmeow:expectedOutcome` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Conforms,
    Violates,
}

/// `gmeow:cqReasoning` value — the entailment lane a competency question runs under.
///
/// Three lanes: [`None`](ReasoningProfile::None) (asserted graph, SPARQL property paths),
/// [`Rdfs`](ReasoningProfile::Rdfs) (RDFS closure), and [`Native`](ReasoningProfile::Native)
/// (the full native `logic:` reasoner — the lane the n-ary `math:` algebra laws entail
/// their consequents through).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningProfile {
    /// `gmeow:reasoningNone` (the default): the asserted merged graph; SPARQL
    /// property paths supply transitive subclass/subproperty closure.
    #[default]
    None,
    /// `gmeow:reasoningRdfs`: the merged graph closed under RDFS (domain/range
    /// typing + type/subclass/subproperty propagation).
    Rdfs,
    /// `gmeow:reasoningLogic`: the merged graph closed under the NATIVE `logic:` reasoner
    /// (`gmeow_logic::reason::reason_program`) — the canonical entailment engine, run over
    /// the `LogicProgram` compiled from the slice sources plus the authored law examples.
    /// This is the lane the four `math:` algebra laws (associativity, the determinant
    /// homomorphism, the E8 group action, and homomorphic encryption) run under as LIVE
    /// entailment consumers: each competency question queries a consequent the law DERIVES
    /// through fixed-arity n-ary predication, not an asserted fact.
    Native,
}

// ── Introspection queries ──────────────────────────────────────────────────────

const PREFIX: &str = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n";

const Q_COMPETENCY: &str = "
SELECT ?cq ?queryFile ?query ?project ?expectAsk ?rowCount ?exactRows ?reasoning ?dataFile ?resultShape ?inputShape ?consumes ?rationale WHERE {
  ?cq a gmeow:CompetencyQuestion .
  OPTIONAL { ?cq gmeow:cqQueryFile ?queryFile }
  OPTIONAL { ?cq gmeow:cqQuery ?query }
  OPTIONAL { ?cq gmeow:cqProject ?project }
  OPTIONAL { ?cq gmeow:cqExpectAsk ?expectAsk }
  OPTIONAL { ?cq gmeow:cqExpectRowCount ?rowCount }
  OPTIONAL { ?cq gmeow:cqExactRows ?exactRows }
  OPTIONAL { ?cq gmeow:cqReasoning ?reasoning }
  OPTIONAL { ?cq gmeow:cqDataFile ?dataFile }
  OPTIONAL { ?cq gmeow:cqResultShape ?resultShape }
  OPTIONAL { ?cq gmeow:cqInputShape ?inputShape }
  OPTIONAL { ?cq gmeow:cqConsumes ?consumes }
  OPTIONAL { ?cq gmeow:cqRationale ?rationale }
}";

const Q_ROWS: &str = "
SELECT ?cq ?row ?var ?iri ?lit WHERE {
  ?cq gmeow:cqExpectRow ?row .
  ?row gmeow:rowCell ?cell .
  ?cell gmeow:cellVar ?var .
  OPTIONAL { ?cell gmeow:cellValueIri ?iri }
  OPTIONAL { ?cell gmeow:cellValueLiteral ?lit }
}";

const Q_STRUCTURAL: &str = "
SELECT ?sa ?polarity ?pattern ?shape ?scope ?failWitness ?rationale WHERE {
  ?sa a gmeow:StructuralAssertion ;
      gmeow:saPolarity ?polarity .
  OPTIONAL { ?sa gmeow:saPattern ?pattern }
  OPTIONAL { ?sa gmeow:saShape ?shape }
  OPTIONAL { ?sa gmeow:saScope ?scope }
  OPTIONAL { ?sa gmeow:saFailWitness ?failWitness }
  OPTIONAL { ?sa gmeow:saRationale ?rationale }
}";

const Q_CONFORMANCE: &str = "
SELECT ?ec ?file ?outcome ?code ?shape ?failureClass ?sole ?rationale WHERE {
  ?ec a gmeow:ExampleConformance ;
      gmeow:exampleFile ?file ;
      gmeow:expectedOutcome ?outcome .
  OPTIONAL { ?ec gmeow:expectedViolationCode ?code }
  OPTIONAL { ?ec gmeow:expectedSourceShape ?shape }
  OPTIONAL { ?ec gmeow:expectedFailureClass ?failureClass }
  OPTIONAL { ?ec gmeow:expectedSoleFinding ?sole }
  OPTIONAL { ?ec gmeow:conformanceRationale ?rationale }
}";

// ── Public entry point ─────────────────────────────────────────────────────────

/// Load one `tests/*.ttl` spec file and introspect every cell it declares.
///
/// # Errors
///
/// Hard-fails if the file cannot be parsed, an introspection query fails, or a cell
/// declares an unrecognized controlled-vocabulary value.
pub fn load_spec(path: &Path) -> Result<SpecFile> {
    let dataset = native_query::dataset_from_file(path).map_err(|e| {
        Diag::of_kind(SpecLoad {
            detail: format!("failed to load spec {}: {e}", path.display()),
        })
    })?;
    Ok(SpecFile {
        competency: parse_competency(&dataset)?,
        structural: parse_structural(&dataset)?,
        conformance: parse_conformance(&dataset)?,
    })
}

// ── Parsers ────────────────────────────────────────────────────────────────────

fn parse_competency(store: &Arc<RdfDataset>) -> Result<Vec<CompetencyQuestion>> {
    // 1. Scalars: one solution per CompetencyQuestion (all OPTIONALs single-valued).
    let mut by_iri: BTreeMap<String, CompetencyQuestion> = BTreeMap::new();
    for sol in select(store, &with_prefix(Q_COMPETENCY))? {
        let iri = require_iri(&sol, "cq")?;
        let candidate = CompetencyQuestion {
            iri: iri.clone(),
            query_inline: opt_string(&sol, "query"),
            query_file: opt_string(&sol, "queryFile"),
            project_query_file: opt_string(&sol, "project"),
            expect_ask: opt_bool(&sol, "expectAsk")?,
            expect_row_count: opt_u64(&sol, "rowCount")?,
            exact_rows: opt_bool(&sol, "exactRows")?.unwrap_or(false),
            expected_rows: Vec::new(),
            reasoning: match sol.get("reasoning").and_then(term_iri) {
                None => ReasoningProfile::None, // default
                Some(iri) => match local_name(&iri) {
                    "reasoningNone" => ReasoningProfile::None,
                    "reasoningRdfs" => ReasoningProfile::Rdfs,
                    "reasoningLogic" => ReasoningProfile::Native,
                    other => {
                        return Err(Diag::of_kind(SpecCell {
                            detail: format!("unknown cqReasoning gmeow:{other}"),
                        }));
                    }
                },
            },
            data_file: opt_string(&sol, "dataFile"),
            result_shape: opt_shape(store, &sol, "resultShape")?,
            input_shape: opt_shape(store, &sol, "inputShape")?,
            consumes: opt_string(&sol, "consumes"),
            rationale: opt_string(&sol, "rationale"),
        };
        // A multi-valued OPTIONAL (or a duplicated triple) can yield more than one
        // ?cq solution for the same question. Identical repeats are harmless, but
        // conflicting scalar fields would otherwise overwrite silently — making the
        // parsed spec depend on SPARQL solution order. Hard-fail on conflict.
        match by_iri.entry(iri) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(candidate);
            }
            std::collections::btree_map::Entry::Occupied(existing) => {
                if *existing.get() != candidate {
                    return Err(Diag::of_kind(SpecCell {
                        detail: format!(
                            "competency question {} has conflicting duplicate definitions \
                             (multiple solutions with differing scalar fields)",
                            existing.key()
                        ),
                    }));
                }
            }
        }
    }

    // 2. Rows: group cells by (cq, row), preserving one cell per variable.
    //    rows_by_cq: cq IRI -> (row term-string -> Vec<ExpectedCell>).
    let mut rows_by_cq: BTreeMap<String, BTreeMap<String, Vec<ExpectedCell>>> = BTreeMap::new();
    for sol in select(store, &with_prefix(Q_ROWS))? {
        let cq = require_iri(&sol, "cq")?;
        let row_key = sol.get("row").map(render_term).ok_or_else(|| {
            Diag::of_kind(SpecCell {
                detail: "cqExpectRow row missing ?row binding".to_owned(),
            })
        })?;
        let var = opt_string(&sol, "var").ok_or_else(|| {
            Diag::of_kind(SpecCell {
                detail: "ExpectedCell missing gmeow:cellVar".to_owned(),
            })
        })?;
        let value = match (sol.get("iri"), sol.get("lit")) {
            (Some(iri), None) => iri.clone(),
            (None, Some(lit)) => lit.clone(),
            (Some(_), Some(_)) => {
                return Err(Diag::of_kind(SpecCell {
                    detail: format!(
                        "ExpectedCell ?{var} binds both cellValueIri and cellValueLiteral (exactly one allowed)"
                    ),
                }));
            }
            (None, None) => {
                return Err(Diag::of_kind(SpecCell {
                    detail: format!(
                        "ExpectedCell ?{var} binds neither cellValueIri nor cellValueLiteral"
                    ),
                }));
            }
        };
        rows_by_cq
            .entry(cq)
            .or_default()
            .entry(row_key)
            .or_default()
            .push(ExpectedCell { var, value });
    }
    for (cq, rows) in rows_by_cq {
        // A gmeow:cqExpectRow whose ?cq matches no CompetencyQuestion is a spec
        // typo — fail loudly rather than silently dropping the rows, which could
        // leave a subset-check question passing against an empty expectation.
        let Some(question) = by_iri.get_mut(&cq) else {
            return Err(Diag::of_kind(SpecCell {
                detail: format!("gmeow:cqExpectRow references unknown competency question {cq}"),
            }));
        };
        question.expected_rows = rows
            .into_values()
            .map(|cells| ExpectedRow { cells })
            .collect();
    }

    Ok(by_iri.into_values().collect())
}

fn parse_structural(store: &Arc<RdfDataset>) -> Result<Vec<StructuralAssertion>> {
    let mut out = Vec::new();
    for sol in select(store, &with_prefix(Q_STRUCTURAL))? {
        let iri = require_iri(&sol, "sa")?;
        let polarity = match local_name(&require_iri(&sol, "polarity")?) {
            "must" => Polarity::Must,
            "mustNot" => Polarity::MustNot,
            other => {
                return Err(Diag::of_kind(SpecCell {
                    detail: format!("{iri}: unknown saPolarity gmeow:{other}"),
                }));
            }
        };
        let scope = match sol.get("scope").and_then(term_iri) {
            None => Scope::ModuleAndExamples, // default per the DSL vocabulary
            Some(iri) => match local_name(&iri) {
                "scopeModule" => Scope::Module,
                "scopeModuleAndExamples" => Scope::ModuleAndExamples,
                other => {
                    return Err(Diag::of_kind(SpecCell {
                        detail: format!("unknown saScope gmeow:{other}"),
                    }));
                }
            },
        };
        out.push(StructuralAssertion {
            iri,
            polarity,
            pattern: opt_string(&sol, "pattern"),
            shape: sol.get("shape").and_then(term_iri),
            scope,
            fail_witness: opt_string(&sol, "failWitness"),
            rationale: opt_string(&sol, "rationale"),
        });
    }
    Ok(out)
}

fn parse_conformance(store: &Arc<RdfDataset>) -> Result<Vec<ExampleConformance>> {
    let mut out = Vec::new();
    for sol in select(store, &with_prefix(Q_CONFORMANCE))? {
        let iri = require_iri(&sol, "ec")?;
        let outcome = match local_name(&require_iri(&sol, "outcome")?) {
            "conforms" => Outcome::Conforms,
            "violates" => Outcome::Violates,
            other => {
                return Err(Diag::of_kind(SpecCell {
                    detail: format!("{iri}: unknown expectedOutcome gmeow:{other}"),
                }));
            }
        };
        out.push(ExampleConformance {
            iri,
            file: opt_string(&sol, "file").ok_or_else(|| {
                Diag::of_kind(SpecCell {
                    detail: "ExampleConformance missing gmeow:exampleFile".to_owned(),
                })
            })?,
            outcome,
            violation_code: opt_string(&sol, "code"),
            // An IRI object; term_iri keeps it as the resolved absolute IRI so it
            // compares directly against the finding's `sh:sourceShape` term.
            expected_source_shape: sol.get("shape").and_then(term_iri),
            expected_failure_class: sol.get("failureClass").and_then(term_iri),
            expected_sole_finding: opt_bool(&sol, "sole")?,
            rationale: opt_string(&sol, "rationale"),
        });
    }
    Ok(out)
}

// ── logic:ResultShape parsing ────────────────────────────────────────────────────

/// The local name of a `logic:` IRI (the part after the namespace).
fn logic_local(iri: &str) -> &str {
    iri.strip_prefix(LOGIC_NS).unwrap_or(iri)
}

/// Resolve an optional `gmeow:cqResultShape` / `gmeow:cqInputShape` IRI binding into
/// the typed [`ResultShape`] it points at, parsed from the same spec store.
fn opt_shape(store: &Arc<RdfDataset>, sol: &Sol, var: &str) -> Result<Option<ResultShape>> {
    match sol.get(var).and_then(term_iri) {
        None => Ok(None),
        Some(iri) => Ok(Some(parse_result_shape(store, &iri)?)),
    }
}

/// Introspect a `logic:ResultShape` individual out of the spec store into the
/// canonical [`ResultShape`] type (the same authority the contract check uses).
///
/// # Errors
/// Hard-fails on an empty shape, a `logic:declaresColumn` node missing any of
/// the three required fields (`columnVariable`, `columnTermKind`, `columnBinding`),
/// an unknown term-kind / binding / cardinality value, or a `logic:RowsCount`
/// shape missing its `logic:shapeRowCount`.
///
/// The three required fields are fetched with OPTIONAL so that every declared
/// column node yields exactly one solution row — a missing required field becomes
/// an observable NULL rather than silently dropping the row and narrowing the
/// contract without error.
fn parse_result_shape(store: &Arc<RdfDataset>, shape_iri: &str) -> Result<ResultShape> {
    // Match every declared column node unconditionally, then OPTIONAL the three
    // required fields.  This guarantees one solution row per declared column,
    // so a missing required field is an observable NULL that we can name precisely
    // rather than a silently-vanishing row that shrinks the contract undetected.
    let cols_q = format!(
        "PREFIX logic: <{LOGIC_NS}>\n\
         SELECT ?col ?var ?kind ?datatype ?binding WHERE {{\n\
         \x20 <{shape_iri}> logic:declaresColumn ?col .\n\
         \x20 OPTIONAL {{ ?col logic:columnVariable ?var }}\n\
         \x20 OPTIONAL {{ ?col logic:columnTermKind ?kind }}\n\
         \x20 OPTIONAL {{ ?col logic:columnBinding ?binding }}\n\
         \x20 OPTIONAL {{ ?col logic:columnDatatype ?datatype }}\n\
         }}"
    );
    let mut columns: Vec<ResultColumn> = Vec::new();
    for sol in select(store, &cols_q)? {
        // Name the column node in every error so the spec author can locate the
        // offending blank node or IRI in their Turtle source.
        let col = sol
            .get("col")
            .map(render_term)
            .unwrap_or_else(|| "<unknown>".to_owned());

        // All three fields are required; their absence is a hard-fail with a
        // precise per-field error naming both the shape and the column node.
        let var = opt_string(&sol, "var").ok_or_else(|| {
            Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                     is missing logic:columnVariable"
                ),
            })
        })?;
        let kind_iri = sol.get("kind").and_then(term_iri).ok_or_else(|| {
            Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                     is missing logic:columnTermKind"
                ),
            })
        })?;
        let kind_local = logic_local(&kind_iri).to_owned();
        let term_kind = TermKind::from_local(&kind_local).ok_or_else(|| {
            Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: unknown logic:columnTermKind logic:{kind_local}"
                ),
            })
        })?;
        let binding_iri = sol.get("binding").and_then(term_iri).ok_or_else(|| {
            Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                     is missing logic:columnBinding"
                ),
            })
        })?;
        let binding_local = logic_local(&binding_iri).to_owned();
        let binding = ColumnBinding::from_local(&binding_local).ok_or_else(|| {
            Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: unknown logic:columnBinding logic:{binding_local}"
                ),
            })
        })?;
        // columnDatatype is genuinely optional — absent for IRI/blank-node columns
        // and for untyped-literal columns.
        let datatype = sol.get("datatype").and_then(term_iri);
        let kind = match term_kind {
            TermKind::Iri => ColumnKind::Iri,
            TermKind::BlankNode => ColumnKind::BlankNode,
            TermKind::Literal => ColumnKind::Literal { datatype },
            TermKind::TripleTerm => ColumnKind::TripleTerm,
        };
        columns.push(ResultColumn { var, kind, binding });
    }
    if columns.is_empty() {
        return Err(Diag::of_kind(ResultShapeParse {
            detail: format!(
                "ResultShape <{shape_iri}> declares no logic:declaresColumn — an empty result shape types nothing"
            ),
        }));
    }

    let card_q = format!(
        "PREFIX logic: <{LOGIC_NS}>\n\
         SELECT ?card ?count WHERE {{\n\
         \x20 <{shape_iri}> logic:shapeCardinality ?card .\n\
         \x20 OPTIONAL {{ <{shape_iri}> logic:shapeRowCount ?count }}\n\
         }}"
    );
    let card_sols = select(store, &card_q)?;
    let card_sol = card_sols.first().ok_or_else(|| {
        Diag::of_kind(ResultShapeParse {
            detail: format!("ResultShape <{shape_iri}> has no logic:shapeCardinality"),
        })
    })?;
    let card_local = logic_local(&require_iri(card_sol, "card")?).to_owned();
    let cardinality = match card_local.as_str() {
        "RowsExact" => RowCardinality::Exact,
        "RowsContains" => RowCardinality::Contains,
        "RowsCount" => {
            let count = opt_u64(card_sol, "count")?.ok_or_else(|| {
                Diag::of_kind(ResultShapeParse {
                    detail: format!(
                        "ResultShape <{shape_iri}>: logic:RowsCount requires logic:shapeRowCount"
                    ),
                })
            })?;
            RowCardinality::Count(count)
        }
        other => {
            return Err(Diag::of_kind(ResultShapeParse {
                detail: format!(
                    "ResultShape <{shape_iri}>: unknown logic:shapeCardinality logic:{other}"
                ),
            }));
        }
    };
    Ok(ResultShape::new(columns, cardinality))
}

// ── SPARQL + term helpers ──────────────────────────────────────────────────────

fn with_prefix(body: &str) -> String {
    format!("{PREFIX}{body}")
}

/// One introspection solution: a variable→term view over a single native result row,
/// the native replacement for the oxigraph `QuerySolution`.
pub struct Sol {
    variables: Arc<Vec<String>>,
    row: Vec<Option<TermValue>>,
}

impl Sol {
    /// The bound term for `var`, if any (an unbound OPTIONAL column reads `None`).
    fn get(&self, var: &str) -> Option<&TermValue> {
        let idx = self.variables.iter().position(|v| v == var)?;
        self.row.get(idx).and_then(Option::as_ref)
    }
}

/// Run a SELECT introspection query and collect its solutions as [`Sol`] views.
fn select(store: &Arc<RdfDataset>, query: &str) -> Result<Vec<Sol>> {
    let solutions = native_query::select(store, query).map_err(|e| {
        Diag::of_kind(SparqlEval {
            detail: format!("introspection query error: {e}"),
        })
    })?;
    let variables = Arc::new(solutions.variables);
    Ok(solutions
        .rows
        .into_iter()
        .map(|row| Sol {
            variables: Arc::clone(&variables),
            row,
        })
        .collect())
}

/// The local name of a `gmeow:` IRI (the part after the namespace).
fn local_name(iri: &str) -> &str {
    iri.strip_prefix(NS).unwrap_or(iri)
}

fn term_iri(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(iri) => Some(iri.clone()),
        _ => None,
    }
}

fn require_iri(sol: &Sol, var: &str) -> Result<String> {
    sol.get(var).and_then(term_iri).ok_or_else(|| {
        Diag::of_kind(TypedBinding {
            detail: format!("expected ?{var} to bind an IRI"),
        })
    })
}

/// The lexical value of a bound literal (or the IRI string of a named node).
fn opt_string(sol: &Sol, var: &str) -> Option<String> {
    sol.get(var).map(|term| match term {
        TermValue::Literal { lexical_form, .. } => lexical_form.clone(),
        TermValue::Iri(iri) => iri.clone(),
        other => render_term(other),
    })
}

fn opt_bool(sol: &Sol, var: &str) -> Result<Option<bool>> {
    match sol.get(var) {
        None => Ok(None),
        // xsd:boolean lexical space: true/false plus the 1/0 alternatives. Anything
        // else is a malformed literal that would silently read as `false` if coerced
        // — hard-fail instead (a typoed boolean must never quietly weaken a guard).
        Some(TermValue::Literal { lexical_form, .. }) => match lexical_form.as_str() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            other => Err(Diag::of_kind(TypedBinding {
                detail: format!("?{var} is not an xsd:boolean (true/false): {other:?}"),
            })),
        },
        Some(other) => Err(Diag::of_kind(TypedBinding {
            detail: format!("?{var} expected a literal, got {}", render_term(other)),
        })),
    }
}

fn opt_u64(sol: &Sol, var: &str) -> Result<Option<u64>> {
    match sol.get(var) {
        None => Ok(None),
        Some(TermValue::Literal { lexical_form, .. }) => {
            lexical_form.parse::<u64>().map(Some).map_err(|e| {
                Diag::of_kind(TypedBinding {
                    detail: format!("?{var} is not a non-negative integer: {e}"),
                })
            })
        }
        Some(other) => Err(Diag::of_kind(TypedBinding {
            detail: format!("?{var} expected a literal, got {}", render_term(other)),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;

    fn epistemics_tests_dir() -> std::path::PathBuf {
        paths::slices_root().join("core/epistemics/tests")
    }

    #[test]
    fn parses_competency_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("competency.ttl"))
            .expect("competency.ttl must parse");
        assert_eq!(spec.competency.len(), 2, "two competency questions");

        let agents = spec
            .competency
            .iter()
            .find(|c| c.iri.ends_with("cqAgentKinds"))
            .expect("cqAgentKinds present");
        assert_eq!(
            agents.query_file.as_deref(),
            Some("queries/competency/agents.rq")
        );
        assert!(agents.exact_rows);
        assert_eq!(
            agents.expected_rows.len(),
            8,
            "eight fully-enumerated agent kinds: the six rigid Kinds (Agent, Builder, \
             Organization, Person, Sensor, SoftwareAgent) plus the inhabitation-slice \
             role-mixins DigitalSubject and Inhabitant, both grounded rdfs:subClassOf gmeow:Agent"
        );
        // Each row has exactly one cell binding ?agentKind to a gmeow: IRI.
        for row in &agents.expected_rows {
            assert_eq!(row.cells.len(), 1);
            assert_eq!(row.cells[0].var, "agentKind");
            assert!(matches!(row.cells[0].value, TermValue::Iri(_)));
        }

        let roles = spec
            .competency
            .iter()
            .find(|c| c.iri.ends_with("cqContributionRoles"))
            .expect("cqContributionRoles present");
        // T3 finished the enumeration: the coarse row-count escape hatch is gone,
        // every one of the 48 roles is pinned, and cqExactRows is set.
        assert_eq!(roles.expect_row_count, None);
        assert!(roles.exact_rows);
        assert_eq!(roles.expected_rows.len(), 48, "all 48 roles enumerated");
    }

    #[test]
    fn parses_structural_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("structural.ttl"))
            .expect("structural.ttl must parse");
        // The T3 migration expanded the two T2 keystones to the full set of
        // module invariants lifted from test_epistemics.py.
        assert!(
            spec.structural.len() >= 13,
            "the migrated structural invariants are present, got {}",
            spec.structural.len()
        );

        let must = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saKnowsThatSubPropertyOfBelieves"))
            .expect("keystone entailment present");
        assert_eq!(must.polarity, Polarity::Must);
        assert_eq!(must.scope, Scope::Module);
        assert!(must.pattern.as_deref().unwrap().contains("subPropertyOf"));

        let must_not = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saNoTruthBit"))
            .expect("no-truth-bit present");
        assert_eq!(must_not.polarity, Polarity::MustNot);

        // A migrated MUST-NOT over a VALUES set (the open-range invariant) parses.
        let open_range = spec
            .structural
            .iter()
            .find(|s| s.iri.ends_with("saSpineOpenRange"))
            .expect("migrated spine-open-range present");
        assert_eq!(open_range.polarity, Polarity::MustNot);
    }

    #[test]
    fn parses_conformance_exemplars() {
        let spec = load_spec(&epistemics_tests_dir().join("example-conformance.ttl"))
            .expect("example-conformance.ttl must parse");
        assert_eq!(spec.conformance.len(), 2);

        let conforms = spec
            .conformance
            .iter()
            .find(|c| c.outcome == Outcome::Conforms)
            .expect("conforming fixture present");
        assert_eq!(conforms.file, "examples/justification-and-defeat.ttl");
        assert!(conforms.violation_code.is_none());

        let violates = spec
            .conformance
            .iter()
            .find(|c| c.outcome == Outcome::Violates)
            .expect("violating fixture present");
        assert_eq!(
            violates.violation_code.as_deref(),
            Some("shacl.MinCountConstraintComponent")
        );
    }

    /// Load inline Turtle into a native dataset via the canonical codec:
    /// `parse_dataset` into the frozen IR — the same codec the rest of the stack uses
    /// (and lenient on long private-use language tags, like the harness).
    fn store_from_turtle(ttl: &str) -> Arc<RdfDataset> {
        native_query::dataset_from_turtle(ttl).expect("valid turtle")
    }

    #[test]
    fn rejects_conflicting_duplicate_competency_questions() {
        // Two cqExpectRowCount values for the SAME question yield two ?cq solutions
        // with conflicting scalar fields. Silently keeping whichever the SPARQL
        // engine returned last would make the spec solution-order-dependent.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqDup a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExpectRowCount 1, 2 .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("conflicting duplicate must hard-fail");
        assert!(
            err.message().contains("conflicting duplicate"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn identical_duplicate_competency_solutions_are_harmless() {
        // A repeated identical scalar (here a duplicated cqQueryFile triple) is not a
        // conflict — it must parse, collapsing to one question.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqSame a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\", \"q.rq\" .\n";
        let spec = parse_competency(&store_from_turtle(ttl)).expect("identical repeat is fine");
        assert_eq!(spec.len(), 1);
    }

    #[test]
    fn rejects_malformed_boolean_literal() {
        // A typoed boolean must hard-fail, never silently coerce to false (which would
        // quietly flip cqExactRows from exact-match to subset).
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqBad a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqExactRows \"ture\" .\n"; // codespell:ignore ture
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("malformed boolean must hard-fail");
        assert!(
            err.message().contains("xsd:boolean"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parses_cq_result_shape_into_the_canonical_type() {
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"agent\" ; logic:columnTermKind logic:TermKindIri ; logic:columnBinding logic:BindingRequired ] ,\n\
                                     [ logic:columnVariable \"name\" ; logic:columnTermKind logic:TermKindLiteral ; logic:columnDatatype <http://www.w3.org/2001/XMLSchema#string> ; logic:columnBinding logic:BindingOptional ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
        let spec = parse_competency(&store_from_turtle(ttl)).expect("shape parses");
        let cq = &spec[0];
        let shape = cq.result_shape.as_ref().expect("result_shape present");
        assert_eq!(shape.cardinality, RowCardinality::Exact);
        assert_eq!(shape.columns.len(), 2);
        // columns are canonicalised sorted-by-var: agent, name
        assert_eq!(shape.columns[0].var, "agent");
        assert_eq!(shape.columns[0].kind, ColumnKind::Iri);
        assert_eq!(shape.columns[0].binding, ColumnBinding::Required);
        assert_eq!(shape.columns[1].var, "name");
        assert_eq!(
            shape.columns[1].kind,
            ColumnKind::Literal {
                datatype: Some("http://www.w3.org/2001/XMLSchema#string".to_owned())
            }
        );
        assert_eq!(shape.columns[1].binding, ColumnBinding::Optional);
    }

    #[test]
    fn rejects_result_shape_with_unknown_term_kind() {
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ; logic:columnTermKind logic:TermKindBogus ; logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsContains .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("unknown term-kind must hard-fail");
        assert!(
            err.message().contains("columnTermKind"),
            "unexpected error: {err}"
        );
    }

    // ── ResultShape hard-fail discipline ─────────────────────────────
    //
    // A `logic:declaresColumn` node MISSING any of the three required fields
    // (`columnVariable`, `columnTermKind`, `columnBinding`) must HARD-FAIL with
    // a precise per-field error that names the missing predicate and the column
    // node.  Before the C5 fix the required fields were basic-graph-pattern
    // triples in the SPARQL query, so a missing field silently dropped that
    // column's solution row — the contract shrank without any diagnostic.

    #[test]
    fn rejects_result_shape_column_missing_term_kind() {
        // columnTermKind is absent; the column must not be silently dropped.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ;\n\
                                       logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("missing columnTermKind must hard-fail");
        assert!(
            err.message().contains("columnTermKind"),
            "error must name the missing predicate; got: {err}"
        );
    }

    #[test]
    fn rejects_result_shape_column_missing_column_variable() {
        // columnVariable is absent; the column must not be silently dropped.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnTermKind logic:TermKindIri ;\n\
                                       logic:columnBinding logic:BindingRequired ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("missing columnVariable must hard-fail");
        assert!(
            err.message().contains("columnVariable"),
            "error must name the missing predicate; got: {err}"
        );
    }

    #[test]
    fn rejects_result_shape_column_missing_column_binding() {
        // columnBinding is absent; the column must not be silently dropped.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn [ logic:columnVariable \"x\" ;\n\
                                       logic:columnTermKind logic:TermKindIri ] ;\n\
                logic:shapeCardinality logic:RowsExact .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("missing columnBinding must hard-fail");
        assert!(
            err.message().contains("columnBinding"),
            "error must name the missing predicate; got: {err}"
        );
    }

    #[test]
    fn well_formed_multi_column_shape_still_parses() {
        // A complete multi-column shape (all three required fields present on
        // every column) must still parse to the expected canonical columns after
        // the C5 OPTIONAL rewrite.  Regression guard for the positive path.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" ;\n\
                gmeow:cqResultShape ex:shape .\n\
            ex:shape a logic:ResultShape ;\n\
                logic:declaresColumn\n\
                    [ logic:columnVariable \"agent\" ;\n\
                      logic:columnTermKind logic:TermKindIri ;\n\
                      logic:columnBinding logic:BindingRequired ] ,\n\
                    [ logic:columnVariable \"score\" ;\n\
                      logic:columnTermKind logic:TermKindLiteral ;\n\
                      logic:columnBinding logic:BindingOptional ] ;\n\
                logic:shapeCardinality logic:RowsContains .\n";
        let spec = parse_competency(&store_from_turtle(ttl))
            .expect("well-formed multi-column shape must parse");
        let cq = &spec[0];
        let shape = cq.result_shape.as_ref().expect("result_shape present");
        assert_eq!(shape.cardinality, RowCardinality::Contains);
        // columns are canonically sorted by var: agent, score
        assert_eq!(shape.columns.len(), 2, "both columns present, none dropped");
        assert_eq!(shape.columns[0].var, "agent");
        assert_eq!(shape.columns[0].kind, ColumnKind::Iri);
        assert_eq!(shape.columns[0].binding, ColumnBinding::Required);
        assert_eq!(shape.columns[1].var, "score");
        assert_eq!(
            shape.columns[1].kind,
            ColumnKind::Literal { datatype: None }
        );
        assert_eq!(shape.columns[1].binding, ColumnBinding::Optional);
    }

    #[test]
    fn rejects_expected_rows_for_unknown_competency_question() {
        // A gmeow:cqExpectRow whose ?cq matches no CompetencyQuestion is a spec
        // typo; the rows must not be silently dropped.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cqReal a gmeow:CompetencyQuestion ;\n\
                gmeow:cqQueryFile \"q.rq\" .\n\
            ex:cqTypo gmeow:cqExpectRow [ gmeow:rowCell [ gmeow:cellVar \"x\" ; gmeow:cellValueLiteral \"v\" ] ] .\n";
        let err = parse_competency(&store_from_turtle(ttl))
            .expect_err("orphan expected rows must hard-fail");
        assert!(
            err.message().contains("unknown competency question"),
            "unexpected error: {err}"
        );
    }
}
