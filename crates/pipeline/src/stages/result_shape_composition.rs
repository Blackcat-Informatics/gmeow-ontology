// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build-time validation of cross-slice CQ composition links: every
//! `gmeow:cqConsumes` edge is checked to ensure the producer's
//! `gmeow:cqResultShape` structurally satisfies the consumer's
//! `gmeow:cqInputShape` **before** any query executes.
//!
//! The single exported stage is `ResultShapeCompositionStage`, a pure-leaf
//! validation that runs on the competency corpus at build time, hard-failing on
//! any composition mismatch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::sync::Arc;

use gmeow_logic_compile::result_shape::{
    ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality, TermKind,
};
use purrdf::{RdfDataset, TermValue};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::native_query::{self, Solutions};
use crate::stages::result_shapes::competency_files;

// ── Namespace constants ────────────────────────────────────────────────────────

use gmeow_ns::LOGIC_NS;

// ── Internal SPARQL helpers ────────────────────────────────────────────────────

/// A single solution row paired with its variable schema (column-indexed access).
struct Row<'a> {
    vars: &'a [String],
    cells: &'a [Option<TermValue>],
}

impl Row<'_> {
    fn cell(&self, var: &str) -> Option<&TermValue> {
        self.vars
            .iter()
            .position(|v| v == var)
            .and_then(|i| self.cells.get(i))
            .and_then(Option::as_ref)
    }
}

fn run_select(dataset: &Arc<RdfDataset>, query: &str) -> Result<Solutions, gmeow_errors::Diag> {
    native_query::select(dataset, query)
}

fn sol_iri(row: &Row<'_>, var: &str) -> Option<String> {
    match row.cell(var) {
        Some(TermValue::Iri(iri)) => Some(iri.clone()),
        _ => None,
    }
}

fn sol_str(row: &Row<'_>, var: &str) -> Option<String> {
    row.cell(var).map(|t| match t {
        TermValue::Literal { lexical_form, .. } => lexical_form.clone(),
        TermValue::Iri(n) => n.clone(),
        TermValue::Blank { label, .. } => format!("_:{label}"),
        TermValue::Triple { .. } => native_query_term_to_string(t),
    })
}

fn sol_u64(row: &Row<'_>, var: &str) -> Result<Option<u64>, gmeow_errors::Diag> {
    match row.cell(var) {
        None => Ok(None),
        Some(TermValue::Literal { lexical_form, .. }) => {
            lexical_form.parse::<u64>().map(Some).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("?{var} is not a non-negative integer: {e}"),
                })
            })
        }
        Some(other) => Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "?{var} expected a literal, got {}",
                native_query_term_to_string(other)
            ),
        })),
    }
}

/// A debug-ish string form of a term for error messages (never on the byte-exact path).
fn native_query_term_to_string(t: &TermValue) -> String {
    match t {
        TermValue::Iri(iri) => format!("<{iri}>"),
        TermValue::Blank { label, .. } => format!("_:{label}"),
        TermValue::Literal { lexical_form, .. } => format!("{lexical_form:?}"),
        TermValue::Triple { s, p, o } => format!(
            "<< {} {} {} >>",
            native_query_term_to_string(s),
            native_query_term_to_string(p),
            native_query_term_to_string(o)
        ),
    }
}

fn logic_local(iri: &str) -> &str {
    iri.strip_prefix(LOGIC_NS).unwrap_or(iri)
}

// ── ResultShape parsing (mirrors dsl.rs::parse_result_shape) ──────────────────

/// Parse a `logic:ResultShape` individual from the competency store into the
/// canonical [`ResultShape`] type.  Hard-fails on any structural defect.
fn parse_result_shape(
    store: &Arc<RdfDataset>,
    shape_iri: &str,
) -> Result<ResultShape, gmeow_errors::Diag> {
    // Columns — use OPTIONAL so a missing required field is an observable NULL.
    let cols_q = format!(
        "PREFIX logic: <{LOGIC_NS}>\n\
         SELECT ?col ?var ?kind ?datatype ?binding WHERE {{\n\
           <{shape_iri}> logic:declaresColumn ?col .\n\
           OPTIONAL {{ ?col logic:columnVariable ?var }}\n\
           OPTIONAL {{ ?col logic:columnTermKind ?kind }}\n\
           OPTIONAL {{ ?col logic:columnBinding ?binding }}\n\
           OPTIONAL {{ ?col logic:columnDatatype ?datatype }}\n\
         }}"
    );
    let mut columns: Vec<ResultColumn> = Vec::new();
    let cols_sol = run_select(store, &cols_q)?;
    for cells in &cols_sol.rows {
        let sol = Row {
            vars: &cols_sol.variables,
            cells,
        };
        let col = sol
            .cell("col")
            .map(native_query_term_to_string)
            .unwrap_or_else(|| "<unknown>".to_owned());

        let var = sol_str(&sol, "var").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                 is missing logic:columnVariable"
                ),
            })
        })?;
        let kind_iri = sol_iri(&sol, "kind").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                 is missing logic:columnTermKind"
                ),
            })
        })?;
        let kind_local = logic_local(&kind_iri).to_owned();
        let term_kind = TermKind::from_local(&kind_local).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}>: unknown logic:columnTermKind logic:{kind_local}"
                ),
            })
        })?;
        let binding_iri = sol_iri(&sol, "binding").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}>: logic:declaresColumn {col} \
                 is missing logic:columnBinding"
                ),
            })
        })?;
        let binding_local = logic_local(&binding_iri).to_owned();
        let binding = ColumnBinding::from_local(&binding_local).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}>: unknown logic:columnBinding logic:{binding_local}"
                ),
            })
        })?;
        let datatype = sol_iri(&sol, "datatype");
        let kind = match term_kind {
            TermKind::Iri => ColumnKind::Iri,
            TermKind::BlankNode => ColumnKind::BlankNode,
            TermKind::Literal => ColumnKind::Literal { datatype },
            TermKind::TripleTerm => ColumnKind::TripleTerm,
        };
        columns.push(ResultColumn { var, kind, binding });
    }
    if columns.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::InvalidDeclaration {
                message: format!(
                    "ResultShape <{shape_iri}> declares no logic:declaresColumn — \
             an empty result shape types nothing"
                ),
            },
        ));
    }

    // Cardinality.
    let card_q = format!(
        "PREFIX logic: <{LOGIC_NS}>\n\
         SELECT ?card ?count WHERE {{\n\
           <{shape_iri}> logic:shapeCardinality ?card .\n\
           OPTIONAL {{ <{shape_iri}> logic:shapeRowCount ?count }}\n\
         }}"
    );
    let card_sols = run_select(store, &card_q)?;
    let card_cells = card_sols.rows.first().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!("ResultShape <{shape_iri}> has no logic:shapeCardinality"),
        })
    })?;
    let card_sol = Row {
        vars: &card_sols.variables,
        cells: card_cells,
    };
    let card_iri = sol_iri(&card_sol, "card").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!(
                "ResultShape <{shape_iri}>: logic:shapeCardinality did not bind an IRI"
            ),
        })
    })?;
    let card_local = logic_local(&card_iri).to_owned();
    let cardinality = match card_local.as_str() {
        "RowsExact" => RowCardinality::Exact,
        "RowsContains" => RowCardinality::Contains,
        "RowsCount" => {
            let count = sol_u64(&card_sol, "count")?.ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                    message: format!(
                        "ResultShape <{shape_iri}>: logic:RowsCount requires logic:shapeRowCount"
                    ),
                })
            })?;
            RowCardinality::Count(count)
        }
        other => {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::InvalidDeclaration {
                    message: format!(
                        "ResultShape <{shape_iri}>: unknown logic:shapeCardinality logic:{other}"
                    ),
                },
            ));
        }
    };
    Ok(ResultShape::new(columns, cardinality))
}

// ── Core validation logic ──────────────────────────────────────────────────────

/// Load every competency file into one native dataset (same approach as `result_shapes`):
/// each file parsed through the canonical native codec and unioned (blanks standardized
/// apart per source).
fn load_competency_store(root: &Path) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    native_query::dataset_from_files(&competency_files(root)?)
}

/// Validate every `gmeow:cqConsumes` composition link in the given store.
///
/// For each `?consumer gmeow:cqConsumes ?producer` triple the function:
/// 1. Resolves the consumer's `gmeow:cqInputShape` (hard-fail if absent).
/// 2. Resolves the producer's `gmeow:cqResultShape` (hard-fail if producer not
///    found or result shape absent).
/// 3. Calls `input_shape.is_satisfiable_by(&producer_shape)` and records any
///    [`Mismatch`](gmeow_logic_compile::result_shape::Mismatch).
///
/// All violations are collected and returned as a single combined error message
/// (sorted by consumer IRI for determinism).  Returns `Ok(())` when every link
/// is structurally compatible.
pub fn validate_store(store: &Arc<RdfDataset>) -> Result<(), gmeow_errors::Diag> {
    // Find all cqConsumes links.
    let links_q = "\
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
        SELECT ?consumer ?producer WHERE {\n\
          ?consumer gmeow:cqConsumes ?producer .\n\
        }";
    let links = run_select(store, links_q)?;

    // Collect violations keyed by consumer IRI for deterministic output.
    let mut violations: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for cells in &links.rows {
        let sol = Row {
            vars: &links.variables,
            cells,
        };
        let consumer_iri = sol_iri(&sol, "consumer").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: "cqConsumes: ?consumer did not bind an IRI".to_owned(),
            })
        })?;
        let producer_iri = sol_iri(&sol, "producer").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                message: "cqConsumes: ?producer did not bind an IRI".to_owned(),
            })
        })?;

        // Resolve the consumer's input shape (required when cqConsumes is declared).
        let input_shape_iri_q = format!(
            "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
             SELECT ?shape WHERE {{\n\
               <{consumer_iri}> gmeow:cqInputShape ?shape .\n\
             }}"
        );
        let input_sols = run_select(store, &input_shape_iri_q)?;
        let input_shape_iri = input_sols
            .rows
            .first()
            .and_then(|cells| {
                sol_iri(
                    &Row {
                        vars: &input_sols.variables,
                        cells,
                    },
                    "shape",
                )
            })
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                    message: format!(
                        "composition: <{consumer_iri}> has gmeow:cqConsumes \
                     but no gmeow:cqInputShape"
                    ),
                })
            })?;

        // Resolve the producer's result shape (required when consumed).
        let result_shape_iri_q = format!(
            "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
             SELECT ?shape WHERE {{\n\
               <{producer_iri}> gmeow:cqResultShape ?shape .\n\
             }}"
        );
        let result_sols = run_select(store, &result_shape_iri_q)?;
        let result_shape_iri = result_sols
            .rows
            .first()
            .and_then(|cells| {
                sol_iri(
                    &Row {
                        vars: &result_sols.variables,
                        cells,
                    },
                    "shape",
                )
            })
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
                    message: format!(
                        "composition: <{consumer_iri}> consumes <{producer_iri}> \
                     but that producer has no gmeow:cqResultShape"
                    ),
                })
            })?;

        // Parse both shapes.
        let input_shape = parse_result_shape(store, &input_shape_iri)?;
        let producer_shape = parse_result_shape(store, &result_shape_iri)?;

        // Check structural compatibility.
        if let Err(mismatch) = input_shape.is_satisfiable_by(&producer_shape) {
            violations
                .entry(consumer_iri.clone())
                .or_default()
                .push(format!(
                    "consumer <{consumer_iri}> <- producer <{producer_iri}>: {mismatch}"
                ));
        }
    }

    if violations.is_empty() {
        return Ok(());
    }

    // Aggregate all violations into one error message (sorted by consumer IRI).
    let mut lines: Vec<String> = Vec::new();
    for msgs in violations.values() {
        for msg in msgs {
            lines.push(msg.clone());
        }
    }
    Err(gmeow_errors::Diag::of_kind(
        crate::error::InvalidDeclaration {
            message: format!(
                "result-shape composition violations ({} total):\n{}",
                lines.len(),
                lines.join("\n")
            ),
        },
    ))
}

/// Validate all `gmeow:cqConsumes` links found under `root/slices/`.
pub fn validate_compositions(root: &Path) -> Result<(), gmeow_errors::Diag> {
    let store = load_competency_store(root)?;
    validate_store(&store)
}

// ── Stage impl ────────────────────────────────────────────────────────────────

/// The `result-shape-composition` validation leaf stage.
pub struct ResultShapeCompositionStage;

impl Stage for ResultShapeCompositionStage {
    fn id(&self) -> &str {
        "stage-validate-result-shape-composition"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "result_shape_composition.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        competency_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        validate_compositions(input.root)?;
        // Pure validation leaf — no artifacts emitted.
        Ok(StageOutput::new(StageProduct::new(
            self.id(),
            "result-shape-composition-ok",
        )))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Positive: every committed composition link in the real corpus is compatible.
    #[test]
    fn validate_compositions_corpus_ok() {
        let root = repo_root();
        validate_compositions(&root).expect("corpus compositions must all be compatible");
    }

    /// Negative: a consumer whose cqInputShape requires a column the producer's
    /// cqResultShape does NOT provide must produce an Err.
    #[test]
    fn incompatible_composition_hard_fails() {
        // Producer declares only column ?x (IRI); consumer requires ?x AND ?y (IRI).
        // is_satisfiable_by must reject the link.
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex:    <https://example.org/> .\n\
            \n\
            ex:producer gmeow:cqResultShape ex:producerShape .\n\
            ex:consumer gmeow:cqConsumes    ex:producer ;\n\
                        gmeow:cqInputShape  ex:consumerShape .\n\
            \n\
            ex:producerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n\
            \n\
            ex:consumerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"y\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n";
        let store = native_query::dataset_from_turtle(ttl.as_bytes(), "test").unwrap();
        let result = validate_store(&store);
        assert!(
            result.is_err(),
            "expected Err for missing column ?y, got Ok"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains('y') || msg.contains("mismatch"),
            "error message should mention the missing column or mismatch: {msg}"
        );
    }

    /// Positive in-memory: a perfectly compatible link passes validate_store.
    #[test]
    fn compatible_composition_passes() {
        // Producer declares ?x (IRI, Required); consumer requires exactly ?x (IRI).
        let ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix ex:    <https://example.org/> .\n\
            \n\
            ex:producer gmeow:cqResultShape ex:producerShape .\n\
            ex:consumer gmeow:cqConsumes    ex:producer ;\n\
                        gmeow:cqInputShape  ex:consumerShape .\n\
            \n\
            ex:producerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n\
            \n\
            ex:consumerShape a logic:ResultShape ;\n\
                logic:shapeCardinality logic:RowsContains ;\n\
                logic:declaresColumn [\n\
                    logic:columnVariable \"x\" ;\n\
                    logic:columnTermKind logic:TermKindIri ;\n\
                    logic:columnBinding  logic:BindingRequired\n\
                ] .\n";
        let store = native_query::dataset_from_turtle(ttl.as_bytes(), "test").unwrap();
        validate_store(&store).expect("compatible composition must pass");
    }
}
