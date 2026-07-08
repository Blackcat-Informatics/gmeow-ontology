// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-lsp` binary entry-point.
//!
//! Two modes:
//!
//! * **Default (no subcommand)** — run as a synchronous LSP server over stdio,
//!   using `lsp-server`'s `Connection::stdio()`.  Handles
//!   `textDocument/didOpen`, `textDocument/didChange`, and
//!   `textDocument/didClose` notifications, publishing `publishDiagnostics`
//!   after every open/change.
//!
//! * **`sarif` subcommand** — parse one or more `.ttl` / `.logic` files from
//!   the command line, emit a SARIF 2.1.0 file to `--out <dir>/gmeow-feedback.sarif`,
//!   and exit.  Designed for `actions/upload-sarif` in GitHub code-scanning
//!   workflows.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};

use lsp_server::{Connection, Message, Notification, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, PublishDiagnostics,
};
use lsp_types::{
    InitializeResult, PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

use gmeow_cli_core::{ConsoleMode, Reporter, report_diag};
use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint, render};
use gmeow_lsp::{analyze, classify, report_to_diagnostics};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");

/// A boxed reporter for this bin. stdout is the LSP JSON-RPC channel (in server
/// mode) / carries no product in the sarif mode, so diagnostics default to the
/// HUMAN stderr surface and never corrupt the protocol; an agent opts into the
/// machine surface with `GMEOW_CONSOLE=jsonl`.
fn reporter() -> Box<dyn Reporter> {
    let mode = ConsoleMode::resolve_stderr_default(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
    );
    gmeow_cli_core::reporter_for(mode)
}

/// Surface an Error-grade diagnostic on the console sink — the substrate
/// replacement for a bare fatal-error stderr write at a site that exits itself.
fn emit_error(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    let diag = Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    );
    reporter.report(&report_diag(diag, NAME));
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let reporter = reporter();

    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("{NAME} {VERSION}");
        }
        Some("--help") | Some("-h") => {
            print_usage();
        }
        Some("sarif") => {
            if let Err(e) = run_sarif(&args[2..], reporter.as_ref()) {
                emit_error(reporter.as_ref(), "gmeow-lsp.sarif", e.to_string());
                std::process::exit(1);
            }
        }
        _ => {
            if let Err(e) = run_lsp_server() {
                emit_error(reporter.as_ref(), "gmeow-lsp.server", e.to_string());
                std::process::exit(1);
            }
        }
    }
}

fn print_usage() {
    let usage = concat!(
        "Usage: gmeow-lsp [SUBCOMMAND]\n",
        "\n",
        "SUBCOMMANDS:\n",
        "  (none)                  Run as an LSP server over stdio\n",
        "  sarif [OPTIONS] FILES   Emit SARIF for FILES and exit\n",
        "  --version               Print version and exit\n",
        "  --help                  Print this help\n",
        "\n",
        "SARIF OPTIONS:\n",
        "  --out DIR               Output directory (default: .)\n",
        "  --category NAME         SARIF automationDetails.id category\n",
        "  --output-file FILENAME  Output file name (default: gmeow-feedback.sarif)\n",
    );
    print!("{usage}");
}

// ─── SARIF subcommand ────────────────────────────────────────────────────────

fn run_sarif(argv: &[String], reporter: &dyn Reporter) -> io::Result<()> {
    let mut out_dir = PathBuf::from(".");
    let mut category: Option<String> = None;
    let mut output_file = "gmeow-feedback.sarif".to_owned();
    let mut files: Vec<PathBuf> = Vec::new();

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(argv.get(i).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--out requires a value")
                })?);
            }
            "--category" => {
                i += 1;
                category = Some(
                    argv.get(i)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "--category requires a value",
                            )
                        })?
                        .clone(),
                );
            }
            "--output-file" => {
                i += 1;
                output_file = argv
                    .get(i)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--output-file requires a value",
                        )
                    })?
                    .clone();
            }
            flag if flag.starts_with('-') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown flag: {flag}"),
                ));
            }
            path => {
                files.push(PathBuf::from(path));
            }
        }
        i += 1;
    }

    if files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no input files specified",
        ));
    }

    let mut combined = gmeow_errors::model::Report::new("gmeow-lsp");
    if let Some(cat) = category {
        combined
            .metadata
            .insert("category".to_owned(), serde_json::json!(cat));
    }

    for path in &files {
        let path_str = path.to_string_lossy().into_owned();
        let Some(lang) = classify(&path_str) else {
            gmeow_cli_core::note(
                reporter,
                NAME,
                "gmeow-lsp.sarif.note",
                format!("skipping {} (unrecognised extension)", path.display()),
            );
            continue;
        };
        let text = fs::read_to_string(path)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
        let virtual_path = repo_relative_path(path);
        let report = analyze(lang, &text, &virtual_path);
        for finding in report.findings {
            combined.add_finding(finding);
        }
    }
    combined.normalize();

    let sarif = render::to_sarif(&combined)
        .map_err(|e| io::Error::other(format!("SARIF render error: {e}")))?;

    fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join(&output_file);
    fs::write(&out_path, &sarif)?;
    gmeow_cli_core::note(
        reporter,
        NAME,
        "gmeow-lsp.sarif.note",
        format!(
            "wrote {} finding(s) to {}",
            combined.findings.len(),
            out_path.display()
        ),
    );
    Ok(())
}

fn repo_relative_path(path: &Path) -> String {
    if let Ok(cwd) = env::current_dir()
        && let Ok(rel) = path.strip_prefix(&cwd)
    {
        return rel.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

// ─── LSP server ──────────────────────────────────────────────────────────────

fn run_lsp_server() -> io::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    })
    .map_err(io::Error::other)?;

    let (id, _params) = connection.initialize_start().map_err(io::Error::other)?;

    let init_result = InitializeResult {
        capabilities: serde_json::from_value(server_capabilities).map_err(io::Error::other)?,
        server_info: Some(lsp_types::ServerInfo {
            name: NAME.to_owned(),
            version: Some(VERSION.to_owned()),
        }),
    };
    connection
        .initialize_finish(
            id,
            serde_json::to_value(init_result).map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;

    let mut open_docs: HashMap<String, String> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req).map_err(io::Error::other)? {
                    break;
                }
                let resp = Response::new_err(
                    req.id,
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("unsupported request method: {}", req.method),
                );
                connection
                    .sender
                    .send(Message::Response(resp))
                    .map_err(io::Error::other)?;
            }
            Message::Notification(notif) => {
                handle_notification(&connection, &mut open_docs, notif)
                    .map_err(io::Error::other)?;
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join().map_err(io::Error::other)
}

fn handle_notification(
    connection: &Connection,
    open_docs: &mut HashMap<String, String>,
    notif: Notification,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match notif.method.as_str() {
        <DidOpenTextDocument as lsp_types::notification::Notification>::METHOD => {
            let params: lsp_types::DidOpenTextDocumentParams =
                serde_json::from_value(notif.params)?;
            let uri_str = params.text_document.uri.as_str().to_owned();
            let text = params.text_document.text.clone();
            open_docs.insert(uri_str.clone(), text.clone());
            publish_diagnostics(connection, &params.text_document.uri, &text)?;
        }
        <DidChangeTextDocument as lsp_types::notification::Notification>::METHOD => {
            let params: lsp_types::DidChangeTextDocumentParams =
                serde_json::from_value(notif.params)?;
            let uri_str = params.text_document.uri.as_str().to_owned();
            if let Some(change) = params.content_changes.last() {
                let text = change.text.clone();
                open_docs.insert(uri_str, text.clone());
                publish_diagnostics(connection, &params.text_document.uri, &text)?;
            }
        }
        <DidCloseTextDocument as lsp_types::notification::Notification>::METHOD => {
            let params: lsp_types::DidCloseTextDocumentParams =
                serde_json::from_value(notif.params)?;
            let uri_str = params.text_document.uri.as_str().to_owned();
            open_docs.remove(&uri_str);
            send_empty_diagnostics(connection, &params.text_document.uri)?;
        }
        _ => {}
    }
    Ok(())
}

/// Build the `PublishDiagnosticsParams` for `uri`/`text` — the exact value the
/// server serializes into the `textDocument/publishDiagnostics` notification.
///
/// Factored out of [`publish_diagnostics`] so the analysis → `report_to_diagnostics`
/// → params pipeline is a pure, testable function; the server loop only adds the
/// notification send around it. `uri` is threaded into `report_to_diagnostics` so
/// secondary labels anchor their `DiagnosticRelatedInformation` to the document.
fn build_publish_params(uri: &Uri, text: &str) -> PublishDiagnosticsParams {
    let virtual_path = uri_to_virtual_path(uri.as_str());

    match classify(&virtual_path) {
        Some(lang) => {
            let report = analyze(lang, text, &virtual_path);
            params_from_report(uri, &report)
        }
        None => PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: Vec::new(),
            version: None,
        },
    }
}

/// Project an analyzed [`Report`](gmeow_errors::model::Report) into the
/// `PublishDiagnosticsParams` the server sends for `uri`.
///
/// This is the innermost production seam: `report_to_diagnostics` (which projects
/// each finding's secondary labels into `DiagnosticRelatedInformation`) followed by
/// the param wrapper. Split out so a test can drive it with any `Report` — including
/// one carrying `related_labels` — without a live editor session.
fn params_from_report(uri: &Uri, report: &gmeow_errors::model::Report) -> PublishDiagnosticsParams {
    PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: report_to_diagnostics(report, uri),
        version: None,
    }
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Uri,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let params = build_publish_params(uri, text);
    let notif = Notification::new(
        <PublishDiagnostics as lsp_types::notification::Notification>::METHOD.to_owned(),
        serde_json::to_value(params)?,
    );
    connection.sender.send(Message::Notification(notif))?;
    Ok(())
}

fn send_empty_diagnostics(
    connection: &Connection,
    uri: &Uri,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: Vec::new(),
        version: None,
    };
    let notif = Notification::new(
        <PublishDiagnostics as lsp_types::notification::Notification>::METHOD.to_owned(),
        serde_json::to_value(params)?,
    );
    connection.sender.send(Message::Notification(notif))?;
    Ok(())
}

/// Convert a URI string to a virtual path for [`analyze`].
///
/// For `file://` URIs the path is percent-decoded and returned as an
/// absolute filesystem path (platform-native, including Windows drive
/// letters and UNC paths).  `file:///path/to/My%20Files/x.ttl` →
/// `/path/to/My Files/x.ttl`.
///
/// Non-`file:` URIs (e.g. `untitled:Untitled-1`) are returned as-is so
/// that callers that use virtual paths for routing continue to work.
///
/// Any parse or conversion failure falls back to returning the raw
/// `uri_str` unchanged — diagnostics must never panic.
fn uri_to_virtual_path(uri_str: &str) -> String {
    url::Url::parse(uri_str)
        .ok()
        .and_then(|u| u.to_file_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| uri_str.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{build_publish_params, params_from_report, uri_to_virtual_path};
    use gmeow_errors::model::{Finding, Location, RelatedLabel, Report, Severity};
    use lsp_types::Uri;

    /// The END-TO-END production publish path — `didOpen` → `build_publish_params` →
    /// `publishDiagnostics` — over a REAL `.ttl` document that VIOLATES a bundled SHACL
    /// shape. `build_publish_params` is the exact value the server serializes into the
    /// notification: it classifies the `.ttl` URI, runs `analyze` (which now READS THE
    /// SUBSTRATE — routing the parsed graph through the bundled shapes and a
    /// `DiagLedger`), and projects to `PublishDiagnosticsParams`.
    ///
    /// The fixture is a `gmeow:DoxasticState` with no `gmeow:epistemicAgent`. The
    /// bundled `gmeow:DoxasticStateShape` pins `sh:path gmeow:epistemicAgent ;
    /// sh:minCount 1`, so the substrate emits `shacl.MinCountConstraintComponent` with
    /// a result-path secondary span — projected by the ledger into a text-bearing
    /// `related_label` whose message is `"path"`. The assertion is that a REAL secondary
    /// label produced by the substrate (not an injected one) rides in the published
    /// diagnostic's `related_information`.
    #[test]
    fn publish_path_over_real_shape_violation_carries_shacl_secondary_label() {
        let doc_uri: Uri = "file:///tmp/agentless-belief.ttl"
            .parse()
            .expect("valid uri");
        // A DoxasticState omitting the required epistemicAgent — a genuine violation of
        // the bundled gmeow:DoxasticStateShape (sh:minCount 1 on gmeow:epistemicAgent).
        let ttl = concat!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
            "@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/lsp/> .\n",
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
            "ex:agentlessBelief a gmeow:DoxasticState ;\n",
            "    rdfs:label \"an agent-less doxastic state\"@x-gmeow-english .\n",
        );

        let params = build_publish_params(&doc_uri, ttl);

        assert_eq!(params.uri.as_str(), doc_uri.as_str());
        assert!(
            !params.diagnostics.is_empty(),
            "the substrate must surface at least one SHACL violation for an \
             agent-less DoxasticState"
        );

        // A MinCount violation must be present with a text-bearing "path" secondary
        // label — the REAL SHACL result-path span the substrate produced.
        let mincount = params
            .diagnostics
            .iter()
            .find(|d| {
                matches!(
                    &d.code,
                    Some(lsp_types::NumberOrString::String(code))
                        if code == "shacl.MinCountConstraintComponent"
                )
            })
            .expect("a shacl.MinCountConstraintComponent diagnostic must be published");

        let infos = mincount
            .related_information
            .as_ref()
            .expect("the MinCount diagnostic must carry SHACL secondary labels");
        assert!(
            infos.iter().any(|i| i.message == "path"),
            "the published related_information must carry the substrate's SHACL \
             result-path secondary label (\"path\"): {infos:?}"
        );
    }

    /// Drive the SERVER publish path — the same `params_from_report` seam
    /// `publish_diagnostics` sends over the connection after `didOpen` — with a
    /// report carrying a text-bearing secondary label, and assert the emitted
    /// `PublishDiagnosticsParams` carries the label MESSAGE as
    /// `DiagnosticRelatedInformation`.
    ///
    /// The real `analyze()` path does not yet mint multi-label findings, so the
    /// report is built via the public `gmeow_errors` API; the production surface
    /// under test is the params/diagnostics-building code the server sends verbatim.
    #[test]
    fn publish_params_carry_related_information_message() {
        let doc_uri: Uri = "file:///home/user/rules.ttl".parse().expect("valid uri");

        let mut report = Report::new("gmeow-lsp");
        let mut finding = Finding::new(Severity::Error, "logic.conflict", "unsatisfiable class");
        finding.add_location(Location::new(
            Some("/home/user/rules.ttl".to_owned()),
            Some(7),
            Some(3),
            None,
        ));
        finding.add_related_label(RelatedLabel {
            location: Location::new(
                Some("/home/user/companion.ttl".to_owned()),
                Some(2),
                Some(1),
                None,
            ),
            message: "conflicting axiom defined here".to_owned(),
        });
        report.add_finding(finding);

        let params = params_from_report(&doc_uri, &report);

        assert_eq!(params.uri.as_str(), doc_uri.as_str());
        assert_eq!(params.diagnostics.len(), 1);
        let infos = params.diagnostics[0]
            .related_information
            .as_ref()
            .expect("related_information should be published");
        assert!(
            infos
                .iter()
                .any(|i| i.message == "conflicting axiom defined here"),
            "expected the secondary-label message in published related_information: {infos:?}"
        );
        let info = &infos[0];
        assert_eq!(
            info.location.uri.as_str(),
            "file:///home/user/companion.ttl"
        );
        // 1-based (2,1) → 0-based (1,0).
        assert_eq!(info.location.range.start.line, 1);
        assert_eq!(info.location.range.start.character, 0);
    }

    #[test]
    fn file_uri_simple() {
        assert_eq!(
            uri_to_virtual_path("file:///home/user/data.ttl"),
            "/home/user/data.ttl"
        );
    }

    #[test]
    fn file_uri_percent_encoded_space() {
        // Regression: the old hand-rolled strip did not percent-decode,
        // so paths containing %20 produced NotFound errors.
        assert_eq!(
            uri_to_virtual_path("file:///home/user/My%20Files/data.ttl"),
            "/home/user/My Files/data.ttl"
        );
    }

    #[test]
    fn file_uri_localhost_authority() {
        // `file://localhost/path` is a valid RFC 8089 form.
        assert_eq!(
            uri_to_virtual_path("file://localhost/tmp/foo.ttl"),
            "/tmp/foo.ttl"
        );
    }

    #[test]
    fn non_file_uri_passes_through() {
        // Non-file schemes must be returned unchanged (virtual-path routing).
        let s = "untitled:foo";
        assert_eq!(uri_to_virtual_path(s), s);
    }

    #[test]
    fn unparseable_uri_passes_through() {
        // Garbage input must not panic; just pass through.
        let s = "not a uri at all \x00";
        assert_eq!(uri_to_virtual_path(s), s);
    }
}
