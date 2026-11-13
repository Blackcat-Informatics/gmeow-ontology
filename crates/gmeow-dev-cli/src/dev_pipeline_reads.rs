// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated `gmeow-dev pipeline-reads` maintenance command.
//!
//! The pipeline description declares, on every dataflow edge, what the consuming stage
//! reads of the producer's product, and a production run fails a stage whose recorded
//! reads differ. This command is how those declarations are authored: it runs the
//! committed-scope pipeline once with recording in place of enforcement and writes the
//! Turtle that declares exactly what each stage body read. It writes no pipeline output.

use std::path::Path;

/// The emitting tool name.
const TOOL: &str = "pipeline-reads";

/// Record one run of the selected checkout and write its read declarations to `out`.
///
/// The CLI admits the producer first. Returns 0 after writing `out`, or 1 when the run
/// or the write fails.
pub(crate) fn run(out: &Path, jobs: Option<usize>) -> i32 {
    let jobs = match crate::dev_common::resolve_jobs(jobs) {
        Ok(jobs) => jobs,
        Err(code) => return code,
    };
    let root = crate::dev_common::project_root();
    let turtle = match gmeow_pipeline::reads::record_run(root.as_path(), jobs) {
        Ok(turtle) => turtle,
        Err(err) => {
            eprintln!("{TOOL}: the recorded run failed: {err:#}");
            return 1;
        }
    };
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty())
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        eprintln!("{TOOL}: cannot create {}: {err}", parent.display());
        return 1;
    }
    if let Err(err) = std::fs::write(out, &turtle) {
        eprintln!("{TOOL}: cannot write {}: {err}", out.display());
        return 1;
    }
    println!("wrote {} ({} bytes)", out.display(), turtle.len());
    0
}
