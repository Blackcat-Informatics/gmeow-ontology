// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `distribution_matrix` serves the catalog the CLI serves, out of the same graph.
//!
//! `read_distribution_matrix` used to live inside `gmeow-pipeline`, with exactly one
//! consumer (`gmeow docs matrix`), which meant an agent could not ask what documentation
//! surfaces the bundle ships without dragging in the build executor. It now lives in
//! `gmeow-docs-catalog`, a wasm-clean leaf, and `gmeow-pipeline` re-exports it at the
//! historical path so `commands.rs` is unchanged.
//!
//! What this file pins:
//!
//! * the tool's `distributions` rows ARE `gmeow_docs_catalog::read_distribution_matrix`
//!   over the same bundle — the reader `gmeow docs matrix` prints, not a second one;
//! * the `gmeow:DocumentationDistribution` filter still selects exactly the eight declared
//!   distributions, which is the property the move most easily could have broken;
//! * `concepts` is a SEPARATE reader with its own row shape, and an empty lattice is a
//!   valid answer rather than a failure.

use std::path::{Path, PathBuf};

use gmeow_mcp::McpServer;
use serde_json::{Value, json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mcp has a workspace root two levels up")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn snapshot() -> Vec<u8> {
    std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("read committed snapshot")
}

fn payload(server: &McpServer) -> Value {
    let envelope = server.call_tool_result("distribution_matrix", &json!({}));
    let text = envelope["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("distribution_matrix returned no text: {envelope}"));
    let parsed: Value = serde_json::from_str(text).expect("tool text is JSON");
    assert_eq!(
        envelope.get("isError"),
        Some(&json!(false)),
        "distribution_matrix hard-failed: {parsed}"
    );
    assert_eq!(parsed["ok"], json!(true), "{parsed}");
    parsed
}

/// The tool's rows are the catalog reader's rows, field for field and in the same order —
/// so an agent and `gmeow docs matrix` are reading one table.
#[test]
fn the_tool_rows_are_the_shared_catalog_readers_rows() {
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
    let direct = gmeow_docs_catalog::read_distribution_matrix(&bytes)
        .expect("the shipped bundle carries a distribution catalog");

    let served = payload(&server);
    let rows = served["distributions"]
        .as_array()
        .expect("distributions is an array");
    assert_eq!(
        rows.len(),
        direct.len(),
        "the tool and the shared reader must return the same number of rows"
    );
    for (served_row, direct_row) in rows.iter().zip(&direct) {
        assert_eq!(served_row["slug"], json!(direct_row.slug));
        assert_eq!(served_row["family"], json!(direct_row.family));
        assert_eq!(served_row["media_type"], json!(direct_row.media_type));
        assert_eq!(served_row["consumers"], json!(direct_row.consumers));
        assert_eq!(
            served_row["dropped_capabilities"],
            json!(direct_row.dropped_capabilities)
        );
    }
}

/// The `gmeow:DocumentationDistribution` filter is unchanged by the crate move: the matrix
/// is exactly the eight declared distributions, sorted by slug. This is the property that
/// would break first if the reader started selecting on a broader type — the same graph
/// also carries family, capability, loss and site-sub-asset subjects.
#[test]
fn the_matrix_is_exactly_the_eight_declared_distributions() {
    let server = McpServer::from_snapshot(&snapshot()).expect("consumer server constructs");
    let served = payload(&server);
    let slugs: Vec<&str> = served["distributions"]
        .as_array()
        .expect("distributions is an array")
        .iter()
        .map(|row| row["slug"].as_str().expect("slug"))
        .collect();
    assert_eq!(
        slugs,
        [
            "jsonld", "mdbook", "okf", "pdf", "pydantic", "site", "snippets", "yamlld"
        ],
        "the matrix must carry exactly the eight declared distributions, sorted by slug"
    );

    // Two spot-checks that the FACETS survived the move, not just the row count.
    let rows = served["distributions"].as_array().expect("array");
    let site = rows
        .iter()
        .find(|row| row["slug"] == json!("site"))
        .expect("site row");
    assert_eq!(site["family"], json!("doc-render"));
    assert_eq!(site["media_type"], json!("text/html"));
    assert_eq!(site["consumers"], json!(["consumerPublicSite"]));

    let okf = rows
        .iter()
        .find(|row| row["slug"] == json!("okf"))
        .expect("okf row");
    assert_eq!(okf["family"], json!("serialization"));
    assert_eq!(
        okf["dropped_capabilities"],
        json!([]),
        "the serialization family declares no loss"
    );
}

/// `concepts` is its own reader over the same graph, with its own row shape (`concept`,
/// `extent`, `intent`). An EMPTY list is the honest reading of a catalog that declares no
/// lattice — the emitter is a separate producer — so this asserts the shape and the
/// agreement with the shared reader, and deliberately does NOT gate on non-emptiness.
#[test]
fn the_concept_rows_are_the_shared_lattice_readers_rows() {
    let bytes = snapshot();
    let server = McpServer::from_snapshot(&bytes).expect("consumer server constructs");
    let direct = gmeow_docs_catalog::read_concept_lattice(&bytes)
        .expect("reading the lattice out of a catalog-bearing bundle must succeed");

    let served = payload(&server);
    let rows = served["concepts"].as_array().expect("concepts is an array");
    assert_eq!(rows.len(), direct.len());
    for (served_row, direct_row) in rows.iter().zip(&direct) {
        assert_eq!(served_row["concept"], json!(direct_row.concept));
        assert_eq!(served_row["extent"], json!(direct_row.extent));
        assert_eq!(served_row["intent"], json!(direct_row.intent));
    }

    // A concept is not a distribution: the two row shapes are disjoint, which is why they
    // are two readers rather than one with an optional field.
    for row in rows {
        assert!(row.get("slug").is_none(), "a concept has no slug: {row}");
        assert!(row["extent"].is_array(), "{row}");
        assert!(row["intent"].is_array(), "{row}");
    }
}

/// The move kept ONE definition site: `gmeow-pipeline` re-exports the reader rather than
/// keeping a copy, and the CLI's call site is untouched.
#[test]
fn the_pipeline_re_exports_the_reader_rather_than_keeping_a_copy() {
    let docs_distribution = read("crates/pipeline/src/docs_distribution.rs");
    assert!(
        docs_distribution
            .contains("pub use gmeow_docs_catalog::{DistributionRow, read_distribution_matrix};"),
        "gmeow-pipeline must RE-EXPORT the catalog reader at its historical path"
    );
    assert!(
        !docs_distribution.contains("pub fn read_distribution_matrix"),
        "gmeow-pipeline must not keep a second definition of the catalog reader"
    );
    assert!(
        !docs_distribution.contains("pub struct DistributionRow"),
        "gmeow-pipeline must not keep a second definition of the catalog row type"
    );

    let commands = read("crates/gmeow-cli/src/commands.rs");
    assert!(
        commands
            .contains("gmeow_pipeline::docs_distribution::read_distribution_matrix(BUNDLE_GTS)"),
        "`gmeow docs matrix`'s call site must be unchanged by the move"
    );

    // The leaf's manifest must DECLARE none of these. Matched on the dependency-entry form
    // (`<name> = `) rather than as a bare substring, because the manifest's comments name
    // several of them precisely to explain why they are absent.
    let manifest = read("crates/docs-catalog/Cargo.toml");
    for banned in [
        "gmeow-pipeline",
        "gmeow-docs",
        "gmeow-conformance",
        "typst",
        "rayon",
    ] {
        assert!(
            !manifest.contains(&format!("\n{banned} = ")),
            "gmeow-docs-catalog is a wasm-clean LEAF: it must never depend on `{banned}`"
        );
    }
}
