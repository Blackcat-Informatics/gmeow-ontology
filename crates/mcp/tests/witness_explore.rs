// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The W2b bundle-explorer `describe` WITNESS (T1/F2) — now an EQUIVALENCE.
//!
//! The browser bundle explorer answers `describe <term>` over the object-level ontology.
//! It used to do that by parsing a 27 MB `gmeow-core.nq` re-serialization in a vendored
//! purrdf wasm engine; it now sends one `query_local` frame at the MCP engine already
//! booted over `gmeow.gts`. The vendored engine is retired, so an attestation that pinned
//! only the native purrdf path would no longer be attesting the shipped surface.
//!
//! This test therefore proves BOTH sides against ONE committed attestation
//! (`tests/witness/describe.nt`):
//!
//! 1. the NATIVE oracle — [`gmeow_validate::store::core_browser_bundle_nquads`] projects
//!    the bundle's default graph, and a deterministic renderer describes a deterministic
//!    subject out of it. This is the independent definition of "the object-level
//!    description of a term", derived without the MCP surface;
//! 2. the SHIPPED route — the exact `query_local` frame the docs controller sends
//!    (`describeQuery` in `crates/docs/assets/docs-controller.mjs`), rendered through the
//!    SAME renderer.
//!
//! Both must equal the committed bytes. That is strictly stronger than what the retired
//! witness proved: it pins the describe AND pins the two paths to each other, so the
//! explorer cannot drift from the projection it claims to describe.
//!
//! # Why a bound-subject CONSTRUCT and not `DESCRIBE`
//!
//! SPARQL leaves `DESCRIBE`'s result implementation-defined, and this engine's gathers
//! across every named graph: `DESCRIBE <AboutnessMode>` returns 38 quads, picking up the
//! documentation graph's `addedInVersion` / `definitionDigest` / `inScheme` rows. A
//! bound-subject pattern reads the active (default) graph alone and returns the 11 the
//! object-level ontology asserts. The explorer means the second, so the query says the
//! second rather than depending on a DESCRIBE dialect.
//!
//! Refreshed with the bundle only by an explicit maintainer producer.

use std::path::PathBuf;

use purrdf::{DatasetView, GraphMatch, TermRef};
use serde_json::json;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/witness/describe.nt")
}

/// Render `subject`'s describe (every quad with it as subject) as sorted N-Triples — a
/// deterministic, engine-independent describe projection. Language tags are dropped and
/// the lines deduplicated, so the attestation is the LANGUAGE-INDEPENDENT description and
/// does not churn when a translation lands.
fn describe(dataset: &purrdf::RdfDataset, subject_iri: &str) -> String {
    let term = |t: TermRef<'_>| -> String {
        match t {
            TermRef::Iri(iri) => format!("<{iri}>"),
            TermRef::Blank { label, .. } => format!("_:{label}"),
            TermRef::Literal { lexical, .. } => format!("\"{}\"", lexical.replace('"', "\\\"")),
            TermRef::Triple { .. } => "<<triple>>".to_owned(),
        }
    };
    let mut lines: Vec<String> = dataset
        .quads_for_pattern(None, None, None, GraphMatch::Default)
        .filter(|q| matches!(dataset.resolve(q.s), TermRef::Iri(iri) if iri == subject_iri))
        .map(|q| {
            format!(
                "{} {} {} .",
                term(dataset.resolve(q.s)),
                term(dataset.resolve(q.p)),
                term(dataset.resolve(q.o))
            )
        })
        .collect();
    lines.sort();
    lines.dedup();
    lines.join("\n")
}

/// The describe query the docs controller sends, spelled once here as it is spelled once
/// there. A divergence between the two is the failure this test exists to catch, so the
/// text is kept in the same shape rather than being loosely "equivalent".
fn describe_query(subject_iri: &str) -> String {
    format!("CONSTRUCT {{ <{subject_iri}> ?p ?o }} WHERE {{ <{subject_iri}> ?p ?o }}")
}

#[test]
fn explorer_describe_is_the_same_on_both_routes_and_matches_the_attestation() {
    let root = repo_root();
    let full = gmeow_bundle_import::load_authenticated_source_bytes(&root)
        .expect("authenticated bundle; tests never produce it");

    // ── 1. The native oracle: the object-level projection, described directly ─────────
    let core = gmeow_bundle_import::load_authenticated_repository_bundle(&root)
        .expect("authenticated bundle dataset; tests never produce it")
        .dataset;

    // A deterministic subject: the lexicographically smallest GMEOW-namespace IRI that
    // appears in subject position (the same term the explorer would describe).
    let ns = "https://blackcatinformatics.ca/gmeow/";
    let mut subject: Option<String> = None;
    for q in core.quads_for_pattern(None, None, None, GraphMatch::Default) {
        if let TermRef::Iri(iri) = core.resolve(q.s)
            && iri.starts_with(ns)
            && subject.as_deref().map(|s| iri < s).unwrap_or(true)
        {
            subject = Some(iri.to_owned());
        }
    }
    let subject = subject.expect("core bundle carries a GMEOW-namespace subject");
    let native = describe(core.as_ref(), &subject);
    assert!(
        !native.is_empty(),
        "the describe of {subject} must be non-empty"
    );

    // ── 2. The shipped route: the controller's frame, through the real engine ─────────
    let server = gmeow_mcp::McpServer::from_snapshot(&full).expect("boot the MCP server");
    let envelope = server.call_tool_result(
        "query_local",
        &json!({
            "data": "",
            "format": "turtle",
            "scope": "bundle",
            "query": describe_query(&subject),
        }),
    );
    let payload: serde_json::Value = serde_json::from_str(
        envelope["content"][0]["text"]
            .as_str()
            .expect("the tool envelope carries text content"),
    )
    .expect("the tool payload is JSON");
    assert_eq!(payload["ok"], true, "query_local must answer: {payload}");
    assert_eq!(
        payload["form"], "graph",
        "a CONSTRUCT must come back as a graph, not bindings: {payload}"
    );
    let returned = payload["graph_nquads"]
        .as_str()
        .expect("a graph result carries graph_nquads");
    let via_query_local = describe(
        &purrdf::parse_dataset(returned.as_bytes(), "application/n-quads", None)
            .expect("parse the returned graph"),
        &subject,
    );

    assert_eq!(
        native, via_query_local,
        "the explorer's `query_local` describe drifted from the object-level projection it \
         claims to describe — the browser and the CLI would now answer differently"
    );

    // ── 3. Both against the committed attestation ────────────────────────────────────
    let rendered = format!("# describe <{subject}>\n{native}\n");
    let path = attestation_path();
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "describe witness attestation {} missing; refresh it through the explicit maintainer producer: {e}",
            path.display()
        )
    });
    assert_eq!(
        rendered, committed,
        "the object-level describe drifted from the committed witness attestation"
    );
}
