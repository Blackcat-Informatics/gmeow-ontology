// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let sols = run_temporal_query(&tql_dir(), "timeline", EVENTS_TTL, &[]).expect("timeline runs");
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
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        // A missing directory is benign (nothing to collect); any other error
        // (permissions, I/O) is a hard fail — never silently under-collect.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("read_dir {}: {e}", dir.display()),
    };
    for entry in read {
        let entry = entry.unwrap_or_else(|e| panic!("dir entry under {}: {e}", dir.display()));
        // `file_type()` reports the entry's own kind without following symlinks
        // (one syscall), so a symlink to a directory reads as a symlink — not a
        // dir — and recursion cannot loop through a circular link.
        let ft = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file_type {}: {e}", entry.path().display()));
        if ft.is_dir() {
            let path = entry.path();
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
    let sols =
        run_temporal_query(&tql_dir(), "before-event", &src, &bindings).expect("before-event runs");
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
    let sols =
        run_temporal_query(&tql_dir(), "during-event", &src, &bindings).expect("during-event runs");
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
