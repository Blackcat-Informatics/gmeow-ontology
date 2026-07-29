// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Every SPARQL query the site SHIPS is executed here, against the real bundle, through the
//! real engine — and must return a non-empty result.
//!
//! # Why this exists
//!
//! The site used to ship four runnable queries that all returned nothing:
//!
//! * the playground's own prefilled textarea (`SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 20`);
//! * every term page's "Describe this term in the SPARQL playground" `?q=` link;
//! * every slice page's equivalent;
//! * the playground's worked "explain a chase-invented witness" example.
//!
//! All four were graph-less patterns, and the asset they ran against
//! (`assets/playground.trig`) routed EVERY statement into a named graph — so each matched an
//! empty default graph. Nothing caught it, because every gate in the build asked structural
//! questions ("is the asset emitted?", "is it byte-deterministic?", "does the page contain
//! the link?") and none asked the one question that matters: does it answer?
//!
//! This test asks that question. A query added to a page and not to
//! [`gmeow_docs::render::shipped_queries`] is a query nobody proves answers, so the
//! enumeration is deliberately the same one the pages render from.
//!
//! # Why an empty result is a FAILURE and not a valid answer
//!
//! In general an empty SPARQL result is a legitimate answer. Not here: each of these is
//! shipped TO A READER as a demonstration, prefilled or one click away. A demonstration that
//! returns nothing does not teach that the ontology lacks the data — it teaches that the
//! surface is broken. So emptiness is the failure condition for exactly this set.

use serde_json::json;

mod common;

// The repo root is `common::repo_root()` — the SAME anchor every other gmeow-docs
// integration binary uses. A second local copy here could drift from it silently.
use common::repo_root;

/// What the engine answered for one shipped query.
///
/// A TOTAL enum rather than a `Result`: a refusal is one of the outcomes this lane reports
/// on, not an error it propagates, so every query still gets run and every verdict still
/// gets collected. (It is also the shape the Diag-substrate invariant wants — a bare
/// `String` error type is banned repo-wide, and inventing a `Diag` kind to carry a message
/// this test only ever prints would be ceremony.)
enum Answer {
    Bindings(usize),
    Graph(u64),
    Boolean(bool),
    /// The engine declined: its own message, verbatim.
    Refused(String),
}

impl Answer {
    /// Whether the reader sees something. See the module docs on why emptiness fails here.
    fn is_non_empty(&self) -> bool {
        match self {
            Answer::Bindings(rows) => *rows > 0,
            Answer::Graph(quads) => *quads > 0,
            Answer::Boolean(value) => *value,
            Answer::Refused(_) => false,
        }
    }

    fn describe(&self) -> String {
        match self {
            Answer::Bindings(rows) => format!("{rows} row(s)"),
            Answer::Graph(quads) => format!("{quads} quad(s)"),
            Answer::Boolean(value) => format!("boolean {value}"),
            Answer::Refused(message) => format!("REFUSED — {message}"),
        }
    }
}

/// Run `sparql` exactly as the browser does: `query_local`, `scope: "bundle"`, empty
/// overlay — the frame `queryBundle` in `assets/mcp-transport.mjs` builds.
fn run(server: &gmeow_mcp::McpServer, sparql: &str) -> Answer {
    let envelope = server.call_tool_result(
        "query_local",
        &json!({
            "data": "",
            "format": "turtle",
            "scope": "bundle",
            "query": sparql,
        }),
    );
    let Some(text) = envelope["content"][0]["text"].as_str() else {
        return Answer::Refused("the tool envelope carried no text content".to_owned());
    };
    let payload: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) => return Answer::Refused(format!("the tool payload is not JSON: {e}")),
    };
    if payload["ok"] != true {
        return Answer::Refused(
            payload["error"]
                .as_str()
                .unwrap_or("query_local refused without a message")
                .to_owned(),
        );
    }
    match payload["form"].as_str() {
        Some("bindings") => Answer::Bindings(
            payload["results"]["bindings"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0),
        ),
        Some("graph") => Answer::Graph(payload["quad_count"].as_u64().unwrap_or(0)),
        Some("boolean") => Answer::Boolean(payload["boolean"] == true),
        other => Answer::Refused(format!("unexpected result form {other:?}")),
    }
}

#[test]
fn every_shipped_query_executes_and_returns_a_non_empty_result() {
    let root = repo_root();
    let bytes = std::fs::read(root.join("generated/dist/gmeow.gts"))
        .unwrap_or_else(|e| panic!("this lane needs the generated bundle (run `make regen`): {e}"));
    let server = gmeow_mcp::McpServer::from_snapshot(&bytes).expect("boot the MCP server");

    let model = common::cached_model();
    let queries = gmeow_docs::render::shipped_queries(&model);
    assert!(
        queries.len() >= 4,
        "the shipped-query enumeration collapsed to {} entries — the playground default, \
         the witness example and both export prefills must all be covered",
        queries.len()
    );

    // Every query is run and every failure collected, so one run names EVERY broken surface
    // rather than stopping at the first. That matters here: the four dead queries all shared
    // one root cause, and a fail-fast lane would have reported them one release at a time.
    let mut failures: Vec<String> = Vec::new();
    for (label, sparql) in &queries {
        let answer = run(&server, sparql);
        if answer.is_non_empty() {
            eprintln!("shipped query OK — {label}: {}", answer.describe());
        } else {
            failures.push(format!(
                "{label}: {} — a shipped demonstration that answers nothing reads as a broken \
                 surface, not as an honest negative\n    query: {sparql}",
                answer.describe()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} shipped queries do not answer:\n{}",
        failures.len(),
        queries.len(),
        failures.join("\n")
    );
}
