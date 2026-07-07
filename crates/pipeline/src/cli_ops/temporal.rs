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

    // ── TQL behavioural twins over the events worked-example fixtures ──────────
    //
    // These migrate the Python `test_temporal_query.py` cases that ran each TQL
    // query over `load_merged_graph(include_imports=False) + <fixture>.ttl` and
    // asserted the temporal answers. The Rust twin builds the same source: the
    // merged ontology (needed so the `rdfs:subClassOf*` type test reaches
    // gmeow:LifeEvent occurrences) concatenated with the coverage fixture, passed
    // to `run_temporal_query` as Turtle.

    /// The `ex:` namespace of the events worked example.
    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/events/";
    /// `xsd:dateTime` — the datatype the window/clock parameters carry.
    const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

    /// An `ex:` IRI in the events example namespace.
    fn ex(local: &str) -> String {
        format!("{EX}{local}")
    }

    /// The repository root (`crates/pipeline/../..`).
    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    /// Collect every `module.ttl` under `slices/`, recursively.
    fn collect_module_ttls(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                let candidate = path.join("module.ttl");
                if candidate.is_file() {
                    out.push(candidate);
                }
                collect_module_ttls(&path, out);
            }
        }
    }

    /// The TQL query source: the merged ontology (every `slices/**/module.ttl`)
    /// concatenated with the coverage fixture — the native twin of
    /// `load_merged_graph(include_imports=False) + <fixture>`.
    fn events_source(fixture: &str) -> String {
        let root = repo_root();
        let mut modules = Vec::new();
        collect_module_ttls(&root.join("slices"), &mut modules);
        modules.sort();
        let mut buf = String::new();
        for path in &modules {
            buf.push_str(
                &std::fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
            );
            buf.push('\n');
        }
        let fixture_path = root.join("tests/fixtures/coverage").join(fixture);
        buf.push_str(
            &std::fs::read_to_string(&fixture_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", fixture_path.display())),
        );
        buf
    }

    /// The IRI carried at column `idx` of `row`, or `None` if unbound / not an IRI.
    fn iri_at(row: &[Option<TermValue>], idx: usize) -> Option<String> {
        match row.get(idx) {
            Some(Some(TermValue::Iri(iri))) => Some(iri.clone()),
            _ => None,
        }
    }

    /// The IRI values of column `var` across every result row.
    fn iri_column(sols: &Solutions, var: &str) -> Vec<String> {
        let col = sols
            .col(var)
            .unwrap_or_else(|| panic!("column {var:?} present"));
        sols.rows
            .iter()
            .filter_map(|row| iri_at(row, col))
            .collect()
    }

    #[test]
    fn allen_closure_is_transitive() {
        let src = events_source("events.ttl");
        let sols =
            run_temporal_query(&tql_dir(), "allen-closure", &src, &[]).expect("allen-closure runs");
        let (early, late) = (
            sols.col("earlier").expect("earlier column"),
            sols.col("later").expect("later column"),
        );
        let pairs: std::collections::BTreeSet<(String, String)> = sols
            .rows
            .iter()
            .filter_map(|row| Some((iri_at(row, early)?, iri_at(row, late)?)))
            .collect();
        // The asserted chain dawn→noon→dusk plus the ENTAILED transitive edge.
        assert!(pairs.contains(&(ex("dawn"), ex("noon"))));
        assert!(pairs.contains(&(ex("noon"), ex("dusk"))));
        assert!(
            pairs.contains(&(ex("dawn"), ex("dusk"))),
            "the transitive dawn→dusk edge must be computed by the property path"
        );
    }

    #[test]
    fn before_event_reaches_lifeevents_and_orders_by_time() {
        let src = events_source("events.ttl");
        let bindings = vec![("focus".to_string(), TermValue::iri(ex("reception")))];
        let sols = run_temporal_query(&tql_dir(), "before-event", &src, &bindings)
            .expect("before-event runs");
        let events = iri_column(&sols, "event");
        // alexBirth is a gmeow:LifeEvent, reached only via rdfs:subClassOf* (merged ontology).
        assert!(
            events.contains(&ex("alexBirth")),
            "LifeEvent birth must be reached"
        );
        assert!(
            events.contains(&ex("wedding")),
            "the crisp interval event qualifies"
        );
        assert!(events.contains(&ex("siege")), "the fuzzy event qualifies");
        assert!(
            !events.contains(&ex("standup1")),
            "a 2024 event is after the 2015 reception and must be excluded"
        );
    }

    #[test]
    fn during_event_follows_relation_and_inverse() {
        let src = events_source("events.ttl");
        let bindings = vec![("focus".to_string(), TermValue::iri(ex("conference")))];
        let sols = run_temporal_query(&tql_dir(), "during-event", &src, &bindings)
            .expect("during-event runs");
        let events = iri_column(&sols, "event");
        // talk is a directly-asserted during; keynote via the inverse of an asserted contains.
        assert!(events.contains(&ex("talk")), "asserted during edge");
        assert!(events.contains(&ex("keynote")), "inverse-of-contains edge");
    }

    #[test]
    fn timeline_orders_all_events_by_effective_start() {
        let src = events_source("events.ttl");
        let sols = run_temporal_query(&tql_dir(), "timeline", &src, &[]).expect("timeline runs");
        let ordered = iri_column(&sols, "event");
        let siege = ordered.iter().position(|e| e == &ex("siege"));
        let standup2 = ordered.iter().position(|e| e == &ex("standup2"));
        assert!(
            siege.is_some() && standup2.is_some(),
            "both events on the timeline"
        );
        // The fuzzy 1453 siege precedes the 2024 standup.
        assert!(
            siege < standup2,
            "siege (1453) must order before standup2 (2024)"
        );
    }

    #[test]
    fn overlapping_window_matches_crisp_point_and_fuzzy() {
        let src = events_source("events.ttl");
        let bindings = vec![
            (
                "windowStart".to_string(),
                TermValue::typed_literal("2015-06-20T00:00:00Z", XSD_DATE_TIME),
            ),
            (
                "windowEnd".to_string(),
                TermValue::typed_literal("2015-06-20T23:59:59Z", XSD_DATE_TIME),
            ),
        ];
        let sols = run_temporal_query(&tql_dir(), "overlapping-window", &src, &bindings)
            .expect("overlapping-window runs");
        let events = iri_column(&sols, "event");
        // The interval wedding + the point reception, both on that day.
        assert!(
            events.contains(&ex("wedding")),
            "the interval event on the window day"
        );
        assert!(
            events.contains(&ex("reception")),
            "the point event on the window day"
        );
        assert!(
            !events.contains(&ex("standup1")),
            "a 2024 event does not overlap 2015"
        );
    }

    #[test]
    fn bitemporal_four_clocks_returns_standpoint_indexed_claims() {
        let src = events_source("events-contested.ttl");
        let bindings = vec![
            (
                "validAt".to_string(),
                TermValue::typed_literal("1895-01-01T00:00:00Z", XSD_DATE_TIME),
            ),
            (
                "asOf".to_string(),
                TermValue::typed_literal("2020-01-01T00:00:00Z", XSD_DATE_TIME),
            ),
        ];
        let sols =
            run_temporal_query(&tql_dir(), "bitemporal", &src, &bindings).expect("bitemporal runs");
        let subjects: std::collections::BTreeSet<String> =
            iri_column(&sols, "subject").into_iter().collect();
        let standpoints: std::collections::BTreeSet<String> =
            iri_column(&sols, "standpoint").into_iter().collect();
        assert!(
            subjects.contains(&ex("disputedEvent")),
            "the disputed event's standpoint-indexed claims are returned"
        );
        // Both asserting standpoints coexist — no single winner.
        assert!(
            standpoints.contains(&ex("standpoint-A")) && standpoints.contains(&ex("standpoint-B")),
            "coexisting standpoints A and B must both appear; got {standpoints:?}"
        );
    }
}
