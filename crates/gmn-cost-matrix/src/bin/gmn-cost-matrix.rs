// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `maint-gmn-cost-matrix` entrypoint: the FULL five-family sweep.
//!
//! The two OpenAI vocabularies are embedded (tiktoken-rs) and Qwen is vendored + blake3-pinned in
//! the repo; the Llama and Gemma `tokenizer.json` assets are FETCHED at maint-time (the Makefile
//! `curl`s them into the git-ignored `.tmp/` — never committed, because their licenses are
//! AGPL-incompatible) and their paths are passed here with `--llama <path>` and `--gemma <path>`.
//! Each fetched asset is blake3-verified against its committed pin.
//!
//! No-optionality: this lane runs the FULL five families. A missing `--llama`/`--gemma` path, an
//! unreadable/undecodable asset, or a digest mismatch is a HARD FAIL (non-zero exit) — the lane
//! never silently degrades to a three-family matrix.
//!
//! Usage:
//!   gmn-cost-matrix --llama <tokenizer.json> --gemma <tokenizer.json> [--out <report.md>]

use std::path::PathBuf;
use std::process::ExitCode;

use gmeow_gmn_cost_matrix::{
    MatrixError, Result, Vocab, build_corpus, default_report_path, load_cl100k, load_gemma,
    load_llama, load_o200k, load_qwen, render_matrix, repo_root, write_report,
};

fn main() -> ExitCode {
    match run() {
        Ok(path) => {
            eprintln!(
                "✓ wrote full five-family GMN token-cost matrix to {}",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("ERROR (maint-gmn-cost-matrix): {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf> {
    let mut llama: Option<PathBuf> = None;
    let mut gemma: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--llama" => llama = Some(require_value(&mut args, "--llama")?),
            "--gemma" => gemma = Some(require_value(&mut args, "--gemma")?),
            "--out" => out = Some(require_value(&mut args, "--out")?),
            other => {
                return Err(MatrixError::Cli(format!("unrecognized argument {other:?}")));
            }
        }
    }

    // No-optionality: the FULL five families are mandatory. A missing fetched path is a hard fail,
    // never permission to run a smaller matrix.
    let llama = llama.ok_or_else(|| {
        MatrixError::Cli(
            "--llama <tokenizer.json> is required (the maint lane fetches it into .tmp/ before \
             invoking this binary); refusing to emit a partial matrix"
                .to_owned(),
        )
    })?;
    let gemma = gemma.ok_or_else(|| {
        MatrixError::Cli(
            "--gemma <tokenizer.json> is required (the maint lane fetches it into .tmp/ before \
             invoking this binary); refusing to emit a partial matrix"
                .to_owned(),
        )
    })?;

    let root = repo_root();

    // Load all five families up front — a digest mismatch or decode failure aborts before any
    // report is written.
    let vocabs: Vec<Vocab> = vec![
        load_o200k()?,
        load_cl100k()?,
        load_qwen()?,
        load_llama(&llama)?,
        load_gemma(&gemma)?,
    ];

    let (dict, artifacts) = build_corpus(&root)?;
    let report = render_matrix(&dict, &artifacts, &vocabs)?;

    let out_path = out.unwrap_or_else(|| default_report_path(&root));
    write_report(&report, &out_path)?;
    Ok(out_path)
}

fn require_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| MatrixError::Cli(format!("{flag} requires a path argument")))
}
