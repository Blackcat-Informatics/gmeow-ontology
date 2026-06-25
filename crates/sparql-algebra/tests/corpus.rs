// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Corpus conformance: every hand-authored `queries/**/*.rq` and every committed
//! DSL-generated `generated/queries/*.rq` MUST parse into the algebra.
//!
//! This is the acceptance gate for purrdf S5 (#911): parse our 90 `.rq` files
//! plus the DSL-generated projections. Both trees are tracked in git, so reading
//! them directly pins the test to committed artifacts (no regeneration needed).
//!
//! One corpus file (`accessibility-competency.rq`) concatenates three queries
//! that share a leading `PREFIX` prologue; [`split_queries`] splits such files so
//! each query is parsed independently with the shared prologue.

use std::path::{Path, PathBuf};

use gmeow_sparql_algebra::SparqlParser;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect_rq(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rq(&path, out);
        } else if path.extension().map(|x| x == "rq").unwrap_or(false) {
            out.push(path);
        }
    }
    out.sort();
}

/// Split a possibly-multi-query `.rq` file into individual queries, each carrying
/// the shared leading prologue (comments + `PREFIX`/`BASE` before the first form).
fn split_queries(text: &str) -> Vec<String> {
    let mut prologue = String::new();
    let mut bodies: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    for line in text.lines() {
        let t = line.trim_start();
        let is_form = !t.starts_with('#')
            && ["SELECT", "CONSTRUCT", "ASK", "DESCRIBE"].iter().any(|k| {
                let up = t.to_ascii_uppercase();
                up.starts_with(k) && up[k.len()..].starts_with([' ', '\t', '*', '{', '?'])
            });
        if is_form {
            if started && !cur.trim().is_empty() {
                bodies.push(std::mem::take(&mut cur));
            }
            started = true;
        }
        if started {
            cur.push_str(line);
            cur.push('\n');
        } else {
            prologue.push_str(line);
            prologue.push('\n');
        }
    }
    if !cur.trim().is_empty() {
        bodies.push(cur);
    }
    if bodies.len() <= 1 {
        return vec![text.to_owned()];
    }
    bodies
        .into_iter()
        .map(|b| format!("{prologue}\n{b}"))
        .collect()
}

fn assert_all_parse(dir: &Path, label: &str) -> usize {
    let mut files = Vec::new();
    collect_rq(dir, &mut files);
    assert!(
        !files.is_empty(),
        "no .rq files found under {}",
        dir.display()
    );
    let mut queries = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        for q in split_queries(&text) {
            queries += 1;
            if let Err(e) = SparqlParser::new().parse_query(&q) {
                failures.push(format!("{}: {e}", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{label}: {} of {queries} queries failed to parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
    queries
}

#[test]
fn all_hand_authored_queries_parse() {
    let n = assert_all_parse(&repo_root().join("queries"), "queries/");
    // 90 files; one file holds 3 queries → 92 individual queries.
    assert!(n >= 90, "expected >= 90 corpus queries, parsed {n}");
}

#[test]
fn all_generated_projections_parse() {
    let dir = repo_root().join("generated/queries");
    if !dir.exists() {
        // Generated artifacts are tracked, but tolerate a stripped checkout.
        eprintln!("generated/queries absent — skipping");
        return;
    }
    let n = assert_all_parse(&dir, "generated/queries/");
    assert!(n >= 40, "expected the DSL projection set, parsed {n}");
}
