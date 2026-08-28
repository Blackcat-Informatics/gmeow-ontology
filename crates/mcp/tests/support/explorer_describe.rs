// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared, verified computation behind the explorer describe witness and its producer.

use std::path::PathBuf;

use purrdf::{DatasetView, GraphMatch, TermRef};
use serde_json::{Value, json};

pub struct DescribeWitness {
    pub subject: String,
    pub rendered: String,
}

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

pub fn attestation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/witness/describe.nt")
}

/// Render every default-graph quad with `subject_iri` as sorted, deduplicated N-Triples.
fn describe(dataset: &purrdf::RdfDataset, subject_iri: &str) -> String {
    let term = |term: TermRef<'_>| -> String {
        match term {
            TermRef::Iri(iri) => format!("<{iri}>"),
            TermRef::Blank { label, .. } => format!("_:{label}"),
            TermRef::Literal { lexical, .. } => {
                format!("\"{}\"", lexical.replace('"', "\\\""))
            }
            TermRef::Triple { .. } => "<<triple>>".to_owned(),
        }
    };
    let mut lines: Vec<String> = dataset
        .quads_for_pattern(None, None, None, GraphMatch::Default)
        .filter(|quad| matches!(dataset.resolve(quad.s), TermRef::Iri(iri) if iri == subject_iri))
        .map(|quad| {
            format!(
                "{} {} {} .",
                term(dataset.resolve(quad.s)),
                term(dataset.resolve(quad.p)),
                term(dataset.resolve(quad.o))
            )
        })
        .collect();
    lines.sort();
    lines.dedup();
    lines.join("\n")
}

fn describe_query(subject_iri: &str) -> String {
    format!("CONSTRUCT {{ <{subject_iri}> ?p ?o }} WHERE {{ <{subject_iri}> ?p ?o }}")
}

fn query_describe(
    server: &gmeow_mcp::McpServer,
    subject: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let envelope = server.call_tool_result(
        "query_local",
        &json!({
            "data": "",
            "format": "turtle",
            "scope": "bundle",
            "query": describe_query(subject),
        }),
    );
    let text = envelope["content"][0]["text"]
        .as_str()
        .ok_or_else(|| fail(format!("query_local returned no text payload: {envelope}")))?;
    let payload: Value = serde_json::from_str(text)?;
    if payload["ok"] != Value::Bool(true) || payload["form"] != "graph" {
        return Err(fail(format!(
            "query_local did not return a successful graph: {payload}"
        )));
    }
    let returned = payload["graph_nquads"]
        .as_str()
        .ok_or_else(|| fail(format!("query_local graph has no N-Quads body: {payload}")))?;
    let dataset = purrdf::parse_dataset(returned.as_bytes(), "application/n-quads", None)?;
    Ok(describe(&dataset, subject))
}

/// Compute the native object-level description and the shipped query route over one
/// caller-selected snapshot, require byte identity and repeat determinism, then return
/// the exact attestation text. Tests supply authenticated producer artifacts; the
/// explicit maintainer producer supplies a freshly folded bundle directly.
pub fn verified_describe(
    snapshot: &[u8],
    core: &purrdf::RdfDataset,
) -> Result<DescribeWitness, Box<dyn std::error::Error>> {
    let namespace = "https://blackcatinformatics.ca/gmeow/";
    let subject = core
        .quads_for_pattern(None, None, None, GraphMatch::Default)
        .filter_map(|quad| match core.resolve(quad.s) {
            TermRef::Iri(iri) if iri.starts_with(namespace) => Some(iri.to_owned()),
            _ => None,
        })
        .min()
        .ok_or_else(|| fail("core bundle carries no GMEOW-namespace subject"))?;
    let native = describe(core, &subject);
    if native.is_empty() {
        return Err(fail(format!("the native describe of {subject} is empty")));
    }

    let server = gmeow_mcp::McpServer::from_snapshot(snapshot)
        .map_err(|error| fail(format!("boot MCP server: {}", error.message())))?;
    let shipped = query_describe(&server, &subject)?;
    let repeated = query_describe(&server, &subject)?;
    if shipped != repeated {
        return Err(fail("the shipped explorer describe is not deterministic"));
    }
    if native != shipped {
        return Err(fail(format!(
            "explorer query describe drifted from the native object-level projection\n\
             native:\n{native}\nshipped:\n{shipped}"
        )));
    }

    Ok(DescribeWitness {
        rendered: format!("# describe <{subject}>\n{native}\n"),
        subject,
    })
}
