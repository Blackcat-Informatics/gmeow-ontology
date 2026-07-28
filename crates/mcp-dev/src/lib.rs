// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-mcp-dev` — the four repo-reading MCP developer tools.
//!
//! `gmeow-mcp` serves the consumer surface off a bundled `gmeow.gts` and nothing
//! else. `gmeow-dev mcp` serves that same surface PLUS four tools that need a
//! checkout: `validate` and `sync` (the pipeline's check/update runs),
//! `reason` (native reasoning over the bundle's carrier graph), and `constitution`
//! (the checked-out `CONSTITUTION.md`, also exposed as a fifth resource). Those four
//! live here.
//!
//! # Why they are a separate crate
//!
//! `validate` and `sync` ARE `gmeow_pipeline::run::run_full`. Leaving them inside
//! the MCP module made the MCP surface depend on the entire build executor — the
//! stage DAG, the scheduler, the persistent cache, rayon, the release signer, the
//! network client — which is exactly what a shippable, repo-free, eventually-wasm
//! `gmeow mcp` must not carry. Splitting them out inverts the edge: `gmeow-mcp` is a
//! leaf, and THIS crate (the only one that needs the executor) depends on both.
//!
//! `reason` has no pipeline coupling at all — it only calls
//! [`gmeow_logic::reason::reason_all`]. It stays dev-gated anyway. Promoting it to
//! the consumer surface would silently change the consumer tool list, and that list
//! is a contract other gates are defined against; a tool changes surface by an
//! explicit decision, never as a side effect of a refactor.
//!
//! # How they attach
//!
//! Through [`gmeow_mcp::extension`]: [`dev_extension`] returns an [`Extension`] of
//! `(descriptor, handler)` pairs, and [`dev_server`] hands it to
//! [`McpServer::from_snapshot_with`]. Each handler owns its own copy of the
//! repository root, so the root is state of the DEV TOOLS rather than a field on the
//! consumer server — there is no `root: Option<PathBuf>` to be `None` and no mode
//! flag to re-check at every call site. A name collision with a consumer tool, or a
//! dispatch of a tool this extension did not register, is a named hard error raised
//! by the seam.
//!
//! # Boundary rules
//!
//! * Everything here needs a checkout. A tool that does NOT need one belongs in
//!   `gmeow-mcp`, on the consumer surface.
//! * Nothing depends on this crate except a launcher that already has a checkout.
//!
//! # Direct dependencies
//!
//! The list below is the crate's complete direct dependency set — it must set-equal
//! `cargo tree -p gmeow-mcp-dev --depth 1 -e normal`, and the
//! `documented_dependencies` gate in `crates/mcp/tests/` asserts exactly that, naming
//! the symmetric difference in both directions when it drifts. Each entry carries the
//! reason it is here:
//!
//! * `gmeow-mcp` — the consumer MCP engine: the server, the bundle view, and the
//!   [`Extension`] seam these four tools register through. Every consumer tool is
//!   inherited unchanged.
//! * `gmeow-pipeline` — the build executor: `run::run_full` + `run::RunMode` are what
//!   the `validate` and `sync` tools ARE. This single edge is the whole reason the dev
//!   tools were split out of `gmeow-mcp`.
//! * `gmeow-logic` — the native reasoner: the `reason` tool runs
//!   [`gmeow_logic::reason::reason_all`] over the bundle's folded carrier graph.
//! * `gmeow-errors` — the diagnostic substrate: the dev tools raise typed `DiagKind`s
//!   as `Diag`s, on the same content-bound catalog as every other crate.
//! * `serde_json` — the MCP wire format: the tool descriptors and every result
//!   envelope.

pub mod error;

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use gmeow_mcp::extension::Extension;
use gmeow_mcp::{McpServer, resource, tool};

use crate::error::McpDev;

/// The URI of the Constitution resource this crate adds to the resource surface.
pub const CONSTITUTION_URI: &str = "gmeow://ontology/constitution";

/// The four repo-reading developer tools plus the Constitution resource, bound to
/// the checkout at `root`.
///
/// Each handler captures its own copy of `root`: the seam passes only the server and
/// the call arguments, so the checkout path is carried by the tools that need it
/// rather than by the server that does not.
#[must_use]
pub fn dev_extension(root: PathBuf) -> Extension {
    let validate_root = root.clone();
    let sync_root = root.clone();
    let constitution_tool_root = root.clone();
    let constitution_resource_root = root;
    Extension::new()
        .with_tool(
            tool("validate", "Run the native validation/check surface.", &[]),
            move |_server, _args| run_pipeline(&validate_root, "check"),
        )
        .with_tool(
            tool(
                "reason",
                "Run native reasoning over the bundled snapshot.",
                &[],
            ),
            |server, _args| {
                let result =
                    gmeow_logic::reason::reason_all(server.view().graph_dataset()?.as_ref())
                        .map_err(|e| {
                            gmeow_errors::Diag::of_kind(McpDev {
                                message: format!("native reasoning failed: {e}"),
                            })
                        })?;
                Ok(json!({
                    "ok": true,
                    "input": result.input.wire(),
                    "evaluation": result.evaluation.wire(),
                    "completeness": result.completeness.wire(),
                    "information": result.information.wire(),
                })
                .to_string())
            },
        )
        .with_tool(
            tool(
                "sync",
                "Run the native pipeline update-and-check surface.",
                &[],
            ),
            move |_server, _args| run_pipeline(&sync_root, "update"),
        )
        .with_tool(
            tool(
                "constitution",
                "Read the checked-out GMEOW Constitution.",
                &[],
            ),
            move |_server, _args| read_constitution(&constitution_tool_root),
        )
        .with_resource(
            resource(
                CONSTITUTION_URI,
                "constitution",
                "The checked-out GMEOW Constitution.",
                "text/markdown",
            ),
            move |_server, _requested| read_constitution(&constitution_resource_root),
        )
}

/// Build the DEVELOPER MCP server: the consumer surface over `snapshot`, plus the
/// four repo-reading tools and the Constitution resource bound to `root`.
///
/// # Errors
///
/// Whatever [`McpServer::from_snapshot_with`] raises — a snapshot that will not
/// read, an unknown `GMEOW_LANG`, or a surface that will not assemble.
pub fn dev_server(snapshot: &[u8], root: PathBuf) -> gmeow_errors::Result<McpServer> {
    McpServer::from_snapshot_with(snapshot, dev_extension(root))
}

/// The shared body of `validate` (one job, check mode) and `sync` (all cores, update
/// mode): ONE pipeline run over the checkout, reported as its drift counts.
///
/// `mode` is `"check"` or `"update"` and selects both the
/// [`RunMode`](gmeow_pipeline::run::RunMode) and the job
/// count, because those two choices are the SAME choice — a read-only verification
/// run is deliberately serial, an update run uses the machine.
fn run_pipeline(root: &std::path::Path, mode: &'static str) -> gmeow_errors::Result<String> {
    let (run_mode, jobs) = match mode {
        "check" => (gmeow_pipeline::run::RunMode::Check, 1),
        _ => (
            gmeow_pipeline::run::RunMode::Update,
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        ),
    };
    let report = gmeow_pipeline::run::run_full(root, jobs, run_mode)?;
    Ok(json!({
        "ok": report.is_clean(),
        "mode": mode,
        "produced": report.produced,
        "reproduced": report.reproduced,
        "drifted": report.drifted,
    })
    .to_string())
}

/// Read `CONSTITUTION.md` out of the checkout — the body BOTH the `constitution`
/// tool and the `gmeow://ontology/constitution` resource serve, so the two can never
/// diverge.
fn read_constitution(root: &std::path::Path) -> gmeow_errors::Result<String> {
    Ok(fs::read_to_string(root.join("CONSTITUTION.md"))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn snapshot() -> Vec<u8> {
        fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("read committed snapshot")
    }

    /// The DEV surface is the 35 consumer tools plus exactly 4, and the 5 consumer
    /// resources plus exactly 1. The dev tools are advertised AFTER the builtins.
    #[test]
    fn dev_surface_is_thirty_nine_tools_and_six_resources() {
        let server = dev_server(&snapshot(), repo_root()).expect("dev server constructs");
        let names = server.surface().tool_names();
        assert_eq!(
            names.len(),
            39,
            "the dev tool surface is the 35 consumer tools + 4, got {names:?}"
        );
        assert_eq!(
            &names[35..],
            ["validate", "reason", "sync", "constitution"],
            "the four dev tools are advertised after the consumer builtins"
        );
        let resources = server.surface().resource_descriptors();
        assert_eq!(
            resources.len(),
            6,
            "the dev resource surface is the 5 consumer resources + the Constitution"
        );
        assert_eq!(
            resources[5]["uri"], CONSTITUTION_URI,
            "the Constitution resource is advertised after the consumer builtins"
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
}
