// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verify an emitted mdBook source tree against the HTML corpus built from it.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(source_root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: mdbook-smoke <emitted-book-root> <rendered-book-root>");
        return ExitCode::from(2);
    };
    let Some(rendered_root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: mdbook-smoke <emitted-book-root> <rendered-book-root>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: mdbook-smoke <emitted-book-root> <rendered-book-root>");
        return ExitCode::from(2);
    }

    let audit = gmeow_docs::rendered_book::audit_rendered_book(&source_root, &rendered_root);
    if !audit.report.ok() {
        for finding in &audit.report.findings {
            let location = finding
                .locations
                .first()
                .and_then(|location| location.path.as_deref())
                .map_or(String::new(), |path| format!(" ({path})"));
            eprintln!(
                "{} [{}] {}{}",
                finding.severity, finding.code, finding.message, location
            );
        }
        eprintln!(
            "FAIL: rendered mdBook audit found {} error(s) across {} HTML page(s) and {} local href/src reference(s)",
            audit.report.error_count(),
            audit.html_pages,
            audit.local_references
        );
        return ExitCode::FAILURE;
    }

    println!(
        "OK: rendered mdBook: {} HTML pages, {} local href/src references, {} capability-backed wasm engines",
        audit.html_pages, audit.local_references, audit.wasm_engines
    );
    ExitCode::SUCCESS
}
