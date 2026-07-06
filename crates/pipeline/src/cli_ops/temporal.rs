// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! TQL — the GMEOW Temporal Query Language executor.
//!
//! The Rust port of `gmeow_tools.temporal_query`. TQL is a small *query algebra*
//! over the events model, realized as parameterized SPARQL 1.1 queries
//! (`slices/core/temporal/queries/tql/*.rq`) rather than a bespoke temporal-query
//! engine: the model carries the Allen interval algebra and the four clocks, and
//! standard SPARQL 1.1 property paths compute the transitive temporal closures with
//! no materializing reasoner. This module is their *executor* — it binds query
//! parameters and runs them over an asserted graph through the native
//! [`NativeSparqlEngine`], the same engine the projection drivers use.
//!
//! Parameters are bound as native SPARQL **pre-bindings** ([`SparqlRequest::substitutions`]),
//! the injection-free replacement for rdflib `initBindings`: a value never touches
//! the query text, so there is no injection surface.

use std::collections::BTreeMap;
use std::path::Path;

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{SparqlEngine, SparqlRequest, SparqlResult, TermValue};

use crate::stages::native_query::{Solutions, dataset_from_turtle};

/// A named TQL query and the parameters it expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalQuery {
    /// The registry key (also the `<name>.rq` stem).
    pub name: &'static str,
    /// The SPARQL variable names this query pre-binds (no leading `?`).
    pub parameters: &'static [&'static str],
    /// A one-line human summary.
    pub summary: &'static str,
}

/// The TQL query registry — name → its parameters + one-line summary. Mirrors the
/// Python `TEMPORAL_QUERIES` dict exactly (same names, same parameters, same order).
pub fn temporal_queries() -> BTreeMap<&'static str, TemporalQuery> {
    const ROWS: &[TemporalQuery] = &[
        TemporalQuery {
            name: "allen-closure",
            parameters: &[],
            summary: "every transitively-ordered (earlier, later) event pair",
        },
        TemporalQuery {
            name: "before-event",
            parameters: &["focus"],
            summary: "events temporally before a focus event",
        },
        TemporalQuery {
            name: "during-event",
            parameters: &["focus"],
            summary: "events temporally within a focus event",
        },
        TemporalQuery {
            name: "timeline",
            parameters: &[],
            summary: "every event with its effective start instant, ordered",
        },
        TemporalQuery {
            name: "overlapping-window",
            parameters: &["windowStart", "windowEnd"],
            summary: "events overlapping a [windowStart, windowEnd] span",
        },
        TemporalQuery {
            name: "bitemporal",
            parameters: &["validAt", "asOf"],
            summary: "claims valid at ?validAt and asserted by ?asOf (four clocks)",
        },
        TemporalQuery {
            name: "interval-allen-closure",
            parameters: &[],
            summary: "transitively-ordered TimeInterval pairs via intervalBefore+",
        },
        TemporalQuery {
            name: "period-containment",
            parameters: &[],
            summary: "named periods and their containing ancestors via periodPartOf+",
        },
        TemporalQuery {
            name: "frame-matching",
            parameters: &["frame"],
            summary: "instants/intervals expressed in a given temporal frame",
        },
    ];
    ROWS.iter().map(|q| (q.name, q.clone())).collect()
}

/// The SPARQL text of a named TQL query, read from `query_dir/<name>.rq`.
///
/// # Errors
///
/// - The name is not a known TQL query (mirrors the Python `KeyError`).
/// - The `<name>.rq` file cannot be read.
fn query_text(query_dir: &Path, name: &str) -> Result<String, gmeow_errors::Diag> {
    let registry = temporal_queries();
    if !registry.contains_key(name) {
        let known: Vec<&str> = registry.keys().copied().collect();
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "temporal-query".to_string(),
            message: format!(
                "unknown temporal query {name:?}; known: {}",
                known.join(", ")
            ),
        }));
    }
    let path = query_dir.join(format!("{name}.rq"));
    std::fs::read_to_string(&path).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "temporal-query".to_string(),
            message: format!("read TQL query {}: {e}", path.display()),
        })
    })
}

/// Run a named TQL query over a Turtle events graph.
///
/// The Rust port of `temporal_query.run_temporal_query`. The query is loaded from
/// `query_dir` (the caller resolves the equivalent of `TEMPORAL_QUERY_DIR`), each
/// declared parameter must appear in `bindings`, and the result rows come back as
/// the dataset-independent [`Solutions`] egress shape.
///
/// * `query_dir` — directory holding the `<name>.rq` TQL sources.
/// * `name` — a key of [`temporal_queries`].
/// * `source_ttl` — the graph to query (ontology + instance data), as Turtle.
/// * `bindings` — values for the query's parameters (e.g. `("focus", TermValue::iri(..))`),
///   pre-bound via [`SparqlRequest::substitutions`] — never interpolated into the text.
///
/// # Errors
///
/// - `name` is not a known TQL query.
/// - A declared parameter is missing from `bindings` (mirrors the Python `ValueError`).
/// - The events Turtle fails to parse, or the query does not evaluate to a SELECT.
pub fn run_temporal_query(
    query_dir: &Path,
    name: &str,
    source_ttl: &str,
    bindings: &[(String, TermValue)],
) -> Result<Solutions, gmeow_errors::Diag> {
    let spec = temporal_queries().get(name).cloned().ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "temporal-query".to_string(),
            message: format!("unknown temporal query {name:?}"),
        })
    })?;

    // Every declared parameter must be supplied — a missing parameter is a HARD FAIL,
    // never a silently-unbound variable (mirrors the Python `ValueError`).
    let missing: Vec<&str> = spec
        .parameters
        .iter()
        .copied()
        .filter(|p| !bindings.iter().any(|(name, _)| name == p))
        .collect();
    if !missing.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "temporal-query".to_string(),
            message: format!(
                "temporal query {name:?} needs parameter(s) {}",
                missing.join(", ")
            ),
        }));
    }

    let text = query_text(query_dir, name)?;
    let dataset = dataset_from_turtle(source_ttl.as_bytes(), "temporal events graph")?;
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            &dataset,
            SparqlRequest {
                query: &text,
                base_iri: None,
                substitutions: bindings,
            },
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "temporal-query".to_string(),
                message: format!("temporal query {name:?} evaluation failed: {e}"),
            })
        })?;

    match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Ok(Solutions { variables, rows }),
        SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
            Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "temporal-query".to_string(),
                message: format!("temporal query {name:?} must be a SELECT"),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::TermValue;

    /// The committed TQL query directory in the worktree.
    fn tql_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("slices/core/temporal/queries/tql")
    }

    const EVENTS_TTL: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<urn:e1> rdf:type gmeow:Event ;
    gmeow:eventTime "2020-01-01T00:00:00Z"^^xsd:dateTime ;
    gmeow:before <urn:e2> .
<urn:e2> rdf:type gmeow:Event ;
    gmeow:eventTime "2021-01-01T00:00:00Z"^^xsd:dateTime .
"#;

    #[test]
    fn registry_matches_the_python_surface() {
        let reg = temporal_queries();
        assert_eq!(reg.len(), 9, "nine registered TQL queries");
        assert_eq!(reg["before-event"].parameters, &["focus"]);
        assert_eq!(
            reg["overlapping-window"].parameters,
            &["windowStart", "windowEnd"]
        );
        assert!(reg["timeline"].parameters.is_empty());
    }

    #[test]
    fn timeline_runs_over_a_tiny_events_graph() {
        let sols =
            run_temporal_query(&tql_dir(), "timeline", EVENTS_TTL, &[]).expect("timeline runs");
        // Two events, each with an effective start, ordered.
        let event_col = sols.col("event").expect("event column");
        let events: Vec<String> = sols
            .rows
            .iter()
            .filter_map(|row| match &row[event_col] {
                Some(TermValue::Iri(iri)) => Some(iri.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(events, vec!["urn:e1".to_string(), "urn:e2".to_string()]);
    }

    #[test]
    fn before_event_binds_the_focus_parameter() {
        let bindings = vec![("focus".to_string(), TermValue::iri("urn:e2"))];
        let sols = run_temporal_query(&tql_dir(), "before-event", EVENTS_TTL, &bindings)
            .expect("before-event runs");
        let event_col = sols.col("event").expect("event column");
        let events: Vec<String> = sols
            .rows
            .iter()
            .filter_map(|row| match &row[event_col] {
                Some(TermValue::Iri(iri)) => Some(iri.clone()),
                _ => None,
            })
            .collect();
        // e1 is before the focus e2 (Allen closure via gmeow:before+).
        assert_eq!(events, vec!["urn:e1".to_string()]);
    }

    #[test]
    fn missing_parameter_is_a_hard_fail() {
        let err = run_temporal_query(&tql_dir(), "before-event", EVENTS_TTL, &[])
            .expect_err("missing focus must fail");
        assert!(
            err.to_string().contains("needs parameter(s) focus"),
            "clear missing-parameter message, got: {err}"
        );
    }

    #[test]
    fn unknown_query_name_is_rejected() {
        let err = run_temporal_query(&tql_dir(), "no-such-query", EVENTS_TTL, &[])
            .expect_err("unknown name must fail");
        assert!(err.to_string().contains("unknown temporal query"));
    }
}
