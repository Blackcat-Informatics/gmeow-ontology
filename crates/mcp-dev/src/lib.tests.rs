// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::Path;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn snapshot() -> Vec<u8> {
    gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated snapshot; tests never produce it")
}

/// The registered developer operations extend the consumer surface in order.
#[test]
fn dev_surface_extends_consumer_tools_and_resources() {
    let server = dev_server(&snapshot(), repo_root()).expect("dev server constructs");
    let names = server.surface().tool_names();
    assert_eq!(
        names.len(),
        gmeow_mcp::TOOL_COUNT + 5,
        "the dev tool surface adds exactly its registered operations, got {names:?}"
    );
    assert_eq!(
        &names[gmeow_mcp::TOOL_COUNT..],
        [
            "submit_packet_candidate",
            "validate",
            "reason",
            "sync",
            "constitution"
        ],
        "the dev tools are advertised after the consumer builtins"
    );
    let resources = server.surface().resource_descriptors();
    assert_eq!(
        resources.len(),
        7,
        "the dev resource surface is the 5 consumer resources + the medium registry \
             (which needs the build executor's reader, so it rides an extension) + the \
             Constitution"
    );
    assert_eq!(
        resources[5]["uri"], MEDIUM_URI,
        "the medium registry is advertised after the consumer builtins — the dev \
             extension is composed ON the medium one, so its resource lands first"
    );
    assert_eq!(
        resources[6]["uri"], CONSTITUTION_URI,
        "the Constitution resource is advertised last"
    );
}

/// The authentic policy and snapshot are supplied by the explicit producer.
/// This test authors only a tiny independent candidate slice, never a corpus.
#[test]
fn packet_candidate_composes_the_source_theory_with_the_governed_submission() {
    let snapshot = snapshot();
    let root = repo_root();
    let fixture = tempfile::Builder::new()
        .prefix("packet-candidate-")
        .tempdir_in(root.join(".cache"))
        .unwrap();
    std::fs::write(
        fixture.path().join("manifest.ttl"),
        "<urn:candidate:slice> a <https://blackcatinformatics.ca/gmeow/Slice> .",
    )
    .unwrap();
    std::fs::write(fixture.path().join("module.ttl"), r#"
        @prefix ex: <urn:candidate:> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
        ex:a a ex:B ; rdfs:isDefinedBy ex:slice ; rdfs:label "A" ; skos:definition "The candidate's subject." .
        ex:B rdfs:isDefinedBy ex:slice ; rdfs:label "B" ; skos:definition "The asserted class." .
    "#).unwrap();
    let formula = r#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <urn:candidate:> .
        ex:formula a logic:Formula ;
            logic:relation <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;
            logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .
    "#;
    let server = dev_server(&snapshot, root).unwrap();
    let result = server.call_tool_result(
        "submit_packet_candidate",
        &json!({
            "slice_dir":fixture.path().to_str().unwrap(), "formula":formula,
            "standpoint":"urn:candidate:standpoint", "dry_run":true,
        }),
    );
    assert_eq!(result["isError"], false, "{result}");
    let body: serde_json::Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["admissible"], true);
    assert_eq!(body["packet_context"]["slice"], "urn:candidate:slice");
    assert!(
        body["packet_context"]["packet"]
            .as_str()
            .unwrap()
            .starts_with("urn:candidate:slice/authoring-packet/")
    );
    assert_eq!(body["packet_context"]["kb_format"], "application/n-quads");
}

/// Every dev tool is DISPATCHABLE, not merely advertised — the registration seam
/// binds descriptor and handler together, and this proves the binding for the
/// two tools that are cheap to actually run (`constitution` reads a file;
/// `reason` reasons over the already-loaded carrier graph). `validate` / `sync`
/// drive a full pipeline run and are exercised by the gate, not from here.
#[test]
fn the_constitution_tool_and_resource_serve_the_same_checked_out_text() {
    let server = dev_server(&snapshot(), repo_root()).expect("dev server constructs");
    let expected =
        fs::read_to_string(repo_root().join("CONSTITUTION.md")).expect("read CONSTITUTION.md");

    let from_tool = server.call_tool_result("constitution", &json!({}));
    assert_eq!(from_tool["isError"], json!(false), "{from_tool}");
    assert_eq!(from_tool["content"][0]["text"], json!(expected));

    let from_resource = server.read_resource_result(CONSTITUTION_URI);
    assert!(from_resource.get("isError").is_none(), "{from_resource}");
    assert_eq!(from_resource["contents"][0]["text"], json!(expected));
    assert_eq!(
        from_resource["contents"][0]["mimeType"],
        json!("text/markdown")
    );
}

/// The `reason` tool runs the native reasoner over the bundle's carrier graph
/// and reports its four status axes.
#[test]
fn the_reason_tool_dispatches_and_reports_its_status_axes_heavy_offgate() {
    let server = dev_server(&snapshot(), repo_root()).expect("dev server constructs");
    let out = server.call_tool_result("reason", &json!({}));
    assert_eq!(out["isError"], json!(false), "{out}");
    let text = out["content"][0]["text"].as_str().expect("text content");
    let body: serde_json::Value = serde_json::from_str(text).expect("reason output is JSON");
    assert_eq!(body["ok"], json!(true), "{body}");
    for axis in ["input", "evaluation", "completeness", "information"] {
        assert!(body[axis].is_string(), "missing `{axis}` axis: {body}");
    }
}

/// A consumer server does NOT carry the dev tools: they are registered here, so
/// dispatching one without this extension is the seam's named refusal.
#[test]
fn the_dev_tools_are_absent_from_a_plain_consumer_server() {
    let server = McpServer::from_snapshot(&snapshot()).expect("consumer server constructs");
    for dev_only in [
        "submit_packet_candidate",
        "validate",
        "reason",
        "sync",
        "constitution",
    ] {
        let err = server
            .surface()
            .dispatch_tool(&server, dev_only, &json!({}))
            .expect_err("a dev tool must not dispatch on a consumer server");
        assert_eq!(err.code(), gmeow_mcp::error::UnknownTool::register());
        assert!(err.to_string().contains(dev_only), "{err}");
    }
}
