// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for `tests/WITNESS.mcp.json`.
//!
//! Native and wasm parity tests consume the committed attestation read-only. When a
//! reviewed engine-identity change moves the deterministic response, this producer is the
//! only path that may refresh it; no test contains a blessing or write path.

use std::path::{Path, PathBuf};

use gmeow_mcp_wasm::{init, mcp, ready};
use serde_json::Value;

/// Byte-identical to the request carried by both parity consumers.
const REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conjecture_test","#,
    r#""arguments":{"formula":"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n"#,
    r#"@prefix ex: <http://ex/> .\n"#,
    r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"#,
    r#"ex:phi a logic:Formula ;\n"#,
    r#"    logic:relation rdf:type ;\n"#,
    r#"    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n"#,
    r#"    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n","#,
    r#""kb":"@prefix ex: <http://ex/> .\n"#,
    r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"#,
    r#"ex:a rdf:type ex:B .\n","#,
    r#""standpoint":"https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint"}}}"#,
);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
    std::io::Error::other(message.into()).into()
}

fn validate_answer(frame: &str) -> Result<(), Box<dyn std::error::Error>> {
    let envelope: Value = serde_json::from_str(frame)?;
    if envelope["jsonrpc"] != "2.0"
        || envelope["id"] != 1
        || envelope["result"]["isError"] != Value::Bool(false)
    {
        return Err(fail(format!(
            "producer received a failed JSON-RPC frame: {frame}"
        )));
    }
    let text = envelope["result"]["content"][0]["text"]
        .as_str()
        .ok_or_else(|| fail("producer response has no text payload"))?;
    let payload: Value = serde_json::from_str(text)?;
    if payload["ok"] != Value::Bool(true)
        || payload["verdict"]["lifecycle"] != "corroborated"
        || payload["verdict"]["evaluation"] != "completed"
        || text.contains("mcp.segment-not-loaded")
    {
        return Err(fail(format!(
            "producer response did not execute the corroborated reasoning path: {frame}"
        )));
    }
    let judgment = payload["judgment_nquads"]
        .as_str()
        .ok_or_else(|| fail("producer response has no judgment_nquads attestation"))?;
    if judgment
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(fail(
            "producer response contains a raw control scalar in judgment_nquads",
        ));
    }
    purrdf::parse_dataset(judgment.as_bytes(), "application/n-quads", None)
        .map_err(|error| fail(format!("producer judgment_nquads is invalid RDF: {error}")))?;
    Ok(())
}

fn write_exact(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, bytes)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let bundle_path = root.join("generated/dist/gmeow.gts");
    let bundle = std::fs::read(&bundle_path)?;
    if ready() {
        return Err(fail(
            "MCP witness producer started with an initialized engine",
        ));
    }
    init(&bundle).map_err(|error| fail(format!("initialize MCP witness engine: {error:?}")))?;
    let frame = mcp(REQUEST).map_err(|error| fail(format!("produce MCP witness: {error:?}")))?;
    let repeated = mcp(REQUEST).map_err(|error| fail(format!("repeat MCP witness: {error:?}")))?;
    if frame != repeated {
        return Err(fail("MCP witness response is not deterministic"));
    }
    validate_answer(&frame)?;

    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/WITNESS.mcp.json");
    write_exact(&output, frame.as_bytes())?;
    println!(
        "refreshed {} from {} ({} bundle bytes, {} witness bytes)",
        output.display(),
        bundle_path.display(),
        bundle.len(),
        frame.len()
    );
    Ok(())
}
