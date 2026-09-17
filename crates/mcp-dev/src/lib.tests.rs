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

/// The DEV surface is the 38 consumer tools plus exactly 4, and the 5 consumer
/// resources plus exactly 1. The dev tools are advertised AFTER the builtins.
#[test]
fn dev_surface_is_forty_two_tools_and_seven_resources() {
    let server = dev_server(&snapshot(), repo_root()).expect("dev server constructs");
    let names = server.surface().tool_names();
    assert_eq!(
        names.len(),
        42,
        "the dev tool surface is the 38 consumer tools + 4, got {names:?}"
    );
    assert_eq!(
        &names[38..],
        ["validate", "reason", "sync", "constitution"],
        "the four dev tools are advertised after the consumer builtins"
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
    for dev_only in ["validate", "reason", "sync", "constitution"] {
        let err = server
            .surface()
            .dispatch_tool(&server, dev_only, &json!({}))
            .expect_err("a dev tool must not dispatch on a consumer server");
        assert_eq!(err.code(), gmeow_mcp::error::UnknownTool::register());
        assert!(err.to_string().contains(dev_only), "{err}");
    }
}
