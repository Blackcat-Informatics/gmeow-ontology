// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for the per-term previous-release authority.
//!
//! Ordinary synchronization consumes this evidence and never rewrites it. The
//! normal mode advances an existing accepted release boundary; `--bootstrap` is
//! only for the repository's one-time initial authority and refuses an overwrite.

use std::path::PathBuf;

fn main() {
    let mut bootstrap = false;
    let mut root: Option<PathBuf> = None;
    for argument in std::env::args_os().skip(1) {
        if argument == std::ffi::OsStr::new("--bootstrap") {
            bootstrap = true;
        } else if root.replace(PathBuf::from(argument)).is_some() {
            eprintln!("usage: term-release-authority [--bootstrap] [REPO_ROOT]");
            std::process::exit(2);
        }
    }
    let root = match root {
        Some(root) => root,
        None => match std::env::current_dir() {
            Ok(root) => root,
            Err(error) => {
                eprintln!("term release authority: cannot resolve current directory: {error}");
                std::process::exit(1);
            }
        },
    };

    match gmeow_pipeline::stages::term_manifest::refresh_release_authority(&root, bootstrap) {
        Ok((release, terms, wrote)) => {
            let action = if wrote { "wrote" } else { "kept" };
            println!(
                "{action} {} term records for ontology release {release} at {}",
                terms,
                root.join(gmeow_pipeline::stages::term_manifest::TERM_RELEASE_AUTHORITY_PATH)
                    .display()
            );
        }
        Err(error) => {
            eprintln!("term release authority: {error}");
            std::process::exit(1);
        }
    }
}
