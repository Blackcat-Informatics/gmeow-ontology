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
    Diagnostic, InitializeResult, PublishDiagnosticsParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

use gmeow_diagnostics::render;
use gmeow_lsp::{analyze, classify, report_to_diagnostics};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = env!("CARGO_PKG_NAME");

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("{NAME} {VERSION}");
        }
        Some("--help") | Some("-h") => {
            print_usage();
        }
        Some("sarif") => {
            if let Err(e) = run_sarif(&args[2..]) {
                eprintln!("gmeow-lsp sarif: {e}");
                std::process::exit(1);
            }
        }
        _ => {
            if let Err(e) = run_lsp_server() {
                eprintln!("gmeow-lsp: fatal error: {e}");
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

fn run_sarif(argv: &[String]) -> io::Result<()> {
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

    let mut combined = gmeow_diagnostics::model::Report::new("gmeow-lsp");
    if let Some(cat) = category {
        combined
            .metadata
            .insert("category".to_owned(), serde_json::json!(cat));
    }

    for path in &files {
        let path_str = path.to_string_lossy().into_owned();
        let Some(lang) = classify(&path_str) else {
            eprintln!(
                "gmeow-lsp sarif: skipping {} (unrecognised extension)",
                path.display()
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
    eprintln!(
        "gmeow-lsp sarif: wrote {} finding(s) to {}",
        combined.findings.len(),
        out_path.display()
    );
    Ok(())
}

fn repo_relative_path(path: &Path) -> String {
    if let Ok(cwd) = env::current_dir() {
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.to_string_lossy().into_owned();
        }
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

fn publish_diagnostics(
    connection: &Connection,
    uri: &Uri,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let uri_str = uri.as_str();
    let virtual_path = uri_to_virtual_path(uri_str);

    let diagnostics: Vec<Diagnostic> = match classify(&virtual_path) {
        Some(lang) => {
            let report = analyze(lang, text, &virtual_path);
            report_to_diagnostics(&report)
        }
        None => Vec::new(),
    };

    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
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
    use super::uri_to_virtual_path;

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
